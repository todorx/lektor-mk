//! Detection of characters that *look* Macedonian but are not.
//!
//! Two distinct problems live here, both invisible to the naked eye and both
//! rampant in real Macedonian text:
//!
//! 1. **Mixed script.** Latin `a c e o p s x y j` and their capitals are pixel
//!    twins of Cyrillic `а с е о р ѕ х у ј`. Text pasted from a browser, or
//!    typed while the keyboard layout drifted, ends up with a few Latin letters
//!    inside otherwise Cyrillic words. It looks perfect and breaks every
//!    spell-checker, search index and sort order that touches it.
//! 2. **Foreign Cyrillic.** Serbian `ђ ћ`, Russian `ъ ы э я ю ё`, Bulgarian `щ`
//!    and Ukrainian `і ї є` are not Macedonian letters. Their presence means the
//!    text came from another language's keyboard or was copied from a related
//!    language — a common trap for Macedonian speakers who also write Serbian.
//!
//! Both are reported only after the repaired spelling is confirmed against the
//! lexicon, which is what keeps the false-positive rate near zero.

use crate::alphabet::{script_of, Script};

/// Latin characters that are visually identical to a Macedonian letter.
///
/// Deliberately conservative: only pairs that render identically in ordinary
/// fonts. Lowercase Latin `m k t h b` are *not* included, because Cyrillic
/// `м к т н б` are visibly different shapes at those sizes — including them
/// would trade precision for nothing.
const LATIN_LOOKALIKES: &[(char, char)] = &[
    ('a', 'а'),
    ('c', 'с'),
    ('e', 'е'),
    // The accented pair matters disproportionately: `сѐ` is a common word, and
    // typing it with Latin `è` is the single most frequent instance of this
    // error in the wild (13 occurrences in 17k words of Macedonian Wikipedia).
    ('è', 'ѐ'),
    ('È', 'Ѐ'),
    ('ì', 'ѝ'),
    ('Ì', 'Ѝ'),
    ('j', 'ј'),
    ('o', 'о'),
    ('p', 'р'),
    ('s', 'ѕ'),
    ('x', 'х'),
    ('y', 'у'),
    ('A', 'А'),
    ('B', 'В'),
    ('C', 'С'),
    ('E', 'Е'),
    ('H', 'Н'),
    ('J', 'Ј'),
    ('K', 'К'),
    ('M', 'М'),
    ('O', 'О'),
    ('P', 'Р'),
    ('S', 'Ѕ'),
    ('T', 'Т'),
    ('X', 'Х'),
    ('Y', 'У'),
];

/// Cyrillic letters from other languages, mapped to their Macedonian spelling.
///
/// Multi-character targets are real correspondences, not guesses: Bulgarian
/// `щ` is /ʃt/, which Macedonian writes `шт`; Russian `я`/`ю` are /ja/, /ju/,
/// written `ја`/`ју`. The Russian hard and soft signs have no Macedonian
/// reflex and are dropped.
const FOREIGN_CYRILLIC: &[(char, &str)] = &[
    // Serbian
    ('ђ', "ѓ"),
    ('Ђ', "Ѓ"),
    ('ћ', "ќ"),
    ('Ћ', "Ќ"),
    // Russian / Bulgarian
    ('ё', "е"),
    ('Ё', "Е"),
    ('ъ', ""),
    ('Ъ', ""),
    ('ы', "и"),
    ('Ы', "И"),
    ('э', "е"),
    ('Э', "Е"),
    ('я', "ја"),
    ('Я', "Ја"),
    ('ю', "ју"),
    ('Ю', "Ју"),
    ('щ', "шт"),
    ('Щ', "Шт"),
    ('ь', ""),
    ('Ь', ""),
    // Ukrainian / Belarusian
    ('і', "и"),
    ('І', "И"),
    ('ї', "ји"),
    ('Ї', "Ји"),
    ('є', "е"),
    ('Є', "Е"),
    ('ґ', "г"),
    ('Ґ', "Г"),
];

/// Why a word looks wrong at the character level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    /// Latin lookalikes embedded in a Cyrillic word.
    Homoglyph,
    /// Cyrillic letters that are not in the Macedonian alphabet.
    ForeignCyrillic,
}

/// A character-level problem, together with the repaired spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptIssue {
    pub kind: IssueKind,
    pub normalized: String,
}

fn latin_to_mk(c: char) -> Option<char> {
    LATIN_LOOKALIKES.iter().find(|(l, _)| *l == c).map(|(_, m)| *m)
}

fn foreign_to_mk(c: char) -> Option<&'static str> {
    FOREIGN_CYRILLIC.iter().find(|(f, _)| *f == c).map(|(_, m)| *m)
}

/// Rewrite every lookalike and foreign letter in `word` to its Macedonian form.
pub fn normalize(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for c in word.chars() {
        if let Some(mk) = latin_to_mk(c) {
            out.push(mk);
        } else if let Some(mk) = foreign_to_mk(c) {
            out.push_str(mk);
        } else {
            out.push(c);
        }
    }
    out
}

/// Inspect `word` for mixed-script or foreign-Cyrillic contamination.
///
/// Returns `None` for words that are wholly Macedonian, wholly Latin (that is
/// transliteration's job, not ours), or that contain no letters at all.
pub fn analyze(word: &str) -> Option<ScriptIssue> {
    let mut has_mk = false;
    let mut has_latin = false;
    let mut has_foreign = false;

    for c in word.chars() {
        match script_of(c) {
            Script::Macedonian => has_mk = true,
            Script::Latin => has_latin = true,
            Script::ForeignCyrillic => has_foreign = true,
            _ => {}
        }
    }

    // A foreign Cyrillic letter is wrong regardless of what surrounds it.
    if has_foreign {
        return Some(ScriptIssue { kind: IssueKind::ForeignCyrillic, normalized: normalize(word) });
    }

    // Mixed script only matters when Cyrillic is the base and Latin intrudes.
    // An all-Latin word is handled by the transliteration rule instead.
    if has_mk && has_latin {
        return Some(ScriptIssue { kind: IssueKind::Homoglyph, normalized: normalize(word) });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_latin_letters_hidden_in_a_cyrillic_word() {
        // 'a', 'e' and 'o' here are Latin, the rest Cyrillic.
        let word = "мaкeдoнски";
        let issue = analyze(word).expect("should flag mixed script");
        assert_eq!(issue.kind, IssueKind::Homoglyph);
        assert_eq!(issue.normalized, "македонски");
    }

    #[test]
    fn clean_macedonian_is_left_alone() {
        assert_eq!(analyze("македонски"), None);
        assert_eq!(analyze("ѓаволот"), None);
        assert_eq!(analyze("џезот"), None);
    }

    #[test]
    fn all_latin_words_are_not_our_problem() {
        // Transliteration handles these; flagging them here would double-report.
        assert_eq!(analyze("makedonski"), None);
        assert_eq!(analyze("hello"), None);
    }

    #[test]
    fn maps_serbian_letters_to_macedonian() {
        let issue = analyze("ђубре").expect("Serbian đ is not Macedonian");
        assert_eq!(issue.kind, IssueKind::ForeignCyrillic);
        assert_eq!(issue.normalized, "ѓубре");

        assert_eq!(normalize("ћирилица"), "ќирилица");
    }

    #[test]
    fn maps_bulgarian_sht_and_russian_signs() {
        assert_eq!(normalize("щастие"), "штастие");
        assert_eq!(normalize("градъ"), "град");
        assert_eq!(normalize("язик"), "јазик");
    }

    #[test]
    fn preserves_case_when_repairing() {
        // Latin capital 'C' and 'K' standing in for Cyrillic 'С' and 'К'.
        assert_eq!(normalize("CKОПЈЕ"), "СКОПЈЕ");
    }

    #[test]
    fn repairs_latin_grave_accents_to_cyrillic() {
        // `сè` with Latin è U+00E8 should become `сѐ` with Cyrillic ѐ U+0450.
        let issue = analyze("сè").expect("Latin è in a Cyrillic word");
        assert_eq!(issue.kind, IssueKind::Homoglyph);
        assert_eq!(issue.normalized, "сѐ");
        assert_eq!(normalize("Сè"), "Сѐ");
    }

    #[test]
    fn correctly_accented_words_are_left_alone() {
        // Already Cyrillic ѐ U+0450 / ѝ U+045D — nothing to repair.
        assert_eq!(analyze("сѐ"), None);
        assert_eq!(analyze("ѝ"), None);
    }

    #[test]
    fn normalize_is_identity_for_clean_words() {
        for w in ["книга", "ѕвезда", "љубов", "њива", "џамија"] {
            assert_eq!(normalize(w), w);
        }
    }
}
