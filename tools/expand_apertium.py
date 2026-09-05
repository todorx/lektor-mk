#!/usr/bin/env python3
"""Expand the Apertium Macedonian dictionary into a table of surface forms.

The upstream Hunspell wordlist is a frozen list of forms with no affix rules, so
it misses correctly-formed Macedonian it never happened to enumerate
(`референдумското`, `фонологијата`). Apertium's dictionary is the opposite: 261
paradigms over 30,750 lemmas, from which every form can be generated along with
its lemma and morphological tags.

That gives us two things at once — much better spelling coverage, and the
lemma+tag analyses that grammar rules will need.

Apertium ships `lt-expand` for this, but it means building lttoolbox from
source. The format is plain XML and the expansion is a few dozen lines, so we do
it ourselves and keep the pipeline dependency-free.

Format
------
    <pardef n="куче__n">
      <e><p><l>е</l>  <r>е<s n="n"/><s n="nt"/><s n="sg"/></r></p></e>
      <e><p><l>иња</l><r>е<s n="n"/><s n="nt"/><s n="pl"/></r></p></e>
    </pardef>

    <e lm="куче"><i>куч</i><par n="куче__n"/></e>

`<l>` is the surface side, `<r>` the lemma side plus tags, `<i>` a part shared
by both. A paradigm entry may itself reference another paradigm, in which case
the readings multiply.

Usage:  tools/expand_apertium.py <dictionary.dix> <out.tsv>
Output: TSV of  surface <TAB> lemma <TAB> tags(comma-separated)
"""

from __future__ import annotations

import sys
import xml.etree.ElementTree as ET
from collections import Counter

# Guards against a pathological or cyclic paradigm blowing up the expansion.
MAX_FORMS_PER_ENTRY = 20_000
MAX_DEPTH = 12


def side_text(elem: ET.Element | None) -> str:
    """Concatenate the text of an <l> or <r>, treating <b/> as a space."""
    if elem is None:
        return ""
    out = [elem.text or ""]
    for child in elem:
        if child.tag == "b":
            out.append(" ")
        # <s> carries tags, not characters; <g> groups without adding text.
        out.append(child.tail or "")
    return "".join(out)


def side_tags(elem: ET.Element | None) -> list[str]:
    """Collect <s n="..."/> tag names from an <r>."""
    if elem is None:
        return []
    return [s.get("n", "") for s in elem.iter("s") if s.get("n")]


class Dictionary:
    def __init__(self, path: str):
        self.root = ET.parse(path).getroot()
        self.pardefs: dict[str, ET.Element] = {
            p.get("n", ""): p for p in self.root.iter("pardef")
        }
        self._cache: dict[str, list[tuple[str, str, tuple[str, ...]]]] = {}
        self.stats: Counter[str] = Counter()

    def expand_pardef(self, name: str, depth: int = 0) -> list[tuple[str, str, tuple[str, ...]]]:
        """All (surface_suffix, lemma_suffix, tags) triples a paradigm produces."""
        if name in self._cache:
            return self._cache[name]
        if depth > MAX_DEPTH or name not in self.pardefs:
            self.stats["missing_pardef"] += name not in self.pardefs
            return []

        # Seed the cache before recursing so a cycle terminates rather than hangs.
        self._cache[name] = []
        out: list[tuple[str, str, tuple[str, ...]]] = []
        for entry in self.pardefs[name].findall("e"):
            out.extend(self.expand_entry(entry, depth + 1))
            if len(out) > MAX_FORMS_PER_ENTRY:
                self.stats["truncated_pardef"] += 1
                break
        self._cache[name] = out
        return out

    def expand_entry(
        self, entry: ET.Element, depth: int = 0
    ) -> list[tuple[str, str, tuple[str, ...]]]:
        """Expand one <e>, multiplying out any paradigm references it contains.

        Children are processed in document order: <i> contributes to both sides,
        <p> splits them, and <par> substitutes another paradigm's readings.
        """
        combos: list[tuple[str, str, tuple[str, ...]]] = [("", "", ())]

        for child in entry:
            if child.tag == "i":
                shared = side_text(child)
                combos = [(l + shared, r + shared, t) for l, r, t in combos]
            elif child.tag == "p":
                left = side_text(child.find("l"))
                right = side_text(child.find("r"))
                tags = tuple(side_tags(child.find("r")))
                combos = [(l + left, r + right, t + tags) for l, r, t in combos]
            elif child.tag == "par":
                sub = self.expand_pardef(child.get("n", ""), depth + 1)
                if not sub:
                    return []
                combos = [
                    (l + sl, r + sr, t + st) for l, r, t in combos for sl, sr, st in sub
                ]
                if len(combos) > MAX_FORMS_PER_ENTRY:
                    self.stats["truncated_entry"] += 1
                    return combos[:MAX_FORMS_PER_ENTRY]
            elif child.tag in ("re", "s", "b", "g"):
                # <re> is a regular-expression entry (not an enumerable form).
                if child.tag == "re":
                    return []

        return combos

    def surface_forms(self) -> list[tuple[str, str, tuple[str, ...]]]:
        """Every (surface, lemma, tags) triple in the main section."""
        results = []
        for section in self.root.iter("section"):
            if section.get("id") != "main":
                continue
            for entry in section.findall("e"):
                # Entries marked as ignorable, or with no lemma, are not forms
                # we want to teach the spell-checker.
                if entry.get("i") == "yes":
                    continue
                lemma = entry.get("lm")
                if not lemma:
                    self.stats["no_lemma"] += 1
                    continue
                expanded = self.expand_entry(entry)
                if not expanded:
                    self.stats["unexpandable"] += 1
                    continue
                for surface, _lemma_side, tags in expanded:
                    surface = surface.strip()
                    if surface:
                        results.append((surface, lemma, tags))
                self.stats["entries_expanded"] += 1
        return results


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    src, dest = sys.argv[1], sys.argv[2]

    d = Dictionary(src)
    print(f"paradigms : {len(d.pardefs)}")

    forms = d.surface_forms()
    unique = {(s, l, t) for s, l, t in forms}
    distinct_surfaces = {s for s, _, _ in forms}

    with open(dest, "w", encoding="utf-8") as fh:
        for surface, lemma, tags in sorted(unique):
            fh.write(f"{surface}\t{lemma}\t{','.join(tags)}\n")

    print(f"entries   : {d.stats['entries_expanded']}")
    print(f"analyses  : {len(unique)}")
    print(f"surfaces  : {len(distinct_surfaces)} distinct")
    print(f"wrote     : {dest}")
    for key in ("no_lemma", "unexpandable", "missing_pardef", "truncated_entry"):
        if d.stats[key]:
            print(f"  skipped {key}: {d.stats[key]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
