// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

use log::error;
use windows::Win32::{Foundation::HINSTANCE, UI::WindowsAndMessaging::*};
use windows_core::PCWSTR;

#[derive(Default)]
pub(crate) struct Menu {
    hmenu: HMENU,
}

impl Menu {
    pub(crate) fn load(hinst: HINSTANCE, resource_id: u32) -> Menu {
        let hmenu =
            match unsafe { LoadMenuW(Some(hinst), PCWSTR::from_raw(resource_id as *const u16)) } {
                Ok(menu) => menu,
                Err(error) => {
                    error!("unable to load menu {resource_id}: {error}");
                    HMENU::default()
                }
            };
        Menu { hmenu }
    }
    pub(crate) fn sub_menu(&self, npos: i32) -> HMENU {
        if self.hmenu.is_invalid() {
            return HMENU::default();
        }
        unsafe { GetSubMenu(self.hmenu, npos) }
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        if !self.hmenu.is_invalid() {
            unsafe {
                let _ = DestroyMenu(self.hmenu);
            }
        }
    }
}
