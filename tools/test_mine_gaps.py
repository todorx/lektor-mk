"""Fixture self-check for mine_gaps.py: ordering and every guard.

Run: python tools/test_mine_gaps.py  (or under pytest)
"""
import bz2
import os
import tempfile

from mine_gaps import mine, tokens

def _write(name, data):
    path = os.path.join(tempfile.mkdtemp(), name)
    with open(path, "wb") as fh:
        fh.write(data)
    return path


HAVE = {"книга", "тој", "оди", "дома"}

TEXT = "\n".join([
    "Тој оди дома со референдумското мнозинство.",       # 2 new words; "со" is too short
    "референдумското мнозинство референдумското",          # counts: 3 vs 2
    "МПЦ ДДВ НАТО",                                         # all-caps: dropped
    "мaкeдoнски",                                           # Latin homoglyphs: dropped
    "ђубре ћерка",                                          # foreign Cyrillic line: dropped
    "Старословенскиот јазик ъ се пишуваше",                 # old orthography line: dropped
    "ок да",                                                # shorter than 3: dropped
    "црно-бел",                                             # hyphenated: dropped
    "„книга“, (книга).",                                    # known after punctuation strip
])


def test_guards():
    assert list(tokens("МПЦ ДДВ")) == []
    assert list(tokens("мaкeдoнски")) == []
    assert list(tokens("ђубре")) == []
    assert list(tokens("нешто ъ друго")) == []
    assert list(tokens("ок")) == []
    assert list(tokens("црно-бел")) == []
    assert list(tokens("„Книга“,")) == ["книга"]


def test_ranking_and_sources():
    txt = _write("a.txt", TEXT.encode("utf-8"))
    rows = mine([txt], HAVE, limit=10)
    assert rows == [
        ("референдумското", 3, "a.txt"),
        ("мнозинство", 2, "a.txt"),
    ]
    assert mine([txt], HAVE, limit=1) == [("референдумското", 3, "a.txt")]


def test_dump_reads_main_namespace_text_only():
    xml = (
        "<page><ns>0</ns><text>зборот зборот</text></page>\n"
        "<page><ns>4</ns><text>правилата</text></page>\n"
    )
    dump = _write("w.xml.bz2", bz2.compress(xml.encode("utf-8")))
    assert mine([dump], set(), limit=10) == [("зборот", 2, "w.xml.bz2")]


if __name__ == "__main__":
    test_guards()
    test_ranking_and_sources()
    test_dump_reads_main_namespace_text_only()
    print("ok")
