// Inline-checker decisions (node, with minimal browser stubs).
// Usage: node tools/test_content.js
"use strict";
const assert = require("node:assert");

const listeners = {};
const sent = [];

global.window = {
  __mkInit: false,
  addEventListener: () => {},
  scrollBy: () => {},
};
global.document = {
  addEventListener: (ev, fn) => {
    listeners[ev] = fn;
  },
  getElementById: () => null,
  createElement: () => ({ style: {}, setAttribute: () => {}, addEventListener: () => {} }),
  documentElement: { appendChild: () => {} },
};
global.location = { hostname: "example.com" };
Object.defineProperty(global, "navigator", {
  value: { userAgent: "Mozilla/5.0 (X11; Linux x86_64) Firefox/156.0" },
  configurable: true,
});
global.browser = {
  storage: { local: { get: async () => ({}) }, onChanged: { addListener: () => {} } },
  runtime: {
    getURL: (p) => p,
    sendMessage: async (msg) => {
      sent.push(msg);
      throw new Error("Could not establish connection. Receiving end does not exist.");
    },
  },
};

// Browser loads settings.js before content.js as classic scripts sharing globals.
Object.assign(global, require("../extension/settings.js"));
const { scrollDelta, retryDelay, MAX_RETRIES } = require("../extension/content.js");

// 1. A field above the safe line is left alone; one below it is lifted clear.
const H = 779;
assert.strictEqual(scrollDelta(H * 0.1, H), 0);
assert.strictEqual(scrollDelta(H * 0.4, H), 0);
assert.strictEqual(scrollDelta(H * 0.9, H), H * 0.5);
assert.ok(scrollDelta(2400, H) > 0);

// 2. Backoff grows then gives up, so a permanently dead background is not hammered.
assert.strictEqual(retryDelay(0), 300);
assert.ok(retryDelay(1) > retryDelay(0));
assert.strictEqual(retryDelay(MAX_RETRIES), 0);
assert.strictEqual(retryDelay(MAX_RETRIES + 5), 0);

// 3. A rejected first message is actually retried (the cold-background bug).
const field = {
  tagName: "TEXTAREA",
  disabled: false,
  readOnly: false,
  value: "убавата книгата",
  getBoundingClientRect: () => ({ bottom: 0, top: 0, left: 0, width: 1, height: 1 }),
};
listeners["focusin"]({ target: field });

setTimeout(() => {
  assert.ok(sent.length >= 2, `expected a retry, saw ${sent.length} send(s)`);
  assert.strictEqual(sent[0].type, "mk-check");
  console.log(`PASS content decisions (3 groups, ${sent.length} sends)`);
}, 450);
