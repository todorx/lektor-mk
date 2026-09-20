// Filter contract for the Harper-style settings layer (node, no browser).
// Usage: node tools/test_settings.js
"use strict";
const assert = require("node:assert");
const {
  MK_RULES,
  MK_GROUPS,
  mkDefaults,
  normalizeSettings,
  applySettings,
  siteEnabled,
} = require("../extension/settings.js");

const diag = (rule, text) => ({ rule, text, severity: "error", message: "m", suggestions: [] });

// 1. Catalogue integrity: 13 unique engine IDs in known groups.
assert.strictEqual(MK_RULES.length, 13, "rule catalogue size");
assert.deepStrictEqual(
  MK_RULES.map((r) => r.id).sort(),
  [
    "MK_CLITIC_ORDER", "MK_DATIVE_I", "MK_DOUBLE_DEFINITE", "MK_FOREIGN_CYRILLIC",
    "MK_HOMOGLYPH", "MK_L_PARTICIPLE", "MK_LATIN_TEXT", "MK_NAJ_SEPARATED",
    "MK_NE_FUSED", "MK_PO_SEPARATED", "MK_SENTENCE_CAPITAL", "MK_SPACE_BEFORE_PUNCT",
    "MK_SPELL",
  ].sort()
);
for (const r of MK_RULES) {
  assert.ok(MK_GROUPS.includes(r.group), "known group for " + r.id);
  assert.ok(r.name && r.desc, "name+desc for " + r.id);
}

// 2. Defaults pass everything.
const all = [diag("MK_SPELL", "книгаа"), diag("MK_DATIVE_I", "и")];
assert.deepStrictEqual(applySettings(all, mkDefaults()), all);
assert.deepStrictEqual(applySettings(all, undefined), all);

// 3. Disabled rule is filtered, others survive.
const off = { ...mkDefaults(), rulesOff: ["MK_SPELL"] };
assert.deepStrictEqual(applySettings(all, off), [diag("MK_DATIVE_I", "и")]);

// 4. Personal dictionary is case-insensitive and rule-agnostic.
const dict = { ...mkDefaults(), userWords: ["Скопје"] };
assert.deepStrictEqual(applySettings([diag("MK_SPELL", "скопје")], dict), []);
assert.deepStrictEqual(applySettings([diag("MK_SPELL", "книгаа")], dict).length, 1);

// 5. Unknown rule IDs pass through (forward-compatible with new engine rules).
assert.deepStrictEqual(applySettings([diag("MK_FUTURE", "x")], mkDefaults()).length, 1);

// 6. Normalization drops junk but keeps types.
assert.deepStrictEqual(normalizeSettings(null), mkDefaults());
assert.strictEqual(normalizeSettings({ enabled: "yes" }).enabled, true);
assert.strictEqual(normalizeSettings({ delayMs: 99999 }).delayMs, 700);
assert.strictEqual(normalizeSettings({ delayMs: 250 }).delayMs, 250);

// 7. Site gate: master off wins; listed hosts are skipped.
assert.strictEqual(siteEnabled(mkDefaults(), "example.com"), true);
assert.strictEqual(
  siteEnabled({ ...mkDefaults(), siteOff: ["example.com"] }, "example.com"),
  false
);
assert.strictEqual(siteEnabled({ ...mkDefaults(), enabled: false }, "example.com"), false);

console.log("PASS settings filter (7 groups)");
