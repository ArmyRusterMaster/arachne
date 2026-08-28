//! Export crawled records to CSV, JSONL, or SQLite.
//!
//! A `Record` is the typed row produced by the parser; exporters write
//! it to the chosen backend.

use std::path::Path;

use serde::Serialize;
use thiserror::Error;

/// One scraped row.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub task_id: u64,
    pub page_id: u64,
    pub url: String,
    pub field: String,
    pub value: String,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite not available (enable `sqlite` feature)")]
    SqliteNotAvailable,
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for ExportError {
    fn from(e: rusqlite::Error) -> Self {
        ExportError::Io(std::io::Error::other(e))
    }
}

/// Write records as JSONL (one JSON object per line).
pub fn to_jsonl<P: AsRef<Path>>(path: P, records: &[Record]) -> Result<(), ExportError> {
    let mut f = std::fs::File::create(path)?;
    for r in records {
        let line = serde_json::to_string(r)?;
        use std::io::Write;
        writeln!(f, "{line}")?;
    }
    Ok(())
}

/// Write records as CSV.
pub fn to_csv<P: AsRef<Path>>(path: P, records: &[Record]) -> Result<(), ExportError> {
    let mut wtr = csv::Writer::from_path(path)?;
    for r in records {
        wtr.serialize(r)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Write records to a SQLite table `records` (feature `sqlite`).
#[cfg(feature = "sqlite")]
pub fn to_sqlite<P: AsRef<Path>>(path: P, records: &[Record]) -> Result<(), ExportError> {
    let conn = rusqlite::Connection::open(path.as_ref())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS records (
            task_id  INTEGER,
            page_id  INTEGER,
            url      TEXT,
            field    TEXT,
            value    TEXT
        );",
    )?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO records (task_id, page_id, url, field, value) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for r in records {
        stmt.execute(rusqlite::params![r.task_id, r.page_id, r.url, r.field, r.value])?;
    }
    Ok(())
}

/// Без feature `sqlite` — ошибка (фаза A, Windows без C-тулчейна).
#[cfg(not(feature = "sqlite"))]
pub fn to_sqlite<P: AsRef<Path>>(_path: P, _records: &[Record]) -> Result<(), ExportError> {
    Err(ExportError::SqliteNotAvailable)
}

#[cfg(test)]
mod tests;
