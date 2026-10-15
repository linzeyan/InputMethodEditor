// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write as _,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use chewing_tip_core::shell::user_dir;
use logforth::{
    Append, Diagnostic, Error, Layout,
    kv::{Key, Value, Visitor},
    layout::PlainTextLayout,
    record::{Level, Record},
};
use windows::Win32::System::{
    Diagnostics::Debug::{IsDebuggerPresent, OutputDebugStringW},
    SystemInformation::GetLocalTime,
};
use windows_core::HSTRING;

#[derive(Debug)]
pub(crate) struct WinDbg {
    layout: Box<dyn Layout>,
}

impl Default for WinDbg {
    fn default() -> Self {
        Self {
            layout: Box::new(PlainTextLayout::default()),
        }
    }
}

impl Append for WinDbg {
    fn append(&self, record: &Record, diags: &[Box<dyn Diagnostic>]) -> Result<(), Error> {
        if is_debugger_present() {
            let mut bytes = self.layout.format(record, diags)?;
            bytes.push(b'\n');
            let text = String::from_utf8_lossy(&bytes);
            output_debug_string(&text);
        }
        Ok(())
    }
    fn flush(&self) -> Result<(), Error> {
        Ok(())
    }
}

/// Above this ime.log is moved to ime.old.log, so the two stay under 2 MiB.
const MAX_LOG_SIZE: u64 = 1 << 20;

/// The level the user chose; the debugger may lower `log::max_level` below it.
static FILE_LEVEL: AtomicU8 = AtomicU8::new(Level::Warn as u8);

/// Appends to `%AppData%\InputMethodEditor\ime.log`, the log a user can
/// send. Every process with a text field loads the text service, so the
/// file is opened for each record rather than held: appends from many
/// processes then interleave whole lines, and any of them can rotate it.
#[derive(Debug)]
pub(crate) struct FileLog;

impl Append for FileLog {
    fn append(&self, record: &Record, _diags: &[Box<dyn Diagnostic>]) -> Result<(), Error> {
        if (record.level() as u8) < FILE_LEVEL.load(Ordering::Relaxed) {
            return Ok(());
        }
        let Ok(dir) = user_dir() else {
            return Ok(());
        };
        let path = dir.join("ime.log");
        // ponytail: two processes rotating at once lose one ime.old.log;
        // a lock file if the log ever needs to be complete.
        if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_SIZE) {
            let _ = fs::rename(&path, dir.join("ime.old.log"));
        }
        let t = unsafe { GetLocalTime() };
        let mut line = format!(
            "{}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} {} {} {:>5} {}: {}:{} {}",
            t.wYear,
            t.wMonth,
            t.wDay,
            t.wHour,
            t.wMinute,
            t.wSecond,
            t.wMilliseconds,
            process_name(),
            std::process::id(),
            record.level().name(),
            record.target(),
            record.filename(),
            record.line().unwrap_or_default(),
            record.payload(),
        );
        record.key_values().visit(&mut KvWriter(&mut line))?;
        line.push_str("\r\n");
        // A log that cannot be written must not fail what is being logged.
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = file.write_all(line.as_bytes());
        }
        Ok(())
    }
    fn flush(&self) -> Result<(), Error> {
        Ok(())
    }
}

struct KvWriter<'a>(&'a mut String);

impl Visitor for KvWriter<'_> {
    fn visit(&mut self, key: Key, value: Value) -> Result<(), Error> {
        let _ = write!(self.0, " {key}={value}");
        Ok(())
    }
}

fn process_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.file_name()?.to_string_lossy().into_owned()))
            .unwrap_or_default()
    })
}

/// Sets the least severe level written to ime.log, one of error, warn,
/// info, debug and trace; anything else is warn.
pub(crate) fn set_file_level(level: &str) {
    let file: Level = level.parse().unwrap_or(Level::Warn);
    FILE_LEVEL.store(file as u8, Ordering::Relaxed);
    let file: log::LevelFilter = level.parse().unwrap_or(log::LevelFilter::Warn);
    // The debugger keeps the level it was given at load, whatever the file's.
    let debugger = if !is_debugger_present() {
        log::LevelFilter::Off
    } else if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    log::set_max_level(file.max(debugger));
}

pub(crate) fn output_debug_string(text: &str) {
    unsafe {
        OutputDebugStringW(&HSTRING::from(text));
    }
}

pub(crate) fn is_debugger_present() -> bool {
    unsafe { IsDebuggerPresent().as_bool() }
}
