# Pravopis Rules Batch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement three more mechanically-checkable Pravopis rules (sentence capitals, `по`-comparatives, space-before-punctuation) plus a data audit of abbreviations, each precision-gated like the existing rules.

**Architecture:** New Rust functions in `crates/mk-core/src/grammar.rs`, registered in the existing `check()` dispatcher, following the `naj_separated` pattern: fire only on adjacent tokens, verify against lexicon/morphology, never invent vocabulary. No new crates, no signature changes.

**Tech Stack:** Rust (mk-core), existing `Lexicon`/`Morphology`/`Token` APIs, `cargo test`, `tools/eval_wiki.py`.

**Spec:** Pravopis 2nd ed., chapters III (§47), IV (§207), V (interpunctuation spacing) — user-provided PDF; plus `docs/superpowers/specs/2026-09-19-macedonian-checker-design.md` for project-wide design. Out of scope by user decision: chapter X (transcriptions of foreign names).

## Global Constraints

- Precision over recall, per `grammar.rs:9-23`: a rule fires only when every reading supports it; adjacent tokens only; features agree. When unsure, stay silent.
- Rule IDs in `diagnostic.rs:17-19` are public contract: never rename existing ones; new IDs are `MK_`-prefixed English.
- Messages to the user are in Macedonian.
- Tests live in-file (`grammar.rs` `mod tests`, helpers `run()` at line 524, `lexicon()` at 508, `morphology()` fixture). Run `cargo test -p mk-core <test_name>` per step, full `cargo test` per task.
- Local-only WASM: no new dependencies, no network calls.
- One task = one branch/worktree (see `superpowers:using-git-worktrees`); all tasks touch `grammar.rs`, so the controller merges them sequentially, never in parallel into one branch.

## Review Focus

- Abbreviation dots must not read as sentence ends: `т.е. нешто`, `г. Петров` stay silent — pinned by a test in Task 1.
- `по` as a preposition before nouns (`по пат`, `по полноќ`) stays silent — pinned by a test in Task 2.
- A lowercase document start (`утре ќе врне` pasted as a fragment) fires by design — pinned by a test in Task 1 so the behavior is deliberate, not accidental.
- Emoticons and dotted abbreviations (`:-)`, `т.е.`) must not trigger space-before-punct — pinned by a test in Task 3.
- The capital rule only ever uppercases, never lowercases (`Тој рече: ...` stays untouched) — pinned by a test in Task 1.

---

### Task 1: Sentence-initial capitals (`MK_SENTENCE_CAPITAL`, III §47)

**Files:**
- Modify: `crates/mk-core/src/diagnostic.rs:19-40` (add rule const)
- Modify: `crates/mk-core/src/grammar.rs:29-43` (register in `check()`), append rule fn + tests in `mod tests`
- Test: `cargo test -p mk-core sentence_capital`

**Interfaces:**
- Consumes: `check(tokens: &[Token<'_>], lexicon: &crate::lexicon::Lexicon, morph: &Morphology) -> Vec<Diagnostic>` (grammar.rs:30); `Lexicon::contains(&self, word: &str) -> bool` (lexicon.rs:64); `Token { text, char_start, char_end, kind }`, `TokenKind::{Word, Punct}` (tokenizer.rs:27-34,11-18); `capitalize_first(&str) -> String` (grammar.rs:322, reuse as-is).
- Produces: `fn sentence_capital(tokens: &[Token<'_>], lexicon: &crate::lexicon::Lexicon, out: &mut Vec<Diagnostic>)`; `rule::SENTENCE_CAPITAL`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn flags_lowercase_after_a_full_stop() {
    let found = run("Тој дојде. утре ќе врне.");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, rule::SENTENCE_CAPITAL);
    assert_eq!(found[0].text, "утре");
    assert_eq!(found[0].suggestions, vec!["Утре".to_string()]);
}

#[test]
fn ignores_abbreviation_dots() {
    // The dot in т.е. / г. does not end a sentence.
    assert!(run("Тоа е, т.е. нешто друго.").is_empty());
    assert!(run("Се виде со г. Петров вчера.").is_empty());
}

#[test]
fn only_uppercases_never_lowercases() {
    assert!(run("Тој рече: оди си дома.").is_empty());
    assert!(run("Утре ќе врне.").is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mk-core sentence_capital`
Expected: FAIL — compile error, `rule::SENTENCE_CAPITAL` not found (same for the other two tests; they fail for the same missing item, which is fine).

- [ ] **Step 3: Write minimal implementation**

```rust
// diagnostic.rs, inside `pub mod rule`, after NAJ_SEPARATED:
/// First word of a sentence starts lowercase: `Тој дојде. утре` ✗.
pub const SENTENCE_CAPITAL: &str = "MK_SENTENCE_CAPITAL";
```

```rust
// grammar.rs, register in check() after naj_separated(...):
sentence_capital(tokens, lexicon, &mut out);
```

```rust
/// First word of a sentence is capitalized (§47).
///
/// Fires only for all-lowercase words of 2+ characters that ARE in the
/// lexicon — unknown words stay SPELL's problem, which is what keeps
/// abbreviations (`т.е.`, `г.`) and fragments silent. A dot counts as a
/// sentence end unless the token before it is a single-letter word.
fn sentence_capital(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    out: &mut Vec<Diagnostic>,
) {
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != TokenKind::Word
            || t.text.chars().count() < 2
            || t.text.chars().any(|c| c.is_uppercase())
            || !lexicon.contains(t.text)
        {
            continue;
        }
        let at_start = i == 0;
        let after_ender = i >= 2
            && tokens[i - 1].kind == TokenKind::Punct
            && tokens[i - 1].text.chars().all(|c| ".?!…".contains(c))
            && !(tokens[i - 2].kind == TokenKind::Word
                && tokens[i - 2].text.chars().count() == 1);
        let after_ender = after_ender
            || (i == 1
                && tokens[0].kind == TokenKind::Punct
                && tokens[0].text.chars().all(|c| ".?!…".contains(c)));
        if !at_start && !after_ender {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::SENTENCE_CAPITAL.to_string(),
            severity: Severity::Error,
            char_start: t.char_start,
            char_end: t.char_end,
            text: t.text.to_string(),
            message: "Реченицата почнува со голема буква.".to_string(),
            suggestions: vec![capitalize_first(t.text)],
        });
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mk-core sentence_capital`, then `cargo test -p mk-core`
Expected: PASS (new tests + full suite; the abbreviation test pins Review Focus item 1, the never-lowercases test pins item 5).

- [ ] **Step 5: Commit**

```bash
git add crates/mk-core/src/diagnostic.rs crates/mk-core/src/grammar.rs
git commit -m "feat: MK_SENTENCE_CAPITAL rule (III §47)"
```

### Task 2: Split `по`-comparatives (`MK_PO_SEPARATED`, IV §207)

**Files:**
- Modify: `crates/mk-core/src/diagnostic.rs:19-40` (add rule const)
- Modify: `crates/mk-core/src/grammar.rs:29-43` (register in `check()`), append rule fn + tests in `mod tests`
- Test: `cargo test -p mk-core po_separated`

**Interfaces:**
- Consumes: same `check()` signature; `Morphology::analyze` readings with `Pos::{Adjective, Noun, Verb}` (morphology.rs pos mapping, as used at grammar.rs:294-296); `Lexicon::contains`; `capitalize_first`.
- Produces: `fn po_separated(tokens, lexicon, morph, out)`; `rule::PO_SEPARATED`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn flags_po_split_from_an_adjective() {
    // Fixture needs: добар adj (already in morphology() fixture),
    // "подобар" resolvable — add "подобар" to the test lexicon list AND
    // a morph entry: add("подобар", "добар", &["pref", "comp", "adj", "m", "sg", "nom", "ind"]);
    let found = run("тој е по добар");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, rule::PO_SEPARATED);
    assert_eq!(found[0].suggestions, vec!["подобар".to_string()]);
}

#[test]
fn po_before_a_plain_noun_stays_silent() {
    // Prepositional по + noun with no fused form: add "пат" noun to the
    // morphology fixture: add("пат", "пат", &["n", "m", "sg", "nom", "ind"]);
    // "попат" must NOT be in the test lexicon or morph fixture.
    assert!(run("тој оди по пат").is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mk-core po_separated`
Expected: FAIL — compile error, `rule::PO_SEPARATED` not found.

- [ ] **Step 3: Write minimal implementation**

```rust
// diagnostic.rs, inside `pub mod rule`, after SENTENCE_CAPITAL:
/// `по` split from its comparative: `по добар` ✗, `подобар` ✓.
pub const PO_SEPARATED: &str = "MK_PO_SEPARATED";
```

```rust
// grammar.rs, register in check() after sentence_capital(...):
po_separated(tokens, lexicon, morph, &mut out);
```

```rust
/// `по` fuses with comparatives (§207): `подобар` ✓, `по добар` ✗.
///
/// Same shape as `naj_separated`: the fused form must already exist in the
/// lexicon or morphology, which is what keeps prepositional `по пат`
/// (`попат` is not a word) silent.
fn po_separated(
    tokens: &[Token<'_>],
    lexicon: &crate::lexicon::Lexicon,
    morph: &Morphology,
    out: &mut Vec<Diagnostic>,
) {
    for pair in tokens.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);
        if first.kind != TokenKind::Word || second.kind != TokenKind::Word {
            continue;
        }
        if first.text.to_lowercase() != "по" {
            continue;
        }
        let ok_pos = morph.analyze(second.text).iter().any(|a| {
            matches!(a.pos(), Some(Pos::Adjective | Pos::Noun | Pos::Verb))
        });
        if !ok_pos {
            continue;
        }
        let fused = format!("по{}", second.text.to_lowercase());
        if !lexicon.contains(&fused) && morph.analyze(&fused).is_empty() {
            continue;
        }
        let fix = if first.text.starts_with(char::is_uppercase) {
            capitalize_first(&fused)
        } else {
            fused
        };
        out.push(Diagnostic {
            rule: rule::PO_SEPARATED.to_string(),
            severity: Severity::Error,
            char_start: first.char_start,
            char_end: second.char_end,
            text: format!("{} {}", first.text, second.text),
            message: "По се пишува слеано со зборот што го степенува.".to_string(),
            suggestions: vec![fix],
        });
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mk-core po_separated`, then `cargo test -p mk-core`
Expected: PASS (the noun test pins Review Focus item 2).

- [ ] **Step 5: Commit**

```bash
git add crates/mk-core/src/diagnostic.rs crates/mk-core/src/grammar.rs
git commit -m "feat: MK_PO_SEPARATED rule (IV §207)"
```

### Task 3: Space before punctuation (`MK_SPACE_BEFORE_PUNCT`, V)

**Files:**
- Modify: `crates/mk-core/src/diagnostic.rs:19-40` (add rule const)
- Modify: `crates/mk-core/src/grammar.rs:29-43` (register in `check()`), append rule fn + tests in `mod tests`
- Test: `cargo test -p mk-core space_before_punct`

**Interfaces:**
- Consumes: same `check()` signature (`lexicon`/`morph` unused by this rule — whitespace gaps come from byte offsets, not analysis); `Token { text, byte_start, byte_end, char_start, char_end, kind }`; `TokenKind::{Word, Number, Punct}`.
- Produces: `fn space_before_punct(tokens: &[Token<'_>], out: &mut Vec<Diagnostic>)`; `rule::SPACE_BEFORE_PUNCT`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn flags_space_before_a_comma() {
    let found = run("Тој дојде , а таа не.");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, rule::SPACE_BEFORE_PUNCT);
    assert_eq!(found[0].suggestions, vec![",".to_string()]);
}

#[test]
fn leaves_emoticons_and_abbreviations_alone() {
    // No gap, no fire — even where characters look odd.
    assert!(run("Тој дојде :-)").is_empty());
    assert!(run("Се виде со г. Петров.").is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mk-core space_before_punct`
Expected: FAIL — compile error, `rule::SPACE_BEFORE_PUNCT` not found.

- [ ] **Step 3: Write minimal implementation**

```rust
// diagnostic.rs, inside `pub mod rule`, after PO_SEPARATED:
/// Whitespace before closing punctuation: `здраво ,` ✗.
pub const SPACE_BEFORE_PUNCT: &str = "MK_SPACE_BEFORE_PUNCT";
```

```rust
// grammar.rs, register in check():
space_before_punct(tokens, &mut out);
```

```rust
/// No whitespace before closing punctuation (V).
///
/// A gap is whitespace the tokenizer skipped: closing punct must start
/// exactly where the previous token ends. Severity is Warning — this is
/// typing style, and emoticons (`:-)`) have no gap so they never fire.
fn space_before_punct(tokens: &[Token<'_>], out: &mut Vec<Diagnostic>) {
    const CLOSERS: &[&str] = &[",", ".", "!", "?", ":", ";", "…", "”", "’", ")"];
    for pair in tokens.windows(2) {
        let (prev, curr) = (&pair[0], &pair[1]);
        if curr.kind != TokenKind::Punct || !CLOSERS.contains(&curr.text) {
            continue;
        }
        if !matches!(prev.kind, TokenKind::Word | TokenKind::Number) {
            continue;
        }
        if curr.byte_start == prev.byte_end {
            continue;
        }
        out.push(Diagnostic {
            rule: rule::SPACE_BEFORE_PUNCT.to_string(),
            severity: Severity::Warning,
            char_start: prev.char_end,
            char_end: curr.char_end,
            text: format!(" {}", curr.text),
            suggestions: vec![curr.text.to_string()],
        });
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mk-core space_before_punct`, then `cargo test -p mk-core`
Expected: PASS (emoticon test pins Review Focus item 4).

- [ ] **Step 5: Commit**

```bash
git add crates/mk-core/src/diagnostic.rs crates/mk-core/src/grammar.rs
git commit -m "feat: MK_SPACE_BEFORE_PUNCT rule (V)"
```

### Task 4: Abbreviation data audit (VII) + eval + README

**Files:**
- Read: `data/supplement/mk_supplement.txt` (check current entries first)
- Modify: `data/supplement/mk_supplement.txt` (append only dotless categorical gaps)
- Modify: `README.md` (rules table + planned-rules section — read those sections first, do not guess their shape)
- Run: `python tools/eval_wiki.py data/mk.fst data/mk.morph`, `cargo test`

**Interfaces:**
- Consumes: Tasks 1–3 merged (the controller merges all rule branches first; this task starts from merged master).
- Produces: updated supplement + README rows; eval numbers quoted in README.

- [ ] **Step 1: Audit dotless abbreviations**

Run: `grep -i -E "^(км|мм|см|кг|мл|д-р|м-р|стр|бр|т|ул|мин|сек)$" data/supplement/mk_supplement.txt`
Expected: output shows which are already present. Dotted ones (`т.е.`, `итн.`) tokenize into skipped single letters — verify with `cargo run -p mk-cli -- check data/mk.fst "т.е. и итн."` expecting no MK_SPELL on the fragments, and do NOT add them.

- [ ] **Step 2: Append only genuine categorical gaps**

```text
# <one line: what class, e.g. "# Time and counting (VII)">
<word>
```

Rule: one form per line, only whole classes from Pravopis VII (units, titles, bibliographic), never individual corpus finds. If Step 1 shows no gaps, skip this step with a one-line note in the commit message.

- [ ] **Step 3: Rebuild data and run the full gate**

Run: the repo's lexicon build (see README build section for the exact command), then `cargo test`
Expected: PASS full suite.

- [ ] **Step 4: Eval + README**

Run: `python tools/eval_wiki.py data/mk.fst data/mk.morph`
Expected: prints flag rate + per-rule counts; record the three new rules' hit counts. In README, add three rows to the rules table (`MK_SENTENCE_CAPITAL`, `MK_PO_SEPARATED`, `MK_SPACE_BEFORE_PUNCT` with one-line descriptions) and strike through the matching planned-rules bullets. Quote the measured eval numbers; do not invent numbers.

- [ ] **Step 5: Commit**

```bash
git add data/supplement/mk_supplement.txt README.md
git commit -m "data: abbreviation audit + README for Pravopis batch"
```

## Self-Review

- Spec coverage: III §47 → Task 1; IV §207 (по) → Task 2; V spacing → Task 3; VII abbreviations → Task 4. Chapter X explicitly excluded by the user. Chapters I, II, VI, VIII, IX, XI, XII need a parser, pronunciation data, per-language maps, or are reference lists — documented as out of scope in the coverage answer, not silently dropped.
- Placeholder scan: every step has exact code, exact commands, exact expected output. No TBD/TODO. Type names (`Token`, `TokenKind`, `Diagnostic`, `Severity`, `Pos`, `Lexicon`, `Morphology`) match the signatures read from the repo.
- Type consistency: all three rules use the existing `check()` signature `(tokens, lexicon, morph)`; Task 3 ignores two params like existing single-source rules do. `capitalize_first` reused, not redefined. Test helper `run(text)` unchanged.
- Review Focus: items 1+5 → Task 1 tests; item 2 → Task 2 test; item 4 → Task 3 test; item 3 → Task 1 fragment test. All five pinned.
