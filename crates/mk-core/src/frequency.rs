use std::collections::HashMap;

/// Word → count table, built offline from Wikipedia. Optional at runtime.
pub struct Frequency {
    map: HashMap<String, u32>,
}

impl Frequency {
    /// Serialise entries. Format: b"MKFREQ01" + u32 count + (u16 len + utf8 + u32 count)*.
    pub fn build(entries: &[(&str, u32)]) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        out.extend_from_slice(b"MKFREQ01");
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (w, c) in entries {
            out.extend_from_slice(&(w.len() as u16).to_le_bytes());
            out.extend_from_slice(w.as_bytes());
            out.extend_from_slice(&c.to_le_bytes());
        }
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.get(..8) != Some(b"MKFREQ01") {
            return Err("not a freq file".into());
        }
        let mut pos = 8;
        let n = u32::from_le_bytes(
            bytes.get(pos..pos + 4).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
        ) as usize;
        pos += 4;
        let mut map = HashMap::new();
        for _ in 0..n {
            let l = u16::from_le_bytes(
                bytes.get(pos..pos + 2).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
            ) as usize;
            pos += 2;
            let w = std::str::from_utf8(bytes.get(pos..pos + l).ok_or("truncated")?)
                .map_err(|e| e.to_string())?
                .to_string();
            pos += l;
            let c = u32::from_le_bytes(
                bytes.get(pos..pos + 4).ok_or("truncated")?.try_into().map_err(|e| format!("{e:?}"))?,
            );
            pos += 4;
            map.insert(w, c);
        }
        Ok(Self { map })
    }

    /// Count for `word`, 0 when unknown.
    pub fn get(&self, word: &str) -> u32 {
        self.map.get(word).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_word_outranks_rare_word() {
        let bytes = Frequency::build(&[("книга", 1000), ("книги", 2)]).unwrap();
        let f = Frequency::from_bytes(&bytes).unwrap();
        assert!(f.get("книга") > f.get("книги"));
        assert_eq!(f.get("непостоечка"), 0);
    }
}
