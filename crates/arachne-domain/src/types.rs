//! Domain types for Arachne — all primitives wrapped in Newtypes.
//!
//! See `rules.md` §2: never pass bare primitives between modules.
//! Fields are private; construct via `TryFrom`/builders and read via
//! narrow accessors / `Display` / `AsRef<str>`.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use url::Url as RawUrl;
use uuid::Uuid;
mod url_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use url::Url as RawUrl;

    pub fn serialize<S: Serializer>(u: &RawUrl, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(u.as_str())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<RawUrl, D::Error> {
        let s = String::deserialize(d)?;
        RawUrl::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A validated absolute HTTP(S) URL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Url(#[serde(with = "url_serde")] RawUrl);

/// Error returned when a `Url` cannot be constructed.
#[derive(Debug, Error)]
pub enum UrlError {
    #[error("invalid URL: {0}")]
    Invalid(String, #[source] url::ParseError),
    #[error("unsupported URL scheme: {0} (only http/https are allowed)")]
    UnsupportedScheme(String),
}

impl TryFrom<String> for Url {
    type Error = UrlError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let raw = RawUrl::parse(&s).map_err(|e| UrlError::Invalid(s, e))?;
        if !matches!(raw.scheme(), "http" | "https") {
            return Err(UrlError::UnsupportedScheme(raw.to_string()));
        }
        Ok(Self(raw))
    }
}

impl TryFrom<&str> for Url {
    type Error = UrlError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl FromStr for Url {
    type Err = UrlError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Url {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl Url {
    /// The path+query portion, without the scheme/host.
    pub fn path_and_query(&self) -> String {
        let mut s = self.0.path().to_owned();
        if let Some(q) = self.0.query() {
            s.push('?');
            s.push_str(q);
        }
        s
    }
}

/// A proxy server address (host:port, optionally with user:pass@).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProxyAddr(String);

impl TryFrom<String> for ProxyAddr {
    type Error = ProxyAddrError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.is_empty() {
            return Err(ProxyAddrError::Empty);
        }
        Ok(Self(s))
    }
}

impl TryFrom<&str> for ProxyAddr {
    type Error = ProxyAddrError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl std::fmt::Display for ProxyAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProxyAddr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Error)]
pub enum ProxyAddrError {
    #[error("proxy address must not be empty")]
    Empty,
}

/// Monotonically increasing 64-bit task identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(u64);

impl TaskId {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for TaskId {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

/// Session identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Page identifier in an output set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(u64);

impl PageId {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl From<u64> for PageId {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

/// Milliseconds as a duration quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Millis(u64);

impl Millis {
    pub const fn new(v: u64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> u64 {
        self.0
    }
    pub fn from_secs(s: f64) -> Self {
        Self((s * 1000.0).round() as u64)
    }
    pub fn as_secs(self) -> f64 {
        self.0 as f64 / 1000.0
    }
}

impl From<Millis> for std::time::Duration {
    fn from(m: Millis) -> Self {
        std::time::Duration::from_millis(m.0)
    }
}

/// Seconds as a fractional duration quantity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Seconds(f64);

impl Seconds {
    pub const fn new(v: f64) -> Self {
        Self(v)
    }
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl From<Seconds> for std::time::Duration {
    fn from(s: Seconds) -> Self {
        std::time::Duration::from_secs_f64(s.0.max(0.0))
    }
}

/// RAM limit in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RamLimitBytes(usize);

impl RamLimitBytes {
    pub const fn new(v: usize) -> Self {
        Self(v)
    }
    pub const fn get(self) -> usize {
        self.0
    }
}

impl From<usize> for RamLimitBytes {
    fn from(v: usize) -> Self {
        Self(v)
    }
}

/// HTML document body (zero-copy via `Bytes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Html(Bytes);

impl Html {
    pub fn new(b: Bytes) -> Self {
        Self(b)
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AsRef<[u8]> for Html {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// A CSS selector string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Selector(String);

impl TryFrom<String> for Selector {
    type Error = SelectorError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if s.trim().is_empty() {
            return Err(SelectorError::Empty);
        }
        Ok(Self(s))
    }
}

impl TryFrom<&str> for Selector {
    type Error = SelectorError;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_owned())
    }
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Selector {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Error)]
pub enum SelectorError {
    #[error("selector must not be empty")]
    Empty,
}

/// Одно поле внутри вложенного селектора (docs/03-job-yaml.md §4).
///
/// Пример: внутри блока `.quote` извлечь текст по селектору `.text`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestedField {
    /// Имя поля (будет использовано в `Record.field`).
    pub name: String,
    /// CSS-селектор внутри родительского блока.
    pub selector: String,
}

/// Вложенный селектор: повторяется по `repeat_selector`, а внутри
/// каждого совпадения извлекаются `fields` по их CSS-селекторам.
///
/// Пример для quotes.toscrape.com:
/// ```yaml
/// - repeat_selector: ".quote"
///   fields:
///     - { name: "quote_text", selector: ".text" }
///     - { name: "quote_author", selector: ".author" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NestedSelector {
    /// CSS-селектор повторяющихся блоков.
    pub repeat_selector: String,
    /// Поля для извлечения внутри каждого блока.
    pub fields: Vec<NestedField>,
}
