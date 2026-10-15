// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use self::config::default_config;
use self::config::export_config;
use self::config::import_config;
use self::config::load_config;
use self::config::save_config;
use self::fonts::get_system_fonts;
use self::version::app_version;
use chewing_tip_core::SETTINGS_SCHEME;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::LogicalSize;
use tauri::Manager;
use tauri::Theme;
use tauri::WebviewWindowBuilder;
use tauri::menu::{MenuBuilder, SubmenuBuilder};

mod config;
mod dictionary;
mod fonts;
mod programs;
mod update;
mod version;

pub use update::check_update;

/// The IME's menu opens the user dictionary with `<scheme>://dictionary`,
/// and the settings otherwise.
fn opens_dictionary() -> bool {
    let url = format!("{SETTINGS_SCHEME}://dictionary");
    std::env::args().skip(1).any(|arg| arg.starts_with(&url))
}

#[tauri::command]
fn start_page() -> &'static str {
    if opens_dictionary() {
        "dictionary"
    } else {
        "settings"
    }
}

/// Made when asked for, not with the app: even hidden, a window costs a
/// WebView2 renderer process, about 25 MB. It shows itself once drawn.
fn open_about(app: &AppHandle) -> tauri::Result<()> {
    let conf = app.config().app.windows.iter().find(|w| w.label == "about");
    // The main window's, which may be a theme picked but not saved: webviews
    // share one color scheme, so the saved one would turn the main window too.
    let theme = app
        .get_webview_window("main")
        .map(|w| w.theme())
        .transpose()?;
    WebviewWindowBuilder::from_config(app, conf.expect("about in tauri.conf.json"))?
        .theme(theme)
        .build()?;
    Ok(())
}

/// Opens the folder of the text service's ime.log, to attach to a report.
#[tauri::command]
fn open_log_dir() -> Result<(), String> {
    let dir = chewing_tip_core::shell::user_dir().map_err(|e| e.to_string())?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .manage(dictionary::Loaded::default())
        .setup(|app| {
            let dictionary = opens_dictionary();
            let about_menu = SubmenuBuilder::new(app, "關於")
                .text("about", "關於 InputMethodEditor")
                .build()?;
            let (import, export) = if dictionary {
                ("匯入詞庫...", "匯出詞庫...")
            } else {
                ("匯入設定檔...", "匯出設定檔...")
            };
            let file_menu = SubmenuBuilder::new(app, "檔案")
                .text("import", import)
                .text("export", export)
                .build()?;
            let menu = MenuBuilder::new(app)
                .items(&[&file_menu, &about_menu])
                .build()?;
            // Before they show, so a dark one doesn't flash light first.
            if config::dark_candidates() {
                for window in app.webview_windows().values() {
                    window.set_theme(Some(Theme::Dark))?;
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                if dictionary {
                    window.set_title("InputMethodEditor 使用者詞庫")?;
                    window.set_size(LogicalSize::new(930.0, 600.0))?;
                    window.set_maximizable(true)?;
                    window.center()?;
                }
                window.show().expect("failed to show main window");
                window.set_menu(menu)?;
                let app = app.handle().clone();
                window.on_menu_event(move |window, event| match event.id().0.as_str() {
                    "about" => match app.get_webview_window("about") {
                        Some(about_window) => about_window
                            .set_focus()
                            .expect("failed to focus about window"),
                        None => open_about(&app).expect("failed to open about window"),
                    },
                    "export" => {
                        window
                            .emit("export", ())
                            .expect("failed to emit export event");
                    }
                    "import" => {
                        window
                            .emit("import", ())
                            .expect("failed to emit import event");
                    }
                    _ => {}
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            default_config,
            import_config,
            export_config,
            load_config,
            save_config,
            get_system_fonts,
            app_version,
            start_page,
            dictionary::load,
            dictionary::save,
            dictionary::import_entries,
            dictionary::export_entries,
            dictionary::validate,
            dictionary::map_bopomofo,
            programs::list_programs,
            update::find_update,
            update::install_update,
            open_log_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
