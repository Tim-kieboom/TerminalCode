use ratatui::backend::TestBackend;
use ratatui::layout::Position as ScreenPosition;
use ratatui::{Frame, Terminal};

use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};
use crate::event::mouse::Clicks;
use crate::ui::ComponentKind;
use crate::ui::Render;
use crate::ui::layout::Placement;

/// Rows hold 19 cells at this size: the text area is 20 wide.
const WIDTH: u16 = 26;
const HEIGHT: u16 = 10;
/// The line `aaaa bbbb cccc dddd eeee ffff` is cut into `aaaa bbbb cccc ` and `dddd eeee ffff`.
const LINE: &str = "aaaa bbbb cccc dddd eeee ffff";

fn state(text: &str, wrap: bool) -> AppState {
    let mut state = AppState::new(Editor::new(Buffer::from_text(text)));
    state.workspace_mut().set_wrap(wrap);
    state
}

fn draw_at(state: &mut AppState, width: u16, height: u16) -> (Vec<String>, ScreenPosition) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame: &mut Frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, state, &placement);
        })
        .unwrap();
    let cursor = terminal.get_cursor_position().unwrap();
    let screen = terminal.backend().to_string();
    (screen.lines().map(str::to_owned).collect(), cursor)
}

fn draw(state: &mut AppState) -> (Vec<String>, ScreenPosition) {
    draw_at(state, WIDTH, HEIGHT)
}

fn row_with<'a>(screen: &'a [String], needle: &str) -> &'a str {
    screen
        .iter()
        .find(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle}:\n{}", screen.join("\n")))
}

fn head(state: &AppState) -> Position {
    state.editor().selections().primary().head()
}

fn motion(state: &mut AppState, motion: Motion) {
    state.edit(|editor| editor.move_cursor(motion)).unwrap();
}

#[test]
fn a_long_line_is_drawn_on_two_rows_with_its_number_on_the_first() {
    let mut state = state(LINE, true);
    let (screen, _) = draw(&mut state);
    assert!(row_with(&screen, "aaaa bbbb cccc").contains("  1 aaaa bbbb cccc"));
    let second = row_with(&screen, "dddd eeee ffff");
    assert!(!second.contains("  1 "), "{second}");
    assert!(!second.contains("  2 "), "{second}");
}

#[test]
fn without_wrap_the_line_stays_on_one_row() {
    let mut state = state(LINE, false);
    let (screen, _) = draw(&mut state);
    assert!(!screen.iter().any(|row| row.contains("dddd eeee ffff")));
}

#[test]
fn the_next_line_has_its_own_number_after_the_rows_of_the_first() {
    let mut state = state(&format!("{LINE}\nshort"), true);
    let (screen, _) = draw(&mut state);
    assert!(row_with(&screen, "short").contains("  2 short"));
}

#[test]
fn the_cursor_at_the_end_of_the_text_is_on_the_last_row() {
    let mut state = state(LINE, true);
    motion(&mut state, Motion::DocumentEnd);
    let (_, cursor) = draw(&mut state);
    // The text starts at column 5 and the first row below the tab bar is row 2.
    assert_eq!(cursor, ScreenPosition::new(5 + 14, 3));
}

#[test]
fn end_goes_to_the_end_of_the_row_and_the_second_press_to_the_end_of_the_line() {
    let mut state = state(LINE, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentStart);
    motion(&mut state, Motion::LineEnd);
    assert_eq!(head(&state), Position::new(0, 15));
    let (_, cursor) = draw(&mut state);
    // Drawn at the end of the first row, not at the start of the second.
    assert_eq!(cursor, ScreenPosition::new(5 + 15, 2));
    motion(&mut state, Motion::LineEnd);
    assert_eq!(head(&state), Position::new(0, 29));
}

#[test]
fn home_goes_to_the_start_of_the_row_and_the_second_press_to_the_start_of_the_line() {
    let mut state = state(LINE, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentEnd);
    motion(&mut state, Motion::LineStart);
    assert_eq!(head(&state), Position::new(0, 15));
    motion(&mut state, Motion::LineStart);
    assert_eq!(head(&state), Position::new(0, 0));
}

#[test]
fn any_other_movement_clears_the_end_of_row_cursor() {
    let mut state = state(LINE, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentStart);
    motion(&mut state, Motion::LineEnd);
    motion(&mut state, Motion::Left);
    motion(&mut state, Motion::Right);
    assert_eq!(head(&state), Position::new(0, 15));
    let (_, cursor) = draw(&mut state);
    // The same text position, now drawn at the start of the second row.
    assert_eq!(cursor, ScreenPosition::new(5, 3));
}

#[test]
fn down_and_up_move_by_row_and_keep_the_column() {
    let mut state = state(LINE, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentStart);
    motion(&mut state, Motion::Right);
    motion(&mut state, Motion::Right);
    motion(&mut state, Motion::Down);
    assert_eq!(head(&state), Position::new(0, 17));
    motion(&mut state, Motion::Up);
    assert_eq!(head(&state), Position::new(0, 2));
}

#[test]
fn down_from_the_last_row_goes_to_the_next_line() {
    let mut state = state(&format!("{LINE}\nnext line"), true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentStart);
    motion(&mut state, Motion::Down);
    motion(&mut state, Motion::Down);
    assert_eq!(head(&state).line, 1);
}

#[test]
fn down_onto_a_shorter_row_lands_at_its_end_drawn_there() {
    let mut state = state("aaaa bbbb cccc dddd\nx", true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentStart);
    motion(&mut state, Motion::LineEnd);
    motion(&mut state, Motion::LineEnd);
    motion(&mut state, Motion::Down);
    assert_eq!(head(&state), Position::new(1, 1));
}

#[test]
fn a_click_on_the_second_row_lands_in_the_text_of_that_row() {
    let mut state = state(LINE, true);
    draw(&mut state);
    state
        .edit(|editor| editor.mouse_press(5 + 2, 3, false, Clicks::Single))
        .unwrap();
    assert_eq!(head(&state), Position::new(0, 17));
}

#[test]
fn a_click_past_the_end_of_a_row_that_goes_on_draws_the_cursor_at_its_end() {
    let mut state = state(LINE, true);
    draw(&mut state);
    state
        .edit(|editor| editor.mouse_press(5 + 19, 2, false, Clicks::Single))
        .unwrap();
    assert_eq!(head(&state), Position::new(0, 15));
    let (_, cursor) = draw(&mut state);
    assert_eq!(cursor.y, 2);
}

#[test]
fn the_cursor_stays_on_screen_when_it_moves_far_down_a_wrapped_text() {
    let text = (0..30).map(|_| LINE).collect::<Vec<_>>().join("\n");
    let mut state = state(&text, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentEnd);
    let (screen, cursor) = draw(&mut state);
    assert!(cursor.y >= 2 && cursor.y < HEIGHT - 1, "cursor {cursor:?}");
    assert!(screen.iter().any(|row| row.contains("dddd eeee ffff")));
}

#[test]
fn page_down_and_up_move_by_rows_and_come_back() {
    let text = (0..30).map(|_| LINE).collect::<Vec<_>>().join("\n");
    let mut state = state(&text, true);
    draw(&mut state);
    motion(&mut state, Motion::PageDown);
    let down = head(&state);
    assert!(down.line > 0);
    motion(&mut state, Motion::PageUp);
    assert_eq!(head(&state).line, 0);
}

#[test]
fn the_wheel_scrolls_by_rows() {
    let text = (0..30).map(|_| LINE).collect::<Vec<_>>().join("\n");
    let mut state = state(&text, true);
    draw(&mut state);
    state.edit(|editor| editor.scroll_lines(1));
    let (screen, _) = draw(&mut state);
    // Three rows down is the second row of line 2: line 1 and the start of line 2 are gone.
    assert!(!screen.iter().any(|row| row.contains("  1 aaaa")));
    assert!(!screen.iter().any(|row| row.contains("  2 aaaa")));
    assert!(screen.iter().any(|row| row.contains("  3 aaaa")));
}

#[test]
fn a_line_over_100_kb_stays_on_one_row_and_scrolls_sideways() {
    let long = "x".repeat(150_000);
    let mut state = state(&long, true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentEnd);
    let (screen, cursor) = draw(&mut state);
    assert_eq!(cursor.y, 2);
    assert!(screen.iter().filter(|row| row.contains("  1 ")).count() == 1);
    assert!(state.editor().scroll().left > 0);
}

#[test]
fn a_short_line_beside_a_long_one_still_wraps_and_ignores_the_sideways_scroll() {
    let long = "x".repeat(150_000);
    let mut state = state(&format!("{long}\n{LINE}"), true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentEnd);
    let (screen, _) = draw(&mut state);
    assert!(row_with(&screen, "aaaa bbbb cccc").contains("aaaa bbbb cccc"));
    assert!(screen.iter().any(|row| row.contains("dddd eeee ffff")));
}

#[test]
fn a_resize_rewraps_and_keeps_the_cursor_in_view() {
    let mut state = state(LINE, true);
    draw_at(&mut state, 26, 10);
    motion(&mut state, Motion::DocumentEnd);
    let (_, narrow) = draw_at(&mut state, 16, 10);
    assert!(narrow.y >= 2);
    let (screen, wide) = draw_at(&mut state, 60, 10);
    assert!(screen.iter().any(|row| row.contains(LINE)));
    assert_eq!(wide.y, 2);
}

#[test]
fn a_selection_across_the_break_is_drawn_on_both_rows() {
    let mut state = state(LINE, true);
    draw(&mut state);
    state.edit(|editor| editor.select_range(Position::new(0, 10), Position::new(0, 20)));
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal
        .draw(|frame: &mut Frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, &state, &placement);
        })
        .unwrap();
    let cells = terminal.backend().buffer().clone();
    let background = |column: u16, row: u16| cells[(column, row)].bg;
    // Columns 10 to 14 of the first row and 0 to 4 of the second are selected.
    assert_eq!(background(5 + 10, 2), background(5, 3));
    assert_ne!(background(5 + 10, 2), background(5, 2));
    assert_ne!(background(5, 3), background(5 + 8, 3));
}

#[test]
fn typing_in_a_wrapped_line_keeps_the_cursor_visible_and_rewraps() {
    let mut state = state("aaaa bbbb cccc", true);
    draw(&mut state);
    motion(&mut state, Motion::DocumentEnd);
    state
        .edit(|editor| editor.type_text(" dddd eeee ffff"))
        .unwrap();
    let (screen, cursor) = draw(&mut state);
    assert!(screen.iter().any(|row| row.contains("dddd")));
    assert!(cursor.y >= 2);
}
