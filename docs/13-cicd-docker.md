# 13. CI/CD и контейнеризация

## 13.1. CI-пайплайн (GitHub Actions)

Каждый PR проходит: `fmt` → `clippy -D warnings` → `test` → сборка обоих таргетов.

```yaml
name: CI
on: [push, pull_request]
jobs:
  lint-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest   # Linux — первоклассный таргет (14-linux-target.md)
            target: x86_64-unknown-linux-musl
          - os: windows-latest  # Windows — среда разработки
            target: x86_64-pc-windows-msvc
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }}
```

## 13.2. Сборка Linux-бинарников

- `x86_64-unknown-linux-musl` — статический бинарник «поставил и работает» на любом дистрибутиве;
- крейты с C-зависимостями (OpenSSL и т.п.) не используем — всё pure-Rust (`rquest` с `rustls`/BoringSSL-patch, `trust-dns`), чтобы musl-сборка проходила без боли;
- подробности таргета — [14-linux-target.md](14-linux-target.md).

## 13.3. Docker-образ

Многоступенчатая сборка: компиляция в `rust:alpine`, рантайм — минимальный образ:

```dockerfile
# --- Этап сборки ---
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev mold
WORKDIR /build
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

# --- Этап рантайма ---
FROM scratch
# only-static: бинарник не требует ни libc, ни сертификатов в системе
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/arachne /arachne
# CA-сертификаты для TLS (rustls читает их из файла, если задан путь)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
ENTRYPOINT ["/arachne"]
```

Итоговый образ — **менее 20 МБ** (только статический бинарник + CA-сертификаты).

## 13.4. Стратегия релизов

- Версионирование: [SemVer](https://semver.org/), теги `vX.Y.Z`;
- артефакты релиза: бинарники Windows (msvc) и Linux (musl) — attachment к GitHub Release;
- changelog **генерируется автоматически** (см. [13.5](#135-автогенерация-changelog)), не ведётся вручную;
- ключевые фичи (Smart Router, stealth-профили) помечаются в changelog фазами из [08-development.md](08-development.md).

## 13.5. Автогенерация changelog

**Changelog не редактируется руками.** Источник истины — история Conventional Commits. Генератор — [git-cliff](https://git-cliff.org) (или `cargo-semver-checks` для контроля версии), он шаблонизирует `CHANGELOG.md` и сам ставит версию/тег.

### Правила коммитов (Conventional Commits)
Каждый PR обязан иметь заголовки вида:
- `feat(<scope>): ...` — новая фича → bump MINOR;
- `fix(<scope>): ...` — исправление → bump PATCH;
- `docs(<scope>): ...`, `chore(<scope>): ...`, `test(...)`, `refactor(...)` — не влияют на версию, но попадают в changelog.
- ломающие изменения — `feat!:` / `BREAKING CHANGE:` → bump MAJOR.

Правила для ИИ-агентов — [rules.md](../rules.md#11-коммиты-и-changelog).

### Пайплайн релиза (полностью автоматический)
```
push/PR --> Conventional Commits --> CI (fmt, clippy, test)
        --> git-cliff generate CHANGELOG.md  (из коммитов между тегами)
        --> bump версии (SemVer) + тег vX.Y.Z
        --> cargo build --release (Windows + Linux musl)
        --> GitHub Release с бинарниками + авто-CHANGELOG
```

### Конфиг `cliff.toml` (мин.)
```toml
[changelog]
header = "# Changelog"
body = """
{% for group, commits in commits | group_by(attribute="group") %}
## {{ group | upper_first }}

{% for commit in commits %}
- {{ commit.message | upper_first }}
{%- endfor %}
{% endfor %}
"""
filter_unconventional = true

[git]
# разбирать только conventional commits
commit_parsers = [
  { message = "^feat", group = "Features" },
  { message = "^fix",  group = "Bug Fixes" },
  { message = "^docs", group = "Docs" },
  { message = "^chore|^test|^refactor", group = "Other" },
  { message = ".*",    group = "Other" },
]
```

### Препятствия / проверка CI
- `git diff --exit-code CHANGELOG.md` в stage: если разработчик забыл сгенерировать, CI сам подтянет;
- `cargo-semver-checks` — ловит случайный breaking change до публикации.

## 13.6. Бенчмарки, профиль и чек-лист CI

- [ ] `cargo fmt --check` и `clippy -D warnings` блокируют мерж.
- [ ] Каждая правка кода сопровождена **попутным тестом** (TDD-инвариант) — см. [rules.md](../rules.md#8-тестирование).
- [ ] Тесты не зависят от внешней сети (моки/`--ignored`).
- [ ] Оба таргета (Windows/Linux musl) собираются на каждый PR.
- [ ] Docker-образ собирается и запускается smoke-тестом (`/arachne --version`).
- [ ] **Бенчмарки** (`criterion`) для hot-path вызываются в CI с `--release` — сравнение с baseline, регрессия → fail.
- [ ] Профилирование крупных фаз — `cargo flamegraph`/`perf`, отчёт в `docs/benches/` (см. [08-development.md](08-development.md#83-профилирование-и-отчёт-после-каждой-крупной-фазы)).
- [ ] Кэш зависимостей (`rust-cache`) включён — пайплайн < 10 минут.

> CI-бенчмарки: job на hot-path крейтах (`arachne-net`, `arachne-dom`, `arachne-stream`), gate — отсутствие регрессии относительно прошлого baseline.

### 13.6.1. Пример job бенчмарков (criterion + gate на регрессию)

```yaml
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: criterion
      - uses: Swatinem/rust-cache@v2
      # baseline: сохраняем результаты дефолтного запуска как эталон
      - run: cargo bench -p arachne-dom -p arachne-net -p arachne-stream -- --save-baseline prev
      # при изменении кода запускаем снова и сравниваем с prev; регрессия -> fail
      - uses: boa-dev/criterion-compare-action@v3
        with:
          benchName: prev
          compareTo: current
      - run: echo "benchmark regression gate passed"
```

> Принцип: бенчмарки дают **baseline**, а CI сравнивает «было → стало». Регрессия по времени/памяти → красный PR. Так крупные оптимизации измеряются, а не «по ощущениям».