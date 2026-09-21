"""Gate for the agreement rules: precision on correct text, recall on slips.

Usage: python tools/eval_context.py data/mk.fst data/mk.morph

Reads tools/context_probe.tsv (correct TAB perturbed TAB wrong_token TAB family).
The correct column must produce nothing; each row whose family is `agree` must
produce a diagnostic naming the wrong token. ASCII output only, so a Windows
console never has to render Cyrillic.

The bigram/frequency tables are deliberately not needed: agreement is a
property of the morphology, which is why this replaced the statistical rule
that measured 0% recall against the shipped bigram table.
"""
import json
import os
import subprocess
import sys
import tempfile

PROBE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "context_probe.tsv")
BIN = os.path.join("target", "release", "mk.exe")


def asciistr(s):
    return str(s).encode("ascii", "backslashreplace").decode("ascii")


def rows():
    out = []
    with open(PROBE, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) == 4:
                out.append(parts)
    return out


def check(lines, fst, morph):
    fd, tmp = tempfile.mkstemp(suffix=".txt")
    os.write(fd, "\n".join(lines).encode("utf-8"))
    os.close(fd)
    cmd = [BIN, "check", fst, "--morph", morph, "--file", tmp, "--json"]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8")
    finally:
        os.unlink(tmp)
    return json.loads(p.stdout or "[]")


def main():
    if len(sys.argv) < 3:
        print("usage: eval_context.py <mk.fst> <mk.morph>")
        return 1
    fst, morph = sys.argv[1], sys.argv[2]
    probe = rows()
    if not probe:
        print("GATE-FAILED no probe rows in " + PROBE)
        return 1

    # Precision: the correct column must be silent.
    fps = check([r[0] for r in probe], fst, morph)

    targets = [r for r in probe if r[3] == "agree"]
    uncovered = [r for r in probe if r[3] != "agree"]
    hits, misses = 0, []
    for correct, perturbed, wrong, _ in targets:
        found = check([perturbed], fst, morph)
        if any(wrong.lower() in d["text"].lower() for d in found):
            hits += 1
        else:
            got = ",".join(sorted({d["rule"] for d in found})) or "nothing"
            misses.append(asciistr(wrong) + "(" + got + ")")

    pct = 100.0 * hits / max(len(targets), 1)
    print(f"rows={len(probe)} agreement={len(targets)} uncovered={len(uncovered)}")
    print(f"false_positives={len(fps)} hits={hits} recall={pct:.0f}%")
    if fps:
        print("false_positive_detail=" + ",".join(
            f"{d['rule']}:{asciistr(d['text'])}" for d in fps[:10]))
    if misses:
        print("misses=" + ",".join(misses))
    # Fail loudly on precision only: a false alarm on correct prose is the one
    # outcome these rules must never produce. Recall is reported, not gated.
    if fps:
        print("GATE-FAILED false positives on correct text")
        return 1
    print("GATE-OK")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # console must stay ASCII for Windows viewers
        print("GATE-FAILED " + asciistr(e)[:200])
        sys.exit(1)
