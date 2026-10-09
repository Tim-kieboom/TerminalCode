use std::fs;
use std::ops::Range;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::style::Color;

use crate::buffer::{Buffer, Edit};
use crate::syntax::document::Parses;
use crate::syntax::worker::{Output, Parse};
use crate::syntax::{DocumentSyntax, SyntaxWorker};
use crate::ui::theme::Theme;

/// A list of just one range.
fn one(range: Range<usize>) -> Vec<Range<usize>> {
    vec![range]
}

fn theme() -> Theme {
    Theme::from_toml("[\"syntax.keyword\"]\ntext = \"red\"\n").unwrap()
}

/// A document, its syntax and a real worker, with the worker's answers
/// delivered by hand so a test decides when each one is taken.
struct Rig {
    syntax: DocumentSyntax,
    buffer: Buffer,
    worker: SyntaxWorker,
    answers: mpsc::Receiver<Output>,
    dir: tempfile::TempDir,
}

impl Rig {
    fn new(name: &str, text: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        let (tx, answers) = mpsc::channel();
        Self {
            syntax: DocumentSyntax::new(1),
            buffer: Buffer::open(path).unwrap(),
            worker: SyntaxWorker::spawn(theme(), move |output| {
                let _ = tx.send(output);
            }),
            answers,
            dir,
        }
    }

    fn rs(text: &str) -> Self {
        Self::new("a.rs", text)
    }

    fn whole(&self) -> Vec<Range<usize>> {
        one(0..self.buffer.len_bytes())
    }

    /// Asks the worker for spans of the whole text.
    fn ask(&mut self) {
        let ranges = self.whole();
        self.ask_for(&ranges);
    }

    fn ask_for(&mut self, ranges: &[Range<usize>]) {
        self.syntax.update(&self.buffer, ranges, &self.worker);
    }

    /// Waits for the worker's next answer and takes it.
    fn answer(&mut self) -> bool {
        let output = self
            .answers
            .recv_timeout(Duration::from_secs(10))
            .expect("the worker answered");
        self.syntax.accept(output)
    }

    /// Whether an answer shows up soon.
    fn answers_soon(&self) -> bool {
        self.answers
            .recv_timeout(Duration::from_millis(200))
            .is_ok()
    }

    fn settle(&mut self) {
        self.ask();
        self.answer();
    }

    /// Replaces `range` and tells the syntax, as the workspace does.
    fn edit(&mut self, range: Range<usize>, text: &str) {
        self.change(range, text);
        self.syntax.record_edits(&self.buffer.take_edit_log());
    }

    /// Replaces `range` without telling the syntax, like a reload.
    fn change(&mut self, range: Range<usize>, text: &str) {
        let mut transaction = self.buffer.begin_transaction(Default::default());
        transaction.apply(&Edit::new(range, text)).unwrap();
        transaction.commit(Default::default());
    }

    fn red(&self) -> Vec<String> {
        let text = self.buffer.text();
        self.syntax
            .spans()
            .iter()
            .filter(|span| span.style.fg == Some(Color::Red))
            .map(|span| text[span.range.clone()].to_owned())
            .collect()
    }

    fn starts(&self) -> Vec<usize> {
        self.syntax.spans().iter().map(|s| s.range.start).collect()
    }
}

fn parses(full: usize, incremental: usize, reused: usize) -> Parses {
    Parses {
        full,
        incremental,
        reused,
    }
}

#[test]
fn a_rust_file_gets_spans_for_the_ranges_asked() {
    let mut rig = Rig::rs("fn a() {}\nfn b() {}\n");

    rig.settle();

    assert_eq!(rig.red(), ["fn", "fn"]);
    assert_eq!(rig.syntax.parses(), parses(1, 0, 0));
}

#[test]
fn only_the_given_ranges_are_colored() {
    let mut rig = Rig::rs("fn a() {}\nfn b() {}\nfn c() {}\n");

    rig.ask_for(&[0..9, 20..29]);
    rig.answer();

    assert_eq!(rig.starts(), [0, 20]);
}

#[test]
fn a_file_with_no_known_language_is_never_sent_to_the_worker() {
    let mut rig = Rig::new("notes.txt", "fn a() {}\n");

    rig.ask();

    assert!(!rig.answers_soon());
    assert!(rig.syntax.spans().is_empty());
    assert!(rig.syntax.take_error().is_none());
}

#[test]
fn a_buffer_without_a_path_is_never_sent_to_the_worker() {
    let mut rig = Rig::rs("");
    rig.buffer = Buffer::from_text("fn a() {}\n");

    rig.ask();

    assert!(!rig.answers_soon());
}

#[test]
fn asking_again_for_the_same_thing_sends_nothing() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();

    rig.ask();

    assert!(!rig.answers_soon());
}

#[test]
fn an_edit_that_was_recorded_is_parsed_from_the_old_tree() {
    let mut rig = Rig::rs("fn a() {}\nlet b = 1;\n");
    rig.settle();

    rig.edit(0..2, "pub fn");
    rig.settle();

    assert_eq!(rig.syntax.parses(), parses(1, 1, 0));
    assert_eq!(rig.red(), ["pub", "fn", "let"]);
}

#[test]
fn several_edits_between_two_asks_are_one_incremental_parse() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();

    rig.edit(9..9, "\nlet x");
    rig.edit(15..15, " = 1;");
    rig.edit(0..0, "pub ");
    rig.settle();

    assert_eq!(rig.syntax.parses(), parses(1, 1, 0));
    assert_eq!(rig.red(), ["pub", "fn", "let"]);
}

#[test]
fn a_change_nobody_reported_is_parsed_in_full_and_the_old_spans_go() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();

    rig.change(0..2, "let");
    rig.ask();
    assert!(
        rig.syntax.spans().is_empty(),
        "wrong colors are worse than none"
    );
    rig.answer();

    assert_eq!(rig.syntax.parses(), parses(2, 0, 0));
    assert_eq!(rig.red(), ["let"]);
}

#[test]
fn spans_follow_an_edit_before_the_worker_has_answered() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();
    assert_eq!(rig.starts(), [0]);

    rig.edit(0..0, "    ");

    assert_eq!(rig.starts(), [4], "shifted without waiting for a parse");
}

#[test]
fn an_answer_for_older_text_is_moved_forward_through_the_edits_since() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.ask();
    // Typed after the question went out and before the answer came back.
    rig.edit(0..0, "    ");

    rig.answer();

    assert_eq!(rig.starts(), [4]);
    assert_eq!(rig.syntax.parses(), parses(1, 0, 0));
}

#[test]
fn an_answer_older_than_the_spans_on_screen_is_ignored() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();
    rig.edit(0..0, "    ");
    rig.settle();
    let shown = rig.syntax.spans().to_vec();

    let late = Output {
        key: 1,
        version: 0,
        spans: Vec::new(),
        parse: Parse::Full,
        failure: None,
    };

    assert!(!rig.syntax.accept(late));
    assert_eq!(rig.syntax.spans(), shown);
}

#[test]
fn an_answer_from_the_future_is_ignored() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();

    let bogus = Output {
        key: 1,
        version: 99,
        spans: Vec::new(),
        parse: Parse::Full,
        failure: None,
    };

    assert!(!rig.syntax.accept(bogus));
    assert_eq!(rig.red(), ["fn"]);
}

#[test]
fn an_answer_with_no_record_of_the_text_is_ignored() {
    let mut rig = Rig::rs("fn a() {}\n");

    let early = Output {
        key: 1,
        version: 0,
        spans: Vec::new(),
        parse: Parse::Full,
        failure: None,
    };

    assert!(!rig.syntax.accept(early));
}

#[test]
fn moving_the_range_asks_again_and_reuses_the_tree() {
    let mut rig = Rig::rs("fn a() {}\nfn b() {}\n");
    rig.ask_for(&one(0..9));
    rig.answer();
    assert_eq!(rig.starts(), [0]);

    rig.ask_for(&one(10..19));
    rig.answer();

    assert_eq!(rig.starts(), [10]);
    assert_eq!(rig.syntax.parses(), parses(1, 0, 1));
}

#[test]
fn renaming_to_another_extension_switches_highlighting_off_and_on() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();
    assert!(!rig.syntax.spans().is_empty());

    rig.buffer.set_path(rig.dir.path().join("a.txt"));
    rig.ask();
    assert!(rig.syntax.spans().is_empty());

    rig.buffer.set_path(rig.dir.path().join("a.rs"));
    rig.settle();
    assert!(!rig.syntax.spans().is_empty());
}

#[test]
fn a_forgotten_document_is_parsed_from_scratch_when_it_comes_back() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();

    rig.worker.forget(1);
    rig.edit(0..0, "pub ");
    rig.settle();

    assert_eq!(rig.syntax.parses(), parses(2, 0, 0));
    assert_eq!(rig.red(), ["pub", "fn"]);
}

#[test]
fn a_pile_of_edits_is_dropped_and_the_next_parse_is_in_full() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.settle();
    rig.change(0..0, " ");
    let info = rig.buffer.take_edit_log()[0];

    // Far more edits than are kept.
    rig.syntax.record_edits(&vec![info; 10_001]);
    assert!(rig.syntax.spans().is_empty());
    rig.settle();

    assert_eq!(rig.syntax.parses(), parses(2, 0, 0));
    assert_eq!(rig.red(), ["fn"]);
}

#[test]
fn a_document_the_worker_cannot_highlight_reports_once_and_stops_asking() {
    let mut rig = Rig::rs("fn a() {}\n");
    rig.ask();
    let failure = Output {
        key: 1,
        version: 0,
        spans: Vec::new(),
        parse: Parse::Full,
        failure: Some("no grammar".to_owned()),
    };

    assert!(rig.syntax.accept(failure));

    assert_eq!(rig.syntax.take_error().as_deref(), Some("no grammar"));
    assert_eq!(rig.syntax.take_error(), None);
    // The worker really did parse that first job; let its answer arrive and drop it.
    rig.answers.recv_timeout(Duration::from_secs(10)).unwrap();
    rig.edit(0..0, "x");
    rig.ask();
    assert!(!rig.answers_soon());
}

/// Every step of an edit sequence gives the spans a parse from scratch gives.
fn assert_incremental_matches_fresh(start: &str, steps: &[(Range<usize>, &str)]) {
    let mut rig = Rig::rs(start);
    rig.settle();
    for (range, text) in steps {
        rig.edit(range.clone(), text);
        rig.settle();

        let mut fresh = Rig::rs(&rig.buffer.text());
        fresh.settle();
        assert_eq!(
            rig.syntax.spans(),
            fresh.syntax.spans(),
            "after replacing {range:?} with {text:?}: {:?}",
            rig.buffer.text()
        );
    }
    assert!(rig.syntax.parses().incremental > 0);
}

#[test]
fn typing_a_function_one_key_at_a_time_matches_a_fresh_parse() {
    let source = "fn f(a: u8) -> u8 { a } // x\n";
    let steps: Vec<_> = source
        .char_indices()
        .map(|(at, c)| (at..at, &source[at..at + c.len_utf8()]))
        .collect();

    assert_incremental_matches_fresh("", &steps);
}

#[test]
fn deleting_and_replacing_matches_a_fresh_parse() {
    let start = "use std::fmt;\nfn a() { let s = \"x\"; }\nstruct P { x: u32 }\n";
    assert_incremental_matches_fresh(
        start,
        &[
            (0..4, "pub use"),
            (20..22, "async fn"),
            (30..40, ""),
            (0..0, "// header\n"),
            (5..30, ""),
        ],
    );
}
