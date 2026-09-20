//! WASM bridge for the Firefox extension.
//!
//! Thin by design: all checking lives in `mk_core`. This crate only moves
//! bytes in (compiled lexicon/morphology, text) and JSON out (diagnostics).
//! Diagnostics cross the boundary as a JSON string so the JS side needs no
//! generated bindings for them — `JSON.parse` is the whole protocol.

use mk_core::morphology::Morphology;
use mk_core::Checker;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmChecker {
    inner: Checker,
}

#[wasm_bindgen]
impl WasmChecker {
    /// Spell-checker over the compiled lexicon (`data/mk.fst`).
    ///
    /// The blob is a trusted build artifact: like `fst` itself, corrupt bytes
    /// may trap rather than return `Err`. The build script copies our own
    /// compiled file next to the extension, so no validation layer here.
    #[wasm_bindgen(constructor)]
    pub fn new(fst_bytes: Vec<u8>) -> Result<WasmChecker, JsValue> {
        Checker::new(fst_bytes)
            .map(|inner| WasmChecker { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Spell + grammar checker. Falls back to [`WasmChecker::new`] when the
    /// morphology blob is missing — spelling never pays for grammar.
    pub fn with_grammar(fst_bytes: Vec<u8>, morph_bytes: Vec<u8>) -> Result<WasmChecker, JsValue> {
        let morphology =
            Morphology::from_bytes(&morph_bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Checker::new(fst_bytes)
            .map(|c| WasmChecker {
                inner: c.with_morphology(morphology),
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Check `text`, returning diagnostics as a JSON array string.
    /// Serialization of our own types cannot fail; `"[]"` is the fallback
    /// rather than a WASM panic.
    pub fn check(&self, text: &str) -> String {
        serde_json::to_string(&self.inner.check(text)).unwrap_or_else(|_| "[]".to_string())
    }

    /// Attach frequency-ranked suggestions. Optional: a missing or corrupt
    /// blob is an `Err` and the caller keeps the checker without it.
    pub fn set_frequency(&mut self, freq_bytes: Vec<u8>) -> Result<(), JsValue> {
        let freq = mk_core::frequency::Frequency::from_bytes(&freq_bytes)
            .map_err(|e| JsValue::from_str(&e))?;
        self.inner.set_frequency(freq);
        Ok(())
    }

    /// Attach bigram-aware ranking. Optional, same contract as frequency.
    pub fn set_bigram(&mut self, bigram_bytes: Vec<u8>) -> Result<(), JsValue> {
        let bigram = mk_core::bigram::Bigram::from_bytes(&bigram_bytes)
            .map_err(|e| JsValue::from_str(&e))?;
        self.inner.set_bigram(bigram);
        Ok(())
    }

    /// Next-word completions of `prefix` after `prev`, as a JSON array string.
    /// Empty (`[]`) when no bigram table is attached or nothing was seen.
    pub fn suggest(&self, prev: &str, prefix: &str, limit: usize) -> String {
        serde_json::to_string(&self.inner.suggest_next(prev, prefix, limit))
            .unwrap_or_else(|_| "[]".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_checker() -> WasmChecker {
        let bytes =
            mk_core::Lexicon::build_sorted(["книга", "маса", "тој"].into_iter()).unwrap();
        WasmChecker::new(bytes).unwrap()
    }

    #[test]
    fn check_returns_json_array() {
        // Cyrillic gibberish -> MK_SPELL; Latin-only input takes a different
        // rule path, so it does not belong in this assertion.
        let out = tiny_checker().check("тој книга џџџ");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v.as_array().unwrap().iter().any(|d| d["text"] == "џџџ"));
    }

    #[test]
    fn known_word_has_no_diagnostic() {
        let out = tiny_checker().check("тој");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[test]
    fn frequency_blob_attaches_when_valid() {
        // The corrupt-blob Err path maps through JsValue::from_str, which
        // panics on non-wasm32 targets — it is exercised only in the browser.
        // Corrupt input itself is covered by frequency::tests in mk-core.
        let mut c = tiny_checker();
        let bytes = mk_core::frequency::Frequency::build(&[("книга", 5)]).unwrap();
        assert!(c.set_frequency(bytes).is_ok());
    }

    #[test]
    fn bigram_blob_attaches_and_suggests() {
        let mut c = tiny_checker();
        let bytes =
            mk_core::bigram::Bigram::build(&[("тој", "оди", 9), ("тој", "одидома", 1)]).unwrap();
        assert!(c.set_bigram(bytes).is_ok());
        let v: serde_json::Value = serde_json::from_str(&c.suggest("тој", "оди", 5)).unwrap();
        let words: Vec<&str> = v.as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
        assert_eq!(words.first(), Some(&"оди"));
    }
}
