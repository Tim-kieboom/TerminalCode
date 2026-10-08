use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::*;
use crate::buffer::Buffer;
use crate::components::ComponentKind;
use crate::components::editor::Editor;
use crate::ui::layout::Axis;

fn labels(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| format!(" {name} ")).collect()
}

const BAR: Rect = Rect::new(0, 0, 30, 1);

#[test]
fn tabs_are_placed_one_after_another() {
    let rects = tab_rects(&labels(&["a", "bb"]), 0, BAR);

    assert_eq!(rects, [Rect::new(0, 0, 3, 1), Rect::new(3, 0, 4, 1)]);
}

#[test]
fn tabs_that_do_not_fit_get_empty_rects() {
    let rects = tab_rects(&labels(&["aaaa", "bbbb"]), 0, Rect::new(0, 0, 8, 1));

    assert_eq!(rects[0], Rect::new(0, 0, 6, 1));
    assert_eq!(rects[1], Rect::new(6, 0, 2, 1));
    let rects = tab_rects(&labels(&["aaaa", "bbbb", "cccc"]), 0, Rect::new(0, 0, 8, 1));
    assert_eq!(rects[2].width, 0);
}

#[test]
fn the_active_tab_scrolls_into_view() {
    let rects = tab_rects(&labels(&["aaaa", "bbbb", "cccc"]), 2, Rect::new(0, 0, 8, 1));

    assert_eq!(rects[0].width, 0);
    assert_eq!(rects[1].width, 0);
    assert_eq!(rects[2], Rect::new(0, 0, 6, 1));
}

#[test]
fn truncate_cuts_by_display_width() {
    assert_eq!(truncate("abcdef", 3), "abc");
    assert_eq!(truncate("\u{3053}\u{3053}", 3), "\u{3053}");
    assert_eq!(truncate("ab", 10), "ab");
    assert_eq!(truncate("ab", 0), "");
}

fn screen(workspace: &mut Workspace, width: u16, height: u16) -> String {
    let state = AppState::default();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let placement = Placement {
                kind: ComponentKind::Editor,
                area: frame.area(),
            };
            workspace.prepare(&placement);
            workspace.render(frame, &state, &placement);
        })
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn every_tab_is_named_in_the_tab_bar() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("one")));
    workspace.open_buffer(Buffer::from_text("two"));

    let screen = screen(&mut workspace, 60, 10);

    let first_row = screen.lines().next().unwrap();
    assert_eq!(first_row.matches("[no name]").count(), 2, "{first_row}");
}

#[test]
fn split_panes_show_the_same_text_side_by_side() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("shared text")));
    workspace.split(Axis::Horizontal);

    let screen = screen(&mut workspace, 80, 10);

    let row = screen.lines().nth(2).unwrap();
    assert_eq!(row.matches("shared text").count(), 2, "{row}");
}

#[test]
fn a_tiny_area_does_not_panic() {
    let mut workspace = Workspace::new(Editor::default());
    workspace.split(Axis::Vertical);
    workspace.split(Axis::Horizontal);

    for (width, height) in [(1, 1), (2, 2), (5, 3), (3, 1)] {
        screen(&mut workspace, width, height);
    }
}

#[test]
fn an_editor_area_without_tabs_shows_a_hint() {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("gone")));
    workspace.close_tab();

    let screen = screen(&mut workspace, 60, 10);

    assert!(screen.contains("No open files"), "{screen}");
    assert!(!screen.contains("gone"), "{screen}");
}

#[test]
fn an_empty_editor_area_survives_tiny_sizes() {
    let mut workspace = Workspace::new(Editor::default());
    workspace.close_tab();

    for (width, height) in [(1, 1), (2, 2), (5, 3)] {
        screen(&mut workspace, width, height);
    }
}
