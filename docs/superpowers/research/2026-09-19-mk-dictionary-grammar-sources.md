# Macedonian dictionary & grammar sources — licensing and acquisition research

Date: 2026-09-19. Project: GPL-3.0-or-later, local-only Macedonian spell/grammar checker.
Rule applied: shipped data must be GPL-compatible or cleanly separable (counts only, eval only).
All claims verified against primary sources the same day; every claim cites its URL.

## 1. drmj.manu.edu.mk / drmj.eu (Digital Dictionary of Macedonian)

**What it is.** `drmj.manu.edu.mk` is not itself a dictionary — it is a MANU (Macedonian Academy
of Sciences and Arts) WordPress portal, "Digital resources of the Macedonian language", that links out
to dictionaries, grammars, monographs, a corpus and a bibliography.
Source: https://drmj.manu.edu.mk/
The actual searchable dictionary ("Дигитален речник на македонскиот јазик") lives at
http://drmj.eu/ (mirrored via https://www.makedonski.info) and is a **private initiative financed and
operated by SAM97 GmbH**, compiled from Koneski's dictionary, the IMJ 6-volume explanatory dictionary,
grammars and an orthographic dictionary.
Source: https://www.makedonski.info/impressum

**API / bulk data.** None. No documented API; lookup is HTML only (`POST /search`, `/letter/<буква>`,
`/show/<збор>/<вид>`). The "API" page at https://www.makedonski.info/api is only a mobile-app promo
(no REST docs). A "Digital Macedonian Dictionary" Android app exists on Google Play. Internally the
pages load JSON-ish JS helpers, but there is no public endpoint or dump.

**Terms.** Two layers, both hostile to reuse:
- Portal footer: "Licensed under Creative Commons Attribution-NonCommercial 4.0".
  Source: https://drmj.manu.edu.mk/ — NC kills GPL compatibility outright.
- Dictionary impressum (the binding one): "digitized information … property of SAM97 GmbH and may not
  be copied"; "automatic access by software needs written approval"; threats of criminal complaint and
  damages. A carve-out exists for educational/scientific use with attribution (citing MK copyright law
  art. 52), and universities may request a free copy of the base version on application.
  Source: https://www.makedonski.info/impressum

**Scraping feasibility.** Technically trivial (static-ish HTML, letter-indexed pages). Legally, bulk
scraping plainly violates the written terms (explicit ban on copying + automated access without
permission).

**Verdict: (iii) not usable as shipped lexicon; (ii) eval-only at most — and even that only as manual
spot-checks** (or after written permission / university data request). Do not scrape in bulk.

## 2. Free dictionary sources for word lists

### 2a. Macedonian Wiktionary via Wiktextract/kaikki — BEST structured lexicon source
- Size: en-Wiktionary stats list Macedonian at ~66.8k entries / ~43.4k content pages.
  Source: https://en.wiktionary.org/wiki/Wiktionary:Statistics
- kaikki.org Macedonian dictionary: 66,828 distinct word forms; 37,159 noun, 20,211 verb, 12,159
  adjective senses; per-language postprocessed JSONL download (~290 MB, now deprecated in favour of the
  raw page); extracted 2026-09-16 from the en-Wiktionary dump of 2026-09-02 with wiktextract.
  Source: https://kaikki.org/dictionary/Macedonian/
- Raw bulk alternative (all languages, filter `lang_code == "mk"`): 22.9 GB JSONL / 2.6 GB .gz.
  Source: https://kaikki.org/dictionary/rawdata.html
- License: Wiktionary text is dual CC BY-SA 4.0 + GFDL.
  Source: https://en.wiktionary.org/wiki/Wiktionary:Copyrights
- GPL verdict: **shippable**. CC BY-SA 4.0 is one-way compatible into GPLv3 (Creative Commons /
  FSF, Nov 2015): a GPL-3.0-or-later project may incorporate CC BY-SA 4.0 material with attribution.
  Keep it as a separately-attributed data file; cite Wiktextract
  (http://www.lrec-conf.org/proceedings/lrec2022/pdf/2022.lrec-1.140.pdf) per kaikki's request.

### 2b. Hunspell mk (OSSM) — shippable IF license suffix checks out; NOT in LibreOffice
- Verified via the GitHub API root listing of `LibreOffice/dictionaries`: language dirs run
  `lv_LV` → `mn_MN` — **no `mk` directory exists upstream**. LibreOffice ships no Macedonian
  spell dictionary. Source: https://api.github.com/repos/LibreOffice/dictionaries/contents/
- The mk Hunspell dictionary comes from the Free Software Organisation of Macedonia (OSSM) OpenOffice
  extension, mirrored at https://github.com/gerazov/dictionary-mk (`mk_MK.dic` + `mk_MK.aff`,
  GitHub labels it GPL-2.0).
- GPL verdict: **conditional**. GPL-2.0-*only* is incompatible with a GPL-3.0-or-later project, while
  "v2 or later" is fine. The LICENSE file must be read before vendoring. (Elastic's hunspell index
  likewise records mk_MK as GPL-sourced: https://github.com/elastic/hunspell/blob/master/conf.yaml.)
  The `.aff` affix rules are the valuable part (productive morphology); `unmunch`/`hunspell -m`
  expands stems to full word-form lists.

### 2c. makedonski.gov.mk (Official digital explanatory dictionary) — eval-only
- ~100,000 words (with sub-headwords), normative; plus 1,300+ abbreviations and 1,200+ geographic
  terms; run by the government with the Krste Misirkov Institute.
  Source: https://makedonski.gov.mk/
- No license/terms statement found on the site → treat as all-rights-reserved. HTML lookup only.
  Verdict: **(ii) eval-only** (manual reference while writing entries/rules).

### 2d. Public-domain older dictionaries — none found
- Koneski (1921–1993; https://en.wikipedia.org/wiki/Bla%C5%BEe_Koneski) and the IMJ explanatory
  dictionary (2003–2014) are in copyright for decades under life+70. Miladinovci/Čepenkov folklore
  texts are public domain but are literature, not dictionaries. No PD explanatory dictionary of
  Macedonian was identified. (zoze.mk / Murgoski dictionaries linked from the MANU portal were not
  investigated; presume proprietary.)

## 3. Free grammar references for rule-writing

General principle: grammar *facts* (paradigms, agreement rules) are not copyrightable — any grammar
may be read and re-expressed as original checker rules; only verbatim text cannot be copied.

- **MANU portal grammar shelf** (free PDFs, read-only reference): Kepeski 1946, Koneski–Tošev 1950
  orthography, Lunt 1952, Mareš 1994, Usikova 2000, **Friedman 2001 "Macedonian"** (English, detailed —
  best single rule-writing reference), Topolinjska 2009 excerpt.
  Source: https://drmj.manu.edu.mk/%d0%b3%d1%80%d0%b0%d0%bc%d0%b0%d1%82%d0%b8%d0%ba%d0%b8-%d0%bd%d0%b0-%d0%bc%d0%b0%d0%ba%d0%b5%d0%b4%d0%be%d0%bd%d1%81%d0%ba%d0%b8%d0%be%d1%82-%d1%98%d0%b0%d0%b7%d0%b8%d0%ba/
  Portal-wide CC BY-NC; underlying 20th-c. works remain in copyright — use as references, and for
  eval sentences under the portal's educational clause at most.
- **Wikipedia "Macedonian grammar"** — CC BY-SA 4.0 (site-wide; cf.
  https://en.wikipedia.org/wiki/Category:Macedonian_grammar). Same GPLv3 one-way-compat as Wiktionary.
  Covers the checkable headline rules: obligatory clitic doubling with definite objects, triple
  definite article, no infinitive (да-constructions), ќе future, има/нема perfect.
  Source: https://en.wikipedia.org/wiki/Macedonian_grammar — good rule checklist.
- **UD Macedonian-MTB** (~1K sentences, grammar examples from Sazdov's textbook + Cairo CICLing;
  https://github.com/UniversalDependencies/UD_Macedonian-MTB). LICENSE.txt verified as
  **CC BY-SA 4.0** (https://raw.githubusercontent.com/UniversalDependencies/UD_Macedonian-MTB/master/LICENSE.txt) —
  but the README prose says "CC Attribution-NonCommercial 4.0" while its own metadata block says
  CC BY-SA 4.0. **Resolve this discrepancy with the maintainer before use**; too small for training
  anyway, useful only as annotated eval examples.
- **CLASSLA-web.mk** (https://www.clarin.si/repository/xmlui/handle/11356/1932): 557M tokens /
  479M words / 1.48M texts of crawled .mk/.мкд web text, already tokenized + lemmatized with the
  CLASSLA-Stanza pipeline, genre-labelled. **Licensed CC0** (verified on the repository page) — fully
  GPL-compatible, including shipping derived frequency lists. 4.48 GB VERT download; v2.0 (2024 crawl,
  complementary, ~20% overlap) at http://hdl.handle.net/11356/2079. Caveat: noisy web text, needs
  filtering; VERT format needs parsing (Sketch Engine/CWB style).
- Unverified / tools: `clarinsi/classla` Python pipeline supports mk tokenize/POS/lemma/parse
  (https://github.com/clarinsi/classla) — license not checked here; FlexiMac verb conjugator and the
  VIGNA aspectual conjugator (linked from https://www.makedonski.info) could generate verb paradigms —
  licenses not checked here.

## 4. Recommendations (value/effort order)

1. **CLASSLA-web.mk 1.0 (CC0) — frequency list + eval sentences.**
   URL: https://www.clarin.si/repository/xmlui/handle/11356/1932 (file `CLASSLA-web.mk.1.0.vert.gz`,
   4.48 GB). License verdict: CC0 — shippable, no attribution required (cite Ljubešić et al. 2024
   anyway). How-to: download, parse VERT (one token per line with lemma + MSD tags already annotated),
   count lemma/word-form frequencies, keep top-N forms as the frequency backbone and mine real
   sentences as grammar-rule eval cases; filter short/boilerplate paragraphs via the shipped quality
   labels.
2. **kaikki Macedonian JSONL (CC BY-SA 4.0) — structured lemma + forms + POS bootstrap.**
   URL: https://kaikki.org/dictionary/Macedonian/ (per-language `.jsonl`, ~290 MB) or the raw dump
   page https://kaikki.org/dictionary/rawdata.html. License verdict: shippable inside GPL-3.0-or-later
   via CC BY-SA 4.0 → GPLv3 one-way compatibility, with attribution + a provenance note. How-to:
   stream JSONL, keep entries with `lang_code == "mk"`, extract (form, POS, inflection tags) triples;
   cross-check coverage against the CLASSLA frequency list from (1) to find missing lemmas.
3. **OSSM Hunspell mk (`mk_MK.dic/.aff`) — productive morphology, PENDING license check.**
   URL: https://github.com/gerazov/dictionary-mk. License verdict: ship only if the LICENSE file says
   "version 2 or later" (or the author relicenses); GPL-2.0-only cannot go into a GPL-3.0-or-later
   codebase. How-to: unzip the `.oxt`, read `.dic/.aff`, run `unmunch` to expand all inflected forms,
   diff against (1)+(2) for coverage measurement; regardless of the vendoring outcome it is a free
   coverage-eval oracle.

Explicitly deferred: any bulk use of drmj.eu/makedonski.info (proprietary, anti-scraping terms —
manual eval lookups only); UD-MTB until the BY-NC vs BY-SA contradiction is clarified.
