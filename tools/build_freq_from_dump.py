"""Download the MK Wikipedia article dump and count word frequencies (ns-0 only).

Writes data/interim/mk_freq.tsv (word TAB count, top 100k). Deletes the
300MB download afterwards. Run in background: it takes several minutes.
Usage: python tools/build_freq_from_dump.py
"""
import bz2
import os
import re
import sys
import urllib.request
from collections import Counter

UA = {'User-Agent': 'macedonian-text/0.1 (local freq)'}
URL = 'https://dumps.wikimedia.org/mkwiki/latest/mkwiki-latest-pages-articles.xml.bz2'
CYR = re.compile(r'[Ѐ-Џа-шѓќѕџјљњќѓѐѝ]+')
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, 'data', 'raw', 'mkwiki-articles.xml.bz2')
OUT = os.path.join(ROOT, 'data', 'interim', 'mk_freq.tsv')


def download():
    if os.path.exists(RAW):
        print('reuse', RAW, flush=True)
        return
    os.makedirs(os.path.dirname(RAW), exist_ok=True)
    tmp = RAW + '.part'
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
    download()
    c: Counter[str] = Counter()
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
                for w in CYR.findall(line.lower()):
                    if 2 <= len(w) <= 32:
                        c[w] += 1
            if '</text>' in line:
                in_text = False
    with open(OUT, 'w', encoding='utf-8') as fh:
        # Top 40k: the blob stays ~0.9MB so fst+morph+freq fit the 5MB
        # extension budget. Rarer words tie at ~0 and add nothing.
        for w, n in c.most_common(40_000):
            fh.write(f'{w}\t{n}\n')
    print(f'types={len(c)} kept={min(len(c), 40_000)} -> {OUT}', flush=True)
    os.remove(RAW)
    print('download deleted', flush=True)


if __name__ == '__main__':
    sys.exit(main())
