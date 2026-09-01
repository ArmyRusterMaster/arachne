//! Персистентная очередь краулинга (веха M2): чекпоинт на диск.
//!
//! Состояние (очередь `CrawlItem` + посещённые URL) сериализуется в JSON.
//! При повторном запуске воркер resume'ится с диска вместо полного пересмотра.
//! Позволяет пережить `kill -9` без потери прогресса.

use serde::{Deserialize, Serialize};

/// Элемент очереди краулинга: URL-шаблон + глубина + переменные циклов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlItem {
    pub url: String,
    pub depth: u32,
    /// Переменные циклов/while (для подстановки в URL/заголовки/куки).
    pub vars: Vec<(String, String)>,
}

/// Состояние краулинга для чекпоинта.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CheckpointState {
    /// Очередь (не обработанные URL) в порядке BFS.
    pub queue: Vec<CrawlItem>,
    /// Все посещённые URL (для дедупликации при resume).
    pub visited: Vec<String>,
    /// Текущий счётчик страниц (чтобы page_id не сбрасывался).
    pub page_count: u64,
}

/// Создать чекпоинт (перезапись атомарна: write → rename).
pub fn save(path: &std::path::Path, state: &CheckpointState) -> std::io::Result<()> {
    // Атомарная запись: сначала во временный файл, затем rename.
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string(state).expect("state serializes");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Загрузить чекпоинт, если он существует.
pub fn load(path: &std::path::Path) -> std::io::Result<Option<CheckpointState>> {
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(path)?;
    let state = serde_json::from_str(&json).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("bad checkpoint: {e}"))
    })?;
    Ok(Some(state))
}