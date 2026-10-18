//! What git says, kept up to date by the app: the answer of a refresh reaches
//! the status bar, a failure keeps the marks, and the changes that make a
//! refresh wanted.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::git::{Branch, Change, Entry, GitError, Kind, Status};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "alpha\n").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    state.workspace_mut().open_path(&file).unwrap();
    let (tx, events) = mpsc::channel(1024);
    Rig {
        app: App::new(state).with_events(tx),
        events,
        terminal: Terminal::new(TestBackend::new(120, 30)).unwrap(),
        dir,
    }
}

impl Rig {
    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn texts(&self) -> Vec<String> {
        let notifications = self.app.state().notifications();
        notifications
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn answer(&mut self, result: Result<Status, GitError>) {
        self.app.handle_event(Event::GitStatus {
            took: Duration::from_millis(20),
            result,
        });
    }

    /// Waits for the refresh that is wanted to start, and for its answer.
    fn refresh(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self
            .app
            .git_deadline()
            .is_some_and(|at| Instant::now() < at)
        {
            assert!(Instant::now() < deadline, "the refresh never became due");
            std::thread::sleep(Duration::from_millis(10));
        }
        self.app.git_due();
        loop {
            match self.events.try_recv() {
                Ok(event @ Event::GitStatus { .. }) => return self.app.handle_event(event),
                _ => {
                    assert!(Instant::now() < deadline, "no answer from git");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }
}

fn status(branch: &str, paths: &[&str]) -> Status {
    Status {
        root: PathBuf::from("/repo"),
        branch: Branch {
            name: Some(branch.to_owned()),
            ..Branch::default()
        },
        entries: paths
            .iter()
            .map(|path| Entry {
                path: PathBuf::from(path),
                original: None,
                change: Change::Untracked,
            })
            .collect(),
    }
}

#[test]
fn the_branch_shows_in_the_status_bar() {
    let mut rig = rig();
    assert!(!rig.screen().contains("on main"));

    rig.answer(Ok(status("main", &["a.txt"])));

    assert!(rig.screen().contains("on main"));
}

#[test]
fn a_new_answer_replaces_the_old_one() {
    let mut rig = rig();
    rig.answer(Ok(status("main", &["a.txt"])));

    rig.answer(Ok(status("topic", &[])));

    assert!(rig.screen().contains("on topic"));
    assert_eq!(rig.app.state().git_status().unwrap().entries, vec![]);
}

#[test]
fn an_answer_that_changes_nothing_does_not_redraw() {
    let mut rig = rig();
    rig.answer(Ok(status("main", &[])));
    rig.screen();
    rig.app.mark_drawn();

    rig.answer(Ok(status("main", &[])));

    assert!(!rig.app.needs_redraw());
}

#[test]
fn without_git_the_feature_stops_with_one_message() {
    let mut rig = rig();
    rig.answer(Ok(status("main", &[])));

    rig.answer(Err(GitError::NotInstalled));
    rig.app.git_changed();

    assert!(rig.app.state().git_status().is_none(), "the marks go");
    assert!(!rig.screen().contains("on main"));
    assert_eq!(rig.app.git_deadline(), None, "nothing is asked any more");
    assert_eq!(
        rig.texts(),
        vec!["git is off: `git` was not found on the PATH"]
    );
}

#[test]
fn outside_a_repository_the_feature_stops_with_git_s_own_words() {
    let mut rig = rig();

    rig.answer(Err(GitError::NoRepository(
        "fatal: not a git repository".into(),
    )));

    assert_eq!(rig.texts(), vec!["git is off: fatal: not a git repository"]);
    assert_eq!(rig.app.git_deadline(), None);
}

#[test]
fn a_slow_or_failed_refresh_keeps_the_old_marks_and_says_so_once() {
    let mut rig = rig();
    rig.answer(Ok(status("main", &["a.txt"])));

    rig.answer(Err(GitError::TimedOut(Duration::from_secs(5))));
    rig.answer(Err(GitError::TimedOut(Duration::from_secs(5))));

    assert!(rig.screen().contains("on main"), "the old marks stay");
    assert_eq!(rig.texts().len(), 1);
    assert!(rig.texts()[0].contains("took longer than 5 seconds"));
}

#[test]
fn a_problem_that_comes_back_after_it_went_away_is_told_again() {
    let mut rig = rig();
    let slow = || Err(GitError::TimedOut(Duration::from_secs(5)));
    let later = Instant::now() + Duration::from_secs(3600);
    rig.answer(slow());
    rig.app.expire_notifications(later);
    assert_eq!(rig.texts().len(), 0);

    rig.answer(slow());
    assert_eq!(rig.texts().len(), 0, "still the same problem");
    rig.answer(Ok(status("main", &[])));
    rig.answer(slow());

    assert_eq!(rig.texts().len(), 1);
}

#[test]
fn a_slow_answer_makes_the_next_refresh_wait_twice_as_long() {
    let mut rig = rig();

    rig.app.handle_event(Event::GitStatus {
        took: Duration::from_secs(5),
        result: Err(GitError::TimedOut(Duration::from_secs(5))),
    });
    rig.app.git_changed();

    let wait = rig.app.git_deadline().unwrap() - Instant::now();
    assert!(wait > Duration::from_secs(9), "{wait:?}");
}

#[test]
fn files_changed_outside_ask_for_a_refresh() {
    let mut rig = rig();
    assert_eq!(rig.app.git_deadline(), None);

    rig.app.handle_event(Event::FilesChanged(vec![]));

    assert!(rig.app.git_deadline().is_some());
}

#[test]
fn coming_back_to_the_window_asks_for_a_refresh() {
    let mut rig = rig();

    rig.app.handle_input(crossterm::event::Event::FocusGained);

    assert!(rig.app.git_deadline().is_some());
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
}

#[test]
fn a_refresh_reads_the_repository_of_the_project() {
    let mut rig = rig();
    let root = rig.dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-q", "-m", "first"]);
    std::fs::write(root.join("b.txt"), "new\n").unwrap();

    rig.app.git_changed();
    rig.refresh();

    let status = rig.app.state().git_status().expect("a status");
    assert_eq!(status.branch.name.as_deref(), Some("main"));
    assert_eq!(
        status
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("b.txt")]
    );
    assert_eq!(status.entries[0].change, Change::Untracked);
    assert!(rig.screen().contains("on main"));
}

#[test]
fn a_project_that_is_no_repository_turns_it_off() {
    let mut rig = rig();

    rig.app.git_changed();
    rig.refresh();

    assert!(rig.app.state().git_status().is_none());
    assert_eq!(rig.app.git_deadline(), None);
    assert!(
        rig.texts()[0].starts_with("git is off: "),
        "{:?}",
        rig.texts()
    );
}

#[test]
fn the_second_refresh_sees_what_changed_since() {
    let mut rig = rig();
    let root = rig.dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-q", "-m", "first"]);
    rig.app.git_changed();
    rig.refresh();
    assert_eq!(rig.app.state().git_status().unwrap().entries, vec![]);

    std::fs::write(root.join("a.txt"), "changed\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![root.join("a.txt")]));
    rig.refresh();

    let entries = &rig.app.state().git_status().unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].change,
        Change::Tracked {
            staged: None,
            unstaged: Some(Kind::Modified)
        }
    );
}

// --- colors and letters ---

use ratatui::style::Color;

/// A status for the project of `rig`: `a.txt` modified, `sub/` has an
/// untracked file, and `sub/new.txt` is the one.
fn marked(rig: &mut Rig) {
    let root = crate::files::paths::canonical(rig.dir.path());
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/new.txt"), "x").unwrap();
    let entry = |path: &str, change| Entry {
        path: PathBuf::from(path),
        original: None,
        change,
    };
    rig.answer(Ok(Status {
        root,
        branch: Branch::default(),
        entries: vec![
            entry(
                "a.txt",
                Change::Tracked {
                    staged: None,
                    unstaged: Some(Kind::Modified),
                },
            ),
            entry("sub/new.txt", Change::Untracked),
        ],
    }));
    rig.app
        .state_mut()
        .explorer_mut()
        .refresh_paths(&[rig.dir.path().to_path_buf()])
        .unwrap();
}

impl Rig {
    fn fg_of(&self, slot: &str) -> Option<Color> {
        self.app.state().theme().style(slot).fg
    }

    /// The cells of the first screen line, left of `limit`, that has `text`.
    fn cells_with(&mut self, text: &str, limit: u16) -> Vec<(u16, String, Option<Color>)> {
        self.screen();
        let buffer = self.terminal.backend().buffer();
        for y in 0..buffer.area.height {
            let cells: Vec<_> = (0..limit.min(buffer.area.width))
                .map(|x| {
                    let cell = &buffer[(x, y)];
                    (x, cell.symbol().to_owned(), cell.style().fg)
                })
                .collect();
            let line: String = cells.iter().map(|(_, symbol, _)| symbol.as_str()).collect();
            if line.contains(text) {
                return cells;
            }
        }
        panic!("no line with {text}");
    }
}

#[test]
fn a_changed_file_is_colored_and_has_its_letter_at_the_right_edge_of_the_explorer() {
    let mut rig = rig();
    marked(&mut rig);
    let modified = rig.fg_of("git.modified");

    let cells = rig.cells_with("a.txt", 30);

    let letter = cells.iter().find(|(_, symbol, _)| symbol == "M").unwrap();
    assert_eq!(letter.2, modified, "the letter has the color");
    let name = cells.iter().find(|(_, symbol, _)| symbol == "a").unwrap();
    assert_eq!(name.2, modified, "so has the name");
    let right_edge = cells
        .iter()
        .rev()
        .find(|(_, symbol, _)| symbol.chars().all(char::is_alphanumeric) && symbol != " ")
        .unwrap();
    assert_eq!(right_edge.1, "M", "the letter is the last thing in the row");
}

#[test]
fn a_folder_has_the_color_of_what_is_inside_and_no_letter() {
    let mut rig = rig();
    marked(&mut rig);
    let untracked = rig.fg_of("git.untracked");

    let cells = rig.cells_with("sub", 30);

    let name = cells.iter().find(|(_, symbol, _)| symbol == "s").unwrap();
    assert_eq!(name.2, untracked);
    assert!(!cells.iter().any(|(_, symbol, _)| symbol == "U"));
}

#[test]
fn a_file_that_git_does_not_list_has_no_mark() {
    let mut rig = rig();
    marked(&mut rig);
    std::fs::write(rig.dir.path().join("clean.txt"), "x").unwrap();
    rig.app
        .state_mut()
        .explorer_mut()
        .refresh_paths(&[rig.dir.path().to_path_buf()])
        .unwrap();

    let cells = rig.cells_with("clean.txt", 30);

    let name = cells.iter().find(|(_, symbol, _)| symbol == "c").unwrap();
    assert_eq!(name.2, rig.fg_of("explorer.file"));
    assert!(!cells.iter().any(|(_, symbol, _)| symbol == "M"));
}

#[test]
fn the_tab_of_a_changed_file_has_the_color() {
    let mut rig = rig();
    marked(&mut rig);
    let modified = rig.fg_of("git.modified");

    // The tab is further right than the explorer.
    let cells = rig.cells_with("a.txt", 120);

    let tab_name = cells.iter().rfind(|(_, symbol, _)| symbol == "a").unwrap();
    assert_eq!(tab_name.2, modified);
}

#[test]
fn marks_go_when_git_goes() {
    let mut rig = rig();
    marked(&mut rig);

    rig.answer(Err(GitError::NotInstalled));

    let cells = rig.cells_with("a.txt", 30);
    assert!(!cells.iter().any(|(_, symbol, _)| symbol == "M"));
}

// --- the lines that differ from the index ---

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

const BAR: &str = "▎";
const ABOVE: &str = "▔";

/// A rig whose project is a repository with `a.txt` ("alpha", committed), and
/// whose status and index texts have been read.
fn repository_rig() -> Rig {
    let mut rig = rig();
    let root = rig.dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-q", "-m", "first"]);
    rig.app.git_changed();
    rig.refresh();
    rig.take_base();
    rig
}

impl Rig {
    /// Waits for the answer to the request for an index text and takes it.
    fn take_base(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.events.try_recv() {
                Ok(event @ Event::GitBase { .. }) => return self.app.handle_event(event),
                _ => {
                    assert!(Instant::now() < deadline, "no index text from git");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    fn type_text(&mut self, text: &str) {
        for letter in text.chars() {
            let key = KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE);
            self.app.handle_input(InputEvent::Key(key));
        }
    }

    /// The gutter bars on the screen line that has `text`: the symbol and color.
    fn bars_on_line_with(&mut self, text: &str) -> Vec<(String, Option<Color>)> {
        self.cells_with(text, 120)
            .into_iter()
            .filter(|(_, symbol, _)| symbol == BAR || symbol == ABOVE)
            .map(|(_, symbol, fg)| (symbol, fg))
            .collect()
    }
}

#[test]
fn a_line_that_is_the_same_as_the_index_has_no_bar() {
    let mut rig = repository_rig();

    assert_eq!(rig.bars_on_line_with("alpha"), vec![]);
}

#[test]
fn a_line_changed_since_the_index_has_a_bar_in_the_modified_color() {
    let mut rig = repository_rig();

    rig.type_text("x");

    let modified = rig.fg_of("git.modified");
    assert_eq!(
        rig.bars_on_line_with("xalpha"),
        vec![(BAR.to_owned(), modified)]
    );
}

#[test]
fn a_new_line_has_a_bar_in_the_added_color() {
    let mut rig = repository_rig();

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::End,
        KeyModifiers::NONE,
    )));
    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    rig.type_text("beta");

    let added = rig.fg_of("git.added");
    assert_eq!(rig.bars_on_line_with("beta"), vec![(BAR.to_owned(), added)]);
}

#[test]
fn undoing_the_change_takes_the_bar_away() {
    let mut rig = repository_rig();
    rig.type_text("x");
    assert_eq!(rig.bars_on_line_with("xalpha").len(), 1);

    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('z'),
        KeyModifiers::CONTROL,
    )));

    assert_eq!(rig.bars_on_line_with("alpha"), vec![]);
}

#[test]
fn a_file_git_does_not_have_yet_is_green_all_through() {
    let mut rig = rig();
    let root = rig.dir.path().to_path_buf();
    git(&root, &["init", "-q", "-b", "main"]);
    rig.app.git_changed();
    rig.refresh();
    rig.take_base();

    let added = rig.fg_of("git.added");

    assert_eq!(
        rig.bars_on_line_with("alpha"),
        vec![(BAR.to_owned(), added)]
    );
}

#[test]
fn a_file_outside_the_repository_has_no_bars() {
    let mut rig = rig();

    rig.app.git_changed();
    rig.refresh();

    assert_eq!(rig.bars_on_line_with("alpha"), vec![]);
}

#[test]
fn the_bars_go_when_git_goes() {
    let mut rig = repository_rig();
    rig.type_text("x");
    assert_eq!(rig.bars_on_line_with("xalpha").len(), 1);

    rig.answer(Err(GitError::NotInstalled));

    assert_eq!(rig.bars_on_line_with("xalpha"), vec![]);
}

#[test]
fn staging_the_change_makes_the_index_the_text_to_compare_with() {
    let mut rig = repository_rig();
    let root = rig.dir.path().to_path_buf();
    // Saved to disk: the buffer and the file are the same text.
    rig.type_text("x");
    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )));
    git(&root, &["add", "a.txt"]);

    rig.app
        .handle_event(Event::FilesChanged(vec![root.join("a.txt")]));
    rig.refresh();
    rig.take_base();

    assert_eq!(rig.bars_on_line_with("xalpha"), vec![]);
}
