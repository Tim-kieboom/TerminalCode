//! Dragging the edges of the explorer and of the terminal's bar to make them bigger or smaller:
//! the sizes hold for this run, they are not saved, and they give way to the layout file when
//! the file changes.

use crossterm::event::MouseButton::Left;
use crossterm::event::MouseEventKind::{Down, Drag, Up};
use ratatui::layout::Rect;

use super::*;
use crate::event::action::Action;
use crate::ui::layout::{LayoutTree, Size};

fn shown() -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig("sleep 30");
    rig.toggle();
    rig.wait_for("the shell", |rig| rig.app.state().terminal().is_running());
    rig.draw();
    (rig, dir)
}

impl Rig {
    fn area(&self, kind: ComponentKind) -> Rect {
        self.app
            .state()
            .placements(self.app.state().screen())
            .into_iter()
            .find(|placement| placement.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} is not on the screen"))
            .area
    }

    fn drag_to(&mut self, from: (u16, u16), to: (u16, u16)) {
        let none = KeyModifiers::NONE;
        self.app
            .handle_input(mouse(Down(Left), from.0, from.1, none));
        self.app.handle_input(mouse(Drag(Left), to.0, to.1, none));
        self.app.handle_input(mouse(Up(Left), to.0, to.1, none));
        self.draw();
    }
}

#[test]
fn dragging_the_right_edge_of_the_explorer_makes_it_wider_and_narrower() {
    let (mut rig, _dir) = shown();
    let before = rig.area(ComponentKind::Explorer);

    // Its last column is its border; taken at 5 cells in, put 10 cells further right.
    rig.drag_to((before.right() - 1, 5), (before.right() + 9, 5));
    let wider = rig.area(ComponentKind::Explorer);
    assert_eq!(wider.width, before.width + 10);

    rig.drag_to((wider.right() - 1, 5), (wider.right() - 21, 5));
    assert_eq!(rig.area(ComponentKind::Explorer).width, before.width - 10);
}

#[test]
fn dragging_the_top_edge_of_the_terminal_makes_it_taller_and_shorter() {
    let (mut rig, _dir) = shown();
    let before = rig.area(ComponentKind::Terminal);

    // The top row, away from the tab labels at its left.
    let column = before.right() - 3;
    rig.drag_to((column, before.y), (column, before.y - 6));
    let taller = rig.area(ComponentKind::Terminal);
    assert_eq!(taller.height, before.height + 6);
    assert_eq!(taller.bottom(), before.bottom(), "the bottom stays");

    rig.drag_to((column, taller.y), (column, taller.y + 9));
    assert_eq!(rig.area(ComponentKind::Terminal).height, before.height - 3);
}

#[test]
fn a_bar_cannot_be_dragged_to_nothing_or_over_the_rest() {
    let (mut rig, _dir) = shown();
    let explorer = rig.area(ComponentKind::Explorer);
    rig.drag_to((explorer.right() - 1, 5), (0, 5));
    assert_eq!(rig.area(ComponentKind::Explorer).width, 3);

    let wide = rig.area(ComponentKind::Explorer);
    rig.drag_to((wide.right() - 1, 5), (99, 5));
    let widest = rig.area(ComponentKind::Explorer).width;
    assert_eq!(widest, 100 - 10, "the editor keeps ten columns");
}

#[test]
fn a_press_on_a_tab_of_the_bar_is_a_tab_and_not_a_line() {
    let (mut rig, _dir) = shown();
    let terminal = rig.area(ComponentKind::Terminal);
    let (_, text_row) = rig.text_origin();
    // The text of the shell is below the line: a drag in it selects, as before.
    assert!(text_row > terminal.y);
    let before = rig.area(ComponentKind::Terminal).height;
    // A press on the first label, which the bar's title row has at its left.
    rig.drag_to(
        (terminal.x + 2, terminal.y),
        (terminal.x + 2, terminal.y - 5),
    );
    assert_eq!(rig.area(ComponentKind::Terminal).height, before);
}

#[test]
fn the_dragged_size_holds_for_the_run_but_not_after_the_layout_changes_or_a_reset() {
    let (mut rig, _dir) = shown();
    let original = rig.area(ComponentKind::Explorer).width;
    let edge = rig.area(ComponentKind::Explorer).right() - 1;
    rig.drag_to((edge, 5), (edge + 7, 5));
    assert_eq!(rig.area(ComponentKind::Explorer).width, original + 7);

    // Anything that builds the layout again (a plugin placed, a pane widened) keeps it.
    rig.app.state_mut().set_side_percent(50);
    rig.draw();
    assert_eq!(rig.area(ComponentKind::Explorer).width, original + 7);
    // The same layout read again does not undo it: saving another file reads them all.
    let same = rig.app.state().base_layout().clone();
    rig.app.state_mut().set_layout(same);
    rig.draw();
    assert_eq!(rig.area(ComponentKind::Explorer).width, original + 7);

    // The file changed: what it says wins again.
    let changed = LayoutTree::from_ron(
        "Col([Row([Fixed(20, SideBar(views: [Explorer])), Col([Pane(view: Editor), \
         Fixed(12, BottomBar(views: [Terminal]))])])])",
    )
    .unwrap();
    rig.app.state_mut().set_layout(changed);
    rig.draw();
    assert_eq!(rig.area(ComponentKind::Explorer).width, 20);

    // And a reset puts the file's sizes back.
    let edge = rig.area(ComponentKind::Explorer).right() - 1;
    rig.drag_to((edge, 5), (edge + 4, 5));
    assert_eq!(rig.area(ComponentKind::Explorer).width, 24);
    rig.app.run_action(Action::ResetPaneSizes);
    rig.draw();
    assert_eq!(rig.area(ComponentKind::Explorer).width, 20);
}

#[test]
fn a_size_in_percent_stays_a_percent() {
    let (mut rig, _dir) = shown();
    rig.app.state_mut().set_layout(
        LayoutTree::from_ron(
            "Col([Row([Percent(25, SideBar(views: [Explorer])), Col([Pane(view: Editor), \
             Fixed(12, BottomBar(views: [Terminal]))])])])",
        )
        .unwrap(),
    );
    rig.draw();
    let explorer = rig.area(ComponentKind::Explorer);
    assert_eq!(explorer.width, 25);

    rig.drag_to((explorer.right() - 1, 5), (49, 5));

    assert_eq!(rig.area(ComponentKind::Explorer).width, 50);
    assert_eq!(
        rig.app.state().layout().size_of(&ComponentKind::Explorer),
        Some(Size::Percent(50))
    );
}

#[test]
fn nothing_is_written_when_a_bar_is_dragged() {
    let dir = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let mut rig = rig_in(dir.path(), "sleep 30");
    rig.app = std::mem::take(&mut rig.app).with_config_dir(Some(config.path().to_path_buf()));
    rig.toggle();
    rig.draw();
    let state = config.path().join("state.json");
    let before = std::fs::read_to_string(&state).unwrap_or_default();
    let edge = rig.area(ComponentKind::Explorer).right() - 1;
    rig.drag_to((edge, 5), (edge + 7, 5));
    assert_eq!(rig.area(ComponentKind::Explorer).width, 37);
    // What is remembered is as it was, and the layout file was not made.
    assert_eq!(std::fs::read_to_string(&state).unwrap_or_default(), before);
    assert!(!config.path().join("layout.ron").exists());
}

#[test]
fn the_palette_has_a_command_to_reset_the_sizes() {
    let titles: Vec<_> = Action::palette_actions()
        .iter()
        .filter_map(|action| action.title())
        .collect();
    assert!(titles.contains(&"View: Reset Pane Sizes"));
}
