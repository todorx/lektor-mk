//! The word lexicon, stored as a finite-state transducer.
//!
//! Macedonian inflects heavily — a single noun carries the definite article as a
//! suffix in three deictic series (`книга, книгата, книгава, книгана`) on top of
//! number and case-like forms — so the surface-form list is large and highly
//! prefix-redundant. An FST shares those prefixes and suffixes, which keeps the
//! whole lexicon small enough to ship inside a browser extension, and lets us
//! run a Levenshtein automaton *against the entire set at once* for suggestions.

use fst::{IntoStreamer, Set, SetBuilder, Streamer};

use crate::levenshtein::Levenshtein;

/// A compiled Macedonian word list.
pub struct Lexicon {
    set: Set<Vec<u8>>,
}

impl Lexicon {
    /// Load a lexicon from a previously built FST.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, fst::Error> {
        Ok(Self { set: Set::new(bytes)? })
    }

    /// Build an FST from an iterator of words.
    ///
    /// Words are lowercased and must be supplied in ascending lexicographic
    /// order *after* lowercasing — [`build_from_unsorted`] handles that for you.
    pub fn build_sorted<I, S>(words: I) -> Result<Vec<u8>, fst::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = SetBuilder::memory();
        for w in words {
            builder.insert(w.as_ref())?;
        }
        builder.into_inner()
    }

    /// Sort, deduplicate and lowercase `words`, then build the FST.
    pub fn build_from_unsorted<I, S>(words: I) -> Result<Vec<u8>, fst::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut all: Vec<String> = words.into_iter().map(|w| w.as_ref().to_lowercase()).collect();
        all.sort_unstable();
        all.dedup();
        Self::build_sorted(all)
    }

    /// Number of words in the lexicon.
    pub fn len(&self) -> usize {
        self.set.len()
    }

    /// True when the lexicon holds no words.
    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    /// Is `word` a known Macedonian form? Case-insensitive.
    pub fn contains(&self, word: &str) -> bool {
        self.set.contains(word.to_lowercase())
    }

    /// Spelling suggestions for `word`, best first.
    ///
    /// Tries edit distance 1 before widening to 2, so a near-miss never gets
    /// buried under more distant candidates.
    pub fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let query = word.to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        for distance in 1..=2 {
            let mut hits = self.search_within(&query, distance, limit * 8);
            if !hits.is_empty() {
                rank(&query, &mut hits);
                hits.truncate(limit);
                return hits;
            }
        }
        Vec::new()
    }

    fn search_within(&self, query: &str, distance: u32, cap: usize) -> Vec<String> {
        let Some(automaton) = Levenshtein::new(query, distance) else {
            return Vec::new();
        };
        let mut stream = self.set.search(&automaton).into_stream();
        let mut out = Vec::new();
        while let Some(key) = stream.next() {
            if let Ok(s) = std::str::from_utf8(key) {
                if s != query {
                    out.push(s.to_string());
                }
            }
            if out.len() >= cap {
                break;
            }
        }
        out
    }
}

/// Order candidates so the most plausible correction comes first.
///
/// Typing errors preserve the first letter far more often than not, and a
/// correction close in length is likelier than a much longer or shorter word.
/// Confusable Macedonian letters (к/ќ, е/ѐ) sort before distant ones.
/// Frequency reranks after this in `Checker::ranked_suggestions`.
/// Cost of substituting `a` for `b` when reranking suggestions.
///
/// The FST search stays at exact edit distance ≤2; this only orders the hits.
/// One-key Macedonian slips (к/ќ, е/ѐ) cost 1, everything else 2.
// ponytail: static pair list, HashSet if it grows past ~30 pairs
pub fn confusion_cost(a: char, b: char) -> u32 {
    if a == b {
        return 0;
    }
    const PAIRS: &[(char, char)] = &[
        ('к', 'ќ'),
        ('г', 'ѓ'),
        ('с', 'ѕ'),
        ('з', 'ѕ'),
        ('џ', 'ч'),
        ('е', 'ѐ'),
        ('и', 'ѝ'),
        ('о', 'у'),
    ];
    if PAIRS.contains(&(a, b)) || PAIRS.contains(&(b, a)) {
        1
    } else {
        2
    }
}

/// Aligned confusion cost between `query` and `candidate`, plus 2 per
/// length difference. Cheap proxy, not a full alignment.
pub fn weighted_cost(query: &[char], candidate: &[char]) -> u32 {
    let shared = query.len().min(candidate.len());
    let mut cost = 0u32;
    for i in 0..shared {
        cost += confusion_cost(query[i], candidate[i]);
    }
    cost += 2 * (query.len().abs_diff(candidate.len()) as u32);
    cost
}

fn rank(query: &str, candidates: &mut [String]) {
    let first = query.chars().next();
    let qchars: Vec<char> = query.chars().collect();
    let qlen = qchars.len() as isize;
    candidates.sort_by_cached_key(|c| {
        let cchars: Vec<char> = c.chars().collect();
        let same_first = c.chars().next() != first;
        let len_delta = (cchars.len() as isize - qlen).abs();
        (weighted_cost(&qchars, &cchars), same_first, len_delta, c.clone())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon(words: &[&str]) -> Lexicon {
        Lexicon::from_bytes(Lexicon::build_from_unsorted(words).unwrap()).unwrap()
    }

    #[test]
    fn membership_is_case_insensitive() {
        let lex = lexicon(&["книга", "скопје"]);
        assert!(lex.contains("книга"));
        assert!(lex.contains("Книга"));
        assert!(lex.contains("Скопје"));
        assert!(!lex.contains("книгаа"));
    }

    #[test]
    fn builder_sorts_and_deduplicates() {
        let lex = lexicon(&["јаболко", "книга", "книга", "автобус"]);
        assert_eq!(lex.len(), 3);
    }

    #[test]
    fn suggests_a_single_character_typo() {
        let lex = lexicon(&["книга", "книги", "книгата", "автобус"]);
        let got = lex.suggest("книгa", 5); // trailing Latin 'a'
        assert!(got.contains(&"книга".to_string()), "got {got:?}");
    }

    #[test]
    fn suggestions_exclude_the_query_itself() {
        let lex = lexicon(&["книга", "книги"]);
        assert!(!lex.suggest("книга", 5).contains(&"книга".to_string()));
    }

    #[test]
    fn ranking_prefers_the_same_first_letter() {
        let lex = lexicon(&["дом", "том", "дим"]);
        let got = lex.suggest("дом", 3);
        assert_eq!(got.first().map(String::as_str), Some("дим"), "got {got:?}");
    }

    #[test]
    fn confusion_pair_outranks_distant_word() {
        assert_eq!(confusion_cost('к', 'ќ'), 1);
        assert_eq!(confusion_cost('к', 'м'), 2);
        assert_eq!(confusion_cost('е', 'ѐ'), 1);
    }

    #[test]
    fn ranking_prefers_the_confusable_letter() {
        // ќ is one slip from к; м is not. Both one edit from "как".
        let lex = lexicon(&["ќак", "мак", "как"]);
        let got = lex.suggest("как", 3);
        assert_eq!(got.first().map(String::as_str), Some("ќак"), "got {got:?}");
    }

    #[test]
    fn unknown_word_far_from_everything_yields_nothing() {
        let lex = lexicon(&["книга"]);
        assert!(lex.suggest("автомобилите", 5).is_empty());
    }
}
