"""Build the Firefox extension's WASM package and copy runtime data.

Usage:  python tools/build_extension.py

Steps: cargo build (wasm32) -> wasm-bindgen (no-modules glue for MV2
background pages) -> copy mk.fst / mk.morph next to the extension.

The wasm-bindgen CLI must match the wasm-bindgen crate in Cargo.lock.
`cargo install` compiles in %TEMP%, which locked-down Windows machines may
block, so this script fetches the prebuilt release binary instead.
"""
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXT = ROOT / "extension"
PKG = EXT / "mk_pkg"
BINDIR = ROOT / "tools" / "bin"


def run(*args):
    print("+", " ".join(str(a) for a in args))
    subprocess.run(args, cwd=ROOT, check=True)


def lock_version():
    text = (ROOT / "Cargo.lock").read_text()
    m = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"', text)
    if not m:
        sys.exit("wasm-bindgen not found in Cargo.lock; run cargo build first")
    return m.group(1)


def ensure_cli(version):
    exe = shutil.which("wasm-bindgen")
    if exe:
        return exe
    local = BINDIR / "wasm-bindgen.exe"
    if local.exists():
        return str(local)
    url = (
        "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/"
        f"{version}/wasm-bindgen-{version}-x86_64-pc-windows-msvc.tar.gz"
    )
    print("downloading", url)
    BINDIR.mkdir(parents=True, exist_ok=True)
    archive = BINDIR / "wbg.tar.gz"
    urllib.request.urlretrieve(url, archive)
    with tarfile.open(archive) as t:
        t.extractall(BINDIR, filter="data")
    inner = next(BINDIR.glob(f"wasm-bindgen-{version}-*/wasm-bindgen.exe"))
    inner.rename(local)
    shutil.rmtree(inner.parent)
    archive.unlink()
    return str(local)


def main():
    fst = ROOT / "data" / "mk.fst"
    if not fst.exists():
        sys.exit("data/mk.fst missing: build it first (see README)")
    run("cargo", "build", "-p", "mk-wasm",
        "--target", "wasm32-unknown-unknown", "--release")
    cli = ensure_cli(lock_version())
    wasm = (ROOT / "target" / "wasm32-unknown-unknown" / "release" / "mk_wasm.wasm")
    shutil.rmtree(PKG, ignore_errors=True)
    PKG.mkdir(parents=True)
    run(cli, "--target", "no-modules", "--out-dir", str(PKG), str(wasm))
    shutil.copy(fst, EXT / "mk.fst")
    morph = ROOT / "data" / "mk.morph"
    if morph.exists():
        shutil.copy(morph, EXT / "mk.morph")
    freq = ROOT / "data" / "mk.freq"
    if freq.exists():
        shutil.copy(freq, EXT / "mk.freq")
    bigram = ROOT / "data" / "mk.bigram"
    if bigram.exists():
        shutil.copy(bigram, EXT / "mk.bigram")
    print("done. Load in Firefox via about:debugging -> This Firefox ->")
    print("Load Temporary Add-on -> extension/manifest.json")


if __name__ == "__main__":
    main()
