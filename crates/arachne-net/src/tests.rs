//! Tests for arachne-net.

use super::*;

#[test]
fn config_default_values() {
    let c = SessionConfig::default();
    assert_eq!(c.jitter_ms, 200);
    assert_eq!(c.max_retries, 3);
    assert!(c.proxies.is_empty());
}

#[test]
fn impersonation_default_is_chrome() {
    assert_eq!(Impersonation::default(), Impersonation::Chrome);
}

#[test]
fn next_proxy_empty_pool_is_none() {
    let backend = RustlsBackend::direct(30).unwrap();
    let s = StealthSession::new(backend, SessionConfig::default());
    assert!(s.next_proxy().is_none());
}

#[test]
fn next_proxy_round_robin() {
    let backend = RustlsBackend::direct(30).unwrap();
    let cfg = SessionConfig {
        proxies: vec![
            ProxyAddr::try_from("http://p1:8080").unwrap(),
            ProxyAddr::try_from("http://p2:8080").unwrap(),
        ],
        ..Default::default()
    };
    let s = StealthSession::new(backend, cfg);
    assert_eq!(s.next_proxy().unwrap().as_ref(), "http://p1:8080");
    assert_eq!(s.next_proxy().unwrap().as_ref(), "http://p2:8080");
    assert_eq!(s.next_proxy().unwrap().as_ref(), "http://p1:8080");
}

#[test]
fn parse_proxy_valid() {
    let p = parse_proxy("http://1.2.3.4:8080").unwrap();
    assert_eq!(p.as_ref(), "http://1.2.3.4:8080");
}

#[test]
fn parse_proxy_empty_errors() {
    assert!(matches!(
        parse_proxy(""),
        Err(arachne_domain::ProxyAddrError::Empty)
    ));
}

#[test]
fn net_error_message() {
    let e = NetError::Request("boom".into());
    assert!(e.to_string().contains("boom"));
}

#[tokio::test]
async fn session_counts_requests_and_errors() {
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Mock-бэкенд: первые 2 запроса — 429, затем успех.
    struct FlakyBackend {
        calls: AtomicU32,
    }
    impl HttpFetch for FlakyBackend {
        async fn fetch(&self, _url: &Url) -> Result<Html, NetError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < 2 {
                Err(NetError::Status {
                    status: 429,
                    body: String::new(),
                })
            } else {
                Ok(Html::new(bytes::Bytes::from_static(b"ok")))
            }
        }
    }

    let backend = FlakyBackend {
        calls: AtomicU32::new(0),
    };
    let cfg = SessionConfig {
        jitter_ms: 1,
        max_retries: 3,
        ..Default::default()
    };
    let s = StealthSession::new(backend, cfg);
    let url = Url::try_from("https://example.com/").unwrap();
    let html = s.get(&url).await.unwrap();
    assert_eq!(html.as_str().unwrap(), "ok");
    let snap = s.stats().snapshot();
    assert_eq!(snap.requests, 1);
    assert_eq!(snap.rate_limited, 2);
    assert_eq!(snap.errors, 2);
}

#[tokio::test]
async fn session_exhausts_retries() {
    struct Always429;
    impl HttpFetch for Always429 {
        async fn fetch(&self, _url: &Url) -> Result<Html, NetError> {
            Err(NetError::Status {
                status: 429,
                body: "nope".into(),
            })
        }
    }

    let cfg = SessionConfig {
        jitter_ms: 1,
        max_retries: 2,
        ..Default::default()
    };
    let s = StealthSession::new(Always429, cfg);
    let url = Url::try_from("https://example.com/").unwrap();
    let res = s.get(&url).await;
    assert!(matches!(res, Err(NetError::Status { status: 429, .. })));
    let snap = s.stats().snapshot();
    assert_eq!(snap.rate_limited, 3); // 1 initial + 2 retries
}
