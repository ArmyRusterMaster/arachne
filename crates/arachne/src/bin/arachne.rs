//! Arachne CLI entry point.
//!
//! Phase A («Штамп-ядро»): fetch → DOM → CSS selectors → export.
//! Собственный JS/DOM-движок и Smart Router входят в Фазу B.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use tracing::info;
use tracing_subscriber::EnvFilter;

use arachne_domain::{PageId, Selector, TaskId, Url};
use arachne_export::Record;
use arachne_net::{DefaultSession, GaussJitter, OsJitterRng, SessionConfig};
use arachne_parse::Dom;

/// CLI for Arachne stealth crawler.
#[derive(Parser)]
#[command(name = "arachne", version, about = "Stealth crawler — Phase A stamp kernel")]
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
    },
}

/// Declarative job descriptor (веха M3 — job.yaml).
#[derive(Deserialize)]
struct Job {
    start_urls: Vec<String>,
    #[serde(default)]
    selectors: Vec<SelectorMapping>,
    #[serde(default)]
    proxies: Vec<String>,
    #[serde(default)]
    limit: Option<u64>,
}

#[derive(Deserialize)]
struct SelectorMapping {
    name: String,
    selector: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.cmd {
        Commands::Crawl { job, output, delay_ms } => {
            run_crawl(&job, output.as_deref(), delay_ms).await
        }
    }
}

async fn run_crawl(job_path: &Path, output: Option<&Path>, delay_ms: u64) -> anyhow::Result<()> {
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

    let task_id = TaskId::new(0);
    let mut records: Vec<Record> = Vec::new();
    let mut page_id = PageId::new(0);

    for (i, raw_url) in job.start_urls.iter().enumerate() {
        // Rate limit: пауза между запросами (кроме первого), mean ± sigma/2.
        if i > 0 {
            let d = jitter.delay_ms(delay_ms, delay_ms / 2);
            tokio::time::sleep(std::time::Duration::from(d)).await;
        }

        let url = Url::try_from(raw_url.clone())?;
        let html = session.get(&url).await.context("fetch failed")?;
        let dom = Dom::parse(&html)?;

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

        page_id = PageId::new(page_id.get() + 1);
        if let Some(limit) = job.limit {
            if page_id.get() >= limit {
                info!("limit {} reached, stopping", limit);
                break;
            }
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
            "jsonl" => arachne_export::to_jsonl(out, &records)?,
            "sqlite" | "db" => arachne_export::to_sqlite(out, &records)?,
            other => anyhow::bail!("unsupported output extension: {other}"),
        }
        info!("wrote {} record(s) to {}", records.len(), out.display());
    } else {
        println!("{}", serde_json::to_string_pretty(&records)?);
    }

    Ok(())
}
