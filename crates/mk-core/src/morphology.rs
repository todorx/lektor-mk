//! Morphological analysis: surface form → lemma plus grammatical features.
//!
//! Spelling only needs to know *whether* a word exists. Grammar needs to know
//! *what it is* — that `книгата` is a definite feminine singular noun, that
//! `дошла` is a feminine л-participle. This module provides that lookup, and is
//! the foundation every grammar rule will stand on.
//!
//! # Storage
//!
//! Analyses are enormously repetitive: 185,896 of them share about 30,000
//! lemmas and a far smaller number of distinct tag combinations. Storing them
//! as text would cost several megabytes — too much to ship in a browser
//! extension alongside the spelling lexicon. So everything is interned:
//!
//! * tag names appear once in a vocabulary,
//! * lemmas appear once in a table,
//! * each distinct combination of tags appears once,
//! * an analysis is then just a pair of integer ids.
//!
//! The whole structure, FST included, serialises to one self-contained blob so
//! that a caller has a single file to load.
//!
//! # Binary format
//!
//! All integers little-endian.
//!
//! ```text
//! magic     "MKMORPH2"                       8 bytes
//! fst       u32 length, then that many bytes  (surface -> entry index)
//! vocab     u32 count, each: u16 len + UTF-8  (tag names)
//! lemmas    u32 count, each: u16 len + UTF-8
//! tagsets   u32 count, each: u8 len + u16 vocab ids
//! entries   u32 count, each: u8 len + (u32 lemma id, u32 tagset id) pairs
//! lp_forms  u32 count, each: u32 lemma id + u16 len + UTF-8 surface
//!           (reverse index for л-participle suggestions only — the full
//!           surface table would triple the blob past the extension budget)
//! ```

use std::collections::BTreeMap;

use fst::{Map, MapBuilder};

const MAGIC: &[u8; 8] = b"MKMORPH2";

/// Something went wrong loading or building a morphology table.
#[derive(Debug)]
pub enum Error {
    /// The blob is not a morphology file.
    BadMagic,
    /// The blob ended in the middle of a structure.
    Truncated,
    /// An id pointed outside its table.
    BadIndex,
    /// A tag name or lemma was not valid UTF-8.
    BadUtf8,
    /// The underlying FST rejected the data.
    Fst(fst::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadMagic => write!(f, "not a morphology file"),
            Error::Truncated => write!(f, "morphology data ends unexpectedly"),
            Error::BadIndex => write!(f, "morphology data has an out-of-range index"),
            Error::BadUtf8 => write!(f, "morphology data has invalid UTF-8"),
            Error::Fst(e) => write!(f, "fst: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<fst::Error> for Error {
    fn from(e: fst::Error) -> Self {
        Error::Fst(e)
    }
}

/// Part of speech, for the categories grammar rules actually branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pos {
    Noun,
    ProperNoun,
    Adjective,
    Verb,
    Pronoun,
    Preposition,
    Conjunction,
    Numeral,
    Adverb,
    Particle,
    Determiner,
    Interjection,
    Abbreviation,
}

/// Grammatical gender. Macedonian marks three, plus underspecified forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Masculine,
    Feminine,
    Neuter,
    /// Shared across genders — the tag `mfn` or `mf`.
    Common,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Number {
    Singular,
    Plural,
    /// The count form after a numeral: `два стола`, not `два столови`.
    Count,
}

/// Macedonian marks definiteness as a suffix, in three deictic series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Definiteness {
    /// No article: `книга`.
    Indefinite,
    /// Neutral `-от/-та/-то`: `книгата`.
    Definite,
    /// Proximate `-ов/-ва/-во`: `книгава` — near the speaker.
    Proximate,
    /// Distal `-он/-на/-но`: `книгана` — away from both.
    Distal,
}

/// A single reading of a surface form.
#[derive(Debug, Clone, Copy)]
pub struct Analysis<'a> {
    lemma: &'a str,
    tags: &'a [u16],
    vocab: &'a [String],
}

impl<'a> Analysis<'a> {
    /// The dictionary form.
    pub fn lemma(&self) -> &'a str {
        self.lemma
    }

    /// Every Apertium tag on this reading, in order.
    pub fn tags(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.tags.iter().map(move |&i| self.vocab[i as usize].as_str())
    }

    /// Does this reading carry `tag`? Tag names are Apertium's, e.g. `"lp"`.
    pub fn has(&self, tag: &str) -> bool {
        self.tags.iter().any(|&i| self.vocab[i as usize] == tag)
    }

    pub fn pos(&self) -> Option<Pos> {
        self.tags().find_map(|t| match t {
            "n" => Some(Pos::Noun),
            "np" => Some(Pos::ProperNoun),
            "adj" => Some(Pos::Adjective),
            "vblex" | "vbser" | "vbhaver" | "vbmod" | "vaux" => Some(Pos::Verb),
            "prn" => Some(Pos::Pronoun),
            "pr" => Some(Pos::Preposition),
            "cnjcoo" | "cnjsub" | "cnjadv" => Some(Pos::Conjunction),
            "num" => Some(Pos::Numeral),
            "adv" => Some(Pos::Adverb),
            "part" => Some(Pos::Particle),
            "det" => Some(Pos::Determiner),
            "ij" => Some(Pos::Interjection),
            "abbr" => Some(Pos::Abbreviation),
            _ => None,
        })
    }

    pub fn gender(&self) -> Option<Gender> {
        self.tags().find_map(|t| match t {
            // `mi`/`ma` split masculine by animacy; both are masculine.
            "m" | "mi" | "ma" => Some(Gender::Masculine),
            "f" => Some(Gender::Feminine),
            "nt" => Some(Gender::Neuter),
            "mfn" | "mf" => Some(Gender::Common),
            _ => None,
        })
    }

    pub fn number(&self) -> Option<Number> {
        self.tags().find_map(|t| match t {
            "sg" => Some(Number::Singular),
            "pl" => Some(Number::Plural),
            "ct" => Some(Number::Count),
            _ => None,
        })
    }

    pub fn definiteness(&self) -> Option<Definiteness> {
        self.tags().find_map(|t| match t {
            "ind" => Some(Definiteness::Indefinite),
            "def" => Some(Definiteness::Definite),
            "prx" => Some(Definiteness::Proximate),
            "dst" => Some(Definiteness::Distal),
            _ => None,
        })
    }

    /// True for the л-participle (`дошол`, `дошла`, `дошле`), which must agree
    /// with its subject in gender and number.
    pub fn is_l_participle(&self) -> bool {
        self.has("lp")
    }

    /// True when the form carries any definite article.
    pub fn is_definite(&self) -> bool {
        matches!(
            self.definiteness(),
            Some(Definiteness::Definite | Definiteness::Proximate | Definiteness::Distal)
        )
    }
}

/// Surface form → analyses.
pub struct Morphology {
    map: Map<Vec<u8>>,
    vocab: Vec<String>,
    lemmas: Vec<String>,
    tagsets: Vec<Vec<u16>>,
    /// Per entry, the (lemma id, tagset id) pairs making up its readings.
    entries: Vec<Vec<(u32, u32)>>,
    /// (lemma id, surface) for readings tagged `lp`. The only reverse index
    /// rules get: enough to propose an agreeing participle, small enough
    /// (~12k rows) to keep the blob shippable.
    lp_forms: Vec<(u32, String)>,
}

impl Morphology {
    /// Number of distinct surface forms.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Distinct lemmas known to the table.
    pub fn lemma_count(&self) -> usize {
        self.lemmas.len()
    }

    /// Every reading of `form`. Empty if the form is unknown.
    ///
    /// Lookup is case-insensitive, falling back to the lowercased form so that
    /// a sentence-initial capital still analyses.
    pub fn analyze(&self, form: &str) -> Vec<Analysis<'_>> {
        let index = self.map.get(form).or_else(|| self.map.get(form.to_lowercase()));
        let Some(index) = index else {
            return Vec::new();
        };
        let Some(readings) = self.entries.get(index as usize) else {
            return Vec::new();
        };
        readings
            .iter()
            .filter_map(|&(lemma_id, tagset_id)| {
                Some(Analysis {
                    lemma: self.lemmas.get(lemma_id as usize)?.as_str(),
                    tags: self.tagsets.get(tagset_id as usize)?.as_slice(),
                    vocab: &self.vocab,
                })
            })
            .collect()
    }

    /// Is `form` known to the morphology at all?
    pub fn contains(&self, form: &str) -> bool {
        self.map.get(form).is_some() || self.map.get(form.to_lowercase()).is_some()
    }

    /// Every stored л-participle surface form of `lemma` (empty when unknown).
    ///
    /// Forward lookup is surface → analyses; this is the narrow reverse index
    /// rules need to propose an agreeing participle without inventing one.
    pub fn participle_forms(&self, lemma: &str) -> Vec<&str> {
        let Some(id) = self.lemmas.iter().position(|l| l == lemma) else {
            return Vec::new();
        };
        // ponytail: linear scan over ~12k rows, error path only; index it if ever hot
        self.lp_forms
            .iter()
            .filter(|(lid, _)| *lid == id as u32)
            .map(|(_, s)| s.as_str())
            .collect()
    }

    /// Serialise a morphology table.
    ///
    /// `entries` maps each surface form to its readings, a reading being a
    /// lemma and its list of tags. The map must be ordered, which `BTreeMap`
    /// guarantees, because the FST requires sorted keys.
    pub fn build(entries: &BTreeMap<String, Vec<(String, Vec<String>)>>) -> Result<Vec<u8>, Error> {
        let mut vocab: Vec<String> = Vec::new();
        let mut vocab_ids: BTreeMap<String, u16> = BTreeMap::new();
        let mut lemmas: Vec<String> = Vec::new();
        let mut lemma_ids: BTreeMap<String, u32> = BTreeMap::new();
        let mut tagsets: Vec<Vec<u16>> = Vec::new();
        let mut tagset_ids: BTreeMap<Vec<u16>, u32> = BTreeMap::new();

        let mut table: Vec<Vec<(u32, u32)>> = Vec::with_capacity(entries.len());
        let mut lp_forms: Vec<(u32, String)> = Vec::new();
        let mut fst_builder = MapBuilder::memory();

        for (surface, readings) in entries {
            let mut packed = Vec::with_capacity(readings.len());
            for (lemma, tags) in readings {
                let lemma_id = *lemma_ids.entry(lemma.clone()).or_insert_with(|| {
                    lemmas.push(lemma.clone());
                    (lemmas.len() - 1) as u32
                });
                let tag_ids: Vec<u16> = tags
                    .iter()
                    .map(|t| {
                        *vocab_ids.entry(t.clone()).or_insert_with(|| {
                            vocab.push(t.clone());
                            (vocab.len() - 1) as u16
                        })
                    })
                    .collect();
                let tagset_id = *tagset_ids.entry(tag_ids.clone()).or_insert_with(|| {
                    tagsets.push(tag_ids);
                    (tagsets.len() - 1) as u32
                });
                packed.push((lemma_id, tagset_id));
            }
            // Reverse index input: one row per л-participle reading.
            for (lemma_id, tagset_id) in packed.iter() {
                if tagsets[*tagset_id as usize].iter().any(|&t| vocab[t as usize] == "lp") {
                    lp_forms.push((*lemma_id, surface.clone()));
                }
            }
            fst_builder.insert(surface, table.len() as u64)?;
            table.push(packed);
        }

        let fst_bytes = fst_builder.into_inner()?;

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(fst_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&fst_bytes);

        write_strings(&mut out, &vocab);
        write_strings(&mut out, &lemmas);

        out.extend_from_slice(&(tagsets.len() as u32).to_le_bytes());
        for set in &tagsets {
            out.push(set.len() as u8);
            for &id in set {
                out.extend_from_slice(&id.to_le_bytes());
            }
        }

        out.extend_from_slice(&(table.len() as u32).to_le_bytes());
        for readings in &table {
            out.push(readings.len().min(u8::MAX as usize) as u8);
            for &(lemma_id, tagset_id) in readings.iter().take(u8::MAX as usize) {
                out.extend_from_slice(&lemma_id.to_le_bytes());
                out.extend_from_slice(&tagset_id.to_le_bytes());
            }
        }

        lp_forms.sort_unstable();
        lp_forms.dedup();
        out.extend_from_slice(&(lp_forms.len() as u32).to_le_bytes());
        for (lemma_id, surface) in &lp_forms {
            out.extend_from_slice(&lemma_id.to_le_bytes());
            out.extend_from_slice(&(surface.len() as u16).to_le_bytes());
            out.extend_from_slice(surface.as_bytes());
        }

        Ok(out)
    }

    /// Load a table produced by [`Morphology::build`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { data: bytes, pos: 0 };

        if r.take(MAGIC.len())? != MAGIC {
            return Err(Error::BadMagic);
        }

        let fst_len = r.u32()? as usize;
        let fst_bytes = r.take(fst_len)?.to_vec();
        let map = Map::new(fst_bytes)?;

        let vocab = r.strings()?;
        let lemmas = r.strings()?;

        let tagset_count = r.u32()? as usize;
        let mut tagsets = Vec::with_capacity(tagset_count);
        for _ in 0..tagset_count {
            let n = r.u8()? as usize;
            let mut set = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.u16()?;
                if id as usize >= vocab.len() {
                    return Err(Error::BadIndex);
                }
                set.push(id);
            }
            tagsets.push(set);
        }

        let entry_count = r.u32()? as usize;
        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let n = r.u8()? as usize;
            let mut readings = Vec::with_capacity(n);
            for _ in 0..n {
                let lemma_id = r.u32()?;
                let tagset_id = r.u32()?;
                if lemma_id as usize >= lemmas.len() || tagset_id as usize >= tagsets.len() {
                    return Err(Error::BadIndex);
                }
                readings.push((lemma_id, tagset_id));
            }
            entries.push(readings);
        }

        let lp_count = r.u32()? as usize;
        let mut lp_forms = Vec::with_capacity(lp_count);
        for _ in 0..lp_count {
            let lemma_id = r.u32()?;
            if lemma_id as usize >= lemmas.len() {
                return Err(Error::BadIndex);
            }
            let len = r.u16()? as usize;
            let bytes = r.take(len)?;
            lp_forms.push((
                lemma_id,
                std::str::from_utf8(bytes).map_err(|_| Error::BadUtf8)?.to_string(),
            ));
        }

        Ok(Self { map, vocab, lemmas, tagsets, entries, lp_forms })
    }
}

fn write_strings(out: &mut Vec<u8>, items: &[String]) {
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for s in items {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let slice = self.data.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn strings(&mut self) -> Result<Vec<String>, Error> {
        let count = self.u32()? as usize;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let len = self.u16()? as usize;
            let bytes = self.take(len)?;
            out.push(std::str::from_utf8(bytes).map_err(|_| Error::BadUtf8)?.to_string());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn sample() -> Morphology {
        let mut entries: BTreeMap<String, Vec<(String, Vec<String>)>> = BTreeMap::new();
        entries.insert(
            "книга".into(),
            vec![("книга".into(), tags(&["n", "f", "sg", "nom", "ind"]))],
        );
        entries.insert(
            "книгата".into(),
            vec![("книга".into(), tags(&["n", "f", "sg", "nom", "def"]))],
        );
        entries.insert(
            "книгава".into(),
            vec![("книга".into(), tags(&["n", "f", "sg", "nom", "prx"]))],
        );
        entries.insert(
            "дошла".into(),
            vec![("дојде".into(), tags(&["vblex", "perf", "lp", "f", "sg"]))],
        );
        entries.insert(
            "дошол".into(),
            vec![("дојде".into(), tags(&["vblex", "perf", "lp", "m", "sg"]))],
        );
        // A genuinely ambiguous form with two readings.
        entries.insert(
            "жени".into(),
            vec![
                ("жена".into(), tags(&["n", "f", "pl", "nom", "ind"])),
                ("жени".into(), tags(&["vblex", "impf", "pres", "p3", "sg"])),
            ],
        );
        Morphology::from_bytes(&Morphology::build(&entries).unwrap()).unwrap()
    }

    #[test]
    fn round_trips_through_the_binary_format() {
        let m = sample();
        assert_eq!(m.len(), 6);
        assert_eq!(m.lemma_count(), 4); // книга, дојде, жена, жени
    }

    #[test]
    fn reads_features_off_a_definite_noun() {
        let m = sample();
        let a = m.analyze("книгата");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].lemma(), "книга");
        assert_eq!(a[0].pos(), Some(Pos::Noun));
        assert_eq!(a[0].gender(), Some(Gender::Feminine));
        assert_eq!(a[0].number(), Some(Number::Singular));
        assert_eq!(a[0].definiteness(), Some(Definiteness::Definite));
        assert!(a[0].is_definite());
    }

    #[test]
    fn distinguishes_the_three_deictic_series() {
        let m = sample();
        assert_eq!(m.analyze("книга")[0].definiteness(), Some(Definiteness::Indefinite));
        assert_eq!(m.analyze("книгата")[0].definiteness(), Some(Definiteness::Definite));
        assert_eq!(m.analyze("книгава")[0].definiteness(), Some(Definiteness::Proximate));
        assert!(!m.analyze("книга")[0].is_definite());
        assert!(m.analyze("книгава")[0].is_definite());
    }

    #[test]
    fn identifies_l_participles_and_their_gender() {
        let m = sample();
        let f = m.analyze("дошла");
        assert!(f[0].is_l_participle());
        assert_eq!(f[0].gender(), Some(Gender::Feminine));

        let masc = m.analyze("дошол");
        assert!(masc[0].is_l_participle());
        assert_eq!(masc[0].gender(), Some(Gender::Masculine));
        assert_eq!(masc[0].lemma(), "дојде");
    }

    #[test]
    fn keeps_every_reading_of_an_ambiguous_form() {
        let m = sample();
        let a = m.analyze("жени");
        assert_eq!(a.len(), 2);
        assert!(a.iter().any(|x| x.pos() == Some(Pos::Noun)));
        assert!(a.iter().any(|x| x.pos() == Some(Pos::Verb)));
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let m = sample();
        assert!(!m.analyze("Книгата").is_empty());
        assert!(m.contains("КНИГАТА"));
    }

    #[test]
    fn unknown_forms_analyse_to_nothing() {
        let m = sample();
        assert!(m.analyze("непостоечки").is_empty());
        assert!(!m.contains("непостоечки"));
    }

    #[test]
    fn raw_tags_are_reachable() {
        let m = sample();
        let a = m.analyze("дошла");
        assert!(a[0].has("perf"));
        assert!(!a[0].has("impf"));
        assert_eq!(a[0].tags().collect::<Vec<_>>(), vec!["vblex", "perf", "lp", "f", "sg"]);
    }

    #[test]
    fn reverse_lookup_lists_a_lemmas_participles() {
        let m = sample();
        let mut forms = m.participle_forms("дојде");
        forms.sort_unstable();
        assert_eq!(forms, vec!["дошла", "дошол"]);
        assert!(m.participle_forms("книга").is_empty());
        assert!(m.participle_forms("непостоечка").is_empty());
    }

    #[test]
    fn rejects_the_previous_format_version() {
        let mut bytes = Morphology::build(
            &[("книга".to_string(), vec![("книга".to_string(), tags(&["n"]))])]
                .into_iter()
                .collect(),
        )
        .unwrap();
        bytes[..8].copy_from_slice(b"MKMORPH1");
        assert!(matches!(Morphology::from_bytes(&bytes), Err(Error::BadMagic)));
    }

    #[test]
    fn rejects_corrupt_input() {
        assert!(matches!(Morphology::from_bytes(b"not a morph file"), Err(Error::BadMagic)));
        assert!(matches!(Morphology::from_bytes(b"MKMORPH1"), Err(Error::BadMagic)));
    }
}
