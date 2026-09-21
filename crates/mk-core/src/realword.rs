//! Real-word detection: the word is in the lexicon, so every other rule in the
//! engine is blind to it, but the two contexts it sits between reject it while
//! one near-twin explains both.
//!
//! This is the only rule whose evidence is statistical rather than categorical.
//! The bigram table is pruned, so a missing pair on its own means nothing —
//! which is why the rule demands the pair be missing on *both* sides and a
//! single replacement be attested on *both*. Correct prose almost never
//! satisfies that conjunction; we would rather miss an error than invent one.

use crate::bigram::Bigram;
use crate::frequency::Frequency;
use crate::lexicon::Lexicon;

/// Below this the word is too rare for its absence from the table to mean
/// anything — the table is pruned to the most frequent pairs.
pub const MIN_WORD_FREQ: u32 = 200;
/// Neighbours must themselves be common, otherwise an absent pair is just an
/// unremarkable word in an unremarkable sentence.
pub const MIN_NEIGHBOUR_FREQ: u32 = 500;
/// Each side's attestation for the replacement must reach this. With the
/// shipped table (min count 125) this is effectively "attested at all"; it is
/// a floor rather than a ratio because the missing side is always 0.
pub const MIN_ALT_COUNT: u32 = 30;
/// Short words collide with too many neighbours to be judged this way.
pub const MIN_CHARS: usize = 4;
/// FST candidates considered per word. The search only runs once every cheap
/// guard above has already passed, so this is not on the hot path.
const CANDIDATES: usize = 8;

/// Judges a word by its neighbours. Borrows the optional tables; a caller
/// without them simply does not build one.
pub struct RealWord<'a> {
    lexicon: &'a Lexicon,
    frequency: &'a Frequency,
    bigram: &'a Bigram,
}

impl<'a> RealWord<'a> {
    pub fn new(lexicon: &'a Lexicon, frequency: &'a Frequency, bigram: &'a Bigram) -> Self {
        Self { lexicon, frequency, bigram }
    }

    /// The near-twin that fits `prev _ next`, when this word does not.
    ///
    /// `word` is expected to be a known word; an unknown one is `MK_SPELL`'s
    /// business, not this rule's.
    // ponytail: thresholds are a dial, not a truth — re-calibrate against the
    // corpus gate (tools/eval_context.py) before loosening either of them.
    pub fn alternative(&self, word: &str, prev: &str, next: &str) -> Option<String> {
        let w = word.to_lowercase();
        let p = prev.to_lowercase();
        let n = next.to_lowercase();

        if w.chars().count() < MIN_CHARS
            || self.frequency.get(&w) < MIN_WORD_FREQ
            || self.frequency.get(&p) < MIN_NEIGHBOUR_FREQ
            || self.frequency.get(&n) < MIN_NEIGHBOUR_FREQ
        {
            return None;
        }
        // Both sides must reject the word as written.
        if self.bigram.get(&p, &w) > 0 || self.bigram.get(&w, &n) > 0 {
            return None;
        }

        let mut best: Option<(String, u32)> = None;
        for cand in self.lexicon.suggest(&w, CANDIDATES) {
            let left = self.bigram.get(&p, &cand);
            let right = self.bigram.get(&cand, &n);
            if left < MIN_ALT_COUNT || right < MIN_ALT_COUNT {
                continue;
            }
            // Weakest link decides: the twin has to fit on both sides.
            let score = left.min(right);
            if best.as_ref().map_or(true, |(_, s)| score > *s) {
                best = Some((cand, score));
            }
        }
        best.map(|(cand, _)| cand)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon(words: &[&str]) -> Lexicon {
        let bytes = Lexicon::build_from_unsorted(words.iter().copied()).unwrap();
        Lexicon::from_bytes(bytes).unwrap()
    }

    fn freq(entries: &[(&str, u32)]) -> Frequency {
        Frequency::from_bytes(&Frequency::build(entries).unwrap()).unwrap()
    }

    fn bigrams(entries: &[(&str, &str, u32)]) -> Bigram {
        Bigram::from_bytes(&Bigram::build(entries).unwrap()).unwrap()
    }

    /// A sentence with one plausible slip: `ја види книгата` for `ја виде`.
    fn slip() -> (Lexicon, Frequency, Bigram) {
        (
            lexicon(&["тој", "ја", "виде", "види", "книгата", "масата", "на"]),
            freq(&[
                ("тој", 5000),
                ("ја", 4000),
                ("виде", 3000),
                ("види", 800),
                ("книгата", 600),
                ("масата", 700),
                ("на", 20000),
            ]),
            bigrams(&[("тој", "ја", 900), ("ја", "виде", 700), ("виде", "книгата", 400)]),
        )
    }

    #[test]
    fn a_real_word_rejected_by_both_sides_yields_its_twin() {
        let (l, f, b) = slip();
        let rw = RealWord::new(&l, &f, &b);
        assert_eq!(rw.alternative("види", "ја", "книгата").as_deref(), Some("виде"));
    }

    #[test]
    fn the_correct_word_is_left_alone() {
        let (l, f, b) = slip();
        let rw = RealWord::new(&l, &f, &b);
        assert_eq!(rw.alternative("виде", "ја", "книгата"), None);
    }

    #[test]
    fn one_attested_side_is_enough_to_stay_silent() {
        // (left attested, right not)
        let (l, f, _) = slip();
        let b = bigrams(&[("ја", "види", 5), ("виде", "книгата", 400)]);
        assert_eq!(RealWord::new(&l, &f, &b).alternative("види", "ја", "книгата"), None);

        // (right attested, left not)
        let b = bigrams(&[("ја", "виде", 700), ("види", "книгата", 5)]);
        assert_eq!(RealWord::new(&l, &f, &b).alternative("види", "ја", "книгата"), None);
    }

    #[test]
    fn a_twin_must_fit_both_sides_too() {
        // `виде` stays unattested on the right, so nothing explains the context.
        let (l, f, _) = slip();
        let b = bigrams(&[("ја", "виде", 700)]);
        assert_eq!(RealWord::new(&l, &f, &b).alternative("види", "ја", "книгата"), None);
    }

    #[test]
    fn rare_words_are_not_judged() {
        let (l, _, b) = slip();
        // The word itself is too rare for its absence to mean anything.
        let f = freq(&[("тој", 5000), ("ја", 4000), ("види", 50), ("книгата", 600)]);
        assert_eq!(RealWord::new(&l, &f, &b).alternative("види", "ја", "книгата"), None);

        // ...and a rare neighbour makes the whole context uninformative.
        let f = freq(&[("тој", 5000), ("ја", 4000), ("види", 800), ("книгата", 100)]);
        assert_eq!(RealWord::new(&l, &f, &b).alternative("види", "ја", "книгата"), None);
    }

    #[test]
    fn short_words_are_not_judged() {
        let (l, f, b) = slip();
        assert_eq!(RealWord::new(&l, &f, &b).alternative("ја", "тој", "виде"), None);
    }

    #[test]
    fn the_stronger_twin_wins() {
        // Two candidates fit; the one attested more weakly on its worst side
        // loses, regardless of the order the lexicon search yields them.
        let l = lexicon(&["масата", "маса", "маската"]);
        let f = freq(&[("на", 20000), ("нова", 900), ("масата", 800), ("маса", 600), ("маската", 700)]);
        let b = bigrams(&[("на", "маса", 100), ("маса", "нова", 50), ("на", "маската", 300), ("маската", "нова", 200)]);
        let rw = RealWord::new(&l, &f, &b);
        assert_eq!(rw.alternative("масата", "на", "нова").as_deref(), Some("маската"));
    }

    #[test]
    fn an_unknown_word_is_not_this_rules_business() {
        let (l, f, b) = slip();
        assert_eq!(RealWord::new(&l, &f, &b).alternative("видди", "ја", "книгата"), None);
    }
}
