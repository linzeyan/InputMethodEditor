//! Chinese syllables and bopomofo phonetic symbols.

pub use self::bopomofo::{Bopomofo, BopomofoErrorKind, BopomofoKind, ParseBopomofoError};
/// Errors during decoding a syllable from a u16.
pub use self::syllable::DecodeSyllableError;
/// Errors when parsing a str to a syllable.
pub use self::syllable::ParseSyllableError;
pub use self::syllable::{BuildSyllableError, Syllable, SyllableBuilder, SyllableErrorKind};

pub use self::syllable::SyllableVec;
pub(crate) use self::syllable::fuzzy_initials;
pub use self::syllable::{
    FUZZY_AN_ANG, FUZZY_C_CH, FUZZY_EN_ENG, FUZZY_F_H, FUZZY_IN_ING, FUZZY_N_L, FUZZY_R_L,
    FUZZY_S_SH, FUZZY_Z_ZH, fuzzy_sounds, set_fuzzy_sounds,
};

mod bopomofo;
mod syllable;
