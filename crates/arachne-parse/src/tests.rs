//! Tests for arachne-parse.

use super::*;
use arachne_domain::{Html, Selector};
use bytes::Bytes;

const FIXTURE: &str = r#"
<!DOCTYPE html>
<html>
<head><title>Test</title></head>
<body>
  <h1 class="title">Hello</h1>
  <p class="title">World</p>
  <a href="/page/2">next</a>
</body>
</html>
"#;

fn dom() -> Dom {
    Dom::parse(&Html::new(Bytes::from_static(FIXTURE.as_bytes()))).unwrap()
}

#[test]
fn parse_h1_single_text() {
    let d = dom();
    let sel = Selector::try_from("h1.title").unwrap();
    let txt = d.select_text(&sel).unwrap();
    assert_eq!(txt, Some("Hello".to_string()));
}

#[test]
fn select_text_all_returns_multiple() {
    let d = dom();
    let sel = Selector::try_from(".title").unwrap();
    let txts = d.select_text_all(&sel).unwrap();
    assert_eq!(txts.len(), 2);
    assert_eq!(txts[0], "Hello");
    assert_eq!(txts[1], "World");
}

#[test]
fn count_matches() {
    let d = dom();
    let sel = Selector::try_from("p").unwrap();
    assert_eq!(d.count(&sel).unwrap(), 1);
}

#[test]
fn no_match_returns_none() {
    let d = dom();
    let sel = Selector::try_from(".nonexistent").unwrap();
    assert!(d.select_text(&sel).unwrap().is_none());
}

#[test]
fn invalid_selector_errors() {
    let d = dom();
    let bad = Selector::try_from("}}}").unwrap();
    assert!(d.select_text(&bad).is_err());
}

#[test]
fn extract_pagination_next_link() {
    let d = dom();
    let sel = Selector::try_from("a").unwrap();
    let hrefs = d.select_text_all(&sel).unwrap();
    assert!(hrefs.iter().any(|t| t == "next"));
}
