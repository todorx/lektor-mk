"use strict";
// Harper-style inline checking: a transparent mirror div floats over the
// focused field with the same font metrics, drawing wavy underlines under
// flagged words. Clicking an underline opens a fix card. The field's own DOM
// is never rewritten, so caret state stays intact.
// Only TEXTAREA, text/search INPUTs and contenteditable are ever read —
// email, password, number and every other type are never touched.
(() => {
  if (window.__mkInit) return;
  window.__mkInit = true;

  const MIRROR_ID = "__mk_mirror";
  const CARD_ID = "__mk_card";
  const MAX_UNDERLINES = 100; // ponytail: global cap per field, worst case stays cheap
  let field = null;
  let singleLine = false;
  let timer = 0;
  let checking = false;
  let dirty = false;
  let diags = [];
  let cardDiag = null;
  let delayMs = 700;
  let active = true;
  let retries = 0;
  // Cold event page (WASM + lexicon reload) can reject the first message.
  const MAX_RETRIES = 6;
  // Android keyboards overlay the page instead of resizing it, so a field low
  // on screen keeps both its text and our underlines behind the keyboard.
  // ponytail: fixed 40% line; Android exposes no keyboard-height API to JS.
  const KEYBOARD_SAFE_RATIO = 0.4;
  const scrollDelta = (rectBottom, viewportHeight) => {
    const safe = viewportHeight * KEYBOARD_SAFE_RATIO;
    return rectBottom > safe ? rectBottom - safe : 0;
  };
  const retryDelay = (attempt) => (attempt < MAX_RETRIES ? 300 * (attempt + 1) : 0);

  const esc = (s) =>
    s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  // Allowlist: anything not listed here (password, email, number, …) is ignored.
  const isField = (el) => {
    if (!el || el.disabled || el.readOnly) return false;
    if (el.tagName === "TEXTAREA") return true;
    if (el.tagName === "INPUT") return el.type === "text" || el.type === "search";
    return !!el.isContentEditable;
  };
  const getText = (el) => ("value" in el ? el.value : el.innerText || "");
  const plainField = () => field && "value" in field;
  // Rust reports Unicode scalar offsets; Array.from keeps astral characters
  // intact where plain indexing would split surrogates.
  const splice = (s, start, end, rep) => {
    const c = Array.from(s);
    return c.slice(0, start).join("") + rep + c.slice(end).join("");
  };

  function ensureMirror() {
    let m = document.getElementById(MIRROR_ID);
    if (m) return m;
    m = document.createElement("div");
    m.id = MIRROR_ID;
    m.setAttribute("aria-hidden", "true");
    m.addEventListener("click", (e) => {
      const span = e.target.closest?.("[data-i]");
      if (span && field) openCard(diags[+span.dataset.i], e.clientX, e.clientY);
      e.stopPropagation();
    });
    document.documentElement.appendChild(m);
    return m;
  }

  function ensureCard() {
    let c = document.getElementById(CARD_ID);
    if (c) return c;
    c = document.createElement("div");
    c.id = CARD_ID;
    c.style.display = "none";
    document.documentElement.appendChild(c);
    return c;
  }

  function copyMetrics(m) {
    const cs = getComputedStyle(field);
    for (const p of ["fontFamily", "fontSize", "fontWeight", "fontStyle", "letterSpacing",
      "textTransform", "lineHeight", "textAlign", "direction", "overflowWrap", "wordBreak"]) {
      m.style[p] = cs[p];
    }
    const px = (v) => parseFloat(v) || 0;
    m.style.padding =
      `${px(cs.borderTopWidth) + px(cs.paddingTop)}px ${px(cs.borderRightWidth) + px(cs.paddingRight)}px ` +
      `${px(cs.borderBottomWidth) + px(cs.paddingBottom)}px ${px(cs.borderLeftWidth) + px(cs.paddingLeft)}px`;
    m.style.whiteSpace = singleLine ? "pre" : "pre-wrap";
  }

  function mirrorHtml(text, items) {
    const chars = Array.from(text);
    const parts = chars.map(esc);
    const valid = items
      .map((d, i) => ({ d, i }))
      .filter(({ d }) => d.char_start >= 0 && d.char_end <= chars.length && d.char_start < d.char_end)
      .slice(0, MAX_UNDERLINES)
      .sort((a, b) => b.d.char_start - a.d.char_start);
    for (const { d, i } of valid) {
      parts[d.char_end] = "</span>" + (parts[d.char_end] ?? "");
      parts[d.char_start] =
        `<span class="mk-u mk-${esc(d.severity)}" data-i="${i}">` + (parts[d.char_start] ?? "");
    }
    let html = parts.join("").replace(/\n/g, "<br>");
    if (text.endsWith("\n")) html += "<br>";
    return html;
  }

  function position() {
    const m = document.getElementById(MIRROR_ID);
    if (!m || !field || !document.contains(field)) {
      if (m) m.style.display = "none";
      return false;
    }
    const r = field.getBoundingClientRect();
    if (r.width === 0 || r.bottom < 0 || r.top > innerHeight) {
      m.style.display = "none";
      return false;
    }
    m.style.display = "block";
    m.style.left = r.left + "px";
    m.style.top = r.top + "px";
    m.style.width = r.width + "px";
    m.style.height = r.height + "px";
    m.scrollTop = field.scrollTop || 0;
    m.scrollLeft = field.scrollLeft || 0;
    return true;
  }

  function render() {
    const m = ensureMirror();
    copyMetrics(m);
    // Page text is escaped per-char inside mirrorHtml(); the lint warning
    // on this assignment is a false positive.
    m.innerHTML = mirrorHtml(getText(field), diags);
    position();
  }

  async function check(el) {
    if (checking) {
      dirty = true;
      return;
    }
    if (!active) return;
    checking = true;
    try {
      const res = await browser.runtime.sendMessage({ type: "mk-check", text: getText(el) });
      retries = 0;
      if (el === field) {
        diags = res && res.ok ? JSON.parse(res.json) : [];
        render();
      }
    } catch (e) {
      // Retry so a cold background recovers without requiring another keystroke.
      const wait = el === field && active ? retryDelay(retries++) : 0;
      if (wait) setTimeout(() => { if (el === field && active) check(el); }, wait);
    }
    checking = false;
    if (dirty && el === field) {
      dirty = false;
      schedule();
    }
  }

  function schedule() {
    clearTimeout(timer);
    if (!field) return;
    timer = setTimeout(() => field && check(field), delayMs);
  }

  // Master toggle, per-site toggle and delay come from settings; changes
  // apply live without a page reload.
  async function refreshActive() {
    try {
      const s = await loadSettings();
      delayMs = s.delayMs;
      const was = active;
      active = siteEnabled(s, location.hostname);
      if (!active && was) {
        clearTimeout(timer);
        diags = [];
        closeCard();
        const m = document.getElementById(MIRROR_ID);
        if (m) m.style.display = "none";
        field = null;
      } else if (active && !was && field) {
        check(field);
      }
    } catch (e) {
      /* storage unreachable; keep last known state */
    }
  }

  browser.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes["mk-settings"]) refreshActive();
  });
  refreshActive();

  function openCard(d, x, y) {
    if (!d) return;
    cardDiag = d;
    const c = ensureCard();
    const plain = plainField();
    c.innerHTML =
      `<div class="mk-head"><b>${esc(d.text)}</b><button class="mk-x" type="button">✕</button></div>` +
      `<div class="mk-rule">${esc(ruleName(d.rule))}</div>` +
      `<span>${esc(d.message)}</span><div>` +
      (d.suggestions || [])
        .slice(0, 3)
        .map((s) => `<button type="button" data-s="${esc(s)}">${plain ? "Замени" : "Копирај"}: ${esc(s)}</button>`)
        .join("") +
      `</div><button type="button" class="mk-ignore" data-ignore="1">Игнорирај: ${esc(d.text)}</button>`;
    c.querySelector(".mk-x").addEventListener("click", closeCard);
    c.querySelectorAll("button[data-s]").forEach((btn) =>
      btn.addEventListener("click", () => applyFix(btn.dataset.s))
    );
    const ign = c.querySelector("button[data-ignore]");
    if (ign) ign.addEventListener("click", () => ignoreWord(d.text));
    c.style.display = "block";
    c.style.left = Math.max(8, Math.min(x, innerWidth - 320)) + "px";
    c.style.top = (y > innerHeight - 220 ? y - 190 : y + 14) + "px";
  }

  // Harper-style ignore: the exact word never flags again, everywhere.
  async function ignoreWord(word) {
    try {
      const s = await loadSettings();
      const w = String(word || "").toLowerCase();
      if (w && !s.userWords.includes(w)) {
        s.userWords.push(w);
        await saveSettings(s);
      }
    } catch (e) {
      /* storage unreachable; ignore silently */
    }
    closeCard();
    if (field) check(field);
  }

  function ruleName(id) {
    const r = (typeof MK_RULES !== "undefined" ? MK_RULES : []).find((x) => x.id === id);
    return r ? r.name : id;
  }

  function closeCard() {
    const c = document.getElementById(CARD_ID);
    if (c) c.style.display = "none";
    cardDiag = null;
  }

  // Frameworks (React, …) revert direct `.value` assignment on controlled
  // inputs; going through the native setter updates their state instead.
  function setNativeValue(el, text) {
    const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(el, text);
    else el.value = text;
    el.dispatchEvent(new Event("input", { bubbles: true }));
  }

  // Scalar (code-point) index → UTF-16 code-unit index for Range offsets.
  function scalarToCU(s, k) {
    let cu = 0, i = 0;
    for (const ch of s) {
      if (i >= k) break;
      cu += ch.length;
      i++;
    }
    return cu;
  }

  // Replace a scalar range inside a contenteditable's DOM. Returns false when
  // the DOM text doesn't match the checked text (block breaks, hidden nodes),
  // in which case the caller falls back to clipboard copy.
  function replaceInRich(el, start, end, rep, expectWord) {
    try {
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      const nodes = [];
      let total = 0;
      while (walker.nextNode()) {
        const data = walker.currentNode.data;
        const len = Array.from(data).length;
        nodes.push({ node: walker.currentNode, start: total, end: total + len });
        total += len;
      }
      const all = nodes.map((x) => x.node.data).join("");
      const units = Array.from(all);
      if (units.slice(start, end).join("") !== expectWord) return false;
      const at = (pos) => {
        const hit = nodes.find((x) => pos >= x.start && pos <= x.end) ?? nodes[nodes.length - 1];
        return [hit.node, scalarToCU(hit.node.data, pos - hit.start)];
      };
      const [sn, so] = at(start);
      const [en, eo] = at(end);
      const range = document.createRange();
      range.setStart(sn, so);
      range.setEnd(en, eo);
      range.deleteContents();
      range.insertNode(document.createTextNode(rep));
      el.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: rep }));
      return true;
    } catch (e) {
      return false;
    }
  }

  function applyFix(suggestion) {
    if (!cardDiag || !field) return;
    const { char_start, char_end, text } = cardDiag;
    if (plainField()) {
      closeCard();
      setNativeValue(field, splice(field.value, char_start, char_end, suggestion));
      check(field); // dispatched input also schedules; direct call refreshes now
    } else if (replaceInRich(field, char_start, char_end, suggestion, text)) {
      closeCard();
      check(field);
    } else if (navigator.clipboard) {
      navigator.clipboard.writeText(suggestion);
      closeCard();
    }
  }

  document.addEventListener(
    "focusin",
    (e) => {
      if (!active || !isField(e.target)) return;
      closeCard();
      field = e.target;
      singleLine = field.tagName === "INPUT";
      diags = [];
      retries = 0;
      // Android: lift the field clear of the overlaid keyboard before checking,
      // otherwise the text and its underlines sit behind it.
      if (/Android/i.test(navigator.userAgent || "")) {
        const d = scrollDelta(field.getBoundingClientRect().bottom, innerHeight);
        if (d) window.scrollBy(0, d);
      }
      check(field);
    },
    true
  );
  document.addEventListener("input", (e) => {
    if (e.target === field) {
      closeCard();
      schedule();
    }
  });
  // Capture phase catches page, frame and field-internal scrolls alike.
  window.addEventListener("scroll", () => field && position(), true);
  window.addEventListener("resize", () => field && render());
  document.addEventListener("click", (e) => {
    if (!e.target.closest?.("#" + CARD_ID) && !e.target.closest?.("[data-i]")) closeCard();
  });

  // Export the pure decisions for node; in the browser this script is a plain
  // content script with no module system.
  if (typeof module !== "undefined" && module.exports) {
    module.exports = { scrollDelta, retryDelay, MAX_RETRIES };
  }
})();
