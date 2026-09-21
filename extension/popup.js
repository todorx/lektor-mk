"use strict";
// Popup: status + master/site toggles + paste-text check + options link.
// Diagnostics arrive already filtered by background.js.
const $ = (id) => document.getElementById(id);
const esc = (s) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

let settings = mkDefaults();
let host = "";
let lastDiags = [];

// Scalar-safe splice: replace [start, end) code points with rep, clamped.
// Same unit as content.js's splice; offsets come from the engine in scalars.
function applySuggestionText(text, start, end, rep) {
  const c = Array.from(String(text));
  const n = c.length;
  const a = Math.max(0, Math.min(start | 0, n));
  const b = Math.max(a, Math.min(end | 0, n));
  return c.slice(0, a).join("") + String(rep) + c.slice(b).join("");
}

async function currentHost() {
  try {
    const tabs = await browser.tabs.query({ active: true, currentWindow: true });
    const url = tabs && tabs[0] && tabs[0].url;
    return url ? new URL(url).hostname : "";
  } catch (e) {
    return "";
  }
}

function ruleName(id) {
  const r = MK_RULES.find((x) => x.id === id);
  return r ? r.name : id;
}

function renderStatus() {
  const on = settings.enabled && !settings.siteOff.includes(host);
  $("dot").classList.toggle("off", !on);
  $("status").textContent = !settings.enabled
    ? "Исклучено насекаде."
    : host && settings.siteOff.includes(host)
      ? "Исклучено на овој сајт."
      : "Активно — подвлекува грешки додека пишувате.";
  $("enabled").checked = settings.enabled;
  $("siteOn").checked = host ? !settings.siteOff.includes(host) : false;
  $("siteRow").style.display = host ? "" : "none";
}

$("enabled").addEventListener("change", async (e) => {
  settings.enabled = e.target.checked;
  await saveSettings(settings);
  renderStatus();
});

$("siteOn").addEventListener("change", async (e) => {
  settings.siteOff = e.target.checked
    ? settings.siteOff.filter((h) => h !== host)
    : [...settings.siteOff, host];
  await saveSettings(settings);
  renderStatus();
});

$("options").addEventListener("click", () => browser.runtime.openOptionsPage());

// Autocomplete in the quick-check box: complete the word being typed from
// bigram counts. Fires only with a previous word and 2+ letters typed,
// so it stays silent until it can actually help.
let suggestTimer = 0;
let suggestSeq = 0;
$("in").addEventListener("input", () => {
  clearTimeout(suggestTimer);
  suggestTimer = setTimeout(suggestNow, 150);
});

async function suggestNow() {
  const box = $("suggest");
  const el = $("in");
  const pos = el.selectionStart;
  // Only complete at end of a word: with a mid-word cursor the insert
  // (head + slice(pos)) would duplicate the word's tail.
  if (pos < el.value.length && !/\s/.test(el.value[pos])) {
    box.innerHTML = "";
    return;
  }
  const before = el.value.slice(0, pos).match(/(\S+)\s+(\S*)$/);
  if (!before) {
    box.innerHTML = "";
    return;
  }
  const [, prev, prefix] = before;
  if (prefix.length < 2) {
    box.innerHTML = "";
    return;
  }
  const my = ++suggestSeq;
  try {
    const res = await browser.runtime.sendMessage({ type: "mk-suggest", prev, prefix });
    if (my !== suggestSeq) return; // stale: a newer keystroke already won
    const words = res && res.ok ? res.words : [];
    box.innerHTML = words.map((w) => `<button type="button">${esc(w)}</button>`).join("");
    box.querySelectorAll("button").forEach((b) =>
      b.addEventListener("click", () => {
        const pos = el.selectionStart;
        const head = el.value.slice(0, pos).replace(/\S+$/, b.textContent);
        el.value = head + el.value.slice(pos);
        el.focus();
        box.innerHTML = "";
      })
    );
  } catch (e) {
    box.innerHTML = "";
  }
}

$("go").addEventListener("click", runCheck);

async function runCheck() {
  const out = $("out");
  const warn = $("degraded");
  out.textContent = "Проверува…";
  warn.hidden = true;
  warn.textContent = "";
  try {
    const res = await browser.runtime.sendMessage({ type: "mk-check", text: $("in").value });
    // A failed check used to render as "Нема грешки" — never mask it.
    // The backend reason is shown so a screenshot alone can diagnose.
    if (!res || !res.ok) {
      lastDiags = [];
      const why = res && res.error ? " " + String(res.error).slice(0, 160) : "";
      out.textContent = "Грешка при проверката." + why;
      return;
    }
    const diags = JSON.parse(res.json);
    lastDiags = diags;
    if (res.degraded) {
      warn.textContent = "Само правопис — морфологијата не е вчитана. Реинсталирајте го додатокот.";
      warn.hidden = false;
    }
    out.innerHTML = diags.length
      ? diags
          .map(
            (d, di) =>
              `<div class="mk-item"><b>${esc(d.text)}</b><i>${esc(ruleName(d.rule))}</i>` +
              `<span>${esc(d.message)}</span>` +
              (d.suggestions || []).slice(0, 3).map((s, si) => `<button type="button" data-di="${di}" data-si="${si}">${esc(s)}</button>`).join("") +
              `</div>`
          )
          .join("")
      : "Нема грешки.";
    out.querySelectorAll("button").forEach((b) =>
      b.addEventListener("click", () => applySuggestion(+b.dataset.di, +b.dataset.si))
    );
  } catch (e) {
    out.textContent = "Грешка при проверката.";
  }
}

// Apply one suggestion to the textarea at its diagnostic offsets, then
// re-check so remaining underlines track the new text. Falls back to the
// old clipboard copy when the text moved under us (stale offsets).
function applySuggestion(di, si) {
  const d = lastDiags[di];
  const s = d && (d.suggestions || [])[si];
  if (!d || s == null) return;
  const el = $("in");
  const fixed = applySuggestionText(el.value, d.char_start, d.char_end, s);
  const span = Array.from(el.value).slice(d.char_start, d.char_end).join("");
  if (span !== d.text) {
    if (navigator.clipboard) navigator.clipboard.writeText(s);
    return;
  }
  el.value = fixed;
  el.focus();
  runCheck();
}

(async () => {
  settings = await loadSettings();
  host = await currentHost();
  if (host) $("host").textContent = host;
  renderStatus();
})();

// Export for node tests; in the browser the consts are globals.
if (typeof module !== "undefined" && module.exports) {
  module.exports = { applySuggestionText, runCheck, applySuggestion };
}
