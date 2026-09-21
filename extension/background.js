"use strict";
// Local-only: the lexicon and morphology load once into this background page
// and every check runs here. No text ever leaves the machine.
let checkerPromise = null;
// True when the checker fell back to spell-only (morphology unreadable).
// Reported with every check so the UI never mistakes it for a clean bill.
let spellOnly = false;

async function loadBytes(name) {
  const res = await fetch(browser.runtime.getURL(name));
  if (!res.ok) throw new Error("missing " + name);
  return new Uint8Array(await res.arrayBuffer());
}

async function getChecker() {
  if (!checkerPromise) {
    checkerPromise = (async () => {
      await wasm_bindgen(browser.runtime.getURL("mk_pkg/mk_wasm_bg.wasm"));
      const fst = await loadBytes("mk.fst");
      let checker;
      try {
        checker = wasm_bindgen.WasmChecker.with_grammar(fst, await loadBytes("mk.morph"));
      } catch (e) {
        console.warn("mk: morphology unavailable, spell-only mode", e);
        spellOnly = true;
        checker = new wasm_bindgen.WasmChecker(fst);
      }
      try {
        checker.set_frequency(await loadBytes("mk.freq"));
      } catch (e) {
        console.warn("mk: frequency unavailable, unranked suggestions", e);
      }
      try {
        checker.set_bigram(await loadBytes("mk.bigram"));
      } catch (e) {
        console.warn("mk: bigrams unavailable, no autocomplete", e);
      }
      return checker;
    })();
  }
  return checkerPromise;
}

browser.runtime.onMessage.addListener((msg) => {
  if (!msg) return undefined;
  if (msg.type === "mk-suggest") {
    // Autocomplete: complete the word being typed from bigram counts.
    // { prev, prefix } -> { ok, words[] }. Empty when no table loaded.
    return getChecker().then(
      (c) => {
        // Bigram keys are bare words; strip trailing punctuation so typing
        // after "здраво," still completes (keys never contain punctuation).
        const prev = String(msg.prev || "").replace(/[^\p{L}]+$/u, "");
        return { ok: true, words: JSON.parse(c.suggest(prev, msg.prefix || "", 3)) };
      },
      (e) => ({ ok: false, error: String(e) })
    );
  }
  if (msg.type !== "mk-check" || typeof msg.text !== "string") return undefined;
  // ponytail: global 20k-char cap per check; fields beyond this are truncated
  // rather than freezing the page on paste dumps.
  const text = msg.text.slice(0, 20000);
  return Promise.all([getChecker(), loadSettings()]).then(
    ([c, settings]) => ({ ok: true, degraded: spellOnly, json: JSON.stringify(applySettings(JSON.parse(c.check(text)), settings)) }),
    (e) => ({ ok: false, error: String(e) })
  );
});
