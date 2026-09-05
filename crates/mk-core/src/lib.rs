//! A Macedonian spelling and grammar checker.
//!
//! The engine is deliberately free of I/O and of any platform assumption: it
//! takes a compiled lexicon as bytes and returns [`Diagnostic`]s. That is what
//! lets the same code serve a WebAssembly build inside a browser extension, a
//! native Windows binary and the test CLI without forking.
//!
//! ```no_run
//! use mk_core::{Checker, Lexicon};
//! let fst = std::fs::read("mk.fst").unwrap();
//! let checker = Checker::new(fst).unwrap();
//! for d in checker.check("Тој ја видe кnигата.") {
//!     println!("{} {}..{} {}", d.rule, d.char_start, d.char_end, d.message);
//! }
//! ```

pub mod alphabet;
pub mod diagnostic;
pub mod homoglyph;
pub mod levenshtein;
pub mod lexicon;
pub mod tokenizer;
pub mod translit;

pub use diagnostic::{rule, Diagnostic, Severity};
pub use lexicon::Lexicon;
pub use tokenizer::{tokenize, Token, TokenKind};

use alphabet::{script_of, Script};

/// How many replacements to offer for one problem.
const MAX_SUGGESTIONS: usize = 5;

/// Latin runs shorter than this are too likely to be a genuine abbreviation or
/// a stray English particle to be worth converting.
const MIN_LATIN_WORD_LEN: usize = 3;

/// Words in all capitals up to this length are treated as acronyms (МПЦ, ДДВ,
/// МВР) rather than misspellings. Precision matters more than recall here:
/// underlining every acronym would teach users to ignore the checker.
const MAX_ACRONYM_LEN: usize = 5;

/// The checker: a lexicon plus the rules that consult it.
pub struct Checker {
    lexicon: Lexicon,
}

impl Checker {
    /// Build a checker over a compiled FST lexicon.
    pub fn new(fst_bytes: Vec<u8>) -> Result<Self, fst::Error> {
        Ok(Self { lexicon: Lexicon::from_bytes(fst_bytes)? })
    }

    /// Borrow the underlying lexicon.
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// Check `text` and return every problem found, in document order.
    pub fn check(&self, text: &str) -> Vec<Diagnostic> {
        let mut out = Vec::new();

        for token in tokenize(text) {
            if token.kind != TokenKind::Word {
                continue;
            }
            // Suffixed ordinals and codes (`1-ви`, `А4`) are not lexicon entries.
            if token.text.chars().any(|c| c.is_numeric()) {
                continue;
            }
            if let Some(d) = self.check_word(token.text, token.char_start, token.char_end) {
                out.push(d);
            }
        }

        out
    }

    fn check_word(&self, word: &str, start: usize, end: usize) -> Option<Diagnostic> {
        // 1. Characters that are not Macedonian at all.
        if let Some(issue) = homoglyph::analyze(word) {
            let repaired_is_a_word = self.lexicon.contains(&issue.normalized);
            let (rule_id, message) = match issue.kind {
                homoglyph::IssueKind::Homoglyph => (
                    rule::HOMOGLYPH,
                    "Зборот меша латинични и кирилични букви што изгледаат исто.",
                ),
                homoglyph::IssueKind::ForeignCyrillic => (
                    rule::FOREIGN_CYRILLIC,
                    "Зборот содржи букви што не постојат во македонската азбука.",
                ),
            };
            return Some(Diagnostic {
                rule: rule_id.to_string(),
                // A confirmed repair is an error; an unconfirmed one is still
                // wrong at the character level, but we say so more quietly.
                severity: if repaired_is_a_word { Severity::Error } else { Severity::Warning },
                char_start: start,
                char_end: end,
                text: word.to_string(),
                message: message.to_string(),
                suggestions: if issue.normalized != word {
                    vec![issue.normalized]
                } else {
                    Vec::new()
                },
            });
        }

        // 2. Macedonian written in Latin letters.
        if is_all_latin(word) {
            if word.chars().count() < MIN_LATIN_WORD_LEN {
                return None;
            }
            let readings: Vec<String> = translit::candidates(word)
                .into_iter()
                .filter(|c| self.lexicon.contains(c))
                .take(MAX_SUGGESTIONS)
                .collect();
            // No reading is a Macedonian word, so this is probably a genuine
            // foreign word (a name, a URL fragment, an English term). Leave it.
            if readings.is_empty() {
                return None;
            }
            return Some(Diagnostic {
                rule: rule::LATIN_TEXT.to_string(),
                severity: Severity::Warning,
                char_start: start,
                char_end: end,
                text: word.to_string(),
                message: "Изгледа дека зборот е напишан со латиница.".to_string(),
                suggestions: readings,
            });
        }

        // 3. Ordinary spelling.
        if self.lexicon.contains(word) {
            return None;
        }
        if self.is_known_compound(word) {
            return None;
        }
        if is_acronym(word) {
            return None;
        }
        // A lone letter is nearly always an abbreviation (`г.` година, `ж.`
        // женски, `в.` век) or a list marker, and almost never a typo worth
        // reporting. On Macedonian Wikipedia these accounted for one in eight
        // of all spelling hits, every one of them a false positive.
        if word.chars().count() == 1 {
            return None;
        }

        Some(Diagnostic {
            rule: rule::SPELL.to_string(),
            severity: Severity::Error,
            char_start: start,
            char_end: end,
            text: word.to_string(),
            message: "Непознат збор.".to_string(),
            suggestions: self.lexicon.suggest(word, MAX_SUGGESTIONS),
        })
    }

    /// `црно-бел` will not be in the lexicon, but both halves are. Accept the
    /// compound when every hyphen-separated part is a known word.
    fn is_known_compound(&self, word: &str) -> bool {
        if !word.contains('-') {
            return false;
        }
        let parts: Vec<&str> = word.split('-').collect();
        parts.len() > 1 && parts.iter().all(|p| !p.is_empty() && self.lexicon.contains(p))
    }
}

fn is_all_latin(word: &str) -> bool {
    let mut saw_letter = false;
    for c in word.chars() {
        match script_of(c) {
            Script::Latin => saw_letter = true,
            Script::Macedonian | Script::ForeignCyrillic => return false,
            _ => {}
        }
    }
    saw_letter
}

fn is_acronym(word: &str) -> bool {
    let count = word.chars().count();
    count >= 2
        && count <= MAX_ACRONYM_LEN
        && word.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature lexicon; enough to exercise every rule.
    fn checker() -> Checker {
        let words = [
            "македонски", "книга", "книгата", "книги", "коњ", "коњот", "здраво", "жена", "човек",
            "ѓавол", "ѓубре", "скопје", "цел", "црно", "бел", "тој", "оди", "дома", "ја", "виде",
            "и", "во",
        ];
        Checker::new(Lexicon::build_from_unsorted(words).unwrap()).unwrap()
    }

    fn rules(text: &str) -> Vec<String> {
        checker().check(text).into_iter().map(|d| d.rule).collect()
    }

    #[test]
    fn clean_text_produces_nothing() {
        assert!(checker().check("Тој оди дома.").is_empty());
    }

    #[test]
    fn flags_an_unknown_word_and_suggests_a_fix() {
        let found = checker().check("книгаа");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, rule::SPELL);
        assert!(found[0].suggestions.contains(&"книга".to_string()), "{:?}", found[0].suggestions);
    }

    #[test]
    fn catches_latin_lookalikes_inside_a_cyrillic_word() {
        // 'a', 'e', 'o' are Latin here — the word looks perfect to a human.
        let found = checker().check("мaкeдoнски");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, rule::HOMOGLYPH);
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].suggestions, vec!["македонски".to_string()]);
    }

    #[test]
    fn catches_serbian_letters() {
        let found = checker().check("ђубре");
        assert_eq!(found[0].rule, rule::FOREIGN_CYRILLIC);
        assert_eq!(found[0].suggestions, vec!["ѓубре".to_string()]);
    }

    #[test]
    fn offers_cyrillic_for_latin_typed_macedonian() {
        let found = checker().check("zdravo");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, rule::LATIN_TEXT);
        assert!(found[0].suggestions.contains(&"здраво".to_string()));
    }

    #[test]
    fn ambiguous_transliteration_is_resolved_by_the_lexicon() {
        // `konj` could read коњ or конј; only коњ is a word.
        let found = checker().check("konj");
        assert_eq!(found[0].suggestions, vec!["коњ".to_string()]);
    }

    #[test]
    fn genuine_foreign_words_are_left_alone() {
        // No reading of these is a Macedonian word, so we stay quiet.
        assert!(checker().check("Wikipedia").is_empty());
        assert!(checker().check("github").is_empty());
    }

    #[test]
    fn hyphenated_compounds_of_known_words_are_accepted() {
        assert!(checker().check("црно-бел").is_empty());
    }

    #[test]
    fn acronyms_are_not_reported() {
        assert!(checker().check("МПЦ и ДДВ").is_empty());
    }

    #[test]
    fn suffixed_ordinals_are_skipped() {
        assert!(checker().check("1-ви").is_empty());
    }

    #[test]
    fn offsets_locate_the_word_in_the_original_text() {
        let text = "Тој ја видe книгата.";
        let found = checker().check(text);
        assert_eq!(found.len(), 1, "{found:?}");
        let d = &found[0];
        let sliced: String = text.chars().skip(d.char_start).take(d.char_end - d.char_start).collect();
        assert_eq!(sliced, d.text);
        assert_eq!(d.text, "видe"); // trailing 'e' is Latin
    }

    #[test]
    fn several_problems_are_reported_in_document_order() {
        let got = rules("zdravo, мaкeдoнски книгаа");
        assert_eq!(got, vec![rule::LATIN_TEXT, rule::HOMOGLYPH, rule::SPELL]);
    }
}
