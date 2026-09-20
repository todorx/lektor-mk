"""Build a proper-noun gazetteer from Macedonian Wikipedia link titles.

Titles that Macedonian articles link to are overwhelmingly people, places and
works — exactly the open class the frozen wordlist lacks. Only ns-0 titles
in Macedonian Cyrillic that are missing from every current lexicon input are
kept, so the file stays a curated category, not corpus-chased individuals.

Usage: python tools/build_gazetteer.py  (writes data/supplement/mk_names.txt)
"""
import json
import os
import re
import urllib.request
import urllib.parse

API = "https://mk.wikipedia.org/w/api.php"
UA = {"User-Agent": "macedonian-text/0.1 (local gazetteer)"}
# Hub articles whose outgoing links are dense with Macedonian names.
HUBS = ["Македонија", "Скопје", "Историја на Македонија",
        "Список на градови во Македонија", "Македонска книжевност"]
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Optional offline sources (free downloads, not in repo):
# - mkwiki-latest-all-titles-in-ns0.gz -> data/raw/mkwiki-titles.txt
#   https://dumps.wikimedia.org/mkwiki/latest/mkwiki-latest-all-titles-in-ns0.gz
# - GeoNames MK.zip -> data/raw/mk_geonames.txt, one name per line.
#   https://download.geonames.org/export/zip/MK.zip
#   Raw multi-column rows are skipped, not parsed (see run()).
DUMP_TITLES = os.path.join(ROOT, "data", "raw", "mkwiki-titles.txt")
GEONAMES = os.path.join(ROOT, "data", "raw", "mk_geonames.txt")
OUT = os.path.join(ROOT, "data", "supplement", "mk_names.txt")
# NOTE: the uppercase range must be written А-Ш (U+0410-U+0428), not Ѐ-Џ:
# Џ is U+040F, below А (U+0410), so Ѐ-Џ covers only the supplement capitals
# (Ѓ Ѕ Ј Љ Њ Ќ Џ...) and silently rejects every А-Я-initial word — which is
# nearly every proper noun. (Found 2026-09-20: 0/5699 kept names had А-Я.)
CYR = re.compile(r"^[Ѐ-ЏА-Ша-шѓќѕџјљњќѓѐѝ]{3,30}$")


def paths_from_env():
    """Input/output locations, overridable for offline testing (MK_GAZETTEER_*).

    Note: an empty-string value unsets the variable on some shells
    (PowerShell drops empty $env: entries), so blank also means "none".
    """
    hubs = os.environ.get("MK_GAZETTEER_HUBS")
    return {
        "titles": os.environ.get("MK_GAZETTEER_TITLES", DUMP_TITLES),
        "geonames": os.environ.get("MK_GAZETTEER_GEONAMES", GEONAMES),
        "out": os.environ.get("MK_GAZETTEER_OUT", OUT),
        "wordlist": os.environ.get(
            "MK_GAZETTEER_WORDLIST", os.path.join(ROOT, "data", "interim", "mk_wordlist.utf8.txt")),
        "supplement": os.environ.get(
            "MK_GAZETTEER_SUPPLEMENT", os.path.join(ROOT, "data", "supplement", "mk_supplement.txt")),
        "apertium": os.environ.get(
            "MK_GAZETTEER_APERTIUM", os.path.join(ROOT, "data", "interim", "mk_apertium_forms.txt")),
        "hubs": HUBS if hubs is None else [h.strip() for h in hubs.split(",") if h.strip()],
    }


def title_parts(title):
    """Split a Wikipedia title into candidate words.

    Dump titles use underscores for spaces and carry parenthetical
    disambiguation (`Ржаново_(Струшко)`); without normalizing both,
    the CYR filter rejects every multiword title.
    """
    title = title.replace("_", " ")
    title = re.sub(r"\(.*?\)", " ", title)
    return title.split()


def api(params):
    q = urllib.parse.urlencode(params)
    req = urllib.request.Request(API + "?" + q, headers=UA)
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def links(title):
    out, cont = [], {}
    while True:
        d = api({"action": "query", "prop": "links", "titles": title,
                 "plnamespace": 0, "pllimit": 500, "format": "json",
                 "formatversion": 2, **cont})
        page = d["query"]["pages"][0]
        out += [l["title"] for l in page.get("links", [])]
        if "continue" not in d:
            return out
        cont = d["continue"]


def current_words(paths, include_names=True):
    have = set()
    # The wordlist is the primary input: a missing file is a loud failure,
    # not an empty vocabulary that ingests everything.
    p = paths["wordlist"]
    with open(p, encoding="utf-8") as fh:
        have.update(l.strip().lower() for l in fh if l.strip() and not l.startswith("#"))
    names = (paths["supplement"], paths["out"]) if include_names else (paths["supplement"],)
    for p in names:
        if os.path.exists(p):
            with open(p, encoding="utf-8") as fh:
                have.update(l.strip().lower() for l in fh if l.strip() and not l.startswith("#"))
    p = paths["apertium"]
    if os.path.exists(p):
        with open(p, encoding="utf-8") as fh:
            have.update(l.strip().lower() for l in fh if l.strip())
    return have


def main():
    run(paths_from_env())


def run(paths):
    have = current_words(paths)
    seen, kept = set(), []

    def consider(w):
        """Classify one candidate word; returns 'new', 'known' or 'dropped'."""
        w = w.strip("«»\"'.,:;!?()")
        if not w:
            return "dropped"
        if w.lower() in have or w.lower() in seen:
            return "known" if w.lower() in have else "seen"
        seen.add(w.lower())
        if CYR.match(w) and not w.isupper():
            kept.append(w)
            return "new"
        return "dropped"

    def crawl_dumps():
        """Harvest dump files; returns (new_names, known_names, skipped_tab_lines)."""
        new, known, skipped_tabs = 0, 0, 0
        for path in (paths["titles"], paths["geonames"]):
            if not os.path.exists(path):
                continue
            is_geo = path == paths["geonames"]
            with open(path, encoding="utf-8") as fh:
                for line in fh:
                    line = line.strip()
                    if not line or line.startswith("#"):
                        continue
                    if is_geo and "\t" in line:
                        # Raw multi-column dumps (e.g. unprocessed GeoNames
                        # TSV) are skipped, not parsed: without knowing which
                        # column holds the name, any Cyrillic token in an
                        # auxiliary column would be ingested as a name.
                        skipped_tabs += 1
                        continue
                    for part in title_parts(line):
                        outcome = consider(part)
                        if outcome == "new":
                            new += 1
                        elif outcome == "known":
                            known += 1
        return new, known, skipped_tabs

    # Offline dumps are the primary source when present (deterministic,
    # offline); the hub crawl is fallback when no dump exists — or when the
    # dumps exist but contribute nothing at all (empty file: neither new nor
    # already-known names), which used to end the run silently. A re-run over
    # an already-ingested dump shows high `known` and correctly skips hubs.
    dump_files = [p for p in (paths["titles"], paths["geonames"]) if os.path.exists(p)]
    dump_new, dump_known, skipped_tabs = crawl_dumps()
    if not dump_files or (dump_new == 0 and dump_known == 0):
        if dump_files:
            print("GAZETTEER-WARN dumps yielded 0 new and 0 known names; falling back to hub crawl",
                  flush=True)
        for hub in paths["hubs"]:
            for title in links(hub):
                for part in title_parts(title):
                    consider(part)
    kept = sorted(set(kept))
    # Union the previously curated names back in (unless now covered by the
    # wordlist/paradigms/supplement, in which case they are redundant, not
    # lost): `have` above contains the old names file itself, so without this
    # the rewrite would drop every curated name the dump did not re-yield.
    out = paths["out"]
    old_names = []
    if os.path.exists(out):
        with open(out, encoding="utf-8") as fh:
            old_names = [l.strip() for l in fh if l.strip() and not l.startswith("#")]
    redundant = 0
    covered = current_words(paths, include_names=False) if old_names else set()
    if old_names:
        keep_set = set(kept)
        keep_lower = {w.lower() for w in keep_set}
        for w in old_names:
            if w.lower() in covered:
                redundant += 1
            elif w.lower() not in keep_lower:
                keep_set.add(w)
                keep_lower.add(w.lower())
        kept = sorted(keep_set)
    # Refuse only on genuine loss: every old name that is still uncovered
    # must survive in the output. Drops of now-redundant names are expected
    # cleanup, not shrinkage (the old count-based guard false-positived on
    # exactly those).
    keep_lower = {w.lower() for w in kept}
    lost = [w for w in old_names if w.lower() not in covered and w.lower() not in keep_lower]
    if lost:
        print(f"GAZETTEER-REFUSED lost={len(lost)} names, file untouched: {lost[:5]}")
        raise SystemExit(1)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("# Proper-noun gazetteer: Macedonian Wikipedia link titles\n")
        fh.write("# (people, places, works) absent from the wordlist and paradigms.\n")
        fh.write("# Generated by tools/build_gazetteer.py; review diffs before committing.\n")
        fh.write("\n".join(kept) + "\n")
    print(f"source={'dump' if dump_files else 'hubs'} candidates={len(seen)} "
          f"kept={len(kept)} redundant={redundant} skipped_tabs={skipped_tabs}")


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print("GAZETTEER-FAILED " + ascii(str(e))[:200])
        raise SystemExit(1)
