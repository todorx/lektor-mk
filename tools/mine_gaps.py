"""Mine lexicon-coverage gaps from Macedonian corpora (review-only output).

Counts every Macedonian word in the given corpora, drops the ones any current
lexicon input already knows, and writes the rest ranked by frequency to
data/interim/gap_candidates.tsv (word TAB count TAB source). Nothing here
ships and nothing is written to a curated file: a human routes each row to
the gazetteer, to Apertium paradigm work, or (for a whole category only) to
mk_supplement.txt. Reviewed keeps go to data/supplement/mk_corpus_forms.txt,
rejects to tools/gap_rejected.txt. See
docs/superpowers/specs/2026-09-21-gap-miner-design.md.

Usage:
  python tools/mine_gaps.py CORPUS [CORPUS ...] [--limit N] [--out PATH]

A CORPUS is a plain UTF-8 .txt file, a MediaWiki XML dump (.xml or
.xml.bz2, e.g. mkwikisource-latest-pages-articles.xml.bz2 from
https://dumps.wikimedia.org/mkwikisource/latest/), of which only ns-0 page
text is read, or a CLASSLA VERT file (.vert or .vert.gz, e.g.
CLASSLA-web.mk.1.0.vert.gz from https://hdl.handle.net/11356/1932), of
which only Macedonian paragraphs are read; CLASSLA-web.mk.2.0.vert.tar.gz
(https://hdl.handle.net/11356/2079) is streamed without extracting.
Lexicon inputs are located like build_gazetteer.py's (MK_GAZETTEER_* env
overrides apply).
"""
import argparse
import bz2
import gzip
import os
import re
import sys
import tarfile
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
# Candidates a reviewer has already rejected (Serbianisms, dialect, typos,
# fragments). Grow the file with the diff that rejects them, so the reason is
# reviewable.
REJECTED_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "gap_rejected.txt")
with open(REJECTED_PATH, encoding="utf-8") as _fh:
    REJECTED = frozenset(l.strip() for l in _fh if l.strip() and not l.startswith("#"))

SPLIT = re.compile(r"[^\w-]+")

MIN_LEN, MAX_LEN = 3, 32

# Wikitext that would otherwise mint fake words: a template name ({{знамеикона|
# ...}}) or parameter name (| заглавје = ...) is not vocabulary, and a link or
# bold run with a suffix glued on ([[Македонија|македонск]]ите, '''Скопје'''то)
# must stay one word.
TEMPLATE_NAME = re.compile(r"\{\{[^|{}]*")
PARAM_NAME = re.compile(r"\|[^|=\[\]{}]*=")
LINK = re.compile(r"\[\[(?:[^\]|]*\|)?([^\]|]*)\]\]")
QUOTES = re.compile(r"'{2,}")


def wikitext(line):
    """A dump line with the markup that splits or invents words removed."""
    line = PARAM_NAME.sub(" ", TEMPLATE_NAME.sub(" ", line))
    return QUOTES.sub("", LINK.sub(r"\1", line))


def vert_sentences(fh):
    """Sentences of a VERT file, rebuilt from the word column of its token lines.

    Paragraphs CLASSLA 1.0 tagged with any lang other than "mk" are skipped:
    short fragments it could not identify, or Bulgarian/Serbian text whose
    words would otherwise rank as Macedonian gaps. 2.0 carries no paragraph
    lang because it already dropped those paragraphs, so untagged ones count.
    """
    in_mk = False
    words = []
    for line in fh:
        if line.startswith("<"):
            if line.startswith("<p "):
                in_mk = 'lang="mk"' in line or "lang=" not in line
            elif line.startswith("</s>") and words:
                yield " ".join(words)
                words = []
            continue
        if in_mk:
            words.append(line.split("\t", 1)[0])


def lines_of(path):
    """Yield the text lines of one corpus file (ns-0 page text only for dumps)."""
    if not os.path.exists(path):
        raise SystemExit(f"GAPS-FAILED missing corpus {path}")
    if path.endswith(".tar.gz"):
        # Streamed member by member: CLASSLA 2.0's one VERT member is 55 GB.
        with tarfile.open(path, "r|gz") as tar:
            for member in tar:
                if member.isfile() and member.name.endswith(".vert"):
                    raw = tar.extractfile(member)
                    # Decoded per line: a stream-mode member is not seekable,
                    # which io.TextIOWrapper requires on Python 3.14.
                    yield from vert_sentences(b.decode("utf-8", "replace") for b in raw)
        return
    opener = {".bz2": bz2.open, ".gz": gzip.open}.get(os.path.splitext(path)[1], open)
    name = os.path.basename(path)
    is_dump = ".xml" in name
    with opener(path, "rt", encoding="utf-8", errors="replace") as fh:
        if ".vert" in name:
            yield from vert_sentences(fh)
            return
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
                yield wikitext(line)
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
        if not MIN_LEN <= len(w) <= MAX_LEN or w in REJECTED:
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
