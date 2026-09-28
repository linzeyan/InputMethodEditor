// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use self::config::export_config;
use self::config::import_config;
use self::config::load_config;
use self::config::save_config;
use self::fonts::get_system_fonts;
use self::version::app_version;
use chewing_tip_core::SETTINGS_SCHEME;
use tauri::Emitter;
use tauri::LogicalSize;
use tauri::Manager;
use tauri::menu::{MenuBuilder, SubmenuBuilder};

mod config;
mod dictionary;
mod fonts;
mod version;

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
            let menu = if dictionary {
                MenuBuilder::new(app).items(&[&about_menu]).build()?
            } else {
                let file_menu = SubmenuBuilder::new(app, "檔案")
                    .text("import", "匯入設定檔...")
                    .text("export", "匯出設定檔...")
                    .build()?;
                MenuBuilder::new(app)
                    .items(&[&file_menu, &about_menu])
                    .build()?
            };
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
                    "about" => {
                        if let Some(about_window) = app.get_webview_window("about") {
                            about_window.show().expect("failed to show about window");
                        }
                    }
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
            import_config,
            export_config,
            load_config,
            save_config,
            get_system_fonts,
            app_version,
            start_page,
            dictionary::load,
            dictionary::save,
            dictionary::validate,
            dictionary::map_bopomofo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
