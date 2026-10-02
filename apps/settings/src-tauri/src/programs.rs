// SPDX-License-Identifier: GPL-3.0-or-later

//! The programs the 各程式 page offers to pick from: what the Start menu
//! starts, and what has a window open, which also covers programs whose
//! shortcut starts a launcher rather than the program itself.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{env, fs, ptr};

use base64::prelude::{BASE64_STANDARD, Engine as _};
use serde::Serialize;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
    STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SHFILEINFOW, SHGFI_DISPLAYNAME, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW,
    ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetIconInfo, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowThreadProcessId, HICON, ICONINFO, IsWindowVisible,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, Interface, PWSTR};

#[derive(Serialize)]
pub struct Program {
    /// As the Start menu shows it, or else the executable's file name.
    name: String,
    /// What the IME matches the program by.
    exe: String,
    /// A PNG data URL.
    icon: Option<String>,
}

/// A program by its name, its executable and the file whose icon it shows:
/// the shortcut, which may pick another than the executable's.
type Found = (String, PathBuf, PathBuf);

#[tauri::command]
pub async fn list_programs() -> Result<Vec<Program>, String> {
    tauri::async_runtime::spawn_blocking(programs)
        .await
        .map_err(|error| error.to_string())
}

fn programs() -> Vec<Program> {
    // A pool thread, perhaps initialized by an earlier call: fine either way.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let mut found = vec![];
    for root in [env::var_os("ProgramData"), env::var_os("APPDATA")]
        .into_iter()
        .flatten()
    {
        let start_menu = PathBuf::from(root).join(r"Microsoft\Windows\Start Menu\Programs");
        shortcuts(&start_menu, &mut found);
    }
    found.extend(windowed());
    // The Start menu's name wins: it comes first.
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter_map(|(name, path, shown)| {
            let exe = path.file_name()?.to_str()?.to_owned();
            seen.insert(exe.to_lowercase()).then(|| Program {
                name,
                exe,
                icon: icon(&shown),
            })
        })
        .collect()
}

/// Each shortcut to a program under `dir`, by the name the Start menu shows.
fn shortcuts(dir: &Path, found: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            shortcuts(&path, found);
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
            && let Some(target) = target(&path)
        {
            found.push((display_name(&path), target, path));
        }
    }
}

fn target(link: &Path) -> Option<PathBuf> {
    let mut buf = [0u16; MAX_PATH as usize];
    unsafe {
        let shell_link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        shell_link
            .cast::<IPersistFile>()
            .ok()?
            .Load(&HSTRING::from(link.as_os_str()), STGM_READ)
            .ok()?;
        // Empty for a Store app's shortcut, which names no file.
        shell_link.GetPath(&mut buf, ptr::null_mut(), 0).ok()?;
    }
    let path = PathBuf::from(from_wide(&buf));
    let name = path.file_name()?.to_str()?.to_lowercase();
    // Uninstallers sit beside the programs they remove.
    (name.ends_with(".exe") && !name.contains("unins")).then_some(path)
}

/// Localized, as for 小畫家, and without .lnk.
fn display_name(link: &Path) -> String {
    let mut info = SHFILEINFOW::default();
    unsafe {
        SHGetFileInfoW(
            &HSTRING::from(link.as_os_str()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_DISPLAYNAME,
        );
    }
    let name = from_wide(&info.szDisplayName);
    if name.is_empty() {
        link.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        name
    }
}

/// The programs with a window Alt+Tab would show, named by their files.
fn windowed() -> Vec<Found> {
    unsafe extern "system" fn collect(hwnd: HWND, windows: LPARAM) -> BOOL {
        unsafe { (*(windows.0 as *mut Vec<HWND>)).push(hwnd) };
        true.into()
    }
    let mut windows = Vec::<HWND>::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&raw mut windows as isize));
    }
    windows
        .into_iter()
        .filter(|&hwnd| switchable(hwnd))
        .filter_map(|hwnd| {
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid == std::process::id() {
                return None;
            }
            let path = image_path(pid)?;
            // A Store app's frame; the app itself types in another process,
            // which only its shortcut would name.
            if path
                .file_name()?
                .eq_ignore_ascii_case("ApplicationFrameHost.exe")
            {
                return None;
            }
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some((name, path.clone(), path))
        })
        .collect()
}

fn switchable(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        IsWindowVisible(hwnd).as_bool()
            && GetWindow(hwnd, GW_OWNER).is_err()
            && GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
            && GetWindowTextLengthW(hwnd) > 0
            // Hidden by the shell: other desktops' windows, suspended apps.
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                (&raw mut cloaked).cast(),
                size_of::<u32>() as u32,
            )
            .is_ok()
            && cloaked == 0
    }
}

fn image_path(pid: u32) -> Option<PathBuf> {
    let mut buf = vec![0u16; 32768];
    let mut len = buf.len() as u32;
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
    }
    Some(PathBuf::from(String::from_utf16_lossy(
        &buf[..len as usize],
    )))
}

/// The icon Explorer shows for `path`, without a shortcut's arrow.
fn icon(path: &Path) -> Option<String> {
    let mut info = SHFILEINFOW::default();
    let (width, height, rgba) = unsafe {
        SHGetFileInfoW(
            &HSTRING::from(path.as_os_str()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if info.hIcon.is_invalid() {
            return None;
        }
        let rgba = icon_rgba(info.hIcon);
        let _ = DestroyIcon(info.hIcon);
        rgba?
    };
    let mut png = vec![];
    {
        let mut encoder = png::Encoder::new(&mut png, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(png)
    ))
}

/// Width, height and RGBA rows, top first.
fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut info).ok()? };
    let color = bitmap_bgra(info.hbmColor);
    let mask = bitmap_bgra(info.hbmMask);
    unsafe {
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    // None for a black-and-white icon, whose mask holds both halves.
    let (width, height, mut pixels) = color?;
    // Icons from before alpha channels mark what's see-through in the mask.
    if pixels.chunks(4).all(|pixel| pixel[3] == 0) {
        let (_, _, mask) = mask?;
        for (pixel, bit) in pixels.chunks_mut(4).zip(mask.chunks(4)) {
            pixel[3] = if bit[0] == 0 { 255 } else { 0 };
        }
    }
    for pixel in pixels.chunks_mut(4) {
        pixel.swap(0, 2);
    }
    Some((width, height, pixels))
}

/// Width, height and 32-bit BGRA rows, top first.
fn bitmap_bgra(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    if bitmap.is_invalid() {
        return None;
    }
    let mut size = BITMAP::default();
    unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&raw mut size).cast()),
        )
    };
    let (width, height) = (size.bmWidth, size.bmHeight);
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top row first.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let lines = unsafe {
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        lines
    };
    (height > 0 && lines == height).then_some((width as u32, height as u32, pixels))
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}
