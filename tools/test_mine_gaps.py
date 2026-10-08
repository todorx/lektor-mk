"""Fixture self-check for mine_gaps.py: ordering and every guard.

Run: python tools/test_mine_gaps.py  (or under pytest)
"""
import bz2
import gzip
import io
import os
import tarfile
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


def test_dump_markup_neither_splits_nor_invents_words():
    xml = (
        "<page><ns>0</ns><text>{{знамеикона|Југославија}} "
        "[[Македонија|македонск]]ите '''Скопје'''то [[Охрид]]ска\n"
        "| заглавје = Песна</text></page>\n"
    )
    dump = _write("m.xml.bz2", bz2.compress(xml.encode("utf-8")))
    assert {w for w, _, _ in mine([dump], set(), limit=10)} == {
        "југославија", "македонските", "охридска", "скопјето", "песна",
    }


def test_vert_reads_mk_paragraphs_only():
    vert = "\n".join([
        '<text id="t1">',
        '<p id="p1" quality="good" lang="mk">',
        '<s id="s1">',
        "Зборот\tзбор-n\tNcmsny",
        "<g/>",
        ".\t.-z\tZ",
        "</s>",
        '<s id="s2">',
        "зборот\tзбор-n\tNcmsny",
        "</s>",
        "</p>",
        '<p id="p2" quality="good" lang="bg">',
        '<s id="s3">',
        "думата\tдума-n\tNcfsny",
        "</s>",
        "</p>",
        "</text>",
    ])
    path = _write("c.vert.gz", gzip.compress(vert.encode("utf-8")))
    assert mine([path], set(), limit=10) == [("зборот", 2, "c.vert.gz")]


def test_vert_tarball_reads_untagged_paragraphs():
    # CLASSLA 2.0: one .vert member inside a .tar.gz, paragraphs without lang.
    vert = '<p id="p1">\n<s id="s1">\nкнигата\tкнига-n\tNcfsny\n</s>\n</p>\n'
    data = vert.encode("utf-8")
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        info = tarfile.TarInfo("c/c.vert")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    path = _write("c.vert.tar.gz", buf.getvalue())
    assert mine([path], set(), limit=10) == [("книгата", 1, "c.vert.tar.gz")]


if __name__ == "__main__":
    test_guards()
    test_ranking_and_sources()
    test_dump_reads_main_namespace_text_only()
    test_dump_markup_neither_splits_nor_invents_words()
    test_vert_reads_mk_paragraphs_only()
    test_vert_tarball_reads_untagged_paragraphs()
    print("ok")
