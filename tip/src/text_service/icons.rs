use std::{ffi::c_void, sync::atomic::Ordering};

use windows::Win32::{
    Foundation::HINSTANCE,
    UI::WindowsAndMessaging::{HICON, LoadIconW},
};
use windows_core::PCWSTR;

use crate::{
    com::G_HINSTANCE,
    text_service::resources::{
        IDI_CHI, IDI_CHI_DARK, IDI_ENG, IDI_ENG_DARK, IDI_SIMP, IDI_SIMP_DARK,
    },
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct IconSet {
    pub(crate) dark: HICON,
    pub(crate) light: HICON,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LangIconSet {
    pub(crate) tc: IconSet,
    pub(crate) sc: IconSet,
    pub(crate) en: IconSet,
}

impl LangIconSet {
    pub(crate) fn load() -> LangIconSet {
        let g_hinstance = HINSTANCE(G_HINSTANCE.load(Ordering::Relaxed) as *mut c_void);
        LangIconSet {
            tc: IconSet {
                dark: load_icon(g_hinstance, IDI_CHI_DARK),
                light: load_icon(g_hinstance, IDI_CHI),
            },
            sc: IconSet {
                dark: load_icon(g_hinstance, IDI_SIMP_DARK),
                light: load_icon(g_hinstance, IDI_SIMP),
            },
            en: IconSet {
                dark: load_icon(g_hinstance, IDI_ENG_DARK),
                light: load_icon(g_hinstance, IDI_ENG),
            },
        }
    }
}

fn load_icon(hinst: HINSTANCE, icon_id: u32) -> HICON {
    unsafe { LoadIconW(Some(hinst), PCWSTR::from_raw(icon_id as *const u16)).unwrap_or_default() }
}
