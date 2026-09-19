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
pub fn check(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    morph: &Morphology,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    double_definite_article(tokens, morph, &mut out);
    clitic_order(tokens, morph, &mut out);
    dative_i(tokens, morph, &mut out);
    l_participle(tokens, morph, &mut out);
    ne_fused(tokens, morph, &mut out);
    naj_separated(tokens, lexicon, morph, &mut out);
    sentence_capital(tokens, lexicon, &mut out);
    po_separated(tokens, lexicon, morph, &mut out);
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
        // The first word must be unambiguously pronominal: не reads as an
        // accusative clitic but is usually the negation particle, and swapping
        // it ("им не остави") invents ungrammatical text.
        if !acc.iter().any(is_acc) || acc.iter().any(|a| a.pos() != Some(Pos::Pronoun)) {
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

/// `не` stays separate from finite verbs: `не сака` ✓, `несака` ✗ (§189).
///
/// Precision guards (all must pass):
/// - the whole word has no morphological analysis — established words
///   (`негодува`, `недели`) always win;
/// - lexicalized/ambiguous fusions are excepted (`нестан-` = vanish, but
///   also `не стане`; `непогод-` = disasters, but also `не погоди`);
/// - the stem reads ONLY as a verb (a noun/adjective reading like
///   `прав` in `неправ` vetoes);
/// - the stem has a present, imperative or imperfect reading — aorist-only
///   stems (`објаснив`) double as `-ив` adjectives (`необјаснив`);
/// - gerunds (`несакајќи`, `pprs`) and participles (`ненапишан`, `pp`,
///   л-participles) take `не-` fused (§187).
///
/// Deliberately ignores the spelling lexicon: the upstream wordlist is
/// polluted with fused typos (`несака`), so membership proves nothing.
fn ne_fused(
    tokens: &[Token<'_>],
    morph: &Morphology,
    out: &mut Vec<Diagnostic>,
) {
    /// Fused stems whose correct reading collides with negation.
    const EXCEPTED: &[&str] = &["нестан", "непогод"];
    for t in tokens.iter().filter(|t| t.kind == TokenKind::Word) {
        let lower = t.text.to_lowercase();
        let Some(stem) = lower.strip_prefix("не") else {
            continue;
        };
        if stem.chars().count() < 2 {
            continue;
        }
        if !morph.analyze(t.text).is_empty() {
            continue;
        }
        if EXCEPTED.iter().any(|e| lower.starts_with(e)) {
            continue;
        }
        let readings = morph.analyze(stem);
        if readings.is_empty()
            || readings.iter().any(|a| {
                !matches!(a.pos(), None | Some(Pos::Verb) | Some(Pos::Particle))
            })
        {
            continue;
        }
        let finite = readings.iter().any(|a| {
            a.pos() == Some(Pos::Verb)
                && (a.has("pres") || a.has("imp") || a.has("impf"))
                && !a.has("pp")
                && !a.has("pprs")
                && !a.is_l_participle()
        });
        if !finite {
            continue;
        }
        let cut = t.text.char_indices().nth(2).map(|(i, _)| i).unwrap_or(t.text.len());
        out.push(Diagnostic {
            rule: rule::NE_FUSED.to_string(),
            severity: Severity::Error,
            char_start: t.char_start,
            char_end: t.char_end,
            text: t.text.to_string(),
            message: "Негацијата не се пишува слеано со глаголот.".to_string(),
            suggestions: vec![format!("{} {}", &t.text[..cut], &t.text[cut..])],
        });
    }
}

/// `нај` never stands alone: `најдобар` ✓, `нај добар` ✗ (§207).
///
/// Fires only when the fused form is an established word, so the check
/// never invents vocabulary.
fn naj_separated(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    morph: &Morphology,
    out: &mut Vec<Diagnostic>,
) {
    for pair in tokens.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }
        if first.text.to_lowercase() != "нај" {
            continue;
        }
        let ok_pos = morph.analyze(second.text).iter().any(|a| {
            matches!(a.pos(), Some(Pos::Adjective | Pos::Noun | Pos::Verb))
        });
        if !ok_pos {
            continue;
        }
        let fused = format!("нај{}", second.text.to_lowercase());
        if !lexicon.contains(&fused) && morph.analyze(&fused).is_empty() {
            continue;
        }
        let fix = if first.text.starts_with(char::is_uppercase) {
            capitalize_first(&fused)
        } else {
            fused
        };
        out.push(Diagnostic {
            rule: rule::NAJ_SEPARATED.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: second.char_end,
            text: format!("{} {}", first.text, second.text),
            message: "Нај се пишува слеано со зборот што го степенува.".to_string(),
            suggestions: vec![fix],
        });
    }
}

fn po_separated(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    morph: &Morphology,
    out: &mut Vec<Diagnostic>,
) {
    for pair in tokens.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }
        if first.text.to_lowercase() != "по" {
            continue;
        }
        let ok_pos = morph.analyze(second.text).iter().any(|a| {
            matches!(a.pos(), Some(Pos::Adjective | Pos::Noun | Pos::Verb))
        });
        if !ok_pos {
            continue;
        }
        let fused = format!("по{}", second.text.to_lowercase());
        if !lexicon.contains(&fused) && morph.analyze(&fused).is_empty() {
            continue;
        }
        let fix = if first.text.starts_with(char::is_uppercase) {
            capitalize_first(&fused)
        } else {
            fused
        };
        out.push(Diagnostic {
            rule: rule::PO_SEPARATED.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: second.char_end,
            text: format!("{} {}", first.text, second.text),
            message: "По се пишува слеано со зборот што го степенува.".to_string(),
            suggestions: vec![fix],
        });
    }
}

fn sentence_capital(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    out: &mut Vec<Diagnostic>,
) {
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != TokenKind::Word
            || t.text.chars().count() < 2
            || t.text.chars().any(|c| c.is_uppercase())
            || !lexicon.contains(t.text)
        {
            continue;
        }
        let at_start = i == 0;
        let after_ender = i >= 2
            && tokens[i - 1].kind == TokenKind::Punct
            && tokens[i - 1].text.chars().all(|c| ".?!…".contains(c))
            && !(tokens[i - 2].kind == TokenKind::Word
                && tokens[i - 2].text.chars().count() == 1);
        let after_ender = after_ender
            || (i == 1
                && tokens[0].kind == TokenKind::Punct
                && tokens[0].text.chars().all(|c| ".?!…".contains(c)));
        if !at_start && !after_ender {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::SENTENCE_CAPITAL.to_string(),
            severity: Severity::Error,
            char_start: t.char_start,
            char_end: t.char_end,
            text: t.text.to_string(),
            message: "Реченицата почнува со голема буква.".to_string(),
            suggestions: vec![capitalize_first(t.text)],
        });
    }
}

/// Uppercase the first character, leave the rest untouched.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
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
            // Nominative only: го/ја/ги/и/не are objects or particles that
            // happen to read as pronouns — never the subject.
            .filter(|a| a.pos() == Some(Pos::Pronoun) && a.has("nom"))
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
        // Suggest only when the participle is one lexeme: била reads as бие
        // and е, and picking a paradigm blind proposes the wrong verb.
        let mut lp_lemmas: Vec<&str> = part_forms.iter().map(|(_, _, l)| *l).collect();
        lp_lemmas.sort_unstable();
        lp_lemmas.dedup();
        let lemma = part_forms[0].2.to_string();
        // The agreeing surface form, when one is stored (never invented).
        let fix = if lp_lemmas.len() == 1 {
            best_participle_form(morph, &lemma, sg, sn)
                .map(|f| vec![format!("{} {}", subj.text, f)])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
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
        add("ја", "clitic", &["prn", "pers", "clt", "p3", "f", "sg", "acc"]);
        add("даде", "даде", &["vblex", "perf", "tv", "aor", "p3", "sg"]);
        add("таа", "таа", &["prn", "pers", "p3", "f", "sg", "nom"]);
        add("тој", "тој", &["prn", "pers", "p3", "m", "sg", "nom"]);
        add("тоа", "тоа", &["prn", "pers", "p3", "nt", "sg", "nom"]);
        add("не", "не", &["adv"]);
        add("не", "clitic", &["prn", "pers", "clt", "p1", "mfn", "pl", "acc"]);
        add("им", "clitic", &["prn", "pers", "clt", "p3", "mfn", "pl", "dat"]);
        add("остави", "остави", &["vblex", "perf", "tv", "aor", "p3", "sg"]);
        add("водел", "воде", &["vblex", "impf", "lp", "m", "sg"]);
        add("водела", "воде", &["vblex", "impf", "lp", "f", "sg"]);
        add("тие", "free", &["prn", "pers", "p3", "mfn", "pl", "nom"]);
        add("дошол", "дојде", &["vblex", "perf", "lp", "m", "sg"]);
        add("дошла", "дојде", &["vblex", "perf", "lp", "f", "sg"]);
        add("дошле", "дојде", &["vblex", "perf", "lp", "mfn", "pl"]);
        add("ѝ", "clitic", &["prn", "pers", "clt", "p3", "f", "sg", "dat"]);
        add("и", "clitic", &["prn", "pers", "clt", "p3", "f", "sg", "dat"]);
        add("и", "и", &["cnjcoo"]);
        // не-fusion: verbs, nouns, and non-finite forms sharing stems
        add("сака", "сака", &["vblex", "impf", "tv", "pres", "p3", "sg"]);
        add("пријател", "пријател", &["n", "m", "sg", "nom", "ind"]);
        add("мој", "мој", &["det", "pos", "ind", "sg"]);
        add("сакајќи", "сака", &["vblex", "impf", "tv", "pprs", "adv"]);
        add("напишан", "напише", &["vblex", "perf", "tv", "pp", "m", "sg", "ind"]);
        add("прави", "прав", &["adj", "mfn", "pl", "nom", "ind"]);
        add("прави", "прави", &["vblex", "impf", "tv", "imp", "sg"]);
        add("објаснив", "објасни", &["vblex", "perf", "tv", "aor", "p1", "sg"]);
        add("погоди", "погоди", &["vblex", "perf", "tv", "aor", "p3", "sg"]);
        add("добар", "добар", &["adj", "m", "sg", "nom", "ind"]);
        add("најдобар", "добар", &["pref", "sup", "adj", "m", "sg", "nom", "ind"]);
        add("подобар", "добар", &["pref", "comp", "adj", "m", "sg", "nom", "ind"]);
        add("пат", "пат", &["n", "m", "sg", "nom", "ind"]);
        Morphology::from_bytes(&Morphology::build(&e).unwrap()).unwrap()
    }

    /// Words the test checker treats as established vocabulary.
    fn lexicon() -> crate::lexicon::Lexicon {
        crate::lexicon::Lexicon::from_bytes(
            crate::lexicon::Lexicon::build_from_unsorted(
                [
                    "тој", "таа", "тоа", "тие", "ми", "го", "ја", "им", "даде", "остави",
                    "дошол", "дошла", "дошле", "водел", "водела", "убавата", "книгата",
                    "книга", "и", "ѝ", "не", "сака", "пријател", "мој", "добар",
                    "непријател", "нестане", "најдобар", "подобар", "утре",
                ]
                .into_iter(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn run(text: &str) -> Vec<Diagnostic> {
        check(&tokenize(text), &lexicon(), &morphology())
    }

    /// Diagnostics of one rule only. Older tests feed lowercase fragments;
    /// since the sentence-capital rule, those also read as sentences
    /// starting lowercase, so each older test scopes to its own rule.
    fn run_rule(text: &str, rule: &str) -> Vec<Diagnostic> {
        run(text).into_iter().filter(|d| d.rule == rule).collect()
    }

    #[test]
    fn flags_the_article_marked_twice() {
        let found = run_rule("убавата книгата", rule::DOUBLE_DEFINITE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::DOUBLE_DEFINITE);
        assert_eq!(found[0].text, "убавата книгата");
        assert_eq!(found[0].suggestions, vec!["убавата книга".to_string()]);
    }

    #[test]
    fn accepts_the_article_on_the_adjective_only() {
        assert!(run_rule("убавата книга", rule::DOUBLE_DEFINITE).is_empty());
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
        assert!(run_rule("убавата градот", rule::DOUBLE_DEFINITE).is_empty());
    }

    #[test]
    fn does_not_reach_across_punctuation() {
        // Two separate sentences must never be read as one phrase.
        assert!(
            run_rule("Ја видов убавата. Книгата беше таму.", rule::DOUBLE_DEFINITE).is_empty()
        );
        assert!(run_rule("убавата, книгата", rule::DOUBLE_DEFINITE).is_empty());
    }

    #[test]
    fn stays_silent_on_unanalysed_words() {
        assert!(run("непознатата непознатата").is_empty());
    }

    #[test]
    fn finds_more_than_one_occurrence() {
        let found = run_rule("убавата книгата и новата куќата", rule::DOUBLE_DEFINITE);
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
        assert!(run_rule("тој ми го даде", rule::CLITIC_ORDER).is_empty());
        let found = run_rule("тој го ми даде", rule::CLITIC_ORDER);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::CLITIC_ORDER);
        assert_eq!(found[0].text, "го ми даде");
        assert_eq!(found[0].suggestions, vec!["ми го даде".to_string()]);
    }

    #[test]
    fn clitic_order_stays_silent_without_a_verb() {
        // No verb after the pair — not enough context to judge.
        assert!(run_rule("тој го ми", rule::CLITIC_ORDER).is_empty());
    }

    #[test]
    fn clitic_order_ignores_the_negation_particle() {
        // не is negation here, not an accusative clitic — must stay silent,
        // and must never propose the ungrammatical swap "им не остави".
        assert!(run_rule("не им остави", rule::CLITIC_ORDER).is_empty());
    }

    #[test]
    fn participle_ignores_object_clitics_as_subjects() {
        // го/ја/и are objects (accusative/dative), never subjects.
        assert!(run_rule("го водела", rule::L_PARTICIPLE).is_empty());
        assert!(run_rule("ја водела", rule::L_PARTICIPLE).is_empty());
    }

    #[test]
    fn flags_ne_fused_to_a_verb() {
        let found = run_rule("тој несака", rule::NE_FUSED);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::NE_FUSED);
        assert_eq!(found[0].text, "несака");
        assert_eq!(found[0].suggestions, vec!["не сака".to_string()]);
    }

    #[test]
    fn ne_ignores_established_words() {
        assert!(run_rule("тој непријател", rule::NE_FUSED).is_empty());
        assert!(run_rule("тој нестане", rule::NE_FUSED).is_empty());
        assert!(run_rule("немој вака", rule::NE_FUSED).is_empty());
    }

    #[test]
    fn ne_ignores_nonfinite_stems() {
        // Gerunds and participles take не- fused (§187).
        assert!(run_rule("тој несакајќи", rule::NE_FUSED).is_empty());
        assert!(run_rule("тој ненапишан", rule::NE_FUSED).is_empty());
    }

    #[test]
    fn ne_ignores_stems_with_content_word_readings() {
        // прав is also an adjective (неправ = unjust), so silence wins.
        assert!(run_rule("тој неправи", rule::NE_FUSED).is_empty());
    }

    #[test]
    fn ne_ignores_aorist_only_stems() {
        // Aorist-1sg stems double as -ив adjectives (необјаснив).
        assert!(run_rule("тој необјаснив", rule::NE_FUSED).is_empty());
    }

    #[test]
    fn ne_ignores_lexicalized_fusions() {
        // нестане (vanish) and непогоди (disasters) collide with negation.
        assert!(run_rule("тој нестане", rule::NE_FUSED).is_empty());
        assert!(run_rule("непогоди", rule::NE_FUSED).is_empty());
    }

    #[test]
    fn flags_naj_split_from_an_adjective() {
        let found = run("нај добар");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::NAJ_SEPARATED);
        assert_eq!(found[0].suggestions, vec!["најдобар".to_string()]);
    }

    #[test]
    fn naj_ignores_unverifiable_fusions() {
        assert!(run("нај книга").is_empty());
    }

    #[test]
    fn flags_bare_i_in_a_dative_slot() {
        // таа ѝ го даде ✓ silent; таа и го даде ✗ fires with ѝ.
        assert!(run_rule("таа ѝ го даде", rule::DATIVE_I).is_empty());
        let found = run_rule("таа и го даде", rule::DATIVE_I);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::DATIVE_I);
        assert_eq!(found[0].text, "и");
        assert_eq!(found[0].suggestions, vec!["ѝ".to_string()]);
    }

    #[test]
    fn dative_i_stays_silent_after_a_verb() {
        // "пее и го гледа" — и here is the conjunction, not the clitic.
        assert!(run_rule("таа пее и го даде", rule::DATIVE_I).is_empty());
    }

    #[test]
    fn flags_wrong_participle_gender() {
        assert!(run_rule("таа дошла", rule::L_PARTICIPLE).is_empty());
        assert!(run_rule("тој дошол", rule::L_PARTICIPLE).is_empty());
        assert!(run_rule("тие дошле", rule::L_PARTICIPLE).is_empty());
        let found = run_rule("таа дошол", rule::L_PARTICIPLE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::L_PARTICIPLE);
        assert_eq!(found[0].suggestions, vec!["таа дошла".to_string()]);
    }

    #[test]
    fn flags_wrong_participle_number_without_a_guess_it_cannot_make() {
        // тоа + дошол: neuter subject, masculine participle — fires, and no
        // neuter form is stored, so there is no suggestion to offer.
        let found = run_rule("тоа дошол", rule::L_PARTICIPLE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::L_PARTICIPLE);
        assert!(found[0].suggestions.is_empty(), "{:?}", found[0].suggestions);
    }

    #[test]
    fn flags_po_split_from_an_adjective() {
        let found = run("Тој е по добар");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::PO_SEPARATED);
        assert_eq!(found[0].suggestions, vec!["подобар".to_string()]);
    }

    #[test]
    fn po_before_a_plain_noun_stays_silent() {
        assert!(run("Тој оди по пат").is_empty());
    }

    #[test]
    fn flags_lowercase_after_a_full_stop() {
        let found = run("Тој дојде. утре ќе врне.");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::SENTENCE_CAPITAL);
        assert_eq!(found[0].text, "утре");
        assert_eq!(found[0].suggestions, vec!["Утре".to_string()]);
    }

    #[test]
    fn ignores_abbreviation_dots() {
        assert!(run("Тоа е, т.е. нешто друго.").is_empty());
        assert!(run("Се виде со г. Петров вчера.").is_empty());
    }

    #[test]
    fn only_uppercases_never_lowercases() {
        assert!(run("Тој рече: оди си дома.").is_empty());
        assert!(run("Утре ќе врне.").is_empty());
    }
}
