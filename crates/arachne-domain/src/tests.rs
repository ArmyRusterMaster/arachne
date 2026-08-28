//! Tests for arachne-domain (TDD invariant: code + tests same commit).

use super::types::*;
use bytes::Bytes;

#[test]
fn url_valid_https() {
    let u = Url::try_from("https://example.com/path?q=1").unwrap();
    assert_eq!(u.as_ref(), "https://example.com/path?q=1");
    assert_eq!(u.path_and_query(), "/path?q=1");
}

#[test]
fn url_rejects_non_http_scheme() {
    let err = Url::try_from("ftp://example.com").unwrap_err();
    assert!(matches!(err, UrlError::UnsupportedScheme(_)));
    assert!(err.to_string().contains("unsupported URL scheme"));
}

#[test]
fn url_rejects_garbage() {
    assert!(Url::try_from("not a url").is_err());
}

#[test]
fn url_from_str_roundtrip() {
    let u: Url = "https://x.io/a".parse().unwrap();
    assert_eq!(u.to_string(), "https://x.io/a");
}

#[test]
fn proxy_addr_valid() {
    let p = ProxyAddr::try_from("http://user:pass@1.2.3.4:8080").unwrap();
    assert_eq!(p.as_ref(), "http://user:pass@1.2.3.4:8080");
}

#[test]
fn proxy_addr_rejects_empty() {
    assert!(matches!(
        ProxyAddr::try_from(""),
        Err(ProxyAddrError::Empty)
    ));
}

#[test]
fn task_id_newtype() {
    let t = TaskId::new(42);
    assert_eq!(t.get(), 42);
    assert_eq!(TaskId::from(42u64).get(), 42);
}

#[test]
fn session_id_unique() {
    let a = SessionId::new();
    let b = SessionId::new();
    assert_ne!(a, b);
}

#[test]
fn page_id_from_u64() {
    let p = PageId::from(7u64);
    assert_eq!(p.get(), 7);
}

#[test]
fn millis_from_secs() {
    assert_eq!(Millis::from_secs(1.5).get(), 1500);
    assert_eq!(Millis::new(500).as_secs(), 0.5);
}

#[test]
fn millis_to_duration() {
    let d: std::time::Duration = Millis::new(1234).into();
    assert_eq!(d.as_millis(), 1234);
}

#[test]
fn ram_limit_bytes() {
    let r = RamLimitBytes::new(1_048_576);
    assert_eq!(r.get(), 1_048_576);
}

#[test]
fn html_zero_copy() {
    let b = Bytes::from_static(b"<html></html>");
    let h = Html::new(b.clone());
    assert_eq!(h.as_str().unwrap(), "<html></html>");
    assert!(!h.is_empty());
}

#[test]
fn selector_valid() {
    let s = Selector::try_from(".title").unwrap();
    assert_eq!(s.as_ref(), ".title");
}

#[test]
fn selector_rejects_whitespace() {
    assert!(matches!(
        Selector::try_from("   "),
        Err(SelectorError::Empty)
    ));
}
