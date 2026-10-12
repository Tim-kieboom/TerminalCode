//! How diagnostics look: a mark in the gutter, a line under the text they are
//! about, the message of the cursor's line in the status bar, and `f8`.

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer as Screen;
use ratatui::style::{Color, Modifier};

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::action::Action;
use crate::lsp::{Diagnostic, Severity};
use crate::ui::theme::{Theme, ThemeVariant};

fn diagnostic(range: std::ops::Range<usize>, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        range,
        severity,
        message: message.to_owned(),
        source: None,
        code: None,
    }
}

/// An app with `text` open and `diagnostics` set on it.
fn app_with(text: &str, diagnostics: Vec<Diagnostic>) -> App {
    let mut app = App::new(AppState::new(Editor::new(Buffer::from_text(text))));
    let document = app.state().workspace().active_document_id().unwrap();
    app.state_mut().diagnostics_mut().set(document, diagnostics);
    app
}

fn draw(app: &mut App) -> Screen {
    let mut terminal = Terminal::new(TestBackend::new(100, 12)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(screen: &Screen, row: u16) -> String {
    (0..screen.area.width)
        .map(|column| screen[(column, row)].symbol().to_owned())
        .collect()
}

/// The row that shows `needle`.
fn row_of(screen: &Screen, needle: &str) -> u16 {
    (0..screen.area.height)
        .find(|row| row_text(screen, *row).contains(needle))
        .unwrap_or_else(|| panic!("no row shows {needle:?}"))
}

/// The column of the first cell of `needle` on `row`.
fn column_of(screen: &Screen, row: u16, needle: &str) -> u16 {
    let text: Vec<String> = (0..screen.area.width)
        .map(|column| screen[(column, row)].symbol().to_owned())
        .collect();
    let joined = text.concat();
    let byte = joined.find(needle).unwrap();
    joined[..byte].chars().count() as u16
}

fn is_underlined(screen: &Screen, column: u16, row: u16) -> bool {
    screen[(column, row)]
        .modifier
        .contains(Modifier::UNDERLINED)
}

fn theme_color(slot: &str, underline: bool) -> Color {
    let style = Theme::built_in(ThemeVariant::Dark).style(slot);
    match underline {
        true => style.underline_color.unwrap(),
        false => style.fg.unwrap(),
    }
}

// ---- the message after the code

/// The color of the first cell of `needle` on the row that shows `code`.
fn color_after(screen: &Screen, code: &str, needle: &str) -> Color {
    let row = row_of(screen, code);
    screen[(column_of(screen, row, needle), row)].fg
}

#[test]
fn the_message_of_a_diagnostic_is_after_the_code_of_its_line_in_the_color_of_its_severity() {
    let mut app = app_with(
        "fn main() {}\nlet a = 1;\n",
        vec![diagnostic(3..7, Severity::Error, "the name main is taken")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    let text = row_text(&screen, row);
    assert!(
        text.contains("fn main() {}  the name main is taken"),
        "{text}"
    );
    assert_eq!(
        color_after(&screen, "fn main", "the name main"),
        theme_color("editor.diagnostic.error", false)
    );
    assert!(
        !row_text(&screen, row_of(&screen, "let a")).contains("the name"),
        "only on its own line"
    );
}

#[test]
fn nothing_in_the_gutter_looks_like_a_breakpoint() {
    let mut app = app_with(
        "fn main() {}\nlet a = 1;\n",
        vec![
            diagnostic(3..7, Severity::Error, "e"),
            diagnostic(17..18, Severity::Warning, "w"),
        ],
    );

    let screen = draw(&mut app);

    for row in 0..screen.area.height {
        assert!(
            !row_text(&screen, row).contains('●'),
            "row {row}: {}",
            row_text(&screen, row)
        );
    }
}

#[test]
fn a_file_with_no_diagnostics_has_no_message_and_no_underlines() {
    let mut app = app_with("fn main() {}\n", Vec::new());

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    assert!((0..screen.area.width).all(|column| !is_underlined(&screen, column, row)));
    assert_eq!(row_text(&screen, row).trim_end().chars().last(), Some('}'));
}

#[test]
fn a_line_with_several_diagnostics_says_the_worst_and_counts_the_others() {
    let mut app = app_with(
        "let value = compute(a, b);\n",
        vec![
            diagnostic(4..9, Severity::Hint, "a hint"),
            diagnostic(12..19, Severity::Error, "the error"),
            diagnostic(20..21, Severity::Warning, "a warning"),
        ],
    );

    let screen = draw(&mut app);

    let text = row_text(&screen, row_of(&screen, "let value"));
    assert!(text.contains("compute(a, b);  the error (+2)"), "{text}");
    assert_eq!(
        color_after(&screen, "let value", "the error"),
        theme_color("editor.diagnostic.error", false)
    );
}

#[test]
fn only_the_first_line_of_a_long_message_is_said() {
    let mut app = app_with(
        "fn main() {}\n",
        vec![diagnostic(
            3..7,
            Severity::Error,
            "mismatched types\nexpected `u32`\nfound `&str`",
        )],
    );

    let screen = draw(&mut app);

    let text = row_text(&screen, row_of(&screen, "fn main"));
    assert!(text.contains("mismatched types"), "{text}");
    assert!(!text.contains("expected"), "{text}");
}

#[test]
fn a_diagnostic_over_several_lines_is_said_once_on_the_line_it_starts_on() {
    let mut app = app_with(
        "fn main() {}\nlet a = 1;\nlet b = 2;\n",
        vec![diagnostic(3..16, Severity::Error, "spans two lines")],
    );

    let screen = draw(&mut app);

    let said = |code: &str| row_text(&screen, row_of(&screen, code)).contains("spans two lines");
    assert!(said("fn main"));
    assert!(!said("let a"), "not again on the line it runs into");
    assert!(!said("let b"));
}

#[test]
fn each_line_says_its_own() {
    let mut app = app_with(
        "fn main() {}\nlet a = 1;\n",
        vec![
            diagnostic(3..7, Severity::Error, "first one"),
            diagnostic(17..18, Severity::Warning, "second one"),
        ],
    );

    let screen = draw(&mut app);

    assert!(row_text(&screen, row_of(&screen, "fn main")).contains("first one"));
    assert!(row_text(&screen, row_of(&screen, "let a")).contains("second one"));
}

#[test]
fn a_message_that_does_not_fit_is_cut_with_an_ellipsis_at_the_edge_of_the_text() {
    let mut app = app_with(
        "let value = compute(a, b);\n",
        vec![diagnostic(
            12..19,
            Severity::Error,
            &"very long words ".repeat(20),
        )],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "let value");
    let text = row_text(&screen, row);
    assert!(text.contains("very long words"), "{text}");
    assert!(text.contains('…'), "{text}");
    // It uses all the room there is: the cut is at the last cell of the text area.
    let ellipsis = column_of(&screen, row, "…");
    assert_eq!(ellipsis, screen.area.width - 1);
}

#[test]
fn a_message_is_cut_by_columns_not_by_characters() {
    // Each of these takes two columns.
    let mut app = app_with(
        "let a = 1;\n",
        vec![diagnostic(4..5, Severity::Error, &"日本語".repeat(40))],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "let a");
    let ellipsis = column_of(&screen, row, "…");
    assert!(ellipsis < screen.area.width - 1);
}

#[test]
fn with_no_room_after_the_code_nothing_is_said() {
    let long = "x".repeat(300);
    let mut app = app_with(
        &format!("{long}\nshort\n"),
        vec![diagnostic(0..1, Severity::Error, "never seen")],
    );

    let screen = draw(&mut app);

    assert!(!row_text(&screen, row_of(&screen, "xxxx")).contains("never"));
}

#[test]
fn a_line_that_has_scrolled_out_of_sight_to_the_left_takes_its_message_with_it() {
    let long = "x".repeat(300);
    let text = format!("{long}\nshort\n");
    let mut app = app_with(
        &text,
        vec![diagnostic(
            long.len() + 1..long.len() + 2,
            Severity::Error,
            "on the short line",
        )],
    );
    assert!(
        row_text(&draw(&mut app), row_of(&draw(&mut app), "short")).contains("on the short line")
    );

    // End of the first line: the window scrolls right, past the end of the short line.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::End,
        KeyModifiers::NONE,
    )));
    let screen = draw(&mut app);

    assert!(
        !(0..screen.area.height).any(|row| row_text(&screen, row).contains("on the short line"))
    );
}

#[test]
fn a_tab_in_the_code_is_counted_at_the_width_it_is_drawn() {
    let mut app = app_with(
        "\tlet x = 1;\n",
        vec![diagnostic(5..6, Severity::Error, "on x")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "let x");
    let code_end = column_of(&screen, row, "1;") + 2;
    let message = column_of(&screen, row, "on x");
    assert_eq!(
        message,
        code_end + 2,
        "two columns after the end of the code"
    );
}

#[test]
fn a_diagnostic_on_an_empty_line_is_said_on_that_line() {
    let mut app = app_with(
        "alpha\n\nbeta\n",
        vec![diagnostic(6..6, Severity::Warning, "empty here")],
    );

    let screen = draw(&mut app);

    let said: Vec<u16> = (0..screen.area.height)
        .filter(|row| row_text(&screen, *row).contains("empty here"))
        .collect();
    assert_eq!(said.len(), 1);
    let above = row_of(&screen, "alpha");
    assert_eq!(said[0], above + 1, "on the empty line after alpha");
}

// ---- the line under the text

#[test]
fn the_text_a_diagnostic_is_about_is_underlined_in_the_color_of_its_severity() {
    let mut app = app_with(
        "fn main() {}\n",
        vec![diagnostic(3..7, Severity::Error, "on main")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    let start = column_of(&screen, row, "main");
    for column in 0..screen.area.width.min(60) {
        let inside = (start..start + 4).contains(&column);
        assert_eq!(
            is_underlined(&screen, column, row),
            inside,
            "column {column}"
        );
    }
    assert_eq!(
        screen[(start, row)].underline_color,
        theme_color("editor.diagnostic.error", true)
    );
}

#[test]
fn the_underline_does_not_change_the_color_of_the_text() {
    let mut plain = app_with("fn main() {}\n", Vec::new());
    let mut marked = app_with(
        "fn main() {}\n",
        vec![diagnostic(3..7, Severity::Error, "on main")],
    );

    let before = draw(&mut plain);
    let after = draw(&mut marked);

    let row = row_of(&before, "fn main");
    let column = column_of(&before, row, "main");
    assert_eq!(before[(column, row)].fg, after[(column, row)].fg);
}

#[test]
fn a_worse_diagnostic_is_drawn_over_a_milder_one_where_they_meet() {
    let mut app = app_with(
        "fn main() {}\n",
        vec![
            diagnostic(3..7, Severity::Warning, "w"),
            diagnostic(5..7, Severity::Error, "e"),
        ],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    let start = column_of(&screen, row, "main");
    assert_eq!(
        screen[(start, row)].underline_color,
        theme_color("editor.diagnostic.warning", true)
    );
    assert_eq!(
        screen[(start + 3, row)].underline_color,
        theme_color("editor.diagnostic.error", true)
    );
}

#[test]
fn a_range_after_a_wide_character_is_underlined_under_the_right_cell() {
    // The emoji takes two cells and four bytes.
    let mut app = app_with("a😀b\n", vec![diagnostic(5..6, Severity::Error, "on b")]);

    let screen = draw(&mut app);

    let row = row_of(&screen, "a😀");
    let b = column_of(&screen, row, "b");
    assert!(is_underlined(&screen, b, row));
    assert!(!is_underlined(&screen, b - 1, row));
    assert!(!is_underlined(&screen, b - 3, row), "not the a");
}

#[test]
fn a_range_over_several_lines_is_underlined_on_each_as_far_as_it_goes() {
    let mut app = app_with(
        "fn main() {}\nlet a = 1;\n",
        vec![diagnostic(3..16, Severity::Error, "two lines")],
    );

    let screen = draw(&mut app);

    let first = row_of(&screen, "fn main");
    let second = row_of(&screen, "let a");
    let main = column_of(&screen, first, "main");
    let tail = column_of(&screen, first, "() {}");
    assert!(is_underlined(&screen, main, first));
    assert!(
        is_underlined(&screen, tail, first),
        "to the end of the line"
    );
    let start = column_of(&screen, second, "let");
    assert!(
        is_underlined(&screen, start, second),
        "from the start of the next"
    );
    assert!(is_underlined(&screen, start + 2, second));
    assert!(!is_underlined(&screen, start + 4, second), "and no further");
}

#[test]
fn a_diagnostic_with_no_length_underlines_the_character_it_is_at() {
    let mut app = app_with(
        "fn main() {}\n",
        vec![diagnostic(3..3, Severity::Hint, "here")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    let start = column_of(&screen, row, "main");
    assert!(is_underlined(&screen, start, row));
    assert!(!is_underlined(&screen, start + 1, row));
}

#[test]
fn one_at_the_end_of_a_line_has_nothing_to_underline_but_is_still_said_after_the_code() {
    let mut app = app_with(
        "fn main() {}\nnext\n",
        vec![diagnostic(12..12, Severity::Error, "missing ;")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "fn main");
    assert!((0..screen.area.width).all(|column| !is_underlined(&screen, column, row)));
    assert!(row_text(&screen, row).contains("missing ;"));
}

#[test]
fn a_tab_before_the_range_does_not_shift_the_line_under_the_text() {
    let mut app = app_with(
        "\tlet x = 1;\n",
        vec![diagnostic(5..6, Severity::Error, "on x")],
    );

    let screen = draw(&mut app);

    let row = row_of(&screen, "let x");
    let x = column_of(&screen, row, "x");
    assert!(is_underlined(&screen, x, row));
    assert!(!is_underlined(&screen, x + 1, row));
}

// ---- the status bar

fn status(screen: &Screen) -> String {
    row_text(screen, row_of(screen, "Ln "))
}

// ---- f8

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

fn cursor(app: &App) -> (usize, usize) {
    let head = app.state().editor().selections().primary().head();
    (head.line, head.column)
}

fn three() -> App {
    app_with(
        "fn main() {}\nlet a = 1;\nlet b = 2;\n",
        vec![
            diagnostic(3..7, Severity::Error, "first"),
            diagnostic(17..18, Severity::Warning, "second"),
            diagnostic(28..29, Severity::Hint, "third"),
        ],
    )
}

#[test]
fn f8_goes_to_the_next_diagnostic_and_round_to_the_first_after_the_last() {
    let mut app = three();

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    assert_eq!(cursor(&app), (0, 3));
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    assert_eq!(cursor(&app), (1, 4));
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    assert_eq!(cursor(&app), (2, 4));
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    assert_eq!(cursor(&app), (0, 3), "round to the first");
}

#[test]
fn shift_f8_goes_to_the_previous_one_and_round_to_the_last() {
    let mut app = three();

    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    assert_eq!(cursor(&app), (2, 4), "from the start, round to the last");
    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    assert_eq!(cursor(&app), (1, 4));
    press(&mut app, KeyCode::F(8), KeyModifiers::SHIFT);
    assert_eq!(cursor(&app), (0, 3));
}

#[test]
fn after_a_jump_the_cursor_is_on_the_line_that_says_it() {
    let mut app = three();

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    let screen = draw(&mut app);
    assert!(row_text(&screen, row_of(&screen, "let a")).contains("second"));
    assert_eq!(cursor(&app).0, 1, "the cursor is on that line");
}

#[test]
fn f8_with_no_diagnostics_says_so_and_does_not_move() {
    let mut app = app_with("fn main() {}\n", Vec::new());

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);

    assert_eq!(cursor(&app), (0, 0));
    assert!(
        app.state()
            .latest_notification()
            .is_some_and(|text| text.contains("no diagnostics")),
        "{:?}",
        app.state().latest_notification()
    );
}

#[test]
fn a_jump_scrolls_the_diagnostic_into_view() {
    let mut text = String::new();
    for number in 0..60 {
        text.push_str(&format!("let line{number} = {number};\n"));
    }
    let at = text.find("line50").unwrap();
    let mut app = app_with(
        &text,
        vec![diagnostic(at..at + 6, Severity::Error, "far down")],
    );
    draw(&mut app);

    press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
    let screen = draw(&mut app);

    assert!(row_text(&screen, row_of(&screen, "line50")).contains("let line50"));
}

#[test]
fn the_palette_lists_both_and_the_default_keymap_binds_them() {
    assert!(Action::palette_actions().contains(&Action::NextDiagnostic));
    assert!(Action::palette_actions().contains(&Action::PreviousDiagnostic));
    assert_eq!(Action::NextDiagnostic.title(), Some("Diagnostics: Next"));
    assert_eq!(
        Action::PreviousDiagnostic.title(),
        Some("Diagnostics: Previous")
    );
}

#[test]
fn the_server_status_comes_before_the_key_hints() {
    let mut app = app_with("fn main() {}\n", Vec::new());
    app.state_mut()
        .set_lsp_status(Some("rust-analyzer".to_owned()));

    let status = status(&draw(&mut app));

    assert!(
        status.find("[rust-analyzer]").unwrap() < status.find("quit: Ctr+Q").unwrap(),
        "{status}"
    );
}
