//! Serbian words in Macedonian text, with the Macedonian word to use instead.
//!
//! Most of these are absent from the lexicon, so without this table they would
//! be reported as plain misspellings with edit-distance suggestions that miss
//! the real fix (`увек` is nowhere near `секогаш`). A few are in the upstream
//! wordlist as colloquial borrowings; those get a quiet style note instead.
//!
//! Exact surface forms only: a word that is also a Macedonian word, or common
//! in colloquial Macedonian (`али`, `чак`), does not belong here.

const PAIRS: &[(&str, &str)] = &[
    ("вероватно", "веројатно"),
    ("ваздух", "воздух"),
    ("где", "каде"),
    ("данас", "денес"),
    ("искључиво", "исклучиво"),
    ("ипак", "сепак"),
    ("јер", "бидејќи"),
    ("јуче", "вчера"),
    ("кашика", "лажица"),
    ("људи", "луѓе"),
    ("можда", "можеби"),
    ("наравно", "секако"),
    ("никад", "никогаш"),
    ("ништа", "ништо"),
    ("одмах", "веднаш"),
    ("сутра", "утре"),
    ("тамо", "таму"),
    ("тачно", "точно"),
    ("увек", "секогаш"),
    ("хвала", "благодарам"),
    ("хлеб", "леб"),
];

/// The Macedonian word for a lowercased Serbian one, if listed.
pub fn macedonian_for(lower: &str) -> Option<&'static str> {
    PAIRS.iter().find(|(sr, _)| *sr == lower).map(|(_, mk)| *mk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_listed_words_only() {
        assert_eq!(macedonian_for("увек"), Some("секогаш"));
        assert_eq!(macedonian_for("секогаш"), None);
    }
}
