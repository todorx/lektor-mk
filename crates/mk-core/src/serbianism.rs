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
    ("боље", "подобро"),
    ("ваздух", "воздух"),
    ("ваљда", "веројатно"),
    ("вероватно", "веројатно"),
    ("вожња", "возење"),
    ("вожњата", "возењето"),
    ("где", "каде"),
    ("године", "години"),
    ("данас", "денес"),
    ("детаљи", "детали"),
    ("дешава", "случува"),
    ("допринесе", "придонесе"),
    ("доручек", "појадок"),
    ("доручекот", "појадокот"),
    ("заправо", "всушност"),
    ("зато", "затоа"),
    ("иначе", "инаку"),
    ("ипак", "сепак"),
    ("искључиво", "исклучиво"),
    ("јер", "бидејќи"),
    ("јуче", "вчера"),
    ("кашика", "лажица"),
    ("лопови", "крадци"),
    ("људи", "луѓе"),
    ("много", "многу"),
    ("можда", "можеби"),
    ("нажалост", "за жал"),
    ("највероватно", "најверојатно"),
    ("наравно", "секако"),
    ("насеље", "населба"),
    ("некад", "некогаш"),
    ("никад", "никогаш"),
    ("ништа", "ништо"),
    ("новчаник", "паричник"),
    ("обавезно", "задолжително"),
    ("одмах", "веднаш"),
    ("позориште", "театар"),
    ("понаша", "однесува"),
    ("потрошња", "потрошувачка"),
    ("потрошњата", "потрошувачката"),
    ("радник", "работник"),
    ("рецимо", "на пример"),
    ("сватив", "сфатив"),
    ("свашта", "сешто"),
    ("становништво", "население"),
    ("сутра", "утре"),
    ("тамо", "таму"),
    ("тањир", "чинија"),
    ("тачно", "точно"),
    ("увек", "секогаш"),
    ("уље", "масло"),
    ("уназад", "наназад"),
    ("унапред", "однапред"),
    ("уопште", "воопшто"),
    ("уствари", "всушност"),
    ("утакмица", "натпревар"),
    ("утакмицата", "натпреварот"),
    ("утакмици", "натпревари"),
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
        assert_eq!(macedonian_for("уствари"), Some("всушност"));
        assert_eq!(macedonian_for("секогаш"), None);
    }
}
