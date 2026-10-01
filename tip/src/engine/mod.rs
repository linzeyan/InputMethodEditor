// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

//! Typing, apart from where the text goes. The TSF text service drives it
//! through [`Frontend`].

pub(crate) mod key_event;
pub(crate) mod pinyin;
pub(crate) mod shuangpin;

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::{OsString, c_void};
use std::{env, fs};
use std::io::ErrorKind;
use std::mem;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chewing::editor::zhuyin_layout::{self, KeyBehavior, KeyboardLayoutCompat, SyllableEditor};
use chewing::editor::{
    BasicEditor, CharacterForm, ConversionEngineKind, Editor, EditorKeyBehavior, LanguageMode,
    UserPhraseAddDirection,
};
use chewing::input::keycode::Keycode;
use chewing::input::keymap::{
    DVORAK_MAP, INVERTED_COLEMAK_DH_ANSI_MAP, INVERTED_COLEMAK_DH_ORTH_MAP, INVERTED_COLEMAK_MAP,
    INVERTED_DVORAK_MAP, INVERTED_QGMLWY_MAP, INVERTED_WORKMAN_MAP,
};
use chewing::input::keysym::{
    Keysym, SYM_BACKSPACE, SYM_CAPSLOCK, SYM_ESC, SYM_LEFTSHIFT, SYM_RIGHTSHIFT, SYM_SPACE,
};
use chewing::input::{KeyState, KeyboardEvent, keycode, keysym};
use chewing::zhuyin::{Bopomofo, Syllable, set_fuzzy_sounds};
use chewing_tip_core::config::{ChewingTsfConfig, Config};
use chewing_tip_core::{PRODUCT_NAME, SETTINGS_SCHEME};
use chewing_tip_core::phrases::{self, PHRASES_FILE};
use chewing_tip_core::shell::{open_url, share_user_dir, user_dir};
use log::{debug, error, info};
use scoped_error::{ErrorExt, expect_error};
use windows::Win32::Foundation::{HMODULE, HWND, RECT};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::UI::TextServices::ITfThreadMgr;
use windows::Win32::UI::WindowsAndMessaging::{
    CheckMenuItem, HICON, HMENU, MF_CHECKED, MF_UNCHECKED,
};
use windows_core::{ComObject, HSTRING};
use windows_registry::CURRENT_USER;
use zhconv::{Variant, zhconv};

use self::key_event::{KeymapOp, SimulatedKeyboard, SystemKeyboardEvent};
use self::pinyin::ContinuousPinyin;
use self::shuangpin::{Scheme, Shuangpin};
use crate::com::G_HINSTANCE;
use crate::keybind::Keybinding;
use crate::text_service::icons::LangIconSet;
use crate::text_service::resources::*;
use crate::text_service::theme::{ThemeDetector, WindowsTheme};
use crate::text_service::ui_elements::{
    CandidateList, FilterKeyResult, Model, Notification, NotificationModel,
};
use crate::ui::gfx::color_s;

const SEL_KEYS: [&str; 6] = [
    "1234567890",
    "asdfghjkl;",
    "asdfzxcv89",
    "asdfjkl789",
    "aoeuhtn789",
    "1234qweras",
];

#[derive(Debug)]
enum ShiftKeyState {
    Down(Instant),
    Consumed,
    Up,
}

impl ShiftKeyState {
    fn release(&mut self) -> Option<Duration> {
        let duration = match self {
            ShiftKeyState::Down(instant) => Some(instant.elapsed()),
            ShiftKeyState::Consumed | ShiftKeyState::Up => None,
        };
        *self = ShiftKeyState::Up;
        duration
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TsfLangMode {
    Chinese,
    English,
    DisabledChinese,
    DisabledEnglish,
}

impl TsfLangMode {
    pub(crate) fn is_disabled(&self) -> bool {
        matches!(
            self,
            TsfLangMode::DisabledChinese | TsfLangMode::DisabledEnglish
        )
    }
}

impl From<TsfLangMode> for LanguageMode {
    fn from(value: TsfLangMode) -> Self {
        match value {
            TsfLangMode::Chinese => LanguageMode::Chinese,
            TsfLangMode::English => LanguageMode::English,
            TsfLangMode::DisabledChinese => LanguageMode::Chinese,
            TsfLangMode::DisabledEnglish => LanguageMode::English,
        }
    }
}

impl PartialEq<LanguageMode> for TsfLangMode {
    fn eq(&self, other: &LanguageMode) -> bool {
        matches!(
            (self, other),
            (TsfLangMode::Chinese, LanguageMode::Chinese)
                | (TsfLangMode::English, LanguageMode::English)
        )
    }
}

/// Where typing goes: a TSF document.
pub(crate) trait Frontend {
    /// A composition is open in the document.
    fn has_composition(&self) -> bool;
    /// Whether the document takes typing now.
    fn is_context_mutable(&self) -> Result<bool>;
    /// Where the text being typed is, in screen coordinates; popups open
    /// below it.
    fn caret_rect(&self) -> Result<RECT>;
    /// The window popups belong to.
    fn popup_parent(&self) -> Result<HWND>;
    /// Told about popups, so that apps drawing their own can hide ours.
    fn thread_mgr(&self) -> ITfThreadMgr;
    fn insert_text(&mut self, text: &str) -> Result<()>;
    /// Puts `commit` into the document and shows `preedit` as what is being
    /// typed; `segments` and `cursor` are character offsets into it.
    fn set_composition_string(
        &mut self,
        commit: String,
        preedit: String,
        segments: Vec<(usize, usize)>,
        cursor: usize,
    ) -> Result<()>;
    fn end_composition(&mut self) -> Result<()>;
    /// Shows the language mode and the output settings.
    fn update_lang_buttons(&self, engine: &Engine) -> Result<()>;
    /// The engine switched between Chinese and English itself.
    fn lang_mode_changed(&self);
}

pub(crate) struct Engine {
    pub(crate) lang_mode: Cell<TsfLangMode>,
    shift_key_state: ShiftKeyState,
    pub(crate) cfg: Config,
    kbtype: KeyboardLayoutCompat,
    keymap: KeymapOp,
    keybindings: Vec<Keybinding>,
    pub(crate) chewing_editor: Editor,
    notification: Option<ComObject<Notification>>,
    candidate_list: Option<ComObject<CandidateList>>,
    /// For each slot in the candidate list, the editor's index on the current
    /// page. They differ once converted duplicates are hidden.
    candidate_indices: Vec<usize>,
    /// What the character had before the last key, a Space, made it tone 1.
    /// A second Space puts it back and types a space.
    space_undo: Option<Syllable>,
    /// Custom phrases by code.
    phrases: HashMap<String, String>,
    /// The letters typed since the composition was empty, while nothing else
    /// has been; Space after a code types its phrase.
    typed: Option<String>,
    /// Every character typed fullwidth, as Shift+Space switched it.
    fullwidth: bool,
}

impl Engine {
    pub(crate) fn new(ui: &mut impl Frontend) -> Result<Engine> {
        let cfg = Config::from_reg().unwrap_or_else(|error| {
            error!("unable to load config: {error}");
            Config::default()
        });

        // Initialize a temp editor, this will be replaced in init_chewing_context.
        let editor = new_editor()?;

        let mut engine = Engine {
            lang_mode: Cell::new(TsfLangMode::English),
            shift_key_state: ShiftKeyState::Up,
            cfg,
            kbtype: KeyboardLayoutCompat::Default,
            keymap: KeymapOp::None,
            keybindings: vec![],
            chewing_editor: editor,
            notification: Default::default(),
            candidate_list: Default::default(),
            candidate_indices: vec![],
            space_undo: None,
            phrases: HashMap::new(),
            typed: None,
            fullwidth: false,
        };

        if let Err(error) = engine.init_chewing_context(ui) {
            error!("unable to initialize chewing: {error:#}");
        }

        Ok(engine)
    }

    pub(crate) fn on_test_keydown(
        &mut self,
        ui: &mut impl Frontend,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        // NB: self.lang_mode might have changed earlier
        self.chewing_editor
            .set_editor_options(|opt| opt.language_mode = self.lang_mode.get().into());

        let is_context_mutable = ui.is_context_mutable()?;
        let evt = ev.to_keyboard_event(self.keymap);
        let simulate_english_layout = self.cfg.chewing_tsf.simulate_english_layout != 0;
        // Determine shift key state here, this might be our last chance seeing this key.
        if evt.ksym != SYM_LEFTSHIFT
            && evt.ksym != SYM_RIGHTSHIFT
            && evt.is_state_on(KeyState::Shift)
        {
            self.shift_key_state = ShiftKeyState::Consumed;
        }
        debug!(evt:?, shift_key_state:? = self.shift_key_state; "on_test_keydown");

        let mut shift_down = false;
        if (evt.ksym == SYM_LEFTSHIFT || evt.ksym == SYM_RIGHTSHIFT)
            && self.cfg.chewing_tsf.switch_lang_with_shift
            && matches!(self.shift_key_state, ShiftKeyState::Up)
        {
            debug!("shift_key_state = Down");
            self.shift_key_state = ShiftKeyState::Down(Instant::now());
            shift_down = true;
            // return Ok(false);
        }
        // Ok(handled?.parameters.as_bool().unwrap_or_default())
        //
        // Step 1. apply any config changes
        //
        if let Err(error) = self.apply_config_if_changed(ui) {
            error!("unable to load config: {error:#}");
        }
        //
        // Step 2. handle any mode change related keydown
        //
        // Ignore all keys if keyboard is closed
        if self.lang_mode.get().is_disabled() {
            return Ok(false);
        }
        //
        // Step 2.1 handle switch lang with Shift
        //
        if shift_down {
            return Ok(false);
        }
        //
        // Step 2.2 handle any keybindings
        //
        if self.keybindings.iter().any(|kb| kb.matches(&evt)) {
            return Ok(true);
        }
        //
        // Step 2.3 ignore CapsLock if disabled
        if evt.ksym == SYM_CAPSLOCK && !self.cfg.chewing_tsf.enable_caps_lock {
            return Ok(false);
        }
        //
        // Step 3. ignore key events if the document is readonly or inactive
        //
        if !is_context_mutable {
            return Ok(false);
        }
        //
        // Step 4. ignore key events if they might be shortcut keys
        //
        if evt.is_state_on(KeyState::Alt) {
            // bypass IME. This might be a shortcut key used in the application
            debug!("key not handled - Alt modifier key was down");
            return Ok(false);
        }
        if evt.is_state_on(KeyState::Control) {
            // bypass IME. This might be a shortcut key used in the application
            if self.is_composing(ui.has_composition()) && evt.ksym.is_digit() {
                // need to handle userphrase
                return Ok(true);
            } else if evt.is_state_on(KeyState::Shift)
                && self.cfg.chewing_tsf.easy_symbols_with_shift_ctrl
            {
                // need to handle easy symbol input
                return Ok(true);
            } else {
                debug!("key not handled - Ctrl modifier key was down");
                return Ok(false);
            }
        }
        if self.is_fullwidth_toggle(&evt) {
            return Ok(true);
        }
        if self.cfg.chewing_tsf.enable_caps_lock
            && !self.cfg.chewing_tsf.lock_chinese_on_caps_lock
            && evt.ksym.is_unicode()
        {
            // need to handle case conversion
            return Ok(true);
        }
        if !self.is_composing(ui.has_composition()) {
            // don't do further handling in pure English mode, but chewing
            // makes fullwidth letters and spaces
            if self.lang_mode.get() == LanguageMode::English
                && !simulate_english_layout
                && !self.fullwidth
            {
                debug!("key not handled - in English mode");
                return Ok(false);
            }
            // No need to handle VK_SPACE when not composing
            // This make the space key available for other shortcuts
            if evt.ksym == SYM_SPACE && !evt.is_state_on(KeyState::Shift) && !self.fullwidth {
                return Ok(false);
            }
            if !evt.ksym.is_unicode() {
                debug!("key not handled - key is not printable");
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn on_keydown(
        &mut self,
        ui: &mut impl Frontend,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        // Only the very next key may undo a tone-1 Space.
        let space_undo = self.space_undo.take();
        if !self.on_test_keydown(ui, ev)? {
            return Ok(false);
        }
        let mut evt = ev.to_keyboard_event(self.keymap);
        debug!(evt:?; "on_keydown");

        if self.is_fullwidth_toggle(&evt) {
            self.fullwidth = !self.fullwidth;
            self.apply_character_form();
            if self.cfg.chewing_tsf.show_fullwidth_notification {
                let msg = if self.fullwidth { "全形" } else { "半形" };
                self.show_message(ui, msg, Duration::from_millis(500))?;
            }
            return Ok(true);
        }

        // Handle keybindings
        // FIXME: refactor this
        let mut text_action = None;
        if let Some(keybinding) = self.keybindings.iter().find(|kb| kb.matches(&evt)) {
            debug!("matched keybinding on action={}", keybinding.action);
            let mut handled = true;
            match keybinding.action.as_str() {
                "toggle_simplified_chinese" => {
                    self.toggle_simp_chinese(ui)?;
                    // Re-render now so the text being composed switches script
                    // without waiting for the next key.
                    if self.is_composing(ui.has_composition()) {
                        self.update_candidates(ui)?;
                        self.update_preedit(ui, String::new())?;
                    }
                    if self.cfg.chewing_tsf.show_notification {
                        let msg = if self.cfg.chewing_tsf.output_simp_chinese {
                            "簡體中文"
                        } else {
                            "正體中文"
                        };
                        self.show_message(ui, msg, Duration::from_millis(500))?;
                    }
                }
                "toggle_hsu_keyboard" => {
                    self.toggle_hsu_keyboard(ui)?;
                }
                "toggle_pinyin" => {
                    if self.cfg.chewing_tsf.pinyin && self.chewing_editor.entering_syllable() {
                        self.chewing_editor.process_keyevent(pinyin::end_syllable());
                    }
                    // What is being typed goes out first: toneless pinyin
                    // syllables would convert differently under zhuyin's engine.
                    if !self.chewing_editor.is_empty() {
                        self.chewing_editor.commit()?;
                        self.chewing_editor.flush();
                        // Like the text action with nothing to add: the commit
                        // goes out below and the key goes no further.
                        text_action = Some(String::new());
                        handled = false;
                    }
                    self.toggle_pinyin(ui)?;
                }
                "text" => {
                    if !self.chewing_editor.is_empty() {
                        self.chewing_editor.commit()?;
                        // Only process_keyevent writes what chewing learned to
                        // disk; the next focus change reloads the editor from
                        // there, so anything unwritten is lost.
                        self.chewing_editor.flush();
                    }
                    text_action = Some(keybinding.param.clone());
                    handled = false;
                }
                act => {
                    if act.starts_with("selecting_") {
                        handled = false;
                    } else {
                        error!("Unsupported keybinding action: {act}");
                    }
                }
            }
            if handled {
                return Ok(true);
            }
        }

        if let Some(phrase) = self.custom_phrase(&evt) {
            // The code's letters give way to the phrase.
            self.chewing_editor.clear();
            return self.show_edit(ui, Some(phrase));
        }

        let takes = if self.cfg.chewing_tsf.pinyin {
            match Scheme::from_config(self.cfg.chewing_tsf.shuangpin) {
                Some(scheme) => shuangpin::takes(scheme, &evt),
                None => pinyin::takes(&evt),
            }
        } else {
            // Toneless zhuyin leaves the last syllable waiting like pinyin:
            // chewing ignores Enter then, and reads Shift+, as ㄝ. Space ends
            // a syllable in every layout. With tones typed, a lone syllable is
            // still unfinished, so the other engines keep chewing's way.
            self.cfg.chewing_tsf.conv_engine != 2
                || matches!(evt.ksym, SYM_BACKSPACE | SYM_ESC | SYM_CAPSLOCK)
                || !evt.has_modifiers() && evt.ksym.is_unicode()
        };
        if self.chewing_editor.entering_syllable() && !takes {
            self.chewing_editor.process_keyevent(pinyin::end_syllable());
        }

        if text_action.is_some() {
            // do nothing, handled later
        } else if evt.ksym.is_unicode() {
            let mut momentary_english_mode = false;
            let mut upper_case = false;
            if evt.is_state_on(KeyState::Shift) {
                upper_case = true;
            }
            // If shift is pressed, but we don't want to enter full shape symbols, or easy_symbol_input is not enabled
            if evt.is_state_on(KeyState::Shift)
                && matches!(self.lang_mode.get(), TsfLangMode::Chinese)
                && (!self.cfg.chewing_tsf.full_shape_symbols || evt.ksym.is_atoz())
                && !self.cfg.chewing_tsf.easy_symbols_with_shift
                && !(evt.is_state_on(KeyState::Control)
                    && self.cfg.chewing_tsf.easy_symbols_with_shift_ctrl)
            {
                momentary_english_mode = true;
                if !self.cfg.chewing_tsf.upper_case_with_shift {
                    upper_case = false;
                }
            }
            evt.ksym = if evt.ksym.is_ascii() {
                let code = evt.ksym.to_unicode();
                if upper_case {
                    Keysym::from(code.to_ascii_uppercase())
                } else {
                    Keysym::from(code.to_ascii_lowercase())
                }
            } else {
                evt.ksym
            };
            // HACK: convert sel_keys key to number key
            if self.chewing_editor.is_selecting() {
                let Some(mapped) = self.map_sel_key(evt) else {
                    // Nothing is shown in that slot; chewing would pick the
                    // hidden duplicate there.
                    return Ok(true);
                };
                evt = mapped;
            }
            if evt.ksym == SYM_SPACE && evt.is_state_on(KeyState::Shift) {
                // TODO: maybe this can be merged back to the default branch?
                self.chewing_editor.process_keyevent(evt);
            } else if self.lang_mode.get() == LanguageMode::English || momentary_english_mode {
                let old_lang_mode = self.chewing_editor.editor_options().language_mode;
                self.chewing_editor
                    .set_editor_options(|opt| opt.language_mode = LanguageMode::English);
                self.chewing_editor.process_keyevent(evt);
                self.chewing_editor
                    .set_editor_options(|opt| opt.language_mode = old_lang_mode);
            } else if let Some(old) = space_undo.filter(|_| evt.ksym == SYM_SPACE) {
                // Two Spaces type a space and leave the character as it was.
                retone(&mut self.chewing_editor, self.kbtype, old.tone());
                self.chewing_editor.process_keyevent(evt);
            } else if !self.cfg.chewing_tsf.pinyin
                && let Some(old) = change_tone(&mut self.chewing_editor, self.kbtype, evt)
            {
                if evt.ksym == SYM_SPACE {
                    self.space_undo = Some(old);
                }
            } else {
                self.chewing_editor.process_keyevent(evt);
            }
        } else {
            let mut key_handled = false;
            if self.cfg.chewing_tsf.cursor_cand_list
                && let Some(candidate_list) = &self.candidate_list
            {
                match candidate_list.filter_key_event(evt.ksym) {
                    FilterKeyResult::HandledCommit => {
                        if let Some(&index) =
                            self.candidate_indices.get(candidate_list.current_sel())
                        {
                            self.chewing_editor.select(index)?;
                        }
                        key_handled = true;
                    }
                    FilterKeyResult::Handled => {
                        candidate_list.show();
                        return Ok(true);
                    }
                    FilterKeyResult::NotHandled => {
                        // do nothing
                    }
                }
                if let Some(keybinding) = self.keybindings.iter().find(|kb| kb.matches(&evt)) {
                    debug!("matched keybinding on action={}", keybinding.action);
                    match keybinding.action.as_str() {
                        "selecting_unlearn_phrase" => {
                            if self.chewing_editor.is_selecting() {
                                // The list may show converted text; unlearning
                                // needs the phrase as the dictionary stores it.
                                if self.cfg.chewing_tsf.cursor_cand_list
                                    && let Some(candidate_list) = &self.candidate_list
                                    && let Some(&index) =
                                        self.candidate_indices.get(candidate_list.current_sel())
                                    && let Some(phrase) = self
                                        .chewing_editor
                                        .paginated_candidates()?
                                        .get(index)
                                        .cloned()
                                {
                                    let phrase_len = phrase.chars().count();
                                    // TODO: expose begin and end from selector
                                    let cursor = if self.cfg.chewing_tsf.phrase_choice_rearward {
                                        self.chewing_editor.cursor().saturating_sub(phrase_len - 1)
                                    } else {
                                        self.chewing_editor.cursor()
                                    };
                                    let syllables: Vec<Syllable> = self
                                        .chewing_editor
                                        .symbols()
                                        .iter()
                                        .skip(cursor)
                                        .take(phrase_len)
                                        .map_while(|s| s.to_syllable())
                                        .collect();
                                    if syllables.len() == phrase_len {
                                        if let Err(error) =
                                            self.chewing_editor.unlearn_phrase(&syllables, &phrase)
                                        {
                                            error!("failed to unlearn phrase: {error}");
                                        }
                                        // Not via process_keyevent, so not written yet.
                                        self.chewing_editor.flush();
                                        self.update_candidates(ui)?;
                                        // TODO: move this to editor
                                        let shown = convert_output(&self.cfg.chewing_tsf, &phrase);
                                        self.show_message(
                                            ui,
                                            &format!("刪除：{shown}"),
                                            Duration::from_millis(500),
                                        )?;
                                        key_handled = true;
                                    }
                                }
                            }
                        }
                        act => {
                            error!("Unsupported keybinding action: {act}");
                        }
                    }
                }
            }

            if !key_handled {
                self.chewing_editor.process_keyevent(evt);
            }
        }

        self.show_edit(ui, text_action)
    }

    /// A click on the candidate list's `slot`: selects it as its key would.
    pub(crate) fn select_candidate(&mut self, ui: &mut impl Frontend, slot: usize) -> Result<()> {
        if !self.chewing_editor.is_selecting() {
            return Ok(());
        }
        let Some(&index) = self.candidate_indices.get(slot) else {
            return Ok(());
        };
        self.chewing_editor.select(index)?;
        self.show_edit(ui, None)?;
        Ok(())
    }

    /// Shows what chewing made of the last key, and types what it committed;
    /// `text_action` is typed after it. Whether the key was taken.
    fn show_edit(&mut self, ui: &mut impl Frontend, text_action: Option<String>) -> Result<bool> {
        let last_behavior = self.chewing_editor.last_key_behavior();

        if last_behavior == EditorKeyBehavior::Ignore {
            debug!("early return - chewing ignored key");
            return Ok(false);
        }

        // Not composing so we can commit the text immediately
        if !self.is_composing(ui.has_composition())
            && (last_behavior == EditorKeyBehavior::Commit || text_action.is_some())
        {
            let text = self.chewing_editor.display_commit().to_owned();
            self.chewing_editor.ack();
            debug!(text; "commit string");
            ui.insert_text(&convert_output(&self.cfg.chewing_tsf, &text))?;
            if let Some(param) = text_action {
                ui.insert_text(&convert_output(&self.cfg.chewing_tsf, &param))?;
            }
            debug!("commit string ok");
            return Ok(true);
        }

        if let Err(error) = self.update_candidates(ui) {
            error!("{}", error.report());
        }

        debug!("updated candidates");

        // A custom phrase comes with the editor cleared, not committed.
        let commit = if last_behavior == EditorKeyBehavior::Commit || text_action.is_some() {
            let mut commit = self.chewing_editor.display_commit().to_owned();
            self.chewing_editor.ack();
            if let Some(param) = text_action {
                commit.push_str(&param);
            }
            commit
        } else {
            String::new()
        };

        self.update_preedit(ui, commit)?;

        if !self.chewing_editor.notification().is_empty() {
            let msg = self.chewing_editor.notification().to_owned();
            self.show_message(ui, &msg, Duration::from_millis(500))?;
        }

        Ok(true)
    }

    pub(crate) fn on_test_keyup(
        &mut self,
        ui: &mut impl Frontend,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let ksym = ev.to_keyboard_event(self.keymap).ksym;
        if self.lang_mode.get().is_disabled() && ksym != SYM_LEFTSHIFT && ksym != SYM_RIGHTSHIFT {
            return Ok(false);
        }
        self.on_keyup(ui, ev)
    }

    pub(crate) fn on_keyup(
        &mut self,
        ui: &mut impl Frontend,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let evt = ev.to_keyboard_event(self.keymap);
        let last_is_shift = evt.ksym == SYM_LEFTSHIFT || evt.ksym == SYM_RIGHTSHIFT;
        let last_is_capslock = evt.ksym == SYM_CAPSLOCK;

        debug!(last_is_shift, last_is_capslock; "");

        if last_is_shift
            && self.shift_key_state.release().is_some_and(|duration| {
                duration < Duration::from_millis(self.cfg.chewing_tsf.shift_key_sensitivity as u64)
            })
            && self.cfg.chewing_tsf.switch_lang_with_shift
        {
            // TODO: simplify this
            if self.cfg.chewing_tsf.enable_caps_lock && !self.lang_mode.get().is_disabled() {
                // Locked by CapsLock
                let msg = match self.lang_mode.get() {
                    TsfLangMode::English => "CapsLock 鎖定英數模式",
                    TsfLangMode::Chinese => "CapsLock 鎖定中文模式",
                    _ => "輸入法關閉中", // unreachable
                };
                if self.cfg.chewing_tsf.show_notification {
                    self.show_message(ui, msg, Duration::from_millis(500))?;
                }
            } else {
                self.toggle_lang_mode(ui)?;
                let msg = match self.lang_mode.get() {
                    TsfLangMode::English => "英數模式",
                    TsfLangMode::Chinese => "中文模式",
                    _ => "輸入法關閉中", // unreachable
                };
                if self.cfg.chewing_tsf.show_notification {
                    self.show_message(ui, msg, Duration::from_millis(500))?;
                }
            }
        }

        if self.cfg.chewing_tsf.enable_caps_lock && last_is_capslock {
            self.sync_lang_mode(ui)?;
            let msg = match self.lang_mode.get() {
                TsfLangMode::English => "英數模式",
                TsfLangMode::Chinese => "中文模式",
                _ => "輸入法關閉中", // unreachable
            };
            if self.cfg.chewing_tsf.show_notification {
                self.show_message(ui, msg, Duration::from_millis(500))?;
            }
        }

        // It is usually harmless to bubble up the keyup event but can be problematic if
        // keyup of a corresponding keydown doesn't match. Shortcut might be stuck, and
        // key repeat might not stop. So we always return `false` and handle keyup in
        // `on_test_keyup`.
        Ok(false)
    }

    /// Shift+Space, when it switches between halfwidth and fullwidth.
    fn is_fullwidth_toggle(&self, evt: &KeyboardEvent) -> bool {
        self.cfg.chewing_tsf.enable_fullwidth_toggle
            && evt.ksym == SYM_SPACE
            && evt.is_state_on(KeyState::Shift)
            && !evt.is_state_on(KeyState::Control)
    }

    /// Fullwidth is for every character, and only while Shift+Space may
    /// switch it back.
    fn apply_character_form(&mut self) {
        self.fullwidth &= self.cfg.chewing_tsf.enable_fullwidth_toggle;
        let form = if self.fullwidth {
            CharacterForm::Fullwidth
        } else {
            CharacterForm::Halfwidth
        };
        self.chewing_editor
            .set_editor_options(|opt| opt.character_form = form);
    }

    /// Follows the keyboard's open/close state, which Ctrl+Space and programs
    /// set: while closed, keys go past the engine, but Shift (`on_keyup`).
    pub(crate) fn set_keyboard_open(&self, open: bool) {
        self.lang_mode.update(|mode| match (mode, open) {
            (TsfLangMode::Chinese, false) => TsfLangMode::DisabledChinese,
            (TsfLangMode::English, false) => TsfLangMode::DisabledEnglish,
            (TsfLangMode::DisabledChinese, true) => TsfLangMode::Chinese,
            (TsfLangMode::DisabledEnglish, true) => TsfLangMode::English,
            (mode, _) => mode,
        });
    }

    /// Forgets what was being typed: the document ended the composition.
    pub(crate) fn on_composition_terminated(&mut self) {
        if self.candidate_list.is_some() {
            self.hide_candidates();
        }
        let editor = &mut self.chewing_editor;
        if editor.is_selecting() {
            let _ = editor.cancel_selecting();
        }
        editor.clear_syllable_editor();
        editor.clear_composition_editor();
    }

    /// Runs a command from the menu or a language bar button.
    pub(crate) fn on_command(&mut self, ui: &mut impl Frontend, id: u32) {
        // A click comes without the keydown that reloads the config, and the
        // toggles below save the whole of it: a stale copy would undo what
        // the settings app saved since this app last had a key.
        if let Err(error) = self.apply_config_if_changed(ui) {
            error!("unable to load config: {error:#}");
        }
        match id {
            ID_SWITCH_LANG => {
                if let Err(error) = self.toggle_lang_mode(ui) {
                    error!("unable to toggle lang mode: {error}");
                }
            }
            ID_MODE_ICON => {
                if let Err(error) = self.toggle_lang_mode(ui) {
                    error!("unable to toggle lang mode: {error}");
                }
            }
            ID_OUTPUT_SIMP_CHINESE => {
                if let Err(error) = self.toggle_simp_chinese(ui) {
                    error!("unable to toggle simplified chinese: {error}");
                }
            }
            ID_OUTPUT_SIMP_VOCABULARY => {
                if let Err(error) = self.toggle_simp_vocabulary(ui) {
                    error!("unable to toggle simplified vocabulary: {error}");
                }
            }
            ID_MOEDICT => open_url("https://www.moedict.tw/"),
            ID_DICT => open_url("https://dict.revised.moe.edu.tw/"),
            ID_SIMPDICT => open_url("https://dict.concised.moe.edu.tw/"),
            ID_LITTLEDICT => open_url("https://dict.mini.moe.edu.tw/"),
            ID_PROVERBDICT => open_url("https://dict.idioms.moe.edu.tw/"),
            ID_CHEWING_HELP => open_url("https://chewing.im/features.html"),
            ID_CONFIG => open_url(&format!("{SETTINGS_SCHEME}://open")),
            ID_USER_DICTIONARY => open_url(&format!("{SETTINGS_SCHEME}://dictionary")),
            _ => {}
        }
    }

    fn update_preedit(&mut self, ui: &mut impl Frontend, commit: String) -> Result<()> {
        let mut composition_buf = String::new();
        let mut segments = vec![];
        let cursor = self.chewing_editor.cursor();
        let bopomofo = self.chewing_editor.syllable_buffer_display();
        let bopomofo_len = bopomofo.chars().count();
        let mut need_push_bopomofo = !bopomofo.is_empty();

        for it in self.chewing_editor.intervals() {
            if (it.start <= cursor && it.end >= cursor) && need_push_bopomofo {
                // Bopomofo splits the segment
                let head_len = cursor - it.start;
                composition_buf.extend(it.text.chars().take(head_len));
                composition_buf.push_str(&bopomofo);
                composition_buf.extend(it.text.chars().skip(head_len));
                if cursor == it.start {
                    segments.push((cursor, cursor + bopomofo_len));
                    segments.push((it.start + bopomofo_len, it.end + bopomofo_len));
                } else if cursor == it.end {
                    segments.push((it.start, it.end));
                    segments.push((cursor, cursor + bopomofo_len));
                } else {
                    segments.push((it.start, cursor));
                    segments.push((cursor, cursor + bopomofo_len));
                    segments.push((cursor + bopomofo_len, it.end));
                }
                need_push_bopomofo = false;
            } else {
                composition_buf.push_str(&it.text);
                if it.start > cursor && !bopomofo.is_empty() {
                    segments.push((it.start + bopomofo_len, it.end + bopomofo_len));
                } else {
                    segments.push((it.start, it.end));
                }
            }
        }
        if need_push_bopomofo {
            segments.push((0, bopomofo_len));
            composition_buf.push_str(&bopomofo);
        }

        let commit = convert_output(&self.cfg.chewing_tsf, &commit);
        // has something in composition buffer
        if !composition_buf.is_empty() {
            let preedit = convert_preedit(&self.cfg.chewing_tsf, &composition_buf);
            ui.set_composition_string(commit, preedit, segments, cursor)?;
        } else {
            // nothing left in composition buffer, terminate composition status
            if self.is_composing(ui.has_composition()) {
                ui.set_composition_string(commit, String::new(), vec![], 0)?;
            }
            // We also need to make sure that the candidate window is not
            // currently shown. When typing symbols with ` key, it's possible
            // that the composition string empty, while the candidate window is
            // shown. We should not terminate the composition in this case.
            if self.candidate_list.is_none() {
                ui.end_composition()?;
            }
        }
        Ok(())
    }

    fn show_message(
        &mut self,
        ui: &impl Frontend,
        text: &str,
        dur: Duration,
    ) -> Result<(), scoped_error::Error> {
        expect_error("Failed to show message", || {
            let hwnd = ui.popup_parent()?;
            let notification = Notification::new(hwnd, ui.thread_mgr())?;
            notification.set_model(NotificationModel {
                text: HSTRING::from(text),
                font_family: HSTRING::from(&self.cfg.chewing_tsf.font_family),
                font_size: self.cfg.chewing_tsf.font_size as f32,
                fg_color: color_s(&self.cfg.chewing_tsf.notify_fg_color),
                bg_color: color_s(&self.cfg.chewing_tsf.notify_bg_color),
                border_color: color_s(&self.cfg.chewing_tsf.notify_border_color),
            });
            let position = match self.candidate_list.as_ref().and_then(|c| c.window_rect()) {
                // Below the candidate list, which is raised over anything in
                // its way whenever it is redrawn.
                Some(rect) => Some((rect.left, rect.bottom)),
                None => ui
                    .caret_rect()
                    .ok()
                    .map(|rect| (rect.left + 50, rect.bottom + 50)),
            };
            if let Some((x, y)) = position {
                notification.set_position(x, y);
                // HACK set position again to use correct DPI setting
                notification.set_position(x, y);
            }
            notification.show();
            notification.set_timer(dur);
            self.notification = Some(notification);
            Ok(())
        })
    }

    pub(crate) fn hide_message(&mut self) {
        if let Some(notification) = self.notification.take() {
            notification.set_timer(Duration::ZERO);
            notification.end_ui_element();
        }
    }

    fn update_candidates(&mut self, ui: &impl Frontend) -> Result<(), scoped_error::Error> {
        expect_error("Failed to refresh candidate window", || {
            if !self.chewing_editor.is_selecting() {
                self.hide_candidates();
                return Ok(());
            }
            if self.candidate_list.is_none() {
                let hwnd = ui.popup_parent()?;
                let candidate_list = CandidateList::new(hwnd, ui.thread_mgr())?;
                self.candidate_list = Some(candidate_list);
            }

            let editor = &self.chewing_editor;
            if let Some(candidate_list) = &self.candidate_list {
                let cfg = &self.cfg.chewing_tsf;
                let sel_keys = SEL_KEYS[cfg.sel_key_type as usize];
                let n = editor.editor_options().candidates_per_page;
                let total_page = editor.total_page()? as u32;
                let current_page = editor.current_page_no()? as u32 + 1;
                let mut items = editor.paginated_candidates()?;
                if total_page == 0 {
                    // TODO: handle this properly in chewing-rs
                    self.chewing_editor.cancel_selecting()?;
                    self.hide_candidates();
                    return Ok(());
                }
                items.truncate(n);
                let (items, indices) =
                    dedup_candidates(items.iter().map(|it| convert_output(cfg, it)));
                self.candidate_indices = indices;
                candidate_list.set_model(Model {
                    items,
                    selkeys: sel_keys.chars().take(n).map(|k| k as u16).collect(),
                    cand_per_row: cfg.cand_per_row as u32,
                    total_page,
                    current_page,
                    font_family: HSTRING::from(&cfg.font_family),
                    font_size: cfg.font_size as f32,
                    fg_color: color_s(&cfg.font_fg_color),
                    bg_color: color_s(&cfg.font_bg_color),
                    highlight_fg_color: color_s(&cfg.font_highlight_fg_color),
                    highlight_bg_color: color_s(&cfg.font_highlight_bg_color),
                    border_color: color_s(&cfg.cand_list_border_color),
                    selkey_color: color_s(&cfg.font_number_fg_color),
                    use_cursor: cfg.cursor_cand_list,
                    current_sel: 0,
                });

                candidate_list.show();

                if let Ok(rect) = ui.caret_rect() {
                    candidate_list.set_position(rect.left, rect.bottom);
                    // HACK set position again to use correct DPI setting
                    candidate_list.set_position(rect.left, rect.bottom);
                }
            }

            Ok(())
        })
    }

    pub(crate) fn hide_candidates(&mut self) {
        if let Some(candidate_list) = self.candidate_list.take() {
            candidate_list.end_ui_element();
        }
    }

    /// Saved to the registry so every app shares one switch: the others pick it
    /// up when they next get focus.
    fn toggle_simp_chinese(&mut self, ui: &impl Frontend) -> Result<()> {
        let cfg = &mut self.cfg.chewing_tsf;
        cfg.output_simp_chinese = !cfg.output_simp_chinese;
        debug!(
            "toggle output simplified chinese: {}",
            cfg.output_simp_chinese
        );
        self.cfg.save_reg();
        ui.update_lang_buttons(self)
    }

    fn toggle_simp_vocabulary(&mut self, ui: &impl Frontend) -> Result<()> {
        let cfg = &mut self.cfg.chewing_tsf;
        cfg.output_simp_vocabulary = !cfg.output_simp_vocabulary;
        self.cfg.save_reg();
        ui.update_lang_buttons(self)
    }

    /// Saved to the registry like the simplified switch, so every app types
    /// the same way; the others pick it up when they next get focus.
    fn toggle_pinyin(&mut self, ui: &impl Frontend) -> Result<()> {
        let cfg = &mut self.cfg.chewing_tsf;
        cfg.pinyin = !cfg.pinyin;
        self.cfg.save_reg();
        self.apply_input_method();
        if self.cfg.chewing_tsf.show_notification {
            let msg = if self.cfg.chewing_tsf.pinyin {
                "拼音"
            } else {
                "注音"
            };
            self.show_message(ui, msg, Duration::from_millis(500))?;
        }
        Ok(())
    }

    /// Sets up the editor to type zhuyin in the configured layout, or pinyin.
    fn apply_input_method(&mut self) {
        let cfg = &self.cfg.chewing_tsf;
        self.kbtype = KeyboardLayoutCompat::try_from(cfg.keyboard_layout as u8)
            .unwrap_or(KeyboardLayoutCompat::Default);
        // Pinyin is spelled with the letters on the keys, whatever the zhuyin
        // layout moves around.
        self.keymap = keymap_from_kbtype(if cfg.pinyin {
            KeyboardLayoutCompat::Default
        } else {
            self.kbtype
        });
        if cfg.simulate_english_layout != 0 {
            let sim = SimulatedKeyboard::from(cfg.simulate_english_layout);
            self.keymap = sim.into();
        }
        let editor = &mut self.chewing_editor;
        // Zhuyin spells each sound on its own key, so it has no use for them.
        set_fuzzy_sounds(if cfg.pinyin {
            cfg.fuzzy_pinyin as u32
        } else {
            0
        });
        if cfg.pinyin {
            editor.set_syllable_editor(match Scheme::from_config(cfg.shuangpin) {
                Some(scheme) => Box::new(Shuangpin::new(scheme)),
                None => Box::new(ContinuousPinyin::default()),
            });
            // Typed without tones, a syllable has to match all of them, and an
            // initial alone every syllable it starts; only this engine does.
            editor.set_editor_options(|opt| {
                opt.conversion_engine = ConversionEngineKind::FuzzyChewingEngine
            });
        } else {
            editor.set_syllable_editor(syl_editor_from_kbtype(self.kbtype));
            editor.set_editor_options(|opt| {
                opt.conversion_engine = match cfg.conv_engine {
                    0 => ConversionEngineKind::SimpleEngine,
                    2 => ConversionEngineKind::FuzzyChewingEngine,
                    _ => ConversionEngineKind::ChewingEngine,
                }
            });
        }
    }

    fn toggle_hsu_keyboard(&mut self, ui: &impl Frontend) -> Result<()> {
        if self.kbtype == KeyboardLayoutCompat::Hsu {
            self.kbtype = KeyboardLayoutCompat::Default;
            self.keymap = keymap_from_kbtype(self.kbtype);
            self.chewing_editor
                .set_syllable_editor(syl_editor_from_kbtype(KeyboardLayoutCompat::Default));
            self.show_message(ui, "標準鍵盤", Duration::from_millis(500))?;
        } else {
            self.kbtype = KeyboardLayoutCompat::Hsu;
            self.keymap = keymap_from_kbtype(self.kbtype);
            self.chewing_editor
                .set_syllable_editor(syl_editor_from_kbtype(KeyboardLayoutCompat::Hsu));
            self.show_message(ui, "許氏鍵盤", Duration::from_millis(500))?;
        }
        Ok(())
    }

    /// Follows CapsLock after the engine changed the language mode itself.
    pub(crate) fn sync_lang_mode(&self, ui: &impl Frontend) -> Result<()> {
        ui.lang_mode_changed();
        self.sync_caps_lock();
        ui.update_lang_buttons(self)
    }

    /// Puts the language mode where CapsLock locks it, if it does.
    pub(crate) fn sync_caps_lock(&self) {
        if !self.lang_mode.get().is_disabled() {
            let cfg = &self.cfg.chewing_tsf;
            let evt = SystemKeyboardEvent::default().to_keyboard_event(self.keymap);
            if cfg.enable_caps_lock {
                let (locked_mode, unlocked_mode) = if cfg.lock_chinese_on_caps_lock {
                    (TsfLangMode::Chinese, TsfLangMode::English)
                } else {
                    (TsfLangMode::English, TsfLangMode::Chinese)
                };
                if evt.is_state_on(KeyState::CapsLock) {
                    self.lang_mode.set(locked_mode);
                } else {
                    self.lang_mode.set(unlocked_mode);
                }
            }
        }
        debug!("new lang_mode={:?}", self.lang_mode.get());
    }

    fn toggle_lang_mode(&mut self, ui: &mut impl Frontend) -> Result<()> {
        let prev = self.lang_mode.get();
        self.lang_mode.update(|v| match v {
            TsfLangMode::English => TsfLangMode::Chinese,
            TsfLangMode::Chinese => TsfLangMode::English,
            // Opens a closed keyboard: switching to another IME and back is
            // no way out when this is the only one.
            TsfLangMode::DisabledEnglish | TsfLangMode::DisabledChinese => TsfLangMode::Chinese,
        });
        self.sync_lang_mode(ui)?;

        if prev != self.lang_mode.get() {
            remember_english(matches!(self.lang_mode.get(), TsfLangMode::English));
            self.chewing_editor.clear_syllable_editor();
            self.update_preedit(ui, String::new())?;
        }

        Ok(())
    }

    /// The language-mode icon: 中, 简 or 英, drawn for the taskbar's theme.
    pub(crate) fn lang_icon(&self, icons: &LangIconSet) -> HICON {
        let icons = match (
            self.lang_mode.get(),
            self.cfg.chewing_tsf.output_simp_chinese,
        ) {
            (TsfLangMode::Chinese, true) => icons.sc,
            (TsfLangMode::Chinese, false) => icons.tc,
            _ => icons.en,
        };
        match ThemeDetector::detect_theme() {
            WindowsTheme::Light | WindowsTheme::Unknown => icons.light,
            WindowsTheme::Dark => icons.dark,
        }
    }

    /// Ticks the menu items of the settings that are on.
    pub(crate) fn check_menu_items(&self, menu: HMENU) {
        let cfg = &self.cfg.chewing_tsf;
        for (id, checked) in [
            (ID_OUTPUT_SIMP_CHINESE, cfg.output_simp_chinese),
            (ID_OUTPUT_SIMP_VOCABULARY, cfg.output_simp_vocabulary),
        ] {
            let flag = if checked { MF_CHECKED } else { MF_UNCHECKED };
            unsafe {
                CheckMenuItem(menu, id, flag.0);
            }
        }
    }

    /// When the candidate window is shown we are composing even without a
    /// composition.
    pub(crate) fn is_composing(&self, has_composition: bool) -> bool {
        has_composition || self.candidate_list.is_some()
    }

    fn init_chewing_context(&mut self, ui: &mut impl Frontend) -> Result<()> {
        self.apply_init_config(ui)?;
        self.sync_lang_mode(ui)?;
        Ok(())
    }

    fn build_editor_from_cfg(cfg: &ChewingTsfConfig) -> Result<Editor> {
        // Recreate editor to load latest user files
        let mut editor = new_editor()?;
        editor.set_editor_options(|opt| {
            opt.easy_symbol_input = cfg.easy_symbols_with_shift || cfg.easy_symbols_with_shift_ctrl;
            // NB: Historically the config was inverted
            opt.user_phrase_add_dir = if cfg.add_phrase_forward {
                UserPhraseAddDirection::Backward
            } else {
                UserPhraseAddDirection::Forward
            };
            opt.phrase_choice_rearward = cfg.phrase_choice_rearward;
            opt.auto_shift_cursor = cfg.advance_after_selection;
            opt.candidates_per_page = cfg.cand_per_page as usize;
            opt.esc_clear_all_buffer = cfg.esc_clean_all_buf;
            opt.space_is_select_key = cfg.show_cand_with_space_key;
            opt.disable_auto_learn_phrase = !cfg.enable_auto_learn;
            // on_keydown switches it (apply_character_form): chewing's key
            // works only between syllables, and the form would go with the
            // editor, rebuilt on every focus.
            opt.enable_fullwidth_toggle_key = false;
            opt.sort_candidates_by_frequency = cfg.sort_candidates_by_frequency;
            // Set here, not once at init: every config change and focus
            // rebuilds the editor, which would fall back to chewing's 39.
            opt.auto_commit_threshold = 50;
            // TODO experimental
            opt.auto_snapshot_selections = true;
        });
        Ok(editor)
    }

    fn apply_config_if_changed(&mut self, ui: &impl Frontend) -> Result<()> {
        if self.cfg.reload_if_needed()? {
            self.apply_runtime_config(ui)?;
        }
        Ok(())
    }

    /// Initializes the config to the user default
    fn apply_init_config(&mut self, ui: &impl Frontend) -> Result<()> {
        let english = remembered_english().unwrap_or(self.cfg.chewing_tsf.default_english);
        self.lang_mode.set(if english {
            TsfLangMode::English
        } else {
            TsfLangMode::Chinese
        });
        self.apply_runtime_config(ui)?;
        Ok(())
    }

    /// Applys config changes that should be effective at runtime
    pub(crate) fn apply_runtime_config(&mut self, ui: &impl Frontend) -> Result<()> {
        self.chewing_editor = Self::build_editor_from_cfg(&self.cfg.chewing_tsf)?;
        self.apply_character_form();
        self.phrases = load_phrases();
        self.apply_input_method();
        let _ = ui.update_lang_buttons(self);
        let keybindings = self
            .cfg
            .chewing_tsf
            .keybind
            .iter()
            .filter_map(|kb| Keybinding::try_from(kb).ok())
            .collect();
        self.keybindings = keybindings;
        Ok(())
    }

    /// The custom phrase for Space after the letters typed into an empty
    /// composition. Any other key ends those letters, but Backspace may take
    /// back one still being spelled.
    fn custom_phrase(&mut self, evt: &KeyboardEvent) -> Option<String> {
        let typed = self.typed.take();
        if evt.has_modifiers()
            || !matches!(self.lang_mode.get(), TsfLangMode::Chinese)
            || self.chewing_editor.is_selecting()
        {
            return None;
        }
        match evt.ksym {
            SYM_SPACE => self.phrases.get(&typed?).cloned(),
            SYM_BACKSPACE if self.chewing_editor.entering_syllable() => {
                self.typed = typed.map(|mut it| {
                    it.pop();
                    it
                });
                None
            }
            ksym if ksym.is_atoz() => {
                let empty =
                    self.chewing_editor.is_empty() && !self.chewing_editor.entering_syllable();
                self.typed = if empty { Some(String::new()) } else { typed }.map(|mut it| {
                    it.push(ksym.to_unicode().to_ascii_lowercase());
                    it
                });
                None
            }
            _ => None,
        }
    }

    /// Maps a selection key to the digit chewing selects with. Plain digits
    /// count as slots too, since chewing would otherwise select by them
    /// directly. `None` means nothing is shown in that slot.
    fn map_sel_key(&self, mut evt: KeyboardEvent) -> Option<KeyboardEvent> {
        let key = evt.ksym.to_unicode();
        let slot = SEL_KEYS[self.cfg.chewing_tsf.sel_key_type as usize]
            .chars()
            .position(|it| it == key)
            .or_else(|| key.to_digit(10).map(|digit| (digit as usize + 9) % 10));
        let Some(slot) = slot else {
            return Some(evt);
        };
        match *self.candidate_indices.get(slot)? {
            idx @ 0..9 => {
                evt.code = Keycode(keycode::KEY_1.0 + idx as u8);
                evt.ksym = Keysym(keysym::SYM_1.0 + idx as u32);
            }
            _ => {
                evt.code = keycode::KEY_0;
                evt.ksym = keysym::SYM_0;
            }
        }
        Some(evt)
    }
}

/// The custom phrases the settings app saved. It checks them first, so a line
/// that isn't a phrase was edited in by hand; that gives none rather than some.
fn load_phrases() -> HashMap<String, String> {
    // new_editor has failed already without the user dir.
    let Ok(path) = user_dir().map(|dir| dir.join(PHRASES_FILE)) else {
        return HashMap::new();
    };
    let text = fs::read_to_string(path).unwrap_or_else(|error| {
        // No file is no phrases saved yet.
        if error.kind() != ErrorKind::NotFound {
            error!("unable to read {PHRASES_FILE}: {error}");
        }
        String::new()
    });
    phrases::parse(&text).unwrap_or_else(|error| {
        error!("{PHRASES_FILE}: {error}");
        HashMap::new()
    })
}

/// The Chinese/English mode each program was last switched to, by executable
/// name, so that one opened again starts there (the engine lives only as long
/// as the program). Kept out of Config, which the settings app saves whole.
fn app_modes_key() -> String {
    format!(r"Software\{PRODUCT_NAME}\AppModes")
}

fn app_name() -> Option<String> {
    Some(env::current_exe().ok()?.file_name()?.to_str()?.to_lowercase())
}

fn remembered_english() -> Option<bool> {
    let key = CURRENT_USER.open(app_modes_key()).ok()?;
    Some(key.get_u32(app_name()?).ok()? != 0)
}

fn remember_english(english: bool) {
    // Store apps and low-integrity programs can't write there; they only
    // start in the default mode next time.
    if let (Ok(key), Some(name)) = (CURRENT_USER.create(app_modes_key()), app_name()) {
        let _ = key.set_u32(name, english.into());
    }
}

fn new_editor() -> Result<Editor> {
    let dictionary_dir = dictionary_dir()?;
    let user_dir = user_dir()?;
    // Done here rather than by tsfreg, whose elevated %AppData% may belong to
    // another account. Once per process: the editor is rebuilt on every focus.
    static SHARE_USER_DIR: Once = Once::new();
    SHARE_USER_DIR.call_once(|| {
        // Always fails inside an AppContainer; the next desktop app sets it up.
        if let Err(error) = share_user_dir(&user_dir) {
            info!("{}", error.report());
        }
    });
    Ok(Editor::chewing(
        Some(dictionary_dir.to_string_lossy().into_owned()),
        Some(user_dir.to_string_lossy().into_owned()),
    )?)
}

/// The package keeps the dictionary beside the per-architecture DLL folders
/// (`<root>\x64\chewing_tip.dll`, `<root>\x86\...`, `<root>\Dictionary`), so
/// it is found wherever the package is installed.
fn dictionary_dir() -> Result<PathBuf> {
    let module = HMODULE(G_HINSTANCE.load(Ordering::Relaxed) as *mut c_void);
    let mut buf = vec![0u16; 32768];
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    if len == 0 {
        bail!("unable to locate chewing_tip.dll");
    }
    let dll_path = PathBuf::from(OsString::from_wide(&buf[..len]));
    let root = dll_path
        .parent()
        .and_then(Path::parent)
        .context("chewing_tip.dll is not inside an architecture folder")?;
    Ok(root.join("Dictionary"))
}

/// Converts text leaving the IME: candidates, the composition and commits.
pub(crate) fn convert_output(cfg: &ChewingTsfConfig, text: &str) -> String {
    match (cfg.output_simp_chinese, cfg.output_simp_vocabulary) {
        (false, _) => text.to_owned(),
        (true, false) => zhconv(text, Variant::ZhHans),
        (true, true) => zhconv(text, Variant::ZhCN),
    }
}

/// Segments and the cursor are character offsets into the unconverted text,
/// so a vocabulary swap that changes the length only gets the script
/// conversion while composing; the commit still gets the full one.
fn convert_preedit(cfg: &ChewingTsfConfig, text: &str) -> String {
    let converted = convert_output(cfg, text);
    if converted.chars().count() == text.chars().count() {
        converted
    } else {
        zhconv(text, Variant::ZhHans)
    }
}

/// Converting can map several candidates to the same text (體 and 体 both
/// become 体). Keeps the first, higher ranked, of each and returns the editor
/// index every shown item stands for.
fn dedup_candidates(items: impl IntoIterator<Item = String>) -> (Vec<String>, Vec<usize>) {
    let mut shown: Vec<String> = vec![];
    let mut indices = vec![];
    for (index, item) in items.into_iter().enumerate() {
        if !shown.contains(&item) {
            shown.push(item);
            indices.push(index);
        }
    }
    (shown, indices)
}

fn syl_editor_from_kbtype(kbtype: KeyboardLayoutCompat) -> Box<dyn SyllableEditor> {
    use zhuyin_layout::*;
    match kbtype {
        KeyboardLayoutCompat::Default => Box::new(Standard::new()),
        KeyboardLayoutCompat::Hsu => Box::new(Hsu::new()),
        KeyboardLayoutCompat::Ibm => Box::new(Ibm::new()),
        KeyboardLayoutCompat::GinYieh => Box::new(GinYieh::new()),
        KeyboardLayoutCompat::Et => Box::new(Et::new()),
        KeyboardLayoutCompat::Et26 => Box::new(Et26::new()),
        KeyboardLayoutCompat::Dvorak => Box::new(Standard::new()),
        KeyboardLayoutCompat::DvorakHsu => Box::new(Hsu::new()),
        KeyboardLayoutCompat::DachenCp26 => Box::new(DaiChien26::new()),
        KeyboardLayoutCompat::HanyuPinyin => Box::new(Pinyin::hanyu()),
        KeyboardLayoutCompat::ThlPinyin => Box::new(Pinyin::thl()),
        KeyboardLayoutCompat::Mps2Pinyin => Box::new(Pinyin::mps2()),
        KeyboardLayoutCompat::Carpalx
        | KeyboardLayoutCompat::ColemakDhAnsi
        | KeyboardLayoutCompat::ColemakDhOrth
        | KeyboardLayoutCompat::Workman
        | KeyboardLayoutCompat::Colemak => Box::new(Standard::new()),
    }
}

fn keymap_from_kbtype(kbtype: KeyboardLayoutCompat) -> KeymapOp {
    match kbtype {
        KeyboardLayoutCompat::Dvorak => KeymapOp::Ksym(&INVERTED_DVORAK_MAP),
        KeyboardLayoutCompat::DvorakHsu => KeymapOp::Ksym(&DVORAK_MAP),
        KeyboardLayoutCompat::Carpalx => KeymapOp::Ksym(&INVERTED_QGMLWY_MAP),
        KeyboardLayoutCompat::ColemakDhAnsi => KeymapOp::Ksym(&INVERTED_COLEMAK_DH_ANSI_MAP),
        KeyboardLayoutCompat::ColemakDhOrth => KeymapOp::Ksym(&INVERTED_COLEMAK_DH_ORTH_MAP),
        KeyboardLayoutCompat::Workman => KeymapOp::Ksym(&INVERTED_WORKMAN_MAP),
        KeyboardLayoutCompat::Colemak => KeymapOp::Ksym(&INVERTED_COLEMAK_MAP),
        _ => KeymapOp::None,
    }
}

/// A tone key typed right after a character re-types that character with the
/// new tone: 嗎 then ˇ gives 馬. Chewing would otherwise start a new syllable
/// with the tone alone. Returns the syllable the character had.
fn change_tone(
    editor: &mut Editor,
    kbtype: KeyboardLayoutCompat,
    evt: KeyboardEvent,
) -> Option<Syllable> {
    if evt.has_modifiers() {
        return None;
    }
    let tone = if evt.ksym == SYM_SPACE {
        // Space is tone 1 in every layout, but they only read it to end a
        // syllable, so the probe below can't see it.
        if editor.editor_options().space_is_select_key {
            return None;
        }
        None
    } else {
        // Asking the layout keeps this right for every layout, including those
        // where the tone keys also type consonants, such as Hsu.
        let mut probe = syl_editor_from_kbtype(kbtype);
        probe.key_press(evt);
        let typed = probe.read();
        if typed.has_initial() || typed.has_medial() || typed.has_rime() {
            return None;
        }
        Some(typed.tone()?)
    };
    retone(editor, kbtype, tone)
}

/// Re-types the character before the cursor with `tone`, None being tone 1,
/// which chewing spells without a tone. Returns the syllable it had.
fn retone(
    editor: &mut Editor,
    kbtype: KeyboardLayoutCompat,
    tone: Option<Bopomofo>,
) -> Option<Syllable> {
    if editor.editor_options().language_mode != LanguageMode::Chinese
        || !editor.is_entering()
        || editor.entering_syllable()
        || editor.cursor() == 0
    {
        return None;
    }
    let old = editor.symbols()[editor.cursor() - 1].to_syllable()?;
    let mut new = old;
    match tone {
        Some(tone) => new.update(tone),
        None => {
            new.remove_tone();
        }
    }
    let len = editor.len();
    editor.process_keyevent(
        KeyboardEvent::builder()
            .code(keycode::KEY_BACKSPACE)
            .ksym(keysym::SYM_BACKSPACE)
            .build(),
    );
    insert_syllable(editor, kbtype, new);
    // Chewing drops a syllable no word is spelled with; keep the old one then.
    if editor.len() < len {
        insert_syllable(editor, kbtype, old);
    }
    Some(old)
}

fn insert_syllable(editor: &mut Editor, kbtype: KeyboardLayoutCompat, syllable: Syllable) {
    editor.set_syllable_editor(Box::new(PresetSyllable {
        syllable,
        started: false,
    }));
    let key = KeyboardEvent::builder()
        .code(keycode::KEY_A)
        .ksym(keysym::SYM_LOWER_A)
        .build();
    editor.process_keyevent(key);
    editor.process_keyevent(key);
    editor.set_syllable_editor(syl_editor_from_kbtype(kbtype));
}

/// Hands chewing one ready-made syllable, so a syllable can be put into the
/// composition without knowing which keys type it in the current layout.
/// Chewing starts a syllable when a key is absorbed and inserts it when a key
/// commits, so the first key press absorbs and the second commits.
#[derive(Debug, Clone, Copy)]
struct PresetSyllable {
    syllable: Syllable,
    started: bool,
}

impl std::fmt::Display for PresetSyllable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PresetSyllable")
    }
}

impl SyllableEditor for PresetSyllable {
    fn key_press(&mut self, _key: KeyboardEvent) -> KeyBehavior {
        if mem::replace(&mut self.started, true) {
            KeyBehavior::Commit
        } else {
            KeyBehavior::Absorb
        }
    }
    fn remove_last(&mut self) {
        self.syllable.pop();
    }
    fn clear(&mut self) {
        self.syllable.clear();
    }
    fn is_empty(&self) -> bool {
        self.syllable.is_empty()
    }
    fn read(&self) -> Syllable {
        self.syllable
    }
    fn clone(&self) -> Box<dyn SyllableEditor> {
        Box::new(*self)
    }
}

#[cfg(test)]
mod tests {
    use chewing::dictionary::StringTableBuilder;
    use chewing::editor::zhuyin_layout::KeyboardLayoutCompat;
    use chewing::editor::{BasicEditor, EditorBuilder};
    use chewing::input::keymap::{QWERTY_MAP, map_ascii};
    use chewing::lm::StaticDictBuilder;
    use chewing::syl;
    use chewing::zhuyin::Bopomofo as bpmf;
    use chewing_tip_core::config::ChewingTsfConfig;

    use super::{change_tone, convert_output, convert_preedit, dedup_candidates, retone};

    fn simplified(vocabulary: bool) -> ChewingTsfConfig {
        ChewingTsfConfig {
            output_simp_chinese: true,
            output_simp_vocabulary: vocabulary,
            ..Default::default()
        }
    }

    #[test]
    fn selecting_a_slot_skips_hidden_duplicates() {
        let cfg = simplified(false);
        let (shown, indices) =
            dedup_candidates(["體", "体", "綈"].map(|it| convert_output(&cfg, it)));
        assert_eq!(shown, ["体", "绨"]);
        // The second slot shows 绨, so it must select 綈, not the hidden 体.
        assert_eq!(indices, [0, 2]);
    }

    #[test]
    fn preedit_keeps_its_length_for_segment_offsets() {
        let cfg = simplified(true);
        assert_eq!(convert_output(&cfg, "網際網路"), "互联网");
        assert_eq!(convert_preedit(&cfg, "網際網路"), "网际网路");
        assert_eq!(convert_preedit(&cfg, "軟體"), "软件");
    }

    #[test]
    fn tone_key_retypes_the_previous_character() {
        let mut strings = StringTableBuilder::new();
        strings.insert("嗎");
        strings.insert("馬");
        strings.insert("媽");
        let strings = strings.build();
        let mut dict = StaticDictBuilder::new();
        dict.insert(
            &[syl![bpmf::M, bpmf::A, bpmf::TONE5]],
            strings.get_wid("嗎").unwrap(),
        );
        dict.insert(
            &[syl![bpmf::M, bpmf::A, bpmf::TONE3]],
            strings.get_wid("馬").unwrap(),
        );
        dict.insert(&[syl![bpmf::M, bpmf::A]], strings.get_wid("媽").unwrap());
        let mut editor = EditorBuilder::new()
            .string_table(strings)
            .static_dict(dict.build())
            .build();
        let key = |ascii| map_ascii(&QWERTY_MAP, ascii);
        for ascii in *b"a87" {
            editor.process_keyevent(key(ascii));
        }
        assert_eq!(editor.display(), "嗎");

        let tone3 = syl![bpmf::M, bpmf::A, bpmf::TONE3];
        let default = KeyboardLayoutCompat::Default;
        assert!(change_tone(&mut editor, default, key(b'3')).is_some());
        assert_eq!(editor.display(), "馬");
        // No word here is ㄇㄚˋ; losing the character would be worse than
        // ignoring the key.
        assert_eq!(change_tone(&mut editor, default, key(b'4')), Some(tone3));
        assert_eq!(editor.display(), "馬");
        // D is ˊ in Hsu only after a rime; alone it starts ㄉ, so it must
        // begin the next character.
        assert!(change_tone(&mut editor, KeyboardLayoutCompat::Hsu, key(b'd')).is_none());
        assert_eq!(editor.display(), "馬");

        // Space is tone 1, and what it returns lets a second Space undo it.
        let old = change_tone(&mut editor, default, key(b' '));
        assert_eq!(old, Some(tone3));
        assert_eq!(editor.display(), "媽");
        retone(&mut editor, default, old.unwrap().tone());
        assert_eq!(editor.display(), "馬");
        // Someone who set Space to open the candidate list still gets that.
        editor.set_editor_options(|opt| opt.space_is_select_key = true);
        assert!(change_tone(&mut editor, default, key(b' ')).is_none());
        assert_eq!(editor.display(), "馬");
    }
}
