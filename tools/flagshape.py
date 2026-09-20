"""Classify MK_SPELL flags without printing words (ASCII-only summary).

Usage: python tools/flagshape.py data/mk.fst [data/mk.morph]
Fetches the same Wikipedia extracts as eval_wiki.py and reports the share of
flags on capitalized vs lowercase tokens and on short vs long tokens.
"""
import json
import subprocess
import sys
import tempfile
import os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
from eval_wiki import fetch, TITLES

fst = sys.argv[1]
morph = sys.argv[2] if len(sys.argv) > 2 else None
text = "\n".join(fetch(t) for t in TITLES)
fd, tmp = tempfile.mkstemp(suffix=".txt")
os.write(fd, text.encode("utf-8"))
os.close(fd)
cmd = ["target/release/mk.exe", "check", fst]
if morph:
    cmd += ["--morph", morph]
cmd += ["--file", tmp, "--json"]
p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
os.unlink(tmp)
diags = json.loads(p.stdout or "[]")
spells = [d["text"] for d in diags if d["rule"] == "MK_SPELL"]
n = max(len(spells), 1)
cap = sum(1 for w in spells if w[:1].isupper())
print(f"spell={len(spells)} capitalized={100*cap/n:.1f}%")
print(f"len<=3={100*sum(1 for w in spells if len(w)<=3)/n:.1f}%")
print(f"with_hyphen={100*sum(1 for w in spells if '-' in w)/n:.1f}%")
print(f"with_digit={100*sum(1 for w in spells if any(c.isdigit() for c in w))/n:.1f}%")
