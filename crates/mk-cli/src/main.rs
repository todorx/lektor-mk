//! Command-line driver: compiles the lexicon and runs the checker.
//!
//! ```text
//! mk build-lexicon data/interim/mk_wordlist.utf8.txt data/mk.fst
//! mk check data/mk.fst "Тој ја видe кnигата."
//! mk check data/mk.fst --file article.txt --json
//! ```

use std::io::Read;
use std::process::ExitCode;

use mk_core::{Checker, Lexicon, Severity};

const USAGE: &str = "\
mk — Macedonian spelling checker

USAGE:
    mk build-lexicon <wordlist.txt>... <out.fst>
    mk check <lexicon.fst> <text>
    mk check <lexicon.fst> --file <path> [--json]
    mk check <lexicon.fst> --stdin [--json]
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("build-lexicon") => build_lexicon(&args[1..]),
        Some("check") => check(&args[1..]),
        Some("-h") | Some("--help") | None => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(format!("unknown command: {other}\n\n{USAGE}")),
    };

    match result {
        Ok(found_problems) => {
            if found_problems {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn build_lexicon(args: &[String]) -> Result<bool, String> {
    let Some((output, inputs)) = args.split_last() else {
        return Err(format!("build-lexicon needs at least one input and an output\n\n{USAGE}"));
    };
    if inputs.is_empty() {
        return Err(format!("build-lexicon needs at least one input\n\n{USAGE}"));
    }

    let mut words: Vec<String> = Vec::new();
    let mut raw_size = 0usize;
    for input in inputs {
        let text = std::fs::read_to_string(input).map_err(|e| format!("reading {input}: {e}"))?;
        raw_size += text.len();
        let before = words.len();
        // `#` starts a comment so the curated supplements can explain themselves.
        words.extend(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string),
        );
        eprintln!("read      {:>7} lines from {input}", words.len() - before);
    }

    let bytes = Lexicon::build_from_unsorted(words).map_err(|e| format!("building FST: {e}"))?;
    let fst_size = bytes.len();
    std::fs::write(output, &bytes).map_err(|e| format!("writing {output}: {e}"))?;

    let lex = Lexicon::from_bytes(bytes).map_err(|e| format!("verifying FST: {e}"))?;
    eprintln!("lexicon   {:>7} unique forms", lex.len());
    eprintln!(
        "wrote     {output}  {:.2} MB  (from {:.2} MB of text, {:.1}x smaller)",
        fst_size as f64 / 1e6,
        raw_size as f64 / 1e6,
        raw_size as f64 / fst_size as f64
    );
    Ok(false)
}

fn check(args: &[String]) -> Result<bool, String> {
    let Some(fst_path) = args.first() else {
        return Err(format!("check needs a lexicon path\n\n{USAGE}"));
    };
    let rest = &args[1..];
    let json = rest.iter().any(|a| a == "--json");

    let text = if let Some(pos) = rest.iter().position(|a| a == "--file") {
        let path = rest.get(pos + 1).ok_or("--file needs a path")?;
        std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?
    } else if rest.iter().any(|a| a == "--stdin") {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).map_err(|e| format!("reading stdin: {e}"))?;
        buf
    } else {
        rest.iter().filter(|a| !a.starts_with("--")).cloned().collect::<Vec<_>>().join(" ")
    };

    if text.trim().is_empty() {
        return Err("no text to check".to_string());
    }

    let bytes = std::fs::read(fst_path).map_err(|e| format!("reading {fst_path}: {e}"))?;
    let checker = Checker::new(bytes).map_err(|e| format!("loading lexicon: {e}"))?;

    let started = std::time::Instant::now();
    let found = checker.check(&text);
    let elapsed = started.elapsed();

    if json {
        let out = serde_json::to_string_pretty(&found).map_err(|e| e.to_string())?;
        println!("{out}");
    } else if found.is_empty() {
        println!("Нема пронајдени проблеми.");
    } else {
        for d in &found {
            let mark = match d.severity {
                Severity::Error => "ГРЕШКА ",
                Severity::Warning => "ПРЕДУПР",
                Severity::Info => "ИНФО   ",
            };
            print!("{mark} {:>4}:{:<4} {:<22} {}", d.char_start, d.char_end, d.text, d.message);
            if d.suggestions.is_empty() {
                println!();
            } else {
                println!("  → {}", d.suggestions.join(", "));
            }
            println!("        [{}]", d.rule);
        }
        eprintln!(
            "\n{} проблем(и) во {} збора, за {:.1} ms",
            found.len(),
            text.split_whitespace().count(),
            elapsed.as_secs_f64() * 1000.0
        );
    }

    Ok(!found.is_empty())
}
