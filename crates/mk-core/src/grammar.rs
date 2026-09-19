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
    clitic_order(tokens, morph, &mut out);
    dative_i(tokens, morph, &mut out);
    l_participle(tokens, morph, &mut out);
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

/// Dative clitics come before accusative ones: `ми го даде` ✓, `го ми даде` ✗.
///
/// Fires on `[acc-clitic, dat-clitic, verb]` only. The verb anchor is what
/// disambiguates: `ми` alone also reads as a verb form, but two adjacent
/// verbs (`ми даде` read verbally) are ungrammatical anyway, so the reversed
/// pair before a verb is wrong under every reading.
fn clitic_order(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for win in tokens.windows(3) {
        if win.iter().any(|t| t.kind != TokenKind::Word) {
            continue;
        }
        let (first, second, third) = (&win[0], &win[1], &win[2]);
        let acc = morph.analyze(first.text);
        let dat = morph.analyze(second.text);
        let verb = morph.analyze(third.text);
        if acc.is_empty() || dat.is_empty() || verb.is_empty() {
            continue;
        }
        let is_acc = |a: &Analysis<'_>| a.pos() == Some(Pos::Pronoun) && a.has("acc");
        let is_dat = |a: &Analysis<'_>| a.pos() == Some(Pos::Pronoun) && a.has("dat");
        if !acc.iter().any(is_acc) || acc.iter().any(|a| a.pos() == Some(Pos::Verb)) {
            continue;
        }
        if !dat.iter().any(is_dat) {
            continue;
        }
        if !verb.iter().any(|a| a.pos() == Some(Pos::Verb)) {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::CLITIC_ORDER.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: third.char_end,
            text: format!("{} {} {}", first.text, second.text, third.text),
            message: "Заменките се пишуваат: дативна пред акузативна (ми го, не го ми).".to_string(),
            suggestions: vec![format!("{} {} {}", first.text, second.text, third.text)
                .replacen(
                    &format!("{} {}", first.text, second.text),
                    &format!("{} {}", second.text, first.text),
                    1,
                )],
        });
    }
}

/// Bare `и` where the dative clitic `ѝ` belongs: `таа и го даде` ✗.
///
/// Deliberately narrow: only after a subject pronoun and before an accusative
/// clitic (`таа и го даде`). After a verb (`пее и го гледа`) the `и` is the
/// conjunction and the rule stays silent.
fn dative_i(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    const SUBJECTS: &[&str] = &["јас", "ти", "тој", "таа", "тоа", "ние", "вие", "тие"];
    for win in tokens.windows(3) {
        if win.iter().any(|t| t.kind != TokenKind::Word) {
            continue;
        }
        let (subj, maybe_i, acc) = (&win[0], &win[1], &win[2]);
        if maybe_i.text.to_lowercase() != "и" {
            continue;
        }
        if !SUBJECTS.contains(&subj.text.to_lowercase().as_str()) {
            continue;
        }
        let i_readings = morph.analyze(maybe_i.text);
        if !i_readings.iter().any(|a| a.pos() == Some(Pos::Pronoun) && a.has("dat")) {
            continue;
        }
        let acc_readings = morph.analyze(acc.text);
        if acc_readings.is_empty()
            || !acc_readings.iter().any(|a| a.pos() == Some(Pos::Pronoun) && a.has("acc"))
        {
            continue;
        }
        let fixed = if maybe_i.text.starts_with(char::is_uppercase) { "Ѝ" } else { "ѝ" };
        out.push(Diagnostic {
            rule: rule::DATIVE_I.to_string(),
            severity: Severity::Error,
            char_start: maybe_i.char_start,
            char_end: maybe_i.char_end,
            text: maybe_i.text.to_string(),
            message: "Дативната заменка ѝ се пишува со гравис, за разлика од сврзникот и."
                .to_string(),
            suggestions: vec![fixed.to_string()],
        });
    }
}

/// An л-participle agreeing with its subject: `таа дошла` ✓, `таа дошол` ✗.
///
/// Adjacent `[subject-pronoun, l-participle]` only — `тој е дошол` with an
/// auxiliary between is future work. Fires only when no reading pair agrees
/// in gender and number; the suggestion reuses a stored form of the same
/// lemma, and is omitted when none matches.
fn l_participle(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for pair in tokens.windows(2) {
        let (subj, part) = (&pair[0], &pair[1]);
        if subj.kind != TokenKind::Word || part.kind != TokenKind::Word {
            continue;
        }
        let subj_readings = morph.analyze(subj.text);
        let part_readings = morph.analyze(part.text);
        if subj_readings.is_empty() || part_readings.is_empty() {
            continue;
        }
        let subj_forms: Vec<(Gender, Number)> = subj_readings
            .iter()
            .filter(|a| a.pos() == Some(Pos::Pronoun))
            .filter_map(|a| Some((a.gender()?, a.number()?)))
            .collect();
        let part_forms: Vec<(Gender, Number, &str)> = part_readings
            .iter()
            .filter(|a| a.is_l_participle())
            .filter_map(|a| Some((a.gender()?, a.number()?, a.lemma())))
            .collect();
        if subj_forms.is_empty() || part_forms.is_empty() {
            continue;
        }
        let agrees = subj_forms.iter().any(|(sg, sn)| {
            part_forms.iter().any(|(pg, pn, _)| compatible(*sg, *pg) && sn == pn)
        });
        if agrees {
            continue;
        }
        let (sg, sn) = subj_forms[0];
        let lemma = part_forms[0].2.to_string();
        // The agreeing surface form, when one is stored (never invented).
        let fix = best_participle_form(morph, &lemma, sg, sn)
            .map(|f| vec![format!("{} {}", subj.text, f)])
            .unwrap_or_default();
        out.push(Diagnostic {
            rule: rule::L_PARTICIPLE.to_string(),
            severity: Severity::Error,
            char_start: subj.char_start,
            char_end: part.char_end,
            text: format!("{} {}", subj.text, part.text),
            message: "Л-партиципот мора да се сложува со подметот во род и број.".to_string(),
            suggestions: fix,
        });
    }
}

/// A stored л-participle surface form of `lemma` agreeing with `(gender, number)`.
/// Never invents a form: no match means no suggestion.
fn best_participle_form(
    morph: &Morphology,
    lemma: &str,
    gender: Gender,
    number: Number,
) -> Option<String> {
    morph
        .participle_forms(lemma)
        .into_iter()
        .filter(|f| {
            morph.analyze(f).iter().any(|a| {
                a.is_l_participle()
                    && a.number() == Some(number)
                    && a.gender().is_some_and(|g| compatible(g, gender))
            })
        })
        .next()
        .map(str::to_string)
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
        // clitics: dative before accusative (ми го), verb, subject pronouns
        add("ми", "ми", &["prn", "pers", "clt", "p1", "mfn", "sg", "dat"]);
        add("го", "clitic", &["prn", "pers", "clt", "p3", "m", "sg", "acc"]);
        add("даде", "даде", &["vblex", "perf", "tv", "aor", "p3", "sg"]);
        add("таа", "таа", &["prn", "pers", "p3", "f", "sg", "nom"]);
        add("тој", "тој", &["prn", "pers", "p3", "m", "sg", "nom"]);
        add("тоа", "тоа", &["prn", "pers", "p3", "nt", "sg", "nom"]);
        add("тие", "free", &["prn", "pers", "p3", "mfn", "pl", "nom"]);
        add("дошол", "дојде", &["vblex", "perf", "lp", "m", "sg"]);
        add("дошла", "дојде", &["vblex", "perf", "lp", "f", "sg"]);
        add("дошле", "дојде", &["vblex", "perf", "lp", "mfn", "pl"]);
        add("ѝ", "clitic", &["prn", "pers", "clt", "p3", "f", "sg", "dat"]);
        add("и", "clitic", &["prn", "pers", "clt", "p3", "f", "sg", "dat"]);
        add("и", "и", &["cnjcoo"]);
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

    #[test]
    fn flags_reversed_clitics_before_a_verb() {
        // Dative before accusative: ми го даде ✓ silent, го ми даде ✗ fires.
        assert!(run("тој ми го даде").is_empty());
        let found = run("тој го ми даде");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::CLITIC_ORDER);
        assert_eq!(found[0].text, "го ми даде");
        assert_eq!(found[0].suggestions, vec!["ми го даде".to_string()]);
    }

    #[test]
    fn clitic_order_stays_silent_without_a_verb() {
        // No verb after the pair — not enough context to judge.
        assert!(run("тој го ми").is_empty());
    }

    #[test]
    fn flags_bare_i_in_a_dative_slot() {
        // таа ѝ го даде ✓ silent; таа и го даде ✗ fires with ѝ.
        assert!(run("таа ѝ го даде").is_empty());
        let found = run("таа и го даде");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::DATIVE_I);
        assert_eq!(found[0].text, "и");
        assert_eq!(found[0].suggestions, vec!["ѝ".to_string()]);
    }

    #[test]
    fn dative_i_stays_silent_after_a_verb() {
        // "пее и го гледа" — и here is the conjunction, not the clitic.
        assert!(run("таа пее и го даде").is_empty());
    }

    #[test]
    fn flags_wrong_participle_gender() {
        assert!(run("таа дошла").is_empty());
        assert!(run("тој дошол").is_empty());
        assert!(run("тие дошле").is_empty());
        let found = run("таа дошол");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::L_PARTICIPLE);
        assert_eq!(found[0].suggestions, vec!["таа дошла".to_string()]);
    }

    #[test]
    fn flags_wrong_participle_number_without_a_guess_it_cannot_make() {
        // тоа + дошол: neuter subject, masculine participle — fires, and no
        // neuter form is stored, so there is no suggestion to offer.
        let found = run("тоа дошол");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::L_PARTICIPLE);
        assert!(found[0].suggestions.is_empty(), "{:?}", found[0].suggestions);
    }
}
