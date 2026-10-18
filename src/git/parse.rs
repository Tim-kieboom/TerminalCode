//! Reading the answer of `git status --porcelain=v2 --branch -z`: first the
//! `# branch.*` lines, then one record for each changed file, each ended by a
//! NUL byte (a renamed file's record is followed by one more, its old path).

use std::path::PathBuf;

use super::{Branch, Change, Entry, GitError, Kind};

/// Reads the branch and the changed files from the output of `git status`.
/// Paths that are not UTF-8 come out with replacement characters.
pub(crate) fn parse_status(output: &[u8]) -> Result<(Branch, Vec<Entry>), GitError> {
    let text = String::from_utf8_lossy(output);
    let mut records = text.split('\0').filter(|record| !record.is_empty());
    let mut branch = Branch::default();
    let mut entries = Vec::new();

    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            read_header(header, &mut branch);
            continue;
        }
        let entry = match record.split_at_checked(2) {
            Some(("1 ", rest)) => ordinary(rest, record)?,
            Some(("2 ", rest)) => {
                let original = records
                    .next()
                    .ok_or_else(|| unreadable(record, "a rename without its old path"))?;
                renamed(rest, original, record)?
            }
            Some(("u ", rest)) => unmerged(rest, record)?,
            Some(("? ", path)) => Entry {
                path: PathBuf::from(path),
                original: None,
                change: Change::Untracked,
            },
            // Ignored files (`! `) are not asked for, and an unknown record is
            // from a newer git: skipped, the rest is still right.
            _ => continue,
        };
        entries.push(entry);
    }
    Ok((branch, entries))
}

fn read_header(header: &str, branch: &mut Branch) {
    let Some((name, value)) = header.split_once(' ') else {
        return;
    };
    match name {
        "branch.oid" => branch.oid = (value != "(initial)").then(|| value.to_owned()),
        "branch.head" => branch.name = (value != "(detached)").then(|| value.to_owned()),
        "branch.upstream" => branch.upstream = Some(value.to_owned()),
        "branch.ab" => {
            let count = |sign: char, text: &str| text.strip_prefix(sign)?.parse().ok();
            if let Some((ahead, behind)) = value.split_once(' ') {
                branch.ahead = count('+', ahead).unwrap_or(0);
                branch.behind = count('-', behind).unwrap_or(0);
            }
        }
        _ => {}
    }
}

/// `1 XY sub mH mI mW hH hI path`, after the `1 `.
fn ordinary(rest: &str, record: &str) -> Result<Entry, GitError> {
    let fields: Vec<&str> = rest.splitn(8, ' ').collect();
    let [xy, _, _, _, _, _, _, path] = fields.as_slice() else {
        return Err(unreadable(record, "a changed file with too few fields"));
    };
    Ok(Entry {
        path: PathBuf::from(path),
        original: None,
        change: tracked(xy, record)?,
    })
}

/// `2 XY sub mH mI mW hH hI Xscore path`, after the `2 `, and the old path.
fn renamed(rest: &str, original: &str, record: &str) -> Result<Entry, GitError> {
    let fields: Vec<&str> = rest.splitn(9, ' ').collect();
    let [xy, _, _, _, _, _, _, _, path] = fields.as_slice() else {
        return Err(unreadable(record, "a renamed file with too few fields"));
    };
    Ok(Entry {
        path: PathBuf::from(path),
        original: Some(PathBuf::from(original)),
        change: tracked(xy, record)?,
    })
}

/// `u XY sub m1 m2 m3 mW h1 h2 h3 path`, after the `u `.
fn unmerged(rest: &str, record: &str) -> Result<Entry, GitError> {
    let fields: Vec<&str> = rest.splitn(10, ' ').collect();
    let [_, _, _, _, _, _, _, _, _, path] = fields.as_slice() else {
        return Err(unreadable(record, "a conflicted file with too few fields"));
    };
    Ok(Entry {
        path: PathBuf::from(path),
        original: None,
        change: Change::Conflicted,
    })
}

/// The two letters of `XY`: what the index has changed, and what the working
/// tree has changed since; `.` is nothing.
fn tracked(xy: &str, record: &str) -> Result<Change, GitError> {
    let mut letters = xy.chars();
    let (Some(staged), Some(unstaged), None) = (letters.next(), letters.next(), letters.next())
    else {
        return Err(unreadable(record, "a status that is not two letters"));
    };
    Ok(Change::Tracked {
        staged: kind(staged, record)?,
        unstaged: kind(unstaged, record)?,
    })
}

fn kind(letter: char, record: &str) -> Result<Option<Kind>, GitError> {
    Ok(Some(match letter {
        '.' => return Ok(None),
        'M' => Kind::Modified,
        'T' => Kind::TypeChanged,
        'A' => Kind::Added,
        'D' => Kind::Deleted,
        'R' => Kind::Renamed,
        'C' => Kind::Copied,
        _ => return Err(unreadable(record, "an unknown status letter")),
    }))
}

fn unreadable(record: &str, what: &str) -> GitError {
    GitError::Unreadable(format!("{what}: {record:?}"))
}
