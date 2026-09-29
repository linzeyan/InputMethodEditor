// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The scheduled task's run: no window unless there is an update to offer.
    if std::env::args().nth(1).as_deref() == Some("--check-update") {
        return ime_settings::check_update();
    }
    ime_settings::run()
}
