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
