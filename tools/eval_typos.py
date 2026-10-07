"""How many single-edit typos of frequent words does a lexicon fail to flag?

The counterweight to growing the lexicon: every added form can make some typo
look like a real word. Generates one deletion, substitution or transposition
per frequent word (fixed seed, so runs compare), checks them with the release
binary and prints the miss rate per lexicon. ASCII output only.

Usage: python tools/eval_typos.py data/mk.fst [other.fst ...]
"""
import json
import os
import random
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FREQ = os.path.join(ROOT, "data", "interim", "mk_freq.tsv")
BIN = os.path.join("target", "release", "mk.exe")
LETTERS = "абвгдѓежзѕијклљмнњопрстќуфхцчџш"


def typos(n_words=3000, seed=7):
    rnd = random.Random(seed)
    with open(FREQ, encoding="utf-8") as fh:
        words = [line.split("\t")[0] for line in fh][:n_words]
    out = set()
    for w in words:
        if len(w) < 5:
            continue
        i = rnd.randrange(len(w))
        kind = rnd.choice("dst")
        if kind == "d":
            t = w[:i] + w[i + 1:]
        elif kind == "s":
            t = w[:i] + rnd.choice(LETTERS) + w[i + 1:]
        else:
            i = min(i, len(w) - 2)
            t = w[:i] + w[i + 1] + w[i] + w[i + 2:]
        if t != w:
            out.add(t)
    return sorted(out)


def missed(fst, items):
    fd, tmp = tempfile.mkstemp(suffix=".txt")
    os.write(fd, " ".join(items).encode("utf-8"))
    os.close(fd)
    try:
        p = subprocess.run([BIN, "check", fst, "--file", tmp, "--json"],
                           capture_output=True, text=True, encoding="utf-8")
    finally:
        os.unlink(tmp)
    flagged = {d["text"] for d in json.loads(p.stdout or "[]")}
    return [t for t in items if t not in flagged]


def main():
    items = typos()
    for fst in sys.argv[1:]:
        m = missed(fst, items)
        print(f"{fst}: typos={len(items)} missed={len(m)} ({100 * len(m) / len(items):.1f}%)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
