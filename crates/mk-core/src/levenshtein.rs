//! A Levenshtein automaton over Unicode characters, for fuzzy FST lookup.
//!
//! # Why not `fst::automaton::Levenshtein`
//!
//! The one shipped with `fst` 0.4.7 returns wrong answers on multi-byte UTF-8.
//! Given the set `{дом, том, дим}`, a query of `дом` at distance 1 yields only
//! `том` — silently dropping `дим` — and at distance 2 yields *nothing at all*.
//! A larger edit budget returning a strictly smaller result set is impossible
//! for a correct implementation. The same probe over `{dom, tom, dim}` in ASCII
//! is perfectly correct, which pins the fault to its UTF-8 handling.
//!
//! Since every Macedonian letter is two bytes in UTF-8, that bug would have
//! disabled spelling suggestions entirely.
//!
//! # How this works
//!
//! `fst` walks keys one *byte* at a time, but edit distance is only meaningful
//! over whole *characters*. So the automaton state carries a small UTF-8 decode
//! buffer alongside the dynamic-programming row: bytes accumulate until they
//! form a complete character, and only then does the row advance.
//!
//! The row is the standard Levenshtein DP row against the query. `can_match`
//! prunes a subtree as soon as every cell exceeds the budget, which is what
//! keeps a fuzzy search over a quarter of a million words cheap.

use fst::Automaton;

/// Longest query we will attempt to correct.
///
/// The DP row is proportional to the query length and the search widens fast;
/// beyond this a "suggestion" would be guesswork anyway.
pub const MAX_QUERY_CHARS: usize = 32;

/// A Unicode-correct Levenshtein automaton.
#[derive(Debug, Clone)]
pub struct Levenshtein {
    query: Vec<char>,
    max_distance: u32,
}

/// Automaton state: how far off we are, plus any partially decoded character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// `row[i]` is the edit distance between the input consumed so far and the
    /// first `i` characters of the query. Values are capped at `max_distance+1`
    /// so that equivalent states compare equal and the search can dedupe them.
    row: Vec<u32>,
    /// Bytes of a character we have started but not finished reading.
    pending: [u8; 4],
    pending_len: u8,
    /// How many bytes the in-progress character needs in total.
    pending_want: u8,
    /// Set on invalid UTF-8, or when no cell can come back under budget.
    dead: bool,
}

impl Levenshtein {
    /// Build an automaton matching everything within `max_distance` edits.
    ///
    /// Returns `None` if the query is empty or longer than [`MAX_QUERY_CHARS`].
    pub fn new(query: &str, max_distance: u32) -> Option<Self> {
        let chars: Vec<char> = query.chars().collect();
        if chars.is_empty() || chars.len() > MAX_QUERY_CHARS {
            return None;
        }
        Some(Self { query: chars, max_distance })
    }

    fn cap(&self) -> u32 {
        self.max_distance + 1
    }

    /// Advance the DP row by one decoded character.
    fn step(&self, prev: &[u32], c: char) -> Vec<u32> {
        let cap = self.cap();
        let mut next = Vec::with_capacity(prev.len());
        next.push((prev[0] + 1).min(cap));

        for i in 1..prev.len() {
            let substitution = prev[i - 1] + u32::from(self.query[i - 1] != c);
            let insertion = next[i - 1] + 1;
            let deletion = prev[i] + 1;
            next.push(substitution.min(insertion).min(deletion).min(cap));
        }
        next
    }
}

/// How many bytes a UTF-8 sequence starting with `b` occupies, if valid.
fn utf8_len(b: u8) -> Option<u8> {
    match b {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        // Continuation bytes and overlong/invalid leads cannot start a character.
        _ => None,
    }
}

impl Automaton for Levenshtein {
    type State = State;

    fn start(&self) -> State {
        // Before consuming anything, the distance to the first `i` query
        // characters is exactly `i` (delete them all).
        let cap = self.cap();
        State {
            row: (0..=self.query.len() as u32).map(|v| v.min(cap)).collect(),
            pending: [0; 4],
            pending_len: 0,
            pending_want: 0,
            dead: false,
        }
    }

    fn is_match(&self, state: &State) -> bool {
        // A key only matches on a character boundary.
        !state.dead
            && state.pending_len == 0
            && *state.row.last().expect("row is never empty") <= self.max_distance
    }

    fn can_match(&self, state: &State) -> bool {
        // If every cell is already over budget, no continuation can recover.
        !state.dead && state.row.iter().min().is_some_and(|&m| m <= self.max_distance)
    }

    fn will_always_match(&self, _state: &State) -> bool {
        false
    }

    fn accept(&self, state: &State, byte: u8) -> State {
        if state.dead {
            return state.clone();
        }

        let mut next = state.clone();

        if next.pending_len == 0 {
            match utf8_len(byte) {
                Some(want) => {
                    next.pending[0] = byte;
                    next.pending_len = 1;
                    next.pending_want = want;
                }
                None => {
                    next.dead = true;
                    return next;
                }
            }
        } else {
            // Continuation bytes must be 10xxxxxx.
            if byte & 0xC0 != 0x80 {
                next.dead = true;
                return next;
            }
            next.pending[next.pending_len as usize] = byte;
            next.pending_len += 1;
        }

        if next.pending_len < next.pending_want {
            // Mid-character: the row cannot advance yet.
            return next;
        }

        let Ok(s) = std::str::from_utf8(&next.pending[..next.pending_len as usize]) else {
            next.dead = true;
            return next;
        };
        let Some(c) = s.chars().next() else {
            next.dead = true;
            return next;
        };

        next.row = self.step(&state.row, c);
        next.pending_len = 0;
        next.pending_want = 0;
        next.pending = [0; 4];
        next.dead = next.row.iter().all(|&v| v > self.max_distance);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fst::{IntoStreamer, Set, SetBuilder, Streamer};

    fn build(words: &[&str]) -> Set<Vec<u8>> {
        let mut sorted: Vec<&str> = words.to_vec();
        sorted.sort_unstable();
        let mut b = SetBuilder::memory();
        for w in sorted {
            b.insert(w).unwrap();
        }
        Set::new(b.into_inner().unwrap()).unwrap()
    }

    fn search(set: &Set<Vec<u8>>, query: &str, dist: u32) -> Vec<String> {
        let aut = Levenshtein::new(query, dist).expect("valid query");
        let mut stream = set.search(&aut).into_stream();
        let mut out = Vec::new();
        while let Some(k) = stream.next() {
            out.push(String::from_utf8(k.to_vec()).unwrap());
        }
        out.sort();
        out
    }

    /// The exact case that `fst`'s own Levenshtein gets wrong.
    #[test]
    fn cyrillic_substitutions_are_all_found() {
        let set = build(&["дом", "том", "дим", "дома"]);
        assert_eq!(search(&set, "дом", 1), vec!["дим", "дом", "дома", "том"]);
    }

    #[test]
    fn a_wider_budget_never_returns_less() {
        let set = build(&["дом", "том", "дим", "книга", "книги", "книгата"]);
        for query in ["дом", "книга", "том"] {
            let d1 = search(&set, query, 1);
            let d2 = search(&set, query, 2);
            for hit in &d1 {
                assert!(d2.contains(hit), "{query:?}: d=2 lost {hit:?} (d1={d1:?}, d2={d2:?})");
            }
        }
    }

    #[test]
    fn matches_a_latin_for_cyrillic_substitution() {
        // The homoglyph case: trailing 'a' is Latin, so this is one edit away.
        let set = build(&["книга", "книги", "автобус"]);
        assert!(search(&set, "книгa", 1).contains(&"книга".to_string()));
    }

    #[test]
    fn handles_insertion_and_deletion() {
        let set = build(&["книга"]);
        assert_eq!(search(&set, "книгаа", 1), vec!["книга"]); // extra character
        assert_eq!(search(&set, "книг", 1), vec!["книга"]); // missing character
    }

    #[test]
    fn respects_the_distance_budget() {
        let set = build(&["книга"]);
        assert!(search(&set, "кнштв", 1).is_empty());
    }

    #[test]
    fn ascii_still_behaves() {
        let set = build(&["dom", "tom", "dim", "book"]);
        assert_eq!(search(&set, "dom", 1), vec!["dim", "dom", "tom"]);
    }

    #[test]
    fn mixed_width_characters_work_together() {
        // 1-byte, 2-byte and 4-byte characters in one set.
        let set = build(&["ab", "аб", "a🎉", "аж"]);
        assert!(search(&set, "аб", 1).contains(&"аж".to_string()));
        assert!(search(&set, "a🎉", 0).contains(&"a🎉".to_string()));
    }

    #[test]
    fn distance_zero_is_exact_match() {
        let set = build(&["дом", "том"]);
        assert_eq!(search(&set, "дом", 0), vec!["дом"]);
    }

    #[test]
    fn rejects_unusable_queries() {
        assert!(Levenshtein::new("", 1).is_none());
        assert!(Levenshtein::new(&"а".repeat(MAX_QUERY_CHARS + 1), 1).is_none());
        assert!(Levenshtein::new(&"а".repeat(MAX_QUERY_CHARS), 1).is_some());
    }
}
