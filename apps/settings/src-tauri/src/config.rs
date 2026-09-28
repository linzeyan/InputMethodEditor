// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use chewing_tip_core::config::Config;
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

            Ok(cfg)
        })
    }

    inner().map_err(|e| e.report().to_string())
}

#[tauri::command]
pub fn save_config(mut config: Config) -> Result<(), String> {
    fn inner(config: &mut Config) -> Result<(), Error> {
        expect_error("無法儲存設定", || {
            config.save_reg();

            let sys_symbols_dat = system_path_for_file("symbols.dat")
                .and_then(|path| {
                    expect_error("無法讀取符號檔", || Ok(fs::read_to_string(path)?))
                })
                .unwrap_or_default();
            if config.symbols_dat != sys_symbols_dat {
                let user_symbols_dat_path = default_user_path_for_file("symbols.dat")?;
                fs::create_dir_all(user_symbols_dat_path.parent().unwrap())?;
                fs::write(user_symbols_dat_path, &config.symbols_dat)?;
            }

            let sys_swkb_dat = system_path_for_file("swkb.dat")
                .and_then(|path| {
                    expect_error("無法讀取快捷符號檔", || {
                        Ok(fs::read_to_string(path)?)
                    })
                })
                .unwrap_or_default();
            if config.swkb_dat != sys_swkb_dat {
                let user_swkb_dat_path = default_user_path_for_file("swkb.dat")?;
                fs::create_dir_all(user_swkb_dat_path.parent().unwrap())?;
                fs::write(user_swkb_dat_path, &config.swkb_dat)?;
            }

            Ok(())
        })
    }

    inner(&mut config).map_err(|e| e.report().to_string())
}
