# macedonian-text

A spelling and grammar checker for Macedonian, built to run **entirely on the
user's machine** — no text ever leaves the browser.

Macedonian is not supported by LanguageTool, Grammarly, or any other serious
proofreading tool. This is an attempt at the first real one.

## Why local-only

A proofreader sees everything you write: messages, medical questions, drafts of
things you have not decided to send. Shipping that to a server is a poor trade
when the whole engine fits in a few megabytes of WebAssembly. Local-first also
means it works offline, costs nothing to run, and has no latency.

## Architecture

```
text
  │
  ├─ [0] tokenize        Macedonian-aware: к'смет, црно-бел, д-р, 1-ви
  ├─ [1] spell           FST lexicon + Levenshtein automaton
  ├─ [2] morphology      lemma + POS + features, from Apertium
  ├─ [3] grammar rules   one built; declarative format still to come
  ▼
diagnostics → underlines in the page
```

The engine is a single Rust crate with no I/O and no platform assumptions. It
takes a compiled lexicon as bytes and returns diagnostics. That is what lets one
codebase serve the browser extension (WASM), a future Windows desktop app
(native), and the test CLI.

Heavy NLP runs **at build time** in Python — spaCy's [`mk_core_news_*`] models
and corpus processing bake compact artifacts. The shipped runtime stays small.

[`mk_core_news_*`]: https://spacy.io/models/mk

### Why an FST for the lexicon

Macedonian attaches the definite article as a suffix, in three deictic series:
`книга, книгата, книгава, книгана, книги, книгите, книгиве, книгине`. Surface
forms multiply fast and share long prefixes. A finite-state transducer collapses
that redundancy, and — the real reason — lets a Levenshtein automaton be
intersected with the *entire* lexicon at once, so fuzzy suggestions do not
require scanning candidates one by one.

## Layout

```
crates/mk-core/     the engine — tokenizer, lexicon, homoglyph, transliteration
crates/mk-cli/      command-line driver, used for testing and for building the FST
crates/mk-wasm/     WebAssembly bridge (JSON in/out) for the Firefox extension
extension/          Firefox extension (MV2, local-only WASM checker)
tools/              data preparation
data/               dictionary sources and the compiled lexicon (git-ignored)
```

## Firefox extension

```bash
python tools/build_extension.py   # wasm + glue + data/mk.fst + data/mk.morph
```

Then load it in Firefox via `about:debugging` → This Firefox →
Load Temporary Add-on → `extension/manifest.json`. The checker runs
entirely on-device: text fields get Harper-style inline wavy underlines
as you type (click one for fixes), and the toolbar popup checks
pasted text.

## Build

Requires Rust and a C linker (`sudo dnf install -y gcc glibc-devel` on Fedora).

```bash
tools/prepare_wordlist.sh                                        # fetch + transcode
tools/expand_apertium.py \
    data/raw/apertium-mkd/apertium-mkd.mkd.dix \
    data/interim/mk_morph.tsv \
    data/interim/mk_apertium_forms.txt                           # expand paradigms
python tools/build_gazetteer.py                                  # proper-noun gazetteer
cargo run --release -p mk-cli -- build-lexicon \
    data/interim/mk_wordlist.utf8.txt \
    data/supplement/mk_supplement.txt \
    data/supplement/mk_names.txt \
    data/interim/mk_apertium_forms.txt \
    data/mk.fst                                                  # compile the lexicon
cargo run --release -p mk-cli -- build-morph \
    data/interim/mk_morph.tsv data/mk.morph                      # compile morphology
cargo test                                                       # run the suite

# spelling only
cargo run --release -p mk-cli -- check data/mk.fst "Тој ја видe книгата."
# with grammar
cargo run --release -p mk-cli -- check data/mk.fst --morph data/mk.morph \
    "Убавата книгата е на масата."
# inspect a word's analyses
cargo run --release -p mk-cli -- analyze data/mk.morph книгата дошла
```

## Measured

| | |
|---|---|
| Lexicon | 357,191 forms → **0.69 MB** FST (13× smaller than the raw list) |
| Morphology | 161,953 forms, 30,380 lemmas → **2.65 MB** (incl. participle reverse index) |
| Frequencies | 40,000 words ex Macedonian Wikipedia → **0.86 MB** (`mk.freq`, optional) |
| Extension total | FST + morph + freq → **4.0 MB**, still local-only and offline |
| Throughput | 17,782 words in **0.95 s** |
| Flag rate on Macedonian Wikipedia | 5.13% over 17,105 words (live sample; was 5.06%) |
| Morphology coverage | 83.5% of tokens; 70.6% of adjacent pairs |
| Grammar on Wikipedia | 4 `MK_L_PARTICIPLE` hits, 0 everything else — no clear false positives after the negation/object guards |
| `MK_DOUBLE_DEFINITE` false positives | **0** in 17,782 words of edited prose |

That last row is the number the project lives or dies by. The rule catches
`убавата книгата`, `Големиот градот` and `Новата куќата` while staying silent on
`убавата книга`, `убава книгата` and `Големиот град` — and never once misfired
across a whole corpus of edited Wikipedia text.

The 70.6% adjacent-pair figure is the recall ceiling for any rule that inspects
a bigram: a rule cannot judge a pair it cannot analyse. That is a coverage
limit, not a precision problem, and it improves as the morphology does.

The flag rate is not an error rate — it is dominated by two known gaps, measured
over 17,782 words of Wikipedia:

* **55% proper nouns** (`Глигоров`, `Визбегово`, `Неделковски`). The upstream
  dictionary contains no names or toponyms at all. Needs a gazetteer.
* **45% morphological gaps** (`референдумското`, `старословенскиот`,
  `фонологијата`). All correctly-formed Macedonian the fixed form list never
  enumerated. This is the case for deriving paradigms from apertium-mkd rather
  than shipping a frozen word list.

The script rules fare much better, and found genuine errors *in Wikipedia
itself*: 13 occurrences of `сè` spelled with Latin `è` (U+00E8) instead of
Cyrillic `ѐ` (U+0450), plus `селa`, `театарскa`, `филмскa`, `броeло` carrying a
Latin `a` or `e` mid-word. Exactly the failure mode the rule exists for, and
invisible to a reader.

## What it currently catches

| Rule | What it finds |
|---|---|
| `MK_SPELL` | Word absent from the lexicon, with ranked suggestions |
| `MK_HOMOGLYPH` | Latin `a c e o p s x y` hiding inside Cyrillic words |
| `MK_FOREIGN_CYRILLIC` | Serbian `ђ ћ`, Russian `ъ ы э я ю`, Bulgarian `щ` |
| `MK_LATIN_TEXT` | Macedonian typed in Latin letters, converted back |
| `MK_DOUBLE_DEFINITE` | The definite article marked twice: `убавата книгата` |
| `MK_CLITIC_ORDER` | Dative before accusative: `ми го даде` ✓, `го ми даде` ✗ |
| `MK_DATIVE_I` | Bare `и` where the dative clitic `ѝ` belongs |
| `MK_L_PARTICIPLE` | л-participle disagreeing with its subject: `таа дошол` ✗ |
| `MK_NE_FUSED` | `не` fused to a finite verb: `несака` ✗, `не сака` ✓ |
| `MK_NAJ_SEPARATED` | `нај` split from its word: `нај добар` ✗, `најдобар` ✓ |

Suggestions are frequency-ranked (Wikipedia counts) with Macedonian
confusion costs (`к/ќ`, `е/ѐ`), falling back to edit-distance order when
no frequency table is loaded.

The two script rules matter more than they look. Latin `а е о с р х у` are pixel
twins of their Cyrillic counterparts, so contaminated text looks perfect to a
human while breaking every spell-checker, search index and sort order it touches.
Both rules confirm the repaired spelling against the lexicon before reporting,
which is what keeps false positives near zero.

## Planned grammar rules

These are the checks no generic tool can do, and the reason the project exists:

- ~~**Definite article placement**~~ — built, see `MK_DOUBLE_DEFINITE` above
- ~~**Clitic order**~~ — built (`MK_CLITIC_ORDER`): dative before accusative
- ~~**`ѝ` vs `и`**~~ — built (`MK_DATIVE_I`): dative clitic against the conjunction
- ~~**л-participle agreement**~~ — built (`MK_L_PARTICIPLE`): `тој дошол` / `таа дошла`
- ~~**Separated `не`**~~ — built (`MK_NE_FUSED`): negation stays separate from
  finite verbs, except lexicalized fusions (`нестане`, `непогоди`)
- ~~**Fused `нај`**~~ — built (`MK_NAJ_SEPARATED`): superlative particle fuses
  with the graded word, verified against known vocabulary
- **Object reduplication** — definite objects require a resumptive clitic:
  `Ја видов Марија` ✓
- **Numeral forms** — `два стола` not `два столови`; `двајца студенти`
  (blocked: the morphology has zero `ct` count-form rows, so the rule cannot
  meet the precision gate yet)
- **Serbianisms** — a style layer for Serbian-influenced constructions

Rules will be declarative data, not Rust, so that someone who knows Macedonian
grammar but not systems programming can add them. Every rule ships with positive
*and* negative test cases, and is gated on precision before release: a
proofreader that cries wolf gets switched off.

## Data provenance and licensing

| Source | Use | License |
|---|---|---|
| [gerazov/dictionary-mk](https://github.com/gerazov/dictionary-mk) (OSSM, Taras Bendik) | Base wordlist, 261,460 forms | GPL-2.0 |
| [apertium-mkd-bul](https://github.com/apertium/apertium-mkd-bul) | Morphological analysis (planned) | GPL |
| [spaCy `mk_core_news_*`](https://spacy.io/models/mk) | Build-time tagging (planned) | MIT |
| [UD_Macedonian-MTB](https://universaldependencies.org/treebanks/mk_mtb/index.html) | Evaluation only — 155 sentences | CC BY-SA 4.0 |
| MK Wikipedia article dump (`mkwiki-latest-pages-articles`) | Word-frequency counts only (`mk_freq.tsv`, top 40k ship in `mk.freq`) | CC BY-SA 4.0 (counts ship, text never ships) |
| MK Wikipedia title dump (`mkwiki-latest-all-titles-in-ns0`) | Proper-noun gazetteer (`mk_names.txt`), reviewed before commit | CC BY-SA 4.0 |

The base dictionary is GPL-2.0, so this project is **GPL-3.0-or-later**.

A note on the upstream dictionary: it is clean (261,460 unique forms, exactly the
31 Macedonian letters, no Latin or foreign-Cyrillic contamination) but
unmaintained — its `.aff` file has no affix rules at all and its `TRY` line lists
the *Russian* alphabet. We use the wordlist and ignore the rest.

## License

GPL-3.0-or-later. See the dictionary provenance above for why.
