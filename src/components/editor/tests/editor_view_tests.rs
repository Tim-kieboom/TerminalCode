use ratatui::backend::TestBackend;
use ratatui::layout::Position as ScreenPosition;
use ratatui::{Frame, Terminal};

use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::ComponentKind;
use crate::components::editor::{Editor, Motion};
use crate::ui::Render;
use crate::ui::layout::Placement;

fn state_with(text: &str) -> AppState {
    AppState::new(Editor::new(Buffer::from_text(text)))
}

fn to_placement(frame: &Frame) -> Placement {
    Placement::new(ComponentKind::Editor, frame.area())
}

/// Draws the editor into a terminal of `width` x `height` and returns the
/// screen text and the cursor position.
fn draw(state: &mut AppState, width: u16, height: u16) -> (String, ScreenPosition) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let placement = to_placement(frame);
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, state, &placement);
        })
        .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    (terminal.backend().to_string(), cursor)
}

#[test]
fn draws_line_numbers_and_text() {
    let mut state = state_with("hello\nworld");

    let (screen, _) = draw(&mut state, 30, 6);

    assert!(screen.contains("  1 hello"), "screen:\n{screen}");
    assert!(screen.contains("  2 world"), "screen:\n{screen}");
}

#[test]
fn title_shows_the_name_and_modified_marker() {
    let mut state = state_with("x");
    state.edit(|e| e.insert_text("y")).unwrap();

    let (screen, _) = draw(&mut state, 30, 4);

    assert!(screen.contains("[no name] [+]"), "screen:\n{screen}");
}

#[test]
fn cursor_sits_after_the_gutter_inside_the_border() {
    let mut state = state_with("hello");

    let (_, cursor) = draw(&mut state, 30, 6);

    // 1 border + 4 gutter columns; tab bar row + 1 border row.
    assert_eq!(cursor, ScreenPosition::new(5, 2));
}

#[test]
fn cursor_follows_typing_and_wide_characters() {
    let mut state = state_with("");
    state.edit(|e| e.insert_text("\u{3053}a")).unwrap();

    let (_, cursor) = draw(&mut state, 30, 6);

    // The wide character takes two cells, `a` one.
    assert_eq!(cursor, ScreenPosition::new(5 + 3, 2));
}

#[test]
fn tabs_are_expanded_when_drawn() {
    let mut state = state_with("\tx");

    let (screen, cursor) = draw(&mut state, 30, 4);

    assert!(screen.contains("  1     x"), "screen:\n{screen}");
    assert_eq!(cursor.x, 5);
}

#[test]
fn scrolls_down_to_keep_the_cursor_visible() {
    let text = (1..=50)
        .map(|n| format!("line{n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = state_with(&text);
    state.edit(|e| e.move_cursor(Motion::DocumentEnd)).unwrap();

    let (screen, cursor) = draw(&mut state, 30, 8);

    assert!(screen.contains("line50"), "screen:\n{screen}");
    assert!(!screen.contains("line1 "), "screen:\n{screen}");
    assert_eq!(cursor.y, 6);
}

#[test]
fn scrolls_right_for_long_lines() {
    let mut state = state_with(&"x".repeat(100));
    state.edit(|e| e.move_cursor(Motion::LineEnd)).unwrap();

    let (_, cursor) = draw(&mut state, 30, 4);

    // Text area is 30 - 2 borders - 4 gutter = 24 wide; cursor is on the last cell.
    assert_eq!(cursor.x, 5 + 24 - 1);
}

#[test]
fn drawing_into_a_tiny_area_does_not_panic() {
    let mut state = state_with("hello");

    draw(&mut state, 3, 2);
    draw(&mut state, 1, 1);
}

#[test]
fn render_alone_never_scrolls() {
    let text = (1..=50)
        .map(|n| format!("line{n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = state_with(&text);
    let mut terminal = Terminal::new(TestBackend::new(30, 8)).unwrap();
    terminal
        .draw(|frame| {
            let placement = to_placement(frame);
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, &state, &placement);
        })
        .unwrap();
    state.edit(|e| e.move_cursor(Motion::DocumentEnd)).unwrap();

    // Drawing again without a prepare step must not scroll to the new cursor.
    terminal
        .draw(|frame| {
            let placement = to_placement(frame);
            state.workspace().render(frame, &state, &placement);
        })
        .unwrap();

    assert_eq!(state.editor().scroll().top, 0);
}

#[test]
fn full_layout_render_keeps_the_cursor_in_view() {
    let text = (1..=50)
        .map(|n| format!("line{n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = state_with(&text);
    state.edit(|e| e.move_cursor(Motion::DocumentEnd)).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();

    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
        .unwrap();

    let screen = terminal.backend().to_string();
    assert!(screen.contains("line50"), "screen:\n{screen}");
    assert!(state.editor().scroll().top > 0);
}
