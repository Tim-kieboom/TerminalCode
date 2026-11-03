//! A diff of the file in front against `HEAD`: asked for from the palette, shown in a tab,
//! and followed when `HEAD` moves.

#![cfg(unix)]

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
use crate::event::action::Action;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    dir: tempfile::TempDir,
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
}

/// A repository with `a.txt` committed as `committed`, and `a.txt` open in the editor as it
/// is on disk now, `working`.
fn rig(committed: &str, working: &str) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), committed).unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-q", "-m", "first"]);
    std::fs::write(root.join("a.txt"), working).unwrap();

    let mut state = AppState::default();
    state.open_project(root).unwrap();
    state
        .workspace_mut()
        .open_path(&root.join("a.txt"))
        .unwrap();
    let (tx, events) = mpsc::channel(1024);
    let mut rig = Rig {
        app: App::new(state).with_events(tx),
        events,
        terminal: Terminal::new(TestBackend::new(120, 20)).unwrap(),
        dir,
    };
    rig.app.git_changed();
    rig.refresh_git();
    rig
}

impl Rig {
    fn root(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn said(&self) -> Vec<String> {
        self.app
            .state()
            .notifications()
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Waits for the refresh that is wanted and handles its answer (and what that asks for).
    fn refresh_git(&mut self) {
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
        self.pump_until("the answer of git", |event| {
            matches!(event, Event::GitStatus { .. })
        });
    }

    /// Handles events until one that `last` says is the last has been handled.
    fn pump_until(&mut self, what: &str, last: impl Fn(&Event) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    let done = last(&event);
                    self.app.handle_event(event);
                    if done {
                        return;
                    }
                }
                Err(_) => {
                    assert!(Instant::now() < deadline, "timed out waiting for {what}");
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    fn open_diff(&mut self) {
        self.app.run_action(Action::GitOpenDiff);
        self.pump_until("the text of HEAD", |event| {
            matches!(event, Event::GitHead { .. })
        });
    }
}

#[test]
fn open_diff_shows_the_committed_text_next_to_the_one_being_edited() {
    let mut rig = rig("alpha\nbeta\n", "alpha\nBETA\n");

    rig.open_diff();

    let screen = rig.screen();
    assert!(screen.contains("(diff)"), "{screen}");
    assert!(
        screen.contains("beta") && screen.contains("BETA"),
        "{screen}"
    );
    assert!(
        screen.contains("HEAD ") && screen.contains("Working copy"),
        "{screen}"
    );
}

#[test]
fn a_file_that_is_not_in_the_repository_yet_has_nothing_on_the_left() {
    let mut rig = rig("alpha\n", "alpha\n");
    let fresh = rig.root().join("fresh.txt");
    std::fs::write(&fresh, "only here\n").unwrap();
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&fresh)
        .unwrap();

    rig.open_diff();

    let screen = rig.screen();
    assert!(
        screen.contains("(diff)") && screen.contains("only here"),
        "{screen}"
    );
}

#[test]
fn a_buffer_without_a_file_says_to_save_it_first() {
    let mut rig = rig("alpha\n", "alpha\n");
    rig.app.state_mut().workspace_mut().new_file();

    rig.app.run_action(Action::GitOpenDiff);

    assert!(
        rig.said()
            .iter()
            .any(|text| text.contains("save this one first")),
        "{:?}",
        rig.said()
    );
}

#[test]
fn a_binary_file_is_not_compared() {
    let mut rig = rig("alpha\n", "alpha\n");
    let root = rig.root();
    // HEAD gets a binary a.txt; the editor still has the text.
    std::fs::write(root.join("a.txt"), [0u8, 1, 2, 0, 255]).unwrap();
    git(&root, &["commit", "-q", "-a", "-m", "binary"]);
    std::fs::write(root.join("a.txt"), "alpha\n").unwrap();

    rig.open_diff();

    assert!(
        rig.said().iter().any(|text| text.contains("not text")),
        "{:?}",
        rig.said()
    );
    assert!(!rig.app.state().workspace().active_is_diff());
}

#[test]
fn a_commit_makes_the_left_side_the_new_head_and_leaves_no_changes() {
    let mut rig = rig("alpha\nbeta\n", "alpha\nBETA\n");
    rig.open_diff();
    assert!(!rig.screen().contains("no changes"));

    let root = rig.root();
    git(&root, &["commit", "-q", "-a", "-m", "second"]);
    rig.app.git_changed();
    rig.refresh_git();
    rig.pump_until("the new text of HEAD", |event| {
        matches!(event, Event::GitHead { open: false, .. })
    });

    let screen = rig.screen();
    assert!(screen.contains("no changes"), "{screen}");
    assert!(screen.contains("(diff)"), "the tab stays: {screen}");
}

#[test]
fn next_change_outside_a_diff_says_so() {
    let mut rig = rig("alpha\n", "alpha\n");

    rig.app.run_action(Action::DiffNextChange);

    assert!(
        rig.said().iter().any(|text| text.contains("not a diff")),
        "{:?}",
        rig.said()
    );
}

#[test]
fn ctrl_click_on_the_marker_of_the_gutter_opens_the_diff_and_a_plain_click_does_not() {
    use crossterm::event::{
        Event as InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    let mut rig = rig("alpha\nbeta\n", "alpha\nBETA\n");
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("BETA"))
        .unwrap();
    // The marker is the column just before the text (the screen's lines are quoted).
    let line = line.trim_start_matches('"');
    let column = line[..line.find("BETA").unwrap()].chars().count() - 1;
    let mouse = |modifiers| {
        InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: column as u16,
            row: row as u16,
            modifiers,
        })
    };

    rig.app.handle_input(mouse(KeyModifiers::NONE));
    assert!(!rig.app.state().workspace().active_is_diff());

    rig.app.handle_input(mouse(KeyModifiers::CONTROL));
    rig.pump_until("the text of HEAD", |event| {
        matches!(event, Event::GitHead { .. })
    });

    assert!(rig.app.state().workspace().active_is_diff());
}
