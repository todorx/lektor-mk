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
pub mod frequency;
pub mod grammar;
pub mod homoglyph;
pub mod levenshtein;
pub mod lexicon;
pub mod morphology;
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
///
/// Spelling needs only the lexicon. Grammar additionally needs morphology, so
/// it is optional — a caller that only wants spell-checking pays neither the
/// download nor the memory for the analyses.
pub struct Checker {
    lexicon: Lexicon,
    morphology: Option<morphology::Morphology>,
    frequency: Option<frequency::Frequency>,
}

impl Checker {
    /// Build a checker over a compiled FST lexicon. Spelling rules only.
    pub fn new(fst_bytes: Vec<u8>) -> Result<Self, fst::Error> {
        Ok(Self {
            lexicon: Lexicon::from_bytes(fst_bytes)?,
            morphology: None,
            frequency: None,
        })
    }

    /// Enable the grammar rules by supplying a morphology table.
    pub fn with_morphology(mut self, morphology: morphology::Morphology) -> Self {
        self.morphology = Some(morphology);
        self
    }

    /// Enable frequency-ranked suggestions. Missing table = old ranking.
    pub fn with_frequency(mut self, frequency: frequency::Frequency) -> Self {
        self.frequency = Some(frequency);
        self
    }

    /// Attach a frequency table after construction (for WASM-style setup
    /// where blobs arrive one at a time). Overwrites any previous table.
    pub fn set_frequency(&mut self, frequency: frequency::Frequency) {
        self.frequency = Some(frequency);
    }

    /// Borrow the underlying lexicon.
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// Borrow the morphology, if the checker has one.
    pub fn morphology(&self) -> Option<&morphology::Morphology> {
        self.morphology.as_ref()
    }

    /// Check `text` and return every problem found, in document order.
    pub fn check(&self, text: &str) -> Vec<Diagnostic> {
        let tokens = tokenize(text);
        let words: Vec<Token<'_>> =
            tokens.iter().filter(|t| t.kind == TokenKind::Word).cloned().collect();

        // Someone writing Macedonian on a Latin keyboard writes whole phrases
        // that way — "Zdravo, kako si". A lone Latin word inside Cyrillic text
        // is nearly always a foreign name or a citation, and converting it
        // would be wrong. So the transliteration rule only fires on a run of
        // two or more adjacent Latin words.
        let latin: Vec<bool> = words.iter().map(|t| is_all_latin(t.text)).collect();
        let in_latin_run: Vec<bool> = (0..words.len())
            .map(|i| {
                latin[i]
                    && (i > 0 && latin[i - 1] || i + 1 < latin.len() && latin[i + 1])
            })
            .collect();

        let mut out = Vec::new();
        for (i, token) in words.iter().enumerate() {
            // Suffixed ordinals and codes (`1-ви`, `А4`) are not lexicon entries.
            if token.text.chars().any(|c| c.is_numeric()) {
                continue;
            }
            if let Some(d) =
                self.check_word(token.text, token.char_start, token.char_end, in_latin_run[i])
            {
                out.push(d);
            }
        }

        // Grammar needs the punctuation the spelling pass filtered out, so that
        // a sentence boundary is not mistaken for a phrase boundary.
        if let Some(morphology) = &self.morphology {
            out.extend(grammar::check(&tokens, morphology));
        }

        out.sort_by_key(|d| (d.char_start, d.char_end));
        out
    }

    fn check_word(
        &self,
        word: &str,
        start: usize,
        end: usize,
        in_latin_run: bool,
    ) -> Option<Diagnostic> {
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
            // BBC, CNN, ISDN, and Roman numerals like XIV transliterate into
            // plausible-looking nonsense (Ббц, Цнн, Хив). They are not words.
            if is_acronym(word) {
                return None;
            }
            // An isolated Latin word among Cyrillic is a name or a citation.
            if !in_latin_run {
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
            suggestions: self.ranked_suggestions(word),
        })
    }

    /// FST hits reranked by frequency when a table is loaded.
    /// No table = the lexicon's own order. Never invents candidates.
    fn ranked_suggestions(&self, word: &str) -> Vec<String> {
        let mut hits = self.lexicon.suggest(word, MAX_SUGGESTIONS * 4);
        if let Some(f) = &self.frequency {
            let qchars: Vec<char> = word.to_lowercase().chars().collect();
            hits.sort_by_cached_key(|c| {
                let cchars: Vec<char> = c.chars().collect();
                (
                    std::cmp::Reverse(f.get(&c.to_lowercase())),
                    crate::lexicon::weighted_cost(&qchars, &cchars),
                    c.clone(),
                )
            });
            hits.truncate(MAX_SUGGESTIONS);
        } else {
            hits.truncate(MAX_SUGGESTIONS);
        }
        hits
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
            "и", "во", "како", "денес", "убав", "многу",
        ];
        Checker::new(Lexicon::build_from_unsorted(words).unwrap()).unwrap()
    }

    fn rules(text: &str) -> Vec<String> {
        checker().check(text).into_iter().map(|d| d.rule).collect()
    }

    #[test]
    fn frequency_table_promotes_the_common_word() {
        use crate::frequency::Frequency;
        let words = ["книга", "книги", "книгата"];
        let base = Checker::new(Lexicon::build_from_unsorted(words).unwrap()).unwrap();
        let freq = Frequency::from_bytes(&Frequency::build(&[("книги", 900), ("книга", 1)]).unwrap())
            .unwrap();
        let ranked = base.with_frequency(freq);
        let got = ranked.check("книгаи");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].suggestions.first().map(String::as_str), Some("книги"));
    }

    #[test]
    fn frequency_ties_fall_back_to_confusion_cost() {
        use crate::frequency::Frequency;
        // Neither candidate is in the table: equal frequency, so the
        // confusable ќ must still beat м for the query "как".
        let base =
            Checker::new(Lexicon::build_from_unsorted(["ќак", "мак"]).unwrap()).unwrap();
        let freq =
            Frequency::from_bytes(&Frequency::build(&[("некојдруг", 7)]).unwrap()).unwrap();
        let got = base.with_frequency(freq).check("как");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].suggestions.first().map(String::as_str), Some("ќак"));
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
        let found = checker().check("zdravo kako si denes");
        assert!(!found.is_empty());
        assert!(found.iter().all(|d| d.rule == rule::LATIN_TEXT), "{found:?}");
        assert!(found[0].suggestions.contains(&"здраво".to_string()));
    }

    #[test]
    fn ambiguous_transliteration_is_resolved_by_the_lexicon() {
        // `konj` could read коњ or конј; only коњ is a word.
        let found = checker().check("konj konj");
        assert_eq!(found[0].suggestions, vec!["коњ".to_string()]);
    }

    #[test]
    fn an_isolated_latin_word_among_cyrillic_is_a_name_not_a_typo() {
        // "Ohrid" in Macedonian prose is a citation or a foreign rendering,
        // not someone typing Охрид on a Latin keyboard.
        assert!(checker().check("Тој оди во Skopje дома").is_empty());
    }

    #[test]
    fn acronyms_are_not_transliterated() {
        // Without this, BBC becomes "Ббц" and XIV becomes "Хив".
        assert!(checker().check("BBC CNN").is_empty());
        assert!(checker().check("XIV III").is_empty());
    }

    #[test]
    fn genuine_foreign_words_are_left_alone() {
        // No reading of these is a Macedonian word, so we stay quiet.
        assert!(checker().check("Wikipedia github").is_empty());
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
        let got = rules("zdravo kako, мaкeдoнски книгаа");
        assert_eq!(
            got,
            vec![rule::LATIN_TEXT, rule::LATIN_TEXT, rule::HOMOGLYPH, rule::SPELL]
        );
    }
}
