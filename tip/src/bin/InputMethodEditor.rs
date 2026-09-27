// SPDX-License-Identifier: GPL-3.0-or-later

//! The input method for an account that can't install one: see `hook`.

#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

fn main() -> anyhow::Result<()> {
    chewing_tip::hook::run()
}
