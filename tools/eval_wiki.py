"""Evaluate the checker on live Macedonian Wikipedia extracts (ASCII summary).

Usage: python tools/eval_wiki.py data/mk.fst [data/mk.morph]
Prints: words, flags, flag rate, top flagged tokens by index-free count.
"""
import json
import subprocess
import sys
import tempfile
import os
import urllib.request
import urllib.parse

API = "https://mk.wikipedia.org/w/api.php"
TITLES = ["Македонија", "Скопје", "Македонски јазик"]


def fetch(title):
    q = urllib.parse.urlencode({
        "action": "query", "prop": "extracts", "explaintext": 1,
        "titles": title, "format": "json", "formatversion": 2,
    })
    with urllib.request.urlopen(
        urllib.request.Request(API + "?" + q, headers={"User-Agent": "macedonian-text/0.1 (local eval)"}),
        timeout=30) as r:
        d = json.load(r)
    return d["query"]["pages"][0].get("extract", "")


def main():
    fst = sys.argv[1]
    morph = sys.argv[2] if len(sys.argv) > 2 else None
    texts = [fetch(t) for t in TITLES]
    text = "\n".join(texts)
    words = text.split()
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
    from collections import Counter
    c = Counter(d["rule"] for d in diags)
    toks = Counter(d["text"] for d in diags if d["rule"] == "MK_SPELL")
    print(f"words={len(words)} flags={len(diags)} rate={100*len(diags)/max(len(words),1):.2f}%")
    print("rules=" + ",".join(f"{k}:{v}" for k, v in sorted(c.items())))
    print(f"distinct_spell_tokens={len(toks)}")
    top = toks.most_common(15)
    print("top_n=" + ",".join(str(n) for _, n in top))
    # token indexes into distinct list would need the words; keep counts only
    with open(os.path.join(tempfile.gettempdir(), "eval_toks.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(w for w, _ in top))


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # console must stay ASCII for Windows viewers
        print("EVAL-FAILED " + ascii(str(e))[:200])
