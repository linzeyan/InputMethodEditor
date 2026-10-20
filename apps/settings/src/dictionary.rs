// SPDX-FileCopyrightText: 2025-2026 Chewing Project Authors
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user dictionary. The IME keeps learning into it while this app has it
//! open, so saving applies only what was edited here to the file as it is
//! then, rather than writing back the list as it was loaded.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chewing::{
    dictionary::StringTable,
    editor::zhuyin_layout::{Standard, SyllableEditor},
    input::keymap::{QWERTY_MAP, map_ascii},
    path::SearchPath,
    user::HistoryDict,
    zhuyin::{Syllable, SyllableVec},
};
use chewing_tip_core::config::Config;
use chewing_tip_core::shell::user_dir;
use slint::{ComponentHandle, ModelRc, StandardListViewItem, Timer, TimerMode, VecModel};

use crate::{
    DictionaryWindow, VERSION, bring_forward, center_when_made, error_box, open_file, save_file,
};

const USER_DICT: &str = "user_dict.csv";
const HISTORY_DICT: &str = "history_dict.bin";

/// Where chewing keeps them: a folder named for their format version, inside
/// the IME's user folder.
fn dict_dir() -> Result<PathBuf> {
    SearchPath::from_system_path_and_user_path("", &user_dir()?.to_string_lossy())
        .user_versioned_path()
        .context("no folder for the user dictionary")
}

#[derive(Clone, Default, PartialEq)]
pub(crate) struct Entry {
    word: String,
    bopomofo: String,
    boost: i8,
}

impl Entry {
    fn key(&self) -> (&str, &str) {
        (&self.word, &self.bopomofo)
    }

    /// Rejects what would make chewing drop the whole file when it reads it,
    /// and spells the reading the way chewing writes it, so that an entry
    /// matches the one on disk.
    fn checked(mut self) -> Result<Entry> {
        if self.word.is_empty() || self.word.contains([',', '\r', '\n']) {
            bail!("字詞「{}」不可為空白，也不可有逗號或換行", self.word);
        }
        let bopomofo = self.bopomofo.replace("␣", " ").replace("一", "ㄧ");
        if bopomofo.trim().is_empty() {
            bail!("「{}」的注音不可為空白", self.word);
        }
        let syllables =
            SyllableVec::from_str(&bopomofo.split_whitespace().collect::<Vec<_>>().join(" "))
                .with_context(|| format!("「{}」的注音不正確：{}", self.word, self.bopomofo))?;
        self.bopomofo = syllables
            .iter()
            .map(|syl| syl.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(self)
    }
}

/// Applies to `disk` what `edited` changed from `loaded`, leaving alone the
/// entries the IME changed or learned since. Returns the result and the
/// entries removed.
fn merge(mut disk: Vec<Entry>, loaded: &[Entry], edited: &[Entry]) -> (Vec<Entry>, Vec<Entry>) {
    let kept: HashSet<_> = edited.iter().map(Entry::key).collect();
    let removed: Vec<Entry> = loaded
        .iter()
        .filter(|entry| !kept.contains(&entry.key()))
        .cloned()
        .collect();
    let removed_keys: HashSet<_> = removed.iter().map(Entry::key).collect();
    disk.retain(|entry| !removed_keys.contains(&entry.key()));
    for entry in edited.iter().filter(|entry| !loaded.contains(entry)) {
        match disk.iter_mut().find(|d| d.key() == entry.key()) {
            Some(d) => d.boost = entry.boost,
            None => disk.push(entry.clone()),
        }
    }
    (disk, removed)
}

/// Adds the words `imported` has that `entries` lacks. A word in both keeps
/// the higher preference: then importing each computer's export on the
/// other leaves both with the same dictionary.
fn union(mut entries: Vec<Entry>, imported: Vec<Entry>) -> Vec<Entry> {
    for entry in imported {
        match entries.iter_mut().find(|e| e.key() == entry.key()) {
            Some(e) => e.boost = e.boost.max(entry.boost),
            None => entries.push(entry),
        }
    }
    entries
}

fn read_entries(path: &Path) -> Result<Vec<Entry>> {
    let file = match File::open(path) {
        // The IME creates it when it first starts.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        file => file?,
    };
    let reader = BufReader::new(file);
    let mut entries = vec![];
    for (i, io) in reader.lines().enumerate() {
        let line = io?;
        let mut parts = line.split(',');
        let word = parts
            .next()
            .with_context(|| format!("invalid format at line {i}: {line}"))?
            .to_string();
        let bopomofo = parts
            .next()
            .with_context(|| format!("invalid format at line {i}: {line}"))?
            .to_string();
        let boost = parts
            .next()
            .map(|b| i8::from_str(b).unwrap_or(0).clamp(-100, 100))
            .unwrap_or(0);
        entries.push(Entry {
            word,
            bopomofo,
            boost,
        });
    }
    Ok(entries)
}

/// Writes a new file and renames it over the old one, as chewing does, so
/// the IME never reads half of it.
fn replace_file(path: &Path, write: impl FnOnce(&mut BufWriter<File>) -> Result<()>) -> Result<()> {
    let temp = path.with_extension("tmp");
    let mut writer = BufWriter::new(File::create(&temp)?);
    write(&mut writer)?;
    writer.flush()?;
    drop(writer);
    fs::rename(&temp, path)?;
    Ok(())
}

/// In chewing's format, so an export is also a user_dict.csv, and the
/// user_dict.csv of any chewing install can be imported.
fn write_entries(writer: &mut impl Write, entries: &[Entry]) -> Result<()> {
    for entry in entries {
        writeln!(writer, "{},{},{}", entry.word, entry.bopomofo, entry.boost)?;
    }
    Ok(())
}

/// Removed words also leave the history: chewing still suggests the ones
/// it counted there.
fn forget(path: &Path, removed: &[Entry]) -> Result<()> {
    if removed.is_empty() || !path.exists() {
        return Ok(());
    }
    let history = HistoryDict::open(path, StringTable::new())?;
    for entry in removed {
        let syllables = SyllableVec::from_str(&entry.bopomofo)?;
        history.remove(&syllables, &entry.word);
    }
    replace_file(path, |writer| Ok(history.to_writer(writer)?))
}

fn load() -> Result<Vec<Entry>, String> {
    fn inner() -> Result<Vec<Entry>> {
        read_entries(&dict_dir()?.join(USER_DICT))
    }
    inner().map_err(|e| format!("{:#}", e))
}

/// Returns the dictionary as saved, with what the IME learned meanwhile.
fn save(entries: Vec<Entry>, loaded: &[Entry]) -> Result<Vec<Entry>, String> {
    fn inner(entries: Vec<Entry>, loaded: &[Entry]) -> Result<Vec<Entry>> {
        let edited = entries
            .into_iter()
            .map(Entry::checked)
            .collect::<Result<Vec<_>>>()?;
        let dir = dict_dir()?;
        fs::create_dir_all(&dir)?;
        let disk = read_entries(&dir.join(USER_DICT))?;
        let (merged, removed) = merge(disk, loaded, &edited);
        replace_file(&dir.join(USER_DICT), |writer| {
            write_entries(writer, &merged)
        })?;
        forget(&dir.join(HISTORY_DICT), &removed)?;
        // The IME rereads both files when it sees the settings change.
        Config::from_reg()?.save_reg();
        Ok(merged)
    }
    inner(entries, loaded).map_err(|e| format!("{:#}", e))
}

/// Returns the list with the file's words added; saving writes them.
fn import_entries(path: &Path, entries: Vec<Entry>) -> Result<Vec<Entry>, String> {
    fn inner(path: &Path, entries: Vec<Entry>) -> Result<Vec<Entry>> {
        let imported = read_entries(path)?
            .into_iter()
            .map(Entry::checked)
            .collect::<Result<Vec<_>>>()?;
        Ok(union(entries, imported))
    }
    inner(path, entries).map_err(|e| format!("{:#}", e))
}

fn export_entries(path: &Path, entries: &[Entry]) -> Result<(), String> {
    fn inner(path: &Path, entries: &[Entry]) -> Result<()> {
        let mut writer = BufWriter::new(File::create(path)?);
        write_entries(&mut writer, entries)?;
        writer.flush()?;
        Ok(())
    }
    inner(path, entries).map_err(|e| format!("{:#}", e))
}

fn validate(bopomofo: &str) -> Result<(), String> {
    fn inner(bopomofo: &str) -> Result<()> {
        if bopomofo.is_empty() {
            bail!("注音不可為空白");
        }
        for syl in bopomofo
            .replace("␣", " ")
            // number one vs. bopomofo I
            .replace("一", "ㄧ")
            .split_whitespace()
            .map(Syllable::from_str)
        {
            syl?;
        }
        Ok(())
    }
    inner(bopomofo).map_err(|e| format!("不是正確的注音\n注意：字與字之間須有空格分開\n\n{:#}", e))
}

fn map_bopomofo(input: &str) -> String {
    let mut output = String::new();
    for ch in input.chars() {
        if ch.is_ascii() && ch.is_whitespace().not() {
            let mut ed = Standard::new();
            let ev = map_ascii(&QWERTY_MAP, ch as u8);
            ed.key_press(ev);
            output.push_str(&ed.read().to_string());
        } else {
            output.push(ch);
        }
    }
    output
}

/// The list as being edited, until saved.
struct Editor {
    entries: Vec<Entry>,
    /// As loaded or last saved, to tell what was edited here.
    loaded: Vec<Entry>,
    query: String,
    /// The column sorted by, and whether up.
    sort: (usize, bool),
    /// What the list shows, as places in `entries`.
    view: Vec<usize>,
    selected: Option<usize>,
}

impl Editor {
    /// The list again: the table's rows are places in the view, which a
    /// search, sort, delete or save changes, so the selection is the entry's.
    fn show(&mut self, ui: &DictionaryWindow) {
        let entries = &self.entries;
        let (column, ascending) = self.sort;
        self.view = (0..entries.len())
            .filter(|&i| entries[i].word.contains(&self.query))
            .collect();
        self.view.sort_by(|&a, &b| {
            let (a, b) = (&entries[a], &entries[b]);
            let order = match column {
                0 => a.word.cmp(&b.word),
                1 => a.bopomofo.cmp(&b.bopomofo),
                _ => a.boost.cmp(&b.boost),
            };
            if ascending { order } else { order.reverse() }
        });
        let rows = self.view.iter().map(|&i| {
            let entry = &entries[i];
            ModelRc::new(VecModel::from(vec![
                StandardListViewItem::from(entry.word.as_str()),
                StandardListViewItem::from(entry.bopomofo.as_str()),
                StandardListViewItem::from(entry.boost.to_string().as_str()),
            ]))
        });
        ui.set_rows(ModelRc::new(VecModel::from_iter(rows)));
        let row = self
            .selected
            .and_then(|selected| self.view.iter().position(|&i| i == selected));
        ui.set_current_row(row.map_or(-1, |row| row as i32));
        ui.set_editing(self.selected.is_some());
    }

    /// The right side, for the word newly selected, or none.
    fn edit(&self, ui: &DictionaryWindow) {
        let entry = self
            .selected
            .map(|i| self.entries[i].clone())
            .unwrap_or_default();
        ui.set_word(entry.word.into());
        ui.set_bopomofo(entry.bopomofo.into());
        ui.set_boost(entry.boost.into());
    }

    /// Saves `entries`, which become the list, with `done` told on success.
    fn save(&mut self, ui: &DictionaryWindow, entries: Vec<Entry>) -> Result<(), String> {
        let saved = save(entries, &self.loaded)?;
        self.entries = saved.clone();
        self.loaded = saved;
        self.selected = None;
        self.show(ui);
        self.edit(ui);
        Ok(())
    }
}

pub(crate) fn run() -> Result<(), slint::PlatformError> {
    // As the window's title.
    if bring_forward(&format!("InputMethodEditor 使用者詞庫 ({VERSION})")) {
        return Ok(());
    }
    let ui = DictionaryWindow::new()?;
    ui.set_version(VERSION.into());
    let entries = load().unwrap_or_else(|error| {
        error_box(None, &error);
        vec![]
    });
    let editor = Rc::new(RefCell::new(Editor {
        loaded: entries.clone(),
        entries,
        query: String::new(),
        sort: (1, true),
        view: vec![],
        selected: None,
    }));
    editor.borrow_mut().show(&ui);

    // Shown for two seconds, a later one starting the two over.
    let toast_timer = Rc::new(Timer::default());
    let toast = {
        let ui = ui.as_weak();
        move |text: &str| {
            ui.unwrap().set_toast(text.into());
            let ui = ui.clone();
            toast_timer.start(TimerMode::SingleShot, Duration::from_secs(2), move || {
                ui.unwrap().set_toast("".into());
            });
        }
    };
    let toast = Rc::new(toast);

    ui.on_pick({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move |row| {
            let mut editor = editor.borrow_mut();
            editor.selected = editor.view.get(row as usize).copied();
            let ui = ui.unwrap();
            ui.set_editing(editor.selected.is_some());
            editor.edit(&ui);
        }
    });
    ui.on_insert({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move || {
            let ui = ui.unwrap();
            let mut editor = editor.borrow_mut();
            editor.entries.push(Entry::default());
            editor.query.clear();
            ui.set_query("".into());
            editor.selected = Some(editor.entries.len() - 1);
            editor.show(&ui);
            editor.edit(&ui);
        }
    });
    ui.on_remove({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move || {
            let ui = ui.unwrap();
            let mut editor = editor.borrow_mut();
            if let Some(selected) = editor.selected.take() {
                editor.entries.remove(selected);
            }
            editor.show(&ui);
            editor.edit(&ui);
        }
    });
    ui.on_apply({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move || {
            let ui = ui.unwrap();
            let Some(selected) = editor.borrow().selected else {
                return;
            };
            let entry = Entry {
                word: ui.get_word().into(),
                bopomofo: ui.get_bopomofo().into(),
                boost: ui.get_boost().clamp(-100, 100) as i8,
            };
            // Kept though, as typed: saving rejects it until it is fixed.
            if let Err(error) = validate(&entry.bopomofo) {
                error_box(Some(ui.window()), &error);
            }
            let mut editor = editor.borrow_mut();
            editor.entries[selected] = entry;
            editor.show(&ui);
        }
    });
    ui.on_save({
        let (ui, editor, toast) = (ui.as_weak(), editor.clone(), toast.clone());
        move || {
            let ui = ui.unwrap();
            let saved = {
                let mut editor = editor.borrow_mut();
                let entries = editor.entries.clone();
                editor.save(&ui, entries)
            };
            match saved {
                Ok(()) => toast("存檔成功"),
                Err(error) => error_box(Some(ui.window()), &error),
            }
        }
    });
    ui.on_search({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move |query| {
            let ui = ui.unwrap();
            let mut editor = editor.borrow_mut();
            editor.query = query.into();
            editor.selected = None;
            editor.show(&ui);
            editor.edit(&ui);
        }
    });
    ui.on_sort({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move |column, ascending| {
            let mut editor = editor.borrow_mut();
            editor.sort = (column as usize, ascending);
            editor.show(&ui.unwrap());
        }
    });
    ui.on_map_bopomofo(|input| map_bopomofo(&input).into());
    // Saved right away: this window doesn't warn about unsaved changes on
    // closing, so an import waiting for 存檔 could be lost unnoticed.
    ui.on_import({
        let (ui, editor, toast) = (ui.as_weak(), editor.clone(), toast.clone());
        move || {
            let ui = ui.unwrap();
            let Some(path) = open_file(ui.window(), "CSV", "csv") else {
                return;
            };
            let imported = {
                let mut editor = editor.borrow_mut();
                import_entries(&path, editor.entries.clone())
                    .and_then(|entries| editor.save(&ui, entries))
            };
            match imported {
                Ok(()) => toast("匯入完成"),
                Err(error) => error_box(
                    Some(ui.window()),
                    &format!(
                        "無法匯入，請確認是新酷音的詞庫檔（每行：詞,注音,偏好程度）。\n\n{error}"
                    ),
                ),
            }
        }
    });
    ui.on_export({
        let (ui, editor) = (ui.as_weak(), editor.clone());
        move || {
            let ui = ui.unwrap();
            let Some(path) = save_file(
                ui.window(),
                "CSV",
                "csv",
                "InputMethodEditor 使用者詞庫.csv",
            ) else {
                return;
            };
            let exported = export_entries(&path, &editor.borrow().entries);
            match exported {
                Ok(()) => toast("匯出完成"),
                Err(error) => error_box(Some(ui.window()), &format!("無法寫入檔案。\n\n{error}")),
            }
        }
    });
    ui.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    center_when_made(&ui);
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(word: &str, bopomofo: &str, boost: i8) -> Entry {
        Entry {
            word: word.into(),
            bopomofo: bopomofo.into(),
            boost,
        }
    }

    #[test]
    fn saving_keeps_what_the_ime_learned_meanwhile() {
        let loaded = [
            entry("測試", "ㄘㄜˋ ㄕˋ", 10),
            entry("好", "ㄏㄠˇ", 10),
            entry("字", "ㄗˋ", 10),
        ];
        // Since loading, the IME boosted 好 and 字 and learned 詞.
        let disk = vec![
            entry("測試", "ㄘㄜˋ ㄕˋ", 10),
            entry("好", "ㄏㄠˇ", 20),
            entry("字", "ㄗˋ", 20),
            entry("詞", "ㄘˊ", 10),
        ];
        // Here 測試 was boosted, 好 removed, 字 left as loaded, 新 added.
        let edited = [
            entry("測試", "ㄘㄜˋ ㄕˋ", 50),
            entry("字", "ㄗˋ", 10),
            entry("新", "ㄒㄧㄣ", 0),
        ];
        let (merged, removed) = merge(disk, &loaded, &edited);
        assert!(
            merged
                == [
                    entry("測試", "ㄘㄜˋ ㄕˋ", 50),
                    entry("字", "ㄗˋ", 20),
                    entry("詞", "ㄘˊ", 10),
                    entry("新", "ㄒㄧㄣ", 0),
                ]
        );
        assert!(removed == [entry("好", "ㄏㄠˇ", 10)]);
    }

    #[test]
    fn importing_both_ways_leaves_the_same_dictionary() {
        let home = vec![entry("測試", "ㄘㄜˋ ㄕˋ", 30), entry("好", "ㄏㄠˇ", 10)];
        let work = vec![entry("測試", "ㄘㄜˋ ㄕˋ", 10), entry("詞", "ㄘˊ", 10)];
        let at_home = union(home.clone(), work.clone());
        let at_work = union(work, home);
        for entry in &at_home {
            assert!(at_work.contains(entry));
        }
        assert_eq!(at_home.len(), at_work.len());
        assert!(at_home.contains(&entry("測試", "ㄘㄜˋ ㄕˋ", 30)));
        assert_eq!(at_home.len(), 3);
    }

    #[test]
    fn checked_spells_readings_as_chewing_writes_them() {
        let spelled = entry("一", "  一  ", 0).checked().unwrap();
        assert_eq!(spelled.bopomofo, "ㄧ");
        assert!(entry("測試", "ㄘㄜˋ␣ㄕˋ", 0).checked().unwrap().bopomofo == "ㄘㄜˋ ㄕˋ");
        assert!(entry("a,b", "ㄚ", 0).checked().is_err());
        assert!(entry("錯", "abc", 0).checked().is_err());
    }

    #[test]
    fn keys_typed_become_bopomofo_on_the_standard_layout() {
        // The word editor's 注音 field maps each key as it is typed.
        assert_eq!(map_bopomofo("su3"), "ㄋㄧˇ");
        assert_eq!(map_bopomofo("ㄋㄧˇ cl3"), "ㄋㄧˇ ㄏㄠˇ");
    }
}
