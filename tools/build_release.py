"""Release builder: version -> validated AMO-uploadable zip + source bundle.

Usage: python tools/build_release.py --version 0.2.0
Refuses dirty trees and lexicon-less builds; fails on linter errors.
"""
import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXT = ROOT / "extension"
DIST = ROOT / "dist"

VER = re.compile(r"^\d+\.\d+\.\d+$")


def valid_version(v):
    if not VER.match(v or ""):
        sys.exit(f"bad --version {v!r}: want X.Y.Z")
    return v


def run(*args):
    print("+", " ".join(str(a) for a in args))
    subprocess.run(args, cwd=ROOT, check=True)


def fail(msg):
    sys.exit(f"build_release: {msg}")


def check_tree_clean():
    out = subprocess.run(["git", "status", "--porcelain"],
                         cwd=ROOT, capture_output=True, text=True)
    dirty = [l for l in out.stdout.splitlines()
             if l.strip() and not l.strip().endswith("dist/")]
    if dirty:
        fail("tree dirty — commit first:\n" + "\n".join(dirty[:10]))


def patch_version(path, version, pattern):
    text = path.read_text(encoding="utf-8")
    new, n = pattern.subn(lambda m: m.group(1) + version + '"', text, count=1)
    if n != 1:
        fail(f"version pattern not found once in {path}")
    if new == text:
        return  # already at version
    with tempfile.NamedTemporaryFile("w", delete=False, dir=path.parent,
                                     encoding="utf-8") as f:
        f.write(new)
    Path(f.name).replace(path)


def set_manifest_version(version):
    patch_version(EXT / "manifest.json", version,
                  re.compile(r'("version"\s*:\s*")[^"]+("?)'))


def set_cargo_version(version):
    patch_version(ROOT / "Cargo.toml", version,
                  re.compile(r'(?m)(^version\s*=\s*")[^"]+("?)'))


def sanity(version):
    man = json.loads((EXT / "manifest.json").read_text(encoding="utf-8"))
    if man.get("version") != version:
        fail("manifest version != --version")
    if man.get("manifest_version") != 3:
        fail("manifest_version != 3 (run Phase 1 first)")
    wasm = EXT / "mk_pkg" / "mk_wasm_bg.wasm"
    if not wasm.exists():
        fail("mk_pkg/mk_wasm_bg.wasm missing (build_extension failed?)")
    total = sum(p.stat().st_size for p in EXT.rglob("*") if p.is_file())
    if total > 10 * 1024 * 1024:
        fail(f"unpacked {total/1e6:.1f} MB > 10 MB cap")
    for js in ("content.js", "popup.js", "background.js"):
        if "console.log" in (EXT / js).read_text(encoding="utf-8"):
            print(f"warn: console.log left in {js}")


def lint_zip(zipp):
    run("npx", "--yes", "addons-linter", str(zipp),
        "--output", "json", "--output-file", str(DIST / "lint.json"))
    summary = json.loads((DIST / "lint.json").read_text(encoding="utf-8"))["summary"]
    print(f"linter: {summary['errors']} errors, "
          f"{summary['warnings']} warnings, {summary['notices']} notices")
    if summary["errors"]:
        errs = json.loads((DIST / "lint.json").read_text(encoding="utf-8"))["errors"]
        for m in [x for x in errs if x.get("type") == "error"][:5]:
            print("ERROR", m.get("file", "?") + ":" +
                  str(m.get("line", "?")), m.get("message", "")[:200])
        fail("linter errors — fix and re-run")


def source_bundle(version):
    out = DIST / f"lektor-mk-{version}-source.zip"
    with tempfile.TemporaryDirectory() as tmp:
        arc = Path(tmp) / "src.zip"
        subprocess.run(["git", "archive", "HEAD", "-o", str(arc)],
                       cwd=ROOT, check=True)
        shutil.copy(arc, out)
    print("wrote", out)
    return out


def listing_check():
    need = ["description-mk.txt", "description-en.txt",
            "privacy-policy.txt", "reviewer-notes.txt"]
    missing = [n for n in need
               if not (ROOT / "docs" / "amo-listing" / n).exists()
               or not (ROOT / "docs" / "amo-listing" / n).stat().st_size]
    if missing:
        fail("listing docs missing/empty: " + ", ".join(missing))
    (DIST / "listing-check.txt").write_text(
        "listing docs OK: " + ", ".join(need) + "\n", encoding="utf-8")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    args = ap.parse_args()
    version = valid_version(args.version)
    if not (ROOT / "data" / "mk.fst").exists():
        fail("data/mk.fst missing: build it first (see README)")
    check_tree_clean()
    set_manifest_version(version)
    set_cargo_version(version)
    sys.path.insert(0, str(ROOT / "tools"))
    import build_extension
    build_extension.main()
    sanity(version)
    DIST.mkdir(exist_ok=True)
    zipp = DIST / f"lektor-mk-{version}.zip"
    run("npx", "--yes", "web-ext", "build", "--source-dir", str(EXT),
        "--artifacts-dir", str(DIST), "--filename", zipp.name,
        "--overwrite-dest")
    lint_zip(zipp)
    with zipfile.ZipFile(zipp) as z:
        names = z.namelist()
    for must in ("manifest.json", "mk_pkg/mk_wasm_bg.wasm"):
        if must not in names:
            fail(f"{must} not in zip")
    print(f"zip OK: {len(names)} files, {zipp.stat().st_size/1e6:.1f} MB")
    source_bundle(version)
    listing_check()
    print("done. Submit: AMO Developer Hub -> New Add-on -> upload",
          zipp.name, "+ source zip, paste docs/amo-listing/.")


if __name__ == "__main__":
    main()
