//! How long a frame of wrapped text takes. Ignored in the normal run (it is slow in a debug
//! build): `cargo test --release wrap_perf -- --ignored --nocapture` prints the times; the
//! limits are loose so that a slow machine does not fail them, and mean "not seconds".

use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::{Frame, Terminal};

use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::{Editor, Motion};
use crate::ui::ComponentKind;
use crate::ui::Render;
use crate::ui::layout::Placement;

fn frame(state: &mut AppState, width: u16, height: u16) -> Duration {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let start = Instant::now();
    terminal
        .draw(|frame: &mut Frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, state, &placement);
        })
        .unwrap();
    start.elapsed()
}

fn wrapped(text: &str) -> AppState {
    let mut state = AppState::new(Editor::new(Buffer::from_text(text)));
    state.workspace_mut().set_wrap(true);
    state
}

fn words(bytes: usize, filler: &str) -> String {
    let word = format!("lorem{filler} ipsum ");
    word.repeat(bytes / word.len() + 1)
}

#[test]
#[ignore = "timing: cargo test --release wrap_perf -- --ignored --nocapture"]
fn a_screen_of_lines_near_the_limit_draws_in_good_time() {
    // The worst case that wraps: every visible line is 99 KB.
    for (name, filler) in [("ascii", ""), ("non-ascii", "\u{e9}")] {
        let line = words(99_000, filler);
        let text = vec![line; 40].join("\n");
        let mut state = wrapped(&text);
        let first = frame(&mut state, 120, 50);
        let second = frame(&mut state, 120, 50);
        println!("{name}: first frame {first:?}, second {second:?}");
        assert!(first < Duration::from_secs(20), "{name} first {first:?}");
        assert!(second < Duration::from_secs(20), "{name} second {second:?}");
    }
}

#[test]
#[ignore = "timing: cargo test --release wrap_perf -- --ignored --nocapture"]
fn typing_in_a_line_near_the_limit_stays_usable() {
    for (name, filler) in [("ascii", ""), ("non-ascii", "\u{e9}")] {
        let mut state = wrapped(&words(99_000, filler));
        frame(&mut state, 120, 50);
        let mut worst = Duration::ZERO;
        for _ in 0..5 {
            state.edit(|editor| editor.type_text("x")).unwrap();
            worst = worst.max(frame(&mut state, 120, 50));
        }
        println!("{name}: typing in a 99 KB line, worst frame {worst:?}");
        assert!(worst < Duration::from_secs(5), "{name} {worst:?}");
    }
}

#[test]
#[ignore = "timing: cargo test --release wrap_perf -- --ignored --nocapture"]
fn a_big_file_with_ordinary_lines_is_as_fast_as_ever() {
    let line = words(120, "");
    let text = vec![line; 200_000].join("\n");
    let mut state = wrapped(&text);
    let first = frame(&mut state, 120, 50);
    state
        .edit(|editor| editor.move_cursor(Motion::DocumentEnd))
        .unwrap();
    let at_end = frame(&mut state, 120, 50);
    let mut worst = Duration::ZERO;
    for _ in 0..50 {
        state.edit(|editor| editor.move_cursor(Motion::Up)).unwrap();
        worst = worst.max(frame(&mut state, 120, 50));
    }
    println!("200k lines: first {first:?}, at the end {at_end:?}, worst Up {worst:?}");
    assert!(at_end < Duration::from_secs(2), "{at_end:?}");
    assert!(worst < Duration::from_secs(1), "{worst:?}");
}
