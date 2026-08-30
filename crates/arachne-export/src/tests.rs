//! Tests for arachne-export.

use super::*;
use tempfile::NamedTempFile;

fn sample() -> Vec<Record> {
    vec![Record {
        task_id: 1,
        page_id: 10,
        url: "https://x.io".into(),
        field: "title".into(),
        value: "Hello".into(),
    }]
}

#[test]
fn jsonl_roundtrip() {
    let tmp = NamedTempFile::new().unwrap();
    to_jsonl(tmp.path(), &sample()).unwrap();
    let content = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(content.contains("title"));
    assert_eq!(content.lines().count(), 1);
}

#[test]
fn csv_roundtrip() {
    let tmp = NamedTempFile::new().unwrap();
    to_csv(tmp.path(), &sample()).unwrap();
    let content = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(content.contains("task_id"));
    assert!(content.contains("Hello"));
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_roundtrip() {
    let tmp = NamedTempFile::new().unwrap();
    to_sqlite(tmp.path(), &sample()).unwrap();
    let conn = rusqlite::Connection::open(tmp.path().to_str().unwrap()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_multiple_rows() {
    let tmp = NamedTempFile::new().unwrap();
    let mut records = sample();
    records.push(Record {
        task_id: 1,
        page_id: 11,
        url: "https://y.io".into(),
        field: "title".into(),
        value: "World".into(),
    });
    to_sqlite(tmp.path(), &records).unwrap();
    let conn = rusqlite::Connection::open(tmp.path().to_str().unwrap()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn sqlite_not_available_without_feature() {
    let tmp = NamedTempFile::new().unwrap();
    let res = to_sqlite(tmp.path(), &sample());
    #[cfg(not(feature = "sqlite"))]
    assert!(matches!(res, Err(ExportError::SqliteNotAvailable)));
    #[cfg(feature = "sqlite")]
    assert!(res.is_ok());
}

#[test]
fn export_error_display() {
    let e = ExportError::Io(std::io::Error::other("bad"));
    assert!(e.to_string().contains("IO error"));
}

#[test]
fn template_each_with_flat_and_nested() {
    use serde_json::json;
    fn rec(t: u64, p: u64, f: &str, v: &str) -> Record {
        Record {
            task_id: t,
            page_id: p,
            url: "https://x.io".into(),
            field: f.into(),
            value: v.into(),
        }
    }
    let records = vec![
        rec(1, 1, "title", "Title"),
        rec(1, 1, "quote_text[0]", "Q1"),
        rec(1, 1, "quote_author[0]", "A1"),
        rec(1, 1, "tag_name[0.0]", "t1"),
        rec(1, 1, "tag_name[0.1]", "t2"),
        rec(1, 1, "quote_text[1]", "Q2"),
        rec(1, 1, "quote_author[1]", "A2"),
    ];
    let tpl = json!({
        "title": "{{title}}",
        "quotes": {
            "__each__": "quote_text",
            "text": "{{quote_text}}",
            "author": "{{quote_author}}",
            "tags": "{{tag_name}}"
        }
    });
    let idx = template::group_fields(&records);
    let nested = template::group_nested(&records);
    let out = template::render(&tpl, &idx, &nested).unwrap();
    assert_eq!(out["title"].as_str(), Some("Title"));
    let quotes = out["quotes"].as_array().unwrap();
    assert_eq!(quotes.len(), 2);
    assert_eq!(quotes[0]["text"], "Q1");
    assert_eq!(quotes[0]["author"], "A1");
    assert_eq!(quotes[0]["tags"], json!(["t1", "t2"]));
    assert_eq!(quotes[1]["text"], "Q2");
    assert_eq!(quotes[1]["author"], "A2");
    assert_eq!(quotes[1]["tags"], json!([]));
}
