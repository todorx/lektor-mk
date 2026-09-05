//! Grammar rules over morphologically analysed text.
//!
//! Everything here works on a window of analysed tokens rather than on raw
//! characters, which is what separates these checks from spelling. The rules
//! are Rust functions for now; the intent is to move them to declarative data
//! once the shape of a rule has settled, so that someone who knows Macedonian
//! grammar but not Rust can add one.
//!
//! # Precision over recall
//!
//! A proofreader that cries wolf gets switched off, so every rule here is
//! written to stay silent when it is not sure. In practice that means:
//!
//! * a rule fires only when *every* reading of a word supports it, never when
//!   merely one ambiguous reading does;
//! * the two words must be genuinely adjacent, with no punctuation between
//!   them, so a sentence boundary cannot be mistaken for a phrase;
//! * features must agree, so two unrelated words that happen to sit next to
//!   each other are not read as one phrase.
//!
//! Only 70.6% of adjacent word pairs in real Macedonian have both members
//! analysed, so these rules see about two thirds of the text. That is a recall
//! ceiling, not a precision problem.

use crate::diagnostic::{rule, Diagnostic, Severity};
use crate::morphology::{Analysis, Gender, Morphology, Number, Pos};
use crate::tokenizer::{Token, TokenKind};

/// Run every grammar rule over `tokens`.
pub fn check(tokens: &[Token<'_>], morph: &Morphology) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    double_definite_article(tokens, morph, &mut out);
    out
}

/// The definite article attaches to the **first** element of a noun phrase, and
/// to that element only.
///
/// ```text
/// убавата книга    ✓  article on the adjective
/// убава книгата    ✓  article on the noun, no adjective marking
/// убавата книгата  ✗  marked twice
/// ```
///
/// This is one of the checks no general-purpose proofreader can perform, since
/// it depends on Macedonian marking definiteness as a suffix rather than with a
/// separate word.
fn double_definite_article(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for pair in tokens.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);

        // Adjacent words only. Punctuation between them means these are not one
        // phrase — possibly not even one sentence.
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }

        let adjective = morph.analyze(first.text);
        let noun = morph.analyze(second.text);

        let Some((adj_gender, adj_number)) = definite_reading(&adjective, Pos::Adjective) else {
            continue;
        };
        let Some((noun_gender, noun_number)) = definite_reading(&noun, Pos::Noun) else {
            continue;
        };

        // If they do not agree they are not one noun phrase, and the doubling
        // we think we see is a coincidence of adjacency.
        if !compatible(adj_gender, noun_gender) || adj_number != noun_number {
            continue;
        }

        out.push(Diagnostic {
            rule: rule::DOUBLE_DEFINITE.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: second.char_end,
            text: format!("{} {}", first.text, second.text),
            message: "Членот се пишува само на првиот збор во именската група.".to_string(),
            suggestions: indefinite_form(&noun, morph, noun_gender, noun_number)
                .map(|bare| vec![format!("{} {}", first.text, bare)])
                .unwrap_or_default(),
        });
    }
}

/// Gender and number of `word` when it is unambiguously a definite `pos`.
///
/// Returns `None` unless the word has a reading of that part of speech and
/// *every* such reading is definite — an ambiguous word is left alone.
fn definite_reading(analyses: &[Analysis<'_>], pos: Pos) -> Option<(Gender, Number)> {
    let matching: Vec<&Analysis<'_>> =
        analyses.iter().filter(|a| a.pos() == Some(pos)).collect();
    if matching.is_empty() || !matching.iter().all(|a| a.is_definite()) {
        return None;
    }
    let first = matching.first()?;
    Some((first.gender()?, first.number()?))
}

/// Do two genders agree? `mfn`/`mf` are underspecified and agree with anything,
/// which matters because Macedonian plural adjectives do not distinguish gender.
fn compatible(a: Gender, b: Gender) -> bool {
    a == b || a == Gender::Common || b == Gender::Common
}

/// The bare form of the noun, so the suggestion can drop the second article.
///
/// The lemma is the indefinite singular, so it works directly for singulars.
/// For plurals we would need to generate a form we do not store, so we offer no
/// suggestion rather than a wrong one.
fn indefinite_form(
    analyses: &[Analysis<'_>],
    morph: &Morphology,
    gender: Gender,
    number: Number,
) -> Option<String> {
    let lemma = analyses.iter().find(|a| a.pos() == Some(Pos::Noun))?.lemma();
    let ok = morph.analyze(lemma).iter().any(|a| {
        a.pos() == Some(Pos::Noun)
            && !a.is_definite()
            && a.number() == Some(number)
            && a.gender().is_some_and(|g| compatible(g, gender))
    });
    ok.then(|| lemma.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::morphology::Morphology;
    use crate::tokenizer::tokenize;
    use std::collections::BTreeMap;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn morphology() -> Morphology {
        let mut e: BTreeMap<String, Vec<(String, Vec<String>)>> = BTreeMap::new();
        let mut add = |form: &str, lemma: &str, t: &[&str]| {
            e.entry(form.to_string()).or_default().push((lemma.to_string(), tags(t)));
        };
        // книга: indefinite and definite, singular and plural
        add("книга", "книга", &["n", "f", "sg", "nom", "ind"]);
        add("книгата", "книга", &["n", "f", "sg", "nom", "def"]);
        add("книги", "книга", &["n", "f", "pl", "nom", "ind"]);
        add("книгите", "книга", &["n", "f", "pl", "nom", "def"]);
        // убав: agreeing adjective forms
        add("убава", "убав", &["adj", "f", "sg", "nom", "ind"]);
        add("убавата", "убав", &["adj", "f", "sg", "nom", "def"]);
        add("убавите", "убав", &["adj", "mfn", "pl", "nom", "def"]);
        add("убавиот", "убав", &["adj", "m", "sg", "nom", "def"]);
        // a masculine noun, for the disagreement case
        add("градот", "град", &["n", "m", "sg", "nom", "def"]);
        add("град", "град", &["n", "m", "sg", "nom", "ind"]);
        // a word that is ambiguous between definite adjective and something else
        add("новата", "нов", &["adj", "f", "sg", "nom", "def"]);
        add("куќата", "куќа", &["n", "f", "sg", "nom", "def"]);
        add("куќа", "куќа", &["n", "f", "sg", "nom", "ind"]);
        Morphology::from_bytes(&Morphology::build(&e).unwrap()).unwrap()
    }

    fn run(text: &str) -> Vec<Diagnostic> {
        check(&tokenize(text), &morphology())
    }

    #[test]
    fn flags_the_article_marked_twice() {
        let found = run("убавата книгата");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::DOUBLE_DEFINITE);
        assert_eq!(found[0].text, "убавата книгата");
        assert_eq!(found[0].suggestions, vec!["убавата книга".to_string()]);
    }

    #[test]
    fn accepts_the_article_on_the_adjective_only() {
        assert!(run("убавата книга").is_empty());
    }

    #[test]
    fn accepts_the_article_on_the_noun_only() {
        assert!(run("убава книгата").is_empty());
    }

    #[test]
    fn flags_plurals_but_offers_no_guess_it_cannot_make() {
        // Correct is "убавите книги"; we do not store a generator, so we report
        // the error without inventing a replacement.
        let found = run("убавите книгите");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].suggestions.is_empty(), "{:?}", found[0].suggestions);
    }

    #[test]
    fn ignores_a_pair_that_does_not_agree() {
        // Feminine adjective, masculine noun — not one noun phrase.
        assert!(run("убавата градот").is_empty());
    }

    #[test]
    fn does_not_reach_across_punctuation() {
        // Two separate sentences must never be read as one phrase.
        assert!(run("Ја видов убавата. Книгата беше таму.").is_empty());
        assert!(run("убавата, книгата").is_empty());
    }

    #[test]
    fn stays_silent_on_unanalysed_words() {
        assert!(run("непознатата непознатата").is_empty());
    }

    #[test]
    fn finds_more_than_one_occurrence() {
        let found = run("убавата книгата и новата куќата");
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[1].suggestions, vec!["новата куќа".to_string()]);
    }

    #[test]
    fn spans_cover_both_words() {
        let text = "убавата книгата";
        let found = run(text);
        let sliced: String = text
            .chars()
            .skip(found[0].char_start)
            .take(found[0].char_end - found[0].char_start)
            .collect();
        assert_eq!(sliced, "убавата книгата");
    }
}
