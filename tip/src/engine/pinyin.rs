// SPDX-License-Identifier: GPL-3.0-or-later

//! Hanyu pinyin typed without breaks: "nihao" is split into ni and hao while
//! it is typed, so each syllable converts as soon as the next one starts.

use std::cell::RefCell;
use std::fmt::{self, Display};

use chewing::editor::zhuyin_layout::{KeyBehavior, Pinyin, SyllableEditor};
use chewing::input::KeyboardEvent;
use chewing::input::keycode::KEY_SPACE;
use chewing::input::keysym::{Keysym, SYM_BACKSPACE, SYM_CAPSLOCK, SYM_ESC, SYM_SPACE};
use chewing::syl;
use chewing::zhuyin::{
    Bopomofo, FUZZY_AN_ANG, FUZZY_C_CH, FUZZY_EN_ENG, FUZZY_F_H, FUZZY_IN_ING, FUZZY_N_L,
    FUZZY_R_L, FUZZY_S_SH, FUZZY_Z_ZH, Syllable, fuzzy_sounds,
};

/// Every Hanyu pinyin syllable, ü spelled v (lue and nue as well).
pub(super) const SYLLABLES: &str = "
    a ai an ang ao
    ba bai ban bang bao bei ben beng bi bian biao bie bin bing bo bu
    ca cai can cang cao ce cen ceng cha chai chan chang chao che chen cheng chi chong chou
    chu chua chuai chuan chuang chui chun chuo ci cong cou cu cuan cui cun cuo
    da dai dan dang dao de dei den deng di dia dian diao die ding diu dong dou du duan dui
    dun duo
    e ei en eng er
    fa fan fang fei fen feng fo fou fu
    ga gai gan gang gao ge gei gen geng gong gou gu gua guai guan guang gui gun guo
    ha hai han hang hao he hei hen heng hong hou hu hua huai huan huang hui hun huo
    ji jia jian jiang jiao jie jin jing jiong jiu ju juan jue jun
    ka kai kan kang kao ke kei ken keng kong kou ku kua kuai kuan kuang kui kun kuo
    la lai lan lang lao le lei leng li lia lian liang liao lie lin ling liu lo long lou lu
    luan lue lun luo lv lve
    ma mai man mang mao me mei men meng mi mian miao mie min ming miu mo mou mu
    na nai nan nang nao ne nei nen neng ni nian niang niao nie nin ning niu nong nou nu nuan
    nue nuo nv nve
    o ou
    pa pai pan pang pao pei pen peng pi pian piao pie pin ping po pou pu
    qi qia qian qiang qiao qie qin qing qiong qiu qu quan que qun
    ran rang rao re ren reng ri rong rou ru rua ruan rui run ruo
    sa sai san sang sao se sen seng sha shai shan shang shao she shei shen sheng shi shou shu
    shua shuai shuan shuang shui shun shuo si song sou su suan sui sun suo
    ta tai tan tang tao te tei teng ti tian tiao tie ting tong tou tu tuan tui tun tuo
    wa wai wan wang wei wen weng wo wu
    xi xia xian xiang xiao xie xin xing xiong xiu xu xuan xue xun
    ya yan yang yao ye yi yin ying yo yong you yu yuan yue yun
    za zai zan zang zao ze zei zen zeng zha zhai zhan zhang zhao zhe zhei zhen zheng zhi zhong
    zhou zhu zhua zhuai zhuan zhuang zhui zhun zhuo zi zong zou zu zuan zui zun zuo
";

/// An initial typed alone stands for any syllable it starts: zhg is 這個.
const INITIALS: [&str; 23] = [
    "b", "p", "m", "f", "d", "t", "n", "l", "g", "k", "h", "j", "q", "x", "zh", "ch", "sh", "r",
    "z", "c", "s", "y", "w",
];

#[derive(Debug, Default, Clone)]
pub(super) struct ContinuousPinyin {
    /// Letters of the syllable being typed, not yet handed to the editor.
    pending: String,
    tone: Option<Bopomofo>,
}

impl Display for ContinuousPinyin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContinuousPinyin")
    }
}

impl SyllableEditor for ContinuousPinyin {
    fn key_press(&mut self, key: KeyboardEvent) -> KeyBehavior {
        if key.has_modifiers() {
            return KeyBehavior::KeyError;
        }
        let letter = match key.ksym.to_unicode() {
            letter @ 'a'..='z' => letter,
            _ if self.pending.is_empty() => return KeyBehavior::KeyError,
            ' ' | '\'' | '1' => return KeyBehavior::Commit,
            digit @ '2'..='5' => {
                self.tone = Some(match digit {
                    '2' => Bopomofo::TONE2,
                    '3' => Bopomofo::TONE3,
                    '4' => Bopomofo::TONE4,
                    _ => Bopomofo::TONE5,
                });
                return KeyBehavior::Commit;
            }
            _ => return KeyBehavior::KeyError,
        };
        let mut typed = self.pending.clone();
        typed.push(letter);
        if is_prefix(&typed) {
            self.pending = typed;
            return KeyBehavior::Absorb;
        }
        // The letter can't go on this syllable, so it ends somewhere before it.
        // Pinyin spelling puts ' before a syllable starting with a, o or e
        // (xi'an), so without one fenge is fen ge, not feng e; only when no
        // other split works, as in tiananmen, may the next syllable start so.
        // Among the rest the longest first syllable wins, and backing up is
        // how fangu becomes fan gu.
        for vowel_start in [false, true] {
            for at in (1..=self.pending.len()).rev() {
                let (head, tail) = typed.split_at(at);
                if tail.starts_with(['a', 'o', 'e']) == vowel_start
                    && is_syllable_or_initial(head)
                    && is_prefix(tail)
                {
                    let syllable = to_syllable(head);
                    self.pending = tail.to_owned();
                    // Fuzzy inserts the syllable but keeps this editor's state,
                    // unlike Commit, so the letter just typed isn't lost.
                    return KeyBehavior::Fuzzy(syllable);
                }
            }
        }
        KeyBehavior::KeyError
    }

    fn fuzzy_key_press(&mut self, key: KeyboardEvent) -> KeyBehavior {
        self.key_press(key)
    }

    fn remove_last(&mut self) {
        self.pending.pop();
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.tone = None;
    }

    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn read(&self) -> Syllable {
        let mut syllable = to_syllable(&self.pending);
        if let Some(tone) = self.tone {
            syllable.update(tone);
        }
        syllable
    }

    fn key_seq(&self) -> Option<String> {
        Some(self.pending.clone())
    }

    fn clone(&self) -> Box<dyn SyllableEditor> {
        Box::new(Clone::clone(self))
    }
}

/// Whether the key goes into the syllable being typed. Chewing hands that
/// syllable every key, so any other key has to end it first: "nihao," is
/// 你好，and Enter sends the last syllable too.
pub(super) fn takes(key: &KeyboardEvent) -> bool {
    // Chewing handles these itself: the syllable loses a letter or goes away.
    matches!(key.ksym, SYM_BACKSPACE | SYM_ESC | SYM_CAPSLOCK)
        || !key.has_modifiers()
            && matches!(key.ksym.to_unicode(), 'a'..='z' | ' ' | '\'' | '1'..='5')
}

/// The key that ends the syllable being typed. Pinyin reads the symbol, most
/// zhuyin layouts the key's code, so it carries both.
pub(super) fn end_syllable() -> KeyboardEvent {
    KeyboardEvent::builder()
        .code(KEY_SPACE)
        .ksym(SYM_SPACE)
        .build()
}

/// The fuzzy sound pairs as spelled.
const FUZZY_SPELLINGS: [(u32, &str, &str); 9] = [
    (FUZZY_Z_ZH, "z", "zh"),
    (FUZZY_C_CH, "c", "ch"),
    (FUZZY_S_SH, "s", "sh"),
    (FUZZY_N_L, "n", "l"),
    (FUZZY_F_H, "f", "h"),
    (FUZZY_R_L, "r", "l"),
    (FUZZY_AN_ANG, "an", "ang"),
    (FUZZY_EN_ENG, "en", "eng"),
    (FUZZY_IN_ING, "in", "ing"),
];

thread_local! {
    /// What `spellings` last made, and for which fuzzy sounds.
    static SPELLINGS: RefCell<Option<(u32, Vec<String>)>> = const { RefCell::new(None) };
}

/// Every syllable, and what the fuzzy sounds set make one: with r/l, len
/// is ren (人) as someone who says so types it, though no syllable itself.
/// libchewing spells it ㄌㄣ, which the fuzzy lookup takes for ㄖㄣ.
fn with_spellings<T>(f: impl FnOnce(&[String]) -> T) -> T {
    SPELLINGS.with_borrow_mut(|made| {
        let sounds = fuzzy_sounds();
        if made
            .as_ref()
            .is_none_or(|(made_for, _)| *made_for != sounds)
        {
            *made = Some((sounds, spellings(sounds)));
        }
        f(&made.as_ref().unwrap().1)
    })
}

fn spellings(sounds: u32) -> Vec<String> {
    let mut spellings: Vec<String> = SYLLABLES.split_whitespace().map(str::to_owned).collect();
    for syllable in SYLLABLES.split_whitespace() {
        let initial = INITIALS
            .iter()
            .filter(|it| syllable.starts_with(**it))
            .max_by_key(|it| it.len())
            .map_or("", |it| *it);
        let rest = &syllable[initial.len()..];
        for (sound, a, b) in FUZZY_SPELLINGS {
            if sounds & sound == 0 {
                continue;
            }
            // Initials are swapped whole (z for zh), finals at the end (in
            // for ing, and ian for iang).
            let other = if sound < FUZZY_AN_ANG {
                [(a, b), (b, a)]
                    .into_iter()
                    .find(|(from, _)| initial == *from)
                    .map(|(_, to)| format!("{to}{rest}"))
            } else if let Some(stem) = rest.strip_suffix(b) {
                Some(format!("{initial}{stem}{a}"))
            } else {
                rest.strip_suffix(a)
                    .map(|stem| format!("{initial}{stem}{b}"))
            };
            // Only what libchewing spells: yuang isn't anything.
            if let Some(other) = other
                && !spellings.contains(&other)
                && !parse(&other).is_empty()
            {
                spellings.push(other);
            }
        }
    }
    spellings
}

fn is_prefix(pinyin: &str) -> bool {
    with_spellings(|spellings| spellings.iter().any(|it| it.starts_with(pinyin)))
}

pub(super) fn is_syllable(pinyin: &str) -> bool {
    with_spellings(|spellings| spellings.iter().any(|it| it == pinyin))
}

fn is_syllable_or_initial(pinyin: &str) -> bool {
    INITIALS.contains(&pinyin) || is_syllable(pinyin)
}

/// The syllable `pinyin` stands for: a whole syllable is itself (whatever
/// its tone), while an initial typed alone, or a syllable not typed to its
/// end, stands for any syllable with that initial.
pub(super) fn to_syllable(pinyin: &str) -> Syllable {
    if is_syllable(pinyin) {
        return parse(pinyin);
    }
    // zh, not z, in zhon.
    let initial = INITIALS
        .iter()
        .filter(|it| pinyin.starts_with(**it))
        .max_by_key(|it| it.len());
    match initial {
        // libchewing takes no y or w alone; their syllables start with ㄧ or ㄨ.
        Some(&"y") => syl![Bopomofo::I].abbreviate(),
        Some(&"w") => syl![Bopomofo::U].abbreviate(),
        Some(initial) => parse(initial).abbreviate(),
        None => parse(pinyin),
    }
}

fn parse(pinyin: &str) -> Syllable {
    // libchewing knows the spelling rules (zhi has no rime, ju is ㄐㄩ).
    let mut editor = Pinyin::hanyu();
    for letter in pinyin.chars() {
        editor.key_press(KeyboardEvent::builder().ksym(Keysym::from(letter)).build());
    }
    editor.key_press(end_syllable());
    editor.read()
}

#[cfg(test)]
mod tests {
    use chewing::dictionary::StringTableBuilder;
    use chewing::editor::zhuyin_layout::{KeyBehavior, SyllableEditor};
    use chewing::editor::{BasicEditor, ConversionEngineKind, EditorBuilder};
    use chewing::input::KeyboardEvent;
    use chewing::input::keymap::{QWERTY_MAP, map_ascii};
    use chewing::input::keysym::Keysym;
    use chewing::lm::StaticDictBuilder;
    use chewing::syl;
    use chewing::zhuyin::Bopomofo as bpmf;
    use chewing::zhuyin::{FUZZY_IN_ING, FUZZY_R_L, FUZZY_S_SH, FUZZY_Z_ZH, set_fuzzy_sounds};

    use super::{ContinuousPinyin, SYLLABLES, to_syllable};

    /// The syllables typing `input` then Space hands the editor.
    fn syllables(input: &str) -> Vec<String> {
        let mut editor = ContinuousPinyin::default();
        let mut out = vec![];
        for letter in input.chars().chain([' ']) {
            let key = KeyboardEvent::builder().ksym(Keysym::from(letter)).build();
            match editor.key_press(key) {
                KeyBehavior::Fuzzy(syllable) => out.push(syllable.to_string()),
                KeyBehavior::Commit => {
                    out.push(editor.read().to_string());
                    editor.clear();
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn splits_syllables_as_typed() {
        assert_eq!(syllables("nihao"), ["ㄋㄧ", "ㄏㄠ"]);
        assert_eq!(syllables("zhongguo"), ["ㄓㄨㄥ", "ㄍㄨㄛ"]);
        // Written fen'ge would need no mark; feng'e would.
        assert_eq!(syllables("fenge"), ["ㄈㄣ", "ㄍㄜ"]);
        // Nothing else fits, so an can follow without the mark.
        assert_eq!(syllables("tiananmen"), ["ㄊㄧㄢ", "ㄢ", "ㄇㄣ"]);
        // No syllable starts with u; backing up gives fan guan.
        assert_eq!(syllables("fanguan"), ["ㄈㄢ", "ㄍㄨㄢ"]);
        assert_eq!(syllables("xian"), ["ㄒㄧㄢ"]);
        assert_eq!(syllables("xi'an"), ["ㄒㄧ", "ㄢ"]);
        assert_eq!(syllables("ma3"), ["ㄇㄚˇ"]);
    }

    #[test]
    fn initials_abbreviate_syllables() {
        assert_eq!(syllables("zhg"), ["ㄓ", "ㄍ"]);
        assert_eq!(syllables("bj"), ["ㄅ", "ㄐ"]);
        assert_eq!(syllables("yg"), ["ㄧ", "ㄍ"]);
    }

    #[test]
    fn initials_alone_are_abbreviations() {
        // zhi and zh are both ㄓ; only zh may be 這 (ㄓㄜˋ).
        assert!(!to_syllable("zhi").is_abbreviation());
        assert!(to_syllable("zh").is_abbreviation());
        assert!(to_syllable("y").is_abbreviation());
        // Space after an unfinished syllable.
        assert_eq!(to_syllable("zhon"), to_syllable("zh"));
    }

    #[test]
    fn every_syllable_has_bopomofo() {
        for pinyin in SYLLABLES.split_whitespace() {
            assert!(!to_syllable(pinyin).is_empty(), "{pinyin}");
        }
    }

    #[test]
    fn toneless_pinyin_converts_with_the_fuzzy_engine() {
        let mut strings = StringTableBuilder::new();
        strings.insert("你好");
        strings.insert("擬");
        strings.insert("好");
        let strings = strings.build();
        let mut dict = StaticDictBuilder::new();
        let ni = syl![bpmf::N, bpmf::I, bpmf::TONE3];
        let hao = syl![bpmf::H, bpmf::AU, bpmf::TONE3];
        dict.insert(&[ni, hao], strings.get_wid("你好").unwrap());
        dict.insert(&[ni], strings.get_wid("擬").unwrap());
        // Chewing keeps a syllable only if some word is spelled with it alone.
        dict.insert(&[hao], strings.get_wid("好").unwrap());
        let mut editor = EditorBuilder::new()
            .string_table(strings)
            .static_dict(dict.build())
            .build();
        editor.set_editor_options(|opt| {
            opt.conversion_engine = ConversionEngineKind::FuzzyChewingEngine
        });
        editor.set_syllable_editor(Box::new(ContinuousPinyin::default()));
        for ascii in *b"niha" {
            editor.process_keyevent(map_ascii(&QWERTY_MAP, ascii));
        }
        // ni is in the text already; ha is still being typed.
        assert_eq!(editor.display(), "擬");
        assert_eq!(editor.syllable_buffer_display(), "ha");
        for ascii in *b"o " {
            editor.process_keyevent(map_ascii(&QWERTY_MAP, ascii));
        }
        assert_eq!(editor.display(), "你好");
    }

    #[test]
    fn end_syllable_ends_toneless_zhuyin() {
        let mut strings = StringTableBuilder::new();
        strings.insert("你好");
        strings.insert("擬");
        strings.insert("好");
        let strings = strings.build();
        let mut dict = StaticDictBuilder::new();
        let ni = syl![bpmf::N, bpmf::I, bpmf::TONE3];
        let hao = syl![bpmf::H, bpmf::AU, bpmf::TONE3];
        dict.insert(&[ni, hao], strings.get_wid("你好").unwrap());
        dict.insert(&[ni], strings.get_wid("擬").unwrap());
        dict.insert(&[hao], strings.get_wid("好").unwrap());
        let mut editor = EditorBuilder::new()
            .string_table(strings)
            .static_dict(dict.build())
            .build();
        editor.set_editor_options(|opt| {
            opt.conversion_engine = ConversionEngineKind::FuzzyChewingEngine
        });
        // ㄋㄧㄏㄠ on the standard layout, which reads key codes.
        for ascii in *b"sucl" {
            editor.process_keyevent(map_ascii(&QWERTY_MAP, ascii));
        }
        assert!(editor.entering_syllable());
        // Otherwise Enter would be ignored and Shift+, typed as ㄝ.
        editor.process_keyevent(super::end_syllable());
        assert!(!editor.entering_syllable());
        assert_eq!(editor.display(), "你好");
    }

    #[test]
    fn fuzzy_sounds_add_the_spellings_they_make() {
        // len (人 said with l) is no syllable, so it can't end before shi.
        assert_ne!(syllables("lenshi"), ["ㄌㄣ", "ㄕ"]);
        set_fuzzy_sounds(FUZZY_R_L);
        assert_eq!(syllables("lenshi"), ["ㄌㄣ", "ㄕ"]);
        // 聽 and 雙 said without the ng and h.
        set_fuzzy_sounds(FUZZY_IN_ING | FUZZY_S_SH);
        assert_eq!(syllables("tinsuang"), ["ㄊㄧㄣ", "ㄙㄨㄤ"]);
        // The spellings go with the sounds.
        set_fuzzy_sounds(0);
        assert_ne!(syllables("lenshi"), ["ㄌㄣ", "ㄕ"]);
    }

    #[test]
    fn fuzzy_sounds_convert_the_other_of_a_pair() {
        let mut strings = StringTableBuilder::new();
        strings.insert("中");
        let strings = strings.build();
        let mut dict = StaticDictBuilder::new();
        let zhong = syl![bpmf::ZH, bpmf::U, bpmf::ENG, bpmf::TONE1];
        dict.insert(&[zhong], strings.get_wid("中").unwrap());
        let mut editor = EditorBuilder::new()
            .string_table(strings)
            .static_dict(dict.build())
            .build();
        editor.set_editor_options(|opt| {
            opt.conversion_engine = ConversionEngineKind::FuzzyChewingEngine
        });
        editor.set_syllable_editor(Box::new(ContinuousPinyin::default()));
        let type_zong = |editor: &mut chewing::editor::Editor| {
            for ascii in *b"zong " {
                editor.process_keyevent(map_ascii(&QWERTY_MAP, ascii));
            }
            editor.display()
        };
        assert_ne!(type_zong(&mut editor), "中");
        editor.clear();
        set_fuzzy_sounds(FUZZY_Z_ZH);
        assert_eq!(type_zong(&mut editor), "中");
    }
}
