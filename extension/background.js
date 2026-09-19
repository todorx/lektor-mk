"use strict";
// Local-only: the lexicon and morphology load once into this background page
// and every check runs here. No text ever leaves the machine.
let checkerPromise = null;

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
        checker = new wasm_bindgen.WasmChecker(fst);
      }
      try {
        checker.set_frequency(await loadBytes("mk.freq"));
      } catch (e) {
        console.warn("mk: frequency unavailable, unranked suggestions", e);
      }
      return checker;
    })();
  }
  return checkerPromise;
}

browser.runtime.onMessage.addListener((msg) => {
  if (!msg || msg.type !== "mk-check" || typeof msg.text !== "string") return undefined;
  // ponytail: global 20k-char cap per check; fields beyond this are truncated
  // rather than freezing the page on paste dumps.
  const text = msg.text.slice(0, 20000);
  return getChecker().then(
    (c) => ({ ok: true, json: c.check(text) }),
    (e) => ({ ok: false, error: String(e) })
  );
});
