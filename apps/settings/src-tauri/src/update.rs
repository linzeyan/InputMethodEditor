// SPDX-License-Identifier: GPL-3.0-or-later

//! `--check-update`, which a scheduled task runs at logon and daily: once
//! every `update_check_days` it looks up the latest GitHub release and offers
//! to install it if it is newer.

use std::error::Error;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fs, thread};

use chewing_tip_core::PRODUCT_NAME;
use chewing_tip_core::config::Config;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, GetExitCodeProcess, INFINITE, WaitForSingleObject,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
    MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MESSAGEBOX_RESULT, MESSAGEBOX_STYLE, MessageBoxW,
    SW_HIDE,
};
use windows::core::{HSTRING, PCWSTR, w};
use windows_registry::CURRENT_USER;

const LATEST_RELEASE: &str =
    "https://api.github.com/repos/linzeyan/InputMethodEditor/releases/latest";
const MSI: &str = "InputMethodEditor.msi";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    /// `sha256:<hex>`, which GitHub computes on upload.
    digest: Option<String>,
}

pub fn check_update() {
    // Nobody is there to read why a lookup failed; the next run tries again.
    let _ = check();
}

fn check() -> Result<()> {
    let key = CURRENT_USER.create(format!(r"Software\{PRODUCT_NAME}"))?;
    // Kept out of Config: the settings app saves all of it, stale copy included.
    let last = key.get_u64("LastUpdateCheck").unwrap_or(0);
    let config = Config::from_reg().unwrap_or_default().chewing_tsf;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if !config.check_update || !due(last, now, config.update_check_days) {
        return Ok(());
    }
    let release: Release = serde_json::from_slice(&curl(&[LATEST_RELEASE])?)?;
    key.set_u64("LastUpdateCheck", now)?;

    let current = env!("CARGO_PKG_VERSION");
    let latest = release.tag_name.trim_start_matches('v');
    if !newer(latest, current) {
        return Ok(());
    }
    let Some(asset) = release.assets.iter().find(|asset| asset.name == MSI) else {
        return Ok(());
    };
    let question = format!(
        "InputMethodEditor {latest} 已經推出，目前使用的是 {current}。\n\n要下載並安裝嗎？"
    );
    if show(&question, MB_YESNO | MB_ICONINFORMATION) != IDYES {
        return Ok(());
    }
    if let Err(error) = install(asset, latest) {
        show(&format!("無法更新：{error}"), MB_ICONERROR);
    }
    Ok(())
}

fn due(last: u64, now: u64, days: i32) -> bool {
    now.saturating_sub(last) >= days.clamp(1, 30) as u64 * 24 * 60 * 60
}

/// Compares dotted numbers; anything else is never newer.
fn newer(latest: &str, current: &str) -> bool {
    let parse = |version: &str| -> Option<Vec<u32>> {
        version.split('.').map(|part| part.parse().ok()).collect()
    };
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

fn install(asset: &Asset, version: &str) -> Result<()> {
    let path = env::temp_dir().join(format!("InputMethodEditor-{version}.msi"));
    curl(&["-o", &path.to_string_lossy(), &asset.browser_download_url])?;
    let digest = format!("sha256:{:x}", Sha256::digest(fs::read(&path)?));
    if asset.digest.as_deref() != Some(digest.as_str()) {
        let _ = fs::remove_file(&path);
        return Err("下載的檔案和 GitHub 上記載的不符".into());
    }
    // Every running program has the DLL loaded, and /passive still stops at
    // the files-in-use dialog for them. /qn takes its Ignore instead (programs
    // started afterwards load the new DLL) but can't ask for administrator
    // rights, so msiexec starts elevated and UAC is all that shows.
    // No restart: the old DLLs go at the next one.
    let parameters = format!("/i \"{}\" /qn /norestart", path.display());
    let code = run_elevated("msiexec.exe", &parameters);
    let _ = fs::remove_file(&path);
    match code? {
        // 3010: installed, with the old DLLs left for the restart.
        None | Some(0 | 3010) => Ok(()),
        Some(code) => Err(format!("安裝程式結束代碼 {code}").into()),
    }
}

/// Runs `file` as administrator and waits for its exit code; None if the
/// UAC prompt was declined.
fn run_elevated(file: &str, parameters: &str) -> Result<Option<u32>> {
    let file = HSTRING::from(file);
    let parameters = HSTRING::from(parameters);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe {
        if let Err(error) = ShellExecuteExW(&mut info) {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                return Ok(None);
            }
            return Err(error.into());
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0;
        let exited = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
        exited?;
        Ok(Some(code))
    }
}

/// Windows' own curl, whose TLS is the system's: certificates and all.
fn curl(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("curl.exe")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--max-time", "600"])
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(output.stdout)
}

fn show(text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    wait_until_idle();
    unsafe {
        // Windows keeps what the task scheduler starts from taking the
        // foreground, leaving the box behind the window in use. Sharing that
        // window's input lets it come forward.
        // ponytail: shared while the box is up, so a hung program in front
        // would stall the box too; detach on the box's activation if that bites.
        let this = GetCurrentThreadId();
        let front = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached =
            front != 0 && front != this && AttachThreadInput(this, front, true).as_bool();
        let result = MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(PRODUCT_NAME),
            style | MB_SETFOREGROUND | MB_TOPMOST,
        );
        if attached {
            let _ = AttachThreadInput(this, front, false);
        }
        result
    }
}

/// Waits for two seconds without a key press or click: taking the keyboard,
/// the box would read the next key of someone typing (Space, Enter, y) as an
/// answer.
fn wait_until_idle() {
    loop {
        let mut input = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        let idle = unsafe {
            if !GetLastInputInfo(&mut input).as_bool() {
                return;
            }
            GetTickCount().wrapping_sub(input.dwTime)
        };
        if idle >= 2000 {
            return;
        }
        thread::sleep(Duration::from_millis(u64::from(2000 - idle)));
    }
}

#[cfg(test)]
mod tests {
    use super::{due, newer};

    #[test]
    fn newer_compares_numbers_not_text() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "0.3.0"));
        assert!(!newer("0.3.0", "0.3.0"));
        assert!(!newer("0.2.9", "0.3.0"));
        // A tag that isn't a plain version is never offered.
        assert!(!newer("0.4.0-rc1", "0.3.0"));
    }

    #[test]
    fn due_after_the_interval_kept_to_one_to_thirty_days() {
        let day = 24 * 60 * 60;
        assert!(due(0, 100 * day, 7));
        assert!(!due(10 * day, 16 * day, 7));
        assert!(due(10 * day, 17 * day, 7));
        // Out of range, as a hand-edited registry might have it.
        assert!(due(10 * day, 11 * day, 0));
        assert!(!due(10 * day, 39 * day, 365));
        assert!(due(10 * day, 40 * day, 365));
    }
}
