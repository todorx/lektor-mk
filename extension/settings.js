"use strict";
// Shared settings: rule catalogue, defaults, storage, and the diagnostic
// filter. Loaded as a plain script in background, popup, options and content
// scripts (MV2, no bundler). Pure functions stay testable under node — see
// tools/test_settings.js. Never touches the network; everything is local.

// Rule catalogue: stable engine IDs plus Macedonian UI strings.
// Keep in sync with `rule` in crates/mk-core/src/diagnostic.rs.
const MK_RULES = [
  { id: "MK_SPELL", group: "Правопис", name: "Правопис", desc: "Непознат збор, со предлози за исправка." },
  { id: "MK_HOMOGLYPH", group: "Правопис", name: "Измешани писма", desc: "Латинични букви скриени во кирилички збор." },
  { id: "MK_FOREIGN_CYRILLIC", group: "Правопис", name: "Странски кирилични букви", desc: "Букви што ги нема во македонската азбука (ђ, ъ, щ…)." },
  { id: "MK_LATIN_TEXT", group: "Правопис", name: "Текст на латиница", desc: "Македонски пишуван со латински букви." },
  { id: "MK_DOUBLE_DEFINITE", group: "Граматика", name: "Двоен член", desc: "Членот е означен двапати: убавата книгата." },
  { id: "MK_CLITIC_ORDER", group: "Граматика", name: "Ред на клитики", desc: "Дативната клитика стои пред акузативната: ми го, не го ми." },
  { id: "MK_DATIVE_I", group: "Граматика", name: "ѝ наспроти и", desc: "Дативната заменка ѝ со гравис, за разлика од сврзникот и." },
  { id: "MK_L_PARTICIPLE", group: "Граматика", name: "Л-партицип", desc: "Партиципот се сложува со подметот: таа дошла, не дошол." },
  { id: "MK_NE_FUSED", group: "Граматика", name: "Слеано не", desc: "Негацијата стои одвоено од глаголот: не сака, не несака." },
  { id: "MK_NAJ_SEPARATED", group: "Граматика", name: "Разделено нај", desc: "Нај се пишува слеано: најдобар, не нај добар." },
  { id: "MK_PO_SEPARATED", group: "Граматика", name: "Разделено по", desc: "По се пишува слеано во компаратив: подобар, не по добар." },
  { id: "MK_SENTENCE_CAPITAL", group: "Интерпункција", name: "Голема буква", desc: "Реченицата почнува со голема буква." },
  { id: "MK_SPACE_BEFORE_PUNCT", group: "Интерпункција", name: "Белина пред интерпункција", desc: "Нема белина пред запирка, точка и други знаци." },
];

const MK_GROUPS = ["Правопис", "Граматика", "Интерпункција"];

function mkDefaults() {
  return { enabled: true, delayMs: 700, rulesOff: [], userWords: [], siteOff: [] };
}

/// Stored settings merged over defaults; unknown keys are dropped.
function normalizeSettings(stored) {
  const d = mkDefaults();
  if (!stored || typeof stored !== "object") return d;
  return {
    enabled: typeof stored.enabled === "boolean" ? stored.enabled : d.enabled,
    delayMs:
      Number.isInteger(stored.delayMs) && stored.delayMs >= 0 && stored.delayMs <= 5000
        ? stored.delayMs
        : d.delayMs,
    rulesOff: Array.isArray(stored.rulesOff)
      ? stored.rulesOff.filter((r) => typeof r === "string")
      : d.rulesOff,
    userWords: Array.isArray(stored.userWords)
      ? stored.userWords.filter((w) => typeof w === "string").map((w) => w.toLowerCase())
      : d.userWords,
    siteOff: Array.isArray(stored.siteOff)
      ? stored.siteOff.filter((h) => typeof h === "string")
      : d.siteOff,
  };
}

async function loadSettings() {
  const stored = await browser.storage.local.get("mk-settings");
  return normalizeSettings(stored && stored["mk-settings"]);
}

async function saveSettings(settings) {
  await browser.storage.local.set({ "mk-settings": normalizeSettings(settings) });
}

/// Harper-style filter: drop diagnostics from disabled rules and diagnostics
/// whose exact word the user added to the personal dictionary.
function applySettings(diags, settings) {
  const s = normalizeSettings(settings);
  const off = new Set(s.rulesOff);
  const words = new Set(s.userWords);
  return (diags || []).filter(
    (d) => !off.has(d.rule) && !words.has(String(d.text || "").toLowerCase())
  );
}

/// Is inline checking active for this hostname?
function siteEnabled(settings, hostname) {
  const s = normalizeSettings(settings);
  return !!s.enabled && !s.siteOff.includes(hostname || "");
}

// Export for node tests; in the browser the consts are globals.
if (typeof module !== "undefined" && module.exports) {
  module.exports = { MK_RULES, MK_GROUPS, mkDefaults, normalizeSettings, applySettings, siteEnabled, loadSettings, saveSettings };
}
