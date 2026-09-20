"use strict";
// Popup: status + master/site toggles + paste-text check + options link.
// Diagnostics arrive already filtered by background.js.
const $ = (id) => document.getElementById(id);
const esc = (s) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

let settings = mkDefaults();
let host = "";

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

$("go").addEventListener("click", async () => {
  const out = $("out");
  out.textContent = "Проверува…";
  try {
    const res = await browser.runtime.sendMessage({ type: "mk-check", text: $("in").value });
    const diags = res && res.ok ? JSON.parse(res.json) : [];
    out.innerHTML = diags.length
      ? diags
          .map(
            (d) =>
              `<div class="mk-item"><b>${esc(d.text)}</b><i>${esc(ruleName(d.rule))}</i>` +
              `<span>${esc(d.message)}</span>` +
              (d.suggestions || []).slice(0, 3).map((s) => `<button type="button">${esc(s)}</button>`).join("") +
              `</div>`
          )
          .join("")
      : "Нема грешки.";
    out.querySelectorAll("button").forEach((b) =>
      b.addEventListener("click", () => navigator.clipboard && navigator.clipboard.writeText(b.textContent))
    );
  } catch (e) {
    out.textContent = "Грешка при проверката.";
  }
});

(async () => {
  settings = await loadSettings();
  host = await currentHost();
  if (host) $("host").textContent = host;
  renderStatus();
})();
