//! The open tabs and the split panes of a project folder, kept when the editor ends and
//! brought back when it starts in the same folder (the `restore_session` setting).
//!
//! One file for each project folder in the `sessions` folder of the config folder, named by
//! a hash of the folder's whole path. It holds the files by whole path, never their text:
//! what is unsaved is the business of the recovery files (see `recovery`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::saved_state::write_atomically;

/// The folder of the session files, inside the config folder.
pub(crate) const FOLDER: &str = "sessions";

/// The version of the file's layout; a file of a newer version is left alone.
const LAYOUT: u32 = 1;

/// Which way a split lays its two sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    /// Side by side.
    Horizontal,
    /// One above the other.
    Vertical,
}

/// How the panes are split: a pane, or two sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Layout {
    Pane(PaneSession),
    Split {
        direction: Direction,
        /// How much of the split the first side has, in thousandths.
        ratio: u16,
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

/// The tabs of one pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaneSession {
    pub(crate) tabs: Vec<TabSession>,
    /// The tab in front, as an index into `tabs`.
    pub(crate) active: usize,
    /// Whether the pane had the keyboard.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) focused: bool,
}

/// One tab: a file and how it was being looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TabSession {
    /// The file, a whole path.
    pub(crate) path: PathBuf,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) pinned: bool,
    pub(crate) line: usize,
    pub(crate) column: usize,
    /// The first line and the first display column on screen.
    #[serde(default)]
    pub(crate) top: usize,
    #[serde(default)]
    pub(crate) left: usize,
}

/// What is kept of a project folder's tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Session {
    #[serde(default = "layout_version")]
    version: u32,
    /// The project folder, a whole path.
    pub(crate) project: PathBuf,
    pub(crate) layout: Layout,
}

fn layout_version() -> u32 {
    LAYOUT
}

impl Session {
    pub(crate) fn new(project: &Path, layout: Layout) -> Self {
        Self {
            version: LAYOUT,
            project: project.to_path_buf(),
            layout,
        }
    }

    /// The files of the session, in reading order.
    pub(crate) fn files(&self) -> Vec<&Path> {
        let mut files = Vec::new();
        self.layout.collect_files(&mut files);
        files
    }
}

impl Layout {
    fn collect_files<'a>(&'a self, files: &mut Vec<&'a Path>) {
        match self {
            Self::Pane(pane) => files.extend(pane.tabs.iter().map(|tab| tab.path.as_path())),
            Self::Split { first, second, .. } => {
                first.collect_files(files);
                second.collect_files(files);
            }
        }
    }
}

/// Keeps `session`, or forgets the folder's session when there is none (`None`).
pub(crate) fn save(config_dir: &Path, project: &Path, session: Option<&Session>) -> io::Result<()> {
    let path = path_of(config_dir, project);
    let Some(session) = session else {
        return match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    };
    let text = serde_json::to_string(session).map_err(io::Error::other)?;
    write_atomically(&path, &text)
}

/// The session kept for `project`. No file is normal (`Ok(None)`); a file that cannot be
/// read or is of a newer layout is a problem to report and is left where it is.
pub(crate) fn load(config_dir: &Path, project: &Path) -> Result<Option<Session>, String> {
    let path = path_of(config_dir, project);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let session: Session =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    if session.version > LAYOUT {
        return Err(format!(
            "{}: written by a newer version of the editor (layout {})",
            path.display(),
            session.version
        ));
    }
    Ok(Some(session))
}

fn path_of(config_dir: &Path, project: &Path) -> PathBuf {
    let digest = Sha256::digest(project.to_string_lossy().as_bytes());
    let mut name = String::with_capacity(21);
    for byte in digest.iter().take(8) {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".json");
    config_dir.join(FOLDER).join(name)
}
