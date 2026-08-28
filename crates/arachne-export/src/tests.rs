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
