"""Expand Macedonian Wiktionary (via kaikki.org) into forms and analyses.

Wiktionary's inflection tables cover ~68k entries and ~1.5M forms, against
Apertium's ~30k lemmas. This turns them into the two build inputs the engine
already understands:

  data/interim/mk_wiktionary_forms.txt   one surface form per line -> build-lexicon
  data/interim/mk_wiktionary_morph.tsv   form TAB lemma TAB tags   -> build-morph

Tags are mapped onto the Apertium names the grammar rules read (`n,f,sg,nom,def`,
`vblex,perf,tv,aor,p1,sg`, `lp`, `pp`, `ct` ...). Analyses are written only for
forms the Apertium table does not already analyse, so every word the rules
could judge before is judged exactly as before; Wiktionary only fills gaps.
Entries whose gender (nouns) cannot be read are kept for spelling only;
entries marked nonstandard, dialectal, regional, archaic or obsolete in every
sense are dropped, because accepting them would hide typos.

Source: https://kaikki.org/dictionary/Macedonian/ (Wiktextract over English
Wiktionary, CC BY-SA 4.0 + GFDL). Download kaikki.org-dictionary-Macedonian.jsonl
to data/raw/kaikki-mk.jsonl (~290 MB, gitignored) and run:

  python tools/expand_wiktionary.py
"""
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RAW = os.path.join(ROOT, "data", "raw", "kaikki-mk.jsonl")
APERTIUM_MORPH = os.path.join(ROOT, "data", "interim", "mk_morph.tsv")
OUT_FORMS = os.path.join(ROOT, "data", "interim", "mk_wiktionary_forms.txt")
OUT_MORPH = os.path.join(ROOT, "data", "interim", "mk_wiktionary_morph.tsv")

# One word in Macedonian Cyrillic, capital allowed for names.
WORD = re.compile(r"[А-ШЃЅЈЉЊЌЏЀЍа-шѓѕјљњќџѐѝ]+")
STRESS = "́"
# Rows that are not inflected forms of the headword.
NOT_FORMS = {"romanization", "table-tags", "inflection-template", "canonical",
             "multiword-construction", "comparative", "superlative",
             "abstract-noun", "adverb", "diminutive", "alternative"}

# An entry whose every sense carries one of these is not standard Macedonian:
# accepting it would hide typos (`праи` for `прави`, `глеа` for `гледа`).
MARKED = {"nonstandard", "misspelling", "dialectal", "regional", "archaic", "obsolete"}

GENDER = {"m": "m", "f": "f", "n": "nt"}
DEF = {"unspecified": "def", "proximal": "prx", "distal": "dst"}
PERSON = {"first-person": "p1", "second-person": "p2", "third-person": "p3"}


def clean(form):
    form = (form or "").replace(STRESS, "")
    return form if WORD.fullmatch(form) else None


def definiteness(tags):
    if "indefinite" in tags:
        return "ind"
    if "definite" in tags:
        return next((v for k, v in DEF.items() if k in tags), None)
    return None


def gender_number(tags):
    """Adjective-style columns: masculine/feminine/neuter singular, or plural."""
    if "plural" in tags:
        return "mfn", "pl"
    for k, g in (("masculine", "m"), ("feminine", "f"), ("neuter", "nt")):
        if k in tags:
            return g, "sg"
    return None


def noun_gender(entry):
    for h in entry.get("head_templates", []):
        if h.get("name") == "mk-noun":
            g = (h.get("args") or {}).get("1", "")
            if g[:1] in GENDER:
                return GENDER[g[:1]], g.endswith("-p")
    for f in entry.get("forms", []):
        tags = f.get("tags", [])
        if "canonical" in tags:
            for k, g in (("masculine", "m"), ("feminine", "f"), ("neuter", "nt")):
                if k in tags:
                    return g, "plural" in tags
    return None


def noun_tags(tags, gender):
    number = "ct" if "count-form" in tags else "pl" if "plural" in tags else "sg" if "singular" in tags else None
    if number is None:
        return None
    if "vocative" in tags:
        return ["n", gender, number, "voc"]
    d = definiteness(tags)
    if number == "ct":
        return ["n", gender, "ct"]
    return ["n", gender, number, "nom", d] if d else None


def adj_tags(tags, head="adj"):
    gn, d = gender_number(tags), definiteness(tags)
    if not gn or not d:
        return None
    return [head, gn[0], gn[1], "nom", d]


def verb_aspect(entry):
    for f in entry.get("forms", []):
        tags = f.get("tags", [])
        if "canonical" in tags:
            p, i = "perfective" in tags, "imperfective" in tags
            return "perf" if p and not i else "impf" if i and not p else None
    return None


def verb_valency(entry):
    seen = {t for s in entry.get("senses", []) for t in s.get("tags", [])}
    return "tv" if "transitive" in seen else "iv" if "intransitive" in seen else None


def verb_tags(tags, base):
    person = next((v for k, v in PERSON.items() if k in tags), None)
    number = "pl" if "plural" in tags else "sg" if "singular" in tags else None
    if "imperative" in tags:
        # Apertium marks the imperative by number only, not person.
        return base + ["imp", number] if person == "p2" and number else None
    if person and number:
        if "present" in tags:
            return base + ["pres", person, number]
        if "imperfect" in tags:
            return base + ["pii", person, number]
        if "aorist" in tags:
            return base + ["aor", person, number]
        return None
    if "adverbial" in tags and "participle" in tags:
        return base + ["pprs", "adv"]
    if "noun-from-verb" in tags:
        return None  # verbal noun: spelling only
    if "imperfect" in tags or "aorist" in tags:
        if "participle" in tags:
            return None  # -н/-но participle heads; the declined table below covers pp
        gn = gender_number(tags)
        return base + ["lp", gn[0], gn[1]] if gn else None
    # The declined adjectival participle table under a verb entry.
    if "definite" in tags or "indefinite" in tags:
        gn, d = gender_number(tags), definiteness(tags)
        return base + ["pp", gn[0], gn[1], d] if gn and d else None
    return None


def is_standard(entry):
    senses = entry.get("senses") or [{}]
    return not all(MARKED & set(s.get("tags", [])) for s in senses)


def analyses(entry):
    """(surface, tags) pairs for the forms of one entry; tags None = spelling only."""
    pos, lemma = entry.get("pos"), entry.get("word")
    rows = []
    head = clean(lemma)
    if pos == "noun":
        g = noun_gender(entry)
        gender = g[0] if g else None
    elif pos == "verb":
        base = [t for t in ("vblex", verb_aspect(entry), verb_valency(entry)) if t]
    for f in entry.get("forms", []):
        tags = set(f.get("tags", []))
        form = clean(f.get("form"))
        if not form or tags & NOT_FORMS:
            continue
        mapped = None
        if pos == "noun" and gender:
            mapped = noun_tags(tags, gender)
        elif pos == "adj":
            mapped = adj_tags(tags)
        elif pos == "verb":
            mapped = verb_tags(tags, base)
        rows.append((form, mapped))
    if head:
        rows.append((head, None))
    return rows


def main():
    if not os.path.exists(RAW):
        raise SystemExit(f"WIKT-FAILED missing {RAW} (see module docstring)")
    analysed = set()
    if os.path.exists(APERTIUM_MORPH):
        with open(APERTIUM_MORPH, encoding="utf-8") as fh:
            analysed = {line.split("\t", 1)[0] for line in fh}
    forms, morph = set(), set()
    with open(RAW, encoding="utf-8") as fh:
        for line in fh:
            entry = json.loads(line)
            if entry.get("lang_code") != "mk" or not is_standard(entry):
                continue
            for form, tags in analyses(entry):
                forms.add(form)
                if tags and form not in analysed:
                    morph.add((form, entry["word"].replace(STRESS, ""), ",".join(tags)))
    with open(OUT_FORMS, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(sorted(forms)) + "\n")
    with open(OUT_MORPH, "w", encoding="utf-8", newline="\n") as fh:
        fh.writelines(f"{f}\t{l}\t{t}\n" for f, l, t in sorted(morph))
    new_lemmas = len({l for _, l, _ in morph})
    print(f"forms={len(forms)} new_analyses={len(morph)} lemmas_with_new_analyses={new_lemmas}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
