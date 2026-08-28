//! DOM parsing + CSS selection layer for Arachne.
//!
//! Wraps `scraper`/`html5ever` into a tight, typed API that consumes
//! `arachne_domain::Html` (zero-copy `Bytes`).

use scraper::{Html as ScraperHtml, Selector as ScraperSelector};

use arachne_domain::{Html, Selector};
use thiserror::Error;

/// Errors from parsing / selection.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid CSS selector: {0}")]
    Selector(String),
    #[error("HTML parse error (sourceless)")]
    Html,
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

    /// Count elements matching `sel`.
    pub fn count(&self, sel: &Selector) -> Result<usize, ParseError> {
        let css = ScraperSelector::parse(sel.as_ref())
            .map_err(|e| ParseError::Selector(e.to_string()))?;
        Ok(self.inner.select(&css).count())
    }
}

#[cfg(test)]
mod tests;
