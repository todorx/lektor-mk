"""Probe the checker over a word list file, printing compact ASCII lines.

Usage: python tools/probe.py data/mk.fst tools/probe_words.txt [data/mk.morph]
Words shown by index (Windows-console-safe); rules as CSV.
"""
import json
import subprocess
import sys
import tempfile
import os

fst, words_path = sys.argv[1], sys.argv[2]
morph = sys.argv[3] if len(sys.argv) > 3 else None
with open(words_path, encoding="utf-8") as fh:
    text = fh.read()

fd, tmp = tempfile.mkstemp(suffix=".txt")
os.write(fd, text.encode("utf-8"))
os.close(fd)
cmd = ["target/release/mk.exe", "check", fst]
if morph:
    cmd += ["--morph", morph]
cmd += ["--file", tmp, "--json"]
p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
os.unlink(tmp)
try:
    diags = json.loads(p.stdout or "[]")
except json.JSONDecodeError:
    print("MK-FAILED")
    print(ascii((p.stdout + p.stderr)[-300:]))
    sys.exit(1)
words = text.split()
idx = {}
for i, w in enumerate(words):
    idx.setdefault(w, []).append(i)
print(f"words={len(words)} flags={len(diags)}")
for d in diags:
    print(f"{idx.get(d['text'], ['?'])[0]}:{d['rule']}")
