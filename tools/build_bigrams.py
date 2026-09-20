"""Download the MK Wikipedia article dump and count adjacent word pairs (ns-0 only).

Writes data/interim/mk_bigrams.tsv (prev TAB word TAB count, top 50k).
Deletes the 300MB download afterwards. Run in background: several minutes.
Usage: python tools/build_bigrams.py [keep]

Measured 2026-09-20: top 100k compiles to 3.00MB (over the ~5MB extension
budget), top 50k to 1.47MB at min-count 125 — hence the 50k default.
Counts ship in data/mk.bigram; article text never ships (CC BY-SA safe,
same logic as tools/build_freq_from_dump.py). Pairs never cross a line
boundary, so headings and list items do not glue together.
"""
import bz2
import os
import re
import sys
import urllib.request
from collections import Counter

UA = {'User-Agent': 'macedonian-text/0.1 (local bigrams)'}
URL = 'https://dumps.wikimedia.org/mkwiki/latest/mkwiki-latest-pages-articles.xml.bz2'
CYR = re.compile(r'[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]+')
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, 'data', 'raw', 'mkwiki-articles.xml.bz2')
OUT = os.path.join(ROOT, 'data', 'interim', 'mk_bigrams.tsv')


def download():
    if os.path.exists(RAW):
        print('reuse', RAW, flush=True)
        return
    os.makedirs(os.path.dirname(RAW), exist_ok=True)
    tmp = RAW + '.part'
    if os.path.exists(tmp):
        # Shares RAW with tools/build_freq_from_dump.py by design (sequential
        # runs reuse the 300MB download); a leftover .part means a concurrent
        # or crashed download — refuse rather than interleave bytes.
        print(f'refused: {tmp} exists (another download in progress? delete it if stale)',
              flush=True)
        raise SystemExit(1)
    req = urllib.request.Request(URL, headers=UA)
    with urllib.request.urlopen(req, timeout=120) as r, open(tmp, 'wb') as fh:
        total = int(r.headers.get('Content-Length', 0))
        got = 0
        while True:
            chunk = r.read(1 << 20)
            if not chunk:
                break
            fh.write(chunk)
            got += len(chunk)
            if got % (20 << 20) < (1 << 20):
                print(f'download {got}/{total}', flush=True)
    os.replace(tmp, RAW)


def main():
    keep = int(sys.argv[1]) if len(sys.argv) > 1 else 50_000
    download()
    c: Counter[tuple[str, str]] = Counter()
    in_main = False
    in_text = False
    with bz2.open(RAW, 'rt', encoding='utf-8', errors='replace') as fh:
        for line in fh:
            if '<ns>' in line:
                in_main = '<ns>0</ns>' in line
            if '<text' in line:
                in_text = True
            if in_main and in_text:
                # XML tags are Latin-only so the CYR regex skips them.
                words = [w for w in CYR.findall(line.lower()) if 2 <= len(w) <= 32]
                for a, b in zip(words, words[1:]):
                    c[(a, b)] += 1
            if '</text>' in line:
                in_text = False
    with open(OUT, 'w', encoding='utf-8') as fh:
        for (a, b), n in c.most_common(keep):
            fh.write(f'{a}\t{b}\t{n}\n')
    print(f'pairs={len(c)} kept={min(len(c), keep)} -> {OUT}', flush=True)
    os.remove(RAW)
    print('download deleted', flush=True)


if __name__ == '__main__':
    sys.exit(main())
