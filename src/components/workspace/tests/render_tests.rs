use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::workspace::{Workspace, render::*};
use crate::ui::ComponentKind;
use crate::ui::Render;
use crate::ui::layout::{Axis, Placement};

fn labels(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| format!(" {name} ")).collect()
}

const BAR: Rect = Rect::new(0, 0, 30, 1);

#[test]
fn tabs_are_placed_one_after_another() {
    let rects = tab_bar_layout(&labels(&["a", "bb"]), 0, BAR).rects;

    assert_eq!(rects, [Rect::new(0, 0, 3, 1), Rect::new(3, 0, 4, 1)]);
}

#[test]
fn tabs_that_do_not_fit_get_empty_rects() {
    let bar = tab_bar_layout(&labels(&["aaaa", "bbbb"]), 0, Rect::new(0, 0, 8, 1));

    // The second would be cut, so it is hidden; its marker takes the last cells.
    assert_eq!(bar.rects[0], Rect::new(0, 0, 6, 1));
    assert_eq!(bar.rects[1].width, 0);
    assert_eq!(bar.after, Rect::new(6, 0, 2, 1));
}

#[test]
fn the_active_tab_scrolls_into_view() {
    let bar = tab_bar_layout(&labels(&["aaaa", "bbbb", "cccc"]), 2, Rect::new(0, 0, 8, 1));

    assert_eq!(bar.rects[0].width, 0);
    assert_eq!(bar.rects[1].width, 0);
    assert_eq!(bar.rects[2], Rect::new(2, 0, 6, 1));
    assert_eq!(bar.before, Rect::new(0, 0, 2, 1));
}

fn screen(workspace: &mut Workspace, width: u16, height: u16) -> String {
    let state = AppState::default();
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
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

// ---- overflow markers

fn wide_labels(count: usize) -> Vec<String> {
    // " a1 " .. : four cells each while the numbers have one digit.
    (1..=count).map(|n| format!(" a{n} ")).collect()
}

fn visible(bar: &TabBar) -> Vec<usize> {
    bar.rects
        .iter()
        .enumerate()
        .filter(|(_, rect)| rect.width > 0)
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn nothing_hidden_means_no_markers() {
    let bar = tab_bar_layout(&wide_labels(3), 0, Rect::new(0, 0, 20, 1));

    assert_eq!((bar.hidden_before, bar.hidden_after), (0, 0));
    assert_eq!(bar.before.width + bar.after.width, 0);
    assert_eq!(
        bar.rects,
        [
            Rect::new(0, 0, 4, 1),
            Rect::new(4, 0, 4, 1),
            Rect::new(8, 0, 4, 1)
        ]
    );
}

#[test]
fn tabs_cut_off_on_the_right_are_counted_and_the_marker_takes_the_last_cells() {
    let bar = tab_bar_layout(&wide_labels(6), 0, Rect::new(0, 0, 20, 1));

    // The marker "2›" takes two cells, so four whole tabs fit; the fifth, which
    // would be cut, and the sixth are the two hidden.
    assert_eq!(visible(&bar), [0, 1, 2, 3]);
    assert_eq!((bar.hidden_before, bar.hidden_after), (0, 2));
    assert_eq!(bar.after, Rect::new(18, 0, 2, 1));
    assert_eq!(bar.before.width, 0);
}

#[test]
fn tabs_scrolled_off_on_the_left_are_counted_and_the_marker_takes_the_first_cells() {
    let bar = tab_bar_layout(&wide_labels(6), 5, Rect::new(0, 0, 20, 1));

    assert_eq!(visible(&bar), [2, 3, 4, 5]);
    assert_eq!((bar.hidden_before, bar.hidden_after), (2, 0));
    assert_eq!(bar.before, Rect::new(0, 0, 2, 1));
    assert_eq!(bar.rects[2], Rect::new(2, 0, 4, 1));
}

#[test]
fn both_sides_can_have_markers() {
    let bar = tab_bar_layout(&wide_labels(8), 4, Rect::new(0, 0, 20, 1));

    assert_eq!(visible(&bar), [1, 2, 3, 4]);
    assert_eq!((bar.hidden_before, bar.hidden_after), (1, 3));
    assert_eq!(bar.before, Rect::new(0, 0, 2, 1));
    assert_eq!(bar.after, Rect::new(18, 0, 2, 1));
    assert_eq!(bar.rects[1].x, 2);
}

#[test]
fn a_marker_grows_with_the_digits_of_its_count() {
    let labels: Vec<String> = (0..30).map(|_| " ab ".to_owned()).collect();

    let bar = tab_bar_layout(&labels, 0, Rect::new(0, 0, 20, 1));

    assert!(bar.hidden_after >= 10, "{}", bar.hidden_after);
    assert_eq!(bar.after.width, 3);
    assert_eq!(bar.after.right(), 20);
}

#[test]
fn a_bar_too_narrow_for_its_markers_shows_tabs_only() {
    let bar = tab_bar_layout(&wide_labels(6), 3, Rect::new(0, 0, 3, 1));

    assert_eq!((bar.hidden_before, bar.hidden_after), (0, 0));
    assert_eq!(bar.before.width + bar.after.width, 0);
}

#[test]
fn the_active_tab_is_always_in_view_and_nothing_overlaps_a_marker() {
    for count in 1..9 {
        for width in 6..45 {
            for active in 0..count {
                let labels = wide_labels(count);
                let bar_area = Rect::new(3, 0, width, 1);
                let bar = tab_bar_layout(&labels, active, bar_area);
                let context = format!("{count} tabs, width {width}, active {active}");

                assert!(bar.rects[active].width > 0, "{context}");
                let left_edge = bar.before.right().max(bar_area.x);
                let right_edge = if bar.after.width > 0 {
                    bar.after.x
                } else {
                    bar_area.right()
                };
                for rect in bar.rects.iter().filter(|rect| rect.width > 0) {
                    assert!(
                        rect.x >= left_edge && rect.right() <= right_edge,
                        "{context}"
                    );
                }
                // Without markers (a bar too narrow for them) the count is not kept.
                if bar.before.width + bar.after.width > 0 {
                    let hidden = bar.rects.iter().filter(|rect| rect.width == 0).count();
                    assert_eq!(hidden, bar.hidden_before + bar.hidden_after, "{context}");
                }
            }
        }
    }
}

fn crowded_workspace(count: usize) -> Workspace {
    let mut workspace = Workspace::new(Editor::new(Buffer::from_text("1")));
    for number in 2..=count {
        let mut buffer = Buffer::from_text("x");
        buffer.set_path(std::path::PathBuf::from(format!("file{number}.txt")));
        workspace.open_buffer(buffer);
    }
    workspace
}

#[test]
fn the_tab_bar_shows_how_many_tabs_are_out_of_view() {
    let mut workspace = crowded_workspace(12);
    workspace.activate_tab(6);

    let screen = screen(&mut workspace, 50, 6);
    let first_line = screen.lines().next().unwrap();

    assert!(first_line.contains('‹'), "{first_line}");
    assert!(first_line.contains('›'), "{first_line}");
}

#[test]
fn a_roomy_tab_bar_shows_no_markers() {
    let mut workspace = crowded_workspace(3);

    let screen = screen(&mut workspace, 80, 6);

    let first_line = screen.lines().next().unwrap();
    assert!(
        !first_line.contains('‹') && !first_line.contains('›'),
        "{first_line}"
    );
}
