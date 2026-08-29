//! Stealth HTTP transport for Arachne (Phase A).
//!
//! Два бэкенда за фича-флагом `impersonation`:
//! - **включён** — `wreq` (BoringSSL): TLS-имперсонация браузера (JA4, HTTP/2),
//!   профили из `wreq-util` (`Profile::Chrome133` и др.). Требует clang/LLVM
//!   на Windows (docs/08-development.md §8.1 — иначе dev в WSL/Linux).
//! - **выключен (default)** — pure-Rust `reqwest` + `rustls`, собирается без C-тулчейна.
//!
//! Stealth-паттерны: ротация прокси round-robin, rate-limit с гауссовым джиттером
//! (детерминированный RNG через трейт [`JitterRng`], rules.md §8).

pub mod jitter;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

use arachne_domain::{Html, ProxyAddr, Url};

pub use jitter::{GaussJitter, JitterRng, OsJitterRng};

/// TLS impersonation profile matching a real browser.
///
/// Маппится на `wreq_util::Profile::*` при feature `impersonation`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Impersonation {
    #[default]
    Chrome,
    Firefox,
    Safari,
}

/// Configuration for [`StealthSession`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub impersonation: Impersonation,
    /// Пул прокси (round-robin); пустой = прямое соединение.
    pub proxies: Vec<ProxyAddr>,
    /// База джиттера в миллисекундах (среднее гауссова распределения).
    pub jitter_ms: u64,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            impersonation: Impersonation::default(),
            proxies: Vec::new(),
            jitter_ms: 200,
            timeout_secs: 30,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Error)]
pub enum NetError {
    #[error("HTTP request failed: {0}")]
    Request(String),
    #[error("status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("timeout after {0} secs")]
    Timeout(u64),
    #[error("invalid proxy: {0}")]
    BadProxy(String),
}

/// Статистика сессии (для observability, docs/07). Атомики — rules.md §7 lock-free.
#[derive(Debug, Default)]
pub struct SessionStats {
    pub requests: AtomicU64,
    pub errors: AtomicU64,
    pub rate_limited: AtomicU64,
}

impl SessionStats {
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            requests: self.requests.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            rate_limited: self.rate_limited.load(Ordering::Relaxed),
        }
    }
}

/// Копия метрик на момент вызова.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct StatsSnapshot {
    pub requests: u64,
    pub errors: u64,
    pub rate_limited: u64,
}

/// Контекст запроса: дополнительные заголовки и cookie.
///
/// Заполняется из job.yaml (циклы подставляют `{var}` в значения).
#[derive(Debug, Default, Clone)]
pub struct RequestContext {
    /// Дополнительные заголовки запроса.
    pub headers: Vec<(String, String)>,
    /// Cookie в формате k=v; отправляются одним заголовком `Cookie`.
    pub cookies: Vec<(String, String)>,
}

impl RequestContext {
    /// Собрать значение заголовка `Cookie` из пар, если есть.
    pub fn cookie_header(&self) -> Option<String> {
        if self.cookies.is_empty() {
            return None;
        }
        Some(
            self.cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }
}

/// Бэкенд-независимое ядро сессии: ротация прокси, счётчики, ретраи.
/// Конкретный HTTP-бэкенд подключается через [`HttpFetch`].
pub struct StealthSession<B: HttpFetch> {
    backend: B,
    config: SessionConfig,
    proxy_idx: AtomicUsize,
    stats: SessionStats,
}

impl<B: HttpFetch> StealthSession<B> {
    pub fn new(backend: B, config: SessionConfig) -> Self {
        Self {
            backend,
            config,
            proxy_idx: AtomicUsize::new(0),
            stats: SessionStats::default(),
        }
    }

    pub fn stats(&self) -> &SessionStats {
        &self.stats
    }

    /// Next proxy from the pool (round-robin). `None` = direct connection.
    pub fn next_proxy(&self) -> Option<ProxyAddr> {
        if self.config.proxies.is_empty() {
            return None;
        }
        let i = self.proxy_idx.fetch_add(1, Ordering::Relaxed);
        Some(self.config.proxies[i % self.config.proxies.len()].clone())
    }

    /// GET с ретраями и экспоненциальным backoff (база — `jitter_ms`).
    /// Гауссов джиттер применяется вызывающей стороной через [`GaussJitter`]
    /// перед вызовом (детерминированный RNG — rules.md §8).
    pub async fn get(&self, url: &Url) -> Result<Html, NetError> {
        let ctx = RequestContext::default();
        self.get_with(url, &ctx).await
    }

    /// GET с ретраями, backoff и контекстом запроса (заголовки/куки).
    /// 429 → ретрай; остальные статусы возвращаются как [`NetError::Status`]
    /// вызывающему (например, для `while`-циклов по границе 404).
    pub async fn get_with(&self, url: &Url, ctx: &RequestContext) -> Result<Html, NetError> {
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            match self.backend.fetch_with(url, ctx).await {
                Ok(html) => {
                    Self::bump(&self.stats.requests, 1);
                    return Ok(html);
                }
                Err(NetError::Status { status: 429, body }) => {
                    Self::bump(&self.stats.errors, 1);
                    Self::bump(&self.stats.rate_limited, 1);
                    warn!("rate-limited (429) for {}, attempt {attempt}", url);
                    if attempt > self.config.max_retries {
                        return Err(NetError::Status { status: 429, body });
                    }
                }
                Err(e) => {
                    Self::bump(&self.stats.errors, 1);
                    if attempt > self.config.max_retries {
                        return Err(e);
                    }
                    debug!(
                        "retry {attempt}/{} after error: {e}",
                        self.config.max_retries
                    );
                }
            }
            // Exponential backoff: jitter_ms * 2^(attempt-1), capped at 30s.
            let backoff_ms = self
                .config
                .jitter_ms
                .saturating_mul(1u64 << (attempt - 1).min(10))
                .min(30_000);
            tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            self.next_proxy(); // rotate proxy between attempts
        }
    }

    fn bump(counter: &std::sync::atomic::AtomicU64, by: u64) {
        counter.fetch_add(by, Ordering::Relaxed);
    }
}

/// Абстракция HTTP-бэкенда (reqwest fallback / wreq impersonation).
pub trait HttpFetch: Send + Sync {
    /// fetch с контекстом (заголовки/куки). Единственный обязательный метод.
    fn fetch_with(
        &self,
        url: &Url,
        ctx: &RequestContext,
    ) -> impl std::future::Future<Output = Result<Html, NetError>> + Send;
}

/// Реализация fetch по умолчанию (без дополнительных заголовков/куки).
pub async fn fetch_default(backend: &impl HttpFetch, url: &Url) -> Result<Html, NetError> {
    let ctx = RequestContext::default();
    backend.fetch_with(url, &ctx).await
}

// --- Default backend: reqwest + rustls (pure-Rust, no C toolchain) ---------

/// Session over pure-Rust `reqwest` + `rustls`.
pub type DefaultSession = StealthSession<RustlsBackend>;

#[derive(Debug)]
pub struct RustlsBackend {
    client: reqwest::Client,
}

impl RustlsBackend {
    /// Создать бэкенд с опциональным прокси.
    pub fn new(proxy: Option<&ProxyAddr>, timeout: Duration) -> Result<Self, NetError> {
        let mut b = reqwest::Client::builder().timeout(timeout);
        if let Some(p) = proxy {
            let proxy =
                reqwest::Proxy::all(p.as_ref()).map_err(|e| NetError::BadProxy(e.to_string()))?;
            b = b.proxy(proxy);
        }
        let client = b.build().map_err(|e| NetError::Request(e.to_string()))?;
        Ok(Self { client })
    }

    /// Сессия без прокси.
    pub fn direct(timeout_secs: u64) -> Result<Self, NetError> {
        Self::new(None, Duration::from_secs(timeout_secs))
    }
}

impl HttpFetch for RustlsBackend {
    async fn fetch_with(&self, url: &Url, ctx: &RequestContext) -> Result<Html, NetError> {
        debug!("GET {} (reqwest/rustls)", url);
        let mut req = self
            .client
            .get(url.as_ref())
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36");
        // Job-заголовки (могут переопределить User-Agent).
        for (k, v) in &ctx.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(cookie) = ctx.cookie_header() {
            req = req.header("Cookie", cookie);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| NetError::Request(e.to_string()))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| NetError::Request(e.to_string()))?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes).to_string();
            return Err(NetError::Status {
                status: status.as_u16(),
                body,
            });
        }
        Ok(Html::new(bytes))
    }
}

// --- Optional backend: wreq (BoringSSL TLS impersonation) ------------------

#[cfg(feature = "impersonation")]
pub mod wreq_backend;

#[cfg(feature = "impersonation")]
pub use wreq_backend::WreqBackend;

/// Convert a proxy string into a typed `ProxyAddr` (validation only).
pub fn parse_proxy(s: &str) -> Result<ProxyAddr, arachne_domain::ProxyAddrError> {
    ProxyAddr::try_from(s.to_owned())
}

#[cfg(test)]
mod tests;
