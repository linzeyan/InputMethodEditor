// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod dictionary;
mod fonts;
mod programs;
mod settings;
mod update;

use std::cell::Cell;
use std::env;
use std::path::PathBuf;

use chewing_tip_core::SETTINGS_SCHEME;
use slint::language::ColorScheme;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::platform::windows::{IconExtWindows, WindowAttributesExtWindows};
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::window::{Icon, Theme};
use slint::{ComponentHandle, PhysicalPosition};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IDYES, IsIconic, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_YESNO,
    MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::HSTRING;
use windows_registry::CURRENT_USER;

slint::include_modules!();

/// Shipped in the same package as the IME, so this is its version too.
const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    /// Whether windows are made dark, which the winit hook reads as each is.
    static DARK: Cell<bool> = const { Cell::new(false) };
}

fn main() -> Result<(), slint::PlatformError> {
    // The scheduled task's run: no window unless there is an update to offer.
    if env::args().nth(1).as_deref() == Some("--check-update") {
        update::check_update();
        return Ok(());
    }
    DARK.set(config::dark_candidates());
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(|attributes| {
            // The exe's, which winit's windows otherwise go without.
            let icon = Icon::from_resource(1, None).ok();
            let mut attributes = attributes
                .with_window_icon(icon.clone())
                .with_taskbar_icon(icon);
            // When the window is made, so a dark one doesn't flash light
            // first; Slint's palette takes the theme from it. Light leaves it
            // to Windows.
            if DARK.get() {
                attributes = attributes.with_theme(Some(Theme::Dark));
            }
            attributes
        })
        .select()?;
    // The IME's menu opens the user dictionary with `<scheme>://dictionary`,
    // and the settings otherwise.
    let dictionary = format!("{SETTINGS_SCHEME}://dictionary");
    if env::args().skip(1).any(|arg| arg.starts_with(&dictionary)) {
        dictionary::run()
    } else {
        settings::run()
    }
}

fn hwnd(window: &slint::Window) -> Option<HWND> {
    window.with_winit_window(|window| match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(HWND(handle.hwnd.get() as *mut _)),
        _ => None,
    })?
}

/// Centered on its screen once made: Windows cascades new windows down the
/// screen, which put the buttons at the bottom under the taskbar.
fn center_when_made<T: ComponentHandle + 'static>(ui: &T) {
    let ui = ui.as_weak();
    let _ = slint::spawn_local(async move {
        let Some(ui) = ui.upgrade() else { return };
        let window = ui.window();
        if window.winit_window().await.is_err() {
            return;
        }
        let (Some(hwnd), Some(size)) = (
            hwnd(window),
            window.with_winit_window(|window| window.outer_size()),
        ) else {
            return;
        };
        let mut monitor = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor_handle = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        if !unsafe { GetMonitorInfoW(monitor_handle, &mut monitor) }.as_bool() {
            return;
        }
        let work = monitor.rcWork;
        window.set_position(PhysicalPosition::new(
            work.left + (work.right - work.left - size.width as i32) / 2,
            work.top + (work.bottom - work.top - size.height as i32).max(0) / 2,
        ));
    });
}

/// A Win32 message box, modal to the window when there is one.
fn message_box(
    window: Option<&slint::Window>,
    text: &str,
    title: &str,
    style: MESSAGEBOX_STYLE,
) -> MESSAGEBOX_RESULT {
    let owner = window.and_then(hwnd);
    unsafe { MessageBoxW(owner, &HSTRING::from(text), &HSTRING::from(title), style) }
}

fn error_box(window: Option<&slint::Window>, text: &str) {
    message_box(window, text, "錯誤", MB_OK | MB_ICONERROR);
}

fn ask(window: &slint::Window, question: &str) -> bool {
    message_box(
        Some(window),
        question,
        "InputMethodEditor",
        MB_YESNO | MB_ICONQUESTION,
    ) == IDYES
}

/// The file to read, picked in Windows' dialog.
fn open_file(window: &slint::Window, kind: &str, extension: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(kind, &[extension])
        .set_parent(&window.window_handle())
        .pick_file()
}

fn save_file(window: &slint::Window, kind: &str, extension: &str, name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(kind, &[extension])
        .set_file_name(name)
        .set_parent(&window.window_handle())
        .save_file()
}

/// Whether Windows has apps dark.
fn system_dark() -> bool {
    CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("AppsUseLightTheme"))
        .is_ok_and(|light| light == 0)
}

/// Turns `window` dark or back as the candidate colors go. Returns the
/// palette for it when it changes.
fn follow_candidates(window: &slint::Window, dark: bool) -> Option<ColorScheme> {
    if DARK.get() == dark {
        return None;
    }
    DARK.set(dark);
    // The title bar; winit leaves it to Windows given none.
    window.with_winit_window(|window| window.set_theme(dark.then_some(Theme::Dark)));
    // Told outright: the palette has only seen the theme the window was
    // made with, and light is Windows' own.
    Some(if dark || system_dark() {
        ColorScheme::Dark
    } else {
        ColorScheme::Light
    })
}

/// Brings the window titled `title` to the front, if one is open, and tells
/// whether there was one. The IME's menu starts a process for each click: a
/// second window saving the settings it opened with put back what the first
/// had changed since.
fn bring_forward(title: &str) -> bool {
    let Ok(hwnd) = (unsafe { FindWindowW(None, &HSTRING::from(title)) }) else {
        return false;
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    true
}
