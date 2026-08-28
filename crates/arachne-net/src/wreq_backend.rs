//! `wreq`-бэкенд с TLS-имперсонацией (feature `impersonation`).
//!
//! BoringSSL даёт полноценные JA4/HTTP2-отпечатки реальных браузеров.
//! Профили: `wreq_util::Profile::{Chrome133, Firefox133, Safari18_5}`.

use std::time::Duration;

use tracing::debug;

use arachne_domain::{Html, Url};

use crate::{HttpFetch, Impersonation, NetError};

/// Бэкенд на `wreq` с браузерным TLS-отпечатком.
#[derive(Debug)]
pub struct WreqBackend {
    client: wreq::Client,
}

impl WreqBackend {
    /// Создать бэкенд с профилем браузера и опциональным прокси.
    pub fn new(
        profile: Impersonation,
        proxy: Option<&arachne_domain::ProxyAddr>,
        timeout: Duration,
    ) -> Result<Self, NetError> {
        let emulation = profile.into_emulation();
        let mut b = wreq::Client::builder().emulation(emulation).timeout(timeout);
        if let Some(p) = proxy {
            let proxy = wreq::Proxy::all(p.as_ref())
                .map_err(|e| NetError::BadProxy(e.to_string()))?;
            b = b.proxy(proxy);
        }
        let client = b.build().map_err(|e| NetError::Request(e.to_string()))?;
        Ok(Self { client })
    }

    /// Сессия без прокси.
    pub fn direct(profile: Impersonation, timeout_secs: u64) -> Result<Self, NetError> {
        Self::new(profile, None, Duration::from_secs(timeout_secs))
    }
}

impl Impersonation {
    /// Маппинг на профиль `wreq-util` (реализует `IntoEmulation`).
    fn into_emulation(
        self,
    ) -> wreq::Emulation {
        use wreq_util::emulate::Profile;
        match self {
            Impersonation::Chrome => Profile::Chrome133.into_emulation(),
            Impersonation::Firefox => Profile::Firefox133.into_emulation(),
            Impersonation::Safari => Profile::Safari18_5.into_emulation(),
        }
    }
}

impl HttpFetch for WreqBackend {
    async fn fetch(&self, url: &Url) -> Result<Html, NetError> {
        debug!("GET {} (wreq impersonation)", url);
        let resp = self
            .client
            .get(url.as_ref())
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
        Ok(Html::new(bytes::Bytes::from(bytes)))
    }
}