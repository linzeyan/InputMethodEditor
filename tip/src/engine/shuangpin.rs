// SPDX-License-Identifier: GPL-3.0-or-later

//! Shuangpin: every syllable in two keys, the key its initial is on, then
//! the key its final is on.

use std::fmt::{self, Display};

use chewing::editor::zhuyin_layout::{KeyBehavior, SyllableEditor};
use chewing::input::KeyboardEvent;
use chewing::input::keysym::{SYM_BACKSPACE, SYM_CAPSLOCK, SYM_ESC};
use chewing::zhuyin::Syllable;

use super::pinyin::{is_syllable, to_syllable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scheme {
    Xiaohe,
    Ziranma,
    Microsoft,
    Sogou,
}

impl Scheme {
    /// The scheme the config names, where 0 is full pinyin.
    pub(super) fn from_config(value: i32) -> Option<Scheme> {
        Some(match value {
            1 => Scheme::Xiaohe,
            2 => Scheme::Ziranma,
            3 => Scheme::Microsoft,
            4 => Scheme::Sogou,
            _ => return None,
        })
    }

    /// The finals on `key`. Where an initial goes with two of them, the
    /// likelier comes first: lo is luo. ü is spelled u after j, q, x and y,
    /// so only lü and nü need v.
    fn finals(self, key: char) -> &'static [&'static str] {
        use Scheme::*;
        match (key, self) {
            ('a', _) => &["a"],
            ('e', _) => &["e"],
            ('i', _) => &["i"],
            ('u', _) => &["u"],
            ('o', _) => &["uo", "o"],
            ('q', _) => &["iu"],
            ('r', Microsoft | Sogou) => &["uan", "er"],
            ('r', _) => &["uan"],
            ('t', _) => &["ue"],
            ('s', _) => &["ong", "iong"],
            ('f', _) => &["en"],
            ('g', _) => &["eng"],
            ('h', _) => &["ang"],
            ('j', _) => &["an"],
            ('m', _) => &["ian"],
            ('w', Xiaohe) => &["ei"],
            ('y', Xiaohe) => &["un"],
            ('p', Xiaohe) => &["ie"],
            ('d', Xiaohe) => &["ai"],
            ('k', Xiaohe) => &["ing", "uai"],
            ('l', Xiaohe) => &["iang", "uang"],
            ('z', Xiaohe) => &["ou"],
            ('x', Xiaohe) => &["ia", "ua"],
            ('c', Xiaohe) => &["ao"],
            ('b', Xiaohe) => &["in"],
            ('n', Xiaohe) => &["iao"],
            ('w', _) => &["ia", "ua"],
            ('p', _) => &["un"],
            ('d', _) => &["iang", "uang"],
            ('k', _) => &["ao"],
            ('l', _) => &["ai"],
            ('z', _) => &["ei"],
            ('x', _) => &["ie"],
            ('c', _) => &["iao"],
            ('b', _) => &["ou"],
            ('n', _) => &["in"],
            ('y', Ziranma) => &["ing", "uai"],
            ('y', _) => &["uai", "v"],
            ('v', Xiaohe | Ziranma) => &["ui", "v"],
            // Microsoft has üe on v as well as on t.
            ('v', Microsoft) => &["ui", "ue"],
            ('v', Sogou) => &["ui"],
            (';', Microsoft | Sogou) => &["ing"],
            _ => &[],
        }
    }

    /// The syllable `key` ends, after the key typing `initial`.
    fn syllable(self, initial: &str, key: char) -> Option<String> {
        let finals = self.finals(key).iter();
        let spellings: Vec<String> = if matches!(initial, "a" | "e" | "o") {
            // No initial. Xiaohe and Ziranma type ai as it is spelled, and
            // the vowel then the final's key for the rest (aa, ah for ang);
            // Microsoft and Sogou type o then the final's key (oa, oh).
            let any = initial == "o" && matches!(self, Scheme::Microsoft | Scheme::Sogou);
            [format!("{initial}{key}")]
                .into_iter()
                .chain(
                    finals
                        .filter(|it| any || it.starts_with(initial))
                        .map(|it| it.to_string()),
                )
                .collect()
        } else {
            finals.map(|it| format!("{initial}{it}")).collect()
        };
        spellings.into_iter().find(|it| is_syllable(it))
    }
}

#[derive(Debug, Clone)]
pub(super) struct Shuangpin {
    scheme: Scheme,
    /// The pinyin typed so far: the initial (sh for u), then the syllable.
    spelled: String,
}

impl Shuangpin {
    pub(super) fn new(scheme: Scheme) -> Shuangpin {
        Shuangpin {
            scheme,
            spelled: String::new(),
        }
    }
}

impl Display for Shuangpin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Shuangpin")
    }
}

impl SyllableEditor for Shuangpin {
    fn key_press(&mut self, key: KeyboardEvent) -> KeyBehavior {
        if key.has_modifiers() {
            return KeyBehavior::KeyError;
        }
        let key = key.ksym.to_unicode();
        if self.spelled.is_empty() {
            self.spelled = match key {
                'v' => "zh".to_owned(),
                'i' => "ch".to_owned(),
                'u' => "sh".to_owned(),
                // a, e and o start a syllable with no initial.
                'a'..='z' => key.to_string(),
                _ => return KeyBehavior::KeyError,
            };
            return KeyBehavior::Absorb;
        }
        // The initial alone stands for any syllable it starts, as in full
        // pinyin: u then Space is sh.
        if key == ' ' {
            return KeyBehavior::Commit;
        }
        match self.scheme.syllable(&self.spelled, key) {
            Some(syllable) => {
                self.spelled = syllable;
                KeyBehavior::Commit
            }
            None => KeyBehavior::KeyError,
        }
    }

    fn fuzzy_key_press(&mut self, key: KeyboardEvent) -> KeyBehavior {
        self.key_press(key)
    }

    fn remove_last(&mut self) {
        self.spelled.clear();
    }

    fn clear(&mut self) {
        self.spelled.clear();
    }

    fn is_empty(&self) -> bool {
        self.spelled.is_empty()
    }

    fn read(&self) -> Syllable {
        to_syllable(&self.spelled)
    }

    fn key_seq(&self) -> Option<String> {
        Some(self.spelled.clone())
    }

    fn clone(&self) -> Box<dyn SyllableEditor> {
        Box::new(Clone::clone(self))
    }
}

/// Whether the key goes into the syllable being typed, as `pinyin::takes`.
pub(super) fn takes(scheme: Scheme, key: &KeyboardEvent) -> bool {
    matches!(key.ksym, SYM_BACKSPACE | SYM_ESC | SYM_CAPSLOCK)
        || !key.has_modifiers() && {
            let key = key.ksym.to_unicode();
            key == ' ' || !scheme.finals(key).is_empty()
        }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chewing::dictionary::StringTableBuilder;
    use chewing::editor::zhuyin_layout::{KeyBehavior, SyllableEditor};
    use chewing::editor::{BasicEditor, ConversionEngineKind, EditorBuilder};
    use chewing::input::KeyboardEvent;
    use chewing::input::keymap::{QWERTY_MAP, map_ascii};
    use chewing::input::keysym::Keysym;
    use chewing::lm::StaticDictBuilder;
    use chewing::syl;
    use chewing::zhuyin::Bopomofo as bpmf;

    use super::super::pinyin::{SYLLABLES, to_syllable};
    use super::Scheme::{self, *};
    use super::Shuangpin;

    /// The syllable `keys` type, if they end one.
    fn typed(scheme: Scheme, keys: &str) -> Option<String> {
        let mut editor = Shuangpin::new(scheme);
        for key in keys.chars() {
            let key = KeyboardEvent::builder().ksym(Keysym::from(key)).build();
            match editor.key_press(key) {
                KeyBehavior::Absorb => {}
                KeyBehavior::Commit => return Some(editor.read().to_string()),
                _ => return None,
            }
        }
        None
    }

    #[test]
    fn published_codes_type_their_syllables() {
        for (scheme, keys, pinyin) in [
            // 雙拼
            (Xiaohe, "ul", "shuang"),
            (Xiaohe, "pb", "pin"),
            (Ziranma, "ud", "shuang"),
            (Microsoft, "pn", "pin"),
            (Sogou, "ud", "shuang"),
            // 英、快、小
            (Xiaohe, "yk", "ying"),
            (Ziranma, "yy", "ying"),
            (Microsoft, "y;", "ying"),
            (Sogou, "y;", "ying"),
            (Xiaohe, "kk", "kuai"),
            (Ziranma, "ky", "kuai"),
            (Xiaohe, "xn", "xiao"),
            (Sogou, "xc", "xiao"),
            // 綠、略: Microsoft has üe on v too.
            (Xiaohe, "lv", "lv"),
            (Microsoft, "ly", "lv"),
            (Xiaohe, "lt", "lue"),
            (Microsoft, "lv", "lue"),
            (Sogou, "lt", "lue"),
            // 落 over 咯.
            (Ziranma, "lo", "luo"),
            // No initial: 啊、愛、昂、歐、兒.
            (Xiaohe, "aa", "a"),
            (Xiaohe, "ai", "ai"),
            (Xiaohe, "ah", "ang"),
            (Ziranma, "ou", "ou"),
            (Ziranma, "er", "er"),
            (Microsoft, "oa", "a"),
            (Microsoft, "ol", "ai"),
            (Sogou, "oh", "ang"),
            (Sogou, "ob", "ou"),
            (Sogou, "or", "er"),
        ] {
            assert_eq!(
                typed(scheme, keys),
                Some(to_syllable(pinyin).to_string()),
                "{scheme:?} {keys}"
            );
        }
    }

    #[test]
    fn every_syllable_takes_two_keys() {
        let keys: Vec<char> = ('a'..='z').chain([';']).collect();
        for scheme in [Xiaohe, Ziranma, Microsoft, Sogou] {
            let made: HashSet<String> = keys
                .iter()
                .flat_map(|first| keys.iter().map(move |second| format!("{first}{second}")))
                .filter_map(|it| typed(scheme, &it))
                .collect();
            // lo is the rare one of lo and luo, which share their keys.
            for pinyin in SYLLABLES.split_whitespace().filter(|it| *it != "lo") {
                assert!(
                    made.contains(&to_syllable(pinyin).to_string()),
                    "{scheme:?} {pinyin}"
                );
            }
        }
    }

    #[test]
    fn keys_ending_no_syllable_keep_the_initial() {
        let mut editor = Shuangpin::new(Xiaohe);
        let key = |key: char| KeyboardEvent::builder().ksym(Keysym::from(key)).build();
        assert_eq!(editor.key_press(key('f')), KeyBehavior::Absorb);
        // f has neither ing nor uai.
        assert_eq!(editor.key_press(key('k')), KeyBehavior::KeyError);
        assert_eq!(editor.key_seq().unwrap(), "f");
        assert_eq!(editor.key_press(key('j')), KeyBehavior::Commit);
        assert_eq!(editor.read(), to_syllable("fan"));
    }

    #[test]
    fn an_initial_alone_abbreviates() {
        assert_eq!(typed(Xiaohe, "u "), Some(to_syllable("sh").to_string()));
        assert!(to_syllable("sh").is_abbreviation());
    }

    #[test]
    fn syllables_convert_as_their_second_key_is_typed() {
        let mut strings = StringTableBuilder::new();
        strings.insert("雙");
        let strings = strings.build();
        let mut dict = StaticDictBuilder::new();
        let shuang = syl![bpmf::SH, bpmf::U, bpmf::ANG, bpmf::TONE1];
        dict.insert(&[shuang], strings.get_wid("雙").unwrap());
        let mut editor = EditorBuilder::new()
            .string_table(strings)
            .static_dict(dict.build())
            .build();
        editor.set_editor_options(|opt| {
            opt.conversion_engine = ConversionEngineKind::FuzzyChewingEngine
        });
        editor.set_syllable_editor(Box::new(Shuangpin::new(Xiaohe)));
        editor.process_keyevent(map_ascii(&QWERTY_MAP, b'u'));
        // Shown as the initial it is, not the key.
        assert_eq!(editor.syllable_buffer_display(), "sh");
        editor.process_keyevent(map_ascii(&QWERTY_MAP, b'l'));
        assert_eq!(editor.display(), "雙");
        assert_eq!(editor.syllable_buffer_display(), "");
    }
}
