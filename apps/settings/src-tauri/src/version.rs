// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

/// Shipped in the same package as the IME, so this is its version too.
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
