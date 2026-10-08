//! Each component honors the frame the layout gives it. Layouts are written
//! in the real RON format, so this also covers the file syntax.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::state::AppState;
use crate::ui::layout::LayoutTree;

/// Draws a layout stacking the explorer (6 rows), the editor (the rest) and
/// the status bar (3 rows), each with the given RON frame, e.g. `()` for all
/// defaults or `(border: Rounded)`.
fn screen_with(explorer: &str, editor: &str, status: &str) -> Vec<String> {
    let source = format!(
        "Split(direction: Vertical, children: [
            (size: Fixed(6), node: Framed(component: Explorer, frame: {explorer})),
            (size: Fill, node: Framed(component: Editor, frame: {editor})),
            (size: Fixed(3), node: Framed(component: StatusBar, frame: {status})),
        ])"
    );
    let mut state = AppState::default();
    state.set_layout(LayoutTree::from_ron(&source).unwrap());

    let mut terminal = Terminal::new(TestBackend::new(50, 20)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
        .unwrap();
    terminal
        .backend()
        .to_string()
        .lines()
        .map(|line| line.trim_matches('"').to_owned())
        .collect()
}

fn defaults() -> &'static str {
    "()"
}

fn first_char(row: &str) -> char {
    row.chars().next().unwrap()
}

#[test]
fn the_explorer_shows_its_name_by_default() {
    let rows = screen_with(defaults(), defaults(), defaults());

    assert!(rows[0].starts_with("┌Explorer"), "{rows:#?}");
}

#[test]
fn the_explorer_title_can_be_hidden_or_replaced() {
    let hidden = screen_with("(title: Hidden)", defaults(), defaults());
    assert!(
        hidden.iter().all(|row| !row.contains("Explorer")),
        "{hidden:#?}"
    );
    assert!(hidden[0].starts_with("┌──"), "{hidden:#?}");

    let renamed = screen_with("(title: Text(\"Files\"))", defaults(), defaults());
    assert!(renamed[0].starts_with("┌Files"), "{renamed:#?}");
}

#[test]
fn the_explorer_title_can_be_centered_or_right_aligned() {
    let right = screen_with("(title_align: Right)", defaults(), defaults());
    assert!(right[0].ends_with("Explorer┐"), "{right:#?}");

    let center = screen_with("(title_align: Center)", defaults(), defaults());
    let left_dashes = center[0].chars().skip(1).take_while(|c| *c == '─').count();
    assert!(left_dashes > 10, "{center:#?}");
}

#[test]
fn borders_can_be_rounded_double_thick_or_gone() {
    let corner = |frame| first_char(&screen_with(frame, defaults(), defaults())[0]);

    assert_eq!(corner("(border: Plain)"), '┌');
    assert_eq!(corner("(border: Rounded)"), '╭');
    assert_eq!(corner("(border: Double)"), '╔');
    assert_eq!(corner("(border: Thick)"), '┏');
    assert_ne!(corner("(border: Off)"), '┌');
}

#[test]
fn a_borderless_explorer_has_no_border_lines() {
    let rows = screen_with("(border: Off, title: Hidden)", defaults(), defaults());

    // The explorer is the top 6 rows; with no project it only says so.
    for row in &rows[..6] {
        let border = row.chars().any(|c| "┌┐└┘─│".contains(c));
        assert!(!border, "{rows:#?}");
    }
}

#[test]
fn a_title_without_a_border_is_still_drawn_on_its_own_row() {
    let rows = screen_with("(border: Off)", defaults(), defaults());

    assert!(rows[0].contains("Explorer"), "{rows:#?}");
    assert!(!rows[0].contains('┌'), "{rows:#?}");
}

#[test]
fn the_editor_pane_can_be_borderless() {
    let rows = screen_with(defaults(), "(border: Off)", defaults());

    // Row 6 is the editor's tab bar, row 7 the first line of text.
    assert!(rows[7].starts_with("  1 "), "{rows:#?}");
    assert!(!rows[7].contains('│'), "{rows:#?}");
}

#[test]
fn the_editor_can_show_its_file_name_as_a_border_title() {
    let without = screen_with(defaults(), defaults(), defaults());
    let with = screen_with(defaults(), "(title: Name)", defaults());

    let count = |rows: &[String]| rows.join("\n").matches("[no name]").count();
    // The tab bar and the status bar already name the file; the title adds it
    // once more, on the border.
    assert_eq!(count(&with), count(&without) + 1, "{with:#?}");
    assert!(with[7].contains("[no name]"), "{with:#?}");
}

#[test]
fn the_editor_can_have_a_custom_title_and_a_rounded_border() {
    let rows = screen_with(
        defaults(),
        "(border: Rounded, title: Text(\"Code\"), title_align: Center)",
        defaults(),
    );

    let joined = rows.join("\n");
    assert!(joined.contains("Code"), "{rows:#?}");
    assert!(joined.contains('╭'), "{rows:#?}");
}

#[test]
fn the_status_bar_is_bare_by_default() {
    let rows = screen_with(defaults(), defaults(), defaults());

    assert!(rows[17].contains("Ln 1, Col 1"), "{rows:#?}");
    assert!(!rows[17].contains('┌'), "{rows:#?}");
}

#[test]
fn the_status_bar_can_be_boxed_with_a_title() {
    let rows = screen_with(
        defaults(),
        defaults(),
        "(border: Rounded, title: Text(\"Status\"))",
    );

    assert!(rows[17].starts_with("╭Status"), "{rows:#?}");
    assert!(rows[18].contains("Ln 1, Col 1"), "{rows:#?}");
}

#[test]
fn frames_do_not_leak_between_components() {
    let rows = screen_with("(border: Double)", "(border: Thick)", "(border: Rounded)");

    assert_eq!(first_char(&rows[0]), '╔');
    assert_eq!(first_char(&rows[6]), ' ', "tab bar row: {rows:#?}");
    assert_eq!(first_char(&rows[7]), '┏', "{rows:#?}");
    assert_eq!(first_char(&rows[17]), '╭');
}

#[test]
fn a_tiny_terminal_with_every_kind_of_frame_does_not_panic() {
    for explorer in ["()", "(border: Off, title: Hidden)", "(border: Double)"] {
        let source = format!(
            "Split(direction: Vertical, children: [
                (size: Fixed(6), node: Framed(component: Explorer, frame: {explorer})),
                (size: Fill, node: Framed(component: Editor, frame: (border: Rounded, title: Name))),
                (size: Fixed(3), node: Framed(component: StatusBar, frame: (border: Thick))),
            ])"
        );
        let mut state = AppState::default();
        state.set_layout(LayoutTree::from_ron(&source).unwrap());

        for (width, height) in [(1, 1), (3, 2), (6, 4), (20, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
                .unwrap();
        }
    }
}

// ---- background painting

use ratatui::style::Color;

use crate::ui::theme::{Rgb, Theme};

/// Draws the default layout with a theme and returns every cell's background.
fn backgrounds(theme: &str, terminal: Option<Rgb>) -> Vec<Color> {
    let mut state = AppState::default();
    state.set_theme(Theme::from_toml(theme).unwrap());
    state.learn_terminal_background(|| terminal);

    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.bg)
        .collect()
}

#[test]
fn without_a_background_setting_no_cell_gets_a_background() {
    let cells = backgrounds("", None);

    assert!(cells.iter().all(|bg| *bg == Color::Reset));
}

#[test]
fn a_solid_background_covers_every_cell_including_empty_ones() {
    let cells = backgrounds("[background]\ncolor = \"#102030\"\n", None);

    assert_eq!(cells.len(), 60 * 20);
    assert!(cells.iter().all(|bg| *bg == Color::Rgb(0x10, 0x20, 0x30)));
}

#[test]
fn a_tinted_background_uses_the_blend_with_the_terminals_color() {
    let cells = backgrounds(
        "[background]\ncolor = \"#ffffff\"\nopacity = 0.5\n",
        Some(Rgb { r: 0, g: 0, b: 0 }),
    );

    assert!(cells.iter().all(|bg| *bg == Color::Rgb(128, 128, 128)));
}

#[test]
fn transparent_leaves_the_terminals_background_to_show() {
    let cells = backgrounds("[background]\ncolor = \"transparent\"\n", None);

    assert!(cells.iter().all(|bg| *bg == Color::Reset));
}

#[test]
fn explicit_slot_backgrounds_still_win_over_the_painted_one() {
    let cells = backgrounds(
        "[background]\ncolor = \"#102030\"\n\n[\"status.bar\"]\nbg = \"red\"\n",
        None,
    );

    // The status bar's own background is on top of the painted one.
    assert!(cells.contains(&Color::Red));
    assert!(cells.contains(&Color::Rgb(0x10, 0x20, 0x30)));
}
