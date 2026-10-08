use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use super::explorer_tests::project;
use crate::components::ComponentKind;
use crate::components::explorer::{Explorer, ExplorerCommand};
use crate::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;

/// Draws the explorer alone into `width` x `height` cells and returns the screen.
fn draw(explorer: &mut Explorer, width: u16, height: u16) -> String {
    let state = AppState::default();
    let placement = Placement::new(ComponentKind::Explorer, Rect::new(0, 0, width, height));
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    explorer.prepare(&placement);
    terminal
        .draw(|frame| explorer.render(frame, &state, &placement))
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn rows_show_disclosure_markers_and_indent() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();
    explorer.apply(ExplorerCommand::Down).unwrap();
    explorer.apply(ExplorerCommand::Down).unwrap();
    explorer.apply(ExplorerCommand::Expand).unwrap();

    let screen = draw(&mut explorer, 30, 16);

    assert!(screen.contains("▾ src"), "{screen}");
    assert!(screen.contains("▸ docs"), "{screen}");
    assert!(
        screen.contains("│ │   main.rs"),
        "files sit below their directory: {screen}"
    );
}

#[test]
fn without_a_project_it_says_so() {
    let mut explorer = Explorer::default();

    let screen = draw(&mut explorer, 30, 6);

    assert!(screen.contains("no folder open"), "{screen}");
}

#[test]
fn the_selection_scrolls_into_view() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();
    explorer.apply(ExplorerCommand::Last).unwrap();

    let screen = draw(&mut explorer, 30, 4);

    assert!(screen.contains("zeta.txt"), "{screen}");
}

#[test]
fn wheel_scrolling_does_not_snap_back_to_the_selection() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();
    draw(&mut explorer, 30, 4);

    explorer.scroll_by(3);
    let screen = draw(&mut explorer, 30, 4);

    assert!(!screen.contains("▾"), "the root row scrolled out: {screen}");
}

#[test]
fn a_tiny_area_does_not_panic() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();

    for (width, height) in [(1, 1), (3, 2), (30, 1)] {
        draw(&mut explorer, width, height);
    }
}

#[test]
fn a_line_runs_down_the_children_of_each_directory() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();
    explorer.apply(ExplorerCommand::Down).unwrap();
    explorer.apply(ExplorerCommand::Down).unwrap();
    explorer.apply(ExplorerCommand::Expand).unwrap();

    let screen = draw(&mut explorer, 30, 16);

    let row = |name: &str| {
        screen
            .lines()
            .find(|line| line.contains(name))
            .unwrap_or_else(|| panic!("no row {name}:\n{screen}"))
            .trim_start_matches('"')
            .chars()
            .skip(1) // the pane border
            .collect::<String>()
    };
    // Children of the root have one line, files inside `src` have two, and
    // the line sits under the chevron of the directory it belongs to.
    assert!(row("README.md").starts_with("│   README.md"), "{screen}");
    assert!(row("main.rs").starts_with("│ │   main.rs"), "{screen}");
    assert!(row("▸ util").starts_with("│ │ ▸ util"), "{screen}");
    assert!(row("▾ src").starts_with("│ ▾ src"), "{screen}");
}

#[test]
fn the_root_row_has_no_line() {
    let dir = project(false);
    let mut explorer = Explorer::open(dir.path()).unwrap();

    let screen = draw(&mut explorer, 30, 8);

    let root = screen.lines().find(|line| line.contains('▾')).unwrap();
    let inside_border: String = root.trim_start_matches('"').chars().skip(1).collect();
    assert!(inside_border.starts_with('▾'), "{screen}");
}
