<p align="center">
  <img src="logo.png" width="120" alt="Лектор-МК">
</p>

<h1 align="center">Лектор-МК</h1>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3-blue.svg" alt="License: GPL v3"></a>
  <a href="https://www.mozilla.org/firefox/"><img src="https://img.shields.io/badge/Firefox-extension-orange.svg" alt="Firefox"></a>
  <a href="#why-local-only"><img src="https://img.shields.io/badge/offline-100%25-green.svg" alt="Offline"></a>
</p>

<p align="center">
  <em>Проверка на правопис и граматика за македонски — целосно локално, без сервер, без интернет.</em>
</p>

Macedonian is not supported by LanguageTool, Grammarly, or any other serious proofreading tool. Лектор-МК is an attempt at the first real one: a spelling and grammar checker that runs **entirely on your machine** — no text ever leaves the browser.

## Before / after

You write `убавата книгата` (the article marked twice — wrong, but invisible to most eyes):

```
убавата книгата е на масата.
~~~~~~~~~~~~~~
MK_DOUBLE_DEFINITE: членот е означен двапати → убавата книга
```

Or a word that *looks* perfect but hides Latin letters inside Cyrillic ones (`мaкeдoнски` — the `a e o` are Latin). Лектор-МК finds it, repairs it, and confirms the repair against the lexicon before reporting — which is what keeps false positives near zero.

## Numbers

Honest measurements, not marketing. Live Wikipedia sample, reproducible via `tools/eval_wiki.py`:

| | |
|---|---|
| Lexicon | 880,826 forms → **1.50 MB** FST (16.7× smaller than the raw list) |
| Morphology | 161,953 forms, 30,380 lemmas → **2.65 MB** |
| Frequencies | 40,000 words ex Macedonian Wikipedia → **0.86 MB** (`mk.freq`, optional) |
| Bigrams | 50,000 pairs ex Macedonian Wikipedia → **1.47 MB** (`mk.bigram`, optional) |
| Extension total | FST + morph + freq → **5.0 MB**, + bigram → **6.5 MB**, still local-only and offline |
| Throughput | 17,782 words in **0.95 s** |
| Flag rate on Macedonian Wikipedia | **1.49%** over 17,104 words (down from 5.13%: the gazetteer took it to 2.79%, Wiktionary forms to 1.49%) |
| Typos still caught | 92.4% of 2,446 single-edit typos of frequent words (`tools/eval_typos.py`); was 93.1% before the Wiktionary forms |
| Morphology coverage | 83.5% of tokens; 70.6% of adjacent pairs |
| Grammar on Wikipedia | 4 `MK_L_PARTICIPLE` + 3 `MK_SPACE_BEFORE_PUNCT` + 2 `MK_ADJ_AGREEMENT` hits (all verified true), 0 everywhere else |
| Grammar gate | 46 labelled sentences (`tools/context_probe.tsv`): **0** false positives on the correct ones, 27/27 injected slips caught |
| `MK_DOUBLE_DEFINITE` false positives | **0** in 17,782 words of edited prose |

That last row is the number the project lives or dies by. A proofreader that cries wolf gets switched off — every rule ships with positive *and* negative test cases and is gated on precision before release.

The flag rate is not an error rate. Before the gazetteer it was dominated by two known gaps:

* **Proper nouns** (`Глигоров`, `Визбегово`, `Неделковски`). The upstream dictionary contains no names or toponyms at all. Covered since by the Wikipedia-title gazetteer (71,109 names), which cut the rate from 5.13% to 2.78%.
* **Morphological gaps** (`референдумското`, `старословенскиот`). Correctly-formed Macedonian the frozen word list never enumerated — the case for deriving paradigms from Apertium rather than shipping a frozen word list. Wiktionary's inflection tables (453k forms the other sources lacked) then cut spelling flags on the sample from 378 to 156.

Growing the lexicon has a price: every added form can make a typo look like a word. `tools/eval_typos.py` measures it, and Wiktionary entries marked nonstandard, dialectal, regional, archaic or obsolete in every sense are left out for exactly that reason (`праи`, `глеа`).

The script rules fare even better, and found genuine errors *in Wikipedia itself*: 13 occurrences of `сè` spelled with Latin `è` (U+00E8) instead of Cyrillic `ѐ` (U+0450), plus `селa`, `театарскa`, `филмскa` carrying a Latin `a` mid-word. Invisible to a reader; exactly the failure mode the rule exists for.

## How it works

```
text
  │
  ├─ [0] tokenize        Macedonian-aware: к'смет, црно-бел, д-р, 1-ви
  ├─ [1] spell           FST lexicon + Unicode-correct Levenshtein automaton
  ├─ [2] morphology      lemma + POS + features, expanded from Apertium
  ├─ [3] grammar rules   morphology-driven, precision-gated
  ├─ [4] ranking         unigram frequency + Macedonian confusion costs (к/ќ, е/ѐ),
  │                      bigram context when the table is loaded
  ▼
diagnostics → underlines in the page
```

The engine is a single Rust crate with no I/O and no platform assumptions. It takes compiled artifacts as bytes and returns diagnostics — one codebase serves the browser extension (WASM), a future desktop app (native), and the test CLI. Heavy NLP runs **at build time** in Python (paradigm expansion, frequency counting); the shipped runtime stays small.

### Why an FST for the lexicon

Macedonian attaches the definite article as a suffix, in three deictic series: `книга, книгата, книгава, книгана, книги, книгите, книгиве, книгине`. Surface forms multiply fast and share long prefixes. A finite-state transducer collapses that redundancy — and, the real reason, lets a Levenshtein automaton be intersected with the *entire* lexicon at once, so fuzzy suggestions never scan candidates one by one.

## Install

**Firefox** (Manifest V3, desktop 140+ and Android 142+):

```bash
python tools/build_extension.py   # wasm + glue + data/mk.fst + data/mk.morph
```

Then `about:debugging` → This Firefox → Load Temporary Add-on → `extension/manifest.json`. Text fields get inline wavy underlines as you type (click one for fixes), and the toolbar popup checks pasted text plus next-word autocomplete from bigram counts. All on-device, all offline.

Where inline checking works: `<textarea>`, text and search `<input>`s, and `contenteditable` editors in the page itself. Where it does not: **Google Docs** draws its text on a canvas, so no extension can read it — paste the text into the toolbar popup instead. Editors that live inside an `<iframe>` are not reached either. Password, email and number fields are never read.

Ignoring a flagged word adds it to the personal dictionary. If you ignore words that are correct Macedonian, *Settings → Личен речник → Извези* saves them to a text file you can attach to an issue — that is the only way feedback leaves your machine.

**CLI** (for testing and corpus evaluation):

```bash
cargo run --release -p mk-cli -- check data/mk.fst "Тој ја видe книгата."
cargo run --release -p mk-cli -- check data/mk.fst --morph data/mk.morph \
    "Убавата книгата е на масата."
```

## Build from source

Requires Rust and a C linker.

```bash
tools/prepare_wordlist.sh                                        # fetch + transcode upstream wordlist
tools/expand_apertium.py \
    data/raw/apertium-mkd/apertium-mkd.mkd.dix \
    data/interim/mk_morph.tsv \
    data/interim/mk_apertium_forms.txt                           # expand paradigms
python tools/expand_wiktionary.py                                # Wiktionary forms (needs data/raw/kaikki-mk.jsonl)
python tools/build_gazetteer.py                                  # proper-noun gazetteer
cargo run --release -p mk-cli -- build-lexicon \
    data/interim/mk_wordlist.utf8.txt \
    data/supplement/mk_supplement.txt \
    data/supplement/mk_names.txt \
    data/interim/mk_apertium_forms.txt \
    data/interim/mk_wiktionary_forms.txt \
    data/mk.fst                                                  # compile the lexicon
cargo run --release -p mk-cli -- build-morph \
    data/interim/mk_morph.tsv data/mk.morph                      # compile morphology
cargo test                                                       # run the suite (155+ tests)
```

Evaluate on live Wikipedia text and inspect coverage:

```bash
python tools/eval_wiki.py data/mk.fst data/mk.morph               # flag rate + rule breakdown
python tools/flagshape.py data/mk.fst                             # shape of spelling flags
python tools/probe.py data/mk.fst tools/probe_words.txt           # curated probe list
python tools/eval_context.py data/mk.fst data/mk.morph            # grammar gate: labelled slips + clean prose
```

`tools/context_probe.tsv` is the grammar gate: correct sentences that must stay silent (including first-person and colloquial prose, which Wikipedia barely contains) and the same sentences with one injected error that must be caught. It is hand-written, so its recall figure measures the rules against known targets, not against real-world error rates.

Find words the lexicon is missing, from any corpus you have (plain text or a MediaWiki XML dump such as `mkwikisource-latest-pages-articles.xml.bz2`):

```bash
python tools/mine_gaps.py corpus.txt [more.xml.bz2 ...]            # -> data/interim/gap_candidates.tsv
```

The output is a ranked review list, never an automatic import: proper nouns go to the gazetteer, missing lemmas to the Apertium paradigms, and only whole categories to `data/supplement/`.

## What it currently catches

| Rule | What it finds |
|---|---|
| `MK_SPELL` | Word absent from the lexicon, with ranked suggestions |
| `MK_ADJ_AGREEMENT` | Adjective disagreeing with its noun in gender or number: `убаво книга` ✗ |
| `MK_VERB_AGREEMENT` | Finite verb disagreeing with its subject in person or number: `тој сакаат` ✗ |
| `MK_HOMOGLYPH` | Latin `a c e o p s x y` hiding inside Cyrillic words |
| `MK_FOREIGN_CYRILLIC` | Serbian `ђ ћ`, Russian `ъ ы э я ю`, Bulgarian `щ` |
| `MK_LATIN_TEXT` | Macedonian typed in Latin letters, converted back |
| `MK_DOUBLE_DEFINITE` | The definite article marked twice: `убавата книгата` |
| `MK_CLITIC_ORDER` | Dative before accusative: `ми го даде` ✓, `го ми даде` ✗ |
| `MK_DATIVE_I` | Bare `и` where the dative clitic `ѝ` belongs |
| `MK_L_PARTICIPLE` | л-participle disagreeing with its subject: `таа дошол` ✗ |
| `MK_NE_FUSED` | `не` fused to a finite verb: `несака` ✗, `не сака` ✓ |
| `MK_NAJ_SEPARATED` | `нај` split from its word: `нај добар` ✗, `најдобар` ✓ |
| `MK_SENTENCE_CAPITAL` | Lowercase sentence start |
| `MK_PO_SEPARATED` | `по` split from the graded word: `по добар` ✗, `подобар` ✓ |
| `MK_SPACE_BEFORE_PUNCT` | Space before closing punctuation |
| `MK_NUMERAL_GENDER` | `два` with a feminine noun or `две` with a masculine one: `два книги` ✗, `две книги` ✓ |
| `MK_COUNT_FORM` | Plain plural after a numeral where a count form exists: `пет градови` → `пет града` |
| `MK_OBJECT_DOUBLING` | A definite object without its clitic, after a first- or second-person verb: `Видов книгата` → `Ја видов книгата` |
| `MK_SERBIANISM` | Serbian words with the Macedonian one as the fix: `увек` → `секогаш`, `сутра` → `утре` |

Deliberately narrow, for precision: object doubling ignores third-person verbs (`Така рече човекот` is verb–subject order, not a missing clitic), proper-noun objects and time adverbials (`Работев ноќта`); numeral gender ignores neuter nouns. Not yet covered: tense and mood slips that are themselves real words (`ја види` for `ја виде`).

Grammar rules are Rust functions in `crates/mk-core/src/grammar.rs`, each with positive and negative tests. `data/rules/` holds a sketch of a declarative format that is not read by the engine yet.

## Data provenance and licensing

Every source, what it is used for, and under what terms. Only counts and curated lists ship — article text and definitions never do.

| Source | Use | License |
|---|---|---|
| [gerazov/dictionary-mk](https://github.com/gerazov/dictionary-mk) (OSSM, Taras Bendik) | Base wordlist, 261,460 forms | GPL-2.0 |
| [apertium-mkd](https://github.com/apertium/apertium-mkd) | Paradigm expansion → morphology + inflected forms | GPL |
| [kaikki.org](https://kaikki.org/dictionary/Macedonian/) Wiktextract of English Wiktionary (Ylonen, LREC 2022) | Inflected forms for the spelling lexicon (`expand_wiktionary.py`) | CC BY-SA 4.0 / GFDL |
| MK Wikipedia article dump (`mkwiki-latest-pages-articles`) | Word/bigram counts only (top 40k unigrams, top 50k pairs) | CC BY-SA 4.0 (counts ship, text never ships) |
| MK Wikipedia title dump (`mkwiki-latest-all-titles-in-ns0`) | Proper-noun gazetteer (`mk_names.txt`), reviewed before commit | CC BY-SA 4.0 |
| Curated `data/supplement/` | Hand-reviewed gap fills (accents, abbreviations, compounds) | Same as this project |

The base dictionary is GPL-2.0, so this project is **GPL-3.0-or-later** (see [LICENSE](LICENSE)).

A note on the upstream dictionary: it is clean (exactly the 31 Macedonian letters, no Latin or foreign-Cyrillic contamination) but unmaintained — its `.aff` file has no affix rules at all and its `TRY` line lists the *Russian* alphabet. We use the wordlist and ignore the rest.

## Contributing

Small, precise contributions beat large ones. The rules of the house:

1. **Precision before recall.** A new rule must come with positive *and* negative test cases and zero false positives on the Wikipedia sample (`tools/eval_wiki.py`).
2. **Measure, don't assert.** Any claim about coverage or speed needs a before/after number from the tools above.
3. **Build-time over runtime.** Anything computable offline (paradigms, counts) belongs in `tools/`, not in the shipped extension.
4. `cargo test` must stay green. All of it, every time.

## License

GPL-3.0-or-later. See the [dictionary provenance](#data-provenance-and-licensing) above for why.
