// Popup suggestion-apply contract (node, with minimal browser stubs).
// Usage: node tools/test_popup.js
"use strict";
const assert = require("node:assert");

// Minimal stubs so the real popup.js top-level wiring can load under node.
function makeEl() {
  const el = {
    listeners: {},
    addEventListener(ev, fn) {
      this.listeners[ev] = fn;
    },
    classList: { toggle: () => {} },
    style: {},
    checked: false,
    value: "",
    dataset: {},
    focus: () => {},
    querySelectorAll: () => [],
    querySelector: () => null,
  };
  // DOM keeps textContent/innerHTML in sync; mirror that so render
  // assertions observe what a browser would.
  let html = "";
  let syncing = false;
  Object.defineProperty(el, "innerHTML", {
    get: () => html,
    set: (v) => {
      html = String(v);
      if (!syncing) {
        syncing = true;
        el.textContent = html.replace(/<[^>]*>/g, "");
        syncing = false;
      }
    },
  });
  let text = "";
  Object.defineProperty(el, "textContent", {
    get: () => text,
    set: (v) => {
      text = String(v);
      if (!syncing && v !== "") html = "";
    },
  });
  return el;
}
const elements = {};
global.document = {
  getElementById: (id) => (elements[id] || (elements[id] = makeEl())),
};
let mockCheckResponse = { ok: true, json: "[]" };
global.browser = {
  tabs: { query: async () => [] },
  storage: { local: { get: async () => ({}), set: async () => {} } },
  runtime: {
    openOptionsPage: () => {},
    sendMessage: async () => mockCheckResponse,
    getURL: (p) => p,
  },
};
Object.defineProperty(global, "navigator", { value: {}, configurable: true });

const popup = (() => {
  // Browser loads settings.js before popup.js as classic scripts sharing
  // globals; mirror that order here.
  Object.assign(global, require("../extension/settings.js"));
  return require("../extension/popup.js");
})();
const { applySuggestionText, runCheck, applySuggestion } = popup;

// 1. ASCII replace at diagnostic offsets.
assert.strictEqual(applySuggestionText("убавата книгата", 0, 7, "убава"), "убава книгата");

// 2. Multipoint offsets are scalar-safe (astral chars don't split).
assert.strictEqual(applySuggestionText("a𐀀bcd", 2, 3, "X"), "a𐀀Xcd");

// 3. Stale offsets clamp instead of throwing.
assert.strictEqual(applySuggestionText("ab", 0, 99, "X"), "X");
assert.strictEqual(applySuggestionText("ab", -5, 1, "X"), "Xb");

// 4. Empty replacement deletes the span.
assert.strictEqual(applySuggestionText("не сака", 0, 2, ""), " сака");

// 5. Failed check renders an error, never "Нема грешки" (masked failure),
// with the backend's reason so a screenshot alone can diagnose it.
(async () => {
  mockCheckResponse = { ok: false, error: "missing mk.fst" };
  elements["in"].value = "убавата книгата е на масата";
  await runCheck();
  assert.notStrictEqual(elements["out"].textContent, "Нема грешки.");
  assert.match(elements["out"].textContent, /Грешка/);
  assert.match(elements["out"].textContent, /missing mk\.fst/);

  // 6. Degraded (spell-only) check says so instead of looking clean.
  mockCheckResponse = { ok: true, degraded: true, json: "[]" };
  await runCheck();
  assert.strictEqual(elements["out"].textContent, "Нема грешки.");
  assert.match(elements["degraded"].textContent, /правопис/);

  // 7. Full loop: diag renders a button, clicking it fixes the textarea.
  const diag = {
    rule: "MK_DOUBLE_DEFINITE",
    text: "убавата книгата",
    char_start: 0,
    char_end: 15,
    severity: "error",
    message: "m",
    suggestions: ["убавата книга"],
  };
  mockCheckResponse = { ok: true, json: JSON.stringify([diag]) };
  elements["in"].value = "убавата книгата е на масата";
  await runCheck();
  assert.match(elements["out"].innerHTML, /data-di="0" data-si="0"/);
  applySuggestion(0, 0);
  assert.strictEqual(elements["in"].value, "убавата книга е на масата");

  console.log("PASS popup suggestion-apply (7 groups)");
})().catch((e) => {
  console.error("FAIL popup render:", e);
  process.exit(1);
});
