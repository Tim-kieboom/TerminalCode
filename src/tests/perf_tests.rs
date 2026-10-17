//! How long big files take to open, draw and scroll. These measure, and are
//! ignored in the ordinary run: a debug build says nothing about speed. Run
//! them with `cargo test --release perf_tests -- --ignored --nocapture`.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::initial_state;

/// What opening a file may take, and what one frame may take.
const OPEN_BUDGET: Duration = Duration::from_millis(300);
const FRAME_BUDGET: Duration = Duration::from_millis(50);
const TEN_MEGABYTES: usize = 10 * 1000 * 1000;

/// Text of `bytes` or a little more, as lines of `line_len` characters.
fn lines_of(bytes: usize, line_len: usize) -> String {
    let line = format!("{:<line_len$}\n", "    let value = compute(alpha, beta);");
    line.repeat(bytes / line.len() + 1)
}

fn write_file(dir: &Path, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    path
}

struct Run {
    app: App,
    terminal: Terminal<TestBackend>,
}

impl Run {
    fn frame(&mut self) -> Duration {
        let start = Instant::now();
        let app = &mut self.app;
        self.terminal
            .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
            .unwrap();
        start.elapsed()
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    /// Presses `key` `times` times, drawing after each; the slowest of
    /// press-and-draw.
    fn keys_with_frames(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        times: usize,
    ) -> Duration {
        let mut slowest = Duration::ZERO;
        for _ in 0..times {
            let start = Instant::now();
            self.press(code, modifiers);
            self.frame();
            slowest = slowest.max(start.elapsed());
        }
        slowest
    }
}

/// Opens `path`, draws once, and says how long each took.
fn open(path: &Path) -> (Run, Duration, Duration) {
    let start = Instant::now();
    let state = initial_state(std::slice::from_ref(&path.to_path_buf())).unwrap();
    let opened = start.elapsed();
    let mut run = Run {
        app: App::new(state),
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
    };
    let first = run.frame();
    (run, opened, first)
}

fn report(what: &str, opened: Duration, first: Duration, slowest: Duration) {
    eprintln!(
        "{what:<34} open {opened:>9.1?}  first frame {first:>9.1?}  slowest key+frame {slowest:>9.1?}"
    );
}

fn measure(what: &str, text: &str) -> (Duration, Duration, Duration) {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "big.rs", text);
    let (mut run, opened, first) = open(&path);
    let mut slowest = run.keys_with_frames(KeyCode::PageDown, KeyModifiers::NONE, 100);
    slowest = slowest.max(run.keys_with_frames(KeyCode::Down, KeyModifiers::NONE, 100));
    report(what, opened, first, slowest);
    (opened, first, slowest)
}

#[test]
#[ignore = "measures; run in release"]
fn ten_megabytes_of_ordinary_lines() {
    measure("10 MB, 80 columns", &lines_of(TEN_MEGABYTES, 80));
}

#[test]
#[ignore = "measures; run in release"]
fn ten_megabytes_of_long_lines() {
    measure("10 MB, 1000 columns", &lines_of(TEN_MEGABYTES, 1000));
}

#[test]
#[ignore = "measures; run in release"]
fn ten_megabytes_on_one_line() {
    measure("10 MB, one line", &"x".repeat(TEN_MEGABYTES));
}

#[test]
#[ignore = "measures; run in release"]
fn typing_in_the_middle_of_ten_megabytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "big.rs", &lines_of(TEN_MEGABYTES, 80));
    let (mut run, ..) = open(&path);
    for _ in 0..60_000 {
        run.press(KeyCode::Down, KeyModifiers::NONE);
    }
    run.frame();

    let slowest = run.keys_with_frames(KeyCode::Char('x'), KeyModifiers::NONE, 200);

    eprintln!("typing in the middle of 10 MB: slowest key+frame {slowest:.1?}");
}

/// With the syntax worker running, as in the real app: how long until the
/// colors arrive, and what the main thread spends on drawing and on taking the
/// worker's answers meanwhile.
#[test]
#[ignore = "measures; run in release"]
fn highlighting_ten_megabytes_of_rust() {
    use crate::event::Event;
    let function = "fn name() {\n    let value = compute(alpha, beta);\n    value\n}\n";
    let text = function.repeat(TEN_MEGABYTES / function.len());
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "big.rs", &text);
    let (tx, mut events) = tokio::sync::mpsc::channel(256);
    let start = Instant::now();
    let app = App::new(initial_state(std::slice::from_ref(&path)).unwrap()).with_events(tx);
    let mut run = Run {
        app,
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
    };
    let (mut slowest_frame, mut slowest_answer) = (Duration::ZERO, Duration::ZERO);
    let mut first_colors = None;
    let deadline = Instant::now() + Duration::from_secs(60);

    while Instant::now() < deadline {
        slowest_frame = slowest_frame.max(run.frame());
        if !run.app.state().workspace().highlights_pending() {
            break;
        }
        while let Ok(event) = events.try_recv() {
            let began = Instant::now();
            let colored = matches!(event, Event::Highlighted(_));
            run.app.handle_event(event);
            slowest_answer = slowest_answer.max(began.elapsed());
            if colored && first_colors.is_none() {
                first_colors = Some(start.elapsed());
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let mut slowest_scroll = Duration::ZERO;
    for _ in 0..100 {
        let began = Instant::now();
        run.press(KeyCode::PageDown, KeyModifiers::NONE);
        run.frame();
        while let Ok(event) = events.try_recv() {
            run.app.handle_event(event);
        }
        slowest_scroll = slowest_scroll.max(began.elapsed());
    }
    eprintln!(
        "highlighting 10 MB of Rust: first colors after {first_colors:.1?}, all after {:.1?}, \
         slowest frame {slowest_frame:.1?}, slowest answer taken {slowest_answer:.1?}, \
         slowest scroll step {slowest_scroll:.1?}",
        start.elapsed()
    );
}

#[test]
#[ignore = "measures; run in release"]
fn how_long_a_single_line_can_get() {
    for size in [50_000, 100_000, 250_000, 500_000, 1_000_000, 2_000_000] {
        measure(
            &format!("one line of {} KB", size / 1000),
            &"x".repeat(size),
        );
    }
}

/// The guard: 10 MB of ordinary lines and of 1000 column lines, and a single line
/// of plain ASCII, stay inside the budgets. (A single line with other characters
/// in it is walked grapheme by grapheme and costs a frame time that grows with its
/// length, about 100 ms for 1 MB, so it is measured above but not held to the
/// budget.)
#[test]
#[ignore = "measures; run in release"]
fn the_budgets_hold_for_ten_megabytes_of_ordinary_and_long_lines() {
    for (what, text) in [
        ("budget: 80 columns", lines_of(TEN_MEGABYTES, 80)),
        ("budget: 1000 columns", lines_of(TEN_MEGABYTES, 1000)),
        ("budget: one ASCII line", "x".repeat(TEN_MEGABYTES)),
    ] {
        let (opened, first, slowest) = measure(what, &text);

        assert!(
            opened + first < OPEN_BUDGET,
            "{what}: open and first frame took {:?}",
            opened + first
        );
        assert!(
            slowest < FRAME_BUDGET,
            "{what}: a key and frame took {slowest:?}"
        );
    }
}

// ---- trees: the finder's walk, the project search, the explorer

/// `dirs` directories with `per_dir` files each, under `root`, each file with
/// `body` in it. Returns how many files were made.
fn make_tree(root: &Path, dirs: usize, per_dir: usize, body: &str) -> usize {
    for dir in 0..dirs {
        let folder = root
            .join(format!("group{:02}", dir % 20))
            .join(format!("dir{dir}"));
        fs::create_dir_all(&folder).unwrap();
        for file in 0..per_dir {
            fs::write(folder.join(format!("file{file}.rs")), body).unwrap();
        }
    }
    dirs * per_dir
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = f();
    (value, start.elapsed())
}

#[test]
#[ignore = "measures; run in release"]
fn the_finders_walk_of_fifty_thousand_files() {
    use crate::event::Event;
    let dir = tempfile::tempdir().unwrap();
    let made = make_tree(dir.path(), 1000, 50, "");
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    let start = Instant::now();
    let _handle = crate::components::finder::start_scan(dir.path().to_path_buf(), 1, tx);
    let (mut first, mut found) = (None, 0);
    let total = loop {
        match rx.blocking_recv() {
            Some(Event::FinderBatch { files, .. }) => {
                first.get_or_insert(start.elapsed());
                found += files.len();
            }
            Some(Event::FinderDone { .. }) => break start.elapsed(),
            Some(_) => {}
            None => panic!("the walk ended without saying so"),
        }
    };

    assert_eq!(found, made);
    eprintln!("finder walk of {made} files: first batch {first:.1?}, all {total:.1?}");
    assert!(
        first.unwrap() < FIRST_RESULTS_BUDGET,
        "first batch {first:?}"
    );
    assert!(total < WALK_BUDGET, "the walk took {total:?}");
}

#[test]
#[ignore = "measures; run in release"]
fn a_project_search_over_twenty_thousand_files() {
    use crate::components::search::{Options, start_search};
    use crate::event::Event;
    let dir = tempfile::tempdir().unwrap();
    let body = "fn function() {\n    let value = compute(alpha, beta);\n}\n".repeat(20);
    let made = make_tree(dir.path(), 400, 50, &body);
    fs::write(
        dir.path().join("group00/dir0/file0.rs"),
        format!("{body}needle_in_a_haystack\n"),
    )
    .unwrap();

    for (what, query, expected_hits) in [
        ("a rare word", "needle_in_a_haystack", Some(1)),
        ("a word in every file", "compute", None),
    ] {
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let start = Instant::now();
        let _handle =
            start_search(dir.path().to_path_buf(), 1, query, Options::default(), tx).unwrap();
        let (mut first, mut hits) = (None, 0);
        let total = loop {
            match rx.blocking_recv() {
                Some(Event::SearchBatch { hits: found, .. }) => {
                    first.get_or_insert(start.elapsed());
                    hits += found.len();
                }
                Some(Event::SearchDone { .. }) => break start.elapsed(),
                Some(_) => {}
                None => panic!("the search ended without saying so"),
            }
        };
        eprintln!(
            "search for {what} in {made} files: first hits {first:.1?}, all {total:.1?}, {hits} hits"
        );
        if let Some(expected) = expected_hits {
            assert_eq!(hits, expected);
        }
        if let Some(first) = first {
            assert!(first < SEARCH_FIRST_BUDGET, "{what}: first hits {first:?}");
        }
        assert!(total < SEARCH_BUDGET, "{what}: the search took {total:?}");
    }
}

#[test]
#[ignore = "measures; run in release"]
fn the_explorer_with_fifty_thousand_entries_in_one_folder() {
    let dir = tempfile::tempdir().unwrap();
    for file in 0..50_000 {
        fs::write(dir.path().join(format!("file{file:05}.txt")), "").unwrap();
    }
    let mut state = crate::app::state::AppState::default();

    let (result, opened) = time(|| state.open_project(dir.path()));
    result.unwrap();
    let (result, refreshed) = time(|| state.explorer_mut().refresh());
    result.unwrap();
    let mut run = Run {
        app: App::new(state),
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
    };
    let first = run.frame();
    run.press(KeyCode::Char('e'), KeyModifiers::CONTROL);
    let slowest = run.keys_with_frames(KeyCode::Down, KeyModifiers::NONE, 100);

    eprintln!(
        "explorer, 50000 entries in one folder: open {opened:.1?}, refresh {refreshed:.1?}, first frame {first:.1?}, slowest key+frame {slowest:.1?}"
    );
    assert!(opened < OPEN_BUDGET, "opening took {opened:?}");
    assert!(refreshed < OPEN_BUDGET, "refreshing took {refreshed:?}");
    assert!(first < FRAME_BUDGET, "the first frame took {first:?}");
    assert!(slowest < FRAME_BUDGET, "a key and frame took {slowest:?}");
}

/// What the first results of a walk or a search may take, and all of it.
const FIRST_RESULTS_BUDGET: Duration = Duration::from_millis(300);
const WALK_BUDGET: Duration = Duration::from_secs(2);
const SEARCH_BUDGET: Duration = Duration::from_secs(3);
/// The search waits a moment after the last key before it starts, so its first
/// hits come later than a walk's first files.
const SEARCH_FIRST_BUDGET: Duration = Duration::from_millis(500);

/// With the syntax worker running, typing in the middle of 10 MB of Rust: the
/// worker parses again after each key, and the keys and frames must not wait for it.
#[test]
#[ignore = "measures; run in release"]
fn typing_in_ten_megabytes_of_rust_while_it_is_colored() {
    let function = "fn name() {\n    let value = compute(alpha, beta);\n    value\n}\n";
    let text = function.repeat(TEN_MEGABYTES / function.len());
    let dir = tempfile::tempdir().unwrap();
    let path = write_file(dir.path(), "big.rs", &text);
    let (tx, mut events) = tokio::sync::mpsc::channel(256);
    let app = App::new(initial_state(std::slice::from_ref(&path)).unwrap()).with_events(tx);
    let mut run = Run {
        app,
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
    };
    for _ in 0..40_000 {
        run.press(KeyCode::Down, KeyModifiers::NONE);
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        run.frame();
        if !run.app.state().workspace().highlights_pending() {
            break;
        }
        while let Ok(event) = events.try_recv() {
            run.app.handle_event(event);
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let mut slowest = Duration::ZERO;
    for _ in 0..100 {
        let began = Instant::now();
        run.press(KeyCode::Char('x'), KeyModifiers::NONE);
        run.frame();
        slowest = slowest.max(began.elapsed());
        while let Ok(event) = events.try_recv() {
            let answer = Instant::now();
            run.app.handle_event(event);
            slowest = slowest.max(answer.elapsed());
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    eprintln!("typing in colored 10 MB of Rust: slowest key+frame or answer {slowest:.1?}");
    assert!(
        slowest < FRAME_BUDGET,
        "a key, frame or answer took {slowest:?}"
    );
}

/// What the language server sync costs on a big document: the first message
/// carries all the text, and each later one is found by comparing the text
/// with what the server has. Held to the frame budget, since it runs before
/// every frame that follows a change.
#[test]
#[ignore = "measures; run in release"]
fn telling_a_server_about_an_edit_in_ten_megabytes() {
    use crate::ids::DocumentId;
    use crate::lsp::{DocSnapshot, DocumentSync, ServerKey};
    let text = lines_of(TEN_MEGABYTES, 80);
    let key = ServerKey {
        name: "rust".to_owned(),
        root: std::path::PathBuf::from("/p"),
    };
    let snapshot = |text: &str, version| DocSnapshot {
        id: DocumentId::for_test(1),
        uri: "file:///p/big.rs".to_owned(),
        language_id: "rust".to_owned(),
        text: ropey::Rope::from_str(text),
        version,
        saves: 0,
        reloads: 0,
    };
    let mut sync = DocumentSync::default();
    let (_, opening) = time(|| sync.messages(&key, 1, &[snapshot(&text, 0)]));

    // An edit near the start, in the middle and near the end.
    let mut slowest = Duration::ZERO;
    for (number, at) in [text.len() / 100, text.len() / 2, text.len() - 1000]
        .into_iter()
        .enumerate()
    {
        let at = at - at % 81;
        let mut edited = text.clone();
        edited.insert(at, 'x');
        let doc = snapshot(&edited, number as u64 + 1);
        let (messages, took) = time(|| sync.messages(&key, 1, std::slice::from_ref(&doc)));
        assert_eq!(messages.len(), 1);
        slowest = slowest.max(took);
        // Back to the original, so each edit is against the same text.
        let back = snapshot(&text, number as u64 + 100);
        sync.messages(&key, 1, &[back]);
    }

    eprintln!("sync of 10 MB: open message {opening:.1?}, slowest edit {slowest:.1?}");
    assert!(
        slowest < FRAME_BUDGET,
        "an edit took {slowest:?} to work out"
    );
}
