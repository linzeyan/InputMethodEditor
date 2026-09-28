// SPDX-License-Identifier: GPL-3.0-or-later

//! Typing without TSF, for an account that can't register an input method,
//! which takes admin rights: a low-level keyboard hook hands keys to the
//! engine, and SendInput types the text into whatever window has the focus.
//! That window never learns about a composition, so what is being typed shows
//! in a window of our own at its caret.

use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use anyhow::{Result, ensure};
use log::{debug, error, info, warn};
use logforth::record::{Level, LevelFilter};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MapWindowPoints, MonitorFromWindow};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    AccessibleObjectFromWindow, HWINEVENTHOOK, IAccessible, SetWinEventHook, UnhookWinEvent,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, GetDpiForWindow,
    MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_PACKET, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::TextServices::ITfThreadMgr;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CHILDID_SELF, CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    EVENT_SYSTEM_FOREGROUND, GUITHREADINFO, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo,
    GetMessageW, GetWindowThreadProcessId, HC_ACTION, HICON, HMENU, KBDLLHOOKSTRUCT, KillTimer,
    MF_SEPARATOR, MF_STRING, MSG, OBJID_CARET, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SetForegroundWindow, SetTimer, SetWindowsHookExW, TPM_BOTTOMALIGN,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_KEYDOWN, WM_LBUTTONUP, WM_RBUTTONUP, WM_SYSKEYDOWN,
    WM_TIMER, WNDCLASSEXW, WS_POPUP,
};
use windows_core::{ComObject, HSTRING, Interface, PCWSTR, w};

use crate::com::G_HINSTANCE;
use crate::engine::key_event::SystemKeyboardEvent;
use crate::engine::{Engine, Frontend};
use crate::text_service::icons::LangIconSet;
use crate::text_service::menu::Menu;
use crate::text_service::resources::{ID_SWITCH_LANG, IDR_MENU};
use crate::text_service::ui_elements::{CandidateList, Notification, NotificationModel};
use crate::ui::gfx::color_s;

/// Marks the keys we send: they come back through the hook.
const OUR_INPUT: usize = 0x494D_4521;
const WM_TRAY: u32 = WM_APP + 1;
/// `caret_worker` found where an app's caret is (`WPARAM(1)`), or not.
const WM_CARET: u32 = WM_APP + 2;
/// Gives up waiting for the app to say where its caret is: VS Code took 200 ms
/// while starting, and a hung app takes forever.
const CARET_TIMER: usize = 1;
const CARET_TIMEOUT_MS: u32 = 250;
/// The candidate list's item `wparam` was clicked.
pub(crate) const WM_CANDIDATE: u32 = WM_APP + 3;
/// The menu resource's own commands start at 101.
const ID_EXIT: u32 = 1;

/// Where the caret last was in an app drawing its own, and in which window.
static APP_CARET: Mutex<Option<(isize, RECT)>> = Mutex::new(None);

struct State {
    engine: Engine,
    ui: HookUi,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Runs the hook until the tray menu's 結束.
pub fn run() -> Result<()> {
    logforth::starter_log::builder()
        .dispatch(|d| {
            d.filter(if cfg!(debug_assertions) {
                LevelFilter::MoreSevereEqual(Level::Debug)
            } else {
                LevelFilter::MoreSevereEqual(Level::Info)
            })
            .append(logforth::append::Stderr::default())
        })
        .apply();
    unsafe {
        // A second one would type everything twice.
        let _mutex = CreateMutexW(None, false, w!("Local\\InputMethodEditor"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            info!("already running");
            return Ok(());
        }
        // Caret positions and our windows in physical pixels, whatever the
        // app being typed into scales.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hinstance = HINSTANCE(GetModuleHandleW(None)?.0);
        // The engine finds the dictionary and the resources through it, as
        // it does beside chewing_tip.dll.
        G_HINSTANCE.store(hinstance.0 as usize, Ordering::Relaxed);
        CandidateList::window_register_class(hinstance);
        Notification::window_register_class(hinstance);

        let mut ui = HookUi::new(hinstance)?;
        let engine = Engine::new(&mut ui)?;
        STATE.with(|state| state.replace(Some(State { engine, ui })));

        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(hinstance), 0)?;
        info!("keyboard hook installed");
        // Out of context, so called from this thread's message loop.
        let foreground_hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        if foreground_hook.is_invalid() {
            error!("unable to watch the foreground window");
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWinEvent(foreground_hook);
        let _ = UnhookWindowsHookEx(hook);
        if let Some(state) = STATE.with(|state| state.take()) {
            state.ui.remove_tray_icon();
        }
    }
    Ok(())
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if on_key(key, down) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Another window came to the front: drops what was being typed, as TSF ends
/// a composition when the focus moves. Committing it would type it into the
/// new window, which is where SendInput goes now.
unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    debug!("foreground window changed");
    with_state(|State { engine, ui }| {
        engine.on_composition_terminated();
        let _ = ui.end_composition();
    });
}

/// Whether to swallow the key: the engine took it.
fn on_key(key: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    // Text we type, and text other programs type as characters (a password
    // manager, say), is not keys to compose with.
    if key.dwExtraInfo == OUR_INPUT || key.vkCode == VK_PACKET.0 as u32 {
        return false;
    }
    let vk = VIRTUAL_KEY(key.vkCode as u16);
    let key_state = key_state(vk, down);
    // Win and its shortcuts belong to the shell.
    if key_state[VK_LWIN.0 as usize] & 0x80 != 0
        || key_state[VK_RWIN.0 as usize] & 0x80 != 0
        || vk == VK_LWIN
        || vk == VK_RWIN
    {
        return false;
    }
    let ev = SystemKeyboardEvent::with_key_state(vk.0, key.scanCode as u16, key_state);
    let start = Instant::now();
    // None when reentered, as while the tray menu is open.
    let handled = with_state(|State { engine, ui }| {
        let result = if down {
            engine.on_keydown(ui, ev)
        } else {
            engine.on_test_keyup(ui, ev)
        };
        result.unwrap_or_else(|error| {
            error!("unable to handle key {:#x}: {error:#}", vk.0);
            false
        })
    })
    .unwrap_or(false);
    // Windows drops a hook that keeps it waiting too long, without telling.
    let elapsed = start.elapsed();
    if elapsed.as_millis() > 100 {
        warn!("key {:#x} took {elapsed:?}", vk.0);
    } else {
        debug!(vk = vk.0, down, handled, elapsed:?; "key");
    }
    // Unlike under TSF, a key swallowed here never reaches Windows' own key
    // state: a modifier would stay down, CapsLock wouldn't toggle. A keyup
    // whose keydown the app didn't get is harmless.
    handled && down && !is_state_key(vk)
}

fn is_state_key(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_CONTROL
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_MENU
            | VK_LMENU
            | VK_RMENU
            | VK_CAPITAL
            | VK_NUMLOCK
    )
}

/// The key state the window being typed into will see. The hook runs before
/// Windows records the key, and this thread, which gets no keyboard input,
/// has no key state of its own worth reading.
fn key_state(vk: VIRTUAL_KEY, down: bool) -> [u8; 256] {
    let mut state = [0u8; 256];
    for side in [
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ] {
        if unsafe { GetAsyncKeyState(side.0 as i32) } < 0 {
            state[side.0 as usize] = 0x80;
        }
    }
    state[vk.0 as usize] = if down { 0x80 } else { 0 };
    for (either, left, right) in [
        (VK_SHIFT, VK_LSHIFT, VK_RSHIFT),
        (VK_CONTROL, VK_LCONTROL, VK_RCONTROL),
        (VK_MENU, VK_LMENU, VK_RMENU),
    ] {
        state[either.0 as usize] |= state[left.0 as usize] | state[right.0 as usize];
    }
    for toggle in [VK_CAPITAL, VK_NUMLOCK] {
        if unsafe { GetKeyState(toggle.0 as i32) } & 1 != 0 {
            state[toggle.0 as usize] |= 1;
        }
    }
    state
}

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    STATE.with(|state| state.try_borrow_mut().ok()?.as_mut().map(f))
}

/// Types text as characters, whatever the keyboard layout.
fn send_text(text: &str) {
    let inputs: Vec<INPUT> = text
        .encode_utf16()
        .flat_map(|unit| {
            [KEYBD_EVENT_FLAGS(0), KEYEVENTF_KEYUP].map(|up| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE | up,
                        time: 0,
                        dwExtraInfo: OUR_INPUT,
                    },
                },
            })
        })
        .collect();
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        error!("SendInput took {sent} of {} events", inputs.len());
    }
}

/// Where the caret of the window being typed into is, in physical screen
/// pixels.
enum Caret {
    Known(RECT),
    /// The app draws its own and has been asked; `caret_worker` posts the
    /// answer. Meanwhile, where it was last time, or the pointer.
    Guessed(RECT),
}

fn focus_caret(caret_requests: &Sender<isize>) -> Result<Caret> {
    unsafe {
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let foreground = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(foreground, None);
        GetGUIThreadInfo(thread, &mut info)?;
        let hwnd = info.hwndCaret;
        if hwnd.is_invalid() {
            // Apps drawing their own caret (browsers, VS Code) say where it is
            // when asked, which takes another thread.
            let focus = if info.hwndFocus.is_invalid() {
                foreground
            } else {
                info.hwndFocus
            };
            let _ = caret_requests.send(focus.0 as isize);
            if let Some((window, rect)) = *APP_CARET.lock().unwrap()
                && window == focus.0 as isize
            {
                return Ok(Caret::Guessed(rect));
            }
            return Ok(Caret::Guessed(pointer_rect()?));
        }
        let rect = info.rcCaret;
        // The caret is in the window's own pixels, which Windows scales up
        // for apps that aren't DPI aware; to this thread, per-monitor aware,
        // the window's client area is in physical pixels.
        let mut monitor_dpi = (0, 0);
        GetDpiForMonitor(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut monitor_dpi.0,
            &mut monitor_dpi.1,
        )?;
        let window_dpi = GetDpiForWindow(hwnd).max(1);
        let scale = |v: i32| v * monitor_dpi.0 as i32 / window_dpi as i32;
        let mut points = [
            POINT {
                x: scale(rect.left),
                y: scale(rect.top),
            },
            POINT {
                x: scale(rect.right),
                y: scale(rect.bottom),
            },
        ];
        MapWindowPoints(Some(hwnd), None, &mut points);
        debug!(rect:?, monitor_dpi:?, window_dpi, points:?; "caret");
        Ok(Caret::Known(RECT {
            left: points[0].x,
            top: points[0].y,
            right: points[1].x,
            bottom: points[1].y,
        }))
    }
}

/// For apps that don't say where their caret is: the pointer, usually where
/// the text field was clicked.
fn pointer_rect() -> Result<RECT> {
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point)? };
    Ok(RECT {
        left: point.x,
        top: point.y,
        right: point.x,
        bottom: point.y + 20,
    })
}

/// Asks apps drawing their own caret where it is, as magnifiers do: MSAA's
/// caret object, which Chromium and VS Code answer. On a thread of its own
/// because an app may take its time (VS Code took 200 ms while starting), and
/// Windows drops a hook that keeps it waiting.
fn caret_worker(requests: Receiver<isize>, window: isize) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    while let Ok(mut focus) = requests.recv() {
        // Asked on every key: only the latest matters.
        while let Ok(latest) = requests.try_recv() {
            focus = latest;
        }
        let found = match accessible_caret(HWND(focus as _)) {
            Ok(rect) => {
                debug!(rect:?; "accessible caret");
                *APP_CARET.lock().unwrap() = Some((focus, rect));
                true
            }
            Err(error) => {
                debug!("no accessible caret: {error:#}");
                false
            }
        };
        let _ = unsafe {
            PostMessageW(
                Some(HWND(window as _)),
                WM_CARET,
                WPARAM(found.into()),
                LPARAM(0),
            )
        };
    }
}

fn accessible_caret(window: HWND) -> Result<RECT> {
    unsafe {
        let mut object = std::ptr::null_mut();
        AccessibleObjectFromWindow(window, OBJID_CARET.0 as u32, &IAccessible::IID, &mut object)?;
        let caret = IAccessible::from_raw(object);
        let (mut x, mut y, mut width, mut height) = (0, 0, 0, 0);
        let child = VARIANT::from(CHILDID_SELF as i32);
        caret.accLocation(&mut x, &mut y, &mut width, &mut height, &child)?;
        // Empty when the window has no caret.
        ensure!(height > 0, "empty");
        Ok(RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        })
    }
}

/// How what is being typed looks: like the candidate list below it.
#[derive(Default)]
struct PreeditStyle {
    font_family: HSTRING,
    font_size: f32,
    fg_color: D2D1_COLOR_F,
    bg_color: D2D1_COLOR_F,
    border_color: D2D1_COLOR_F,
}

struct HookUi {
    /// Hidden; gets the tray icon's clicks.
    window: HWND,
    _menu: Menu,
    popup_menu: HMENU,
    lang_icons: LangIconSet,
    /// Shows what is being typed, which the app doesn't see until committed.
    preedit: ComObject<Notification>,
    preedit_style: RefCell<PreeditStyle>,
    composing: bool,
    /// To `caret_worker`: the windows to ask where their caret is.
    caret_requests: Sender<isize>,
}

impl HookUi {
    fn new(hinstance: HINSTANCE) -> Result<HookUi> {
        let class = w!("InputMethodEditorHook");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class,
            ..Default::default()
        };
        let window = unsafe {
            RegisterClassExW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("InputMethodEditor"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinstance),
                None,
            )?
        };
        let menu = Menu::load(hinstance, IDR_MENU);
        let popup_menu = menu.sub_menu(0);
        unsafe {
            AppendMenuW(popup_menu, MF_SEPARATOR, 0, PCWSTR::null())?;
            AppendMenuW(popup_menu, MF_STRING, ID_EXIT as usize, w!("結束 (&X)"))?;
        }
        let preedit = Notification::new(HWND::default(), None)?;
        preedit.hide();
        let (caret_requests, requests) = mpsc::channel();
        let window_id = window.0 as isize;
        std::thread::spawn(move || caret_worker(requests, window_id));
        let ui = HookUi {
            window,
            _menu: menu,
            popup_menu,
            lang_icons: LangIconSet::load(),
            preedit,
            preedit_style: Default::default(),
            composing: false,
            caret_requests,
        };
        ui.add_tray_icon(ui.lang_icons.tc.light);
        Ok(ui)
    }

    fn tray_icon(&self, icon: HICON) -> NOTIFYICONDATAW {
        // Filled apart: the struct is packed on x86, where a reference into
        // it may be misaligned.
        let mut tip = [0; 128];
        for (to, from) in tip.iter_mut().zip("InputMethodEditor".encode_utf16()) {
            *to = from;
        }
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: icon,
            szTip: tip,
            ..Default::default()
        }
    }

    fn add_tray_icon(&self, icon: HICON) {
        let icon = self.tray_icon(icon);
        if !unsafe { Shell_NotifyIconW(NIM_ADD, &icon) }.as_bool() {
            error!("unable to add the tray icon");
        }
    }

    fn remove_tray_icon(&self) {
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &self.tray_icon(HICON::default())) };
    }

    fn place_preedit(&self, caret: RECT) {
        self.preedit.set_position(caret.left, caret.bottom);
        // HACK set position again to use correct DPI setting
        self.preedit.set_position(caret.left, caret.bottom);
        self.preedit.show();
    }

    /// The app said where its caret is, or couldn't (`found` false), or took
    /// too long.
    fn caret_answered(&self, found: bool) {
        let _ = unsafe { KillTimer(Some(self.window), CARET_TIMER) };
        if !self.composing {
            return;
        }
        match *APP_CARET.lock().unwrap() {
            Some((_, rect)) if found => self.place_preedit(rect),
            // Still waiting to be shown.
            _ if self.preedit.window_rect().is_none() => match pointer_rect() {
                Ok(rect) => {
                    debug!("no caret; using the pointer");
                    self.place_preedit(rect);
                }
                Err(error) => error!("unable to place the composition: {error:#}"),
            },
            _ => {}
        }
    }
}

impl Frontend for HookUi {
    fn has_composition(&self) -> bool {
        self.composing
    }

    fn is_context_mutable(&self) -> Result<bool> {
        Ok(true)
    }

    fn caret_rect(&self) -> Result<RECT> {
        // Popups go below what is being typed, which covers the caret.
        if self.composing
            && let Some(rect) = self.preedit.window_rect()
        {
            return Ok(rect);
        }
        match focus_caret(&self.caret_requests)? {
            Caret::Known(rect) | Caret::Guessed(rect) => Ok(rect),
        }
    }

    fn popup_parent(&self) -> Result<HWND> {
        // The candidate list tells it which item was clicked.
        Ok(self.window)
    }

    fn thread_mgr(&self) -> Option<ITfThreadMgr> {
        None
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        send_text(text);
        Ok(())
    }

    fn set_composition_string(
        &mut self,
        commit: String,
        preedit: String,
        _segments: Vec<(usize, usize)>,
        cursor: usize,
    ) -> Result<()> {
        debug!(commit, preedit; "set composition string");
        if !commit.is_empty() {
            send_text(&commit);
        }
        if preedit.is_empty() {
            return self.end_composition();
        }
        let caret = preedit
            .chars()
            .take(cursor)
            .map(char::len_utf16)
            .sum::<usize>();
        let style = self.preedit_style.borrow();
        self.preedit.set_model(NotificationModel {
            text: HSTRING::from(&preedit),
            caret: Some(caret as u32),
            font_family: style.font_family.clone(),
            font_size: style.font_size,
            fg_color: style.fg_color,
            bg_color: style.bg_color,
            border_color: style.border_color,
        });
        // Every time: the app's caret moves once it gets what was committed.
        match focus_caret(&self.caret_requests)? {
            // Rather than show up at a guess and then jump to the caret, it
            // waits for the app to answer, a millisecond usually.
            Caret::Guessed(_) if self.preedit.window_rect().is_none() => {
                debug!("waiting for the app's caret");
                let _ = unsafe { SetTimer(Some(self.window), CARET_TIMER, CARET_TIMEOUT_MS, None) };
            }
            Caret::Known(rect) | Caret::Guessed(rect) => self.place_preedit(rect),
        }
        self.composing = true;
        Ok(())
    }

    fn end_composition(&mut self) -> Result<()> {
        self.preedit.hide();
        self.composing = false;
        Ok(())
    }

    fn update_lang_buttons(&self, engine: &Engine) -> Result<()> {
        engine.check_menu_items(self.popup_menu);
        let icon = self.tray_icon(engine.lang_icon(&self.lang_icons));
        if !unsafe { Shell_NotifyIconW(NIM_MODIFY, &icon) }.as_bool() {
            error!("unable to update the tray icon");
        }
        // Here because the engine calls it whenever the config changes.
        let cfg = &engine.cfg.chewing_tsf;
        *self.preedit_style.borrow_mut() = PreeditStyle {
            font_family: HSTRING::from(&cfg.font_family),
            font_size: cfg.font_size as f32,
            fg_color: color_s(&cfg.font_fg_color),
            bg_color: color_s(&cfg.font_bg_color),
            border_color: color_s(&cfg.cand_list_border_color),
        };
        Ok(())
    }

    fn lang_mode_changed(&self) {}
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Explorer restarted: the tray icon is gone.
    static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();
    let taskbar_created =
        *TASKBAR_CREATED.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });
    match msg {
        WM_TRAY => {
            match (lparam.0 & 0xffff) as u32 {
                WM_LBUTTONUP => {
                    with_state(|State { engine, ui }| engine.on_command(ui, ID_SWITCH_LANG));
                }
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CARET => {
            with_state(|State { ui, .. }| ui.caret_answered(wparam.0 != 0));
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == CARET_TIMER => {
            with_state(|State { ui, .. }| ui.caret_answered(false));
            LRESULT(0)
        }
        WM_CANDIDATE => {
            with_state(|State { engine, ui }| {
                if let Err(error) = engine.select_candidate(ui, wparam.0) {
                    error!("unable to select candidate {}: {error:#}", wparam.0);
                }
            });
            LRESULT(0)
        }
        _ if msg == taskbar_created => {
            with_state(|State { engine, ui }| ui.add_tray_icon(engine.lang_icon(&ui.lang_icons)));
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_menu(hwnd: HWND) {
    let Some(menu) = with_state(|state| state.ui.popup_menu) else {
        return;
    };
    let mut pos = POINT::default();
    // Not holding the state: keys typed while the menu is open reenter.
    let command = unsafe {
        let _ = GetCursorPos(&mut pos);
        // Otherwise the menu stays open when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
            pos.x,
            pos.y,
            None,
            hwnd,
            None,
        )
    };
    match command.0 as u32 {
        0 => {}
        ID_EXIT => unsafe { PostQuitMessage(0) },
        id => {
            with_state(|State { engine, ui }| engine.on_command(ui, id));
        }
    }
}
