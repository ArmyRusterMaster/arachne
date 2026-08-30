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

#[test]
fn extract_links_extracts_href() {
    let html = Html::new(Bytes::from_static(
        r#"<html><body>
            <a href="/page/2">next</a>
            <a href="https://ext.io/x">ext</a>
            <a>no href</a>
        </body></html>"#
            .as_bytes(),
    ));
    let d = Dom::parse(&html).unwrap();
    let sel = Selector::try_from("a").unwrap();
    let links = d.extract_links(&sel).unwrap();
    assert_eq!(links.len(), 2); // якорь без href пропущен
    assert_eq!(links[0], "/page/2");
    assert_eq!(links[1], "https://ext.io/x");
}

#[test]
fn nested_selector_extracts_repeated_blocks() {
    let html = Html::new(Bytes::from_static(
        r#"<html><body>
            <div class="item"><span class="name">Alice</span><span class="age">30</span></div>
            <div class="item"><span class="name">Bob</span><span class="age">25</span></div>
        </body></html>"#
            .as_bytes(),
    ));
    let d = Dom::parse(&html).unwrap();
    let nested = arachne_domain::NestedSelector {
        repeat_selector: ".item".to_string(),
        fields: vec![
            arachne_domain::NestedField {
                name: "name".to_string(),
                selector: ".name".to_string(),
            },
            arachne_domain::NestedField {
                name: "age".to_string(),
                selector: ".age".to_string(),
            },
        ],
        nested: vec![],
    };
    let records = d.select_all_nested(&nested).unwrap();
    assert_eq!(records.len(), 4); // 2 блока × 2 поля

    assert_eq!(records[0].index, 0);
    assert_eq!(records[0].field, "name[0]");
    assert_eq!(records[0].value, "Alice");

    assert_eq!(records[1].index, 0);
    assert_eq!(records[1].field, "age[0]");
    assert_eq!(records[1].value, "30");

    assert_eq!(records[2].index, 1);
    assert_eq!(records[2].field, "name[1]");
    assert_eq!(records[2].value, "Bob");

    assert_eq!(records[3].index, 1);
    assert_eq!(records[3].field, "age[1]");
    assert_eq!(records[3].value, "25");
}

#[test]
fn nested_selector_recurses_two_levels() {
    // Структура как на quotes.toscrape.com: .quote → .tags .tag
    let html = Html::new(Bytes::from_static(
        r#"<html><body>
            <div class="quote">
                <span class="text">Quote one</span>
                <div class="tags"><a class="tag">life</a><a class="tag">love</a></div>
            </div>
            <div class="quote">
                <span class="text">Quote two</span>
                <div class="tags"><a class="tag">books</a></div>
            </div>
        </body></html>"#
            .as_bytes(),
    ));
    let d = Dom::parse(&html).unwrap();
    let root = arachne_domain::NestedSelector {
        repeat_selector: ".quote".to_string(),
        fields: vec![arachne_domain::NestedField {
            name: "quote_text".to_string(),
            selector: ".text".to_string(),
        }],
        nested: vec![arachne_domain::NestedSelector {
            repeat_selector: ".tags .tag".to_string(),
            fields: vec![arachne_domain::NestedField {
                name: "tag_name".to_string(),
                selector: ".".to_string(), // текст самого блока
            }],
            nested: vec![],
        }],
    };
    let records = d.select_all_nested(&root).unwrap();
    // Уровень 1: 2 цитаты × 1 поле; уровень 2: 2 + 1 тегов × 1 поле = 5 записей
    assert_eq!(records.len(), 5);

    // Порядок: блок 0 → его поля → его вложенные; затем блок 1.
    assert_eq!(records[0].field, "quote_text[0]");
    assert_eq!(records[0].value, "Quote one");

    // Вложенные: путь индексов родитель_блок.вложенный_блок
    assert_eq!(records[1].field, "tag_name[0.0]");
    assert_eq!(records[1].value, "life");
    assert_eq!(records[2].field, "tag_name[0.1]");
    assert_eq!(records[2].value, "love");

    assert_eq!(records[3].field, "quote_text[1]");
    assert_eq!(records[3].value, "Quote two");
    assert_eq!(records[4].field, "tag_name[1.0]");
    assert_eq!(records[4].value, "books");
    // index вложенной записи — индекс внутри родительского блока
    assert_eq!(records[4].index, 0);
}
