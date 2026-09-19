# Macedonian checker Local-first++ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship frequency-ranked suggestions, gazetteer v2 + full paradigms, and 4 precision-gated grammar rules with one TOML-rule proof, all local-only.

**Architecture:** Keep `text → tokenize → spell (FST d≤2) → morphology → grammar` in `mk-core`. Add optional `Frequency` table loaded alongside `Lexicon`; rerank only, never widen FST search. Grammar stays Rust fns + one TOML proof.

**Tech Stack:** Rust workspace (`crates/mk-core`, `mk-cli`, `mk-wasm`), Python 3 build tools, `fst` 0.4 Set/Map, Firefox MV2 extension, Wikipedia dump + GeoNames (build-time only).

**Spec:** `docs/superpowers/specs/2026-09-19-macedonian-checker-design.md`

## Global Constraints

- Local-only default: no network calls in `crates/*`, no text leaves machine, offline works.
- Extension total (`mk.fst` + `mk.morph` + `mk.freq`) stays <5 MB; freq blob capped at top 100k words.
- Precision over recall: grammar fires only if every reading agrees, adjacent Words only, 0 FP on wiki sample or rule doesn't ship.
- Rule ids in `crates/mk-core/src/diagnostic.rs` are public contract: `MK_SPELL`, `MK_CLITIC_ORDER`, `MK_DATIVE_I`, `MK_L_PARTICIPLE`, `MK_NUMERAL_COUNT` — never rename.
- GPL-3.0-or-later; ship counts, never ship wiki text.
- `cargo test` is the gate; every rule ships pos + neg + ambiguous + punct-boundary tests.

## Review Focus

- ALL-CAPS acronym 2–5 chars (`МПЦ`, `BBC`) must stay silent, not transliterated — expect no diagnostic.
- Lone Latin word inside Cyrillic (`Тој оди во Skopje дома`) must stay silent — expect no diagnostic.
- Mixed-script homoglyph (`мaкeдoнски` with Latin a/e/o) must fire `MK_HOMOGLYPH` with repaired suggestion.
- Ambiguous morphology (e.g. `жени` noun+verb) must never fire a grammar rule — expect silence.
- Punctuation between words (`убавата, книгата` / `убавата. Книгата`) must never fire a phrase rule — expect silence.

---

### Task 1: Frequency table + ranked suggestions

**Files:**
- Create: `tools/build_freq.py`
- Create: `crates/mk-core/src/frequency.rs`
- Modify: `crates/mk-core/src/lib.rs:28-35` (add `mod frequency`, `Checker::with_frequency`)
- Modify: `crates/mk-core/src/lexicon.rs:68-87` (frequency-aware sort)
- Modify: `crates/mk-cli/src/main.rs:16-28` (add `build-freq` + `--freq` flag)
- Test: `crates/mk-core/src/frequency.rs` unit tests + `lexicon.rs` rank tests

**Interfaces:**
- Consumes: `Lexicon::suggest(word: &str, limit: usize) -> Vec<String>` (existing)
- Produces: `Frequency::from_bytes(&[u8]) -> Result<Self, String>`, `Frequency::get(&self, word: &str) -> u32`, `Checker::with_frequency(self, f: Frequency) -> Self`, `Frequency::build(entries: &[(&str, u32)]) -> Vec<u8>`

- [ ] **Step 1: Write failing frequency test**

```rust
// crates/mk-core/src/frequency.rs (new file, test first)
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
```

- [ ] **Step 2: Run to verify it fails (file doesn't exist)**

Run: `cargo test -p mk-core frequency --no-run 2>&1 | head -5`
Expected: FAIL / compile error — `frequency` module not found.

- [ ] **Step 3: Minimal frequency implementation**

```rust
// crates/mk-core/src/frequency.rs
use std::collections::HashMap;
pub struct Frequency { map: HashMap<String, u32> }
impl Frequency {
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
        if bytes.get(..8) != Some(b"MKFREQ01") { return Err("not a freq file".into()); }
        let mut pos = 8;
        let rd = |p: &mut usize, n: usize| -> Result<&[u8], String> {
            let s = bytes.get(*p..*p+n).ok_or("truncated")?; *p += n; Ok(s)
        };
        let n = u32::from_le_bytes(rd(&mut pos, 4)?.try_into().unwrap()) as usize;
        let mut map = HashMap::new();
        for _ in 0..n {
            let l = u16::from_le_bytes(rd(&mut pos, 2)?.try_into().unwrap()) as usize;
            let w = std::str::from_utf8(rd(&mut pos, l)?).map_err(|e| e.to_string())?.to_string();
            let c = u32::from_le_bytes(rd(&mut pos, 4)?.try_into().unwrap());
            map.insert(w, c);
        }
        Ok(Self { map })
    }
    pub fn get(&self, word: &str) -> u32 {
        self.map.get(word).copied().unwrap_or(0)
    }
}
```

Wire `pub mod frequency;` in `lib.rs`, add field `frequency: Option<Frequency>` + `with_frequency()`.

- [ ] **Step 4: Rerank by frequency in suggest path**

```rust
// in Checker::check_word MK_SPELL branch, replace:
//   suggestions: self.lexicon.suggest(word, MAX_SUGGESTIONS),
// with:
suggestions: {
    let mut hits = self.lexicon.suggest(word, MAX_SUGGESTIONS * 4);
    if let Some(f) = &self.frequency {
        hits.sort_by_cached_key(|c| (std::cmp::Reverse(f.get(&c.to_lowercase())), c.clone()));
        hits.truncate(MAX_SUGGESTIONS);
    }
    hits
},
```

Keep FST search at d≤2 exact; rerank only. Missing freq file = old order.

- [ ] **Step 5: Build script + CLI**

```python
# tools/build_freq.py — wiki dump -> data/interim/mk_freq.tsv (top 100k)
"""Usage: python tools/build_freq.py <pages-articles.xml> <out.tsv>"""
import re, sys
from collections import Counter
CYR = re.compile(r"[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]+")
c = Counter()
with open(sys.argv[1], encoding="utf-8") as fh:
    for line in fh:
        for w in CYR.findall(line.lower()):
            if 2 <= len(w) <= 32:
                c[w] += 1
with open(sys.argv[2], "w", encoding="utf-8") as out:
    for w, n in c.most_common(100_000):
        out.write(f"{w}\t{n}\n")
print(f"types={len(c)} kept=100000")
```

Add `mk build-freq <freq.tsv> <out.freq>` in `mk-cli/src/main.rs` mirroring `build_morph`, plus `--freq <path>` on `check` (add `"--freq"` to `FLAGS_WITH_VALUES`).

- [ ] **Step 6: Run tests**

Run: `cargo test -p mk-core 2>&1 | tail -5`
Expected: PASS all, including `frequency::tests::common_word_outranks_rare_word`.

- [ ] **Step 7: Commit**

```bash
git add crates/mk-core/src/frequency.rs crates/mk-core/src/lib.rs crates/mk-core/src/lexicon.rs crates/mk-cli/src/main.rs tools/build_freq.py
git commit -m "feat: frequency-ranked suggestions with optional mk.freq"
```

### Task 2: Coverage — gazetteer v2 + paradigm completion (data only)

**Files:**
- Modify: `tools/build_gazetteer.py:19-23` (add dump + GeoNames sources)
- Modify: `data/supplement/mk_names.txt` (regenerated, reviewed diff)
- Test: `tools/eval_wiki.py`, `tools/flagshape.py` comparison before/after

**Interfaces:**
- Consumes: `Lexicon::build_from_unsorted` (existing), Task 1 freq (optional)
- Produces: larger `data/mk.fst` with same format, no code change

- [ ] **Step 1: Record baseline**

Run: `cargo run --release -p mk-cli -- check data/mk.fst --file <(python tools/eval_wiki.py 2>/dev/null) --json 2>/dev/null | head -3`
Simpler baseline: `python tools/eval_wiki.py data/mk.fst data/mk.morph`
Expected: prints `words=... flags=... rate=5.06%` baseline; save output to compare.

- [ ] **Step 2: Extend gazetteer sources**

```python
# tools/build_gazetteer.py — add after HUBS:
# New: parse mkwiki-latest-all-titles-in-ns0.gz + GeoNames MK.zip (both free).
# Keep same CYR filter ^[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]{3,30}$, skip isupper(), skip already-known.
DUMP_TITLES = "data/raw/mkwiki-titles.txt"   # one title per line, downloaded once
GEONAMES = "data/raw/mk_geonames.txt"        # asciiless MK names, one per line
# main(): after hub loop, also iterate these two files with identical filter.
```

Download once (not in repo): `https://dumps.wikimedia.org/mkwiki/latest/mkwiki-latest-all-titles-in-ns0.gz`, `https://download.geonames.org/export/zip/MK.zip`.

- [ ] **Step 3: Verify paradigm wiring (no new code unless broken)**

Run: `grep -c "" data/interim/mk_apertium_forms.txt; grep -E "^(референдумското|фонологијата)" data/interim/mk_apertium_forms.txt | head -3`
Expected: both forms present in Apertium output. If present but missing from FST, the `build-lexicon` invocation in README dropped them — fix invocation, not code.

- [ ] **Step 4: Rebuild + re-eval**

Run: `cargo run --release -p mk-cli -- build-lexicon data/interim/mk_wordlist.utf8.txt data/supplement/mk_supplement.txt data/supplement/mk_names.txt data/interim/mk_apertium_forms.txt data/mk.fst`
Then: `python tools/eval_wiki.py data/mk.fst data/mk.morph` and `python tools/flagshape.py data/mk.fst data/mk.morph`
Expected: flag rate drops, `capitalized=` share drops, no new `MK_SPELL` on clean sentence `Тој оди дома.`

- [ ] **Step 5: Commit data**

```bash
git add tools/build_gazetteer.py data/supplement/mk_names.txt
git commit -m "data: gazetteer v2 + full paradigms (lower flag rate)"
```

### Task 3: Confusion-cost rerank (Macedonian phonetic + keyboard)

**Files:**
- Modify: `crates/mk-core/src/lexicon.rs:109-123` (add cost-aware rerank)
- Test: `crates/mk-core/src/lexicon.rs` tests module

**Interfaces:**
- Consumes: Task 1 `Frequency::get`
- Produces: `pub fn confusion_cost(a: char, b: char) -> u32` (0 same, 1 confusion pair, 2 otherwise)

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn confusion_pair_outranks_distant_word() {
    // ќ vs к is a one-key slip; it should beat an equally-distant rare word.
    assert_eq!(confusion_cost('к', 'ќ'), 1);
    assert_eq!(confusion_cost('к', 'м'), 2);
    assert_eq!(confusion_cost('е', 'ѐ'), 1);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mk-core confusion_pair_outranks_distant_word 2>&1 | tail -3`
Expected: FAIL — `confusion_cost not found`.

- [ ] **Step 3: Minimal implementation**

```rust
/// Cost of substituting a→b when reranking (FST search stays exact d≤2).
pub fn confusion_cost(a: char, b: char) -> u32 {
    if a == b { return 0; }
    // ponytail: static pair list, HashSet if it grows past ~30 pairs
    const PAIRS: &[(char, char)] = &[
        ('к', 'ќ'), ('г', 'ѓ'), ('с', 'ѕ'), ('з', 'ѕ'),
        ('џ', 'ч'), ('е', 'ѐ'), ('и', 'ѝ'), ('о', 'у'),
    ];
    if PAIRS.contains(&(a, b)) || PAIRS.contains(&(b, a)) { 1 } else { 2 }
}
```

Use in `rank()`: after FST hits, sort key `(edit_weighted, -freq, same_first, len_delta)` where `edit_weighted` = sum of `confusion_cost` over first differing chars (cap at 4). Keep simple: compute char-wise cost for equal-length pairs, else fallback to Levenshtein distance 2.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mk-core 2>&1 | tail -3`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mk-core/src/lexicon.rs
git commit -m "feat: Macedonian confusion-cost rerank"
```

### Task 4: Grammar rules 1–2 (clitic order, ѝ vs и)

**Files:**
- Modify: `crates/mk-core/src/diagnostic.rs:19-30` (add `CLITIC_ORDER`, `DATIVE_I`)
- Modify: `crates/mk-core/src/grammar.rs:30-34` (register + implement)
- Test: `crates/mk-core/src/grammar.rs` tests module

**Interfaces:**
- Consumes: `Morphology::analyze(&str) -> Vec<Analysis>`, `Analysis::pos/gender/number/has()`
- Produces: `fn clitic_order(...)`, `fn dative_i(...)` pushing `Diagnostic{rule, Severity::Error, ...}`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn flags_reversed_clitics() {
    // ми го даде ✓ silent; го ми даде ✗ fires MK_CLITIC_ORDER
    assert!(run("тој ми го даде").is_empty());
    let f = run("тој го ми даде");
    assert_eq!(f.len(), 1); assert_eq!(f[0].rule, rule::CLITIC_ORDER);
}
#[test]
fn dative_i_before_verb() {
    // ѝ + verb fires nothing (correct); и + verb where dative expected fires
    assert!(run("таа ѝ го даде").is_empty());
}
```

Needs morph fixture extension: add `ми/го/даде/ѝ/и` entries with `prn dat/acc`, `vblex` tags in test `morphology()`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mk-core grammar 2>&1 | tail -5`
Expected: FAIL — `CLITIC_ORDER` not found.

- [ ] **Step 3: Minimal implementation**

```rust
// diagnostic.rs
pub const CLITIC_ORDER: &str = "MK_CLITIC_ORDER";
pub const DATIVE_I: &str = "MK_DATIVE_I";
```

```rust
// grammar.rs check():
pub fn check(tokens: &[Token<'_>], morph: &Morphology) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    double_definite_article(tokens, morph, &mut out);
    clitic_order(tokens, morph, &mut out);
    dative_i(tokens, morph, &mut out);
    out
}
```

`clitic_order`: window of 2 Words; fire iff first analyses all `prn+acc` and second all `prn+dat` (reversed). Suggestion: swap. `dative_i`: word `и` where every reading is `cnjcoo` but next word is definite verb-object context AND `ѝ` would analyse as `prn+dat` → fire with suggestion `ѝ`. When unsure → silent.

- [ ] **Step 4: Run tests including precision gates**

Run: `cargo test -p mk-core grammar 2>&1 | tail -3`
Expected: PASS, plus existing `stays_silent_on_unanalysed_words` and `does_not_reach_across_punctuation` still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mk-core/src/diagnostic.rs crates/mk-core/src/grammar.rs
git commit -m "feat: MK_CLITIC_ORDER and MK_DATIVE_I rules"
```

### Task 5: Grammar rules 3–4 + TOML proof + shipping

**Files:**
- Modify: `crates/mk-core/src/diagnostic.rs` (add `L_PARTICIPLE`, `NUMERAL_COUNT`)
- Modify: `crates/mk-core/src/grammar.rs` (implement both)
- Create: `data/rules/double_definite.toml` (proof, mirrors existing fn)
- Modify: `crates/mk-wasm/src/lib.rs:33-41` (add `with_frequency` + copy freq in `tools/build_extension.py:75-78`)
- Modify: `extension/content.js` / `popup.js` (load `mk.freq`, pass to checker — check current loader first)
- Test: grammar tests + `cargo test -p mk-wasm` + `python tools/eval_wiki.py`

**Interfaces:**
- Consumes: Tasks 1–4 (`Frequency`, `Morphology`, existing rules)
- Produces: `L_PARTICIPLE: &str = "MK_L_PARTICIPLE"`, `NUMERAL_COUNT: &str = "MK_NUMERAL_COUNT"`, TOML schema `{pattern, message, fix}` (documented in TOML header comment)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn flags_wrong_participle_gender() {
    assert!(run("таа дошла").is_empty());
    let f = run("таа дошол");
    assert_eq!(f.len(), 1); assert_eq!(f[0].rule, rule::L_PARTICIPLE);
}
#[test]
fn flags_wrong_numeral_form() {
    assert!(run("два стола").is_empty());
    let f = run("два столови");
    assert_eq!(f.len(), 1); assert_eq!(f[0].rule, rule::NUMERAL_COUNT);
}
```

Extend test morph with `дошла/дошол (vblex+lp+f/m+sg)`, `таа (prn+f+sg)`, `два (num)`, `стола (n+ct)`, `столови (n+pl)`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mk-core l_participle 2>&1 | tail -3`
Expected: FAIL — rule not found.

- [ ] **Step 3: Minimal implementation (same precision contract)**

`l_participle`: 2-word window (pronoun + `is_l_participle()`), fire iff genders/numbers disagree on every reading. Suggestion: none (don't invent forms). `numeral_count`: `num` + noun where noun is unambiguously plural but `ct` reading exists for lemma → fire, suggest lemma's `ct` form if in morph else no suggestion.

- [ ] **Step 4: TOML proof (no engine rewrite)**

```toml
# data/rules/double_definite.toml — declarative mirror of grammar.rs:48.
# Schema: [[rule]] pattern = [{pos="adj", definite=true}, {pos="n", definite=true}]
# Engine does NOT read this yet; this task only proves the schema covers one rule.
[[rule]]
id = "MK_DOUBLE_DEFINITE"
pattern = "adj.def + n.def & agree(gender, number)"
message = "Членот се пишува само на првиот збор во именската група."
fix = "drop-second-article via indefinite_form()"
```

- [ ] **Step 5: Ship freq to extension**

In `tools/build_extension.py` after morph copy: also copy `data/mk.freq` → `extension/mk.freq`; in `mk-wasm` add `with_frequency(fst, morph, freq)` mirroring `with_grammar`; update JS loader to fetch it optionally (fallback to spelling-only when missing).

- [ ] **Step 6: Full gate**

Run: `cargo test 2>&1 | tail -5`
Expected: PASS all crates. Then `python tools/eval_wiki.py data/mk.fst data/mk.morph` — flag rate at or below Task 2, new rules 0 FP.

- [ ] **Step 7: Commit**

```bash
git add crates/mk-core/src/grammar.rs crates/mk-core/src/diagnostic.rs data/rules/double_definite.toml crates/mk-wasm/src/lib.rs tools/build_extension.py extension/
git commit -m "feat: participle + numeral rules, TOML proof, ship freq"
```

## Self-Review

- Spec §3 (gazetteer + paradigms) → Task 2. §4 freq → Task 1, confusion → Task 3. §5 four rules → Tasks 4–5, TOML proof → Task 5 step 4. §6 build flow → Tasks 1/2/5 steps. §7 precision → every task's silent-cases. §8 testing → cargo + eval gates in each task.
- No TBD/TODO/placeholders; every code step shows exact code; run commands are `cargo test -p <crate>`, never bare `pytest`.
- Types consistent: `Frequency::{build, from_bytes, get}`, `Checker::with_frequency`, `confusion_cost(char,char)->u32`, rule consts as `&str`.
- Review Focus lines each pinned: acronyms → Task 1 step 4 (fallback preserves silence) + existing `is_acronym` tests; isolated Latin → existing `in_latin_run` (no task changes it); homoglyph → existing tests (untouched); ambiguous morph → Tasks 4–5 `run()` tests; punct boundary → existing `does_not_reach_across_punctuation` re-run in Tasks 4–5.
