"use strict";
// Options page: general toggles, personal dictionary, searchable rule list.
// Saves through settings.js; content scripts pick changes up live.
const $ = (id) => document.getElementById(id);

let settings = mkDefaults();

function ruleOff(id) {
  return settings.rulesOff.includes(id);
}

function renderSites() {
  const ul = $("siteOff");
  ul.innerHTML = "";
  if (!settings.siteOff.length) {
    const li = document.createElement("li");
    li.className = "mk-empty";
    li.textContent = "Нема исклучени сајтови.";
    ul.appendChild(li);
    return;
  }
  for (const host of [...settings.siteOff].sort()) {
    const li = document.createElement("li");
    li.className = "mk-list-row";
    const span = document.createElement("span");
    span.textContent = host;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = "Вклучи";
    btn.addEventListener("click", async () => {
      settings.siteOff = settings.siteOff.filter((h) => h !== host);
      await saveSettings(settings);
      renderSites();
    });
    li.append(span, btn);
    ul.appendChild(li);
  }
}

function ruleMatches(r, q) {
  return !q || r.name.toLowerCase().includes(q) || r.desc.toLowerCase().includes(q);
}

function renderRules() {
  const q = $("search").value.trim().toLowerCase();
  const box = $("rules");
  box.innerHTML = "";
  for (const group of MK_GROUPS) {
    const items = MK_RULES.filter((r) => r.group === group && ruleMatches(r, q));
    if (!items.length) continue;
    const h = document.createElement("h3");
    h.textContent = group;
    box.appendChild(h);
    for (const r of items) {
      const label = document.createElement("label");
      label.className = "mk-row";
      const text = document.createElement("span");
      const name = document.createElement("strong");
      name.textContent = r.name;
      const desc = document.createElement("small");
      desc.textContent = r.desc;
      text.append(name, desc);
      const input = document.createElement("input");
      input.type = "checkbox";
      input.className = "mk-switch";
      input.checked = !ruleOff(r.id);
      input.addEventListener("change", async () => {
        settings.rulesOff = input.checked
          ? settings.rulesOff.filter((id) => id !== r.id)
          : [...settings.rulesOff, r.id];
        await saveSettings(settings);
        refreshToggleAll();
      });
      label.append(text, input);
      box.appendChild(label);
    }
  }
}

function refreshToggleAll() {
  $("toggleAll").textContent = settings.rulesOff.length ? "Вклучи ги сите" : "Исклучи ги сите";
}

async function init() {
  settings = await loadSettings();
  $("enabled").checked = settings.enabled;
  $("delayMs").value = settings.delayMs;
  $("userWords").value = settings.userWords.join("\n");
  renderSites();
  renderRules();
  refreshToggleAll();

  $("enabled").addEventListener("change", async (e) => {
    settings.enabled = e.target.checked;
    await saveSettings(settings);
  });
  $("delayMs").addEventListener("change", async (e) => {
    settings.delayMs = Math.max(0, Math.min(5000, parseInt(e.target.value, 10) || 0));
    e.target.value = settings.delayMs;
    await saveSettings(settings);
  });
  let dictTimer = 0;
  $("userWords").addEventListener("input", (e) => {
    clearTimeout(dictTimer);
    dictTimer = setTimeout(async () => {
      settings.userWords = e.target.value.split("\n").map((w) => w.trim().toLowerCase()).filter(Boolean);
      await saveSettings(settings);
    }, 400);
  });
  $("search").addEventListener("input", renderRules);
  $("reset").addEventListener("click", async () => {
    const keep = settings;
    settings = { ...mkDefaults(), siteOff: keep.siteOff };
    await saveSettings(settings);
    $("enabled").checked = settings.enabled;
    $("delayMs").value = settings.delayMs;
    $("userWords").value = "";
    renderSites();
    renderRules();
    refreshToggleAll();
  });
  $("toggleAll").addEventListener("click", async () => {
    settings.rulesOff = settings.rulesOff.length ? [] : MK_RULES.map((r) => r.id);
    await saveSettings(settings);
    renderRules();
    refreshToggleAll();
  });
}

init();
