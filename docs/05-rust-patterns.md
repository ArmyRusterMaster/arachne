# 5. Специфические Rust-паттерны для краулера

> Паттерны **Newtype** и **Typestate** обязательны к применению — правила для них зафиксированы в [rules.md](rules.md). Здесь — примеры применения.

## 5.1. Асинхронные трейты (Async Traits) для логики шагов

Спецификация сценариев (макросов) часто требует гибкости. С появлением нативного синтаксиса `async fn` в трейтах (с Rust 1.75) можно создать чистый интерфейс для шагов автоматизации:

```rust
pub trait BrowserAction {
    async fn execute(&self, tab: &mut Tab) -> Result<(), ActionError>;
}
```

Это позволяет писать кастомные плагины/действия (например, `HumanClick`, `SmartScroll`) в виде отдельных изолированных модулей.

## 5.2. Паттерн Newtype

Оборачиваем примитивные типы в новые типы, чтобы компилятор не путал домены и ошибки ловились на этапе компиляции, а не в рантайме:

```rust
// Плохо: URL и Proxy строкой — легко перепутать
fn fetch(url: String, proxy: String) { ... }

// Хорошо: Newtype — каждый домен — свой тип
pub struct Url(String);
pub struct ProxyAddr(String);
pub struct SessionId(Uuid);
pub struct Millis(u64);
pub struct RamLimitBytes(usize);

pub fn fetch(url: &Url, proxy: Option<&ProxyAddr>) -> Result<Html, FetchError> { ... }
```

**Правила:**
- идентификаторы (`SessionId`, `TaskId`, `PageId`) — всегда Newtype, никогда «голый» `Uuid`/`u64`;
- единицы измерения (`Millis`, `Bytes`) — всегда Newtype;
- конструирование — через `TryFrom` с валидацией (URL не должен быть невалидным в рантайме).

## 5.3. Паттерн Typestate

Состояния сессии/режима кодируются на уровне типов: нельзя вызвать `headless`-действие у Fast HTTP-сессии:

```rust
// Fast-режим: доступны только HTTP-операции
pub struct Session<Mode> { ... }
pub struct Fast;
pub struct Headless;

impl Session<Fast> {
    pub fn get(self, url: &Url) -> Result<(Self, Html), FetchError> { ... }
    // Переход в headless возможен ТОЛЬКО через явный метод — компилятор
    // гарантирует, что cookie были перенесены
    pub fn escalate(self) -> Session<Headless> { ... }
}

impl Session<Headless> {
    pub fn click(self, sel: &Selector) -> Result<(Self, Html), ActionError> { ... }
    pub fn back_to_fast(self) -> Session<Fast> { ... }
}
```

Так невозможно по ошибке вызвать `click()` у HTTP-сессии, а перенос cookie при `escalate()` становится **обязательным по типу**, а не по соглашению.

## 5.4. Использование `bytes::Bytes` для Zero-Copy передачи данных

При переключении в режим FastHttp (`reqwest`/`wreq`) и передаче HTML в парсер `scraper` **никогда не копировать строки через `.clone()`**. Использовать тип `Bytes` из одноимённого крейта:

- работает как атомарно-подсчитываемый указатель (Arc) на область памяти;
- позволяет обрабатывать гигабайты страниц в секунду без нагрузки на аллокатор памяти.

## 5.5. Пул сетевых соединений (перспектива)

> **Статус:** план (зависимость `moka` пока не входит в стек, см. [01-tech-stack.md](01-tech-stack.md)).

Антиботы вычисляют скрипты по **аномально высокой скорости открытия новых TCP-соединений**. Крейт `mока` (высокопроизводительный асинхронный кэш на Rust) планируется для создания пула **«прогретых» сетевых сессий и прокси-соединений**, чтобы переиспользовать их (Keep-Alive) и имитировать поведение обычного браузера, который держит вкладки открытыми.

## 5.6. Нативные Rust-биндинги к JS вместо инъекций

Вместо выполнения скрытых JS-скриптов перед загрузкой страницы — привязка свойств `navigator`, `screen`, `window` напрямую на Rust, скомпилированных как встроенные функции движка `boa`. Это делает код неотличимым от «родного» браузерного (подробнее — [02-stealth.md](02-stealth.md)).

## 5.7. Lock-free структуры данных и асинхронная конвейерная обработка

Всё ядро работает на **lock-free структурах данных** — без блокирующих `Mutex`/`RwLock` в горячих путях. Всё это даёт нулевую конкурентную деградацию и упрощает перенос в мультинодный режим:

```rust
// MPMC-канал без блокировок — канал point-to-point и fan-out
use flume;

// Общая конкурентная карта без блокировок на чтение
use dashmap::DashMap;

// Эпохальная сборка (lock-free трейты, конкурентные списки/деки)
use crossbeam_epoch as epoch;
```

**Правила:**
- обмен сообщениями между потоками — только через каналы (`flume`, `crossbeam-channel`, `tokio::sync::mpsc` с backpressure);
- общее конкурентное состояние — `DashMap`/`crossbeam`, **не** `Mutex<HashMap>` (лидирует на hotspots);
- счётчики и флаги — `AtomicU64`/`AtomicBool` с правильным ordering, без глобальных блокировок;
- большие куски данных передаются как `Arc<Bytes>`/`Bytes` (zero-copy), не копируются.

> Детальный разбор — [18-distributed.md](18-distributed.md#184-локфри-структуры-данных).

## 5.8. CQRS-принцип: команды и запросы для стриминга и мультинодности

Архитектура строится на **CQRS (Command-Query Responsibility Segregation)** даже в одном бинарнике — командный и запросный пути разделены. Это даёт стрим и асинхронность сегодня, а завтра облегчает переход в мультинодный режим.

```rust
// Команды изменяют состояние (эскалация, ротация прокси, запуск задачи).
pub enum Command { RunTask(TaskId), Escalate(SessionId), RotateProxy(SessionId) }

// Запросы читают данные (статус, метрики, результаты) без побочных эффектов.
pub enum Query { GetStatus(SessionId), ListWorkers, GetResult(TaskId) }

// Единая точка входа — шина команд/запросов, реализация за трейтом.
pub trait Bus: Send + Sync {
    fn dispatch(&self, cmd: Command) -> Pin<Box<dyn Future<Output = Result<(), BusError>> + Send>>;
    fn query(&self, q: Query)  -> Pin<Box<dyn Future<Output = Result<Value, BusError>> + Send>>;
}
```

**Зачем CQRS:**
- команды и запросы оптимизируются независимо (команды идемпотентны, запросы кешируются);
- стриминг результатов — через `futures::Stream`/каналы: данные текут кусками, а не ждут полной готовности;
- транспортно независимы: та же `Command`/`Query` посылается через очередь в мультинодном режиме ([18-distributed.md](18-distributed.md)), т.е. локальная шина == распределённая шина.
## 5.9. Паттерн Builder (Строитель)

Для сложных объектов с множеством параметров (конфиг сценария, настройки сессии, кастомный `NetRequest`) используем строитель. Две формы — по потребности:

```rust
// Consuming: методы принимают self и «сжигают» предыдущее состояние.
pub struct ScenarioBuilder { steps: SmallVec<[Step; 8]> }
impl ScenarioBuilder {
    pub fn new() -> Self { Self { steps: SmallVec::new() } }
    // consuming: возвращает новое состояние, инвалидируя прежнее
    pub fn step(self, s: Step) -> Self {
        let mut b = self;
        b.steps.push(s);
        b
    }
    pub fn build(self) -> Scenario { Scenario { steps: self.steps.into_vec() } }
}

// Non-consuming: методы принимают &mut self — билдер переиспользуется по месту.
pub struct ConfigBuilder { max_retries: u32 }
impl ConfigBuilder {
    pub fn retries(&mut self, n: u32) -> &mut Self { self.max_retries = n; self }
    pub fn build(&self) -> Config { Config { max_retries: self.max_retries } }
}
```

- **Consuming** — для **линейных конвейеров** (шаг строится один раз) и когда нужен Typestate-контроль невалидных последовательностей.
- **Non-consuming** — для канонических конфигов, которые расширяют по месту и переиспользуют.

## 5.10. `Cow`, `SmallVec` / `ArrayVec` и экономия на аллокациях

- **`Cow<'a, B>`** (Clone-on-Write) — «ленивое» владение: пока данные не изменены, работаем со ссылкой без копирования; первая мутация делает ленивый переход `Borrowed → Owned`. Идеально для полей «до/после нормализации» (URL, заголовки, конфиг-строка).
  ```rust
  fn canon<'a>(u: &'a str) -> Cow<'a, str> {
      if u.starts_with("http") { Cow::Borrowed(u) } else { Cow::Owned(format!("https://{u}")) }
  }
  ```
- **`SmallVec` / `ArrayVec`** — размещение малых коллекций с известным верхним пределом **на стеке**, без аллокаций в куче. Применяем в hot-path парсинга (`html5ever`, заголовки, query-параметры) и в узких структурах (например, список шагов сценария редко превышает 8 элементов).

> Правила для нейросетей по применению паттернов — в [rules.md](rules.md). Инфраструктурные сетевые хитрости — в [06-infrastructure.md](06-infrastructure.md) и [15-proxy-infra.md](15-proxy-infra.md).