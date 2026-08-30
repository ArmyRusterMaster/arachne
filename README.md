# 🕷️ Arachne — Стелс-краулер на Rust

Многопоточный «комбайн» для скрытного парсинга веб-сайтов. Комбинирует сверхбыстрый HTTP-режим с эмуляцией браузера для обхода антибот-защит.

## ✨ Ключевые возможности

- **Два режима работы:** Fast HTTP (статика/API) → Headless (JS-рендеринг, Фаза B)
- **Имперсонация браузера:** TLS JA4, HTTP/2, профили Chrome/Firefox/Safari
- **Декларативные задания:** job.yaml вместо жёсткода
- **Стелс-техники:** ротация прокси, rate-limit с гауссовым джиттером, retry+backoff
- **Вложенные селекторы:** произвольная глубина вложенности, рекурсия
- **BFS-обход:** переход по ссылкам с дедупцией и лимитами

## 🚀 Быстрый старт

### Требования

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- LLVM/Clang (опционально, для TLS-имперсонации)

### Сборка

```bash
# Debug-сборка (быстрая, для разработки)
cargo build

# Release-сборка (оптимизированная)
cargo build --release

# С TLS-имперсонией (требует clang)
cargo build --release --features impersonation
```

### Запуск

```bash
# Базовый запуск
cargo run -- crawl --job job.yaml --output data.jsonl

# Release с параметрами
./target/release/arachne crawl --job job_quotes.yaml --output quotes.jsonl --delay-ms 500

# С переопределением task-id
arachne crawl --job job.yaml --output out.csv --task-id 42
```

## 📋 job.yaml — декларативный формат заданий

```yaml
# Стартовые URL (поддерживают {var} из loops)
start_urls:
  - https://example.com/page/{page}/

# Циклы подстановки переменных
loops:
  - var: page
    range: { start: 1, end: 10 }

# Плоские CSS-селекторы (первое совпадение)
selectors:
  - name: title
    selector: h1
  - name: description
    selector: meta[name="description"]@content

# Вложенные селекторы (списки с полями)
nested_selectors:
  - repeat_selector: .item          # Повторяющийся блок
    fields:
      - name: name
        selector: .name
      - name: price
        selector: .price
    nested:                        # Вложенный уровень (рекурсия)
      - repeat_selector: .tag
        fields:
          - name: tag_name
            selector: "."          # "." = текст самого блока

# Переход по ссылкам (BFS-обход)
follow:
  selector: "a.next"
  max_depth: 3
  pattern: "/page/"
  same_host: true

# Заголовки и cookie
headers:
  - name: Accept-Language
    value: en-US
cookies:
  - name: session
    value: "{page}"

# Пул прокси (round-robin)
proxies:
  - http://user:pass@1.2.3.4:8080
  - socks5://5.6.7.8:1080

# Лимит страниц
limit: 100
```

### Параметры командной строки

| Параметр | Описание | По умолчанию |
|----------|----------|--------------|
| `--job` | Путь к job.yaml | обязательно |
| `--output` | Выходной файл (формат по расширению) | обязательно |
| `--delay-ms` | Средняя задержка между запросами (мс) | 200 |
| `--task-id` | ID задачи в записях | 1 |

### Форматы вывода

- `.csv` — CSV с колонками: task_id, page_id, url, field, value
- `.jsonl` — JSON Lines (один JSON-объект на строку)
- `.json` — Структурированный JSON (требует `output_template`)
- `.sqlite` / `.db` — SQLite (требует feature `sqlite`)

## 📁 Структура проекта

```
arachne/
├── crates/
│   ├── arachne/           # CLI-бинарник
│   ├── arachne-domain/    # Newtype-типы (Url, TaskId, Html...)
│   ├── arachne-net/       # HTTP-транспорт (reqwest/wreq, прокси, jitter)
│   ├── arachne-parse/     # DOM-парсинг (html5ever + CSS-селекторы)
│   ├── arachne-export/    # Экспорт (CSV/JSONL/SQLite)
│   └── arachne-cqrs/      # CQRS-каркас (задел под мультинодность)
├── docs/                  # Документация
├── job.yaml               # Пример задания
├── Cargo.toml             # Workspace manifest
└── .github/workflows/     # CI/CD
```

## 🧪 Тестирование

```bash
# Все тесты workspace
cargo test --workspace

# Тесты конкретного крейта
cargo test -p arachne-parse
cargo test -p arachne-net

# С выводом логов
cargo test -- --nocapture
```

## ⚙️ Конфигурация

### TLS-имперсонация (опционально)

Для полной эмуляции браузерного отпечатка (TLS JA4, HTTP/2):

1. Установите LLVM/Clang: https://releases.llvm.org/
2. Соберите с feature-флагом:

```bash
cargo build --release --features impersonation
```

> **Примечание:** На Windows без clang dev-таргет — WSL/Linux.

### Прокси

Укажите в job.yaml:

```yaml
proxies:
  - http://user:pass@host:port
  - socks5://host:port
  - socks5h://user:pass@host:port  # DNS через прокси
```

## 📊 Статус проекта

| Фаза | Статус | Описание |
|------|--------|----------|
| **A. Штамп-ядро** | ✅ Готово | Fast HTTP, парсинг, экспорт, CLI |
| **B. Single-node** | 📅 План | Собственный движок, Smart Router |
| **C. Мультинода** | 📅 План | Master/Worker поверх CQRS |
| **SaaS** | 📅 Перспектива | Low-code конструктор, биллинг |

Подробная дорожная карта: [docs/08-development.md](docs/08-development.md)

## 🛡️ Правила для AI-агентов

См. [docs/rules.md](docs/rules.md) — обязательные правила для LLM-агентов при работе с кодовой базой.

## 📚 Документация

- [docs/README.md](docs/README.md) — Полная документация
- [docs/01-tech-stack.md](docs/01-tech-stack.md) — Технологический стек
- [docs/02-stealth.md](docs/02-stealth.md) — Стелс-фичи и обход защит
- [docs/03-job-yaml.md](docs/03-job-yaml.md) — Формат job.yaml
- [docs/04-architecture.md](docs/04-architecture.md) — Архитектура
- [docs/05-rust-patterns.md](docs/05-rust-patterns.md) — Rust-паттерны
- [docs/08-development.md](docs/08-development.md) — Дорожная карта

## 📄 Лицензия

Apache-2.0
