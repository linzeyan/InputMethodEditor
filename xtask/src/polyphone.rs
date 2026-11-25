// SPDX-License-Identifier: GPL-3.0-or-later

//! How much less likely a character read several ways is at each reading
//! than at its commonest, for libchewing to rank it by the reading typed
//! when one syllable is all there is to go by.
//!
//! libchewing's language model counts a character once however it is read:
//! 紅 brings what ㄏㄨㄥˊ earned to ㄍㄨㄥ, which it has in 女紅 alone, and
//! comes before 工 there. Unihan's kHanyuPinlu counts each reading of 3,800
//! characters in running text, which says how to split that count.
//!
//! Those counts are the mainland's, so they only judge the readings they
//! list. A reading they lack is Taiwan's (期 ㄑㄧˊ), one typed loosely (和
//! ㄏㄢˋ) or a rare one (紅 ㄍㄨㄥ); the words libchewing's own dictionary
//! spells with it tell the first from the others. A character they lack
//! altogether is judged the same way from kMandarin, the reading it is known
//! by: else 骯, which they lack, would come before 慷 at ㄎㄤˇ, a reading as
//! rare for one as for the other.

use std::collections::HashMap;
use std::fmt::Write;
use std::io::{Cursor, Read};
use std::path::Path;

use chewing::dictionary::StringTable;
use chewing::lm::{LoadMode, StaticDict, StaticLm};
use chewing::zhuyin::{Bopomofo, Syllable};
use scoped_error::{Error, expect_error};
use zip::ZipArchive;

use crate::download::download_unihan;
use crate::unihan::{fields, zhuyin};

/// Penalties are a user dictionary's boosts, which libchewing reads as a
/// twentieth of a tenfold each.
const PER_TENFOLD: f64 = 20.0;
/// The most a reading the counts lack takes: a hundredfold. It may be how
/// Taiwan says a common word (我和你 with ㄏㄢˋ), and the word before it
/// has to be able to bring it back.
const UNLISTED_FLOOR: f64 = 0.01;
/// The share from which a reading ranks with the commonest. The counts are
/// another region's and era's: they tell a rare reading from a common one,
/// not which of two common ones comes first (長 is counted zhǎng more than
/// cháng).
const AS_COMMON: f64 = 1.0 / 3.0;

/// Writes `polyphone_dict.csv` into `dict_dir`, which has libchewing's dictionary.
pub(crate) fn write_polyphone_dict(dict_dir: &Path) -> Result<(), Error> {
    expect_error("failed to build the polyphone dictionary", || {
        let mut zip = ZipArchive::new(Cursor::new(download_unihan()?))?;
        let mut unihan = String::new();
        zip.by_name("Unihan_Readings.txt")?
            .read_to_string(&mut unihan)?;
        let words = StringTable::open_bin(dict_dir.join("static_words.bin"))?;
        let dict = StaticDict::open(dict_dir.join("static_dict.bin"))?;
        let lm = StaticLm::open(dict_dir.join("static_lm.bin"), LoadMode::Eager)?;

        let entries: Vec<(Vec<Syllable>, String, f64)> = dict
            .entries()
            .into_iter()
            .filter_map(|(syllables, wid)| {
                Some((syllables, words.get_text(wid)?, 10f64.powf(lm.unigram(wid))))
            })
            .collect();
        let rows = polyphone_rows(&unihan, &entries);

        // The report that started this: were it to come back, the release
        // must not be built.
        let penalty = |word: &str, reading: &str| {
            rows.iter()
                .find(|row| row.0.to_string() == word && row.1.to_string() == reading)
                .map(|row| row.2)
        };
        if !matches!(penalty("紅", "ㄍㄨㄥ"), Some(..=-20)) || penalty("紅", "ㄏㄨㄥˊ") != Some(0)
        {
            Err("紅 has to rank lower at ㄍㄨㄥ than at ㄏㄨㄥˊ")?;
        }

        let mut csv = String::new();
        for (ch, reading, penalty) in &rows {
            writeln!(csv, "{ch},{reading},{penalty}")?;
        }
        std::fs::write(dict_dir.join("polyphone_dict.csv"), csv)?;
        eprintln!(
            "Wrote {} readings of characters read several ways",
            rows.len()
        );
        Ok(())
    })
}

/// `(character, reading, penalty)` for every reading of each character that
/// is less likely at one of them. `entries` is the dictionary: a word, the
/// syllables it is spelled with, and its probability.
fn polyphone_rows(
    unihan: &str,
    entries: &[(Vec<Syllable>, String, f64)],
) -> Vec<(char, Syllable, i8)> {
    let counts: HashMap<char, Vec<(Syllable, f64)>> = fields(unihan, "kHanyuPinlu")
        .map(|(ch, value)| (ch, value.split(' ').filter_map(counted_reading).collect()))
        .collect();
    // The reading a character is known by. Of two, the first is the
    // mainland's and the second Taiwan's.
    let customary: HashMap<char, Vec<Syllable>> = fields(unihan, "kMandarin")
        .map(|(ch, value)| (ch, value.split(' ').filter_map(syllable).collect()))
        .collect();

    // A word spelled several ways is as likely as itself in all, not in each.
    let mut spellings: HashMap<&str, f64> = HashMap::new();
    for (_, word, _) in entries {
        *spellings.entry(word).or_default() += 1.0;
    }
    let mut characters: HashMap<char, Character> = HashMap::new();
    for (syllables, word, probability) in entries {
        if word.chars().count() != syllables.len() {
            continue;
        }
        for (ch, &reading) in word.chars().zip(syllables) {
            let character = characters.entry(ch).or_default();
            if syllables.len() == 1 {
                character.readings.push(reading);
                character.alone = *probability;
            } else {
                *character.in_words.entry(reading).or_default() +=
                    probability / spellings[word.as_str()];
            }
        }
    }

    let mut rows = vec![];
    for (ch, character) in characters {
        let counts = counts.get(&ch).map_or(&[][..], Vec::as_slice);
        let customary = customary.get(&ch).map_or(&[][..], Vec::as_slice);
        if character.readings.len() < 2 || (counts.is_empty() && customary.is_empty()) {
            continue;
        }
        let penalties = character.penalties(counts, customary);
        if penalties.iter().any(|&penalty| penalty < 0) {
            rows.extend(
                character
                    .readings
                    .iter()
                    .zip(penalties)
                    .map(|(&reading, penalty)| (ch, reading, penalty)),
            );
        }
    }
    rows.sort_by_key(|&(ch, reading, _)| (ch, reading.to_u16()));
    rows
}

/// What libchewing's dictionary says of a character.
#[derive(Default)]
struct Character {
    /// The readings it has as a word of its own.
    readings: Vec<Syllable>,
    /// Its probability as a word of its own.
    alone: f64,
    /// The probability of the longer words each reading spells.
    in_words: HashMap<Syllable, f64>,
}

impl Character {
    /// What each of `readings` takes off the character's rank, given how
    /// often it was counted with each reading and which it is known by.
    fn penalties(&self, counts: &[(Syllable, f64)], customary: &[Syllable]) -> Vec<i8> {
        let commonest = counts
            .iter()
            .filter_map(|count| count_of(count.0, counts))
            .fold(0.0, f64::max);
        // The most the character is known for: alone, or in one reading's words.
        let known_for = self.in_words.values().copied().fold(self.alone, f64::max);
        let shares: Vec<f64> = self
            .readings
            .iter()
            .map(|&reading| match count_of(reading, counts) {
                Some(count) => count / commonest,
                None if customary.contains(&reading) => 1.0,
                None => {
                    let in_words = self.in_words.get(&reading).copied().unwrap_or(0.0);
                    (in_words / known_for).max(UNLISTED_FLOOR)
                }
            })
            .collect();
        // Against the likeliest of the readings the dictionary has, which
        // may be none of Unihan's: the character still has to rank as it
        // did somewhere.
        let likeliest = shares.iter().copied().fold(0.0, f64::max);
        shares
            .into_iter()
            .map(|share| share / likeliest)
            .map(|share| {
                if share >= AS_COMMON {
                    0
                } else {
                    (share.log10() * PER_TENFOLD).round().max(-100.0) as i8
                }
            })
            .collect()
    }
}

/// How often the character was counted read as `reading`. A neutral tone is
/// the same reading as the tone it wears down: 個 is counted ge as well as
/// gè, and one typing ㄍㄜˋ or ㄍㄜ˙ means either. With several tones to
/// wear down it is the commonest, and with none counted, every one the
/// dictionary has (們 is counted men, and typed ㄇㄣˊ too).
fn count_of(reading: Syllable, counts: &[(Syllable, f64)]) -> Option<f64> {
    let count = |of: Syllable| {
        counts
            .iter()
            .find(|count| count.0 == of)
            .map(|count| count.1)
    };
    let same_sound = |a: Syllable, b: Syllable| {
        (a.initial(), a.medial(), a.rime()) == (b.initial(), b.medial(), b.rime())
    };
    let neutral = |syllable: Syllable| syllable.tone() == Some(Bopomofo::TONE5);
    let worn_down = counts
        .iter()
        .filter(|count| !neutral(count.0) && same_sound(count.0, reading))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|count| count.0);
    let neutral_count = counts
        .iter()
        .find(|count| neutral(count.0) && same_sound(count.0, reading))
        .map(|count| count.1);
    if neutral(reading) {
        Some(count(reading)? + worn_down.and_then(count).unwrap_or(0.0))
    } else if worn_down.is_none_or(|worn_down| worn_down == reading) {
        match (count(reading), neutral_count) {
            (None, None) => None,
            (own, neutral) => Some(own.unwrap_or(0.0) + neutral.unwrap_or(0.0)),
        }
    } else {
        count(reading)
    }
}

/// `zháo(545)` as ㄓㄠˊ counted 545 times.
fn counted_reading(text: &str) -> Option<(Syllable, f64)> {
    let (pinyin, count) = text.strip_suffix(')')?.split_once('(')?;
    Some((syllable(pinyin)?, count.parse().ok()?))
}

fn syllable(pinyin: &str) -> Option<Syllable> {
    zhuyin(pinyin)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use chewing::zhuyin::Syllable;

    use super::polyphone_rows;

    fn syllables(text: &str) -> Vec<Syllable> {
        text.split(' ')
            .map(|syllable| syllable.parse().unwrap())
            .collect()
    }

    /// The rows for `unihan` lines and `(word, spelling, probability)`.
    fn rows(unihan: &[&str], entries: &[(&str, &str, f64)]) -> Vec<(String, i8)> {
        let unihan: String = unihan.iter().map(|line| format!("{line}\n")).collect();
        let entries: Vec<_> = entries
            .iter()
            .map(|&(word, spelling, probability)| {
                (syllables(spelling), word.to_owned(), probability)
            })
            .collect();
        polyphone_rows(&unihan, &entries)
            .into_iter()
            .map(|(ch, reading, penalty)| (format!("{ch}{reading}"), penalty))
            .collect()
    }

    fn sorted(mut rows: Vec<(String, i8)>) -> Vec<(String, i8)> {
        rows.sort();
        rows
    }

    #[test]
    fn a_reading_counted_less_ranks_lower_by_as_much() {
        // 給 is counted jǐ a thirty-sixth as often as gěi: 1.55 tenfolds.
        assert_eq!(
            sorted(rows(
                &[
                    "U+7D66\tkHanyuPinlu\tgěi(2860) jǐ(80)",
                    "U+7D66\tkMandarin\tgěi"
                ],
                &[("給", "ㄍㄟˇ", 1e-3), ("給", "ㄐㄧˇ", 1e-3)],
            )),
            [("給ㄍㄟˇ".to_owned(), 0), ("給ㄐㄧˇ".to_owned(), -31)]
        );
    }

    #[test]
    fn a_reading_never_counted_goes_by_the_words_spelled_with_it() {
        let unihan = [
            "U+7D05\tkHanyuPinlu\thóng(839)",
            "U+671F\tkHanyuPinlu\tqī(1012)",
        ];
        let got = sorted(rows(
            &unihan,
            &[
                // In one rare word: as low as a reading never counted goes.
                ("紅", "ㄏㄨㄥˊ", 3e-4),
                ("紅", "ㄍㄨㄥ", 3e-4),
                ("紅色", "ㄏㄨㄥˊ ㄙㄜˋ", 1e-4),
                ("女紅", "ㄋㄩˇ ㄍㄨㄥ", 1e-7),
                // Taiwan's reading, which spells the character's words.
                ("期", "ㄑㄧ", 1e-4),
                ("期", "ㄑㄧˊ", 1e-4),
                ("星期", "ㄒㄧㄥ ㄑㄧˊ", 1e-3),
            ],
        ));
        assert_eq!(
            got,
            [("紅ㄍㄨㄥ".to_owned(), -40), ("紅ㄏㄨㄥˊ".to_owned(), 0)]
        );
    }

    #[test]
    fn the_reading_unihan_gives_taiwan_is_never_lower() {
        let entries = [
            ("誰", "ㄕㄨㄟˊ", 1e-3),
            ("誰", "ㄕㄟˊ", 1e-3),
            ("誰", "ㄕㄨㄟ", 1e-3),
        ];
        assert_eq!(
            sorted(rows(
                &[
                    "U+8AB0\tkHanyuPinlu\tshuí(1065)",
                    "U+8AB0\tkMandarin\tshuí shéi"
                ],
                &entries,
            )),
            [
                ("誰ㄕㄟˊ".to_owned(), 0),
                ("誰ㄕㄨㄟ".to_owned(), -40),
                ("誰ㄕㄨㄟˊ".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn a_neutral_tone_counts_for_the_tone_it_wears_down() {
        // ge is gè said lightly, not gě; men has only ㄇㄣˊ to be.
        assert_eq!(
            sorted(rows(
                &[
                    "U+500B\tkHanyuPinlu\tgè(11693) ge(1891) gě(18)",
                    "U+5011\tkHanyuPinlu\tmen(14950)",
                ],
                &[
                    ("個", "ㄍㄜˋ", 1e-2),
                    ("個", "ㄍㄜ˙", 1e-2),
                    ("個", "ㄍㄜˇ", 1e-2),
                    ("們", "ㄇㄣ˙", 1e-2),
                    ("們", "ㄇㄣˊ", 1e-2),
                ],
            )),
            [
                ("個ㄍㄜˇ".to_owned(), -58),
                ("個ㄍㄜˋ".to_owned(), 0),
                ("個ㄍㄜ˙".to_owned(), 0),
            ]
        );
    }

    #[test]
    fn two_common_readings_keep_the_rank_they_share() {
        // The mainland counts 長 zhǎng more: nothing to rank ㄔㄤˊ lower by.
        assert_eq!(
            rows(
                &[
                    "U+9577\tkHanyuPinlu\tzhǎng(1879) cháng(1179)",
                    "U+9577\tkMandarin\tzhǎng"
                ],
                &[("長", "ㄓㄤˇ", 1e-3), ("長", "ㄔㄤˊ", 1e-3)],
            ),
            []
        );
    }

    #[test]
    fn a_character_never_counted_goes_by_the_reading_it_is_known_by() {
        assert_eq!(
            sorted(rows(
                &["U+9AAF\tkMandarin\tāng"],
                &[
                    ("骯", "ㄤ", 4e-7),
                    ("骯", "ㄎㄤˇ", 4e-7),
                    ("骯髒", "ㄤ ㄗㄤ", 1e-5)
                ],
            )),
            [("骯ㄎㄤˇ".to_owned(), -40), ("骯ㄤ".to_owned(), 0)]
        );
    }

    #[test]
    fn the_likeliest_reading_the_dictionary_has_keeps_its_rank() {
        // Without the reading Unihan knows 期 by, ㄑㄧˊ is where it ranks as
        // it did, and ㄐㄧ a tenth as likely as that.
        assert_eq!(
            sorted(rows(
                &["U+671F\tkHanyuPinlu\tqī(1012)", "U+671F\tkMandarin\tqī"],
                &[
                    ("期", "ㄑㄧˊ", 1e-3),
                    ("期", "ㄐㄧ", 1e-3),
                    ("星期", "ㄒㄧㄥ ㄑㄧˊ", 1e-4)
                ],
            )),
            [("期ㄐㄧ".to_owned(), -20), ("期ㄑㄧˊ".to_owned(), 0)]
        );
    }

    #[test]
    fn characters_unihan_lacks_or_with_one_reading_are_left_alone() {
        assert_eq!(
            rows(
                &["U+5DE5\tkHanyuPinlu\tgōng(4000)"],
                &[
                    ("工", "ㄍㄨㄥ", 1e-4),
                    ("乾", "ㄍㄢ", 1e-4),
                    ("乾", "ㄑㄧㄢˊ", 1e-4)
                ],
            ),
            []
        );
    }
}
