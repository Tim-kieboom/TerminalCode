//! What a server says about files that are not open (a checker reports the whole project): the
//! Errors view lists it, and a row opens the file.

use super::*;

use crate::components::errors::{ErrorTarget, ErrorsCommand};
use crate::event::action::Action;

/// A rig whose server publishes one problem about the file `about` makes (from the project's
/// folder) when it is told about the file in front, and, when `then_nothing` is set, nothing at
/// all about it on the next change. The file is written, with three lines.
fn about_another_file(
    about: impl FnOnce(&Path) -> PathBuf,
    then_nothing: bool,
) -> (Rig, tempfile::TempDir, PathBuf) {
    let (dir, file) = rust_project();
    let other = about(dir.path());
    write(&other, "fn a() {}\n\nlet x: u8 = 1000;\n");
    let uri = json!(crate::lsp::uri::file_uri(&other).unwrap());
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        let uri = uri.clone();
        responding(sent.clone(), move |message| match method_of(message) {
            Some("textDocument/didOpen") => vec![publication(
                &uri,
                json!(null),
                vec![diagnostic_json(
                    (2, 4),
                    (2, 9),
                    1,
                    "mismatched types\nexpected `u8`",
                )],
            )],
            Some("textDocument/didChange") if then_nothing => {
                vec![publication(&uri, json!(null), Vec::new())]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), std::slice::from_ref(&file), launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    (rig, dir, other)
}

fn in_project(project: &Path) -> PathBuf {
    project.join("src/other.rs")
}

fn rows(rig: &Rig) -> Vec<(String, ErrorTarget)> {
    rig.app
        .state()
        .error_rows()
        .into_iter()
        .map(|row| (format!("{} {}", row.place(), row.message), row.target))
        .collect()
}

#[test]
fn a_problem_in_a_file_that_is_not_open_is_listed_with_its_file_and_place() {
    let (mut rig, _dir, other) = about_another_file(in_project, false);

    rig.wait_until("the row", |rig| !rows(rig).is_empty());

    let rows = rows(&rig);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, "src/other.rs:3:5 mismatched types");
    assert_eq!(rows[0].1, ErrorTarget::File(other));
}

#[test]
fn a_file_outside_the_project_is_not_listed() {
    let elsewhere = tempfile::tempdir().unwrap();
    let path = elsewhere.path().join("outside.rs");
    let (mut rig, _dir, _other) = about_another_file(|_| path.clone(), false);
    std::thread::sleep(Duration::from_millis(150));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rows(&rig).is_empty(), "{:?}", rows(&rig));
}

#[test]
fn publishing_nothing_about_the_file_takes_its_row_away() {
    let (mut rig, _dir, _other) = about_another_file(in_project, true);
    rig.wait_until("the row", |rig| !rows(rig).is_empty());

    rig.type_text("x");
    rig.sync();

    rig.wait_until("the row to go", |rig| rows(rig).is_empty());
}

#[test]
fn the_tab_of_the_view_counts_the_problems_of_closed_files() {
    let (mut rig, _dir, _other) = about_another_file(in_project, false);

    rig.wait_until("the row", |rig| !rows(rig).is_empty());

    rig.app.run_action(Action::ShowErrors);
    let screen = rig.screen();
    assert!(screen.contains("Errors (1)"), "{screen}");
    assert!(screen.contains("src/other.rs:3:5"), "{screen}");
}

#[test]
fn a_row_opens_the_file_with_the_cursor_on_the_problem() {
    let (mut rig, _dir, other) = about_another_file(in_project, false);
    rig.wait_until("the row", |rig| !rows(rig).is_empty());

    rig.app.run_action(Action::Errors(ErrorsCommand::Open));

    let state = rig.app.state();
    assert_eq!(state.editor().buffer().path(), Some(other.as_path()));
    let head = state.editor().selections().primary().head();
    assert_eq!((head.line, head.column), (2, 4));
    assert!(
        rows(&rig).is_empty(),
        "an open file is listed by its document"
    );
}
