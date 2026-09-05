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
  ├─ [2] morphology      lemma + POS + features        (not yet built)
  ├─ [3] grammar rules   declarative, data-driven      (not yet built)
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
tools/              data preparation
data/               dictionary sources and the compiled lexicon (git-ignored)
extension/          browser extension (not yet built)
```

## Build

Requires Rust and a C linker (`sudo dnf install -y gcc glibc-devel` on Fedora).

```bash
tools/prepare_wordlist.sh                                        # fetch + transcode
cargo run --release -p mk-cli -- build-lexicon \
    data/interim/mk_wordlist.utf8.txt \
    data/supplement/mk_supplement.txt \
    data/mk.fst                                                  # compile the lexicon
cargo test                                                       # run the suite
cargo run --release -p mk-cli -- check data/mk.fst "Тој ја видe книгата."
```

## Measured

| | |
|---|---|
| Lexicon | 261,500 forms → **0.48 MB** FST (11× smaller than the raw list) |
| Throughput | 17,782 words in **0.95 s**; 40 words in 0.5 ms |
| Flag rate on Macedonian Wikipedia | 6.46% |

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

The two script rules matter more than they look. Latin `а е о с р х у` are pixel
twins of their Cyrillic counterparts, so contaminated text looks perfect to a
human while breaking every spell-checker, search index and sort order it touches.
Both rules confirm the repaired spelling against the lexicon before reporting,
which is what keeps false positives near zero.

## Planned grammar rules

These are the checks no generic tool can do, and the reason the project exists:

- **Definite article placement** — the article attaches to the first element of
  the noun phrase: `убавата книга` ✓, `убава книгата` ✗, `убавата книгата` ✗
- **Object reduplication** — definite objects require a resumptive clitic:
  `Ја видов Марија` ✓
- **Clitic order** — dative before accusative: `ми го даде` ✓, `го ми даде` ✗
- **`ѝ` vs `и`** — dative clitic against the conjunction
- **л-participle agreement** — `тој дошол` / `таа дошла` / `тие дошле`
- **Numeral forms** — `два стола` not `два столови`; `двајца студенти`
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

The base dictionary is GPL-2.0, so this project is **GPL-3.0-or-later**.

A note on the upstream dictionary: it is clean (261,460 unique forms, exactly the
31 Macedonian letters, no Latin or foreign-Cyrillic contamination) but
unmaintained — its `.aff` file has no affix rules at all and its `TRY` line lists
the *Russian* alphabet. We use the wordlist and ignore the rest.

## License

GPL-3.0-or-later. See the dictionary provenance above for why.
