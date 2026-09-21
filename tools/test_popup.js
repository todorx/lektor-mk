// Popup suggestion-apply contract (node, with minimal browser stubs).
// Usage: node tools/test_popup.js
"use strict";
const assert = require("node:assert");

// Minimal stubs so the real popup.js top-level wiring can load under node.
const listeners = {};
global.document = {
  getElementById: (id) => ({
    addEventListener: (ev, fn) => {
      listeners[id + ":" + ev] = fn;
    },
    classList: { toggle: () => {} },
    style: {},
    checked: false,
    value: "",
    textContent: "",
  }),
};
global.browser = {
  tabs: { query: async () => [] },
  storage: { local: { get: async () => ({}), set: async () => {} } },
  runtime: { openOptionsPage: () => {}, sendMessage: async () => ({ ok: true, json: "[]" }) },
};

const { applySuggestionText } = (() => {
  // Browser loads settings.js before popup.js as classic scripts sharing
  // globals; mirror that order here.
  Object.assign(global, require("../extension/settings.js"));
  return require("../extension/popup.js");
})();

// 1. ASCII replace at diagnostic offsets.
assert.strictEqual(applySuggestionText("убавата книгата", 0, 7, "убава"), "убава книгата");

// 2. Multipoint offsets are scalar-safe (astral chars don't split).
assert.strictEqual(applySuggestionText("a𐀀bcd", 2, 3, "X"), "a𐀀Xcd");

// 3. Stale offsets clamp instead of throwing.
assert.strictEqual(applySuggestionText("ab", 0, 99, "X"), "X");
assert.strictEqual(applySuggestionText("ab", -5, 1, "X"), "Xb");

// 4. Empty replacement deletes the span.
assert.strictEqual(applySuggestionText("не сака", 0, 2, ""), " сака");

console.log("PASS popup suggestion-apply (4 groups)");
