//! The entries of a folder, matched against what is typed of a path: the list under the Open
//! File and Open Folder prompts. This file is the part with no screen and no thread: reading a
//! folder, splitting a typed path, and ranking the entries.

use std::cmp::Reverse;
use std::fs;
use std::path::{self, Path};

use crate::ui::fuzzy::Query;

/// How many matches are kept to show; the rest are counted.
const MOST_KEPT: usize = 200;

/// What a folder holds, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    /// A folder, or a link to one.
    pub(crate) is_dir: bool,
}

/// The entries of one folder, as of when they were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) dir: std::path::PathBuf,
    pub(crate) entries: Vec<Entry>,
}

/// Reads the folder `dir`: every entry with its name and whether it is a folder. An entry that
/// cannot be told is taken for a file, so one broken link does not hide the rest. The error is
/// what to tell the user.
pub(crate) fn read_dir(dir: &Path) -> Result<Vec<Entry>, String> {
    let read = fs::read_dir(dir).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => format!("{} is not a folder that exists", dir.display()),
        std::io::ErrorKind::NotADirectory => format!("{} is not a folder", dir.display()),
        _ => format!("cannot read {}: {error}", dir.display()),
    })?;
    let mut entries = Vec::new();
    for entry in read.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let is_dir = match kind.is_symlink() {
            true => fs::metadata(entry.path()).is_ok_and(|meta| meta.is_dir()),
            false => kind.is_dir(),
        };
        entries.push(Entry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir,
        });
    }
    Ok(entries)
}

/// Splits what was typed at its last separator: the folder part, with its separator (empty when
/// there is none), and the name being typed.
pub(crate) fn split_typed(text: &str) -> (&str, &str) {
    match text.rfind(path::is_separator) {
        Some(at) => text.split_at(at + 1),
        None => ("", text),
    }
}

/// An entry that matches the name being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Match {
    /// Into the entries of the listing.
    pub(crate) index: usize,
    /// The characters of the name the match uses, for drawing them out.
    pub(crate) positions: Vec<usize>,
}

/// The entries of `entries` that match `partial`, best first: folders before files, then the
/// better match, then the name. Entries whose names start with a dot are only offered when
/// `partial` starts with one. With `dirs_only` files are left out. At most [`MOST_KEPT`] are
/// returned, with how many there were.
pub(crate) fn matching(entries: &[Entry], partial: &str, dirs_only: bool) -> (Vec<Match>, usize) {
    let query = Query::new(partial);
    let dotted = partial.starts_with('.');
    let mut scored: Vec<(usize, i32)> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| !dirs_only || entry.is_dir)
        .filter(|(_, entry)| dotted || !entry.name.starts_with('.'))
        .filter_map(|(index, entry)| Some((index, query.score(&entry.name)?)))
        .collect();
    let total = scored.len();
    // The name as written exactly beats any other match of it.
    let exact = |index: usize| entries[index].name.eq_ignore_ascii_case(partial);
    let better = |a: &(usize, i32), b: &(usize, i32)| {
        let key = |(index, score): &(usize, i32)| {
            (
                Reverse(exact(*index)),
                Reverse(entries[*index].is_dir),
                Reverse(*score),
            )
        };
        key(a).cmp(&key(b)).then_with(|| {
            let name = |index: usize| entries[index].name.to_lowercase();
            name(a.0).cmp(&name(b.0))
        })
    };
    if scored.len() > MOST_KEPT {
        scored.select_nth_unstable_by(MOST_KEPT, better);
        scored.truncate(MOST_KEPT);
    }
    scored.sort_by(better);
    let found = scored
        .into_iter()
        .map(|(index, _)| Match {
            index,
            positions: query.positions(&entries[index].name).unwrap_or_default(),
        })
        .collect();
    (found, total)
}

/// What Tab writes when nothing is highlighted: the longest start the names that begin with
/// `partial` (ignoring case) share, never shorter than what is typed.
pub(crate) fn common_start(names: &[&str], partial: &str) -> String {
    let mut starting: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| starts_with_ignoring_case(name, partial))
        .collect();
    let Some(first) = starting.pop() else {
        return partial.to_owned();
    };
    let mut shared = first.to_owned();
    for name in starting {
        let common = shared
            .chars()
            .zip(name.chars())
            .take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
            .count();
        shared = shared.chars().take(common).collect();
    }
    match shared.chars().count() > partial.chars().count() {
        true => shared,
        false => partial.to_owned(),
    }
}

fn starts_with_ignoring_case(name: &str, start: &str) -> bool {
    let mut name = name.chars().flat_map(char::to_lowercase);
    start
        .chars()
        .flat_map(char::to_lowercase)
        .all(|wanted| name.next() == Some(wanted))
}

#[cfg(test)]
mod tests;
