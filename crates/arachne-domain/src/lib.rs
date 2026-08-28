//! Domain types for Arachne — all primitives wrapped in Newtypes.
//!
//! See `rules.md` §2: never pass bare primitives between modules.

#[cfg(test)]
mod tests;

pub mod types;

pub use types::*;
