# Arachne — Стелс-краулер на Rust

> Многопоточный «комбайн» для скрытного парсинга веб-сайтов: комбинирует сверхбыстрый HTTP-режим с эмуляцией браузера для обхода антибот-защит.

## О проекте

Arachne — это pure-Rust движок скрытного краулинга, который на лету переключает сессию между двумя режимами:

- **Fast HTTP режим** — парсинг API и статического HTML с полной имитацией браузерного сетевого отпечатка (TLS JA4, HTTP/2).
- **Headless-браузерный режим** — эмуляция реальных действий пользователя в **собственном модульном JS/DOM-движке** (без Chromium и CDP, без готовых браузеров) для прохождения JS-защит.

Цель — выглядеть как самый обычный пользователь: от TLS-рукопожатия до поведения мыши.

## Содержание документации

| № | Документ | О чём |
|---|----------|-------|
| 1 | [01-tech-stack.md](01-tech-stack.md) | Технологический стек и ключевые фичи |
| 2 | [02-stealth.md](02-stealth.md) | Стелс-фичи, анти-детекшн, обход защит |
| 3 | [03-smart-routing.md](03-smart-routing.md) | Smart Router: выбор режима, эскалация сессий |
| 3b | [03-job-yaml.md](03-job-yaml.md) | Формат job.yaml: селекторы, вложенные селекторы, прокси, лимиты |
| 4 | [04-architecture.md](04-architecture.md) | Базовая архитектура: сессии, память, DOM-стриминг |
| 5 | [05-rust-patterns.md](05-rust-patterns.md) | Специфические Rust-паттерны (Newtype, Typestate) |
| 6 | [06-infrastructure.md](06-infrastructure.md) | Сетевая инфраструктура (базовый вариант) |
| 7 | [07-observability.md](07-observability.md) | Мониторинг, отладка, логирование |
| 8 | [08-development.md](08-development.md) | План разработки: фазы A/B/C, вехи M0–M4, чек-лист |
| 9 | [09-analogues.md](09-analogues.md) | Обзор аналогов и заимствуемые практики |
| 10 | [10-resources.md](10-resources.md) | Ресурсы для изучения и реверс-инжиниринга |
| 11 | [11-browser-engines.md](11-browser-engines.md) | Свой модульный движок, отказ от готовых браузеров |
| 12 | [rules.md](rules.md) | Правила для нейросетей при работе с кодовой базой |
| 13 | [backlog.md](backlog.md) | Backlog пожеланий и идей |
| 14 | [13-cicd-docker.md](13-cicd-docker.md) | CI/CD (GitHub Actions) и Docker-контейнеризация |
| 15 | [14-linux-target.md](14-linux-target.md) | Linux как первоклассный production-таргет |
| 16 | [15-proxy-infra.md](15-proxy-infra.md) | Продвинутая proxy/DNS инфраструктура (поздний этап) |
| 17 | [16-llm.md](16-llm.md) | LLM-интеграции: Vision-селекторы, капчи ONNX (поздний этап) |
| 18 | [17-ebpf.md](17-ebpf.md) | eBPF / TUN-TAP на уровне ядра (экспериментальный этап) |
| 19 | [18-distributed.md](18-distributed.md) | Распределённые ноды / кластеринг (отложенный этап) |
| 20 | [19-saas-platform.md](19-saas-platform.md) | SaaS-платформа: low-code конструктор, мультитенантность, биллинг, API (перспектива) |

## Технологический стек (кратко)

Фактические зависимости Фазы A (см. `Cargo.toml` крейтов):

- **Сеть / имперсонация:** default — `reqwest` + `rustls` + `webpki-roots` (pure-Rust, собирается без C-тулчейна); за feature-флагом `impersonation` в `arachne-net` — `wreq` (BoringSSL, профили Chrome133/Firefox133/Safari18_5: TLS JA4, HTTP/2); `tokio`
- **DOM-парсинг:** `scraper` (html5ever) — CSS-селекторы, рекурсивные вложенные списки
- **Данные / конфигурация:** `serde` / `serde_json` / `serde_yaml`, `csv`, `rusqlite` (feature `sqlite` в `arachne-export`), `url`, `bytes`
- **CLI / инфраструктура:** `clap`, `tracing` + `tracing-subscriber`, `anyhow`, `thiserror`, атомики (lock-free счётчики)
- **Архитектурный каркас:** CQRS-шина `arachne-cqrs` (in-process, `async-trait`) — задел под Фазу B/C ([18-distributed.md](18-distributed.md))
- **Веб-слой (перспектива SaaS):** `axum` (REST API) + `leptos` (SSR/CSR), биллинг по подписке/запросу, API-ключи — см. [19-saas-platform.md](19-saas-platform.md)

Полный разбор каждого компонента — в [01-tech-stack.md](01-tech-stack.md).

## Стратегия фаз

Разработка идёт **тремя фазами** — каждая фаза самодостаточна и является точкой выхода с рабочим продуктом; переход к следующей — **по спросу, не по календарю** (детали и вехи M0–M4 — [08-development.md](08-development.md)):

| Фаза | Суть | Гейт |
|---|---|---|
| **A. Штамп-ядро** | Fast HTTP (`reqwest`/`wreq`) + DOM-парсинг (`scraper`/`html5ever`) + экспорт CSV/JSONL/SQLite + `job.yaml` + resume; движок и Smart Router **не входят** | Устойчивый прогон 10к–100к страниц без бана + resume |
| **B. Полноценный single-node** | Собственный модульный движок, Smart Router, полный stealth, Shadow Recorder | Стабильное прохождение JS-защит |
| **C. Мультинода + платформы** | Master/Worker поверх CQRS-шины, матрица платформ (musl/ARM64/контейнеры), eBPF — опционально | Линейное масштабирование воркеров |

**SaaS-платформа — вне фаз** (перспектива без гейтов, решение о старте отдельно): см. [19-saas-platform.md](19-saas-platform.md).

## Статус

**Фаза A «Штамп-ядро» — реализована (вехи M0–M3).** Codebase — Cargo workspace из шести крейтов:

| Крейт | Назначение |
|---|---|
| `arachne-domain` | Newtype-типы (`Url`, `ProxyAddr`, `TaskId`, `SessionId`, `PageId`, `Millis`, `Seconds`, `RamLimitBytes`, `Html(Bytes)`, `Selector`) + типизированные ошибки (rules.md §2); домен job.yaml: `Loop`/`RangeLoop`, `While`/`StopCondition`, `NestedSelector`/`NestedField`, `OutputTemplate` |
| `arachne-cqrs` | CQRS-каркас: трейт `Bus`, `Command`/`Query`, in-process реализация (docs/05-rust-patterns.md §5.8) — задел под Фазу B/C, пока CLI работает напрямую |
| `arachne-net` | Транспорт: generic `StealthSession<B: HttpFetch>` — ротация прокси round-robin, ретраи с экспоненциальным backoff (429 → ретрай, остальные статусы — наружу для `while`-границ), `SessionStats` на атомиках; гауссов джиттер (Box-Muller) с внедряемым RNG (`JitterRng`) — rules.md §8; `RequestContext` (заголовки/куки из job.yaml) |
| `arachne-parse` | DOM-парсинг поверх `scraper` (html5ever): плоские CSS-селекторы, рекурсивные вложенные селекторы (`nested`, произвольная глубина, спец-селектор `"."` для листовых элементов), `extract_links` (href по селектору) для BFS-обхода |
| `arachne-export` | Экспорт CSV/JSONL/JSON/SQLite (SQLite за feature-флагом `sqlite`); шаблонизатор `output_template`: `{{field}}`, `{{field[*]}}`, секция `__each__` |
| `arachne` | CLI-бинарник (`clap`): `arachne crawl --job job.yaml --output out.jsonl`; декларативный job.yaml — `loops` (декартово произведение `{var}`), `while` с авто-границей (`stop_when: {status\|text\|text_not}`), `follow` — BFS-обход по ссылкам (дедуп URL, `max_depth`, `pattern`, `same_host`), экспорт по расширению файла, `--delay-ms`, `--task-id` |

Транспорт — двухбэкендный (за фича-флагом `impersonation` в `arachne-net`):

- **default** — pure-Rust `reqwest` + `rustls` + `webpki-roots` (собирается на Windows **без** C-тулчейна);
- **`--features impersonation`** — `wreq` (BoringSSL, профили `wreq-util::Profile::{Chrome133, Firefox133, Safari18_5}`): полная TLS-имперсонация (JA4, HTTP/2). Требует clang/LLVM; на Windows без clang — dev через WSL/Linux (docs/08-development.md §8.1, docs/14-linux-target.md).

> Примечание об экосистеме: крейт `rquest` отзнан (yanked) автором с crates.io в пользу форка **`wreq`** (тот же автор, `0x676e67`). Документация упоминает `rquest`/`reqwest-impersonate` — в коде Фазы A используется `wreq` как прямой преемник (hard fork of reqwest, Apache-2.0).

CI: `.github/workflows/ci.yml` — fmt → clippy `-D warnings` → test → build (ubuntu-musl + windows-msvc) → bench (baseline для `arachne-parse`, `arachne-net`). Юнит-тесты: **55** в workspace (`cargo test --workspace`). Дальнейшие вехи M2–M4 (очередь URL + дедуп + чекпоинт-резюм, профиль-отчёт) — дорожная карта в [08-development.md](08-development.md). Общие правила — в [rules.md](rules.md).

## Примечание

Исторические упоминания `rquest`/`reqwest-impersonate` в старых разделах документации означают тот же класс инструментов (TLS-имперсонация на BoringSSL); фактический стек см. в разделе «Технологический стек» выше и [01-tech-stack.md](01-tech-stack.md).