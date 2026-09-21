# MV3 + AMO Publish Kit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the extension to Manifest V3, then ship a one-command release script producing an AMO-upload-ready zip.

**Architecture:** Phase 1 hand-edits `extension/manifest.json` to MV3 (Firefox keeps `background.scripts`; zero JS changes). Phase 2 adds stdlib-only `tools/build_release.py` that reuses `build_extension.main()` by import, then gates on `web-ext build` + `addons-linter`.

**Tech Stack:** Python 3 stdlib, Firefox MV3, `web-ext` 10.7.0, `addons-linter` 10.13.0, Rust/wasm-bindgen via existing `tools/build_extension.py`.

**Spec:** `docs/superpowers/specs/2026-09-21-firefox-publish-design.md`

## Global Constraints

- Firefox-only; `browser.*` namespace, no `chrome.*`.
- `background.scripts` unchanged — Firefox MV3 has no `service_worker` support.
- `browser_specific_settings.gecko.strict_min_version` stays `109.0`.
- Permissions stay `activeTab, storage, clipboardWrite` — no additions.
- Version single-sourced from `--version` flag; manifest + `Cargo.toml` patched atomically (temp + rename).
- Release aborts on dirty tree (except `dist/`), missing `data/mk.fst`, or linter `errorCount > 0`.
- Never `cargo install` (locked `%TEMP%`); use `build_extension.py` prebuilt-download path.
- `cargo test` stays green; no Rust changes.
- Unpacked extension ≤ 10 MB.

## Review Focus

- MV3 `action` key typo'd (`browser_action` left behind) → linter error names the bad key; Task 1 pins `action` presence.
- MV3 `web_accessible_resources` left as string array → linter `MANIFEST_FIELD_INVALID`; Task 1 pins object form.
- `icon.svg` `<text>М</text>` renders differently per rasterizer → PNGs visually checked once at Task 1; acceptable variance, not pixel-pinned.
- `data/mk.fst` missing on fresh clone → script aborts with README pointer, never a lexicon-less zip; Task 3 pins the message.
- Dirty-tree release (uncommitted manifest tweak) → abort before mutation; Task 3 pins no-mutation-on-abort.

---

### Task 1: MV3 manifest migration

**Files:**
- Modify: `extension/manifest.json`
- Create: `extension/icons/icon-48.png`, `extension/icons/icon-96.png`, `extension/icons/icon-128.png`

**Interfaces:**
- Consumes: `extension/icons/icon.svg` (existing 64x64 red `М`).
- Produces: MV3 `manifest.json` that Task 3's `web-ext build` + linter gate consumes; `icons` key paths Task 3's sanity check expects.

- [ ] **Step 1: Check PNG tooling**

Run: `python -c "import PIL; print(PIL.__version__)"`
Expected: version printed (proceed to Step 2a) or `ModuleNotFoundError` (proceed to Step 2b).

- [ ] **Step 2a: Generate PNGs (if Pillow present)**

Run:

```bash
python -c "from PIL import Image; [Image.open('extension/icons/icon.svg') and None] " 2>NUL || python - <<'EOF'
from PIL import Image
import io
try:
    import cairosvg
    have_cairo = True
except ImportError:
    have_cairo = False
print(have_cairo)
EOF
```

Note: Pillow cannot rasterize SVG alone. If `cairosvg` is also present, run:

```bash
python - <<'EOF'
import cairosvg
svg = open('extension/icons/icon.svg','rb').read()
for s in (48, 96, 128):
    cairosvg.svg2png(bytestring=svg, write_to=f'extension/icons/icon-{s}.png',
                     output_width=s, output_height=s)
print('wrote 48/96/128')
EOF
```

Expected: three PNGs on disk. If neither `cairosvg` nor another rasterizer exists, fall through to Step 2b.

- [ ] **Step 2b: PNG fallback (no rasterizer)**

Keep `icons/icon.svg` as the only icon. Record in the commit message that PNGs are deferred until AMO demands raster icons. Skip the `icons` manifest key addition (Step 3) for PNG sizes; keep SVG reference only if already present (it is not — manifest currently has no `icons` key, so add nothing).

- [ ] **Step 3: Edit manifest to MV3**

Apply exactly this diff shape to `extension/manifest.json` (keep `name`, `version`, `description`, `browser_specific_settings`, `permissions`, `background`, `options_ui`, `content_scripts` untouched):

```json
{
  "manifest_version": 3,
  "action": {
    "default_title": "Check Macedonian text",
    "default_popup": "popup.html",
    "default_icon": "icons/icon.svg"
  },
  "web_accessible_resources": [
    {
      "resources": ["mk_pkg/mk_wasm_bg.wasm", "mk.fst", "mk.morph", "mk.freq", "mk.bigram"],
      "matches": ["*://*/*"]
    }
  }
}
```

I.e. delete the `browser_action` block, insert the identical `action` block; replace the string-array `web_accessible_resources` with the object above; change `2` to `3`. If Step 2a produced PNGs, use `"default_icon": {"48": "icons/icon-48.png", "96": "icons/icon-96.png", "128": "icons/icon-128.png"}` and add top-level `"icons": {"48": "icons/icon-48.png", "96": "icons/icon-96.png", "128": "icons/icon-128.png"}`.

- [ ] **Step 4: Lint the MV3 manifest**

Run: `npx --yes addons-linter extension --output json 2>NUL | python -c "import json,sys; d=json.load(sys.stdin); print('errors:', d['summary']['errors'], 'warnings:', d['summary']['warnings'])"`
Expected: `errors: 0` (warnings echoed, allowed).

- [ ] **Step 5: Manual smoke in Firefox (human step, executor notes result)**

`python tools/build_extension.py`, then `about:debugging` → This Firefox → Load Temporary Add-on → `extension/manifest.json`; type in one textarea (underline appears), open popup (status renders), open options (toggle saves). Record pass/fail in commit message.

- [ ] **Step 6: Commit MV3 alone**

```bash
git add extension/manifest.json extension/icons/
git commit -m "feat: migrate extension to Manifest V3 (Firefox)"
```

### Task 2: AMO listing docs

**Files:**
- Create: `docs/amo-listing/description-mk.txt`
- Create: `docs/amo-listing/description-en.txt`
- Create: `docs/amo-listing/privacy-policy.txt`
- Create: `docs/amo-listing/reviewer-notes.txt`

**Interfaces:**
- Consumes: README numbers (427,464 forms → 1.25 MB FST; 4.8 MB total / 6.2 MB with bigrams; offline), manifest permission list.
- Produces: text files Task 3's listing check asserts non-empty.

- [ ] **Step 1: Write `description-mk.txt`**

Content (save verbatim, adjust only if factually wrong):

```text
Лектор-МК — проверка на правопис и граматика за македонски, целосно локално во вашиот прелистувач. Ниеден текст никогаш не го напушта вашиот компјутер: нема сервер, нема интернет, работи офлајн.
Подвлекува грешки додека пишувате во секое текстуално поле (кликнете на подвлечен збор за предлози), а popup-от проверува вметнат текст и нуди автодовршување на следниот збор.
Опфаќа правопис (427.000+ форми), измешани латинични/кирилични букви, странски кирилични букви, двоен член (убавата книгата), ред на клитики, дативно ѝ, л-партицип, слеано не/нај/по и интерпункција. Правилата се прецизносензитивни: секое правило има позитивни и негативни тестови.
```

- [ ] **Step 2: Write `description-en.txt`**

```text
Lektor-MK — Macedonian spelling and grammar checking, fully on-device in your browser. No text ever leaves your machine: no server, no internet, works offline.
Underlines mistakes as you type in any text field (click an underline for fixes); the toolbar popup checks pasted text and suggests next-word completions.
Covers spelling (427,000+ forms), Latin-inside-Cyrillic homoglyphs, foreign Cyrillic letters, double definiteness, clitic order, dative ѝ, l-participle agreement, fused не/нај/по, and punctuation. Every rule ships precision-gated with positive and negative tests.
```

- [ ] **Step 3: Write `privacy-policy.txt`**

```text
Privacy policy for Лектор-МК.
1. No data collection: the extension collects, stores, transmits, and shares nothing about you or your text.
2. No network: all checking runs locally (WebAssembly + local lexicon files); the extension makes zero network requests and includes no analytics.
3. Local storage only: the `storage` permission keeps your settings, rule toggles, personal dictionary, and per-site preferences on your device.
4. `activeTab` is used to detect the current site for per-site toggles and paste-box checking; `clipboardWrite` copies a chosen suggestion on click.
Contact: via the add-on listing support address.
```

- [ ] **Step 4: Write `reviewer-notes.txt`**

```text
Reviewer notes for Лектор-МК (Firefox, MV3, local-only).
BUILD REPRO: python tools/build_extension.py runs: cargo build -p mk-wasm --target wasm32-unknown-unknown --release, then wasm-bindgen --target no-modules (CLI version pinned to Cargo.lock; prebuilt binary fetched to tools/bin, no cargo install) into extension/mk_pkg/, then copies data/mk.{fst,morph,freq,bigram} next to the extension.
WASM GLUE: extension/mk_pkg/mk_wasm.js is unmodified wasm-bindgen output (plus .d.ts), not hand-minified.
DATA PROVENANCE (counts/lists only, no article text ships): base wordlist gerazov/dictionary-mk (GPL-2.0, hence project GPL-3.0-or-later); morphology expanded from apertium-mkd paradigms (GPL) via tools/expand_apertium.py; unigram/bigram counts (top 40k/50k) from the MK Wikipedia dump (CC BY-SA 4.0, counts only); proper-noun gazetteer from Wikipedia titles, reviewed before commit. Rebuild steps: README "Build from source".
PERMISSIONS: activeTab (per-site toggle + popup host detection), storage (settings/user dict), clipboardWrite (copy suggestion on click). No remote code, no network calls.
```

- [ ] **Step 5: Commit listing docs**

```bash
git add docs/amo-listing/
git commit -m "docs: AMO listing + privacy + reviewer notes"
```

### Task 3: `tools/build_release.py`

**Files:**
- Create: `tools/build_release.py`
- Modify: `.gitignore` (append `/dist/`)
- Modify: `extension/manifest.json` (version only, by script at runtime — no manual edit)
- Modify: `Cargo.toml` (version only, by script at runtime — no manual edit)

**Interfaces:**
- Consumes: `tools/build_extension.py:main()` (import, not copy); Task 1 MV3 manifest; Task 2 listing docs; `data/mk.{fst,morph,freq,bigram}`.
- Produces: `dist/lektor-mk-<ver>.zip`, `dist/lektor-mk-<ver>-source.zip`, `dist/lint.json`, `dist/listing-check.txt`.

- [ ] **Step 1: Write failing smoke test (version-format guard)**

Create `tools/test_build_release.py`:

```python
from build_release import valid_version

def test_valid_version():
    assert valid_version("0.1.0") == "0.1.0"
    assert valid_version("1.2.3") == "1.2.3"

def test_invalid_version():
    for bad in ("0.1", "v0.1.0", "0.1.0.0", "abc", ""):
        try:
            valid_version(bad)
        except SystemExit:
            continue
        raise AssertionError(f"accepted {bad!r}")
```

- [ ] **Step 2: Run it, watch it fail**

Run: `python -m pytest tools/test_build_release.py -v 2>&1 | Select-Object -Last 5`
Expected: FAIL/ERROR — `build_release` module does not exist yet.

- [ ] **Step 3: Write `tools/build_release.py` (full content)**

```python
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
    new, n = pattern.subn(lambda m: m.group(1) + version, text, count=1)
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
        msgs = json.loads((DIST / "lint.json").read_text(encoding="utf-8"))["messages"]
        for m in [x for x in msgs if x.get("type") == "error"][:5]:
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
```

- [ ] **Step 4: Run the smoke test, watch it pass**

Run: `python -m pytest tools/test_build_release.py -v 2>&1 | Select-Object -Last 5`
Expected: 2 passed.

- [ ] **Step 5: Append `/dist/` to `.gitignore`, full dry run**

Run:

```bash
python - <<'EOF'
p = open('.gitignore', encoding='utf-8').read()
if '/dist/' not in p:
    open('.gitignore', 'a', encoding='utf-8').write('\n/dist/\n')
print('gitignore ok')
EOF
python tools/build_release.py --version 0.1.0
```

Expected: `dist/lektor-mk-0.1.0.zip` + `-source.zip` + `lint.json` (0 errors) + `listing-check.txt`. NOTE: this bumps manifest/Cargo to 0.1.0 (already 0.1.0 — no-op) and rebuilds the extension (~1 min: cargo wasm + data copy).

- [ ] **Step 6: `cargo test` still green, then commit**

Run: `cargo test 2>&1 | Select-Object -Last 3`
Expected: all suites pass.

```bash
git add tools/build_release.py tools/test_build_release.py .gitignore
git commit -m "feat: one-command AMO release builder"
```

Note: `dist/` output stays untracked; the manifest/Cargo version bump (if any) commits with the next version bump, not here — if the dry run dirtied them, `git checkout -- extension/manifest.json Cargo.toml` before committing.

## Self-review

- Spec coverage: Phase 1 (§2/§4) → Task 1; listing docs (§3) → Task 2; release script (§4 Phase 2, §5) → Task 3; verification (§6) → Task 1 Step 5 + Task 3 Steps 5–6. Stale "Manifest stays MV2" line in spec §2 fixed inline (now reads MV3).
- Placeholders: none — all file contents, commands, and expected outputs concrete.
- Type consistency: `valid_version(v: str) -> str | SystemExit`; `patch_version(path: Path, version: str, pattern: re.Pattern)`; task interfaces share `manifest.json`, `icons/`, `dist/` names verbatim.
- Review Focus items each pinned: action-key and WAR-object tests live in Task 1 Step 4 (linter catches both, error text echoed); icon raster variance accepted with one visual check; missing-fst abort and dirty-tree no-mutation pinned in Task 3 script + dry run.
