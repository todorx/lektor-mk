//! Macedonian-aware tokenizer.
//!
//! Splits text into words, numbers and punctuation while keeping the
//! apostrophe and hyphen attached when they sit *between* letters, so that
//! `к'смет`, `црно-бел`, `д-р` and `1-ви` survive as single tokens.

use crate::alphabet::is_word_internal;

/// What kind of thing a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Contains at least one letter.
    Word,
    /// Digits only (possibly with internal separators).
    Number,
    /// Everything else that is not whitespace.
    Punct,
}

/// A single token, carrying both byte and character offsets.
///
/// Byte offsets index back into the original `&str`; character offsets are what
/// the JavaScript side needs to position a highlight. Macedonian Cyrillic lives
/// entirely in the Basic Multilingual Plane, so character offsets and JavaScript
/// UTF-16 offsets coincide for all-Macedonian text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    pub text: &'a str,
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub char_end: usize,
    pub kind: TokenKind,
}

/// Tokenize `text`, skipping whitespace.
pub fn tokenize(text: &str) -> Vec<Token<'_>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let (byte_start, c) = chars[i];
        let char_start = i;

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_alphabetic() || c.is_numeric() {
            let mut j = i;
            let mut has_letter = c.is_alphabetic();

            while j < chars.len() {
                let (_, cj) = chars[j];
                if cj.is_alphabetic() || cj.is_numeric() {
                    has_letter |= cj.is_alphabetic();
                    j += 1;
                } else if is_word_internal(cj)
                    && chars
                        .get(j + 1)
                        .is_some_and(|(_, next)| next.is_alphabetic() || next.is_numeric())
                {
                    // Keep the separator only when a letter or digit follows it.
                    j += 1;
                } else {
                    break;
                }
            }

            let byte_end = chars.get(j).map_or(text.len(), |(b, _)| *b);
            tokens.push(Token {
                text: &text[byte_start..byte_end],
                byte_start,
                byte_end,
                char_start,
                char_end: j,
                kind: if has_letter { TokenKind::Word } else { TokenKind::Number },
            });
            i = j;
        } else {
            // A word may legitimately *start* with an apostrophe: 'рж, 'рбет.
            if is_word_internal(c)
                && chars.get(i + 1).is_some_and(|(_, next)| next.is_alphabetic())
            {
                let mut j = i + 1;
                while j < chars.len() {
                    let (_, cj) = chars[j];
                    if cj.is_alphabetic() || cj.is_numeric() {
                        j += 1;
                    } else if is_word_internal(cj)
                        && chars.get(j + 1).is_some_and(|(_, n)| n.is_alphabetic())
                    {
                        j += 1;
                    } else {
                        break;
                    }
                }
                let byte_end = chars.get(j).map_or(text.len(), |(b, _)| *b);
                tokens.push(Token {
                    text: &text[byte_start..byte_end],
                    byte_start,
                    byte_end,
                    char_start,
                    char_end: j,
                    kind: TokenKind::Word,
                });
                i = j;
                continue;
            }

            let byte_end = chars.get(i + 1).map_or(text.len(), |(b, _)| *b);
            tokens.push(Token {
                text: &text[byte_start..byte_end],
                byte_start,
                byte_end,
                char_start,
                char_end: i + 1,
                kind: TokenKind::Punct,
            });
            i += 1;
        }
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        tokenize(text)
            .into_iter()
            .filter(|t| t.kind == TokenKind::Word)
            .map(|t| t.text)
            .collect()
    }

    #[test]
    fn splits_a_plain_sentence() {
        assert_eq!(words("Тој оди дома."), vec!["Тој", "оди", "дома"]);
    }

    #[test]
    fn keeps_apostrophe_inside_words() {
        // The apostrophe spells the syllabic schwa; it is not punctuation here.
        assert_eq!(words("к'смет"), vec!["к'смет"]);
        assert_eq!(words("с'нце и к'смет"), vec!["с'нце", "и", "к'смет"]);
    }

    #[test]
    fn keeps_word_initial_apostrophe() {
        assert_eq!(words("'рж и 'рбет"), vec!["'рж", "и", "'рбет"]);
    }

    #[test]
    fn keeps_hyphenated_compounds_and_abbreviations() {
        assert_eq!(words("црно-бел"), vec!["црно-бел"]);
        assert_eq!(words("д-р Петар"), vec!["д-р", "Петар"]);
        assert_eq!(words("1-ви мај"), vec!["1-ви", "мај"]);
    }

    #[test]
    fn trailing_punctuation_is_not_glued_to_the_word() {
        let toks = tokenize("здраво!");
        assert_eq!(toks[0].text, "здраво");
        assert_eq!(toks[0].kind, TokenKind::Word);
        assert_eq!(toks[1].text, "!");
        assert_eq!(toks[1].kind, TokenKind::Punct);
    }

    #[test]
    fn dash_between_words_is_punctuation_not_a_compound() {
        // A spaced dash is a separator, so the words stay apart.
        assert_eq!(words("сакам — не сакам"), vec!["сакам", "не", "сакам"]);
    }

    #[test]
    fn offsets_point_back_at_the_source() {
        let text = "Тој оди";
        for t in tokenize(text) {
            assert_eq!(&text[t.byte_start..t.byte_end], t.text);
            assert_eq!(t.text.chars().count(), t.char_end - t.char_start);
        }
    }

    #[test]
    fn numbers_are_tagged_separately() {
        let toks = tokenize("во 2026 година");
        assert_eq!(toks[1].kind, TokenKind::Number);
        assert_eq!(toks[1].text, "2026");
    }
}
