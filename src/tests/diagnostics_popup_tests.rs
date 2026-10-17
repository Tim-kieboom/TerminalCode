//! The box with the whole text of the diagnostics on a line, opened by `f7`.

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer as Screen;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::popups::diagnostic_popup::wrap_text;
use crate::components::prepare_and_render;
use crate::event::action::Action;
use crate::lsp::{Diagnostic, Severity};

fn diagnostic(
    range: std::ops::Range<usize>,
    severity: Severity,
    message: &str,
    source: Option<&str>,
    code: Option<&str>,
) -> Diagnostic {
    Diagnostic {
        range,
        severity,
        message: message.to_owned(),
        source: source.map(str::to_owned),
        code: code.map(str::to_owned),
    }
}

fn app_with(text: &str, diagnostics: Vec<Diagnostic>) -> App {
    let mut app = App::new(AppState::new(Editor::new(Buffer::from_text(text))));
    let document = app.state().workspace().active_document_id().unwrap();
    app.state_mut().diagnostics_mut().set(document, diagnostics);
    // A first frame, so that the cursor has a place on the screen.
    draw(&mut app, 100, 30);
    app
}

fn draw(app: &mut App, width: u16, height: u16) -> Screen {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn rows(screen: &Screen) -> Vec<String> {
    (0..screen.area.height)
        .map(|row| {
            (0..screen.area.width)
                .map(|column| screen[(column, row)].symbol().to_owned())
                .collect()
        })
        .collect()
}

fn text_of(screen: &Screen) -> String {
    rows(screen).join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn open(app: &App) -> bool {
    app.state().diagnostic_popup().is_some()
}

const RUSTC_MESSAGE: &str = "mismatched types\nexpected `u32`, found `&str`\nnote: expected type `u32`\n         found reference `&'static str`\nhelp: try converting it";

// ---- wrapping

fn wrap(text: &str, width: usize) -> Vec<String> {
    wrap_text(text, width)
}

#[test]
fn short_text_is_not_wrapped_and_line_breaks_stay() {
    assert_eq!(wrap("one\ntwo\n\nfour", 20), ["one", "two", "", "four"]);
    assert_eq!(wrap("", 10), [""]);
}

#[test]
fn a_long_line_is_broken_at_a_space() {
    assert_eq!(
        wrap("the quick brown fox jumps", 10),
        ["the quick", "brown fox", "jumps"]
    );
    assert_eq!(wrap("aaaa bbbb cccc", 9), ["aaaa bbbb", "cccc"]);
}

#[test]
fn a_word_longer_than_a_line_is_broken_in_the_middle() {
    assert_eq!(wrap("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap("ab abcdefghij", 4), ["ab", "abcd", "efgh", "ij"]);
}

#[test]
fn the_spaces_at_the_start_of_a_line_are_kept_for_what_a_compiler_quotes() {
    assert_eq!(wrap("    indented code", 40), ["    indented code"]);
    assert_eq!(
        wrap("  | let x: u32 = \"a\";", 40),
        ["  | let x: u32 = \"a\";"]
    );
}

#[test]
fn wide_characters_count_two_columns() {
    assert_eq!(wrap("日本語日本語", 6), ["日本語", "日本語"]);
    assert_eq!(wrap("日本語日本語", 5), ["日本", "語日", "本語"]);
}

#[test]
fn no_wrapped_line_is_wider_than_asked_and_no_text_is_lost() {
    let text = "a b  cc ddd eeee fffff gggggg hhhhhhh iiiiiiii jjjjjjjjj kkkkkkkkkk";
    for width in 1..30 {
        let lines = wrap(text, width);
        for line in &lines {
            assert!(
                unicode_width::UnicodeWidthStr::width(line.as_str()) <= width,
                "{width}: {line:?}"
            );
        }
        let squashed = |text: &str| text.split_whitespace().collect::<String>();
        assert_eq!(squashed(&lines.join(" ")), squashed(text), "width {width}");
    }
}

#[test]
fn a_zero_width_asked_for_does_not_loop_forever() {
    assert!(!wrap("abc", 0).is_empty());
}

// ---- opening it

#[test]
fn f7_shows_the_whole_message_with_its_notes_on_the_cursors_line() {
    let mut app = app_with(
        "let wrong: u32 = \"a\";\n",
        vec![diagnostic(
            17..20,
            Severity::Error,
            RUSTC_MESSAGE,
            Some("rustc"),
            Some("E0308"),
        )],
    );

    press(&mut app, KeyCode::F(7));
    let screen = text_of(&draw(&mut app, 100, 30));

    assert!(open(&app));
    assert!(screen.contains("error[E0308] (rustc)"), "{screen}");
    for line in RUSTC_MESSAGE.lines() {
        assert!(
            screen.contains(line.trim_end()),
            "missing {line:?} in\n{screen}"
        );
    }
    assert!(screen.contains("Diagnostics"), "{screen}");
}

#[test]
fn a_message_that_was_cut_after_the_code_can_be_read_whole() {
    let long = "this message is much longer than the line it is about and goes on and on until it cannot fit after the code ".repeat(2);
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Warning, long.trim(), None, None)],
    );

    press(&mut app, KeyCode::F(7));
    let screen = text_of(&draw(&mut app, 100, 30));

    assert!(screen.contains("cannot fit"), "{screen}");
    assert!(screen.contains("after the code"), "{screen}");
    assert!(screen.contains("warning"));
}

#[test]
fn all_the_diagnostics_of_the_line_are_there_the_worst_first() {
    let mut app = app_with(
        "let value = compute(a, b);\n",
        vec![
            diagnostic(4..9, Severity::Hint, "a hint here", None, None),
            diagnostic(12..19, Severity::Error, "the error here", None, None),
            diagnostic(20..21, Severity::Warning, "a warning here", None, None),
        ],
    );

    press(&mut app, KeyCode::F(7));
    let screen = text_of(&draw(&mut app, 100, 30));

    let at = |needle: &str| {
        screen
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle:?} in\n{screen}"))
    };
    assert!(at("the error here") < at("a warning here"));
    assert!(at("a warning here") < at("a hint here"));
}

#[test]
fn a_diagnostic_that_runs_into_the_line_from_above_is_there_too() {
    let mut app = app_with(
        "first line\nsecond line\n",
        vec![diagnostic(
            5..15,
            Severity::Error,
            "spans both lines",
            None,
            None,
        )],
    );
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::F(7));
    let screen = text_of(&draw(&mut app, 100, 30));

    assert!(screen.contains("spans both lines"), "{screen}");
}

#[test]
fn only_the_cursors_line_is_shown() {
    let mut app = app_with(
        "first\nsecond\n",
        vec![
            diagnostic(0..5, Severity::Error, "about the first", None, None),
            diagnostic(6..12, Severity::Error, "about the second", None, None),
        ],
    );
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::F(7));
    let popup = app.state().diagnostic_popup().unwrap().lines(60);

    let text: Vec<&str> = popup.iter().map(|(_, line)| line.as_str()).collect();
    assert!(text.contains(&"about the second"));
    assert!(!text.contains(&"about the first"));
}

#[test]
fn a_line_with_no_diagnostics_says_so_and_opens_nothing() {
    let mut app = app_with(
        "fine\nbad\n",
        vec![diagnostic(5..8, Severity::Error, "bad", None, None)],
    );

    press(&mut app, KeyCode::F(7));

    assert!(!open(&app));
    assert!(
        app.state()
            .latest_notification()
            .is_some_and(|text| text.contains("no diagnostics on this line")),
        "{:?}",
        app.state().latest_notification()
    );
}

#[test]
fn the_palette_lists_it() {
    assert!(Action::palette_actions().contains(&Action::ShowDiagnostics));
    assert_eq!(
        Action::ShowDiagnostics.title(),
        Some("Diagnostics: Show at Cursor")
    );
}

// ---- closing it

#[test]
fn any_other_key_closes_it_and_does_nothing_else() {
    for key in [
        KeyCode::Esc,
        KeyCode::Char('x'),
        KeyCode::Enter,
        KeyCode::F(7),
        KeyCode::Left,
        KeyCode::Backspace,
    ] {
        let mut app = app_with(
            "let x = 1;\n",
            vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
        );
        press(&mut app, KeyCode::F(7));
        assert!(open(&app));

        press(&mut app, key);

        assert!(!open(&app), "{key:?} did not close it");
        assert_eq!(
            app.state().editor().buffer().text(),
            "let x = 1;\n",
            "{key:?} changed the text"
        );
        assert_eq!(
            app.state().editor().selections().primary().head().column,
            0,
            "{key:?} moved the cursor"
        );
    }
}

#[test]
fn a_click_closes_it() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );
    press(&mut app, KeyCode::F(7));

    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 10,
        modifiers: KeyModifiers::NONE,
    }));

    assert!(!open(&app));
}

#[test]
fn after_it_closes_the_screen_is_as_it_was() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );
    let before = rows(&draw(&mut app, 100, 30));
    press(&mut app, KeyCode::F(7));
    draw(&mut app, 100, 30);

    press(&mut app, KeyCode::Esc);

    assert_eq!(rows(&draw(&mut app, 100, 30)), before);
}

#[test]
fn it_can_be_opened_again_after_closing() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );

    press(&mut app, KeyCode::F(7));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::F(7));

    assert!(open(&app));
}

// ---- where it goes

/// The first row and column of the box: where its top left corner is.
fn corner(screen: &Screen) -> (u16, u16) {
    let rows = rows(screen);
    let row = rows
        .iter()
        .position(|row| row.contains("Diagnostics"))
        .expect("no box");
    let column = rows[row]
        .find('╭')
        .map(|byte| rows[row][..byte].chars().count())
        .expect("no corner");
    (column as u16, row as u16)
}

#[test]
fn the_box_is_under_the_cursors_line_when_there_is_room() {
    let mut app = app_with(
        "fn main() {}\n",
        vec![diagnostic(3..7, Severity::Error, "boom", None, None)],
    );
    let cursor = app.state().editor().cursor_cell().unwrap();

    press(&mut app, KeyCode::F(7));
    let (column, row) = corner(&draw(&mut app, 100, 30));

    assert_eq!(row, cursor.y + 1);
    assert_eq!(column, cursor.x);
}

#[test]
fn the_box_goes_above_the_line_when_there_is_no_room_below() {
    let mut text = String::new();
    for number in 0..40 {
        text.push_str(&format!("let line{number} = {number};\n"));
    }
    let mut app = app_with(
        &text,
        vec![diagnostic(0..3, Severity::Error, "boom", None, None)],
    );
    // The cursor is on the last visible row.
    for _ in 0..8 {
        press(&mut app, KeyCode::Down);
    }
    draw(&mut app, 100, 12);
    let at = text
        .lines()
        .take(8)
        .map(|line| line.len() + 1)
        .sum::<usize>();
    let document = app.state().workspace().active_document_id().unwrap();
    app.state_mut().diagnostics_mut().set(
        document,
        vec![diagnostic(at..at + 3, Severity::Error, "boom", None, None)],
    );
    let cursor = app.state().editor().cursor_cell().unwrap();

    press(&mut app, KeyCode::F(7));
    let (_, row) = corner(&draw(&mut app, 100, 12));

    assert!(
        row < cursor.y,
        "the box is above the cursor line {}: it starts at {row}",
        cursor.y
    );
}

#[test]
fn the_box_stays_inside_a_narrow_screen() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(
            4..5,
            Severity::Error,
            "a message that is wider than the screen is",
            None,
            None,
        )],
    );

    press(&mut app, KeyCode::F(7));
    let screen = draw(&mut app, 40, 20);

    let text = text_of(&screen);
    assert!(text.contains("Diagnostics"));
    for row in rows(&screen) {
        assert_eq!(row.chars().count(), 40);
    }
}

#[test]
fn a_tiny_screen_does_not_break_it() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );

    press(&mut app, KeyCode::F(7));

    for (width, height) in [(10, 5), (30, 4), (5, 3), (1, 1)] {
        draw(&mut app, width, height);
    }
}

// ---- a long message

fn long_message() -> String {
    (1..=40)
        .map(|number| format!("note number {number}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_message_longer_than_the_box_scrolls_with_the_arrow_and_page_keys() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(
            4..5,
            Severity::Error,
            &long_message(),
            None,
            None,
        )],
    );
    press(&mut app, KeyCode::F(7));
    let first = text_of(&draw(&mut app, 100, 20));
    assert!(
        first.contains("note number 1\n") || first.contains("note number 1 "),
        "{first}"
    );
    assert!(!first.contains("note number 40"), "{first}");
    assert!(
        first.contains("scroll"),
        "the help says it scrolls: {first}"
    );

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.state().diagnostic_popup().unwrap().scroll_position(), 2);
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::PageDown);
    let last = text_of(&draw(&mut app, 100, 20));

    assert!(last.contains("note number 40"), "{last}");
    assert!(open(&app), "scrolling does not close it");
}

#[test]
fn scrolling_stops_at_the_ends() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(
            4..5,
            Severity::Error,
            &long_message(),
            None,
            None,
        )],
    );
    press(&mut app, KeyCode::F(7));
    draw(&mut app, 100, 20);

    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.state().diagnostic_popup().unwrap().scroll_position(), 0);
    for _ in 0..20 {
        press(&mut app, KeyCode::PageDown);
    }
    let bottom = app.state().diagnostic_popup().unwrap().scroll_position();
    press(&mut app, KeyCode::Down);

    assert_eq!(
        app.state().diagnostic_popup().unwrap().scroll_position(),
        bottom
    );
    assert!(bottom > 0);
}

#[test]
fn a_short_message_has_nothing_to_scroll() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );
    press(&mut app, KeyCode::F(7));
    draw(&mut app, 100, 20);

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::PageDown);

    assert_eq!(app.state().diagnostic_popup().unwrap().scroll_position(), 0);
    assert!(open(&app));
    assert!(text_of(&draw(&mut app, 100, 20)).contains("any key: close"));
}

#[test]
fn a_long_line_of_the_message_is_wrapped_inside_the_box() {
    let long = "word ".repeat(60);
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, long.trim(), None, None)],
    );

    press(&mut app, KeyCode::F(7));
    let screen = draw(&mut app, 60, 30);

    let box_rows: Vec<String> = rows(&screen)
        .into_iter()
        .filter(|row| row.contains('│'))
        .collect();
    assert!(box_rows.len() > 2, "{box_rows:?}");
    // Every row of the box ends inside the box: the right border is on it.
    assert!(box_rows.iter().all(|row| row.contains('│')), "{box_rows:?}");
}

#[test]
fn the_message_of_a_diagnostic_with_wide_characters_is_shown() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(
            4..5,
            Severity::Error,
            "名前が見つかりません: 日本語のエラー",
            None,
            None,
        )],
    );

    press(&mut app, KeyCode::F(7));
    let screen = text_of(&draw(&mut app, 60, 20));

    // A wide character takes two cells and the buffer shows a blank after it.
    let squeezed: String = screen.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains("日本語のエラー"), "{screen}");
}

#[test]
fn editing_after_it_closed_still_works_and_the_diagnostics_still_follow() {
    let mut app = app_with(
        "let x = 1;\n",
        vec![diagnostic(4..5, Severity::Error, "boom", None, None)],
    );
    press(&mut app, KeyCode::F(7));
    press(&mut app, KeyCode::Esc);

    press(&mut app, KeyCode::Char('x'));

    assert_eq!(app.state().editor().buffer().text(), "xlet x = 1;\n");
}
