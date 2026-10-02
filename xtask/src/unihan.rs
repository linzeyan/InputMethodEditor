// SPDX-License-Identifier: GPL-3.0-or-later

//! Characters libchewing's dictionary lacks, with their reading from Unicode's
//! Unihan database, for libchewing to offer after every other word.

use std::collections::HashSet;
use std::fmt::Write;
use std::io::{Cursor, Read};
use std::path::Path;

use chewing::dictionary::StringTable;
use chewing::editor::zhuyin_layout::{Pinyin, SyllableEditor};
use chewing::input::KeyboardEvent;
use chewing::input::keysym::Keysym;
use chewing::user::UserDict;
use scoped_error::{Error, expect_error};
use zip::ZipArchive;

use crate::download::download_unihan;

/// Writes `unihan_dict.csv` into `dict_dir`, which has libchewing's dictionary.
pub(crate) fn write_unihan_dict(dict_dir: &Path) -> Result<(), Error> {
    expect_error("failed to build the Unihan dictionary", || {
        let mut zip = ZipArchive::new(Cursor::new(download_unihan()?))?;
        let mut readings = String::new();
        zip.by_name("Unihan_Readings.txt")?
            .read_to_string(&mut readings)?;
        let mut variants = String::new();
        zip.by_name("Unihan_Variants.txt")?
            .read_to_string(&mut variants)?;
        let words = StringTable::open_bin(dict_dir.join("static_words.bin"))?;

        // Simplified forms: libchewing has their traditional characters, and
        // converts to them when simplified output is on.
        let simplified: HashSet<char> = fields(&variants, "kTraditionalVariant")
            .filter(|&(ch, value)| !value.split(' ').any(|v| code_point(v) == Some(ch)))
            .map(|(ch, _)| ch)
            .collect();
        let mut csv = String::new();
        let mut count = 0;
        let mut nasals = 0;
        for (ch, value) in fields(&readings, "kMandarin") {
            // Beyond the BMP too, though fonts there are patchy: an
            // unrendered candidate still beats a character one can't type.
            if simplified.contains(&ch) || words.get_wid(&ch.to_string()).is_some() {
                continue;
            }
            // Of two readings, the first is the mainland's, the second Taiwan's.
            let pinyin = value.rsplit(' ').next().unwrap_or(value);
            let Some(zhuyin) = zhuyin(pinyin) else {
                if syllabic_nasal(pinyin) {
                    nasals += 1;
                    continue;
                }
                Err(format!("no zhuyin for {ch}'s reading {pinyin}"))?
            };
            // The lowest a user dictionary takes, as libchewing's rare words get.
            writeln!(csv, "{ch},{zhuyin},{}", UserDict::MIN)?;
            count += 1;
        }
        std::fs::write(dict_dir.join("unihan_dict.csv"), csv)?;
        // Unicode's license asks for its notice to go with the data.
        std::fs::copy(
            "xtask/unicode-license.txt",
            dict_dir.join("unihan_license.txt"),
        )?;
        eprintln!("Wrote {count} characters from Unihan, skipped {nasals} read as m or n");
        Ok(())
    })
}

/// `(character, value)` of each `field` line in a Unihan file.
fn fields<'a>(text: &'a str, field: &'a str) -> impl Iterator<Item = (char, &'a str)> {
    text.lines().filter_map(move |line| {
        let mut parts = line.split('\t');
        let ch = code_point(parts.next()?)?;
        (parts.next()? == field).then_some((ch, parts.next()?))
    })
}

/// `U+4A3B` as 䨻.
fn code_point(text: &str) -> Option<char> {
    char::from_u32(u32::from_str_radix(text.strip_prefix("U+")?, 16).ok()?)
}

/// Tone-marked pinyin (bèng) as zhuyin (ㄅㄥˋ).
fn zhuyin(pinyin: &str) -> Option<String> {
    const MARKED: [(char, [char; 4]); 6] = [
        ('a', ['ā', 'á', 'ǎ', 'à']),
        ('e', ['ē', 'é', 'ě', 'è']),
        ('i', ['ī', 'í', 'ǐ', 'ì']),
        ('o', ['ō', 'ó', 'ǒ', 'ò']),
        ('u', ['ū', 'ú', 'ǔ', 'ù']),
        ('v', ['ǖ', 'ǘ', 'ǚ', 'ǜ']),
    ];
    let key = |c: char| KeyboardEvent::builder().ksym(Keysym::from(c)).build();
    let mut editor = Pinyin::hanyu();
    // Unmarked is the neutral tone.
    let mut tone = '5';
    for c in pinyin.chars() {
        let letter = match c {
            'a'..='z' => c,
            'ü' => 'v',
            _ => {
                let (letter, marks) = MARKED.iter().find(|(_, marks)| marks.contains(&c))?;
                tone = char::from_digit(marks.iter().position(|&m| m == c)? as u32 + 1, 10)?;
                *letter
            }
        };
        editor.key_press(key(letter));
    }
    editor.key_press(key(tone));
    let syllable = editor.read();
    (syllable.has_initial() || syllable.has_medial() || syllable.has_rime())
        .then(|| syllable.to_string())
}

/// Syllabic m or n (𠮾 ǹ), which zhuyin has no syllable for: any other
/// reading that finds none is a bug in [`zhuyin`].
fn syllabic_nasal(pinyin: &str) -> bool {
    !pinyin
        .chars()
        .any(|c| "aeiouüāáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ".contains(c))
}

#[cfg(test)]
mod tests {
    use super::{syllabic_nasal, zhuyin};

    #[test]
    fn only_syllabic_nasals_may_lack_zhuyin() {
        assert!(syllabic_nasal("ǹ"));
        assert!(syllabic_nasal("ḿ"));
        assert!(!syllabic_nasal("bèng"));
        assert!(!syllabic_nasal("lǘ"));
    }

    #[test]
    fn zhuyin_from_tone_marked_pinyin() {
        assert_eq!(zhuyin("bèng").as_deref(), Some("ㄅㄥˋ"));
        assert_eq!(zhuyin("xiāng").as_deref(), Some("ㄒㄧㄤ"));
        assert_eq!(zhuyin("lǘ").as_deref(), Some("ㄌㄩˊ"));
        assert_eq!(zhuyin("jǔ").as_deref(), Some("ㄐㄩˇ"));
        assert_eq!(zhuyin("ma").as_deref(), Some("ㄇㄚ˙"));
        // ê and syllabic m aren't typable in libchewing's pinyin.
        assert_eq!(zhuyin("ḿ"), None);
    }
}
