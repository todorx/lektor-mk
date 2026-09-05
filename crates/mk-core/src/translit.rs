//! Latin → Cyrillic transliteration for Macedonian typed on a Latin keyboard.
//!
//! Macedonians routinely write Macedonian in Latin letters — in chat, in search
//! boxes, on phones without a Cyrillic layout. Converting it back is genuinely
//! useful, but the mapping is ambiguous in both directions:
//!
//! * `nj` is `њ` in `konj` → `коњ`, but `н` + `ј` in `injekcija` → `инјекција`.
//! * `dz` is `ѕ` in `dzvezda` → `ѕвезда`, but `д` + `з` elsewhere.
//! * `c` is usually `ц`, but `к` in unadapted loanwords.
//!
//! Rather than guess, we enumerate the readings and let the lexicon adjudicate:
//! whichever candidate is a real Macedonian word wins. That turns an ambiguous
//! rewrite into a decision backed by evidence.

/// Latin sequences and the Macedonian letters they may stand for.
///
/// Order does not matter — [`candidates`] tries every entry that matches at a
/// given position, so both the digraph and the single-letter readings are
/// explored. [`transliterate`] separately prefers the longest match.
const TABLE: &[(&str, &str)] = &[
    // Digraphs and diacritic forms, longest first.
    ("dzh", "џ"),
    ("dž", "џ"),
    ("zh", "ж"),
    ("ž", "ж"),
    ("ch", "ч"),
    ("č", "ч"),
    ("sh", "ш"),
    ("š", "ш"),
    ("gj", "ѓ"),
    ("gy", "ѓ"),
    ("dj", "ѓ"),
    ("đ", "ѓ"),
    ("ǵ", "ѓ"),
    ("kj", "ќ"),
    ("ky", "ќ"),
    ("ć", "ќ"),
    ("ḱ", "ќ"),
    ("dz", "ѕ"),
    ("lj", "љ"),
    ("nj", "њ"),
    ("ts", "ц"),
    // Single letters.
    ("a", "а"),
    ("b", "б"),
    ("v", "в"),
    ("g", "г"),
    ("d", "д"),
    ("e", "е"),
    ("z", "з"),
    ("i", "и"),
    ("j", "ј"),
    ("k", "к"),
    ("l", "л"),
    ("m", "м"),
    ("n", "н"),
    ("o", "о"),
    ("p", "п"),
    ("r", "р"),
    ("s", "с"),
    ("t", "т"),
    ("u", "у"),
    ("f", "ф"),
    ("h", "х"),
    ("c", "ц"),
    ("w", "в"),
    ("q", "к"),
    ("x", "кс"),
    ("y", "ј"),
];

/// Alternative readings for genuinely ambiguous input.
const ALTERNATES: &[(&str, &str)] = &[("c", "к"), ("j", "џ"), ("y", "и"), ("x", "х")];

/// Maximum number of readings to enumerate for one word.
///
/// A long word with several ambiguous digraphs could otherwise branch
/// exponentially; in practice the true reading appears well inside this bound.
const MAX_CANDIDATES: usize = 64;

/// The single most likely transliteration, using longest-match-first.
///
/// Use this when there is no lexicon to consult. When there is one, prefer
/// [`candidates`], which surfaces the readings this function has to discard.
pub fn transliterate(input: &str) -> String {
    let lower = input.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        let mut matched = false;
        // Longest match wins: try 3-char sequences, then 2, then 1.
        for len in (1..=3).rev() {
            if i + len > chars.len() {
                continue;
            }
            let slice: String = chars[i..i + len].iter().collect();
            if let Some((_, cyr)) = TABLE.iter().find(|(lat, _)| *lat == slice) {
                out.push_str(cyr);
                i += len;
                matched = true;
                break;
            }
        }
        if !matched {
            out.push(chars[i]);
            i += 1;
        }
    }

    restore_case(input, &out)
}

/// Every plausible reading of `input`, for a caller that can check them against
/// a lexicon. The longest-match reading is always included.
pub fn candidates(input: &str) -> Vec<String> {
    let lower = input.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut out = Vec::new();
    expand(&chars, 0, &mut String::new(), &mut out);

    // Re-apply the original capitalisation to each reading.
    let mut seen = Vec::new();
    for c in out {
        let cased = restore_case(input, &c);
        if !seen.contains(&cased) {
            seen.push(cased);
        }
    }
    seen
}

fn expand(chars: &[char], i: usize, acc: &mut String, out: &mut Vec<String>) {
    if out.len() >= MAX_CANDIDATES {
        return;
    }
    if i >= chars.len() {
        out.push(acc.clone());
        return;
    }

    let mut matched_any = false;
    // Longest first, so the most likely reading lands in `out` earliest.
    for len in (1..=3).rev() {
        if i + len > chars.len() {
            continue;
        }
        let slice: String = chars[i..i + len].iter().collect();
        for (lat, cyr) in TABLE.iter().chain(ALTERNATES.iter()) {
            if *lat == slice {
                matched_any = true;
                let mark = acc.len();
                acc.push_str(cyr);
                expand(chars, i + len, acc, out);
                acc.truncate(mark);
            }
        }
    }

    if !matched_any {
        let mark = acc.len();
        acc.push(chars[i]);
        expand(chars, i + 1, acc, out);
        acc.truncate(mark);
    }
}

/// If `original` started with a capital, capitalise `converted` to match.
fn restore_case(original: &str, converted: &str) -> String {
    let starts_upper = original.chars().next().is_some_and(char::is_uppercase);
    if !starts_upper {
        return converted.to_string();
    }
    let mut chars = converted.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => converted.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_plain_words() {
        assert_eq!(transliterate("makedonski"), "македонски");
        assert_eq!(transliterate("zdravo"), "здраво");
        assert_eq!(transliterate("kniga"), "книга");
    }

    #[test]
    fn handles_macedonian_specific_digraphs() {
        assert_eq!(transliterate("kje"), "ќе");
        assert_eq!(transliterate("gjavol"), "ѓавол");
        assert_eq!(transliterate("zhena"), "жена");
        assert_eq!(transliterate("chovek"), "човек");
        assert_eq!(transliterate("shema"), "шема");
        assert_eq!(transliterate("njiva"), "њива");
        assert_eq!(transliterate("ljubov"), "љубов");
    }

    #[test]
    fn accepts_diacritics_as_well_as_digraphs() {
        assert_eq!(transliterate("žena"), "жена");
        assert_eq!(transliterate("čovek"), "човек");
        assert_eq!(transliterate("šema"), "шема");
    }

    #[test]
    fn candidates_cover_both_readings_of_an_ambiguous_digraph() {
        // `nj` is њ in `konj`, but н + ј in `injekcija`.
        let konj = candidates("konj");
        assert!(konj.contains(&"коњ".to_string()), "got {konj:?}");
        assert!(konj.contains(&"конј".to_string()), "got {konj:?}");
    }

    #[test]
    fn candidates_offer_both_readings_of_c() {
        let got = candidates("cel");
        assert!(got.contains(&"цел".to_string()), "got {got:?}");
        assert!(got.contains(&"кел".to_string()), "got {got:?}");
    }

    #[test]
    fn longest_match_reading_comes_first() {
        assert_eq!(candidates("konj").first().map(String::as_str), Some("коњ"));
    }

    #[test]
    fn capitalisation_is_preserved() {
        assert_eq!(transliterate("Skopje"), "Скопје");
        assert_eq!(transliterate("Makedonija"), "Македонија");
    }

    #[test]
    fn candidate_count_stays_bounded() {
        // Many ambiguity points must not blow up.
        assert!(candidates("cjcjcjcjcjcjcj").len() <= MAX_CANDIDATES);
    }
}
