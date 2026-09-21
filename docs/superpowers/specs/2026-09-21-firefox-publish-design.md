# Лектор-МК — AMO publish kit (manual upload) — design

Date: 2026-09-21. Path: architectural (brainstormed, option A approved).
Goal: one command produces an AMO-listed-ready submission; human uploads via Developer Hub. No API keys, no CI, no MV3.

## 1. Intent (agreed)

- Outcome: `python tools/build_release.py --version X` → upload-ready zip + reviewer source bundle + listing text, all validated.
- Who: maintainer (you) submitting Лектор-МК as a public listed add-on on addons.mozilla.org.
- Success: `addons-linter` 0 errors on the zip; zip contents match a clean git commit; AMO upload form can be completed from `docs/amo-listing/` without improvising text; `cargo test` stays green.
- Non-goals: auto-sign (`web-ext sign`), CI, MV3/Chrome, screenshots generation.

## 2. Architecture & scope

- One new script `tools/build_release.py` (~150 lines, stdlib only). Reuses `tools/build_extension.py:main()` by import — no duplicated build logic.
- Existing tools assumed present: `cargo`, node 24, `npx web-ext` (10.7.0 verified), `npx addons-linter` (10.13.0 verified), wasm-bindgen prebuilt-binary path already handled by `build_extension.py` (no `cargo install` into locked `%TEMP%`).
- Version single-sourced from `--version` flag; script patches `extension/manifest.json` and `Cargo.toml`, aborts on dirty tree (except `dist/`).
- Outputs (gitignored) under `dist/`: `lektor-mk-<ver>.zip`, `lektor-mk-<ver>-source.zip`, `lint.json`, `listing-check.txt`.
- Manifest stays MV2 (`mk-proofreader@macedonian-text`); Firefox accepts MV2. No permission changes.

## 3. Components

| File | Change |
|---|---|
| `tools/build_release.py` | NEW. Orchestrates clean → build → validate → `web-ext build` → `addons-linter` → source bundle → listing check. |
| `extension/manifest.json` | PATCH version field only, atomic write (temp + rename). |
| `Cargo.toml` | PATCH workspace version only, same atomicity. |
| `extension/icons/icon-{48,96,128}.png` | NEW, derived from `icon.svg`. Pillow if importable; else warn + continue with SVG (AMO may require raster — manual fallback documented). |
| `docs/amo-listing/description-mk.txt` | NEW. Macedonian listing text (local-only, offline, rule coverage summary). |
| `docs/amo-listing/description-en.txt` | NEW. English equivalent. |
| `docs/amo-listing/privacy-policy.txt` | NEW. "No data leaves device; no network calls; no analytics; storage permission only for settings/user dict." Must match `activeTab, storage, clipboardWrite` reality. |
| `docs/amo-listing/reviewer-notes.txt` | NEW. WASM repro (`cargo build -p mk-wasm --target wasm32-unknown-unknown --release`, wasm-bindgen version = `Cargo.lock`), FST/morph provenance (gerazov wordlist GPL-2.0 → project GPL-3.0; Apertium paradigms; Wikipedia counts-only + title gazetteer), minified/glue explanation (`mk_pkg/mk_wasm.js` is wasm-bindgen output, not hand-minified). |
| `dist/` + `.gitignore` entry | NEW output dir, ignored. |
| `web-ext-artifacts/` | UNCHANGED/ignored; release uses `dist/` to avoid stale `macedonian_proofreader-0.1.0.zip` confusion. |

Skipped: screenshots (add 1–2 real popup/content PNGs manually at submit time); homepage URL; `web-ext sign`.

## 4. Data flow

1. Preconditions: `git status --porcelain` clean (ignoring `dist/`); `--version` matches `X.Y.Z`; `data/mk.fst` exists.
2. Rebuild via imported `build_extension.main()`: wasm32 WASM → `wasm-bindgen --target no-modules` → copy `data/mk.{fst,morph,freq,bigram}` to `extension/`.
3. Sanity asserts: `manifest.json` version == flag; `mk_pkg/mk_wasm_bg.wasm` exists; unpacked size ≤ 10 MB; warn (not fail) on `console.log` in `content.js`/`popup.js`/`background.js`.
4. `npx web-ext build --source-dir=extension --artifacts-dir=dist --filename=lektor-mk-<ver>.zip --overwrite-dest`.
5. `npx addons-linter dist/lektor-mk-<ver>.zip --output json --output-file dist/lint.json`. Fail on `errorCount > 0`; echo top 5 errors; warnings pass with output.
6. Source bundle: `git archive HEAD` + note pointing at data rebuild (README build-from-source) → `dist/lektor-mk-<ver>-source.zip`. Satisfies AMO "source code required" for WASM/bundled output.
7. Listing check: assert all 4 `docs/amo-listing/` files exist and non-empty → `dist/listing-check.txt`; print submit checklist (Developer Hub → New Add-on → upload zip → upload source zip → paste listing → reply with reviewer-notes).

## 5. Error handling

- Missing `cargo`/`node`/`npx`: fail at first missing, printing the exact install command; no partial builds.
- Missing `data/mk.fst`: abort, point to README "Build from source" (never ship lexicon-less zip).
- Dirty tree/version mismatch: abort before mutation.
- Non-atomic writes forbidden: manifest/Cargo patches via temp + rename.
- Linter errors: abort + top-5 errors with file:line. Warnings: continue, echoed.
- Windows: never `cargo install` (locked `%TEMP%`); use existing prebuilt-download path.

## 6. Verification

- `python tools/build_release.py --version 0.1.0` → both zips + `lint.json` (0 errors); `unzip -l` (or `Expand-Archive` list) shows `manifest.json`, `mk_pkg/mk_wasm_bg.wasm`, `mk.{fst,morph,freq,bigram}`, icons.
- Source bundle: `git archive`-based; unpack + follow reviewer-notes repro.
- `cargo test` green (no Rust touched).
- Final gate is AMO Developer Hub's server-side validator on first upload.

## 7. Upgrade paths (explicitly deferred)

- AMO API keys available → add `--sign` flag (`web-ext sign --channel=listed`); store issuer/secret in env, never in repo.
- Repeated releases → GitHub workflow on tag; snapshot passing `lint.json` as regression gate.
- AMO rejects SVG-only icons → make Pillow PNG step mandatory.
- Chrome Web Store → separate MV3 effort, out of scope.

## Self-review (2026-09-21)

- Placeholders: none — versions, filenames, commands concrete.
- Consistency: `dist/` vs `web-ext-artifacts/` resolved (release uses `dist/`); MV2 retained consistently; data files rebuilt, not stored.
- Scope: single plan-sized (one script + 4 text files + icons); screenshots/CI/signing explicitly deferred.
- Ambiguity: "clean tree" = `git status --porcelain` empty except `dist/` + icons output; "0 errors" = linter `errorCount`; size cap 10 MB unpacked.
