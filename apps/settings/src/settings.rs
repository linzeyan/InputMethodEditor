// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The settings window. Most tabs are tables of settings, drawn as rows; the
//! texts (program lists, symbol tables, phrases) are bound to the window,
//! read when saving.

use std::cell::RefCell;
use std::collections::HashSet;
use std::process::Command;
use std::rc::Rc;
use std::sync::OnceLock;
use std::thread;

use chewing_tip_core::config::{ChewingTsfConfig, Config};
use chewing_tip_core::shell::{open_url, user_dir};
use serde_json::{Value, json};
use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString,
    StyledText, VecModel,
};

use crate::fonts::{FontFamilyName, system_fonts};
use crate::{
    Keybind, Program, Row, RowKind, Settings, SettingsWindow, VERSION, ask, bring_forward,
    center_when_made, config, error_box, follow_candidates, open_file, programs, save_file, update,
};

enum Kind {
    Header,
    Check,
    /// One bit of a number, as each sound of fuzzy_pinyin.
    Bit(i64),
    /// The setting's values with their names, in the order shown.
    Choice(&'static [(i64, &'static str)]),
    /// As Choice, for a setting kept as text.
    Named(&'static [(&'static str, &'static str)]),
    Slider,
    Spin(i32, i32),
    /// A color, as RRGGBBAA.
    Color,
    /// The fonts installed.
    Font,
    /// The candidate colors, all together.
    Theme,
    More,
}

/// A row of a tab, for the setting `key` of ChewingTsfConfig.
struct Spec {
    kind: Kind,
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    /// Shown only with 進階設定 open.
    advanced: bool,
    /// Off while that setting is so, as when it takes this one's place.
    disabled_when: Option<(&'static str, bool)>,
}

const fn header(label: &'static str) -> Spec {
    Spec {
        kind: Kind::Header,
        key: "",
        label,
        hint: "",
        advanced: false,
        disabled_when: None,
    }
}

const fn check(key: &'static str, label: &'static str, hint: &'static str) -> Spec {
    Spec {
        kind: Kind::Check,
        key,
        label,
        hint,
        advanced: false,
        disabled_when: None,
    }
}

const fn more() -> Spec {
    Spec {
        kind: Kind::More,
        ..header("進階設定")
    }
}

const fn color(key: &'static str, label: &'static str, hint: &'static str) -> Spec {
    Spec {
        kind: Kind::Color,
        ..check(key, label, hint)
    }
    .advanced()
}

impl Spec {
    const fn advanced(self) -> Spec {
        Spec {
            advanced: true,
            ..self
        }
    }

    const fn disabled_when(self, key: &'static str, value: bool) -> Spec {
        Spec {
            disabled_when: Some((key, value)),
            ..self
        }
    }
}

const TYPING_LEFT: &[Spec] = &[
    header("中英切換"),
    check(
        "switch_lang_with_shift",
        "使用 Shift 快速切換中英文",
        "單獨按一下 Shift（不和其他鍵一起按）就切換中文／英文；按住超過「進階設定」的長壓時間則不切換。使用 CapsLock 切換時停用。",
    )
    .disabled_when("enable_caps_lock", true),
    check(
        "enable_caps_lock",
        "使用 CapsLock 快速切換中英文",
        "改用 CapsLock 切換中文／英文：中英由 CapsLock 燈號決定，亮燈是哪一種在「進階設定」調整。勾選後「使用 Shift 快速切換中英文」與「預設以英文模式啟動」停用。",
    ),
    check(
        "default_english",
        "預設以英文模式啟動",
        "每個程式會記住上次切到的中文或英文，下次開啟時沿用；這項決定還沒切換過的程式從哪個模式開始。「各程式」頁列出的程式照那裡的設定。使用 CapsLock 切換中英文時由燈號決定，這項停用。",
    )
    .disabled_when("enable_caps_lock", true),
    check(
        "show_notification",
        "顯示中／英切換通知訊息",
        "切換中文／英文、正體／簡體、注音／拼音時，短暫顯示一個小視窗告訴你現在的模式。",
    ),
    more(),
    Spec {
        kind: Kind::Slider,
        key: "shift_key_sensitivity",
        label: "Shift 長壓敏感度：",
        hint: "單獨按住 Shift 超過這個時間才放開，視為長壓，不切換中英文。",
        advanced: true,
        disabled_when: None,
    },
    check(
        "lock_chinese_on_caps_lock",
        "CapsLock 亮燈鎖定中文",
        "勾選時 CapsLock 亮燈是中文、熄燈是英文；不勾選則相反。亮燈鎖定中文與各種軟體的相容性較好。需先勾選「使用 CapsLock 快速切換中英文」。",
    )
    .advanced()
    .disabled_when("enable_caps_lock", false),
    check(
        "sync_lang_mode_openclose",
        "設定英文模式等於關閉鍵盤",
        "切到英文模式時，也把 Windows 的輸入法開關設為關閉，和按 Ctrl＋Space 關閉輸入法相同。只有依這個開關判斷輸入法狀態的程式才需要，一般不用勾選。",
    )
    .advanced(),
    header("標點與符號（中文模式）"),
    check(
        "full_shape_symbols",
        "使用 Shift 輸入全形標點符號",
        "Shift＋標點鍵打出全形符號，例如 Shift＋1 是「！」；不勾選則是半形「!」。勾選「按住 Shift 輸入快捷符號」時一律是全形，這項停用。",
    )
    .disabled_when("easy_symbols_with_shift", true),
    check(
        "ctrl_full_shape_symbols",
        "使用 Ctrl 輸入全形標點符號",
        "不用按 Shift，Ctrl＋標點鍵打出全形標點：Ctrl＋, 是，、Ctrl＋. 是。、Ctrl＋/ 是／、Ctrl＋; 是；、Ctrl＋' 是、、Ctrl＋[ 與 ] 是「 與 」。勾選後中文模式下這些組合不再交給程式，例如 VS Code 的 Ctrl＋/（註解）與 Ctrl＋,（設定）、終端機的 Ctrl＋[（Esc），要用時切到英文模式。和 Ctrl＋數字（加詞）、Ctrl＋Shift＋字母（快捷符號）不衝突；「自訂快捷鍵」用到同一個組合時以快捷鍵為準。",
    ),
    check(
        "upper_case_with_shift",
        "按住 Shift 輸入大寫英文字母",
        "Shift＋字母打出英文字母：勾選是大寫（A），不勾選是小寫（a）。勾選「按住 Shift 輸入快捷符號」時 Shift＋字母打的是快捷符號，這項停用。",
    )
    .disabled_when("easy_symbols_with_shift", true),
    check(
        "easy_symbols_with_shift",
        "按住 Shift 輸入快捷符號",
        "Shift＋字母打出「快捷符號」頁設定的符號，例如 Shift＋A 是「【」。勾選後 Shift＋字母不再打英文字母，標點一律全形。",
    ),
    check(
        "easy_symbols_with_shift_ctrl",
        "按住 Shift + Ctrl 輸入快捷符號",
        "Ctrl＋Shift＋字母打出「快捷符號」頁設定的符號。只勾這一項時，Shift＋字母仍然打英文字母。",
    ),
    check(
        "enable_fullwidth_toggle",
        "使用 Shift + 空白鍵切換全形／半形",
        "按 Shift＋空白鍵在半形與全形之間切換，一開始是半形。全形時英文字母、數字、符號和空白都打成全形，例如 ＡＢＣ１２３，中文、英文模式都是。不勾選時不能切換，只有中文模式的標點是全形（，。？）。",
    ),
    check(
        "show_fullwidth_notification",
        "顯示全形／半形切換通知訊息",
        "按 Shift＋空白鍵切換時，短暫顯示「全形」或「半形」。需先勾選「使用 Shift + 空白鍵切換全形／半形」。",
    )
    .disabled_when("enable_fullwidth_toggle", false),
];

const TYPING_RIGHT: &[Spec] = &[
    header("選字"),
    Spec {
        kind: Kind::Choice(&[
            (0, "1234567890"),
            (1, "asdfghjkl;"),
            (2, "asdfzxcv89"),
            (3, "asdfjkl789"),
            (4, "aoeuhtn789"),
            (5, "1234qweras"),
        ]),
        ..check(
            "sel_key_type",
            "選字鍵：",
            "選字視窗裡第 1、2、3… 個候選用哪些鍵選取。選 asdf 開頭的組合，手不用離開主鍵區。",
        )
    },
    check(
        "cursor_cand_list",
        "使用方向鍵移動游標選字",
        "選字視窗出現時，用方向鍵移動反白、按 Enter 選取；Ctrl＋Delete 忘記詞也需要這一項，作用在反白的那一個。不勾選時用選字鍵選字。",
    ),
    check(
        "show_cand_with_space_key",
        "按空白鍵叫出選字視窗",
        "注音組字中，打完一個字後按空白鍵叫出選字視窗，和 ↓ 相同。不勾選時，空白鍵把游標前的字改成一聲，連按兩下才打出空白。",
    ),
    check(
        "phrase_choice_rearward",
        "游標在詞的後面選字（macOS 方式）",
        "組字中按 ↓ 選字時，改的是游標哪一邊的詞。不勾選（微軟新注音的方式）：游標後面的詞，游標在最後時是最後一個字，選字時游標移到詞的前面。勾選（macOS 注音的方式）：游標前面、到游標為止的詞，游標一直留在詞的後面。例如打完「我覺得」直接按 ↓，不勾選列出「得、德…」，勾選列出「覺得」；按一次 ← 再按 ↓，不勾選改「得」，勾選改「覺」。",
    ),
    check(
        "advance_after_selection",
        "選字完畢自動跳到下一個字",
        "按 ↓ 選好一個詞後，游標自動移到它後面，可以接著選下一段；不勾選時游標留在原處。勾選「游標在詞的後面選字」時游標本來就在詞後面，這項停用。",
    )
    .disabled_when("phrase_choice_rearward", true),
    check(
        "sort_candidates_by_frequency",
        "依照常用程度排序手動選字選單",
        "按 ↓ 叫出的候選依常用程度排序（字典的統計加上你的使用習慣），常用的在前面；不勾選時依字典的固定順序。",
    ),
    header("組字與學習"),
    Spec {
        kind: Kind::Choice(&[
            (1, "智慧選詞（每個字打聲調）"),
            (2, "智慧選詞（可省略聲調，連打）"),
            (0, "逐字選字"),
        ]),
        ..check(
            "conv_engine",
            "注音模式：",
            "注音怎麼轉成中文（拼音一律可省略聲調）。每個字打聲調：依整句自動挑字，一聲按空白。可省略聲調：ㄋㄧㄏㄠ 就是「你好」，只打聲母也行，ㄐㄊㄊㄑㄏㄏ 是「今天天氣很好」，最後一個字按空白、Enter 或標點結束；ˊˇˋ˙ 照樣有效，但空白不再限定一聲，例如 ㄉㄠ 加空白可能是「到」而不是「刀」。逐字選字：每打完一個字就跳出候選，不做整句轉換。",
        )
    },
    check(
        "pinyin",
        "以漢語拼音輸入（或使用 Ctrl + F11 切換）",
        "改用漢語拼音輸入：直接連打、不用聲調，例如 nihao 是「你好」。模糊音與雙拼在「拼音」頁設定。隨時可用 Ctrl＋F11 切換，所有程式共用。",
    ),
    check(
        "esc_clean_all_buf",
        "使用 Esc 清空編輯區字串",
        "組字中按 Esc 清掉所有還沒送出的字。不勾選時，Esc 只清掉正在打的注音或拼音，已轉好的字保留。",
    ),
    check(
        "composition_bar",
        "使用打字列",
        "打的字先出現在游標下方的打字列，選好送出後才進到程式裡，像自然輸入法。適合不顯示組字的程式（遊戲、部分終端機），或不想讓組字中的字干擾文件排版時。離開視窗時打字列裡的字會直接送出。",
    ),
    check(
        "add_phrase_forward",
        "Ctrl + 數字儲存游標前方的詞",
        "組字中按 Ctrl＋2～9，把游標前面的 2～9 個字加入使用者詞庫，之後就能整個詞一起打出來；不勾選則是游標後面的字。例如打完「中文輸入法」按 Ctrl＋3，加入「輸入法」。",
    ),
    check(
        "enable_auto_learn",
        "自動學習常用詞與新詞",
        "送出文字時記住你打過的詞和選過的字，之後整句轉換會優先用它們。關閉後不再學習，已學到的保留，可在「編輯使用者詞庫」管理。",
    ),
    header("簡體輸出"),
    check(
        "output_simp_chinese",
        "預設輸出簡體中文（或使用 Ctrl + F12 切換）",
        "把打出的字轉成簡體字形，例如 軟體→软体。隨時可用 Ctrl＋F12 或右鍵選單切換，所有程式共用。",
    ),
    check(
        "output_simp_vocabulary",
        "簡體改用大陸用語，如 軟體→软件",
        "簡體輸出時連用語一起換成大陸說法，例如 軟體→软件、網際網路→互联网；不勾選只轉字形。需先勾選簡體輸出。",
    )
    .disabled_when("output_simp_chinese", false),
];

const APPEARANCE: &[Spec] = &[
    Spec {
        kind: Kind::Spin(1, 10),
        ..check(
            "cand_per_row",
            "每列顯示候選字個數：",
            "選字視窗每一列排幾個候選，排不下的換到下一列。設為 1 就是直的一排。",
        )
    },
    Spec {
        kind: Kind::Spin(1, 10),
        ..check(
            "cand_per_page",
            "每頁顯示候選字個數：",
            "選字視窗一頁最多幾個候選，其餘的要翻頁；也決定用到幾個選字鍵。",
        )
    },
    Spec {
        kind: Kind::Spin(1, 100),
        ..check(
            "font_size",
            "選字及訊息視窗文字大小：",
            "選字視窗和切換模式時的訊息視窗的文字大小。",
        )
    },
    Spec {
        kind: Kind::Font,
        ..check(
            "font_family",
            "選字視窗字型：",
            "選字視窗和訊息視窗使用的字型。",
        )
    },
    Spec {
        kind: Kind::Theme,
        ..check(
            "",
            "色彩佈景主題：",
            "選字視窗和訊息視窗的配色。選「自訂」可在進階設定逐一調整每個顏色。",
        )
    },
    Spec {
        kind: Kind::Named(&[
            ("", "無：用色彩佈景主題"),
            ("wood", "木紋"),
            ("washi", "和紙"),
            ("marble", "大理石"),
            ("bamboo", "竹簾"),
            ("night", "夜幕"),
        ]),
        ..check(
            "skin",
            "打字列與選字視窗背景：",
            "給打字列和選字視窗換上有紋理的背景，文字配色跟著背景走；訊息視窗不受影響。",
        )
    },
    more(),
];

// Every color field takes the same format.
macro_rules! rgba {
    ($hint:literal) => {
        concat!(
            $hint,
            "格式是十六進位 RRGGBBAA，最後兩位是不透明度（FF 為完全不透明），例如 000000FF 是黑色。"
        )
    };
}

const COLORS_LEFT: &[Spec] = &[
    color(
        "font_fg_color",
        "文字顏色 RGB(A)",
        rgba!("候選字的文字顏色。"),
    ),
    color(
        "font_bg_color",
        "選字背景顏色 RGB(A)",
        rgba!("選字視窗的背景顏色。"),
    ),
    color(
        "cand_list_border_color",
        "選字邊框顏色 RGB(A)",
        rgba!("選字視窗的邊框顏色。"),
    ),
    color(
        "font_highlight_fg_color",
        "焦點文字顏色 RGB(A)",
        rgba!("用方向鍵反白的那個候選的文字顏色。"),
    ),
    color(
        "font_highlight_bg_color",
        "焦點背景顏色 RGB(A)",
        rgba!("用方向鍵反白的那個候選的背景顏色。"),
    ),
    color(
        "font_number_fg_color",
        "數字顏色 RGB(A)",
        rgba!("候選前面選字鍵（1、2、3…）的顏色。"),
    ),
];

const COLORS_RIGHT: &[Spec] = &[
    color(
        "notify_fg_color",
        "訊息文字顏色 RGB(A)",
        rgba!("切換模式時訊息視窗的文字顏色。"),
    ),
    color(
        "notify_bg_color",
        "訊息背景顏色 RGB(A)",
        rgba!("切換模式時訊息視窗的背景顏色。"),
    ),
    color(
        "notify_border_color",
        "訊息邊框顏色 RGB(A)",
        rgba!("切換模式時訊息視窗的邊框顏色。"),
    ),
];

/// The colors 色彩佈景主題 sets, in the order of each theme below.
const COLOR_KEYS: [&str; 9] = [
    "font_fg_color",
    "font_bg_color",
    "cand_list_border_color",
    "font_highlight_fg_color",
    "font_highlight_bg_color",
    "font_number_fg_color",
    "notify_fg_color",
    "notify_bg_color",
    "notify_border_color",
];

/// 深色主題 and 淺色主題; 自訂 is any other colors.
const THEMES: [(&str, [&str; 9]); 2] = [
    (
        "深色主題",
        [
            "EEEEEEFF", "333333FF", "222222FF", "FFFFFFFF", "111111FF", "8C8DFFFF", "EEEEEEFF",
            "333333FF", "222222FF",
        ],
    ),
    (
        "淺色主題",
        [
            "000000FF", "FAFAFAFF", "D6D9DBFF", "FFFFFFFF", "000000FF", "0000FFFF", "000000FF",
            "FCFBDAFF", "D6D9DBFF",
        ],
    ),
];

const LAYOUT: &[Spec] = &[
    Spec {
        kind: Kind::Choice(&[
            (0, "標準鍵盤"),
            (1, "許氏鍵盤"),
            (2, "IBM 鍵盤"),
            (3, "精業鍵盤"),
            (4, "倚天鍵盤"),
            (5, "倚天 26 鍵"),
            (6, "Dvorak (掃描碼變換)"),
            (7, "Dvorak 許氏 (掃描碼變換)"),
            (8, "大千 26 鍵"),
            (9, "漢語拼音"),
            (10, "台灣華語羅馬拼音"),
            (11, "注音二式"),
            (12, "Carplx (掃描碼變換)"),
            (16, "Colemak (掃描碼變換)"),
            (13, "Colemak-DH ANSI (掃描碼變換)"),
            (14, "Colemak-DH Orth (掃描碼變換)"),
            (15, "Workman (掃描碼變換)"),
        ]),
        ..check(
            "keyboard_layout",
            "中文鍵盤布局：",
            "注音符號在鍵盤上的排列，照你習慣的注音鍵盤選，只在注音模式作用。標「掃描碼變換」的，是給 Windows 英文鍵盤設成 Dvorak、Colemak 等配置的人：注音仍在標準（或許氏）鍵盤的實體位置。「漢語拼音」「台灣華語羅馬拼音」「注音二式」是新酷音內建的逐字拼音，每個字要打聲調或空白；要連打拼音請改用「打字行為」的「以漢語拼音輸入」。",
        )
    },
    more(),
    Spec {
        kind: Kind::Choice(&[
            (0, "無"),
            (1, "Dvorak"),
            (2, "Carplx (QGMLWY)"),
            (3, "Colemak"),
            (4, "Colemak-DH ANSI"),
            (5, "Colemak-DH Orth"),
            (6, "Workman"),
        ]),
        ..check(
            "simulate_english_layout",
            "模擬英文鍵盤布局：",
            "Windows 的鍵盤維持 QWERTY，英文模式時由輸入法把按鍵換成 Dvorak 等配置的字母。可能會讓某些網頁的快捷鍵失效。",
        )
    }
    .advanced(),
];

const PINYIN: &[Spec] = &[Spec {
    // Indexed by the config's shuangpin.
    kind: Kind::Choice(&[
        (0, "全拼"),
        (1, "小鶴雙拼"),
        (2, "自然碼雙拼"),
        (3, "微軟雙拼"),
        (4, "搜狗雙拼"),
    ]),
    ..check(
        "shuangpin",
        "拼法：",
        "全拼：照拼音的字母打，例如 shuang。雙拼：每個字固定兩鍵（聲母鍵＋韻母鍵），打完第二鍵就轉成中文，鍵位依方案而定。只在拼音模式作用。",
    )
}];

const fn fuzzy(label: &'static str, bit: i64) -> Spec {
    Spec {
        kind: Kind::Bit(bit),
        ..check("fuzzy_pinyin", label, "")
    }
}

// Bits of chewing's zhuyin::FUZZY_*.
const FUZZY_INITIALS: &[Spec] = &[
    header("聲母"),
    fuzzy("z = zh", 1 << 0),
    fuzzy("c = ch", 1 << 1),
    fuzzy("s = sh", 1 << 2),
    fuzzy("n = l", 1 << 3),
    fuzzy("f = h", 1 << 4),
    fuzzy("r = l", 1 << 5),
];

const FUZZY_FINALS: &[Spec] = &[
    header("韻母"),
    fuzzy("an = ang（含 ian = iang、uan = uang）", 1 << 6),
    fuzzy("en = eng", 1 << 7),
    fuzzy("in = ing", 1 << 8),
];

const UPDATE: &[Spec] = &[
    header("更新"),
    check(
        "check_update",
        "自動檢查更新",
        "登入後和每天中午，看 GitHub 上有沒有新版本；有新版本時會跳出詢問，同意才下載安裝。取消勾選就不再連到 GitHub，要更新時自己下載新版的安裝檔。",
    ),
    check(
        "check_update_at_logon",
        "登入時也檢查一次",
        "每次登入 Windows（包括開機後登入）約 5 分鐘時查看一次，不管距離上次多久；同一次登入之後仍照下面的間隔。",
    )
    .disabled_when("check_update", false),
    Spec {
        kind: Kind::Spin(1, 30),
        ..check(
            "update_check_days",
            "每隔幾天檢查一次新版本：",
            "距離上次查看要滿這麼多天才會再查，1 到 30 天。",
        )
    }
    .disabled_when("check_update", false),
];

const LOG: &[Spec] = &[
    header("問題記錄"),
    Spec {
        kind: Kind::Named(&[
            ("error", "error：只記錯誤"),
            ("warn", "warn：錯誤與警告（預設）"),
            ("info", "info：再加上狀態變化"),
            ("debug", "debug：再加上按鍵與打出的字"),
            ("trace", "trace：全部"),
        ]),
        ..check(
            "log_level",
            "記錄等級：",
            r"輸入法把發生的事記在 %AppData%\InputMethodEditor\ime-<日期>.log，一天一個檔，保留 14 天、總共最多 100 MB；回報問題時附上發生問題那天的檔。debug 和 trace 會記下每個按鍵和打出的字，找到問題後請改回 warn。",
        )
    },
];

/// In the order of Settings.tables, which the window's Columns name.
const TABLES: [&[Spec]; 11] = [
    TYPING_LEFT,
    TYPING_RIGHT,
    APPEARANCE,
    COLORS_LEFT,
    COLORS_RIGHT,
    LAYOUT,
    PINYIN,
    FUZZY_INITIALS,
    FUZZY_FINALS,
    UPDATE,
    LOG,
];

/// The tables on each tab, which its reset puts back.
const TAB_TABLES: [&[usize]; 11] = [
    &[0, 1],
    &[],
    &[2, 3, 4],
    &[5],
    &[6, 7, 8],
    &[],
    &[],
    &[],
    &[],
    &[10],
    &[9],
];
const KEYBIND_TAB: i32 = 5;
const SYMBOLS_TAB: i32 = 6;
const SWKB_TAB: i32 = 7;

const ACTIONS: [(&str, &str); 5] = [
    ("text", "輸入文字或符號"),
    ("toggle_simplified_chinese", "切換輸出簡體中文"),
    ("toggle_hsu_keyboard", "切換標準或許氏鍵盤"),
    ("toggle_pinyin", "切換注音或拼音"),
    ("selecting_unlearn_phrase", "忘掉選擇中詞彙"),
];

static FONTS: OnceLock<Vec<FontFamilyName>> = OnceLock::new();

/// The fonts to pick from, the one set first if it isn't installed.
fn font_choices(current: &str) -> Vec<(String, String)> {
    let fonts = FONTS.get_or_init(system_fonts);
    let mut choices: Vec<_> = fonts
        .iter()
        .map(|font| (font.name.clone(), font.display_name.clone()))
        .collect();
    if !fonts.iter().any(|font| font.name == current) {
        choices.insert(0, (current.to_owned(), current.to_owned()));
    }
    choices
}

/// 深色主題, 淺色主題 or 自訂, by the colors.
fn theme(cfg: &Value) -> usize {
    THEMES
        .iter()
        .position(|(_, colors)| {
            COLOR_KEYS
                .iter()
                .zip(colors)
                .all(|(key, color)| cfg[*key] == *color)
        })
        .unwrap_or(THEMES.len())
}

fn names<S: Into<SharedString>>(names: impl IntoIterator<Item = S>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from_iter(names.into_iter().map(Into::into)))
}

fn row(spec: &Spec, cfg: &Value) -> Row {
    let value = &cfg[spec.key];
    let mut row = Row {
        kind: RowKind::Check,
        label: spec.label.into(),
        hint: hint(spec.hint),
        advanced: spec.advanced,
        enabled: spec.disabled_when.is_none_or(|(key, on)| cfg[key] != on),
        checked: value.as_bool() == Some(true),
        choices: ModelRc::default(),
        value: 0,
        minimum: 0,
        maximum: 0,
        text: SharedString::default(),
    };
    match spec.kind {
        Kind::Header => row.kind = RowKind::Header,
        Kind::More => row.kind = RowKind::More,
        Kind::Check => {}
        Kind::Bit(bit) => row.checked = value.as_i64().unwrap_or_default() & bit != 0,
        Kind::Choice(choices) => {
            row.kind = RowKind::Choice;
            row.choices = names(choices.iter().map(|(_, name)| *name));
            row.value = choices
                .iter()
                .position(|(v, _)| value.as_i64() == Some(*v))
                .unwrap_or_default() as i32;
        }
        Kind::Named(choices) => {
            row.kind = RowKind::Choice;
            row.choices = names(choices.iter().map(|(_, name)| *name));
            row.value = choices
                .iter()
                .position(|(v, _)| *value == *v)
                .unwrap_or_default() as i32;
        }
        Kind::Font => {
            let choices = font_choices(value.as_str().unwrap_or_default());
            row.kind = RowKind::Choice;
            row.value = choices
                .iter()
                .position(|(name, _)| *value == name.as_str())
                .unwrap_or_default() as i32;
            row.choices = names(choices.into_iter().map(|(_, display)| display));
        }
        Kind::Theme => {
            row.kind = RowKind::Choice;
            row.choices = names(THEMES.iter().map(|(name, _)| *name).chain(["自訂"]));
            row.value = theme(cfg) as i32;
        }
        Kind::Slider => {
            let ms = value.as_i64().unwrap_or_default();
            row.kind = RowKind::Slider;
            row.label = format!("{}{ms} ms", spec.label).into();
            row.value = ms as i32;
        }
        Kind::Spin(minimum, maximum) => {
            row.kind = RowKind::Spin;
            row.value = value.as_i64().unwrap_or_default() as i32;
            row.minimum = minimum;
            row.maximum = maximum;
        }
        Kind::Color => {
            row.kind = RowKind::Text;
            row.text = value.as_str().unwrap_or_default().into();
        }
    }
    row
}

/// Every row again: one setting can turn others off or on.
fn refresh(models: &[Rc<VecModel<Row>>], cfg: &Value) {
    for (model, specs) in models.iter().zip(TABLES) {
        for (i, spec) in specs.iter().enumerate() {
            model.set_row_data(i, row(spec, cfg));
        }
    }
}

/// Tooltips don't wrap, so a hint is broken into lines here.
fn hint(text: &str) -> StyledText {
    // Any ASCII punctuation may be escaped, so none is taken for markup.
    let markdown = wrap(text, 52)
        .iter()
        .map(|line| {
            line.chars()
                .flat_map(|c| {
                    c.is_ascii_punctuation()
                        .then_some('\\')
                        .into_iter()
                        .chain([c])
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    StyledText::from_markdown(&markdown).unwrap_or_else(|_| StyledText::from_plain_text(text))
}

/// Lines of at most `width` columns, a Chinese character taking two. A run of
/// ASCII stays whole, as Ctrl or Space, and closing punctuation stays on the
/// line it closes.
fn wrap(text: &str, width: usize) -> Vec<String> {
    const CLOSING: &str = "，。、；：？！）」』";
    let mut lines = vec![String::new()];
    let mut used = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let mut word = String::from(c);
        if c.is_ascii_graphic() {
            while let Some(next) = chars.next_if(char::is_ascii_graphic) {
                word.push(next);
            }
        }
        while let Some(next) = chars.next_if(|c| CLOSING.contains(*c)) {
            word.push(next);
        }
        let columns: usize = word.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum();
        if used > 0 && used + columns > width {
            lines.push(String::new());
            used = 0;
            if word == " " {
                continue;
            }
        }
        lines.last_mut().unwrap().push_str(&word);
        used += columns;
    }
    lines
}

fn keybinds(cfg: &Value) -> Vec<Keybind> {
    cfg["keybind"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|keybind| Keybind {
            action: ACTIONS
                .iter()
                .position(|(action, _)| keybind["action"] == *action)
                .unwrap_or_default() as i32,
            param: keybind["param"].as_str().unwrap_or_default().into(),
            key: keybind["key"].as_str().unwrap_or_default().into(),
        })
        .collect()
}

/// A listed program as the IME matches it: in any case, with or without .exe.
fn app_key(line: &str) -> String {
    let name = line.trim().to_lowercase();
    if name.ends_with(".exe") {
        name
    } else {
        format!("{name}.exe")
    }
}

/// The programs in the list being added to, and in the other one.
fn picker_lists(ui: &SettingsWindow) -> (HashSet<String>, HashSet<String>) {
    let keys = |apps: SharedString| apps.lines().map(app_key).collect::<HashSet<_>>();
    let (english, chinese) = (keys(ui.get_english_apps()), keys(ui.get_chinese_apps()));
    if ui.get_picking() == 1 {
        (english, chinese)
    } else {
        (chinese, english)
    }
}

fn show_programs(ui: &SettingsWindow, mut found: Vec<programs::Program>) {
    let (listed, other) = picker_lists(ui);
    let other_mode = if ui.get_picking() == 1 {
        "中文"
    } else {
        "英數"
    };
    found.sort_by_cached_key(|program| program.name.to_lowercase());
    let rows = found.into_iter().map(|program| {
        let key = app_key(&program.exe);
        let (added, taken) = (listed.contains(&key), other.contains(&key));
        let note = if added {
            " · 已在清單中".to_owned()
        } else if taken {
            format!(" · 已在{other_mode}清單")
        } else {
            String::new()
        };
        let icon = program.icon.map(|(width, height, rgba)| {
            Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
                &rgba, width, height,
            ))
        });
        Program {
            name: program.name.into(),
            exe: program.exe.into(),
            note: note.into(),
            has_icon: icon.is_some(),
            icon: icon.unwrap_or_default(),
            checked: added,
            enabled: !added && !taken,
            shown: true,
        }
    });
    ui.set_programs(ModelRc::new(VecModel::from_iter(rows)));
    ui.set_programs_loaded(true);
    count_picks(ui);
}

/// For 加入 N 個 and 全選, which takes what the search shows; picks hidden by
/// it stay as they are.
fn count_picks(ui: &SettingsWindow) {
    let programs = ui.get_programs();
    let pickable = || programs.iter().filter(|p| p.shown && p.enabled);
    let (shown, picked_shown) = (pickable().count(), pickable().filter(|p| p.checked).count());
    ui.set_picked(programs.iter().filter(|p| p.enabled && p.checked).count() as i32);
    ui.set_pickable(shown as i32);
    ui.set_all_picked(shown > 0 && picked_shown == shown);
}

fn offer_update(ui: &SettingsWindow, found: Result<Option<(String, update::Asset)>, String>) {
    let done = |status: String| {
        ui.set_update_status(status.into());
        ui.set_checking(false);
    };
    let (latest, asset) = match found {
        Ok(Some(found)) => found,
        Ok(None) => return done(format!("已是最新版本（{VERSION}）。")),
        Err(error) => return done(format!("無法更新：{error}")),
    };
    let question = format!(
        "InputMethodEditor {latest} 已經推出，目前使用的是 {VERSION}。\n\n要下載並安裝嗎？"
    );
    if !ask(ui.window(), &question) {
        return done(format!("有新版本 {latest}，尚未安裝。"));
    }
    ui.set_update_status("正在下載並安裝…".into());
    let weak = ui.as_weak();
    thread::spawn(move || {
        let status = match update::install_update(&asset, &latest) {
            Ok(true) => format!("已更新到 {latest}，之後開啟的程式會用新版。"),
            Ok(false) => format!("有新版本 {latest}，尚未安裝。"),
            Err(error) => format!("無法更新：{error}"),
        };
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_update_status(status.into());
            ui.set_checking(false);
        });
    });
}

/// The settings as being edited, until saved: the tables' part; the texts
/// are the window's.
struct Draft {
    tsf: Value,
    defaults: Config,
}

impl Draft {
    fn config(&self, ui: &SettingsWindow) -> Config {
        let mut tsf: ChewingTsfConfig =
            serde_json::from_value(self.tsf.clone()).expect("a ChewingTsfConfig");
        tsf.english_apps = ui.get_english_apps().into();
        tsf.chinese_apps = ui.get_chinese_apps().into();
        tsf.enable_symbol_key = ui.get_symbol_key_on();
        tsf.symbol_key = ui.get_symbol_key().into();
        Config {
            chewing_tsf: tsf,
            symbols_dat: ui.get_symbols().into(),
            swkb_dat: ui.get_swkb().into(),
            custom_phrase_dat: ui.get_phrases().into(),
        }
    }
}

/// The texts of `config` into the window.
fn show_texts(ui: &SettingsWindow, config: &Config) {
    let tsf = &config.chewing_tsf;
    ui.set_english_apps(tsf.english_apps.as_str().into());
    ui.set_chinese_apps(tsf.chinese_apps.as_str().into());
    ui.set_symbol_key_on(tsf.enable_symbol_key);
    ui.set_symbol_key(tsf.symbol_key.as_str().into());
    ui.set_symbols(config.symbols_dat.as_str().into());
    ui.set_swkb(config.swkb_dat.as_str().into());
    ui.set_phrases(config.custom_phrase_dat.as_str().into());
}

fn save(ui: &SettingsWindow, draft: &Draft) -> bool {
    config::save_config(draft.config(ui))
        .inspect_err(|error| error_box(Some(ui.window()), error))
        .is_ok()
}

pub(crate) fn run() -> Result<(), slint::PlatformError> {
    // As the window's title.
    if bring_forward(&format!("InputMethodEditor 設定 ({VERSION})")) {
        return Ok(());
    }
    let loaded = match config::load_config() {
        Ok(config) => config,
        Err(error) => {
            error_box(None, &error);
            return Ok(());
        }
    };
    let ui = SettingsWindow::new()?;
    ui.set_version(VERSION.into());
    show_texts(&ui, &loaded);
    ui.set_symbol_hint(hint(
        "勾選時，中文模式下按這個鍵叫出下面的符號表。不勾選，或換成別的鍵時，` 照常打出 `（寫 Markdown 常用）。按鍵的寫法同「自訂快捷鍵」，例如 `、Ctrl+`、F2；寫錯的話不會開啟符號表。",
    ));
    ui.set_keybind_hints(ModelRc::new(VecModel::from(vec![
        hint(
            "按下快捷鍵時做什麼。「輸入文字或符號」打出參數裡的文字；「忘掉選擇中詞彙」要在選字視窗用方向鍵反白一個詞時按，讓它不再優先出現。",
        ),
        hint("只有「輸入文字或符號」用得到：按下快捷鍵時打出的文字。"),
        hint(
            "用 + 連接修飾鍵和按鍵，例如 Ctrl+F12、Ctrl+Delete。修飾鍵：Ctrl、Shift、Alt；按鍵：單一字元、F1～F12、Esc、Tab、Enter、Space、Delete、Backspace、Home、End、CapsLock。",
        ),
    ])));
    ui.set_actions(names(ACTIONS.iter().map(|(_, name)| *name)));

    let tsf = serde_json::to_value(&loaded.chewing_tsf).expect("a ChewingTsfConfig");
    let models: Rc<Vec<Rc<VecModel<Row>>>> = Rc::new(
        TABLES
            .iter()
            .map(|specs| {
                Rc::new(VecModel::from_iter(
                    specs.iter().map(|spec| row(spec, &tsf)),
                ))
            })
            .collect(),
    );
    let settings = ui.global::<Settings>();
    settings.set_tables(ModelRc::new(VecModel::from_iter(
        models.iter().map(|model| ModelRc::from(model.clone())),
    )));
    let keybind_model = Rc::new(VecModel::from(keybinds(&tsf)));
    ui.set_keybinds(keybind_model.clone().into());
    let draft = Rc::new(RefCell::new(Draft {
        tsf,
        defaults: config::default_config(),
    }));

    let edit = Rc::new({
        let (ui, draft, models) = (ui.as_weak(), draft.clone(), models.clone());
        move |change: &dyn Fn(&mut Value)| {
            let mut draft = draft.borrow_mut();
            change(&mut draft.tsf);
            let tsf = &mut draft.tsf;
            // CapsLock takes over Shift's switching.
            if tsf["enable_caps_lock"] == true {
                tsf["switch_lang_with_shift"] = false.into();
            }
            refresh(&models, tsf);
            // Dark candidate colors turn the windows dark at once, as when
            // they open; light leaves them to Windows.
            let ui = ui.unwrap();
            let dark = config::is_dark(tsf["font_bg_color"].as_str().unwrap_or_default());
            if let Some(scheme) = follow_candidates(ui.window(), dark) {
                ui.set_scheme(scheme);
            }
        }
    });
    let spec = |table: i32, row: i32| &TABLES[table as usize][row as usize];
    settings.on_toggled({
        let edit = edit.clone();
        move |table, row, on| {
            let spec = spec(table, row);
            edit(&|tsf| match spec.kind {
                Kind::Bit(bit) => {
                    let bits = tsf[spec.key].as_i64().unwrap_or_default();
                    tsf[spec.key] = (if on { bits | bit } else { bits & !bit }).into();
                }
                _ => tsf[spec.key] = on.into(),
            })
        }
    });
    settings.on_selected({
        let (ui, edit) = (ui.as_weak(), edit.clone());
        move |table, row, index| {
            let spec = spec(table, row);
            let index = index as usize;
            match spec.kind {
                Kind::Choice(choices) => edit(&|tsf| tsf[spec.key] = choices[index].0.into()),
                Kind::Named(choices) => edit(&|tsf| tsf[spec.key] = choices[index].0.into()),
                Kind::Font => edit(&|tsf| {
                    let current = tsf[spec.key].as_str().unwrap_or_default().to_owned();
                    tsf[spec.key] = font_choices(&current)[index].0.clone().into();
                }),
                Kind::Theme => match THEMES.get(index) {
                    Some((_, colors)) => edit(&|tsf| {
                        for (key, color) in COLOR_KEYS.iter().zip(colors) {
                            tsf[*key] = (*color).into();
                        }
                    }),
                    // 自訂, with the colors to set.
                    None => {
                        ui.unwrap().global::<Settings>().set_show_advanced(true);
                        edit(&|_| {});
                    }
                },
                _ => {}
            }
        }
    });
    settings.on_valued({
        let edit = edit.clone();
        move |table, row, value| {
            let spec = spec(table, row);
            edit(&|tsf| tsf[spec.key] = value.into())
        }
    });
    settings.on_typed({
        let edit = edit.clone();
        move |table, row, text| {
            let spec = spec(table, row);
            edit(&|tsf| tsf[spec.key] = text.as_str().into())
        }
    });

    ui.on_keybind_action({
        let (draft, model) = (draft.clone(), keybind_model.clone());
        move |index, action| {
            let index = index as usize;
            draft.borrow_mut().tsf["keybind"][index]["action"] = ACTIONS[action as usize].0.into();
            if let Some(mut keybind) = model.row_data(index) {
                keybind.action = action;
                model.set_row_data(index, keybind);
            }
        }
    });
    ui.on_keybind_param({
        let (draft, model) = (draft.clone(), keybind_model.clone());
        move |index, text| {
            let index = index as usize;
            draft.borrow_mut().tsf["keybind"][index]["param"] = text.as_str().into();
            if let Some(mut keybind) = model.row_data(index) {
                keybind.param = text;
                model.set_row_data(index, keybind);
            }
        }
    });
    ui.on_keybind_key({
        let (draft, model) = (draft.clone(), keybind_model.clone());
        move |index, text| {
            let index = index as usize;
            draft.borrow_mut().tsf["keybind"][index]["key"] = text.as_str().into();
            if let Some(mut keybind) = model.row_data(index) {
                keybind.key = text;
                model.set_row_data(index, keybind);
            }
        }
    });
    ui.on_keybind_remove({
        let (draft, model) = (draft.clone(), keybind_model.clone());
        move |index| {
            let index = index as usize;
            if let Some(list) = draft.borrow_mut().tsf["keybind"].as_array_mut() {
                list.remove(index);
            }
            model.remove(index);
        }
    });
    ui.on_keybind_add({
        let (draft, model) = (draft.clone(), keybind_model.clone());
        move || {
            if let Some(list) = draft.borrow_mut().tsf["keybind"].as_array_mut() {
                list.push(json!({ "key": "", "action": "text", "param": "" }));
            }
            model.push(Keybind {
                action: 0,
                param: "".into(),
                key: "".into(),
            });
        }
    });

    ui.on_open_picker({
        let ui = ui.as_weak();
        move |list| {
            let ui = ui.unwrap();
            ui.set_picking(list);
            ui.set_programs(ModelRc::default());
            ui.set_programs_loaded(false);
            count_picks(&ui);
            let weak = ui.as_weak();
            thread::spawn(move || {
                let found = programs::programs();
                let _ = weak.upgrade_in_event_loop(move |ui| show_programs(&ui, found));
            });
        }
    });
    ui.on_search_programs({
        let ui = ui.as_weak();
        move |query| {
            let ui = ui.unwrap();
            let needle = query.trim().to_lowercase();
            let programs = ui.get_programs();
            for (i, mut program) in programs.iter().enumerate() {
                let shown = program.name.to_lowercase().contains(&needle)
                    || program.exe.to_lowercase().contains(&needle);
                if program.shown != shown {
                    program.shown = shown;
                    programs.set_row_data(i, program);
                }
            }
            count_picks(&ui);
        }
    });
    ui.on_pick_program({
        let ui = ui.as_weak();
        move |index, on| {
            let ui = ui.unwrap();
            let programs = ui.get_programs();
            if let Some(mut program) = programs.row_data(index as usize) {
                program.checked = on;
                programs.set_row_data(index as usize, program);
            }
            count_picks(&ui);
        }
    });
    ui.on_pick_all({
        let ui = ui.as_weak();
        move |on| {
            let ui = ui.unwrap();
            let programs = ui.get_programs();
            for (i, mut program) in programs.iter().enumerate() {
                if program.shown && program.enabled && program.checked != on {
                    program.checked = on;
                    programs.set_row_data(i, program);
                }
            }
            count_picks(&ui);
        }
    });
    ui.on_add_programs({
        let ui = ui.as_weak();
        move || {
            let ui = ui.unwrap();
            let exes: Vec<_> = ui
                .get_programs()
                .iter()
                .filter(|program| program.enabled && program.checked)
                .map(|program| program.exe.to_string())
                .collect();
            let add = |list: SharedString| {
                let kept = list.trim_end();
                let added = exes.join("\n");
                SharedString::from(if kept.is_empty() {
                    added
                } else {
                    format!("{kept}\n{added}")
                })
            };
            if ui.get_picking() == 1 {
                ui.set_english_apps(add(ui.get_english_apps()));
            } else {
                ui.set_chinese_apps(add(ui.get_chinese_apps()));
            }
            ui.set_picking(0);
        }
    });
    ui.on_close_picker({
        let ui = ui.as_weak();
        move || ui.unwrap().set_picking(0)
    });

    ui.on_check_update({
        let ui = ui.as_weak();
        move || {
            let ui = ui.unwrap();
            ui.set_checking(true);
            ui.set_update_status("正在檢查…".into());
            let weak = ui.as_weak();
            thread::spawn(move || {
                let found = update::find_update();
                let _ = weak.upgrade_in_event_loop(move |ui| offer_update(&ui, found));
            });
        }
    });
    ui.on_open_log_dir({
        let ui = ui.as_weak();
        move || {
            // The text service's ime.log, to attach to a report.
            let opened = user_dir()
                .map_err(|error| error.to_string())
                .and_then(|dir| match dir.is_dir() {
                    true => Command::new("explorer.exe")
                        .arg(dir)
                        .spawn()
                        .map(drop)
                        .map_err(|error| error.to_string()),
                    false => Err(format!("{} 不存在", dir.display())),
                });
            if let Err(error) = opened {
                error_box(
                    Some(ui.unwrap().window()),
                    &format!("無法開啟記錄檔資料夾：{error}"),
                );
            }
        }
    });

    // Only the form: 確定 or 套用 saves it, 取消 leaves things as they were.
    ui.on_reset({
        let (ui, draft, edit, model) = (
            ui.as_weak(),
            draft.clone(),
            edit.clone(),
            keybind_model.clone(),
        );
        move |tab| {
            let ui = ui.unwrap();
            let tsf = serde_json::to_value(&draft.borrow().defaults.chewing_tsf)
                .expect("a ChewingTsfConfig");
            let keys: Vec<&str> = TAB_TABLES[tab as usize]
                .iter()
                .flat_map(|&table| TABLES[table])
                .map(|spec| spec.key)
                .filter(|key| !key.is_empty())
                .collect();
            edit(&|cfg| {
                for key in &keys {
                    cfg[*key] = tsf[*key].clone();
                }
                if tab == KEYBIND_TAB {
                    cfg["keybind"] = tsf["keybind"].clone();
                }
            });
            let defaults = &draft.borrow().defaults;
            match tab {
                KEYBIND_TAB => model.set_vec(keybinds(&tsf)),
                // The symbol tables go back to the system's.
                SYMBOLS_TAB => {
                    ui.set_symbol_key_on(defaults.chewing_tsf.enable_symbol_key);
                    ui.set_symbol_key(defaults.chewing_tsf.symbol_key.as_str().into());
                    ui.set_symbols(defaults.symbols_dat.as_str().into());
                }
                SWKB_TAB => ui.set_swkb(defaults.swkb_dat.as_str().into()),
                _ => {}
            }
        }
    });
    ui.on_import({
        let (ui, edit, model) = (ui.as_weak(), edit.clone(), keybind_model.clone());
        move || {
            let ui = ui.unwrap();
            let Some(path) = open_file(ui.window(), "TOML", "toml") else {
                return;
            };
            match config::import_config(&path) {
                Ok(config) => {
                    show_texts(&ui, &config);
                    let tsf =
                        serde_json::to_value(&config.chewing_tsf).expect("a ChewingTsfConfig");
                    model.set_vec(keybinds(&tsf));
                    edit(&|cfg| *cfg = tsf.clone());
                }
                Err(error) => error_box(
                    Some(ui.window()),
                    &format!("無法匯入設定檔，請確認檔案格式正確。\n\n{error}"),
                ),
            }
        }
    });
    ui.on_export({
        let (ui, draft) = (ui.as_weak(), draft.clone());
        move || {
            let ui = ui.unwrap();
            let Some(path) = save_file(ui.window(), "TOML", "toml", "InputMethodEditor 設定.toml")
            else {
                return;
            };
            let config = draft.borrow().config(&ui);
            if let Err(error) = config::export_config(&path, &config) {
                error_box(Some(ui.window()), &format!("無法寫入檔案。\n\n{error}"));
            }
        }
    });
    ui.on_open_url(|url| open_url(&url));
    ui.on_ok({
        let (ui, draft) = (ui.as_weak(), draft.clone());
        move || {
            let ui = ui.unwrap();
            if save(&ui, &draft.borrow()) {
                // Shown, the error leaves the window open to fix it.
                let _ = slint::quit_event_loop();
            }
        }
    });
    ui.on_cancel(|| {
        let _ = slint::quit_event_loop();
    });
    ui.on_apply({
        let ui = ui.as_weak();
        move || {
            save(&ui.unwrap(), &draft.borrow());
        }
    });
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    center_when_made(&ui);
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::{TABLES, THEMES, app_key, theme, wrap};
    use serde_json::json;

    #[test]
    fn a_hint_wraps_between_words_and_before_closing_punctuation() {
        let lines = wrap("按 Shift＋空白鍵在半形與全形之間切換，一開始是半形。", 20);
        let width =
            |line: &str| -> usize { line.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum() };
        // Wider lines would run the tooltip off the window.
        assert!(lines.iter().all(|line| width(line) <= 20), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("Shift")), "{lines:?}");
        assert!(
            lines
                .iter()
                .all(|line| !line.starts_with(['，', '。', ' '])),
            "{lines:?}"
        );
        assert_eq!(
            lines.concat().replace(' ', ""),
            "按Shift＋空白鍵在半形與全形之間切換，一開始是半形。"
        );
    }

    #[test]
    fn every_setting_in_the_tables_is_a_setting() {
        // A key typed wrong would show a default and save nothing.
        let defaults =
            serde_json::to_value(chewing_tip_core::config::ChewingTsfConfig::default()).unwrap();
        for spec in TABLES.iter().flat_map(|specs| specs.iter()) {
            assert!(
                spec.key.is_empty() || defaults.get(spec.key).is_some(),
                "{}",
                spec.key
            );
        }
    }

    #[test]
    fn the_theme_is_custom_unless_every_color_matches() {
        let keys = super::COLOR_KEYS;
        let mut cfg = json!({});
        for (key, color) in keys.iter().zip(THEMES[0].1) {
            cfg[*key] = color.into();
        }
        assert_eq!(theme(&cfg), 0);
        cfg["notify_border_color"] = "222223FF".into();
        assert_eq!(theme(&cfg), THEMES.len());
    }

    #[test]
    fn a_listed_program_matches_in_any_case_with_or_without_exe() {
        assert_eq!(app_key(" Chrome "), "chrome.exe");
        assert_eq!(app_key("Telegram.EXE"), "telegram.exe");
    }
}
