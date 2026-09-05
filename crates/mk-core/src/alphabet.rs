//! The Macedonian alphabet, and script classification for individual characters.
//!
//! Macedonian uses a 31-letter Cyrillic alphabet. Any Cyrillic letter *outside*
//! that set (Russian `ъ ы э я ю щ ё`, Serbian `ђ ћ`, Ukrainian `і ї`) is a
//! reliable signal that the text came from the wrong keyboard layout or was
//! pasted from another language.

/// The 31 letters of the Macedonian alphabet, lowercase, in alphabetical order.
pub const MK_LETTERS: [char; 31] = [
    'а', 'б', 'в', 'г', 'д', 'ѓ', 'е', 'ж', 'з', 'ѕ', 'и', 'ј', 'к', 'л', 'љ', 'м', 'н', 'њ', 'о',
    'п', 'р', 'с', 'т', 'ќ', 'у', 'ф', 'х', 'ц', 'ч', 'џ', 'ш',
];

/// Which writing system a character belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    /// A letter in the 31-letter Macedonian alphabet.
    Macedonian,
    /// A Cyrillic letter that is *not* part of the Macedonian alphabet.
    ForeignCyrillic,
    /// A Latin letter.
    Latin,
    /// A decimal digit.
    Digit,
    /// Anything else: punctuation, whitespace, symbols.
    Other,
}

/// Grave-accented vowels. These are not separate letters of the alphabet and
/// have no place in it, but they are correct Macedonian spelling: the accent is
/// what separates two different words that are otherwise identical.
///
/// * `сѐ` "everything" vs `се` the reflexive clitic
/// * `ѝ` the dative pronoun "to her" vs `и` the conjunction "and"
/// * `нѐ`, `вѐ` stressed pronouns vs their clitic forms
///
/// Note the codepoints: the correct characters are Cyrillic `ѐ` U+0450 and
/// `ѝ` U+045D. Latin `è` U+00E8 and `ì` U+00EC look identical and are a common
/// substitution — those stay wrong, and the homoglyph rule catches them.
pub const MK_ACCENTED: [char; 2] = ['ѐ', 'ѝ'];

/// True if `c` is one of the 31 Macedonian letters or an accented vowel,
/// in either case.
pub fn is_mk_letter(c: char) -> bool {
    // Lowercasing a Macedonian capital always yields exactly one char, so the
    // single-char fast path is sound here.
    let lower = c.to_lowercase().next().unwrap_or(c);
    MK_LETTERS.contains(&lower) || MK_ACCENTED.contains(&lower)
}

/// True if `c` lies in one of the Unicode Cyrillic blocks.
pub fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}' | '\u{2DE0}'..='\u{2DFF}' | '\u{A640}'..='\u{A69F}')
}

/// Classify a single character.
pub fn script_of(c: char) -> Script {
    if is_mk_letter(c) {
        Script::Macedonian
    } else if is_cyrillic(c) {
        Script::ForeignCyrillic
    } else if c.is_ascii_alphabetic() || (c.is_alphabetic() && c.is_ascii()) {
        Script::Latin
    } else if c.is_alphabetic() && !is_cyrillic(c) {
        // Latin letters carrying diacritics: ž, č, š, ć, đ, ǵ, ḱ …
        Script::Latin
    } else if c.is_numeric() {
        Script::Digit
    } else {
        Script::Other
    }
}

/// Characters that may appear *inside* a Macedonian word.
///
/// The apostrophe is a real letter-like character in Macedonian: it marks the
/// syllabic /ə/ in words such as `к'смет` and word-initial syllabic *р* in
/// `'рж`, `'рбет`. The hyphen joins compounds (`црно-бел`), abbreviations
/// (`д-р`, `г-ѓа`) and suffixed ordinals (`1-ви`, `2-ри`).
pub fn is_word_internal(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{02BC}' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macedonian_letters_are_recognised() {
        for c in MK_LETTERS {
            assert_eq!(script_of(c), Script::Macedonian, "lowercase {c}");
            let upper = c.to_uppercase().next().unwrap();
            assert_eq!(script_of(upper), Script::Macedonian, "uppercase {upper}");
        }
    }

    #[test]
    fn accented_vowels_count_as_macedonian() {
        // сѐ / ѝ are correct Macedonian, not foreign contamination.
        for c in ['ѐ', 'ѝ', 'Ѐ', 'Ѝ'] {
            assert_eq!(script_of(c), Script::Macedonian, "{c} U+{:04X}", c as u32);
        }
    }

    #[test]
    fn latin_lookalikes_of_the_accented_vowels_stay_foreign() {
        // è U+00E8 and ì U+00EC are Latin and remain errors.
        assert_eq!(script_of('è'), Script::Latin);
        assert_eq!(script_of('ì'), Script::Latin);
    }

    #[test]
    fn foreign_cyrillic_is_separated_from_macedonian() {
        // Russian, Serbian and Ukrainian letters absent from the Macedonian alphabet.
        for c in ['ъ', 'ы', 'э', 'я', 'ю', 'щ', 'ё', 'ђ', 'ћ', 'і', 'ї'] {
            assert_eq!(script_of(c), Script::ForeignCyrillic, "{c}");
        }
    }

    #[test]
    fn latin_and_digits_are_classified() {
        assert_eq!(script_of('a'), Script::Latin);
        assert_eq!(script_of('Z'), Script::Latin);
        assert_eq!(script_of('ž'), Script::Latin);
        assert_eq!(script_of('7'), Script::Digit);
        assert_eq!(script_of(' '), Script::Other);
        assert_eq!(script_of('.'), Script::Other);
    }
}
