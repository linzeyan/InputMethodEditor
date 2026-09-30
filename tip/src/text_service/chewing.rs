// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::ffi::c_void;
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::atomic::Ordering;

use anyhow::{Context, Result, bail};
use log::{debug, error, info};
use scoped_error::expect_error;
use scoped_error::impl_context_error;
use windows::Win32::Foundation::{GetLastError, HINSTANCE, HWND, POINT, RECT};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::TextServices::{
    GUID_COMPARTMENT_EMPTYCONTEXT, GUID_COMPARTMENT_KEYBOARD_DISABLED,
    GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, GUID_LBI_INPUTMODE, ITfCompartmentMgr, ITfCompositionSink,
    ITfContext, TF_ATTR_INPUT, TF_DISPLAYATTRIBUTE, TF_ES_ASYNC, TF_ES_READ, TF_ES_READWRITE,
    TF_ES_SYNC, TF_LBI_STYLE_BTN_BUTTON, TF_LBI_STYLE_BTN_MENU, TF_LS_DOT, TF_LS_SOLID,
    TF_SD_READONLY,
};
use windows::Win32::UI::TextServices::{
    ITfComposition, ITfLangBarItemButton, ITfLangBarItemMgr, ITfThreadMgr,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, HMENU, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_LEFTBUTTON, TPM_NONOTIFY,
    TPM_RETURNCMD, TrackPopupMenu,
};
use windows_core::{ComObject, ComObjectInner, GUID, Interface};

use super::CommandType;
use super::GUID_INPUT_DISPLAY_ATTRIBUTE_1;
use super::GUID_INPUT_DISPLAY_ATTRIBUTE_2;
use super::display_attribute::register_display_attribute;
use super::edit_session::InsertText;
use super::edit_session::{EndComposition, SelectionRect, SetCompositionString};
use super::lang_bar::LangBarButton;
use super::menu::Menu;
use super::resources::*;
use super::theme::ThemeDetector;
use super::ui_elements::{CandidateList, Notification};
use crate::com::G_HINSTANCE;
use crate::engine::key_event::SystemKeyboardEvent;
use crate::engine::{Engine, Frontend, TsfLangMode, convert_output};
use crate::text_service::TextService;
use crate::text_service::edit_session::request_edit_session;
use crate::text_service::icons::LangIconSet;
use crate::text_service::lang_bar::LangBarFactory;

const GUID_MODE_BUTTON: GUID = GUID::from_u128(0xD7F58996_ED24_4B6C_AAC6_2499672B9902);
const GUID_SETTINGS_BUTTON: GUID = GUID::from_u128(0xFE45B830_ABF1_4BAA_9E25_1BCD2AA164E4);

pub(crate) const CLSID_TEXT_SERVICE: GUID = GUID::from_u128(0xE0C45601_7E8F_4FEF_9871_8B0C785B9B48);

impl_context_error!(TsfError);

pub(super) struct CompositionString {
    pub(super) commit: String,
    pub(super) preedit: String,
    pub(super) segments: Vec<(usize, usize)>,
    pub(super) cursor: usize,
}

pub(super) struct ChewingTextService {
    engine: Engine,
    ui: TsfUi,
}

/// The TSF side: the document's composition and the language bar.
struct TsfUi {
    thread_mgr: ITfThreadMgr,
    tid: u32,
    input_da_atom: [VARIANT; 2],
    _menu: Menu,
    popup_menu: HMENU,
    lang_icons: LangIconSet,
    lang_bar_buttons: Vec<ITfLangBarItemButton>,
    composition_sink: ITfCompositionSink,

    switch_lang_button: ComObject<LangBarButton>,
    ime_mode_button: ComObject<LangBarButton>,

    pending_lang_mode_change: Cell<bool>,

    has_focus: bool,
    composition: Rc<RefCell<Option<ITfComposition>>>,
    pending_edit: Weak<RefCell<Option<CompositionString>>>,
}

/// What the engine types into: [`TsfUi`] and the document of the key being
/// handled.
struct Tsf<'a> {
    ui: &'a mut TsfUi,
    /// None outside key events; the focused document then.
    context: Option<ITfContext>,
}

impl ChewingTextService {
    pub(super) fn new(
        thread_mgr: ITfThreadMgr,
        tid: u32,
        ts: ComObject<TextService>,
    ) -> Result<ChewingTextService> {
        let da = TF_DISPLAYATTRIBUTE {
            lsStyle: TF_LS_DOT,
            bAttr: TF_ATTR_INPUT,
            ..Default::default()
        };
        let input_da_atom_1 = register_display_attribute(&GUID_INPUT_DISPLAY_ATTRIBUTE_1, da)?;
        let da = TF_DISPLAYATTRIBUTE {
            lsStyle: TF_LS_SOLID,
            bAttr: TF_ATTR_INPUT,
            ..Default::default()
        };
        let input_da_atom_2 = register_display_attribute(&GUID_INPUT_DISPLAY_ATTRIBUTE_2, da)?;

        let g_hinstance = HINSTANCE(G_HINSTANCE.load(Ordering::Relaxed) as *mut c_void);
        let menu = Menu::load(g_hinstance, IDR_MENU);

        CandidateList::window_register_class(g_hinstance);
        Notification::window_register_class(g_hinstance);

        let lang_bar_item_mgr: ITfLangBarItemMgr = thread_mgr.cast()?;
        info!("Detected theme info: {:?}", ThemeDetector::get_theme_info());

        // Create a small factory to reduce repetition when creating the langbar buttons.
        let popup_menu = menu.sub_menu(0);
        let factory = LangBarFactory::new(
            g_hinstance,
            lang_bar_item_mgr.clone(),
            thread_mgr.clone(),
            popup_menu,
        );

        let switch_lang_button = factory.create_button(
            GUID_MODE_BUTTON,
            TF_LBI_STYLE_BTN_BUTTON,
            IDS_SWITCH_LANG,
            IDI_CHI,
            HMENU::default(),
            ID_SWITCH_LANG,
        )?;

        info!("Add button for settings and others, may open a popup menu");
        let settings_button = factory.create_button(
            GUID_SETTINGS_BUTTON,
            TF_LBI_STYLE_BTN_MENU,
            IDS_SETTINGS,
            IDI_CONFIG,
            popup_menu,
            0,
        )?;

        // Windows 8 systray IME mode icon
        info!("Add systray IME mode icon to switch Chinese/English modes");
        let ime_mode_button = factory.create_button(
            GUID_LBI_INPUTMODE,
            TF_LBI_STYLE_BTN_BUTTON,
            IDS_SWITCH_LANG,
            IDI_CHI,
            HMENU::default(),
            ID_MODE_ICON,
        )?;

        let lang_bar_buttons = vec![
            switch_lang_button.cast()?,
            settings_button.cast()?,
            ime_mode_button.cast()?,
        ];

        let mut ui = TsfUi {
            thread_mgr,
            tid,
            composition_sink: ts.cast()?,
            input_da_atom: [input_da_atom_1, input_da_atom_2],
            _menu: menu,
            popup_menu,
            lang_icons: LangIconSet::load(),
            has_focus: true,
            lang_bar_buttons,
            switch_lang_button,
            ime_mode_button,
            composition: Default::default(),
            pending_edit: Weak::new(),
            pending_lang_mode_change: Cell::new(false),
        };

        if let Err(error) = ui.init_openclose(tid) {
            error!("unable to initialize openclose: {error:#}");
        }

        let engine = Engine::new(&mut Tsf {
            ui: &mut ui,
            context: None,
        })?;

        Ok(ChewingTextService { engine, ui })
    }

    pub(super) fn deactivate(mut self) -> ITfThreadMgr {
        if let Ok(lang_bar_item_mgr) = self.ui.thread_mgr.cast::<ITfLangBarItemMgr>() {
            for button in self.ui.lang_bar_buttons.drain(0..) {
                if let Err(error) = unsafe { lang_bar_item_mgr.RemoveItem(&button) } {
                    error!("unable to remove lang bar item: {error}");
                }
            }
        }
        // TSF doc: The corresponding ITfTextInputProcessor::Deactivate
        // method that shuts down the text service must release all references
        // to the ptim parameter.
        self.ui.thread_mgr
    }

    pub(super) fn on_kill_focus(&mut self, context: Option<ITfContext>) -> Result<()> {
        debug!("on_kill_focus");
        self.ui.has_focus = false;
        if self.engine.is_composing(self.ui.has_composition())
            && let Some(context) = context
        {
            self.ui.end_composition(&context)?;
        }
        self.engine.hide_candidates();
        self.engine.hide_message();
        Ok(())
    }

    pub(super) fn on_focus(&mut self) -> Result<()> {
        debug!("on_focus");
        self.ui.has_focus = true;
        Ok(())
    }

    pub(super) fn on_thread_focus(&mut self) -> Result<()> {
        let _ = self.engine.cfg.reload_if_needed();
        let ui = Tsf {
            ui: &mut self.ui,
            context: None,
        };
        self.engine.apply_runtime_config(&ui)?;
        self.sync_lang_mode(true)?;
        Ok(())
    }

    pub(super) fn on_test_keydown(
        &mut self,
        context: &ITfContext,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let mut ui = Tsf {
            ui: &mut self.ui,
            context: Some(context.clone()),
        };
        self.engine.on_test_keydown(&mut ui, ev)
    }

    pub(super) fn on_keydown(
        &mut self,
        context: &ITfContext,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let mut ui = Tsf {
            ui: &mut self.ui,
            context: Some(context.clone()),
        };
        self.engine.on_keydown(&mut ui, ev)
    }

    pub(super) fn on_test_keyup(
        &mut self,
        context: &ITfContext,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let mut ui = Tsf {
            ui: &mut self.ui,
            context: Some(context.clone()),
        };
        self.engine.on_test_keyup(&mut ui, ev)
    }

    pub(super) fn on_keyup(
        &mut self,
        context: &ITfContext,
        ev: SystemKeyboardEvent,
    ) -> Result<bool> {
        let mut ui = Tsf {
            ui: &mut self.ui,
            context: Some(context.clone()),
        };
        self.engine.on_keyup(&mut ui, ev)
    }

    pub(super) fn on_composition_terminated(
        &mut self,
        ecwrite: u32,
        composition: &ITfComposition,
    ) -> Result<()> {
        unsafe {
            let composition_range = composition
                .GetRange()
                .inspect_err(|_| debug!("failed to get composition range"))?;
            let doc_mgr = self
                .ui
                .thread_mgr
                .GetFocus()
                .context("failed to get current ITfDocumentMgr")?;
            let context = doc_mgr
                .GetTop()
                .context("failed to get current ITfContext")?;

            // When a composition is interrupted by the application we only need to
            // clear the display attributes. When I tested this, clearing the display attribute
            // is not necessary, but this is what mozc was doing.
            //
            // In pure TSF mode, the composition string is automatically committed on termination.
            // In TSF/IMM32 bridge mode, the bridge sets a default property which we override to
            // have the same commit on unselect behavior. See [`TextService_Impl::Activate`].
            let disp_attr_prop =
                context.GetProperty(&windows::Win32::UI::TextServices::GUID_PROP_ATTRIBUTE)?;
            disp_attr_prop
                .Clear(ecwrite, &composition_range)
                .inspect_err(|_| debug!("failed to clear display attribute"))?;
        }
        self.on_composition_terminated_tail()
    }

    // SAFETY: this method must not cause TSF callback reentrant
    pub(super) fn on_composition_terminated_tail(&mut self) -> Result<()> {
        debug!(has_focus=self.ui.has_focus; "on_composition_terminated_tail");
        self.engine.on_composition_terminated();
        if let Some(cell) = self.ui.pending_edit.upgrade() {
            debug!("Clear pending edits to avoid double commits");
            cell.replace(None);
        }
        self.ui.pending_edit = Weak::new();
        self.ui.composition.replace(None);
        Ok(())
    }

    pub(super) fn on_compartment_change_ro(&self, guid: &GUID) -> Result<()> {
        debug!(has_focus=self.ui.has_focus; "on_compartment_change_ro");
        // Not our own change from sync_keyboard_openclose: Ctrl+Space or the
        // program, starting a sync_lang_mode cycle.
        if guid == &GUID_COMPARTMENT_KEYBOARD_OPENCLOSE && !self.ui.pending_lang_mode_change.take()
        {
            self.engine.set_keyboard_open(self.ui.keyboard_open()?);
            self.sync_lang_mode(false)?;
        }
        Ok(())
    }

    pub(super) fn on_compartment_change(&mut self, guid: &GUID) -> Result<()> {
        debug!(has_focus=self.ui.has_focus; "on_compartment_change");
        // Taken as it is, whenever it comes: flipping on each change, or
        // skipping one while unfocused, left the keyboard closed for good
        // once a change was missed or repeated.
        if guid == &GUID_COMPARTMENT_KEYBOARD_OPENCLOSE {
            self.engine.set_keyboard_open(self.ui.keyboard_open()?);
            self.sync_lang_mode(false)?;
            if self.engine.is_composing(self.ui.has_composition())
                && self.engine.lang_mode.get().is_disabled()
            {
                let editor = &mut self.engine.chewing_editor;
                editor.commit()?;
                // Not via process_keyevent, so not written yet.
                editor.flush();
                let commit = editor.display_commit().to_owned();
                editor.ack();
                debug!(commit; "commit string");
                let commit = convert_output(&self.engine.cfg.chewing_tsf, &commit);
                unsafe {
                    let doc_mgr = self
                        .ui
                        .thread_mgr
                        .GetFocus()
                        .context("failed to get current ITfDocumentMgr")?;
                    let context = doc_mgr
                        .GetTop()
                        .context("failed to get current ITfContext")?;
                    self.ui
                        .set_composition_string(&context, commit, String::new(), vec![], 0)?;
                    self.ui.end_composition(&context)?;
                }
                debug!("commit string ok");
            }
        }
        Ok(())
    }

    pub(super) fn on_command(&mut self, id: u32, cmd_type: CommandType) {
        if matches!(cmd_type, CommandType::RightClick) {
            if id == ID_MODE_ICON {
                let mut pos = POINT::default();
                let ret = unsafe {
                    let _ = GetCursorPos(&mut pos);
                    TrackPopupMenu(
                        self.ui.popup_menu,
                        TPM_NONOTIFY
                            | TPM_RETURNCMD
                            | TPM_LEFTALIGN
                            | TPM_BOTTOMALIGN
                            | TPM_LEFTBUTTON,
                        pos.x,
                        pos.y,
                        None,
                        GetFocus(),
                        None,
                    )
                };
                if ret.as_bool() {
                    self.on_command(ret.0 as u32, CommandType::Menu);
                } else {
                    let last_error = unsafe { GetLastError() };
                    let hresult = last_error.to_hresult();
                    error!("unable to open popup menu: {}", hresult.message());
                }
            }
        } else {
            let mut ui = Tsf {
                ui: &mut self.ui,
                context: None,
            };
            if matches!(cmd_type, CommandType::Candidate) {
                if let Err(error) = self.engine.select_candidate(&mut ui, id as usize) {
                    error!("unable to select candidate {id}: {error:#}");
                }
            } else {
                self.engine.on_command(&mut ui, id);
            }
        }
    }

    /// Follows a change of the language mode; `internal` if we made it, so
    /// the compartment change it causes is not taken for the user's.
    fn sync_lang_mode(&self, internal: bool) -> Result<()> {
        debug!("set pending_lang_mode_change to {internal}");
        self.ui.pending_lang_mode_change.set(internal);
        self.engine.sync_caps_lock();
        self.ui.update_lang_buttons(&self.engine)
    }

    pub(crate) fn should_sync_keyboard_openclose(&self) -> bool {
        self.engine.cfg.chewing_tsf.sync_lang_mode_openclose
    }
}

impl TsfUi {
    fn has_composition(&self) -> bool {
        self.composition.borrow().is_some()
    }

    fn is_context_mutable(&self, context: &ITfContext) -> Result<bool, TsfError> {
        expect_error("Failed to query ITfContext status", || {
            let status = unsafe { context.GetStatus()? };
            if status.dwDynamicFlags & TF_SD_READONLY != 0 {
                debug!("key not handled - readonly document");
                return Ok(false);
            }
            let compartment_mgr: ITfCompartmentMgr = context.cast()?;
            unsafe {
                let empty_context =
                    compartment_mgr.GetCompartment(&GUID_COMPARTMENT_EMPTYCONTEXT)?;
                let value = i32::try_from(&empty_context.GetValue()?)?;
                if value == 1 {
                    debug!("key not handled - empty context");
                    return Ok(false);
                }

                let disabled =
                    compartment_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_DISABLED)?;
                let value = i32::try_from(&disabled.GetValue()?)?;
                if value == 1 {
                    debug!("key not handled - keyboard disabled");
                    return Ok(false);
                }
            }
            Ok(true)
        })
    }

    fn insert_text(&self, context: &ITfContext, text: &str) -> Result<()> {
        debug!(text; "going to request immediate text insertion");
        let htext = text.into();
        let session = InsertText::new(context.clone(), htext).into_object();
        request_edit_session(
            context,
            self.tid,
            session.as_interface(),
            TF_ES_ASYNC | TF_ES_READWRITE,
        );
        Ok(())
    }

    fn end_composition(&mut self, context: &ITfContext) -> Result<()> {
        // Transfer the ownership of this composition to the EndComposition edit session.
        let composition = mem::replace(&mut self.composition, Rc::new(RefCell::new(None)));
        self.pending_edit = Weak::new();
        let session = EndComposition::new(context.clone(), composition).into_object();
        request_edit_session(
            context,
            self.tid,
            session.as_interface(),
            TF_ES_ASYNC | TF_ES_READWRITE,
        );
        Ok(())
    }

    fn set_composition_string(
        &mut self,
        context: &ITfContext,
        commit: String,
        preedit: String,
        segments: Vec<(usize, usize)>,
        cursor: usize,
    ) -> Result<()> {
        debug!(commit, preedit; "set composition string");
        if let Some(cell) = self.pending_edit.upgrade() {
            debug!(cursor, preedit:%; "Reuse existing edit session");
            cell.replace(Some(CompositionString {
                commit,
                preedit,
                segments,
                cursor,
            }));
        } else {
            let pending = Rc::new(RefCell::new(Some(CompositionString {
                commit,
                preedit,
                segments,
                cursor,
            })));
            let session = SetCompositionString::new(
                context.clone(),
                self.composition.clone(),
                self.composition_sink.clone(),
                self.input_da_atom.clone(),
                pending.clone(),
            )
            .into_object();
            self.pending_edit = Rc::downgrade(&pending);
            request_edit_session(
                context,
                self.tid,
                session.as_interface(),
                TF_ES_ASYNC | TF_ES_READWRITE,
            );
        }
        Ok(())
    }

    fn get_selection_rect(&self, context: &ITfContext) -> Result<RECT> {
        let session = SelectionRect::new(context.clone()).into_object();
        request_edit_session(
            context,
            self.tid,
            session.as_interface(),
            TF_ES_SYNC | TF_ES_READ,
        );
        Ok(session.rect())
    }

    fn init_openclose(&self, tid: u32) -> Result<()> {
        let compartment_mgr: ITfCompartmentMgr = self.thread_mgr.cast()?;
        unsafe {
            let compartment =
                compartment_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)?;
            let _ = compartment.GetValue()?;
            let _ = compartment.SetValue(tid, &1i32.into());
        }
        Ok(())
    }

    fn keyboard_open(&self) -> Result<bool> {
        let compartment_mgr: ITfCompartmentMgr = self.thread_mgr.cast()?;
        unsafe {
            let compartment =
                compartment_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)?;
            Ok(i32::try_from(&compartment.GetValue()?)? != 0)
        }
    }

    fn update_lang_buttons(&self, engine: &Engine) -> Result<()> {
        let icon = engine.lang_icon(&self.lang_icons);
        self.switch_lang_button.set_icon(icon)?;
        self.ime_mode_button.set_icon(icon)?;
        let _ = self
            .ime_mode_button
            .set_enabled(!engine.lang_mode.get().is_disabled());
        engine.check_menu_items(self.popup_menu);
        Ok(())
    }
}

impl Tsf<'_> {
    fn context(&self) -> Result<ITfContext> {
        if let Some(context) = &self.context {
            return Ok(context.clone());
        }
        unsafe {
            let doc_mgr = self
                .ui
                .thread_mgr
                .GetFocus()
                .context("failed to get current ITfDocumentMgr")?;
            doc_mgr.GetTop().context("failed to get current ITfContext")
        }
    }
}

impl Frontend for Tsf<'_> {
    fn has_composition(&self) -> bool {
        self.ui.has_composition()
    }

    fn is_context_mutable(&self) -> Result<bool> {
        Ok(self.ui.is_context_mutable(&self.context()?)?)
    }

    fn caret_rect(&self) -> Result<RECT> {
        self.ui.get_selection_rect(&self.context()?)
    }

    fn popup_parent(&self) -> Result<HWND> {
        let view = unsafe { self.context()?.GetActiveView()? };
        // UILess console may not have valid HWND
        Ok(unsafe { view.GetWnd().unwrap_or_default() })
    }

    fn thread_mgr(&self) -> ITfThreadMgr {
        self.ui.thread_mgr.clone()
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        let context = self.context()?;
        self.ui.insert_text(&context, text)
    }

    fn set_composition_string(
        &mut self,
        commit: String,
        preedit: String,
        segments: Vec<(usize, usize)>,
        cursor: usize,
    ) -> Result<()> {
        let context = self.context()?;
        self.ui
            .set_composition_string(&context, commit, preedit, segments, cursor)
    }

    fn end_composition(&mut self) -> Result<()> {
        let context = self.context()?;
        self.ui.end_composition(&context)
    }

    fn update_lang_buttons(&self, engine: &Engine) -> Result<()> {
        self.ui.update_lang_buttons(engine)
    }

    fn lang_mode_changed(&self) {
        debug!("set pending_lang_mode_change to true");
        self.ui.pending_lang_mode_change.set(true);
    }
}

/// Reentrant prone operations can only be done via this type to ensure
/// we don't hold mutable borrow while performing reentrant operations.
pub(crate) struct ReentrantOps<'a> {
    tip: Ref<'a, Option<ChewingTextService>>,
}

impl<'a> ReentrantOps<'a> {
    pub(crate) fn from_ref(
        cell: &'a RefCell<Option<ChewingTextService>>,
        tip_ref: Ref<'a, Option<ChewingTextService>>,
    ) -> ReentrantOps<'a> {
        let _ = cell;
        ReentrantOps { tip: tip_ref }
    }
    pub(crate) fn from_mut(
        cell: &'a RefCell<Option<ChewingTextService>>,
        tip_mut: RefMut<'_, Option<ChewingTextService>>,
    ) -> ReentrantOps<'a> {
        // Drop the only mutabble reference so we can create an immutable one
        drop(tip_mut);
        ReentrantOps { tip: cell.borrow() }
    }

    pub(crate) fn sync_keyboard_openclose(&self, force: bool) -> Result<()> {
        let Some(tip) = self.tip.as_ref() else {
            bail!("chewing_tip is not initialized");
        };
        debug!(force, pending_lang_mode_change=tip.ui.pending_lang_mode_change.get(); "sync_keyboard_openclose");
        if !force && !tip.ui.pending_lang_mode_change.get() {
            return Ok(());
        }
        let sync = tip.engine.cfg.chewing_tsf.sync_lang_mode_openclose;
        if !sync {
            // sync openclose is disabled by default
            tip.ui.pending_lang_mode_change.set(false);
            // Still, Shift opens a closed keyboard (Engine::on_keyup), which
            // Ctrl+Space and the program should see.
            if tip.engine.lang_mode.get().is_disabled() || tip.ui.keyboard_open()? {
                return Ok(());
            }
        }
        let compartment_mgr: ITfCompartmentMgr = tip.ui.thread_mgr.cast()?;
        unsafe {
            let compartment =
                compartment_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)?;
            let openclose: i32 = match tip.engine.lang_mode.get() {
                _ if !sync => 1,
                TsfLangMode::Chinese => 1,
                TsfLangMode::English => 0,
                _ => 0,
            };
            // NB: recursively call this inside compartment callback will fail
            let _ = compartment.SetValue(tip.ui.tid, &openclose.into());
        }

        Ok(())
    }
}
