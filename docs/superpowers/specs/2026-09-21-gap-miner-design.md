# Gap-miner for lexicon coverage — design

Date: 2026-09-21. Status: draft for review.
Goal (agreed): fewer false positives on modern text via lexicon coverage.
Non-goals: ranking, grammar eval sentences.

## Constraints (existing, not new)

- GPL-3.0-or-later; shipped data must be GPL-compatible or counts-only.
- Extension budget ~5 MB (`tools/build_freq_from_dump.py:62`).
- Precision before recall: `tools/eval_wiki.py` gates, 0 new false positives.
- `data/supplement/mk_supplement.txt:7` forbids corpus-chased individual words;
  general vocab is a morphology (Apertium) problem.
- Build-time over runtime: new work lives in `tools/`, no Rust changes.

## Sources evaluated (lexicon-coverage lens)

- MANU areal-linguistics / electronic corpus of literary texts: research-use
  framing, plain text if downloadable; candidate generator only.
- DAMJ (MANU + Metamorphosis): free-to-download scans; V1 plain-text only, OCR deferred.
- mk.wikisource (Macedonianess / MANU PD works): public domain but pre-1945
  orthography; archaic forms must be filtered, never shipped raw.
- dlib.mk (NUB): default CC BY-NC-ND; not shippable. Counts/gap-mining only.
- MK Wikipedia dump: already mined (freq top-40k, gazetteer names). Excluded
  from V1 to avoid re-mining.

## §1 Architecture / data flow (approved)

`tools/mine_gaps.py` (build-time only, nothing ships directly):
corpus texts → tokenize (reuse CYR regex from `build_freq_from_dump.py:16`)
→ frequency rank → diff against `current_words()` union
(`build_gazetteer.py:91`: wordlist + supplement + names + Apertium forms)
→ `data/interim/gap_candidates.tsv` (word TAB count TAB source) for manual
review only. Review routes: proper nouns → gazetteer flow into
`mk_names.txt`; missing lemmas → Apertium paradigm work via
`mk_apertium_forms.txt`; whole categorical class only → `mk_supplement.txt`.

## §2 Components (approved)

- New `tools/mine_gaps.py`; reuses normalization, CYR filter,
  `current_words()` pattern. No Rust changes, no new shipped artifact.
- V1 inputs: Wikisource XML dump + MANU electronic corpus (text). Scanned-PDF
  OCR (DAMJ, dlib.mk) deferred. Wikipedia excluded (already covered).
- Output interim, gitignored; `mk build-lexicon` command unchanged
  (`crates/mk-cli/src/main.rs:68`, `README.md:107`).

## §3 Error handling + testing (approved)

- Guards: skip Latin/foreign-Cyrillic mix, len <3 or >32, all-uppercase,
  archaic-orthography stoplist maintained in-script and reviewed with the diff;
  output capped (default 5000 rows, flag-overridable); never auto-writes to curated files.
- Missing input = loud failure; deterministic sort (−count, word), UTF-8.
- `cargo test` green; Python fixture self-check on a small checked-in fixture
  (20–50 lines covering ordering + each guard); any reviewed intake gated by
  `tools/eval_wiki.py` before/after.

## Success criteria

- `gap_candidates.tsv` produced from V1 sources, ranked, all rows traceable
  to (word, count, source); every shipped addition arrives via existing
  reviewed inputs with eval numbers.
