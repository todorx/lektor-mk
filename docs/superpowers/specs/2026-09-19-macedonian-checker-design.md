# Macedonian checker: Local-first++ — design

Date: 2026-09-19
Status: approved in chat (Local-first++ over Hybrid-assist / Minimal-spike)
Constraint: hybrid allowed, but local stays default. No text leaves machine unless user opts in later. Offline works. Extension stays < ~5 MB.

## 1. Intent

From README + chat: first real Macedonian proofreader. Success = fewer false flags, better suggestions, more grammar — without losing the 0-FP precision gate that makes `MK_DOUBLE_DEFINITE` trustworthy.

Measured baseline (README):
- Lexicon 356,402 forms → 0.68 MB FST; morph 161,953 forms / 30,380 lemmas → 2.47 MB
- Flag rate 5.06% on 17,782 wiki words: 55% proper nouns, 45% paradigm gaps
- Bigram recall ceiling 70.6% (both words analysed)

## 2. Architecture (no change, no new runtime deps)

```
text
 ├─ [0] tokenize  tokenizer.rs — к'смет, црно-бел, д-р, 1-ви
 ├─ [1] spell     lexicon.rs (FST) + levenshtein.rs (Unicode-correct, d≤2)
 ├─ [2] morphology morphology.rs (MKMORPH1 blob)
 ├─ [3] grammar   grammar.rs:30 check() — today 1 rule
 ▼ diagnostics (diagnostic.rs rule ids are public contract)
```

New build-time artifact only: `data/mk.freq` (word → count). Runtime loads it optionally; missing file = old ranking. No new crates. `build_extension.py` copies `mk.fst + mk.morph + mk.freq`.

## 3. Spelling coverage

### 3.1 Gazetteer v2 (fixes 55%)
- Today `tools/build_gazetteer.py` crawls 5 hubs, single-token Cyrillic `^[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]{3,30}$`, skips UPPERCASE.
- Change: source full `mkwiki-latest-all-titles-in-ns0.gz` + GeoNames `MK.zip` (CC-BY), same filter, diff-review before commit to `data/supplement/mk_names.txt`.
- Keep rule: categorical additions only, never corpus-chased individuals (see `mk_supplement.txt` header).

### 3.2 Paradigm completion (fixes 45%)
- `tools/expand_apertium.py` already expands 261 paradigms; ensure **all** single-token surfaces from `mk_apertium_forms.txt` flow into `cargo run -p mk-cli -- build-lexicon` (README step does; verify no filtering drops `референдумското`-class forms).
- Eval: `tools/eval_wiki.py` + `tools/flagshape.py` — flag rate down, capitalized share down, no rise in `MK_SPELL` on clean text.

## 4. Suggestions ("bad written words" + ranking)

Today `lexicon.rs:72 suggest()` tries d=1 then d=2, `lexicon.rs:115 rank()` sorts by (same-first-letter, len-delta, alpha). TODO in code names frequency as biggest win — do that:

1. **Frequency (P0).** Build `tools/build_freq.py`: parse `mkwiki-latest-pages-articles.xml`, lowercase, count Cyrillic tokens, output `data/interim/mk_freq.tsv` → compiled `data/mk.freq` (u16-len + utf8 + u32 count, sorted). `Lexicon::suggest()` loads optional `HashMap`; sort key becomes `(edit_dist, -log(freq+1), same_first, len_delta)`. Unknown words get freq 0 → old order preserved.
2. **Macedonian confusion costs (P1).** One function `confusion_cost(a: char, b: char) -> u32`: pairs `к/ќ, г/ѓ, с/ѕ, з/ѕ, џ/ч, љ/лј, њ/нј, е/ѐ, и/ѝ, о/у` cost 1 instead of 2 in rerank (not in automaton — keep FST search d≤2 exact, rerank only). Plus Latin-keyboard neighbors (`q/w/e...` → `љ/њ/е...`) for transliteration typos.
3. **Deferred (explicit non-goals):** bigram LM rerank, personal dictionary, neural reranker. Add when freq+confusion measurably falls short (`ponytail: O(candidates) rerank, per-bigram LM if needed`).

Free data: MK Wikipedia dump (CC BY-SA — ship counts, not text), Leipzig `mkd` corpus (CC-BY) to cross-check, Wiktionary via Wiktextract (CC BY-SA) for verb gaps. All GPL-3.0 compatible as counts.

## 5. Grammar (4 rules, then declarative)

Keep `grammar.rs:9-23` precision contract: fire only if *every* reading supports it, adjacent `TokenKind::Word` only (punct breaks phrase), features agree. 0 FP on wiki sample or rule doesn't ship.

Order (README planned list, easiest-precision first):
1. `MK_CLITIC_ORDER` — dative before accusative: `ми го даде` ✓ / `го ми даде` ✗. Needs `prn + dat/acc` tags (already in Apertium).
2. `MK_DATIVE_I` — `ѝ` (dat clitic, `mk_supplement.txt:26`) vs `и` (conjunction). Context: `ѝ` before verb, `и` between same-POS.
3. `MK_L_PARTICIPLE` — `is_l_participle()` (`morphology.rs:201`) + gender/number agree with subject in 3-token window. Start with `сум + lp` only.
4. `MK_NUMERAL_COUNT` — `num` + noun must be `ct` form (`два стола` not `столови`), `Number::Count` (`morphology.rs:109`).

Declarative proof: migrate `double_definite_article` (`grammar.rs:48`) to `data/rules/double_definite.toml` (pattern: adj-def + noun-def + agree → message + `drop-second-article` fix via `indefinite_form`). Engine reads TOML at build time into same `Diagnostic`. One rule proves shape; full migration later.

## 6. Data flow / build

```bash
tools/prepare_wordlist.sh
tools/expand_apertium.py <dix> data/interim/mk_morph.tsv data/interim/mk_apertium_forms.txt
python tools/build_gazetteer.py        # v2: dump + geonames
python tools/build_freq.py             # new: wiki → mk_freq.tsv
cargo run -p mk-cli -- build-lexicon ... data/mk.fst
cargo run -p mk-cli -- build-morph ... data/mk.morph
cargo run -p mk-cli -- build-freq ... data/mk.freq   # new
cargo test
python tools/eval_wiki.py data/mk.fst data/mk.morph
python tools/build_extension.py        # also copies mk.freq
```

## 7. Error handling / precision

- Spelling: keep skips — numerics (`lib.rs:99`), acronyms ≤5 caps (`lib.rs:243`), lone letters (`lib.rs:205`), isolated Latin (`lib.rs:167`), hyphen-compounds (`lib.rs:222`). Frequency must never promote a foreign word over silence.
- Grammar: silent on unanalysed (`grammar.rs:213` test), ambiguous readings, cross-punct (`grammar.rs:202` test). Suggestions only when `indefinite_form` verifies (`grammar.rs:113`).
- Sizes: freq blob capped at top 100k words (~1 MB); extension total stays <5 MB.

## 8. Testing

- Unit per rule: pos + neg + ambiguous + punct-boundary + span tests (mirror `grammar.rs:129` module).
- Rank tests in `lexicon.rs:125`: freq prefers common word; confusion prefers `ќ` over distant hit.
- Corpus: `eval_wiki.py` rate + `flagshape.py` shape as release gate; `UD_Macedonian-MTB` (155 sents, CC BY-SA) eval-only.
- No new harness; `cargo test` is the gate.

## 9. What was skipped, when to add

- Skipped: hybrid server second-pass (Stanza/spaCy/LLM), bigram LM, personal dict, full TOML engine. Add when local precision/recall plateaus and user explicitly opts into server.
- Skipped: Serbianism style layer (needs wordlist + community review). Add as `MK_SERBIANISM` Info-severity after the 4 Error rules land.
