"""Mine lexicon-coverage gaps from Macedonian corpora (review-only output).

Counts every Macedonian word in the given corpora, drops the ones any current
lexicon input already knows, and writes the rest ranked by frequency to
data/interim/gap_candidates.tsv (word TAB count TAB source). Nothing here
ships and nothing is written to a curated file: a human routes each row to
the gazetteer, to Apertium paradigm work, or (for a whole category only) to
mk_supplement.txt. See docs/superpowers/specs/2026-09-21-gap-miner-design.md.

Usage:
  python tools/mine_gaps.py CORPUS [CORPUS ...] [--limit N] [--out PATH]

A CORPUS is a plain UTF-8 .txt file, or a MediaWiki XML dump (.xml or
.xml.bz2, e.g. mkwikisource-latest-pages-articles.xml.bz2 from
https://dumps.wikimedia.org/mkwikisource/latest/), of which only ns-0 page
text is read. Lexicon inputs are located like build_gazetteer.py's
(MK_GAZETTEER_* env overrides apply).
"""
import argparse
import bz2
import os
import re
import sys
from collections import Counter

from build_gazetteer import current_words, paths_from_env

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "data", "interim", "gap_candidates.tsv")

# Lowercase Macedonian letters only (plus the accented ѐ ѝ). Anything else
# inside a run splits it, so a Latin homoglyph never yields a "new word".
WORD = re.compile(r"[абвгдѓежзѕијклљмнњопрстќуфхцчџшѐѝ]+")
# Any Cyrillic letter at all, to tell a clean Macedonian token from a mixed one.
ANY_CYR = re.compile(r"[Ѐ-ӿ]")
# Letters of pre-1945 and foreign orthographies. A line containing one is in
# an older or a different standard, so every word on it is suspect, not just
# the word carrying the letter.
OLD_ORTHOGRAPHY = re.compile(r"[ъѣщыьэюяђћѫѧі]", re.IGNORECASE)
# Archaic or dialect spellings a reviewer has already rejected. Grow this with
# the diff that rejects them, so the reason is reviewable.
ARCHAIC = frozenset()

SPLIT = re.compile(r"[^\w-]+")

MIN_LEN, MAX_LEN = 3, 32


def lines_of(path):
    """Yield the text lines of one corpus file (ns-0 page text only for dumps)."""
    if not os.path.exists(path):
        raise SystemExit(f"GAPS-FAILED missing corpus {path}")
    opener = bz2.open if path.endswith(".bz2") else open
    is_dump = ".xml" in os.path.basename(path)
    with opener(path, "rt", encoding="utf-8", errors="replace") as fh:
        if not is_dump:
            yield from fh
            return
        in_main = in_text = False
        for line in fh:
            if "<ns>" in line:
                in_main = "<ns>0</ns>" in line
            if "<text" in line:
                in_text = True
            if in_main and in_text:
                yield line
            if "</text>" in line:
                in_text = False


def tokens(line):
    """Candidate words on one line, after every guard that does not need the lexicon."""
    if OLD_ORTHOGRAPHY.search(line):
        return
    # Split on anything that is not a letter, digit or hyphen, so markup and
    # punctuation fall away but a Latin letter inside a word keeps it whole
    # (and therefore rejectable).
    for raw in SPLIT.split(line):
        raw = raw.strip("-")
        if not raw or not ANY_CYR.search(raw):
            continue
        # All-caps tokens are acronyms or headings; neither is vocabulary.
        if raw.isupper():
            continue
        w = raw.lower()
        # Macedonian letters only. Hyphenated compounds are skipped: the
        # checker already accepts them when every part is known.
        if WORD.fullmatch(w) is None:
            continue
        if not MIN_LEN <= len(w) <= MAX_LEN or w in ARCHAIC:
            continue
        yield w


def mine(corpora, have, limit):
    """Ranked (word, count, source) rows for words missing from `have`."""
    counts = Counter()
    first_source = {}
    for path in corpora:
        source = os.path.basename(path)
        for line in lines_of(path):
            for w in tokens(line):
                if w in have:
                    continue
                counts[w] += 1
                first_source.setdefault(w, source)
    rows = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))
    return [(w, n, first_source[w]) for w, n in rows[:limit]]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("corpora", nargs="+")
    ap.add_argument("--limit", type=int, default=5000)
    ap.add_argument("--out", default=OUT)
    args = ap.parse_args(argv)

    have = current_words(paths_from_env())
    rows = mine(args.corpora, have, args.limit)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        for w, n, src in rows:
            fh.write(f"{w}\t{n}\t{src}\n")
    # ASCII only, so a Windows console never has to render Cyrillic.
    print(f"known={len(have)} candidates={len(rows)} -> {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
