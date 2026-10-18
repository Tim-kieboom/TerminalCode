//! What a plugin sees of git, and what it may do to the repository, through the
//! app: the state is handed to the plugin thread when it changes, pages are asked
//! for and answered there, and changes to the repository are carried out by the
//! app and told back. The plugins show the last message they heard as their view.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::event::Event;
use crate::event::action::Action;
use crate::git::GitError;
use crate::plugin_api::{ActionDecl, Capability};
use crate::plugins::fixtures::{Echo, First};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

const READ: &[Capability] = &[Capability::GitRead];
const WRITE: &[Capability] = &[Capability::GitWrite];
const BOTH: &[Capability] = &[Capability::GitRead, Capability::GitWrite];

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    dir: tempfile::TempDir,
    /// Every different thing the plugin showed, in order.
    seen: Vec<String>,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn project(repository: bool) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(root.join("a.txt"), "alpha\n").unwrap();
    if repository {
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["add", "a.txt"]);
        git(&root, &["commit", "-q", "-m", "first"]);
    }
    std::fs::write(root.join("b.txt"), "beta\n").unwrap();
    (dir, root)
}

fn rig_in(repository: bool) -> Rig {
    let (dir, root) = project(repository);
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.open_project(&root).unwrap();
    state.set_layout(
        LayoutTree::from_ron(
            r#"Row([Pane(view: Editor), Fixed(40, Pane(view: Plugin("hello")))])"#,
        )
        .unwrap(),
    );
    let (tx, events) = mpsc::channel(1024);
    let mut rig = Rig {
        app: App::new(state).with_events(tx),
        events,
        dir,
        seen: Vec::new(),
    };
    rig.app.git_changed();
    rig.wait_until("git to be read", |rig| {
        rig.app.state().git_status().is_some()
            || !rig.app.state().notifications().texts().is_empty()
    });
    rig
}

fn rig() -> Rig {
    rig_in(true)
}

impl Rig {
    fn root(&self) -> PathBuf {
        std::fs::canonicalize(self.dir.path()).unwrap()
    }

    fn view(&self) -> Option<String> {
        let view = self.app.state().plugin_view(&PluginViewId::new("hello"))?;
        match view.content() {
            ViewNode::Lines(lines) => lines.first().map(|line| line.text.to_string()),
            _ => None,
        }
    }

    fn pump(&mut self) {
        if self
            .app
            .git_deadline()
            .is_some_and(|at| Instant::now() >= at)
        {
            self.app.git_due();
        }
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
            if let Some(text) = self.view()
                && self.seen.last() != Some(&text)
            {
                self.seen.push(text);
            }
        }
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn load(&mut self, plugin: Echo, grants: &[Capability]) {
        self.app.load_plugin(
            "hello",
            plugin.build(),
            vec![ActionDecl::new("go", "Go")],
            grants,
        );
        self.wait_until("the plugin to start", |rig| rig.view().is_some());
    }

    /// Runs the action that makes the plugin ask, and waits for a message that is
    /// not the action's own echo and satisfies `wanted`.
    fn ask(&mut self, wanted: &str) -> String {
        self.app.run_action(Action::Plugin("hello:go".into()));
        let before = self.seen.len();
        let marker = wanted.to_owned();
        self.wait_until(wanted, |rig| {
            rig.seen[before.min(rig.seen.len())..]
                .iter()
                .any(|text| text.contains(&marker))
        });
        self.seen
            .iter()
            .rev()
            .find(|text| text.contains(wanted))
            .cloned()
            .unwrap()
    }

    fn change_and_read_again(&mut self, change: impl FnOnce(&Path)) {
        let before = self.app.git_generation();
        change(&self.root());
        self.app
            .handle_event(Event::FilesChanged(vec![self.root()]));
        self.wait_until("a new state of the repository", |rig| {
            rig.app.git_generation() > before
        });
    }
}

const ASK_UNTRACKED: &str =
    r#"{"type":"git_entries","request":5,"section":"untracked","offset":0,"limit":10}"#;

fn asking(request: &str) -> Echo {
    Echo::new(First::Echo).asks_on_action(request)
}

// --- being told ---

#[test]
fn a_plugin_that_may_read_git_is_told_how_the_repository_is() {
    let mut rig = rig();
    let oid = rig
        .app
        .state()
        .git_status()
        .unwrap()
        .branch
        .oid
        .clone()
        .unwrap();

    rig.app
        .load_plugin("hello", Echo::new(First::Echo).build(), vec![], READ);
    rig.wait_until("the summary", |rig| {
        rig.view().is_some_and(|text| text.contains("git_changed"))
    });

    assert_eq!(
        rig.view().unwrap(),
        format!(
            r#"{{"type":"git_changed","summary":{{"generation":1,"branch":"main","commit":"{oid}","upstream":null,"ahead":0,"behind":0,"staged":0,"changes":0,"untracked":1,"conflicts":0}}}}"#
        )
    );
}

#[test]
fn a_plugin_that_may_not_read_git_is_told_nothing() {
    let mut rig = rig();

    rig.load(Echo::new(First::Echo), &[]);
    let waited_until = Instant::now() + Duration::from_millis(400);
    while Instant::now() < waited_until {
        rig.pump();
        std::thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(rig.view().as_deref(), Some(r#"{"type":"start"}"#));
}

#[test]
fn a_plugin_is_told_again_when_the_repository_changes() {
    let mut rig = rig();
    rig.load(Echo::new(First::Echo), READ);
    rig.wait_until("the first summary", |rig| {
        rig.view()
            .is_some_and(|text| text.contains(r#""generation":1"#))
    });

    rig.change_and_read_again(|root| std::fs::write(root.join("c.txt"), "gamma\n").unwrap());

    rig.wait_until("the second summary", |rig| {
        rig.view()
            .is_some_and(|text| text.contains(r#""generation":2"#))
    });
    assert!(rig.view().unwrap().contains(r#""untracked":2"#));
}

#[test]
fn a_plugin_is_told_when_git_goes_away() {
    let mut rig = rig();
    rig.load(Echo::new(First::Echo), READ);
    rig.wait_until("the summary", |rig| {
        rig.view().is_some_and(|text| text.contains("git_changed"))
    });

    rig.app.handle_event(Event::GitStatus {
        took: Duration::from_millis(1),
        result: Err(GitError::NotInstalled),
    });

    rig.wait_until("the news", |rig| {
        rig.view().as_deref() == Some(r#"{"type":"git_unavailable"}"#)
    });
}

// --- asking ---

#[test]
fn a_page_of_the_files_git_lists_is_answered() {
    let mut rig = rig();
    rig.load(asking(ASK_UNTRACKED), READ);

    let answer = rig.ask(r#""type":"git_entries""#);

    assert_eq!(
        answer,
        r#"{"type":"git_entries","request":5,"generation":1,"section":"untracked","offset":0,"total":1,"entries":[{"path":"b.txt","original":null,"kind":"untracked"}]}"#
    );
}

#[test]
fn a_page_asked_for_after_a_change_has_the_new_generation() {
    let mut rig = rig();
    rig.load(asking(ASK_UNTRACKED), READ);
    rig.change_and_read_again(|root| std::fs::write(root.join("c.txt"), "gamma\n").unwrap());

    let answer = rig.ask(r#""type":"git_entries""#);

    assert!(answer.contains(r#""generation":2"#), "{answer}");
    assert!(answer.contains(r#""total":2"#), "{answer}");
}

#[test]
fn asking_without_the_capability_is_refused_and_says_which() {
    let mut rig = rig();
    rig.load(asking(ASK_UNTRACKED), &[]);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":5,"reason":{"type":"denied","capability":"git.read"}}"#
    );
}

#[test]
fn asking_in_a_folder_that_is_no_repository_is_refused() {
    let mut rig = rig_in(false);
    rig.load(asking(ASK_UNTRACKED), READ);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":5,"reason":{"type":"no_repository"}}"#
    );
}

// --- changing the repository ---

const STAGE_B: &str = r#"{"type":"git_stage","request":6,"path":"b.txt"}"#;

fn staged(rig: &Rig) -> String {
    git(&rig.root(), &["diff", "--cached", "--name-only"])
        .trim()
        .to_owned()
}

#[test]
fn a_plugin_that_may_write_stages_a_file_and_is_told_it_is_done() {
    let mut rig = rig();
    rig.load(asking(STAGE_B), BOTH);

    let answer = rig.ask(r#"{"type":"done","request":6}"#);

    assert_eq!(answer, r#"{"type":"done","request":6}"#);
    assert_eq!(staged(&rig), "b.txt");
}

#[test]
fn the_plugin_is_told_the_new_state_soon_after_a_change_it_made() {
    let mut rig = rig();
    rig.load(asking(STAGE_B), BOTH);
    let started = Instant::now();

    rig.ask(r#"{"type":"done","request":6}"#);
    rig.wait_until("the summary with the staged file", |rig| {
        rig.view()
            .is_some_and(|text| text.contains(r#""staged":1"#))
    });

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no wait for a burst of changes to end"
    );
    assert!(rig.view().unwrap().contains(r#""untracked":0"#));
}

#[test]
fn a_file_is_unstaged_again() {
    let mut rig = rig();
    git(&rig.root(), &["add", "b.txt"]);
    rig.load(
        asking(r#"{"type":"git_unstage","request":7,"path":"b.txt"}"#),
        WRITE,
    );
    assert_eq!(staged(&rig), "b.txt");

    let answer = rig.ask(r#"{"type":"done","request":7}"#);

    assert_eq!(answer, r#"{"type":"done","request":7}"#);
    assert_eq!(staged(&rig), "");
}

#[test]
fn a_change_without_the_capability_is_refused_and_nothing_changes() {
    let mut rig = rig();
    rig.load(asking(STAGE_B), READ);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":6,"reason":{"type":"denied","capability":"git.write"}}"#
    );
    assert_eq!(staged(&rig), "");
}

#[test]
fn a_path_outside_the_repository_is_refused_by_the_action() {
    let mut rig = rig();
    rig.load(
        asking(r#"{"type":"git_stage","request":6,"path":"../outside.txt"}"#),
        WRITE,
    );

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":6,"reason":{"type":"bad_argument","detail":"`../outside.txt` goes out of the repository"}}"#
    );
}

#[test]
fn what_git_says_when_it_cannot_is_told_to_the_plugin() {
    let mut rig = rig();
    rig.load(
        asking(r#"{"type":"git_stage","request":6,"path":"nothing-here.txt"}"#),
        WRITE,
    );

    let answer = rig.ask(r#""type":"refused""#);

    assert!(answer.contains(r#""type":"failed""#), "{answer}");
    assert!(answer.contains("nothing-here.txt"), "{answer}");
}

#[test]
fn staging_in_a_folder_that_is_no_repository_is_refused() {
    let mut rig = rig_in(false);
    rig.load(asking(STAGE_B), WRITE);

    let answer = rig.ask(r#""type":"refused""#);

    assert!(answer.contains(r#""type":"no_repository""#), "{answer}");
}

// --- opening files ---

const OPEN_A: &str = r#"{"type":"open_file","request":8,"path":"a.txt"}"#;
const OPEN: &[Capability] = &[Capability::EditorOpen];

fn open_file_name(rig: &Rig) -> Option<String> {
    let editor = rig.app.state().editor();
    let path = editor.buffer().path()?;
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

#[test]
fn a_plugin_that_may_opens_a_file_of_the_repository_in_the_editor() {
    let mut rig = rig();
    rig.load(asking(OPEN_A), OPEN);

    let answer = rig.ask(r#"{"type":"done","request":8}"#);

    assert_eq!(answer, r#"{"type":"done","request":8}"#);
    assert_eq!(open_file_name(&rig).as_deref(), Some("a.txt"));
}

#[test]
fn opening_a_file_leaves_the_keyboard_in_the_plugin_pane() {
    let mut rig = rig();
    rig.load(asking(OPEN_A), OPEN);
    rig.app.run_action(Action::FocusPluginView("hello".into()));

    rig.ask(r#"{"type":"done","request":8}"#);

    assert_eq!(rig.app.state().focus(), crate::app::state::Focus::Plugin);
}

#[test]
fn opening_a_file_without_the_capability_is_refused_and_nothing_opens() {
    let mut rig = rig();
    rig.load(asking(OPEN_A), READ);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":8,"reason":{"type":"denied","capability":"editor.open"}}"#
    );
    assert_eq!(open_file_name(&rig), None);
}

#[test]
fn a_path_outside_the_repository_is_not_opened() {
    let mut rig = rig();
    rig.load(
        asking(r#"{"type":"open_file","request":8,"path":"../outside.txt"}"#),
        OPEN,
    );

    let answer = rig.ask(r#""type":"refused""#);

    assert!(answer.contains(r#""type":"bad_argument""#), "{answer}");
}

#[test]
fn something_that_is_not_a_file_is_not_opened() {
    let mut rig = rig();
    rig.load(
        asking(r#"{"type":"open_file","request":8,"path":"nothing-here.txt"}"#),
        OPEN,
    );

    let answer = rig.ask(r#""type":"refused""#);

    assert!(answer.contains(r#""type":"failed""#), "{answer}");
    assert!(answer.contains("is not a file"), "{answer}");
}

#[test]
fn a_file_is_opened_from_the_project_when_there_is_no_repository() {
    let mut rig = rig_in(false);
    rig.load(asking(OPEN_A), OPEN);

    let answer = rig.ask(r#"{"type":"done","request":8}"#);

    assert_eq!(answer, r#"{"type":"done","request":8}"#);
    assert_eq!(open_file_name(&rig).as_deref(), Some("a.txt"));
}

// --- stage all, unstage all, commit ---

const STAGE_ALL: &str = r#"{"type":"git_stage_all","request":11}"#;
const UNSTAGE_ALL: &str = r#"{"type":"git_unstage_all","request":12}"#;

fn commit_request(message: &str) -> String {
    format!(r#"{{"type":"git_commit","request":13,"message":"{message}"}}"#)
}

/// A rig whose repository may commit whatever the machine's own git config says.
fn committing_rig() -> Rig {
    let rig = rig();
    let root = rig.root();
    git(&root, &["config", "user.name", "Test"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    rig
}

fn subjects(rig: &Rig) -> String {
    git(&rig.root(), &["log", "--format=%s"]).trim().to_owned()
}

#[test]
fn a_plugin_that_may_write_stages_everything_and_is_told_it_is_done() {
    let mut rig = rig();
    rig.load(asking(STAGE_ALL), WRITE);

    let answer = rig.ask(r#"{"type":"done","request":11}"#);

    assert_eq!(answer, r#"{"type":"done","request":11}"#);
    assert_eq!(staged(&rig), "b.txt");
}

#[test]
fn everything_is_unstaged_again() {
    let mut rig = rig();
    git(&rig.root(), &["add", "b.txt"]);
    rig.load(asking(UNSTAGE_ALL), WRITE);
    assert_eq!(staged(&rig), "b.txt");

    let answer = rig.ask(r#"{"type":"done","request":12}"#);

    assert_eq!(answer, r#"{"type":"done","request":12}"#);
    assert_eq!(staged(&rig), "");
}

#[test]
fn a_plugin_that_may_write_commits_what_is_staged() {
    let mut rig = committing_rig();
    git(&rig.root(), &["add", "b.txt"]);
    rig.load(asking(&commit_request("Add b")), WRITE);

    let answer = rig.ask(r#"{"type":"done","request":13}"#);

    assert_eq!(answer, r#"{"type":"done","request":13}"#);
    assert_eq!(subjects(&rig), "Add b\nfirst");
    assert_eq!(staged(&rig), "");
}

#[test]
fn what_git_says_after_a_commit_is_read_at_once() {
    let mut rig = committing_rig();
    git(&rig.root(), &["add", "b.txt"]);
    rig.load(asking(&commit_request("Add b")), WRITE);
    let started = Instant::now();

    rig.ask(r#"{"type":"done","request":13}"#);

    // The file that was untracked is committed, so nothing is left to tell about.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "no wait for a burst of changes to end"
    );
}

#[test]
fn an_empty_commit_message_is_refused_as_a_bad_argument() {
    let mut rig = committing_rig();
    git(&rig.root(), &["add", "b.txt"]);
    rig.load(asking(&commit_request("  ")), WRITE);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":13,"reason":{"type":"bad_argument","detail":"the commit message is empty"}}"#
    );
    assert_eq!(subjects(&rig), "first");
}

#[test]
fn a_commit_with_nothing_staged_is_refused_with_its_own_reason() {
    let mut rig = committing_rig();
    rig.load(asking(&commit_request("Nothing")), WRITE);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":13,"reason":{"type":"nothing_staged"}}"#
    );
}

#[test]
fn the_three_new_requests_need_the_capability_to_write() {
    for (request, id) in [
        (STAGE_ALL.to_owned(), 11),
        (UNSTAGE_ALL.to_owned(), 12),
        (commit_request("x"), 13),
    ] {
        let mut rig = rig();
        git(&rig.root(), &["add", "b.txt"]);
        rig.load(asking(&request), READ);

        let answer = rig.ask(r#""type":"refused""#);

        assert_eq!(
            answer,
            format!(
                r#"{{"type":"refused","request":{id},"reason":{{"type":"denied","capability":"git.write"}}}}"#
            )
        );
        assert_eq!(staged(&rig), "b.txt", "nothing changed");
        assert_eq!(subjects(&rig), "first");
    }
}

#[test]
fn the_new_requests_in_a_folder_that_is_no_repository_are_refused() {
    let mut rig = rig_in(false);
    rig.load(asking(STAGE_ALL), WRITE);

    let answer = rig.ask(r#""type":"refused""#);

    assert_eq!(
        answer,
        r#"{"type":"refused","request":11,"reason":{"type":"no_repository"}}"#
    );
}
