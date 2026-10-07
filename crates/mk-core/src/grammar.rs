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
    space_before_punct(tokens, &mut out);
    adjective_agreement(tokens, morph, &mut out);
    verb_agreement(tokens, morph, &mut out);
    numeral_noun(tokens, morph, &mut out);
    object_doubling(tokens, morph, &mut out);
    out
}

/// A cardinal numeral and the plural noun it counts.
///
/// ```text
/// два стола, две маси    ✓   два маси, две града   ✗   (MK_NUMERAL_GENDER)
/// два града, пет дена    ✓   два градови           ✗   (MK_COUNT_FORM)
/// ```
///
/// Gender only for `два`/`две` against an unambiguously masculine or feminine
/// noun; neuter is left alone. The count form is proposed only when it is a
/// stored plural of the same lemma (`градови` → `града`), so it is never
/// invented — and person nouns, which keep the ordinary plural, have no such
/// stored form.
fn numeral_noun(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for pair in tokens.windows(2) {
        let (num, noun) = (&pair[0], &pair[1]);
        if num.kind != TokenKind::Word || noun.kind != TokenKind::Word {
            continue;
        }
        // A cardinal has no number of its own (ordinals carry sg/pl), and the
        // personal `двајца` and approximate `десетина` take the plain plural.
        let num_readings = morph.analyze(num.text);
        let cardinal = !num_readings.is_empty()
            && num_readings.iter().all(|a| {
                a.pos() == Some(Pos::Numeral) && a.number().is_none() && !a.has("ma") && !a.has("apprx")
            });
        if !cardinal {
            continue;
        }
        let noun_readings = morph.analyze(noun.text);
        if noun_readings.is_empty()
            || !noun_readings.iter().all(|a| {
                a.pos() == Some(Pos::Noun) && a.number() == Some(Number::Plural) && !a.is_definite()
            })
        {
            continue;
        }
        let all_gender = |g: Gender| noun_readings.iter().all(|a| a.gender() == Some(g));
        let masculine = all_gender(Gender::Masculine);

        let swapped = match num.text.to_lowercase().as_str() {
            "два" if all_gender(Gender::Feminine) => Some("две"),
            "две" if masculine => Some("два"),
            _ => None,
        };
        if let Some(fixed) = swapped {
            out.push(Diagnostic {
                rule: rule::NUMERAL_GENDER.to_string(),
                severity: Severity::Error,
                char_start: num.char_start,
                char_end: noun.char_end,
                text: format!("{} {}", num.text, noun.text),
                message: "Два се употребува со машки род, две со женски.".to_string(),
                suggestions: vec![format!("{} {}", match_case(num.text, fixed), noun.text)],
            });
            continue;
        }
        if !masculine {
            continue;
        }
        if let Some(count) = count_form(noun.text, &noun_readings, morph) {
            out.push(Diagnostic {
                rule: rule::COUNT_FORM.to_string(),
                severity: Severity::Warning,
                char_start: num.char_start,
                char_end: noun.char_end,
                text: format!("{} {}", num.text, noun.text),
                message: "По број, именките од машки род го земаат бројниот облик.".to_string(),
                suggestions: vec![format!("{} {}", num.text, match_case(noun.text, &count))],
            });
        }
    }
}

/// The stored count form of a masculine `-ови`/`-еви` plural: `градови` → `града`.
fn count_form(word: &str, readings: &[Analysis<'_>], morph: &Morphology) -> Option<String> {
    let lower = word.to_lowercase();
    let stem = lower.strip_suffix("ови").or_else(|| lower.strip_suffix("еви"))?;
    let candidate = format!("{stem}а");
    let same_lemma = morph.analyze(&candidate).iter().any(|c| {
        c.pos() == Some(Pos::Noun)
            && c.number() == Some(Number::Plural)
            && readings.iter().any(|r| r.lemma() == c.lemma())
    });
    same_lemma.then_some(candidate)
}

/// Nouns of time that stand after a verb as adverbials, not objects:
/// `Работев ноќта`, `Спиев утрото`. Lemmas.
const TIME_NOUNS: &[&str] = &[
    "ден", "ноќ", "утро", "вечер", "пладне", "попладне", "недела", "седмица", "месец",
    "година", "лето", "зима", "пролет", "есен", "викенд", "век", "време", "час", "минута",
    "секунда", "понеделник", "вторник", "среда", "четврток", "петок", "сабота", "празник",
    "сезона", "пат",
];

/// A definite direct object is doubled by a clitic: `Ја видов книгата` ✓,
/// `Видов книгата` ✗.
///
/// Only after a first- or second-person transitive verb. With a third-person
/// verb the definite noun after it may be the subject (`Така рече човекот`),
/// and nothing short of a parser can tell; with first or second person it
/// cannot be, because a subject would have to agree.
fn object_doubling(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for (i, pair) in tokens.windows(2).enumerate() {
        let (verb, object) = (&pair[0], &pair[1]);
        if verb.kind != TokenKind::Word || object.kind != TokenKind::Word {
            continue;
        }
        let verb_readings = morph.analyze(verb.text);
        let verbs: Vec<&Analysis<'_>> =
            verb_readings.iter().filter(|a| a.pos() == Some(Pos::Verb)).collect();
        if verbs.is_empty()
            || !verbs.iter().any(|a| a.has("tv"))
            || !verbs.iter().all(|a| person_of(a).is_some_and(|p| p < 3))
        {
            continue;
        }
        let object_readings = morph.analyze(object.text);
        if object_readings.is_empty()
            || !object_readings.iter().all(|a| a.pos() == Some(Pos::Noun) && a.is_definite())
            || object_readings.iter().any(|a| TIME_NOUNS.contains(&a.lemma()))
        {
            continue;
        }
        let Some(clitic) = doubling_clitic(&object_readings) else {
            continue;
        };
        // `Сакам децата да учат`: the noun is the subject of the `да` clause.
        if tokens.get(i + 2).is_some_and(|t| t.text.eq_ignore_ascii_case("да")) {
            continue;
        }
        if has_accusative_clitic_before(tokens, i, morph) {
            continue;
        }
        let fix = if verb.text.starts_with(char::is_uppercase) {
            format!("{} {} {}", capitalize_first(clitic), verb.text.to_lowercase(), object.text)
        } else {
            format!("{} {} {}", clitic, verb.text, object.text)
        };
        out.push(Diagnostic {
            rule: rule::OBJECT_DOUBLING.to_string(),
            severity: Severity::Warning,
            char_start: verb.char_start,
            char_end: object.char_end,
            text: format!("{} {}", verb.text, object.text),
            message: "Определениот директен предмет се удвојува со кратка заменка: ја видов книгата."
                .to_string(),
            suggestions: vec![fix],
        });
    }
}

/// `го`, `ја` or `ги` for the object, when every reading agrees on one.
fn doubling_clitic(readings: &[Analysis<'_>]) -> Option<&'static str> {
    let mut found = None;
    for a in readings {
        let clitic = match (a.number()?, a.gender()?) {
            (Number::Plural, _) => "ги",
            (Number::Singular, Gender::Feminine) => "ја",
            (Number::Singular, Gender::Masculine | Gender::Neuter) => "го",
            _ => return None,
        };
        if found.is_some_and(|f| f != clitic) {
            return None;
        }
        found = Some(clitic);
    }
    found
}

/// Is there an accusative clitic in the cluster right before the verb at `i`?
/// Steps back over clitics, `не`, `ќе` and `да`, at most three words.
fn has_accusative_clitic_before(tokens: &[Token<'_>], i: usize, morph: &Morphology) -> bool {
    for t in tokens[..i].iter().rev().take(3) {
        if t.kind != TokenKind::Word {
            return false;
        }
        let lower = t.text.to_lowercase();
        if matches!(lower.as_str(), "не" | "ќе" | "да") {
            continue;
        }
        let readings = morph.analyze(t.text);
        if readings.iter().any(|a| a.has("clt") && a.has("acc")) {
            return true;
        }
        if !readings.iter().any(|a| a.has("clt") || a.has("ref")) {
            return false;
        }
    }
    false
}

/// `replacement`, capitalised the way `original` is.
pub(crate) fn match_case(original: &str, replacement: &str) -> String {
    if original.starts_with(char::is_uppercase) {
        capitalize_first(replacement)
    } else {
        replacement.to_string()
    }
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
    for (i, pair) in tokens.windows(2).enumerate() {
        let (first, second) = (&pair[0], &pair[1]);
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }
        if first.text.to_lowercase() != "по" {
            continue;
        }
        let ok_pos = morph.analyze(second.text).iter().any(|a| {
            matches!(a.pos(), Some(Pos::Adjective | Pos::Verb))
        });
        if !ok_pos {
            continue;
        }
        // Definite-marked seconds are prepositional or ambiguous
        // (`по старите улици` could be `постарите`, `по добриот` could be
        // `подобриот`): silence wins over guessing.
        if morph.analyze(second.text).iter().any(|a| a.is_definite()) {
            continue;
        }
        // `по X Y-def` governs a phrase (`По успешно спроведениот референдум`).
        if let Some(third) = tokens.get(i + 2) {
            if third.kind == TokenKind::Word
                && morph.analyze(third.text).iter().any(|a| {
                    matches!(a.pos(), Some(Pos::Adjective | Pos::Noun)) && a.is_definite()
                })
            {
                continue;
            }
        }
        // `професор по X`: noun-governed prepositional complement.
        if i >= 1 {
            let prev = &tokens[i - 1];
            if prev.kind == TokenKind::Word
                && morph.analyze(prev.text).iter().any(|a| a.pos() == Some(Pos::Noun))
            {
                continue;
            }
        }
        // Attributive comparatives fuse in standard language, so `по Adj Noun`
        // (`по железнички пат`, `по стар пат`) is a prepositional phrase,
        // never a split comparative: silence wins.
        if let Some(next) = tokens.get(i + 2) {
            if next.kind == TokenKind::Word
                && morph.analyze(next.text).iter().any(|a| a.pos() == Some(Pos::Noun))
            {
                continue;
            }
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
        if after_ender && !at_start && i >= 2 {
            // Multi-letter abbreviations: итн. др. сл. пр. тн.
            const ABBREV: &[&str] = &["итн", "др", "сл", "пр", "тн"];
            if tokens[i - 2].kind == TokenKind::Word
                && ABBREV.contains(&tokens[i - 2].text.to_lowercase().as_str())
            {
                continue;
            }
            // Number-dots are ordinals/lists: 137. стоеше.
            if tokens[i - 2].kind == TokenKind::Number {
                continue;
            }
            // Ellipsis right after an opening quote: „…X or „...X. Each dot
            // is its own token, so walk back over the dot-run to the opener.
            if tokens[i - 1].text.chars().all(|c| c == '.' || c == '…') {
                let mut j = i - 1;
                while j > 0
                    && tokens[j].kind == TokenKind::Punct
                    && tokens[j].text.chars().all(|c| c == '.' || c == '…')
                {
                    j -= 1;
                }
                if tokens[j].kind == TokenKind::Punct
                    && ["„", "\"", "«", "'", "‘"].contains(&tokens[j].text)
                {
                    continue;
                }
            }
            // URLs glued both sides: град.ск (dot byte-adjacent left AND
            // (byte-adjacent right OR next word ≤ 3 chars)).
            // Normal "реченица. Утре" has a gap on the right and a long next word.
            let ender = &tokens[i - 1];
            let left_glued = tokens[i - 2].byte_end == ender.byte_start;
            let right_glued = ender.byte_end == t.byte_start;
            if left_glued && (right_glued || t.text.chars().count() <= 3) {
                continue;
            }
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

fn space_before_punct(tokens: &[Token<'_>], out: &mut Vec<Diagnostic>) {
    const CLOSERS: &[&str] = &[",", ".", "!", "?", ":", ";", "…", "”", "’", ")"];
    for i in 0..tokens.len().saturating_sub(1) {
        let (prev, curr) = (&tokens[i], &tokens[i + 1]);
        if curr.kind != TokenKind::Punct || !CLOSERS.contains(&curr.text) {
            continue;
        }
        if !matches!(prev.kind, TokenKind::Word | TokenKind::Number) {
            continue;
        }
        if curr.byte_start == prev.byte_end {
            continue;
        }
        // A colon/semicolon glued to following punctuation opens an emoticon
        // (:-) ;-) :)) — not closing punctuation, so the gap before it is fine.
        if (curr.text == ":" || curr.text == ";")
            && tokens.get(i + 2).is_some_and(|n| {
                n.kind == TokenKind::Punct && n.byte_start == curr.byte_end
            })
        {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::SPACE_BEFORE_PUNCT.to_string(),
            severity: Severity::Warning,
            char_start: prev.char_end,
            char_end: curr.char_end,
            text: format!(" {}", curr.text),
            message: "Нема белина пред интерпункциски знак.".to_string(),
            suggestions: vec![curr.text.to_string()],
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
/// An attributive adjective must agree with its noun in gender and number.
///
/// ```text
/// убава книга    ✓   убаво книга    ✗
/// голем човек    ✓   голема човек   ✗
/// ```
///
/// Both words must be unambiguous: if any reading of the adjective or the noun
/// lacks a gender or number, the phrase cannot be read and the rule stays
/// silent. A single agreeing reading is enough to save the pair, so homographs
/// that happen to mismatch are never reported.
fn adjective_agreement(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for i in 0..tokens.len().saturating_sub(1) {
        let (first, second) = (&tokens[i], &tokens[i + 1]);
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }
        let (adj_readings, noun_readings) = (morph.analyze(first.text), morph.analyze(second.text));
        // A word that is also a noun or a verb takes its other reading here
        // (`цел освојување`, `прави разлика`), so only an unambiguous
        // adjective can be judged against a noun.
        if !adj_readings.iter().all(|a| a.pos() == Some(Pos::Adjective)) {
            continue;
        }
        let (Some(adj), Some(noun)) = (
            agree_forms(&adj_readings, Pos::Adjective),
            agree_forms(&noun_readings, Pos::Noun),
        ) else {
            continue;
        };
        // A noun directly followed by another noun is a modifier, not the head
        // of the phrase: in `достапни интернет услуги` the adjective agrees
        // with `услуги`.
        if let Some(after) = tokens.get(i + 2) {
            if after.kind == TokenKind::Word
                && morph.analyze(after.text).iter().any(|a| a.pos() == Some(Pos::Noun))
            {
                continue;
            }
        }
        // A capitalised adjective mid-sentence is part of a name
        // (`Александар Велики`), and a sentence-initial one cannot be told from
        // an adverb without a real sentence segmenter (`Географски, земјата…`).
        // Both would be guesses, so neither is judged.
        if first.text.chars().next().is_some_and(char::is_uppercase) {
            continue;
        }
        let agrees = adj
            .iter()
            .any(|(ag, an)| noun.iter().any(|(ng, nn)| compatible(*ag, *ng) && an == nn));
        if agrees {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::ADJ_AGREEMENT.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: second.char_end,
            text: format!("{} {}", first.text, second.text),
            message: "Придавката мора да се сложува со именката во род и број.".to_string(),
            // No reverse index lemma → adjective forms exists, and inventing
            // the agreeing surface form would guess at a paradigm. The card
            // still names the rule and explains the mismatch.
            suggestions: Vec::new(),
        });
    }
}

/// Gender and number of every `pos` reading of a word.
///
/// `None` when there is no such reading, or when any one of them is missing a
/// feature — an under-specified word is not evidence of disagreement.
fn agree_forms(analyses: &[Analysis<'_>], pos: Pos) -> Option<Vec<(Gender, Number)>> {
    let matching: Vec<&Analysis<'_>> = analyses.iter().filter(|a| a.pos() == Some(pos)).collect();
    if matching.is_empty() {
        return None;
    }
    let mut forms = Vec::with_capacity(matching.len());
    for a in matching {
        let (gender, number) = (a.gender()?, a.number()?);
        // The count form follows a numeral (`два стола`); nothing attributive
        // agrees with it, so it is not evidence either way.
        if number == Number::Count {
            return None;
        }
        forms.push((gender, number));
    }
    Some(forms)
}

/// A finite verb must agree with its subject in person and number.
///
/// ```text
/// тој сака    ✓   тој сакаат    ✗
/// ние одиме   ✓   ние одиш      ✗
/// ```
///
/// The subject must be a nominative personal pronoun, and clitics and `не` are
/// stepped over because they sit between subject and verb in ordinary prose
/// (`тој ја виде`, `тој не дојде`). Neighbouring tokens were tried first, but a
/// clitic phrase like `тој ја виде` is the common case, not the exception.
fn verb_agreement(tokens: &[Token<'_>], morph: &Morphology, out: &mut Vec<Diagnostic>) {
    for (i, subject) in tokens.iter().enumerate() {
        if subject.kind != TokenKind::Word {
            continue;
        }
        let Some((person, number)) = subject_person(morph, subject.text) else {
            continue;
        };

        // At most two intervening clitics or a negation: `тој ми го даде`.
        let mut j = i + 1;
        let mut skipped = 0;
        while j < tokens.len() && skipped < 2 && skippable(morph, &tokens[j]) {
            j += 1;
            skipped += 1;
        }
        let Some(verb) = tokens.get(j) else { continue };
        if verb.kind != TokenKind::Word {
            continue;
        }

        // Only readings that actually carry person and number can disagree.
        // A form whose only reading is imperative or participle (`дојди`,
        // `дошол`) says nothing about the subject, so it is left alone.
        let readings = morph.analyze(verb.text);
        let forms: Vec<(u8, Number)> = readings
            .iter()
            .filter(|a| a.pos() == Some(Pos::Verb))
            .filter_map(|a| Some((person_of(a)?, a.number()?)))
            .collect();
        if forms.is_empty() || forms.contains(&(person, number)) {
            continue;
        }

        out.push(Diagnostic {
            rule: rule::VERB_AGREEMENT.to_string(),
            severity: Severity::Error,
            char_start: subject.char_start,
            char_end: verb.char_end,
            text: format!("{} {}", subject.text, verb.text),
            message: "Глаголот мора да се сложува со подметот во лице и број.".to_string(),
            suggestions: Vec::new(),
        });
    }
}

/// Person and number of a nominative personal pronoun, or `None`.
///
/// A form with any clitic reading is excluded, and a pronoun whose readings
/// disagree about its own person or number is not a subject we can judge.
fn subject_person(morph: &Morphology, form: &str) -> Option<(u8, Number)> {
    let readings = morph.analyze(form);
    // `ти` is also the dative clitic: in `Ти дадов писмото` the subject is
    // `јас`, not `ти`, and nothing local tells the two apart.
    if readings.iter().any(|a| a.has("clt")) {
        return None;
    }
    let mut found: Option<(u8, Number)> = None;
    for a in readings {
        if a.pos() != Some(Pos::Pronoun) || !a.has("nom") {
            continue;
        }
        let (Some(person), Some(number)) = (person_of(&a), a.number()) else {
            continue;
        };
        match found {
            Some(previous) if previous != (person, number) => return None,
            _ => found = Some((person, number)),
        }
    }
    found
}

/// Apertium verbs and pronouns carry `p1`/`p2`/`p3`.
fn person_of(a: &Analysis<'_>) -> Option<u8> {
    ["p1", "p2", "p3"].iter().position(|t| a.has(t)).map(|i| i as u8 + 1)
}

/// A token that sits between a subject and its verb without changing who the
/// subject is: a clitic pronoun (`ја`, `му`, `се`) or the negation.
///
/// Deliberately excludes `да` and `ќе`: `тој рече да одиме` is correct, and
/// stepping over `да` would read `одиме` as agreeing with `тој`.
fn skippable(morph: &Morphology, token: &Token<'_>) -> bool {
    if token.kind != TokenKind::Word {
        return false;
    }
    if token.text.eq_ignore_ascii_case("не") {
        return true;
    }
    let readings = morph.analyze(token.text);
    // `clt` marks the ordinary clitics (`ја`, `го`, `му`); the reflexives `се`
    // and `си` are tagged `ref` instead, and both also carry an unrelated verb
    // reading (`се` lists a spurious `vbser` plural), so "all readings are
    // clitics" would never hold for them.
    !readings.is_empty() && readings.iter().any(|a| a.has("clt") || a.has("ref"))
}

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
        add("стар", "стар", &["adj", "m", "sg", "nom", "ind"]);
        // precision-guard probes: definiteness + POS of по-frames
        add("успешно", "успешен", &["adj", "nt", "sg", "nom", "ind"]);        add("успешно", "успешно", &["adv"]);
        // agreement: a neuter adjective before a feminine noun, a plural verb
        // after a singular subject, and two precision probes
        add("убаво", "убав", &["adj", "nt", "sg", "nom", "ind"]);
        add("сакаат", "сака", &["vblex", "impf", "tv", "pres", "p3", "pl"]);
        add("секаков", "секаков", &["adj", "m"]);
        add("они", "они", &["prn", "pers", "p3", "mfn", "pl", "nom"]);
        add("они", "они", &["prn", "pers", "p2", "mfn", "pl", "nom"]);
        add("спроведениот", "спроведен", &["adj", "m", "sg", "nom", "def"]);
        add("катастрофалниот", "катастрофален", &["adj", "m", "sg", "nom", "def"]);
        add("уставно", "уставен", &["adj", "nt", "sg", "nom", "ind"]);
        add("железнички", "железнички", &["adj", "mfn", "pl", "nom", "ind"]);
        // -ски adjectives are syncretic between masculine singular and plural
        // (real Apertium lists both readings). Without the singular reading the
        // fixture models the word wrongly and the agreement rule reads
        // "железнички пат" as a number mismatch.
        add("железнички", "железнички", &["adj", "m", "sg", "nom", "ind"]);
        add("професор", "професор", &["n", "m", "sg", "nom", "ind"]);
        add("право", "право", &["n", "nt", "sg", "nom", "ind"]);
        add("референдум", "референдум", &["n", "m", "sg", "nom", "ind"]);
        add("земјотрес", "земјотрес", &["n", "m", "sg", "nom", "ind"]);
        // numerals: cardinals carry no number of their own; ordinals do
        add("два", "два", &["num", "m", "nom", "ind"]);
        add("два", "два", &["num", "nt", "nom", "ind"]);
        add("две", "два", &["num", "f", "nom", "ind"]);
        add("пет", "пет", &["num", "mfn", "nom", "ind"]);
        add("двајца", "два", &["num", "ma", "nom", "ind"]);
        add("втори", "втор", &["num", "mfn", "pl", "nom", "ind"]);
        add("градови", "град", &["n", "m", "pl", "nom", "ind"]);
        add("града", "град", &["n", "m", "pl", "nom", "ind"]);
        add("града", "града", &["n", "f", "sg", "nom", "ind"]);
        add("ученици", "ученик", &["n", "m", "pl", "nom", "ind"]);
        add("села", "село", &["n", "nt", "pl", "nom", "ind"]);
        // object doubling: first/third-person verbs, definite objects
        add("видов", "вид", &["n", "m", "sg", "nom", "prx"]);
        add("видов", "види", &["vblex", "perf", "tv", "aor", "p1", "sg"]);
        add("видам", "види", &["vblex", "perf", "tv", "pres", "p1", "sg"]);
        add("сакам", "сака", &["vblex", "impf", "tv", "pres", "p1", "sg"]);
        add("спијам", "спие", &["vblex", "impf", "iv", "pres", "p1", "sg"]);
        add("рече", "рече", &["vblex", "perf", "tv", "aor", "p3", "sg"]);
        add("филмот", "филм", &["n", "m", "sg", "nom", "def"]);
        add("децата", "дете", &["n", "nt", "pl", "nom", "def"]);
        add("ноќта", "ноќ", &["n", "f", "sg", "nom", "def"]);
        add("човекот", "човек", &["n", "m", "sg", "nom", "def"]);
        add("да", "да", &["part"]);
        add("учат", "учи", &["vblex", "impf", "tv", "pres", "p3", "pl"]);
        add("ти", "clitic", &["prn", "pers", "clt", "p2", "mfn", "sg", "dat"]);
        add("ти", "free", &["prn", "pers", "p2", "mfn", "sg", "nom"]);
        add("дадов", "даде", &["vblex", "perf", "tv", "aor", "p1", "sg"]);
        add("писмото", "писмо", &["n", "nt", "sg", "nom", "def"]);
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
                    "непријател", "нестане", "најдобар", "подобар", "утре", "постар",
                    "па", "ск", "град", "дома", "рече", "обединувајќи", "ги",
                    "сите", "точка", "стоеше", "сам", "продолжи", "итн",
                    "поуспешно",
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
    fn flags_an_adjective_disagreeing_with_its_noun() {
        let found = run_rule("убаво книга", rule::ADJ_AGREEMENT);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].text, "убаво книга");
    }

    #[test]
    fn agreeing_adjectives_are_left_alone() {
        assert!(run_rule("убава книга", rule::ADJ_AGREEMENT).is_empty());
        assert!(run_rule("убавата книга", rule::ADJ_AGREEMENT).is_empty());
        assert!(run_rule("добар град", rule::ADJ_AGREEMENT).is_empty());
    }

    #[test]
    fn an_adjective_missing_gender_or_number_is_not_evidence() {
        // `секаков` has a masculine adjective reading but no number at all.
        assert!(run_rule("секаков книга", rule::ADJ_AGREEMENT).is_empty());
    }

    #[test]
    fn flags_a_verb_disagreeing_with_its_subject() {
        let found = run_rule("тој сакаат", rule::VERB_AGREEMENT);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::Error);
        assert_eq!(found[0].text, "тој сакаат");
    }

    #[test]
    fn agreeing_verbs_are_left_alone() {
        assert!(run_rule("тој сака", rule::VERB_AGREEMENT).is_empty());
        assert!(run_rule("тие сакаат", rule::VERB_AGREEMENT).is_empty());
        assert!(run_rule("таа даде", rule::VERB_AGREEMENT).is_empty());
    }

    #[test]
    fn clitics_and_negation_do_not_break_the_subject_chain() {
        // тој (3sg) + ја (clitic) + даде (3sg): agreement still holds.
        assert!(run_rule("тој ја даде", rule::VERB_AGREEMENT).is_empty());
        assert!(run_rule("тој не сака", rule::VERB_AGREEMENT).is_empty());
        // ...and a real mismatch is still caught through them.
        assert_eq!(run_rule("тој не сакаат", rule::VERB_AGREEMENT).len(), 1);
    }

    #[test]
    fn negation_is_not_mistaken_for_a_subject() {
        // `не` also reads as a first-person plural clitic. It has no `nom`
        // reading, so it must never anchor this rule.
        assert!(run_rule("не сака", rule::VERB_AGREEMENT).is_empty());
    }

    #[test]
    fn a_dative_clitic_is_not_mistaken_for_the_subject() {
        // `Ти дадов писмото` = I gave you the letter: `ти` is the clitic.
        assert!(run_rule("Ти дадов писмото.", rule::VERB_AGREEMENT).is_empty());
        // ...and the object still needs its own clitic.
        let found = run_rule("Ти дадов писмото.", rule::OBJECT_DOUBLING);
        assert_eq!(found[0].suggestions, vec!["го дадов писмото".to_string()]);
    }

    #[test]
    fn non_finite_verbs_are_not_judged() {
        // `прави` in its verb reading is an imperative: no person, no verdict.
        assert!(run_rule("тој прави", rule::VERB_AGREEMENT).is_empty());
    }

    #[test]
    fn an_ambiguous_subject_is_not_judged() {
        // `они` reads as both 3pl and 2pl, so nothing can be concluded.
        assert!(run_rule("они сака", rule::VERB_AGREEMENT).is_empty());
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
    fn po_ignores_attributive_adjective_noun() {
        // Attributive comparatives fuse; `по стар пат` is prepositional.
        assert!(run("Тој оди по стар пат").is_empty());
    }

    #[test]
    fn sentence_ignores_ascii_ellipsis_after_quote() {
        assert!(run("Тој рече „... обединувајќи ги сите.").is_empty());
    }

    #[test]
    fn po_ignores_definite_and_governed_frames() {
        assert!(run("По успешно спроведениот референдум").is_empty());
        assert!(run("по катастрофалниот земјотрес").is_empty());
        assert!(run("професор по уставно право").is_empty());
        assert!(run("по железнички пат").is_empty());
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

    #[test]
    fn sentence_ignores_non_ending_dots() {
        assert!(run("Тоа е, итн. па продолжи.").is_empty());
        assert!(run("Види град.ск дома.").is_empty());
        assert!(run("Тој рече „… обединувајќи ги сите.").is_empty());
        assert!(run("Точка 137. стоеше сам.").is_empty());
    }

    #[test]
    fn flags_space_before_a_comma() {
        let found = run("Тој дојде , а таа не.");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].rule, rule::SPACE_BEFORE_PUNCT);
        assert_eq!(found[0].suggestions, vec![",".to_string()]);
    }

    #[test]
    fn flags_dva_dve_against_the_nouns_gender() {
        let found = run_rule("има два книги", rule::NUMERAL_GENDER);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].suggestions, vec!["две книги".to_string()]);
        let found = run_rule("Две градови", rule::NUMERAL_GENDER);
        assert_eq!(found[0].suggestions, vec!["Два градови".to_string()]);
    }

    #[test]
    fn numeral_gender_leaves_agreeing_and_neuter_pairs_alone() {
        assert!(run_rule("две книги", rule::NUMERAL_GENDER).is_empty());
        assert!(run_rule("два града", rule::NUMERAL_GENDER).is_empty());
        // Neuter is not judged either way.
        assert!(run_rule("две села", rule::NUMERAL_GENDER).is_empty());
        assert!(run_rule("два села", rule::NUMERAL_GENDER).is_empty());
    }

    #[test]
    fn flags_a_plain_plural_where_the_count_form_is_stored() {
        let found = run_rule("пет градови", rule::COUNT_FORM);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].suggestions, vec!["пет града".to_string()]);
    }

    #[test]
    fn count_form_is_never_invented() {
        // No stored count form for person nouns, so nothing to propose.
        assert!(run_rule("пет ученици", rule::COUNT_FORM).is_empty());
        assert!(run_rule("пет града", rule::COUNT_FORM).is_empty());
        // Ordinals and the personal numeral are not cardinals.
        assert!(run_rule("втори градови", rule::COUNT_FORM).is_empty());
        assert!(run_rule("двајца ученици", rule::COUNT_FORM).is_empty());
    }

    #[test]
    fn flags_a_definite_object_without_its_clitic() {
        let found = run_rule("Видов книгата.", rule::OBJECT_DOUBLING);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].text, "Видов книгата");
        assert_eq!(found[0].suggestions, vec!["Ја видов книгата".to_string()]);
        let found = run_rule("тој рече да видам децата.", rule::OBJECT_DOUBLING);
        assert_eq!(found[0].suggestions, vec!["ги видам децата".to_string()]);
    }

    #[test]
    fn object_doubling_stays_silent_when_unsure() {
        // Already doubled, with or without negation between.
        assert!(run_rule("Ја видов книгата.", rule::OBJECT_DOUBLING).is_empty());
        assert!(run_rule("Не го видов филмот.", rule::OBJECT_DOUBLING).is_empty());
        // Third person: the noun may be the subject.
        assert!(run_rule("Така рече човекот.", rule::OBJECT_DOUBLING).is_empty());
        // Intransitive verb, time adverbial, indefinite object.
        assert!(run_rule("Спијам ноќта.", rule::OBJECT_DOUBLING).is_empty());
        assert!(run_rule("Сакам ноќта.", rule::OBJECT_DOUBLING).is_empty());
        assert!(run_rule("Видов книга.", rule::OBJECT_DOUBLING).is_empty());
        // The noun is the subject of a `да` clause.
        assert!(run_rule("Сакам децата да учат.", rule::OBJECT_DOUBLING).is_empty());
        // Punctuation between verb and noun.
        assert!(run_rule("Видов, книгата.", rule::OBJECT_DOUBLING).is_empty());
    }

    #[test]
    fn leaves_emoticons_and_abbreviations_alone() {
        assert!(run("Тој дојде :-)").is_empty());
        assert!(run("Се виде со г. Петров.").is_empty());
    }
}
