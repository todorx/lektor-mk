// End-to-end smoke test: real lexicon + morphology through the WASM bridge.
// Usage: node tools/smoke_wasm.js   (after tools/build_extension.py)
const fs = require("fs");
const path = require("path");
const { WasmChecker } = require("../target/smoke-pkg/mk_wasm.js");

const fst = fs.readFileSync(path.join(__dirname, "../data/mk.fst"));
const morph = fs.readFileSync(path.join(__dirname, "../data/mk.morph"));
const c = WasmChecker.with_grammar(fst, morph);

const cases = [
  ["clean", "Убавата книга е на масата.", []],
  ["double-definite", "Убавата книгата е на масата.", ["MK_DOUBLE_DEFINITE"]],
];
let failed = 0;
for (const [name, text, want] of cases) {
  const got = JSON.parse(c.check(text)).map((d) => d.rule);
  const ok = want.every((r) => got.includes(r)) && (want.length || got.length === 0);
  console.log((ok ? "PASS" : "FAIL") + " " + name + " -> " + got.join(","));
  if (!ok) failed++;
}
process.exit(failed ? 1 : 0);
