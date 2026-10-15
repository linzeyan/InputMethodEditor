// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use chewing_tip_core::config::Config;
use chewing_tip_core::phrases::{self, PHRASES_FILE};
use chewing_tip_core::shell::user_dir;
use scoped_error::{Error, ErrorExt, expect_error};

fn default_user_path_for_file(file: &str) -> Result<PathBuf, Error> {
    expect_error("Failed to find user dir", || Ok(user_dir()?.join(file)))
}

fn user_path_for_file(file: &str) -> Result<PathBuf, Error> {
    expect_error("Failed to find user file", || {
        let user_file = default_user_path_for_file(file)?;
        if user_file.exists() {
            return Ok(user_file);
        }
        Err(format!("使用者檔案 {file} 不存在").into())
    })
}

/// The dictionary sits beside the architecture folder this exe is in, where
/// chewing_tip.dll finds it too.
fn system_path_for_file(file: &str) -> Result<PathBuf, Error> {
    expect_error("Failed to find system file", || {
        let exe = env::current_exe()?;
        let root = exe
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("無法判斷輸入法資料夾：{}", exe.display()))?;
        let path = root.join("Dictionary").join(file);
        if path.exists() {
            return Ok(path);
        }
        Err(format!("系統詞庫 {file} 不存在").into())
    })
}

#[tauri::command]
pub(crate) fn import_config(path: String) -> Result<Config, String> {
    fn inner(path: &str) -> Result<Config, Error> {
        expect_error("無法讀取設定檔", || {
            let content = fs::read_to_string(path)?;
            let cfg: Config = toml::from_str(&content)?;
            Ok(cfg)
        })
    }
    inner(&path).map_err(|e| e.report().to_string())
}

#[tauri::command]
pub(crate) fn export_config(path: String, config: Config) -> Result<(), String> {
    fn inner(path: &str, config: &Config) -> Result<(), Error> {
        expect_error("無法匯出設定檔", || {
            let content = toml::to_string_pretty(config)?;
            fs::write(path, &content)?;
            Ok(())
        })
    }
    inner(&path, &config).map_err(|e| e.report().to_string())
}

#[tauri::command]
pub(crate) fn load_config() -> Result<Config, String> {
    fn inner() -> Result<Config, Error> {
        expect_error("無法讀取設定檔", || {
            let mut cfg = Config::from_reg()?;

            if let Ok(path) = user_path_for_file("symbols.dat") {
                cfg.symbols_dat = fs::read_to_string(path)?.into();
            } else if let Ok(path) = system_path_for_file("symbols.dat") {
                cfg.symbols_dat = fs::read_to_string(path)?.into();
            }

            if let Ok(path) = user_path_for_file("swkb.dat") {
                cfg.swkb_dat = fs::read_to_string(path)?.into();
            } else if let Ok(path) = system_path_for_file("swkb.dat") {
                cfg.swkb_dat = fs::read_to_string(path)?.into();
            }

            if let Ok(path) = user_path_for_file(PHRASES_FILE) {
                cfg.custom_phrase_dat = fs::read_to_string(path)?;
            }

            Ok(cfg)
        })
    }

    inner().map_err(|e| e.report().to_string())
}

/// What each setting starts as, the tables being the system's.
#[tauri::command]
pub(crate) fn default_config() -> Config {
    let system = |file| {
        system_path_for_file(file)
            .and_then(|path| expect_error("無法讀取系統檔案", || Ok(fs::read_to_string(path)?)))
            .unwrap_or_default()
    };
    Config {
        symbols_dat: system("symbols.dat"),
        swkb_dat: system("swkb.dat"),
        ..Default::default()
    }
}

#[tauri::command]
pub fn save_config(mut config: Config) -> Result<(), String> {
    // Before anything is saved, so a bad line leaves all as it was; the user
    // fixes it from the message, which needs no source location.
    phrases::parse(&config.custom_phrase_dat).map_err(|error| format!("自訂詞組{error}"))?;
    let apps = &config.chewing_tsf;
    if let Some(app) = in_both(&apps.english_apps, &apps.chinese_apps) {
        return Err(format!(
            "各程式：{app} 同時列在英數和中文兩邊，請從其中一邊刪掉"
        ));
    }
    fn inner(config: &mut Config) -> Result<(), Error> {
        expect_error("無法儲存設定", || {
            config.save_reg();

            let sys_symbols_dat = system_path_for_file("symbols.dat")
                .and_then(|path| {
                    expect_error("無法讀取符號檔", || Ok(fs::read_to_string(path)?))
                })
                .unwrap_or_default();
            let user_symbols_dat_path = default_user_path_for_file("symbols.dat")?;
            if config.symbols_dat != sys_symbols_dat {
                fs::create_dir_all(user_symbols_dat_path.parent().unwrap())?;
                fs::write(user_symbols_dat_path, &config.symbols_dat)?;
            } else if user_symbols_dat_path.exists() {
                // Put back as the system's, as by resetting: the old copy
                // would go on being read.
                fs::remove_file(user_symbols_dat_path)?;
            }

            let sys_swkb_dat = system_path_for_file("swkb.dat")
                .and_then(|path| {
                    expect_error("無法讀取快捷符號檔", || {
                        Ok(fs::read_to_string(path)?)
                    })
                })
                .unwrap_or_default();
            let user_swkb_dat_path = default_user_path_for_file("swkb.dat")?;
            if config.swkb_dat != sys_swkb_dat {
                fs::create_dir_all(user_swkb_dat_path.parent().unwrap())?;
                fs::write(user_swkb_dat_path, &config.swkb_dat)?;
            } else if user_swkb_dat_path.exists() {
                fs::remove_file(user_swkb_dat_path)?;
            }

            let phrases_path = default_user_path_for_file(PHRASES_FILE)?;
            if !config.custom_phrase_dat.is_empty() || phrases_path.exists() {
                fs::create_dir_all(phrases_path.parent().unwrap())?;
                fs::write(phrases_path, &config.custom_phrase_dat)?;
            }

            Ok(())
        })
    }

    inner(&mut config).map_err(|e| e.report().to_string())
}

/// Whether the candidate list is dark, by its background. The settings'
/// windows go dark with it; light, they follow Windows, as before the
/// candidate list was given colors. The frontend's `isDark` is the same.
pub(crate) fn dark_candidates() -> bool {
    Config::from_reg().is_ok_and(|cfg| is_dark(&cfg.chewing_tsf.font_bg_color))
}

fn is_dark(rgba: &str) -> bool {
    let channel = |i: usize| u32::from_str_radix(rgba.get(i..i + 2)?, 16).ok();
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => 299 * r + 587 * g + 114 * b < 128_000,
        _ => false,
    }
}

/// A program listed for both modes, matched as the IME does: in any case,
/// with or without .exe. The IME would quietly take English.
fn in_both(english: &str, chinese: &str) -> Option<String> {
    let keys = |apps: &str| -> Vec<String> {
        apps.lines()
            .map(|app| app.trim().to_lowercase())
            .filter(|app| !app.is_empty())
            .map(|app| {
                if app.ends_with(".exe") {
                    app
                } else {
                    format!("{app}.exe")
                }
            })
            .collect()
    };
    let english = keys(english);
    keys(chinese).into_iter().find(|app| english.contains(app))
}

#[cfg(test)]
mod tests {
    use super::{in_both, is_dark};

    #[test]
    fn the_dark_theme_is_dark_and_the_default_light() {
        assert!(is_dark("333333FF"));
        assert!(!is_dark("FAFAFAFF"));
        // Typed wrong, it stays as Windows has it.
        assert!(!is_dark("33"));
    }

    #[test]
    fn in_both_matches_as_the_ime_does() {
        // Blank lines sit in both lists all the time and mean nothing.
        assert_eq!(in_both("chrome.exe\n\n", "\nTelegram\n"), None);
        assert_eq!(
            in_both("code.exe\r\nChrome\r\n", "Telegram\n chrome.EXE "),
            Some("chrome.exe".to_owned())
        );
    }
}
