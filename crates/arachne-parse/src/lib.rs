//! DOM parsing + CSS selection layer for Arachne.
//!
//! Wraps `scraper`/`html5ever` into a tight, typed API that consumes
//! `arachne_domain::Html` (zero-copy `Bytes`).

use scraper::{ElementRef, Html as ScraperHtml, Selector as ScraperSelector};

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
///
/// `field` содержит имя поля с путём индексов блоков: `quote_text[0]`
/// на корневом уровне, `tag_name[0.2]` — на вложенном (блок 0 → его
/// вложенный блок 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestedRecord {
    pub index: u64,
    /// Имя поля с путём индексов: `name[i]`, `name[i.j]`, ...
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

    /// Вложенный поиск с произвольной вложенностью (рекурсивный).
    ///
    /// Имя поля в результате — `{name}[{путь индексов}]`: `quote_text[0]`,
    /// `tag_name[0.2]` и т.д. Спец-селектор `"."` означает «текст самого
    /// блока» (для листовых элементов вроде `a.tag`).
    pub fn select_all_nested(
        &self,
        nested: &arachne_domain::NestedSelector,
    ) -> Result<Vec<NestedRecord>, ParseError> {
        let repeat = ScraperSelector::parse(&nested.repeat_selector)
            .map_err(|e| ParseError::Selector(e.to_string()))?;
        let mut out = Vec::new();
        self.collect_nested(
            &self.inner.root_element(),
            nested,
            &repeat,
            String::new(),
            &mut out,
        )?;
        Ok(out)
    }

    /// Рекурсивный обход блоков `ns` внутри `parent`.
    fn collect_nested(
        &self,
        parent: &ElementRef,
        ns: &arachne_domain::NestedSelector,
        repeat: &ScraperSelector,
        index_path: String,
        out: &mut Vec<NestedRecord>,
    ) -> Result<(), ParseError> {
        // Селекторы полей и вложенных уровней парсим один раз на уровень,
        // а не для каждого блока. Спец-селектор "." (текст самого блока)
        // не парсится как CSS — помечается через None.
        let fields = ns
            .fields
            .iter()
            .map(|f| {
                if f.selector == "." {
                    Ok((None, f.name.as_str()))
                } else {
                    ScraperSelector::parse(&f.selector)
                        .map(|css| (Some(css), f.name.as_str()))
                        .map_err(|e| ParseError::Selector(e.to_string()))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let children = ns
            .nested
            .iter()
            .map(|c| {
                ScraperSelector::parse(&c.repeat_selector)
                    .map(|css| (css, c))
                    .map_err(|e| ParseError::Selector(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        for (idx, block) in parent.select(repeat).enumerate() {
            let idx = idx as u64;
            let block_path = if index_path.is_empty() {
                format!("{idx}")
            } else {
                format!("{index_path}.{idx}")
            };

            // Поля на этом уровне: field = "{name}[{block_path}]".
            for (css, name) in &fields {
                let value = match css {
                    // Спец-селектор: текст самого блока.
                    None => block.text().collect::<String>(),
                    Some(css) => block
                        .select(css)
                        .next()
                        .map(|el| el.text().collect::<String>())
                        .unwrap_or_default(),
                };
                out.push(NestedRecord {
                    index: idx,
                    field: format!("{name}[{block_path}]"),
                    value: value.trim().to_string(),
                });
            }

            // Спускаемся в вложенные repeat-селекторы внутри блока.
            for (child_repeat, child_ns) in &children {
                self.collect_nested(&block, child_ns, child_repeat, block_path.clone(), out)?;
            }
        }
        Ok(())
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
