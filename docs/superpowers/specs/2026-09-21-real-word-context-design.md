# Real-word detection: catch the errors the lexicon cannot see

Date: 2026-09-21. Path: architectural (agreed in conversation; option A of four).
Status: ready-for-agent. Tracker: not configured (`/setup-matt-pocock-skills` never run) — filed in `docs/superpowers/specs/` per repo convention.

## Problem Statement

Today the checker decides correctness **one word at a time**: a word is either a literal entry in the FST lexicon or it is reported as a misspelling. That means every word that is spelled correctly but is *the wrong word* is invisible.

`Тој ја види книгата.` is wrong and gets no underline, because `види` is a real Macedonian word. `Тој ја видe книгата.` (Latin `e`) is caught by `MK_HOMOGLYPH`, and `Тој ја видаа книгата.` is caught by `MK_SPELL` — but `види` for `виде` is a genuine, frequent, user-visible error class that nothing in the engine can currently see.

From the user's perspective: they are told their prose is clean when it is not. The tool's whole claim is precision without crying wolf, but it currently pays for that precision with a silent blind spot on the single most common error type in Macedonian writing — a real word in the wrong form.

This is the largest remaining recall gap. It is also the one the existing architecture cannot reach by adding more per-word rules, because correctness is no longer a property of the word.

## Solution

A new rule, `MK_REAL_WORD`, that judges a word **by its neighbours instead of by itself**.

It fires only when all of these hold:

1. the word is in the lexicon (so `MK_SPELL` stays silent — this is the blind spot), and
2. the word is frequent enough that we would expect to have seen it in context, and
3. **both** its immediate neighbours are frequent words, and
4. **neither** attested pair (`prev → word`, `word → next`) appears in the bigram table, and
5. a near-twin of the word (FST edit distance ≤ 2) **is** attested in **both** positions.

When all five hold, the user gets a warning on the word with the twin as the one-click fix — the same underline-and-card interaction every other rule already uses.

The two-sided requirement (3–5) is the precision mechanism and the heart of the design. A single unattested pair is weak evidence: the shipped bigram table is pruned to the top 50k pairs at a minimum count of 125, so absence of a pair is normal for perfectly good prose. Requiring the pair to be rejected on **both** sides, and requiring one single near-twin to explain **both** sides, is a conjunction that random correct text almost never satisfies. Precision first; this rule is deliberately allowed to miss.

Reported severity is **Warning**, not Error: the word really is spelled correctly, and the user is the only one who knows what they meant.

## User Stories

1. As someone writing Macedonian, I want `Тој ја види книгата` to be flagged, so that I notice I used the wrong verb form even though the word is spelled correctly.
2. As someone writing Macedonian, I want the fix offered as one click, so that correcting it costs no more than correcting a misspelling.
3. As someone who is not a confident speller, I want the warning to look different from a hard misspelling, so that I know the checker is less certain.
4. As someone who writes correctly, I want the rule to stay silent on my correct prose, because a checker that cries wolf gets switched off.
5. As a user of the browser extension, I want this rule to work inline as I type, not only in a separate check.
6. As a user of the browser extension, I want this rule to be toggleable with every other rule on the options page.
7. As a user of the popup, I want a pasted paragraph to get the same real-word treatment as typed text.
8. As a user on any platform, I want the desktop, browser and CLI paths to agree, because they are one engine.
9. As a user without the optional bigram table, I want the checker to keep working exactly as before rather than error or guess.
10. As a user of a slow device, I want typing to stay responsive, so the expensive part of the check must run only where the cheap part already flagged a suspect.
11. As a maintainer, I want the new rule to be judged by a measured precision number on a corpus, not by an assertion that it seems reasonable.
12. As a maintainer, I want a measured recall number for real-word errors, which does not exist for any rule today.
13. As a maintainer, I want the rule's behaviour pinned by tests that fail if the guards are weakened.
14. As a maintainer, I want the rule id to be stable and reflected in the settings catalogue, so the UI can name and toggle it.
15. As a maintainer, I want honest documentation of what the rule deliberately does not catch.
16. As a reviewer of the published add-on, I want the new rule to be traceable to a rule id and a description in the source, so the shipped behaviour is explainable.
17. As someone reading the README, I want the rule count and the rule table to match the shipped engine.
18. As a future maintainer, I want the one-sided variant (only one neighbour available) to be a documented, deliberate omission rather than an accident.

## Implementation Decisions

- **New module in the engine.** A real-word module alongside the existing rule modules, exposing a single query: given a lowercased word and its two neighbour words, return `Option<replacement>`. It owns all thresholds and all guards, so the rule is testable in isolation without going through the tokenizer.
- **Rule id `MK_REAL_WORD`**, added to the stable rule-id module next to the existing ids. Ids are a public contract and are not renamed.
- **Severity `Warning`**, consistent with the existing "suspicious, worth a look" tier.
- **Requires both optional tables.** The rule is inert unless both the frequency table and the bigram table are loaded. This preserves today's behaviour for callers that pass neither, and keeps the WASM build working when a caller supplies a subset.
- **Neighbour adjacency means adjacency in the token stream, with no punctuation between.** A sentence boundary is not context. Only mid-sentence words are eligible, which is a deliberate narrowing.
- **Both neighbours are required.** The one-sided variant is rejected, not forgotten: it is where precision would be lost. Documented in Out of Scope.
- **Candidate generation reuses the existing lexicon suggestion search** (FST intersection at edit distance ≤ 2) rather than a hand-curated confusion list. This is the key choice: no Macedonian-specific pair list to maintain, no editorial risk, and the candidate set is exactly the set the user can act on. It also means any improvement to suggestion quality improves this rule for free.
- **Cost control by ordering, not by a new cache.** The engine checks the cheap conditions (length, frequency of the word, frequency of both neighbours, both bigram lookups) before ever running the FST candidate search. In normal prose the cheap checks reject almost every word, so the search runs on a small minority of tokens.
- **Thresholds live as named constants in the module** with a comment explaining the calibration direction, so the precision/recall dial is one place, not scattered literals. The bigram table's pruning (top 50k, min count 125) is the reason the alternative-count threshold is a floor rather than a ratio.
- **No new permission, no new data, no format change.** No change to the lexicon, frequency or bigram file formats, so no rebuild of existing artifacts is required.
- **Rule catalogue in the extension settings** gains the new id with a Macedonian name and description, because the options page renders the catalogue and the UI needs to name the rule. The catalogue comment already requires it to stay in sync with the engine's rule ids.
- **Documentation is part of the change**: the README rule table and rule count, and the AMO listing descriptions that quote a rule count, must not drift from the engine.

## Testing Decisions

A good test here asserts **observable checker behaviour** — given a sentence and a set of tables, which diagnostics come out — not the internals of the guard logic. The existing suite already works this way: rule modules are tested by building a tiny table inline and asserting on diagnostics; only behaviour crosses the seam.

- **Engine unit tests (new module).** Build a miniature frequency table and a miniature bigram table inline, construct the real-word checker over them, and assert:
  - a real word whose both contexts are unattested, with a near-twin attested in both, yields the twin;
  - each guard, removed in isolation, stops the rule firing: unknown-in-lexicon is not its business, low word frequency, low neighbour frequency, observed pair attested on one side, observed pair attested on the other side, candidate not attested on either side, candidate identical to the word, candidate too short;
  - the tie-break between two candidates is deterministic.
- **Pipeline test (`Checker::check`).** One test through the public `check` entry point proving the wiring end to end: with both tables loaded a sentence yields `MK_REAL_WORD`; with the bigram table absent the same sentence yields nothing from this rule (regression guard for the optional-table contract).
- **Corpus gate (new tool, tracked fixtures).** A small, hand-labelled probe file of `correct TAB perturbed TAB expected` rows, and a tool that runs the compiled engine over both columns and prints, in ASCII, the false positives on the correct column and the hit rate on the perturbed column. Prior art: the existing live-Wikipedia eval tool, which shells out to the release binary with the same flags and prints an ASCII summary. This is the acceptance gate for the rule: it must not be merged on the strength of unit tests alone.
- The corpus gate is a seed, not a claim of coverage: it exists so that the precision and recall numbers are reproducible and so that future changes to thresholds are measured rather than asserted.
- `cargo test` must stay green, including the existing 128 engine tests, with no test weakened to accommodate the new rule.

## Out of Scope

- **One-sided context.** Firing on only the preceding or only the following neighbour is deliberately excluded; it is the obvious next relaxation but it needs its own precision measurement first.
- **Cross-clause or long-distance agreement.** This rule looks at immediate neighbours only. Subject–predicate agreement at a distance, object reduplication, and numeral agreement remain separate rule families.
- **Hand-curated confusion sets** and any Macedonian-specific pair list. The design deliberately avoids editorial maintenance.
- **Learned or neural scoring.** No new dependency, no model, no training data.
- **Re-tuning the bigram table** (denser pruning to make absence a stronger signal). That is a data decision with a size cost, tracked separately.
- **Retro-fitting the existing rules** to use context, and migrating grammar rules to declarative data.
- **Any UI beyond the existing underline and fix card**, and any change to the extension's permissions or packaging.

## Measurement result (2026-09-21)

Built and run against the shipped tables (`data/mk.fst`, `mk.morph`, `mk.freq`, `mk.bigram`):

```
rows=16 false_positives=0 hits=0 recall=0%
GATE-OK
```

Precision is clean and recall is **zero**. The gate passes only because it gates on precision; the recall number is the finding.

The cause is the evidence base, not the guards. The shipped bigram table is the 50k-pruned set, and even the unpruned 100k TSV it came from does not contain ordinary grammatical pairs:

| pair | in 100k table |
|---|---|
| `ја виде` (correct) | **absent** |
| `ја види` (the slip) | 286 |
| `тој дојде` / `дојде дома` | absent |
| `е убава` / `сака кафе` | absent |

So "the pair is absent" carries no information at all — including for pairs that are plainly common — and the rule's requirement that a twin be attested on *both* sides is almost never satisfiable. The design's stated assumption ("absence is weak, two-sided absence is strong") is wrong in the direction that matters: absence is *uninformative* at this table size.

Two consequences:

1. Rule 5 of the design (a twin attested on both sides) cannot be met with the available data, so the statistical route is parked rather than shipped. Shipping it would put a toggle in the options page that never fires.
2. The probe rows are nonetheless a correct statement of the target error class. Reading them back — `убаво жена`, `голема човек`, `ново книга`, `добра човек`, `сакаа`, `знаат`, `имаат`, `читаат`, `гледаат`, `пишуваат` — they are **agreement** errors, and agreement is a categorical property of the morphology table (161,953 forms), not a statistical property of a 50k bigram list. The dense signal for this error class was there all along; the sparse one was chosen by mistake.

The gate itself earned its place: it cost one run and falsified the premise before any of it reached a user.

## Pivot: morphological agreement (2026-09-21)

`MK_REAL_WORD` was unwired from the pipeline and `realword.rs` kept as a parked,
tested capability. Two rules replaced it, both reading the morphology only:

* `MK_ADJ_AGREEMENT` — an attributive adjective must agree with its noun in
  gender and number.
* `MK_VERB_AGREEMENT` — a finite verb must agree with its subject in person and
  number, looking through clitics and `не`.

Measured:

```
probe  rows=16 agreement=13 uncovered=3  false_positives=0 hits=13 recall=100%
wiki   17104 words  flags=478  rate=2.79%   (baseline 2.78%)
       MK_ADJ_AGREEMENT:2  MK_VERB_AGREEMENT:0
```

Thirteen rules' worth of new detection for two extra flags on 17k words of real
text, one of which is a true positive (`македонски писменост` → a feminine noun
with a masculine adjective) and one a data gap (`главно` is analysed only as an
adjective, never as the adverb it is in `главно Турци`).

Four precision bugs were found by measuring rather than by reasoning, and each
is now a test:

* `се` was read as the verb. It carries a `ref` clitic reading **and** a bogus
  `vbser,pres,p3,mf,pl` reading, so "every reading is a clitic" never held and
  `тој се наоѓа` became a person/number mismatch. Clitics are now recognised by
  `clt` **or** `ref`.
* `цел` (adjective and noun) and `прави` (adjective and verb) were judged on
  their adjective reading in slots where they are not adjectives. The adjective
  side must now be unambiguously adjectival.
* `достапни интернет услуги` — a noun functioning as a modifier of the next
  noun. The head-noun slot is now skipped when another noun follows.
* Capitalised adjectives (`Велики`, `Географски`) are names or sentence-initial
  adverbs; without a real sentence segmenter both readings would be a guess.

Deliberately not covered, and listed as such in the probe: **tense and mood**
(`ја види` for `ја виде`, `дојди` for `дојде`). Those are not agreement
mismatches — `види` genuinely is third-person singular — so the agreement rules
are silent on them by construction, and three probe rows record the gap rather
than hiding it.

Known ceiling: an adjective whose adverb use is missing from the morphology
(`главно`) can still be flagged before a noun. That is a data gap in the
Apertium export, not a rule bug, and it is visible as the one false positive
above.

A further limitation, accepted: `донесен решение` (a genuine error) is now
missed, because `донесен` also carries a verb participle reading and the
unambiguous-adjective guard skips it. Precision was preferred to that one catch.

## Further Notes

The design intentionally accepts a low, precision-biased hit rate. The honest expectation is that this rule fires on a small fraction of the errors that exist, and that this is better than firing on prose that is already correct. The first real measurement of that trade-off is the corpus gate, and the thresholds are expected to move once it has produced a number.

The rule is also the first in the engine whose evidence is statistical rather than categorical. It should be described that way wherever rules are described, including the add-on listing, so that a Warning from this rule is understood as "this is unusual" rather than "this is wrong".

Finally, this change deliberately does not attempt to fix the reverse problem — correct but unlisted inflections reported as misspellings. That is a separate lever (unknown-word acceptance) with the opposite sign, and mixing the two in one change would make the measured effect of either unreadable.
