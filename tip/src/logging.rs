// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) 2026 Kan-Ru Chen

use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
    time::SystemTime,
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

/// Days of logs kept: a problem is often noticed, or reported, days later.
const KEEP_DAYS: u64 = 14;
/// All the logs together; trace fills a day fast, and the oldest go first.
const MAX_TOTAL_SIZE: u64 = 100 << 20;
/// A day's file above this goes on in ime-<date>.1.log, .2 and so on, so
/// that the total limit drops part of a day, not all of it.
const MAX_FILE_SIZE: u64 = 16 << 20;
/// How long ime.log and ime.old.log, from before a file a day, stay unwritten
/// before they are deleted.
const LEGACY_KEEP_SECS: u64 = 24 * 60 * 60;

/// The level the user chose; the debugger may lower `log::max_level` below it.
static FILE_LEVEL: AtomicU8 = AtomicU8::new(Level::Warn as u8);

/// The day being written and its part, to clean up when the day changes.
static CURRENT: Mutex<Option<((u16, u16, u16), u32)>> = Mutex::new(None);

/// Appends to `%AppData%\InputMethodEditor\ime-<date>.log`, the log a user
/// can send. Every process with a text field loads the text service, so the
/// file is opened for each record rather than held: appends from many
/// processes then interleave whole lines. A file per day is never renamed,
/// so no process moves a file another is about to append to.
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
        let t = unsafe { GetLocalTime() };
        let day = (t.wYear, t.wMonth, t.wDay);
        let path = {
            let mut current = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
            let mut part = match *current {
                Some((d, part)) if d == day => part,
                _ => {
                    clean_up(&dir, KEEP_DAYS * 24 * 60 * 60, MAX_TOTAL_SIZE);
                    // Not from 0: the total limit may have deleted the day's
                    // first parts, which a process starting there made again.
                    last_part(&dir, day)
                }
            };
            let mut path = dir.join(log_name(day, part));
            while fs::metadata(&path).is_ok_and(|m| m.len() > MAX_FILE_SIZE) {
                part += 1;
                path = dir.join(log_name(day, part));
            }
            *current = Some((day, part));
            path
        };
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

fn log_name((year, month, day): (u16, u16, u16), part: u32) -> String {
    match part {
        0 => format!("ime-{year}-{month:02}-{day:02}.log"),
        _ => format!("ime-{year}-{month:02}-{day:02}.{part}.log"),
    }
}

/// The highest part of `day` in `dir`, 0 when there is none.
fn last_part(dir: &Path, day: (u16, u16, u16)) -> u32 {
    let first = log_name(day, 0);
    let stem = first.trim_end_matches(".log");
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let rest = name.strip_prefix(stem)?.strip_suffix(".log")?;
            match rest {
                "" => Some(0),
                _ => rest.strip_prefix('.')?.parse().ok(),
            }
        })
        .max()
        .unwrap_or(0)
}

/// Deletes the logs older than `keep_secs`, then the oldest beyond
/// `max_total`, and the ime.log and ime.old.log written before there was a
/// file per day. Other processes may be deleting the same files.
fn clean_up(dir: &Path, keep_secs: u64, max_total: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut kept = vec![];
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("ime") || !name.ends_with(".log") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let modified = meta.modified().unwrap_or(now);
        let age = now.duration_since(modified).unwrap_or_default();
        if !name.starts_with("ime-") {
            // A program started before an update still runs the old DLL and
            // writes ime.log: deleted at once, every process starting made
            // it again. Left a day, it goes once those programs are closed.
            if age.as_secs() > LEGACY_KEEP_SECS {
                let _ = fs::remove_file(entry.path());
            }
        } else if age.as_secs() > keep_secs {
            let _ = fs::remove_file(entry.path());
        } else {
            kept.push((modified, meta.len(), entry.path()));
        }
    }
    kept.sort_by(|a, b| b.0.cmp(&a.0));
    let mut total = 0;
    for (_, len, path) in kept {
        total += len;
        if total > max_total {
            let _ = fs::remove_file(path);
        }
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// A problem is reported days after it happened, so the days before stay;
    /// what is too old, or over the limit, goes oldest first, and nothing but
    /// the text service's own logs is touched.
    #[test]
    fn clean_up_keeps_recent_days_within_the_limit() {
        let dir = std::env::temp_dir().join(format!("ime-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let day = 24 * 60 * 60;
        let write = |name: &str, days_ago: u64, len: usize| {
            let path = dir.join(name);
            fs::write(&path, vec![b'x'; len]).unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            let when = SystemTime::now() - Duration::from_secs(days_ago * day + 60);
            file.set_modified(when).unwrap();
        };
        write("ime-2026-10-19.log", 0, 10);
        write("ime-2026-10-18.1.log", 1, 10);
        write("ime-2026-10-18.log", 1, 10);
        write("ime-2026-10-10.log", 9, 10);
        write("ime-2026-09-01.log", 48, 10);
        write("ime.log", 0, 10);
        write("ime.old.log", 1, 10);
        write("update.log", 48, 10);

        clean_up(&dir, 14 * day, 35);

        let mut left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        let _ = fs::remove_dir_all(&dir);
        // 2026-10-10 is within the days kept but over the 35 bytes. ime.log,
        // written just now by a program still on the old DLL, stays: deleted,
        // that program would make it again for the next process to delete.
        assert_eq!(
            left,
            [
                "ime-2026-10-18.1.log",
                "ime-2026-10-18.log",
                "ime-2026-10-19.log",
                "ime.log",
                "update.log"
            ]
        );
    }

    #[test]
    fn a_day_goes_on_in_numbered_parts() {
        assert_eq!(log_name((2026, 10, 9), 0), "ime-2026-10-09.log");
        assert_eq!(log_name((2026, 10, 9), 2), "ime-2026-10-09.2.log");
    }

    /// A process starting late in a day writes after the other processes,
    /// even once the total limit deleted the day's first parts: starting
    /// from part 0, it made that file again.
    #[test]
    fn a_process_goes_on_from_the_days_last_part() {
        let dir = std::env::temp_dir().join(format!("ime-part-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in [
            "ime-2026-10-19.12.log",
            "ime-2026-10-20.3.log",
            "ime-2026-10-20.11.log",
            "ime-2026-10-20.x.log",
            "update.log",
        ] {
            fs::write(dir.join(name), "x").unwrap();
        }
        let today = last_part(&dir, (2026, 10, 20));
        let tomorrow = last_part(&dir, (2026, 10, 21));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!((today, tomorrow), (11, 0));
    }
}
