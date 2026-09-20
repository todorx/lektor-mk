use std::collections::HashMap;

/// Bigram counts: (previous word, word) → occurrences in Wikipedia.
/// Built offline like [`crate::frequency::Frequency`]; optional at runtime.
pub struct Bigram {
    map: HashMap<(String, String), u32>,
}

impl Bigram {
    /// Serialise entries. Format: b"MKBIGR01" + u32 count +
    /// (u16 len + utf8 prev + u16 len + utf8 word + u32 count)*.
    /// Keys are lowercased on the way in; lookup lowercases its inputs.
    /// Words over u16::MAX bytes are rejected rather than wrapped.
    pub fn build(entries: &[(&str, &str, u32)]) -> Result<Vec<u8>, String> {
        if entries.len() > u32::MAX as usize {
            return Err("too many entries".into());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"MKBIGR01");
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (a, b, c) in entries {
            let (a, b) = (a.to_lowercase(), b.to_lowercase());
            if a.len() > u16::MAX as usize || b.len() > u16::MAX as usize {
                return Err("word over u16::MAX bytes".into());
            }
            out.extend_from_slice(&(a.len() as u16).to_le_bytes());
            out.extend_from_slice(a.as_bytes());
            out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            out.extend_from_slice(b.as_bytes());
            out.extend_from_slice(&c.to_le_bytes());
        }
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.get(..8) != Some(b"MKBIGR01") {
            return Err("not a bigram file".into());
        }
        let mut pos = 8;
        let n = u32::from_le_bytes(
            bytes.get(pos..pos + 4).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
        ) as usize;
        pos += 4;
        let mut map = HashMap::new();
        for _ in 0..n {
            let la = u16::from_le_bytes(
                bytes.get(pos..pos + 2).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
            ) as usize;
            pos += 2;
            let a = std::str::from_utf8(bytes.get(pos..pos + la).ok_or("truncated")?)
                .map_err(|e| e.to_string())?
                .to_string();
            pos += la;
            let lb = u16::from_le_bytes(
                bytes.get(pos..pos + 2).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
            ) as usize;
            pos += 2;
            let b = std::str::from_utf8(bytes.get(pos..pos + lb).ok_or("truncated")?)
                .map_err(|e| e.to_string())?
                .to_string();
            pos += lb;
            let c = u32::from_le_bytes(
                bytes.get(pos..pos + 4).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
            );
            pos += 4;
            map.insert((a, b), c);
        }
        if pos != bytes.len() {
            return Err("trailing bytes".into());
        }
        Ok(Self { map })
    }

    /// Count for the pair, 0 when unseen. Case-insensitive.
    pub fn get(&self, prev: &str, word: &str) -> u32 {
        self.map
            .get(&(prev.to_lowercase(), word.to_lowercase()))
            .copied()
            .unwrap_or(0)
    }

    /// Top completions of `prefix` seen after `prev`, most frequent first.
    // ponytail: linear scan, prefix trie if 100k-entry scans ever show in profiles
    pub fn top_next(&self, prev: &str, prefix: &str, limit: usize) -> Vec<(String, u32)> {
        let prev = prev.to_lowercase();
        let prefix = prefix.to_lowercase();
        let mut hits: Vec<(String, u32)> = self
            .map
            .iter()
            .filter(|((a, b), _)| *a == prev && b.starts_with(&prefix))
            .map(|((_, b), &c)| (b.clone(), c))
            .collect();
        hits.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
        hits.truncate(limit);
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Bigram {
        let bytes = Bigram::build(&[
            ("тој", "оди", 900),
            ("тој", "одидома", 1),
            ("таа", "оди", 700),
        ])
        .unwrap();
        Bigram::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn known_pair_returns_its_count() {
        assert_eq!(table().get("тој", "оди"), 900);
    }

    #[test]
    fn lookup_is_case_insensitive_and_zero_when_unseen() {
        let t = table();
        assert_eq!(t.get("Тој", "Оди"), 900);
        assert_eq!(t.get("тој", "непостоечка"), 0);
        assert_eq!(t.get("непостоечка", "оди"), 0);
    }

    #[test]
    fn top_next_ranks_completions_after_prev() {
        let got = table().top_next("тој", "оди", 5);
        assert_eq!(got.first().map(|(w, _)| w.as_str()), Some("оди"));
        assert!(got.iter().any(|(w, _)| w == "одидома"));
    }

    #[test]
    fn top_next_respects_prefix_and_limit() {
        let t = table();
        assert!(t.top_next("тој", "одид", 5).iter().all(|(w, _)| w.starts_with("одид")));
        assert_eq!(t.top_next("тој", "оди", 1).len(), 1);
        assert!(t.top_next("тој", "zzz", 5).is_empty());
    }

    #[test]
    fn rejects_a_bad_magic() {
        assert!(Bigram::from_bytes(b"NOPE").is_err());
    }

    #[test]
    fn build_rejects_word_over_u16_bytes() {
        let big = "а".repeat(40_000); // 80,000 bytes in UTF-8
        assert!(Bigram::build(&[("тој", &big, 5)]).is_err());
        assert!(Bigram::build(&[(&big, "оди", 5)]).is_err());
    }

    #[test]
    fn from_bytes_rejects_trailing_garbage() {
        let mut bytes = Bigram::build(&[("тој", "оди", 900)]).unwrap();
        bytes.push(b'X');
        assert!(Bigram::from_bytes(&bytes).is_err());
    }

    #[test]
    fn build_lowercases_keys() {
        let bytes = Bigram::build(&[("ТОЈ", "ОДИ", 5)]).unwrap();
        let t = Bigram::from_bytes(&bytes).unwrap();
        assert_eq!(t.get("тој", "оди"), 5);
        assert_eq!(t.top_next("тој", "од", 5).len(), 1);
    }
}
