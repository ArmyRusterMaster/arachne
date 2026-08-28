//! DOM parsing + CSS selection layer for Arachne.
//!
//! Wraps `scraper`/`html5ever` into a tight, typed API that consumes
//! `arachne_domain::Html` (zero-copy `Bytes`).

use scraper::{Html as ScraperHtml, Selector as ScraperSelector};

use arachne_domain::{Html, Selector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Ошибки парсинга / выборки.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid CSS selector: {0}")]
    Selector(String),
    #[error("HTML parse error (sourceless)")]
    Html,
}

/// Запись из вложенного поиска: один блок (index) + поле + значение.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestedRecord {
    pub index: u64,
    pub field: String,
    pub value: String,
}

/// A parsed DOM tree.
#[derive(Debug, Clone)]
pub struct Dom {
    inner: ScraperHtml,
}

impl Dom {
    /// Parse an HTML document from domain `Html`.
    pub fn parse(html: &Html) -> Result<Self, ParseError> {
        let text = html.as_str().map_err(|_| ParseError::Html)?;
        let inner = ScraperHtml::parse_document(text);
        Ok(Self { inner })
    }

    /// Return the text content of the first element matching `sel`,
    /// or `None` if no match.
    pub fn select_text(&self, sel: &Selector) -> Result<Option<String>, ParseError> {
        let css = ScraperSelector::parse(sel.as_ref())
            .map_err(|e| ParseError::Selector(e.to_string()))?;
        Ok(self
            .inner
            .select(&css)
            .next()
            .map(|el| el.text().collect::<String>()))
    }

    /// Return all text matches for `sel` (in document order).
    pub fn select_text_all(&self, sel: &Selector) -> Result<Vec<String>, ParseError> {
        let css = ScraperSelector::parse(sel.as_ref())
            .map_err(|e| ParseError::Selector(e.to_string()))?;
        Ok(self
            .inner
            .select(&css)
            .map(|el| el.text().collect::<String>())
            .collect())
    }

    /// Вложенный поиск: для каждого элемента, найденного по `repeat_selector`,
    /// извлекает текст по каждому полю в `NestedSelector`.
    ///
    /// Возвращает вектор записей с `index` (номер блока, начиная с 0),
    /// `field`, `value`.
    ///
    /// ```yaml
    /// repeat_selector: ".quote"
    /// fields:
    ///   - { name: "text", selector: ".text" }
    ///   - { name: "author", selector: ".author" }
    /// ```
    pub fn select_all_nested(
        &self,
        nested: &arachne_domain::NestedSelector,
    ) -> Result<Vec<NestedRecord>, ParseError> {
        let repeat = ScraperSelector::parse(&nested.repeat_selector)
            .map_err(|e| ParseError::Selector(e.to_string()))?;

        let mut out = Vec::new();
        for (idx, block) in self.inner.select(&repeat).enumerate() {
            for field in &nested.fields {
                let css = ScraperSelector::parse(&field.selector)
                    .map_err(|e| ParseError::Selector(e.to_string()))?;
                // Ищем селектор внутри текущего блока (scraper поддерживает
                // поиск по отношению к элементу через select).
                let value = block
                    .select(&css)
                    .next()
                    .map(|el| el.text().collect::<String>())
                    .unwrap_or_default();
                out.push(NestedRecord {
                    index: idx as u64,
                    field: field.name.clone(),
                    value: value.trim().to_string(),
                });
            }
        }
        Ok(out)
    }

    /// Count elements matching `sel`.
    pub fn count(&self, sel: &Selector) -> Result<usize, ParseError> {
        let css = ScraperSelector::parse(sel.as_ref())
            .map_err(|e| ParseError::Selector(e.to_string()))?;
        Ok(self.inner.select(&css).count())
    }
}

#[cfg(test)]
mod tests;
