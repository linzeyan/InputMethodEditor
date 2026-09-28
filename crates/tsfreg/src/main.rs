// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

#![windows_subsystem = "windows"]

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use chewing_tip_core::{PRODUCT_NAME, SETTINGS_EXE, SETTINGS_SCHEME};
use chewing_tip_core::config::grant_app_container_access;
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        Globalization::*,
        Security::Authorization::{SE_FILE_OBJECT, SE_REGISTRY_KEY},
        Storage::FileSystem::{FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, GetDriveTypeW},
        System::{
            Com::*,
            Registry::{KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS},
            WindowsProgramming::DRIVE_REMOTE,
        },
        UI::{Input::KeyboardAndMouse::HKL, TextServices::*, WindowsAndMessaging::*},
    },
    core::*,
};
use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

// https://learn.microsoft.com/en-us/windows/win32/tsf/installlayoutortip
windows::core::link!("input.dll" "system" fn InstallLayoutOrTip(psz: *const u16, dwFlags: u32));
const ILOT_INSTALL: u32 = 0x00000000;

const CHEWING_TSF_CLSID: GUID = GUID::from_u128(0xE0C45601_7E8F_4FEF_9871_8B0C785B9B48);
const CHEWING_TSF_CLSID_STR: &str = "{E0C45601-7E8F-4FEF-9871-8B0C785B9B48}";
const CHEWING_ZH_TW_PROFILE_GUID: GUID = GUID::from_u128(0x65C3BF2B_7BF5_4B82_8D03_1EC109E380C4);
const CHEWING_TIP_DESC: PCWSTR =
    w!("0x0404:{E0C45601-7E8F-4FEF-9871-8B0C785B9B48}{65C3BF2B-7BF5-4B82-8D03-1EC109E380C4}");

const CATEGORIES: [GUID; 7] = [
    GUID_TFCAT_TIP_KEYBOARD,
    GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT,
    GUID_TFCAT_TIPCAP_UIELEMENTENABLED,
    GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
    GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
    GUID_TFCAT_TIPCAP_COMLESS,
];

type Outcome = std::result::Result<Option<&'static str>, Box<dyn Error>>;
type Step = std::result::Result<(), Box<dyn Error>>;

fn zh_tw_langid() -> u16 {
    let lcid = unsafe { LocaleNameToLCID(w!("zh-TW"), 0) };
    if matches!(lcid, 0 | 0x0C00 | 0x1000) {
        0x404
    } else {
        lcid as u16
    }
}

/// Program Files is only writable by administrators. Anywhere else, any
/// process running as this user could swap the DLL that elevated apps load.
fn is_admin_protected(root: &Path) -> bool {
    let root = root.to_string_lossy().to_lowercase();
    ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|var| env::var(var).ok())
        .any(|dir| root.starts_with(&format!("{}\\", dir.to_lowercase())))
}

fn is_network_drive(root: &Path) -> bool {
    let Some(volume) = root.ancestors().last() else {
        return false;
    };
    unsafe { GetDriveTypeW(&HSTRING::from(volume.as_os_str())) == DRIVE_REMOTE }
}

fn register_com_server(view: REG_SAM_FLAGS, dll: &Path) -> windows_registry::Result<()> {
    let key = LOCAL_MACHINE
        .options()
        .create()
        .write()
        .access(view.0)
        .open(format!(
            r"Software\Classes\CLSID\{CHEWING_TSF_CLSID_STR}\InprocServer32"
        ))?;
    key.set_string("", dll.to_string_lossy())?;
    key.set_string("ThreadingModel", "Apartment")
}

fn register_settings_scheme(exe: &Path) -> windows_registry::Result<()> {
    let key = LOCAL_MACHINE.create(format!(r"Software\Classes\{SETTINGS_SCHEME}"))?;
    key.set_string("", format!("URL:{PRODUCT_NAME} 設定"))?;
    key.set_string("URL Protocol", "")?;
    key.create(r"shell\open\command")?
        .set_string("", format!("\"{}\" \"%1\"", exe.display()))
}

/// On ARM64 Windows, x64 and ARM64 processes read the same 64-bit registry
/// view, so the ARM64 package registers its native DLL there instead of x64.
const NATIVE_DIR: &str = if cfg!(target_arch = "aarch64") {
    "arm64"
} else {
    "x64"
};

fn register(root: &Path) -> Outcome {
    let native_dll = root.join(NATIVE_DIR).join("chewing_tip.dll");
    let x86_dll = root.join("x86").join("chewing_tip.dll");
    let icon = root.join(format!("{PRODUCT_NAME}.ico"));
    let settings = root.join(NATIVE_DIR).join(SETTINGS_EXE);
    for path in [&native_dll, &x86_dll, &icon, &settings] {
        if !path.exists() {
            return Err(format!("找不到 {}", path.display()).into());
        }
    }
    // Every app loads the DLL, including elevated and AppContainer ones that
    // may not reach the share; and network folders take no ACL, which
    // register_machine needs.
    if is_network_drive(root) {
        return Err(format!(
            "資料夾在網路磁碟機上：\n{}\n\n\
             請把整個資料夾複製到這台電腦（建議 C:\\Program Files\\{PRODUCT_NAME}），\
             再從那裡執行 register.bat。",
            root.display()
        )
        .into());
    }

    if !is_admin_protected(root) {
        let answer = message_box(
            &format!(
                "輸入法資料夾不在 Program Files：\n{}\n\n\
                 註冊後，連以系統管理員身分執行的程式也會載入這裡的 DLL。\
                 任何以你的身分執行的程式都能替換它，進而取得系統管理員權限。\n\n\
                 仍要註冊嗎？",
                root.display()
            ),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2,
        );
        if answer != IDOK {
            return Ok(None);
        }
    }

    register_machine(root)?;
    register_user()?;
    Ok(Some(
        "註冊完成。已開啟的程式要重新開啟後才能使用這個輸入法。",
    ))
}

/// The part that needs administrator rights; the MSI runs it as SYSTEM.
fn register_machine(root: &Path) -> Step {
    let native_dll = root.join(NATIVE_DIR).join("chewing_tip.dll");
    let x86_dll = root.join("x86").join("chewing_tip.dll");
    let icon = root.join(format!("{PRODUCT_NAME}.ico"));

    // AppContainer processes (Start menu search, Store apps) cannot load the
    // DLL or dictionary otherwise. Program Files already grants this. First,
    // so a folder that takes no ACL leaves nothing half registered.
    let root_path = HSTRING::from(root.as_os_str());
    grant_app_container_access(
        PCWSTR(root_path.as_ptr()),
        SE_FILE_OBJECT,
        (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE).0,
    )?;

    // Both views: 32-bit apps load the x86 DLL through the WOW64 registry.
    register_com_server(KEY_WOW64_64KEY, &native_dll)?;
    register_com_server(KEY_WOW64_32KEY, &x86_dll)?;
    register_settings_scheme(&root.join(NATIVE_DIR).join(SETTINGS_EXE))?;

    unsafe {
        let input_processor_profile_mgr: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let pw_icon_path = icon.to_string_lossy().encode_utf16().collect::<Vec<_>>();
        input_processor_profile_mgr.RegisterProfile(
            &CHEWING_TSF_CLSID,
            zh_tw_langid(),
            &CHEWING_ZH_TW_PROFILE_GUID,
            &PRODUCT_NAME.encode_utf16().collect::<Vec<_>>(),
            &pw_icon_path,
            0,
            HKL::default(),
            0,
            false,
            0,
        )?;

        let category_manager: ITfCategoryMgr =
            CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for tfcat in &CATEGORIES {
            category_manager.RegisterCategory(&CHEWING_TSF_CLSID, tfcat, &CHEWING_TSF_CLSID)?;
        }
    }

    #[cfg(feature = "nightly")]
    {
        // Enable user-mode minidump for debug build
        let _ = LOCAL_MACHINE
            .create("SOFTWARE\\Microsoft\\Windows\\Windows Error Reporting\\LocalDumps");
    }
    Ok(())
}

/// The part for the user who installs; the MSI runs it as that user.
fn register_user() -> Step {
    // The DLL only reads settings, so the key must exist and be readable from
    // AppContainer processes before any setting is written.
    CURRENT_USER.create(format!(r"Software\{PRODUCT_NAME}"))?;
    let key_path = HSTRING::from(format!(r"CURRENT_USER\Software\{PRODUCT_NAME}"));
    grant_app_container_access(PCWSTR(key_path.as_ptr()), SE_REGISTRY_KEY, KEY_READ.0)?;

    unsafe {
        InstallLayoutOrTip(CHEWING_TIP_DESC.as_ptr(), ILOT_INSTALL);
    }
    Ok(())
}

/// Best effort: keeps going after a failed step so a half-registered state
/// can still be cleaned up, then reports every step that failed.
fn unregister() -> Outcome {
    let mut failures: Vec<String> = vec![];
    unsafe {
        match CoCreateInstance::<_, ITfCategoryMgr>(
            &CLSID_TF_CategoryMgr,
            None,
            CLSCTX_INPROC_SERVER,
        ) {
            Ok(category_manager) => {
                for tfcat in &CATEGORIES {
                    let _ = category_manager.UnregisterCategory(
                        &CHEWING_TSF_CLSID,
                        tfcat,
                        &CHEWING_TSF_CLSID,
                    );
                }
            }
            Err(error) => failures.push(format!("類別：{error}")),
        }

        // Don't uninstall the layout with InstallLayoutOrTip. If the last layout
        // of a language is uninstalled then Windows changes the system locale.
        // Ref: https://github.com/chewing/windows-chewing-tsf/issues/553
        let profile = CoCreateInstance::<_, ITfInputProcessorProfileMgr>(
            &CLSID_TF_InputProcessorProfiles,
            None,
            CLSCTX_INPROC_SERVER,
        )
        .and_then(|mgr| {
            mgr.UnregisterProfile(
                &CHEWING_TSF_CLSID,
                zh_tw_langid(),
                &CHEWING_ZH_TW_PROFILE_GUID,
                0,
            )
        });
        if let Err(error) = profile {
            failures.push(format!("輸入法設定檔：{error}"));
        }
    }

    // UnregisterProfile and UnregisterCategory delete only the leaf entries,
    // leaving the TIP's key tree behind in both registry views.
    for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        for (parent, name, step) in [
            (r"Software\Classes\CLSID", CHEWING_TSF_CLSID_STR, "COM 註冊"),
            (r"SOFTWARE\Microsoft\CTF\TIP", CHEWING_TSF_CLSID_STR, "TSF 登錄"),
            (r"Software\Classes", SETTINGS_SCHEME, "設定程式連結"),
        ] {
            let removed = LOCAL_MACHINE
                .options()
                .write()
                .access(view.0)
                .open(parent)
                .and_then(|key| key.remove_tree(name));
            match removed {
                // Already gone, e.g. a view the registration never wrote to.
                Err(error) if error.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) => {}
                Err(error) => failures.push(format!("{step}：{error}")),
                Ok(()) => {}
            }
        }
    }

    if failures.is_empty() {
        Ok(Some(
            "已解除註冊。使用者詞庫和設定仍保留；已開啟的程式要重新開啟後才會完全移除。",
        ))
    } else {
        Err(format!(
            "部分步驟失敗（之前未註冊時屬正常）：\n{}",
            failures.join("\n")
        )
        .into())
    }
}

fn message_box(text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(PRODUCT_NAME),
            style,
        )
    }
}

/// Returns the IME folder, which is where tsfreg itself is.
fn init() -> std::result::Result<PathBuf, Box<dyn Error>> {
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()? };
    Ok(env::current_exe()?
        .parent()
        .ok_or("無法判斷輸入法資料夾")?
        .to_path_buf())
}

fn run(verb: Option<&str>) -> Outcome {
    let root = init()?;
    match verb {
        Some("register") => register(&root),
        Some("unregister") => unregister(),
        _ => Err("用法：tsfreg register | tsfreg unregister".into()),
    }
}

/// Steps of the MSI, which has already asked for elevation and shows its own
/// errors. No message box: as SYSTEM it would hang the install out of sight.
fn run_msi_step(step: &str) -> Step {
    let root = init()?;
    match step {
        "register" => register_machine(&root),
        "register-user" => register_user(),
        "unregister" => unregister().map(drop),
        _ => Err(format!("unknown MSI step: {step}").into()),
    }
}

fn main() -> ExitCode {
    let verb = env::args().nth(1);
    if let Some(step) = verb.as_deref().and_then(|verb| verb.strip_prefix("msi-")) {
        return match run_msi_step(step) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        };
    }
    match run(verb.as_deref()) {
        Ok(Some(done)) => {
            message_box(done, MB_OK | MB_ICONINFORMATION);
            ExitCode::SUCCESS
        }
        // Cancelled by the user.
        Ok(None) => ExitCode::FAILURE,
        Err(error) => {
            message_box(&format!("失敗：{error}"), MB_OK | MB_ICONERROR);
            ExitCode::FAILURE
        }
    }
}
