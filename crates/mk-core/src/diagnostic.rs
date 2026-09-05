//! What the checker reports back.

use serde::Serialize;

/// How much the reader should care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Almost certainly wrong.
    Error,
    /// Suspicious, worth a look.
    Warning,
    /// Informational; a possible improvement.
    Info,
}

/// Stable rule identifiers. These end up in the UI and in test fixtures, so
/// they are part of the public contract and must not be renamed casually.
pub mod rule {
    /// Word is absent from the lexicon.
    pub const SPELL: &str = "MK_SPELL";
    /// Word mixes Latin and Cyrillic characters that look alike.
    pub const HOMOGLYPH: &str = "MK_HOMOGLYPH";
    /// Word contains Cyrillic letters outside the Macedonian alphabet.
    pub const FOREIGN_CYRILLIC: &str = "MK_FOREIGN_CYRILLIC";
    /// Word appears to be Macedonian typed in the Latin alphabet.
    pub const LATIN_TEXT: &str = "MK_LATIN_TEXT";
}

/// One problem found in the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    /// Stable rule id, e.g. `MK_HOMOGLYPH`.
    pub rule: String,
    pub severity: Severity,
    /// Character offset of the first character of the span.
    pub char_start: usize,
    /// Character offset one past the last character of the span.
    pub char_end: usize,
    /// The offending text, so callers can render without re-slicing.
    pub text: String,
    /// Human-readable explanation, in Macedonian.
    pub message: String,
    /// Replacements, best first.
    pub suggestions: Vec<String>,
}
