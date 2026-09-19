"""Build a word-frequency table from a Wikipedia XML dump (counts only).

Usage: python tools/build_freq.py <pages-articles.xml> <out.tsv>
Output: TSV of word <TAB> count, top 100k Cyrillic tokens, 2-32 chars.
Only counts ship in data/mk.freq; article text never ships (CC BY-SA safe).
"""
import re
import sys
from collections import Counter

CYR = re.compile(r"[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]+")


def main() -> int:
    src, dest = sys.argv[1], sys.argv[2]
    c: Counter[str] = Counter()
    with open(src, encoding="utf-8") as fh:
        for line in fh:
            for w in CYR.findall(line.lower()):
                if 2 <= len(w) <= 32:
                    c[w] += 1
    with open(dest, "w", encoding="utf-8") as out:
        for w, n in c.most_common(100_000):
            out.write(f"{w}\t{n}\n")
    print(f"types={len(c)} kept={min(len(c), 100_000)} -> {dest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
