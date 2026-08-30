//! Arachne CLI entry point.
//!
//! Phase A («Штамп-ядро»): fetch → DOM → CSS selectors → export.
//! Собственный JS/DOM-движок и Smart Router входят в Фазу B.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;

use arachne_domain::{Loop, NestedSelector, OutputTemplate, PageId, Selector, TaskId, Url, While};
use arachne_export::Record;
use arachne_net::{DefaultSession, GaussJitter, OsJitterRng, RequestContext, SessionConfig};
use arachne_parse::Dom;

/// CLI for Arachne stealth crawler.
#[derive(Parser)]
#[command(
    name = "arachne",
    version,
    about = "Stealth crawler — Phase A stamp kernel"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Crawl from a declarative job.yaml descriptor.
    Crawl {
        /// Path to job.yaml.
        #[arg(short, long)]
        job: PathBuf,
        /// Output file (format inferred from extension: .csv/.jsonl/.sqlite).
        #[arg(short, long, value_name = "OUT")]
        output: Option<PathBuf>,
        /// Mean delay between requests in ms (rate-limit base).
        #[arg(long, default_value_t = 200)]
        delay_ms: u64,
        /// Override task id for this crawl run.
        #[arg(long, value_name = "ID")]
        task_id: Option<u64>,
    },
}

/// Declarative job descriptor (веха M3 — job.yaml).
#[derive(Deserialize)]
struct Job {
    start_urls: Vec<String>,
    #[serde(default)]
    selectors: Vec<SelectorMapping>,
    /// Вложенные селекторы: извлекают структурированные данные
    /// из повторяющихся блоков (docs/03-job-yaml.md §4).
    #[serde(default)]
    nested_selectors: Vec<NestedSelector>,
    #[serde(default)]
    proxies: Vec<String>,
    #[serde(default)]
    limit: Option<u64>,
    /// Циклы подстановки значений в URL/заголовки/куки.
    #[serde(default)]
    loops: Vec<Loop>,
    /// Цикл с условием остановки (while) — фетчит пока условие не выполнится.
    #[serde(default)]
    r#while: Option<While>,
    /// Переход по ссылкам (BFS-обход): selector/max_depth/pattern/same_host.
    #[serde(default)]
    follow: Option<Follow>,
    /// Шаблон структуры вывода (JSON-склейка с плейсхолдерами).
    #[serde(default)]
    output_template: Option<OutputTemplate>,
    /// Дополнительные заголовки запроса (можно с {var}).
    #[serde(default)]
    headers: Vec<HeaderMapping>,
    /// Cookie для запросов (можно с {var}).
    #[serde(default)]
    cookies: Vec<CookieMapping>,
}

#[derive(Deserialize)]
struct HeaderMapping {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct CookieMapping {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct SelectorMapping {
    name: String,
    selector: String,
}

/// Настройки перехода по ссылкам (job.yaml `follow`, docs/03-job-yaml.md §5).
#[derive(Deserialize)]
struct Follow {
    /// CSS-селектор элементов-ссылок (берётся их `href`).
    selector: String,
    /// Максимальная глубина переходов: 1 — одна ступень от стартовых страниц.
    #[serde(default = "default_follow_depth")]
    max_depth: u32,
    /// Фильтр-подстрока: в очередь идут только ссылки, содержащие её.
    #[serde(default)]
    pattern: Option<String>,
    /// Не покидать хост страницы, где найдена ссылка.
    #[serde(default = "default_follow_same_host")]
    same_host: bool,
}

fn default_follow_depth() -> u32 {
    1
}

fn default_follow_same_host() -> bool {
    true
}

// NestedField и NestedSelector сериализуются/десериализуются напрямую из
// arachne_domain (они уже имеют serde), поэтому не нужны локальные модели.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.cmd {
        Commands::Crawl {
            job,
            output,
            delay_ms,
            task_id,
        } => run_crawl(&job, output.as_deref(), delay_ms, task_id).await,
    }
}

async fn run_crawl(
    job_path: &Path,
    output: Option<&Path>,
    delay_ms: u64,
    task_id: Option<u64>,
) -> anyhow::Result<()> {
    info!("loading job: {}", job_path.display());
    let text = std::fs::read_to_string(job_path)
        .with_context(|| format!("cannot read job file {}", job_path.display()))?;
    let job: Job = serde_yaml::from_str(&text).context("invalid job.yaml")?;

    // Stealth config: прокси + rate-limit (гауссов джиттер).
    let mut cfg = SessionConfig {
        jitter_ms: delay_ms,
        ..Default::default()
    };
    for p in &job.proxies {
        cfg.proxies.push(arachne_net::parse_proxy(p)?);
    }

    // Phase A default backend: reqwest/rustls (см. docs/01-tech-stack.md).
    let backend = arachne_net::RustlsBackend::direct(cfg.timeout_secs)?;
    let session = DefaultSession::new(backend, cfg);

    // Гауссов джиттер с внедрённым RNG (rules.md §8).
    let jitter = GaussJitter::new(OsJitterRng);

    let task_id = TaskId::new(task_id.unwrap_or(1));
    let mut records: Vec<Record> = Vec::new();
    let mut page_id = PageId::new(1);
    let mut request_count = 0u64;

    // Базовый контекст запроса из job.headers и job.cookies.
    let mut base_ctx = RequestContext::default();
    for h in &job.headers {
        base_ctx.headers.push((h.name.clone(), h.value.clone()));
    }
    for c in &job.cookies {
        base_ctx.cookies.push((c.name.clone(), c.value.clone()));
    }

    // Раскрываем циклы: список (URL-шаблон, набор переменных).
    let seed_urls = expand_loops(&job.start_urls, &job.loops, &job.r#while)?;

    // follow: CSS-селектор ссылок парсим один раз.
    let follow_sel = job
        .follow
        .as_ref()
        .map(|f| Selector::try_from(f.selector.as_str()))
        .transpose()?;

    // BFS-очередь краулинга.
    let mut queue: VecDeque<CrawlItem> = seed_urls.into_iter().map(|(u, v)| (u, 0u32, v)).collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut first_request = true;

    while let Some((url_pattern, depth, vars)) = queue.pop_front() {
        // Rate limit: пауза между запросами (кроме первого), mean ± sigma/2.
        if first_request {
            first_request = false;
        } else {
            let d = jitter.delay_ms(delay_ms, delay_ms / 2);
            tokio::time::sleep(std::time::Duration::from(d)).await;
        }

        let url_str = substitute_vars(&url_pattern, &vars);
        // Дедупликация: каждую страницу фетчим не более одного раза.
        if !visited.insert(url_str.clone()) {
            continue;
        }
        let url = Url::try_from(url_str.clone())?;
        let ctx = apply_context_vars(&base_ctx, &vars);

        let html = session.get_with(&url, &ctx).await.context("fetch failed")?;
        let dom = Dom::parse(&html)?;

        // While-запрос: останавливаемся, когда условие выполнено.
        if let Some(w) = job.r#while.as_ref()
            && should_stop(&html, w)
        {
            info!("while condition met, stopping at page {}", page_id.get());
            break;
        }

        for m in &job.selectors {
            let sel = Selector::try_from(m.selector.as_str())?;
            if let Some(v) = dom.select_text(&sel)? {
                records.push(Record {
                    task_id: task_id.get(),
                    page_id: page_id.get(),
                    url: url.to_string(),
                    field: m.name.clone(),
                    value: v.trim().to_string(),
                });
            }
        }

        // Вложенный поиск.
        for ns in &job.nested_selectors {
            let nested = dom.select_all_nested(ns)?;
            for nr in nested {
                records.push(Record {
                    task_id: task_id.get(),
                    page_id: page_id.get(),
                    url: url.to_string(),
                    field: nr.field,
                    value: nr.value,
                });
            }
        }

        // Переход по ссылкам (job.follow): BFS с ограничением глубины.
        if let (Some(f), Some(fsel)) = (job.follow.as_ref(), follow_sel.as_ref())
            && depth < f.max_depth
        {
            for href in dom.extract_links(fsel)? {
                let Some(abs) = resolve_url(&url_str, &href) else {
                    continue;
                };
                if !should_follow(f, &url_str, &abs) || visited.contains(&abs) {
                    continue;
                }
                debug!(depth = depth + 1, "follow: enqueue {abs}");
                queue.push_back((abs, depth + 1, vars.clone()));
            }
        }

        page_id = PageId::new(page_id.get() + 1);
        request_count += 1;
        if let Some(limit) = job.limit
            && request_count >= limit
        {
            info!("limit {limit} reached, stopping");
            break;
        }
    }

    // Export: формат по расширению.
    let snap = session.stats().snapshot();
    info!(
        "fetched {} page(s), {} error(s), {} rate-limited",
        snap.requests, snap.errors, snap.rate_limited
    );

    if let Some(out) = output {
        let ext = out
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jsonl")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => arachne_export::to_csv(out, &records)?,
            "jsonl" => {
                // Если задан output_template — рендерим структурированный JSON.
                if let Some(tpl) = &job.output_template {
                    let idx = arachne_export::template::group_fields(&records);
                    let nested = arachne_export::template::group_nested(&records);
                    let rendered = arachne_export::template::render(&tpl.0, &idx, &nested)
                        .map_err(|e| anyhow::anyhow!("template: {e}"))?;
                    let json = serde_json::to_string_pretty(&rendered)?;
                    std::fs::write(out, json)?;
                } else {
                    arachne_export::to_jsonl(out, &records)?;
                }
            }
            "json" => {
                // Если задан output_template — рендерим структурированный JSON.
                if let Some(tpl) = &job.output_template {
                    let idx = arachne_export::template::group_fields(&records);
                    let nested = arachne_export::template::group_nested(&records);
                    let rendered = arachne_export::template::render(&tpl.0, &idx, &nested)
                        .map_err(|e| anyhow::anyhow!("template: {e}"))?;
                    let json = serde_json::to_string_pretty(&rendered)?;
                    std::fs::write(out, json)?;
                } else {
                    let json = serde_json::to_string_pretty(&records)?;
                    std::fs::write(out, json)?;
                }
            }
            "sqlite" | "db" => arachne_export::to_sqlite(out, &records)?,
            other => anyhow::bail!("unsupported output extension: {other}"),
        }
        info!("wrote {} record(s) to {}", records.len(), out.display());
    } else {
        println!("{}", serde_json::to_string_pretty(&records)?);
    }

    Ok(())
}

/// Раскрыть циклы в список пар «URL-шаблон → переменные».
#[allow(clippy::type_complexity)]
fn expand_loops(
    base_urls: &[String],
    loops: &[Loop],
    while_loop: &Option<While>,
) -> anyhow::Result<Vec<(String, Vec<(String, String)>)>> {
    let mut result = Vec::new();

    // While: предгенерируем max_iterations URL с инкрементом var.
    if let Some(w) = while_loop {
        let mut var_value = w.start;
        for _ in 0..w.max_iterations {
            let vars = vec![(w.var.clone(), var_value.to_string())];
            for url in base_urls {
                result.push((url.clone(), vars.clone()));
            }
            var_value += w.step;
        }
        return Ok(result);
    }

    // Без циклов — просто базовые URL.
    if loops.is_empty() {
        for url in base_urls {
            result.push((url.clone(), Vec::new()));
        }
        return Ok(result);
    }

    // Декартово произведение значений всех циклов.
    let mut combinations: Vec<Vec<(String, String)>> = vec![Vec::new()];
    for loop_item in loops {
        let values = loop_values(loop_item);
        let mut next = Vec::new();
        for combo in &combinations {
            for val in &values {
                let mut c = combo.clone();
                c.push((loop_item.var.clone(), val.clone()));
                next.push(c);
            }
        }
        combinations = next;
    }
    for url in base_urls {
        for combo in &combinations {
            result.push((url.clone(), combo.clone()));
        }
    }
    Ok(result)
}

/// Значения цикла: из диапазона или из массива.
fn loop_values(loop_item: &Loop) -> Vec<String> {
    if let Some(range) = &loop_item.range {
        let mut vals = Vec::new();
        let mut v = range.start;
        while v <= range.end {
            vals.push(v.to_string());
            v += range.step;
        }
        return vals;
    }
    if let Some(values) = &loop_item.values {
        return values.clone();
    }
    Vec::new()
}

/// Подстановка `{var}` в строку.
fn substitute_vars(s: &str, vars: &[(String, String)]) -> String {
    let mut result = s.to_string();
    for (name, value) in vars {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

/// Применить переменные к контексту запроса (заголовки/куки).
fn apply_context_vars(base: &RequestContext, vars: &[(String, String)]) -> RequestContext {
    let mut ctx = base.clone();
    for (k, v) in &mut ctx.headers {
        *k = substitute_vars(k, vars);
        *v = substitute_vars(v, vars);
    }
    for (k, v) in &mut ctx.cookies {
        *k = substitute_vars(k, vars);
        *v = substitute_vars(v, vars);
    }
    ctx
}

/// Условие остановки while по тексту страницы.
fn should_stop(html: &arachne_domain::Html, w: &While) -> bool {
    if let Ok(text) = html.as_str() {
        if w.stop_when
            .text
            .as_deref()
            .is_some_and(|stop| text.contains(stop))
        {
            return true;
        }
        if w.stop_when
            .text_not
            .as_deref()
            .is_some_and(|not| !text.contains(not))
        {
            return true;
        }
    }
    false
}

/// Элемент BFS-очереди краулинга: URL-шаблон, глубина, переменные цикла.
#[allow(clippy::type_complexity)]
type CrawlItem = (String, u32, Vec<(String, String)>);

/// Резолвит `href` относительно URL страницы; возвращает абсолютную ссылку
/// или `None` для не-HTTP схем (mailto:, javascript:, data:, ...).
fn resolve_url(base: &str, href: &str) -> Option<String> {
    let joined = url::Url::parse(base).ok()?.join(href).ok()?;
    if !matches!(joined.scheme(), "http" | "https") {
        return None;
    }
    Some(joined.to_string())
}

/// Решает, ставить ли найденную ссылку в очередь обхода (фильтры follow):
/// pattern-подстрока и ограничение «не покидать хост».
fn should_follow(f: &Follow, page_url: &str, abs: &str) -> bool {
    if let Some(pat) = &f.pattern
        && !abs.contains(pat.as_str())
    {
        return false;
    }
    if f.same_host
        && let Ok(page) = url::Url::parse(page_url)
        && let Ok(link) = url::Url::parse(abs)
        && page.host_str() != link.host_str()
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use arachne_domain::{RangeLoop, StopCondition};

    #[test]
    fn loop_range_values() {
        let l = Loop {
            var: "page".into(),
            range: Some(RangeLoop {
                start: 1,
                end: 3,
                step: 1,
            }),
            values: None,
        };
        assert_eq!(loop_values(&l), vec!["1", "2", "3"]);
    }

    #[test]
    fn loop_list_values() {
        let l = Loop {
            var: "id".into(),
            range: None,
            values: Some(vec!["a".into(), "b".into()]),
        };
        assert_eq!(loop_values(&l), vec!["a", "b"]);
    }

    #[test]
    fn expand_loops_without_loops() {
        let urls = vec!["https://x.io/".to_string()];
        let out = expand_loops(&urls, &[], &None).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "https://x.io/");
        assert!(out[0].1.is_empty());
    }

    #[test]
    fn expand_loops_with_range() {
        let urls = vec!["https://x.io/page/{page}".to_string()];
        let loops = vec![Loop {
            var: "page".into(),
            range: Some(RangeLoop {
                start: 1,
                end: 2,
                step: 1,
            }),
            values: None,
        }];
        let out = expand_loops(&urls, &loops, &None).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(substitute_vars(&out[0].0, &out[0].1), "https://x.io/page/1");
        assert_eq!(substitute_vars(&out[1].0, &out[1].1), "https://x.io/page/2");
    }

    #[test]
    fn substitute_vars_replaces() {
        let vars = vec![("page".to_string(), "42".to_string())];
        assert_eq!(
            substitute_vars("https://x.io/{page}", &vars),
            "https://x.io/42"
        );
        assert_eq!(substitute_vars("no vars", &vars), "no vars");
    }

    #[test]
    fn should_stop_by_text() {
        let html = arachne_domain::Html::new(bytes::Bytes::from_static(b"Page not found"));
        let w = While {
            var: "p".into(),
            start: 1,
            step: 1,
            max_iterations: 10,
            stop_when: StopCondition {
                status: None,
                text: Some("not found".into()),
                text_not: None,
            },
        };
        assert!(should_stop(&html, &w));
    }

    #[test]
    fn resolve_url_joins_relative() {
        let base = "https://x.io/a/b/";
        assert_eq!(
            resolve_url(base, "page/2").as_deref(),
            Some("https://x.io/a/b/page/2")
        );
        assert_eq!(
            resolve_url(base, "/page/2").as_deref(),
            Some("https://x.io/page/2")
        );
        assert_eq!(
            resolve_url(base, "https://y.io/z").as_deref(),
            Some("https://y.io/z")
        );
        // Не-HTTP схемы отсекаются.
        assert_eq!(resolve_url(base, "mailto:a@b.io"), None);
        assert_eq!(resolve_url(base, "javascript:void(0)"), None);
    }

    #[test]
    fn should_follow_filters_host_and_pattern() {
        let page = "https://x.io/a/";
        let f = Follow {
            selector: "a".into(),
            max_depth: 1,
            pattern: Some("/page/".into()),
            same_host: true,
        };
        assert!(should_follow(&f, page, "https://x.io/page/2"));
        assert!(!should_follow(&f, page, "https://x.io/other"));
        assert!(!should_follow(&f, page, "https://y.io/page/2"));

        let f2 = Follow {
            selector: "a".into(),
            max_depth: 2,
            pattern: None,
            same_host: false,
        };
        assert!(should_follow(&f2, page, "https://y.io/"));
    }
}
