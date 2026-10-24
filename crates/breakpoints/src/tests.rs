use std::path::{Path, PathBuf};

use trcode_ids::DocumentId;

use crate::{Answer, Breakpoints, Change, Kind, Saved, Shown, Text};

/// A document for the tests: lines separated by `\n`.
struct Doc(String);

impl Doc {
    fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    /// Replaces the bytes `start..end` with `with`; the change as the breakpoints are told.
    fn edit(&mut self, start: usize, end: usize, with: &str) -> Change {
        self.0.replace_range(start..end, with);
        Change {
            start,
            old_end: end,
            new_end: start + with.len(),
        }
    }

    /// The byte where `line` starts.
    fn at(&self, line: usize) -> usize {
        self.line_start(line)
    }
}

impl Text for Doc {
    fn line_count(&self) -> usize {
        self.0.split('\n').count()
    }

    fn line_start(&self, line: usize) -> usize {
        self.0
            .split('\n')
            .take(line)
            .map(|text| text.len() + 1)
            .sum::<usize>()
            .min(self.0.len())
    }

    fn line_of(&self, byte: usize) -> usize {
        self.0[..byte.min(self.0.len())].matches('\n').count()
    }

    fn line_text(&self, line: usize) -> String {
        self.0.split('\n').nth(line).unwrap_or_default().to_owned()
    }
}

const FILE: &str = "/p/src/main.rs";
const DOC: DocumentId = DocumentId::new(1);

fn path() -> &'static Path {
    Path::new(FILE)
}

fn set(points: &mut Breakpoints, doc: &Doc, lines: &[usize]) {
    for line in lines {
        assert!(points.toggle(path(), DOC, doc, *line));
    }
}

fn lines(points: &Breakpoints) -> Vec<usize> {
    points.lines(path())
}

const SOURCE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";

#[test]
fn a_breakpoint_is_set_and_taken_away_by_the_same_toggle() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();

    assert!(points.toggle(path(), DOC, &doc, 2));
    assert_eq!((points.len(), lines(&points)), (1, vec![2]));
    assert!(!points.toggle(path(), DOC, &doc, 2));

    assert!(points.is_empty());
    assert_eq!(lines(&points), Vec::<usize>::new());
}

#[test]
fn the_lines_are_in_order_and_each_line_has_one() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();

    set(&mut points, &doc, &[3, 1, 2]);

    assert_eq!(lines(&points), [1, 2, 3]);
    assert_eq!(
        points.shown(DOC),
        [
            Shown {
                line: 1,
                kind: Kind::Set
            },
            Shown {
                line: 2,
                kind: Kind::Set
            },
            Shown {
                line: 3,
                kind: Kind::Set
            },
        ]
    );
}

#[test]
fn lines_inserted_above_move_it_down_and_lines_below_leave_it() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    let below = doc.edit(doc.at(4), doc.at(4), "    let d = 4;\n");
    points.remap(DOC, &[below], &doc);
    assert_eq!(lines(&points), [2]);

    let above = doc.edit(doc.at(1), doc.at(1), "    // a note\n    // another\n");
    points.remap(DOC, &[above], &doc);
    assert_eq!(lines(&points), [4]);
    assert_eq!(doc.line_text(4), "    let b = 2;");
}

#[test]
fn lines_taken_away_above_move_it_up() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[3]);

    let change = doc.edit(doc.at(1), doc.at(3), "");
    points.remap(DOC, &[change], &doc);

    assert_eq!(lines(&points), [1]);
    assert_eq!(doc.line_text(1), "    let c = 3;");
}

#[test]
fn typing_in_the_line_keeps_the_breakpoint_on_it() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    let at_start = doc.edit(doc.at(2), doc.at(2), "// ");
    points.remap(DOC, &[at_start], &doc);
    let in_the_middle = doc.edit(doc.at(2) + 6, doc.at(2) + 6, "x");
    points.remap(DOC, &[in_the_middle], &doc);

    assert_eq!(lines(&points), [2]);
}

#[test]
fn a_line_break_at_the_start_of_the_line_takes_the_breakpoint_down_with_its_code() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    let change = doc.edit(doc.at(2), doc.at(2), "\n");
    points.remap(DOC, &[change], &doc);

    assert_eq!(lines(&points), [3]);
    assert_eq!(doc.line_text(3), "    let b = 2;");
}

#[test]
fn a_line_break_in_the_middle_leaves_the_breakpoint_on_the_first_half() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    let change = doc.edit(doc.at(2) + 7, doc.at(2) + 7, "\n    ");
    points.remap(DOC, &[change], &doc);

    assert_eq!(lines(&points), [2]);
}

#[test]
fn a_breakpoint_inside_what_is_replaced_is_dropped_and_one_at_its_start_stays() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1, 2, 3]);

    // From the middle of line 1 to the middle of line 3: the start of line 2 is inside.
    let change = doc.edit(doc.at(1) + 4, doc.at(3) + 4, "");
    points.remap(DOC, &[change], &doc);

    assert_eq!(
        lines(&points),
        [1],
        "lines 2 and 3 started inside, and line 1 stays"
    );
}

#[test]
fn a_line_that_is_joined_to_the_one_before_takes_its_breakpoint_there() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    // Backspace at the start of line 2: the line break before it goes.
    let change = doc.edit(doc.at(2) - 1, doc.at(2), "");
    points.remap(DOC, &[change], &doc);

    assert_eq!(lines(&points), [1]);
}

#[test]
fn two_breakpoints_that_end_up_on_one_line_are_one() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1, 2]);

    let change = doc.edit(doc.at(2) - 1, doc.at(2), "");
    points.remap(DOC, &[change], &doc);

    assert_eq!(lines(&points), [1]);
    assert_eq!(points.len(), 1);
}

#[test]
fn the_other_documents_are_not_touched() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    let other = Doc::new("x\ny\n");

    let change = doc.edit(0, 0, "\n");
    points.remap(DocumentId::new(9), &[change], &other);

    assert_eq!(lines(&points), [2]);
}

#[test]
fn editing_a_line_that_was_moved_makes_it_an_ordinary_breakpoint_again() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    let saved = points.export();
    // The file changed outside the editor: line 2 says something else now.
    let mut changed = Doc::new("fn main() {\n    let a = 1;\n    let z = 26;\n}\n");
    let mut later = Breakpoints::default();
    later.import(saved);
    later.attach(path(), DOC, &changed);
    assert_eq!(
        later.shown(DOC),
        [Shown {
            line: 2,
            kind: Kind::Moved
        }]
    );

    let change = changed.edit(changed.at(2) + 8, changed.at(2) + 9, "y");
    later.remap(DOC, &[change], &changed);

    assert_eq!(
        later.shown(DOC),
        [Shown {
            line: 2,
            kind: Kind::Set
        }]
    );
}

#[test]
fn what_was_remembered_comes_back_on_its_line_when_the_text_is_the_same() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1, 3]);
    let saved = points.export();
    assert_eq!(
        saved,
        [
            Saved {
                path: PathBuf::from(FILE),
                line: 1,
                text: "let a = 1;".into()
            },
            Saved {
                path: PathBuf::from(FILE),
                line: 3,
                text: "let c = 3;".into()
            },
        ]
    );

    let mut next = Breakpoints::default();
    next.import(saved);
    assert_eq!(lines(&next), [1, 3], "known before the file is open");
    assert!(next.shown(DOC).is_empty(), "nothing to draw until it is");
    next.attach(path(), DOC, &doc);

    assert_eq!(
        next.shown(DOC),
        [
            Shown {
                line: 1,
                kind: Kind::Set
            },
            Shown {
                line: 3,
                kind: Kind::Set
            }
        ]
    );
}

#[test]
fn a_line_with_other_text_than_it_was_set_on_is_moved_and_a_line_past_the_end_too() {
    let mut points = Breakpoints::default();
    points.import([
        Saved {
            path: PathBuf::from(FILE),
            line: 1,
            text: "let a = 1;".into(),
        },
        Saved {
            path: PathBuf::from(FILE),
            line: 40,
            text: "let q = 9;".into(),
        },
    ]);
    let doc = Doc::new("fn main() {\n    let renamed = 1;\n}\n");

    points.attach(path(), DOC, &doc);

    assert_eq!(
        points.shown(DOC),
        [
            Shown {
                line: 1,
                kind: Kind::Moved
            },
            Shown {
                line: 3,
                kind: Kind::Moved
            },
        ],
        "the second goes to the last line"
    );
}

#[test]
fn the_blanks_around_a_line_and_what_is_far_along_it_do_not_matter() {
    let mut points = Breakpoints::default();
    let long = format!("    let v = {};", "x".repeat(200));
    let doc = Doc::new(&format!("fn f() {{\n{long}\n}}\n"));
    set(&mut points, &doc, &[1]);
    let saved = points.export();
    assert_eq!(saved[0].text.chars().count(), 80);

    // Indented another way, and the end of it changed.
    let other = Doc::new(&format!(
        "fn f() {{\n\t\tlet v = {}y;\n}}\n",
        "x".repeat(200)
    ));
    let mut next = Breakpoints::default();
    next.import(saved);
    next.attach(path(), DOC, &other);

    assert_eq!(
        next.shown(DOC),
        [Shown {
            line: 1,
            kind: Kind::Set
        }]
    );
}

#[test]
fn a_closed_document_leaves_its_breakpoints_on_the_lines_they_were_on() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[3]);
    let change = doc.edit(doc.at(1), doc.at(1), "// new\n");
    points.remap(DOC, &[change], &doc);
    assert_eq!(points.path_of(DOC), Some(path()));

    points.detach(DOC);

    assert_eq!(lines(&points), [4]);
    assert_eq!(points.path_of(DOC), None);
    assert!(points.shown(DOC).is_empty());
    // An edit that is told after that finds nothing to move.
    points.remap(
        DOC,
        &[Change {
            start: 0,
            old_end: 0,
            new_end: 5,
        }],
        &doc,
    );
    assert_eq!(lines(&points), [4]);
}

#[test]
fn a_file_opened_again_as_another_document_gets_its_breakpoints_back() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    points.detach(DOC);
    let again = DocumentId::new(2);

    points.attach(path(), again, &doc);

    assert_eq!(
        points.shown(again),
        [Shown {
            line: 2,
            kind: Kind::Set
        }]
    );
    assert!(points.shown(DOC).is_empty());
}

#[test]
fn removing_and_clearing() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1, 2, 3]);

    assert_eq!(points.clear(), 3);

    assert!(points.is_empty());
    assert_eq!(points.paths().count(), 0);
}

// --- the adapter ---

#[test]
fn what_the_adapter_has_not_been_told_is_handed_over_once() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2, 3]);

    assert_eq!(points.take_changes(), [(PathBuf::from(FILE), vec![2, 3])]);
    assert!(points.take_changes().is_empty(), "nothing changed");

    let change = doc.edit(0, 0, "// top\n");
    points.remap(DOC, &[change], &doc);
    assert_eq!(
        points.take_changes(),
        [(PathBuf::from(FILE), vec![3, 4])],
        "the lines moved"
    );

    points.toggle(path(), DOC, &doc, 4);
    assert_eq!(points.take_changes(), [(PathBuf::from(FILE), vec![3])]);
}

#[test]
fn a_file_that_has_no_breakpoint_left_is_told_so_once() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    points.take_changes();

    points.toggle(path(), DOC, &doc, 2);

    assert_eq!(points.take_changes(), [(PathBuf::from(FILE), Vec::new())]);
    assert!(points.take_changes().is_empty());
    assert!(points.is_empty());
}

#[test]
fn a_file_that_never_reached_the_adapter_is_not_told_about() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    points.toggle(path(), DOC, &doc, 2);

    assert!(points.take_changes().is_empty());
}

#[test]
fn a_new_session_is_told_everything_again() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);
    points.take_changes();

    points.forget_sent();

    assert_eq!(points.take_changes(), [(PathBuf::from(FILE), vec![2])]);
}

#[test]
fn a_breakpoint_the_adapter_cannot_place_is_hollow_with_its_reason() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[0, 2]);
    points.take_changes();

    points.answered(
        path(),
        &[
            Answer {
                verified: false,
                message: Some("no code at that line".into()),
            },
            Answer {
                verified: true,
                message: None,
            },
        ],
    );

    assert_eq!(
        points.shown(DOC),
        [
            Shown {
                line: 0,
                kind: Kind::Hollow
            },
            Shown {
                line: 2,
                kind: Kind::Set
            }
        ]
    );
    assert_eq!(points.problem(path(), 0), Some("no code at that line"));
    assert_eq!(points.problem(path(), 2), None);
}

#[test]
fn answers_that_do_not_match_the_breakpoints_now_are_not_used() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1, 2]);

    points.answered(
        path(),
        &[Answer {
            verified: false,
            message: None,
        }],
    );
    points.answered(Path::new("/other.rs"), &[]);

    assert_eq!(points.shown(DOC)[0].kind, Kind::Set);
}

#[test]
fn the_end_of_a_session_clears_what_the_adapter_said() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[1]);
    points.answered(
        path(),
        &[Answer {
            verified: false,
            message: None,
        }],
    );
    assert_eq!(points.shown(DOC)[0].kind, Kind::Hollow);

    points.forget_sent();

    assert_eq!(points.shown(DOC)[0].kind, Kind::Set);
}

#[test]
fn a_moved_breakpoint_stays_moved_whatever_the_adapter_says() {
    let mut points = Breakpoints::default();
    points.import([Saved {
        path: PathBuf::from(FILE),
        line: 1,
        text: "old text".into(),
    }]);
    let doc = Doc::new("a\nnew text\n");
    points.attach(path(), DOC, &doc);

    points.answered(
        path(),
        &[Answer {
            verified: true,
            message: None,
        }],
    );

    assert_eq!(points.shown(DOC)[0].kind, Kind::Moved);
}

#[test]
fn what_is_remembered_does_not_overwrite_a_breakpoint_that_is_set() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[2]);

    points.import([Saved {
        path: PathBuf::from(FILE),
        line: 2,
        text: "other".into(),
    }]);

    assert_eq!(points.len(), 1);
    assert_eq!(points.export()[0].text, "let b = 2;");
}

#[test]
fn several_edits_told_together_move_the_breakpoint_as_they_would_one_by_one() {
    let mut doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    set(&mut points, &doc, &[3]);

    // Two lines at the top, then one more line above the breakpoint, then one below it.
    let first = doc.edit(0, 0, "// a\n// b\n");
    let second = doc.edit(doc.at(3), doc.at(3), "// c\n");
    let third = doc.edit(doc.at(7), doc.at(7), "// d\n");
    points.remap(DOC, &[first, second, third], &doc);

    assert_eq!(lines(&points), [6]);
    assert_eq!(doc.line_text(6), "    let c = 3;");
}

#[test]
fn an_edit_to_the_line_in_a_batch_is_found_after_the_edits_that_moved_it() {
    let mut points = Breakpoints::default();
    points.import([Saved {
        path: PathBuf::from(FILE),
        line: 2,
        text: "something else".into(),
    }]);
    let mut doc = Doc::new(SOURCE);
    points.attach(path(), DOC, &doc);
    assert_eq!(points.shown(DOC)[0].kind, Kind::Moved);

    // Lines are added above it, and then its own line is edited.
    let above = doc.edit(0, 0, "// 1\n// 2\n");
    let own = doc.edit(doc.at(4) + 4, doc.at(4) + 7, "var");
    points.remap(DOC, &[above, own], &doc);

    assert_eq!(
        points.shown(DOC),
        [Shown {
            line: 4,
            kind: Kind::Set
        }]
    );
}

#[test]
fn the_documents_with_breakpoints_are_known() {
    let doc = Doc::new(SOURCE);
    let mut points = Breakpoints::default();
    assert!(points.attached().is_empty());

    set(&mut points, &doc, &[2]);
    assert_eq!(points.attached(), [DOC]);

    points.detach(DOC);
    assert!(points.attached().is_empty());
}
