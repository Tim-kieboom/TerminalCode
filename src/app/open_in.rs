//! Opening the folder of a file in the system's file manager, in a tab of the terminal pane,
//! or in the system's terminal program. The folder is that of the explorer's selected entry
//! (the entry itself when it is a folder), or, with the keyboard elsewhere, of the open file;
//! with neither it is the project folder.

use std::io;
use std::path::{Path, PathBuf};

use super::App;

/// A program to start, with its arguments (`${folder}` in one is the folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Launch {
    pub(super) program: String,
    pub(super) args: Vec<String>,
}

impl Launch {
    fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

/// The systems that have terminal programs of their own to try.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Platform {
    Linux,
    Windows,
    Mac,
}

impl Platform {
    pub(super) fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// The terminal programs to try, first to last: the `terminal` setting alone when there is
/// one, else `$TERMINAL` and the usual ones of the platform. Every one is also started with
/// the folder as its working directory, which most terminals open in without being told.
pub(super) fn terminal_candidates(
    setting: Option<&[String]>,
    from_environment: Option<&str>,
    platform: Platform,
) -> Vec<Launch> {
    if let Some([program, args @ ..]) = setting {
        return vec![Launch {
            program: program.clone(),
            args: args.to_vec(),
        }];
    }
    match platform {
        Platform::Windows => vec![
            Launch::new("wt", &["-d", "${folder}"]),
            Launch::new("cmd", &["/c", "start", "cmd"]),
        ],
        Platform::Mac => vec![Launch::new("open", &["-a", "Terminal", "${folder}"])],
        Platform::Linux => {
            let own = from_environment
                .map(str::trim)
                .filter(|program| !program.is_empty())
                .map(|program| Launch::new(program, &[]));
            own.into_iter()
                .chain([
                    Launch::new("x-terminal-emulator", &[]),
                    Launch::new("gnome-terminal", &["--working-directory=${folder}"]),
                    Launch::new("konsole", &["--workdir", "${folder}"]),
                    Launch::new("kitty", &[]),
                    Launch::new("alacritty", &[]),
                    Launch::new("foot", &[]),
                    Launch::new("xfce4-terminal", &["--working-directory=${folder}"]),
                    Launch::new("xterm", &[]),
                ])
                .collect()
        }
    }
}

/// Why no terminal program started.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum LaunchFailure {
    /// None of them is installed.
    NoneFound,
    /// One is, and could not start.
    Failed { program: String, error: String },
}

/// Starts the first of `candidates` that is installed (a program that is not there is
/// passed over; any other failure stops the search) and says which it was.
pub(super) fn launch_first(
    candidates: &[Launch],
    folder: &Path,
    start: impl Fn(&str, &[String], &Path) -> io::Result<()>,
) -> Result<String, LaunchFailure> {
    let folder_text = folder.display().to_string();
    for candidate in candidates {
        let args: Vec<String> = candidate
            .args
            .iter()
            .map(|arg| arg.replace("${folder}", &folder_text))
            .collect();
        match start(&candidate.program, &args, folder) {
            Ok(()) => return Ok(candidate.program.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(LaunchFailure::Failed {
                    program: candidate.program.clone(),
                    error: error.to_string(),
                });
            }
        }
    }
    Err(LaunchFailure::NoneFound)
}

impl App {
    /// The folder the three actions work on.
    fn folder_in_focus(&self) -> PathBuf {
        let Some(path) = self.path_in_focus() else {
            return self.project_root();
        };
        match path.is_dir() {
            true => path,
            false => path
                .parent()
                .map_or_else(|| self.project_root(), Path::to_path_buf),
        }
    }

    /// The folder, when it is still there.
    fn existing_folder_in_focus(&mut self) -> Option<PathBuf> {
        let folder = self.folder_in_focus();
        if folder.is_dir() {
            return Some(folder);
        }
        self.state
            .notify_error(format!("{} no longer exists", folder.display()));
        None
    }

    pub(super) fn open_in_file_manager(&mut self) {
        let Some(folder) = self.existing_folder_in_focus() else {
            return;
        };
        let (system, front) = super::opener::system_opener();
        let program = self.opener_override().unwrap_or(system).to_owned();
        match super::opener::open(&program, front, &folder) {
            Ok(()) => self
                .state
                .notify(format!("opened {} in the file manager", folder.display())),
            Err(error) => self.state.notify_error(format!(
                "could not open the file manager: {program}: {error}"
            )),
        }
    }

    pub(super) fn open_in_terminal(&mut self) {
        let Some(folder) = self.existing_folder_in_focus() else {
            return;
        };
        self.new_terminal_tab_in(folder);
    }

    pub(super) fn open_in_external_terminal(&mut self) {
        let Some(folder) = self.existing_folder_in_focus() else {
            return;
        };
        let environment = std::env::var("TERMINAL").ok();
        let candidates = terminal_candidates(
            self.settings.terminal.as_deref(),
            environment.as_deref(),
            Platform::current(),
        );
        match launch_first(&candidates, &folder, super::opener::start) {
            Ok(program) => self
                .state
                .notify(format!("opened {} in {program}", folder.display())),
            Err(LaunchFailure::NoneFound) => self.state.notify_error(
                "no terminal program found; set `terminal` in the settings, for example [\"kitty\", \"--directory\", \"${folder}\"]",
            ),
            Err(LaunchFailure::Failed { program, error }) => self
                .state
                .notify_error(format!("could not start {program}: {error}")),
        }
    }
}

#[cfg(test)]
mod tests;
