#!/usr/bin/env bash
# Fetch the Macedonian Hunspell dictionary and turn it into a UTF-8 wordlist.
#
# The upstream dictionary (OSSM / Taras Bendik, GPL-2.0) ships as a CP1251-encoded
# .dic with an empty .aff — there is no affix compression, so the file is already
# a flat list of surface forms. All we have to do is drop the count header and
# transcode it.
#
# Usage:  tools/prepare_wordlist.sh
# Output: data/interim/mk_wordlist.utf8.txt

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RAW_DIR="$REPO_ROOT/data/raw"
OUT_DIR="$REPO_ROOT/data/interim"
SRC="$RAW_DIR/dictionary-mk"
OUT="$OUT_DIR/mk_wordlist.utf8.txt"

mkdir -p "$RAW_DIR" "$OUT_DIR"

if [ ! -d "$SRC" ]; then
    echo "==> cloning upstream dictionary"
    git clone --depth 1 https://github.com/gerazov/dictionary-mk.git "$SRC"
else
    echo "==> using existing checkout at $SRC"
fi

DIC="$SRC/dict-mk/mk_MK.dic"
[ -f "$DIC" ] || { echo "error: $DIC not found" >&2; exit 1; }

echo "==> transcoding CP1251 -> UTF-8 (dropping the line-count header)"
tail -n +2 "$DIC" | iconv -f CP1251 -t UTF-8 | LC_ALL=C sort -u > "$OUT"

echo "==> wrote $OUT"
echo "    $(wc -l < "$OUT") unique forms"
echo "    $(du -h "$OUT" | cut -f1) on disk"
