use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::buffer::Position;
use crate::components::prepare_and_render;

const TEXT: &str = "alpha beta\nBeta gamma\ndelta beta\n";

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn alt(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::ALT));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn app_with(text: &str) -> App {
    let mut app = App::default();
    app.handle_input(InputEvent::Paste(text.to_owned()));
    app.handle_input(key(KeyCode::Home, KeyModifiers::CONTROL));
    app
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn selection(app: &App) -> (Position, Position) {
    let selection = *app.state().editor().selections().primary();
    (selection.start(), selection.end())
}

fn text(app: &App) -> String {
    app.state().editor().buffer().text()
}

#[test]
fn ctrl_f_opens_the_bar_and_typing_selects_the_first_match_from_the_cursor() {
    let mut app = app_with(TEXT);

    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    assert!(app.state().find().is_some());
    assert_eq!(selection(&app), (Position::new(0, 6), Position::new(0, 10)));
}

#[test]
fn enter_steps_to_the_next_match_and_wraps() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(1, 0));
    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(2, 6));
    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(0, 6), "wrapped around");
}

#[test]
fn shift_enter_and_up_step_backwards() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(selection(&app).0, Position::new(2, 6), "wrapped backwards");
    press(&mut app, KeyCode::Up);
    assert_eq!(selection(&app).0, Position::new(1, 0));
}

#[test]
fn escape_closes_the_bar_and_leaves_the_match_selected() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "gamma");

    press(&mut app, KeyCode::Esc);

    assert!(app.state().find().is_none());
    assert_eq!(selection(&app), (Position::new(1, 5), Position::new(1, 10)));
    assert_eq!(text(&app), TEXT, "nothing was typed into the document");
}

#[test]
fn typing_in_the_bar_does_not_edit_the_document() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');

    type_str(&mut app, "abc");
    ctrl(&mut app, 'z');
    ctrl(&mut app, 'q');

    assert_eq!(text(&app), TEXT);
    assert!(!app.should_quit());
}

#[test]
fn match_case_and_regex_toggle_while_the_bar_is_open() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "Beta");
    assert_eq!(app.state().find().unwrap().matches().len(), 3);

    alt(&mut app, 'c');
    assert_eq!(app.state().find().unwrap().matches().len(), 1);
    assert_eq!(selection(&app).0, Position::new(1, 0));

    alt(&mut app, 'c');
    alt(&mut app, 'r');
    type_str(&mut app, "|delta");
    assert_eq!(app.state().find().unwrap().matches().len(), 4);
}

#[test]
fn a_selected_word_becomes_the_query() {
    let mut app = app_with(TEXT);
    for _ in 0..6 {
        press(&mut app, KeyCode::Right);
    }
    for _ in 0..4 {
        app.handle_input(key(KeyCode::Right, KeyModifiers::SHIFT));
    }

    ctrl(&mut app, 'f');

    assert_eq!(app.state().find().unwrap().query(), "beta");
    assert_eq!(
        selection(&app).0,
        Position::new(0, 6),
        "the selected word is the first match"
    );
}

#[test]
fn the_last_query_comes_back_when_the_bar_is_opened_again() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    alt(&mut app, 'c');
    type_str(&mut app, "delta");
    press(&mut app, KeyCode::Esc);

    ctrl(&mut app, 'f');

    let find = app.state().find().unwrap();
    assert_eq!(find.query(), "delta");
    assert!(find.options().case_sensitive);
}

#[test]
fn the_bar_takes_a_row_from_the_editor_and_shows_the_count() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    let screen = screen(&mut app, 120, 20);

    assert!(screen.contains("Find: beta"), "{screen}");
    assert!(screen.contains("1 of 3"), "{screen}");
    assert!(screen.contains("[ ] case (alt+c)"), "{screen}");
    assert!(screen.contains("[ ] regex (alt+r)"), "{screen}");
}

#[test]
fn no_matches_and_an_invalid_pattern_are_said_in_the_bar() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "zzz");
    assert!(screen(&mut app, 100, 20).contains("no results"));

    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    alt(&mut app, 'r');
    type_str(&mut app, "(beta");

    assert!(screen(&mut app, 100, 20).contains("invalid pattern"));
}

#[test]
fn closing_the_bar_gives_the_row_back() {
    let mut app = app_with(&"line\n".repeat(30));
    let before = screen(&mut app, 100, 30);
    ctrl(&mut app, 'f');
    let during = screen(&mut app, 100, 30);
    press(&mut app, KeyCode::Esc);
    let after = screen(&mut app, 100, 30);

    let rows = |screen: &str| screen.lines().filter(|row| row.contains("line")).count();
    assert_eq!(rows(&during) + 1, rows(&before));
    assert_eq!(rows(&after), rows(&before));
}

#[test]
fn a_match_far_down_scrolls_into_view_above_the_bar() {
    let mut text = "filler\n".repeat(40);
    text.push_str("the needle is here\n");
    let mut app = app_with(&text);
    ctrl(&mut app, 'f');

    type_str(&mut app, "needle");

    let screen = screen(&mut app, 100, 30);
    assert!(screen.contains("the needle is here"), "{screen}");
    assert!(screen.contains("Find: needle"), "{screen}");
}

#[test]
fn every_match_on_screen_is_highlighted_not_just_the_current_one() {
    use ratatui::style::Color;
    let mut app = app_with("beta one\nbeta two\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();

    let highlighted = |row: u16| {
        (0..80)
            .filter(|column| buffer[(*column, row)].bg == Color::Yellow)
            .count()
    };
    // The default theme paints matches on a yellow background. Find the rows
    // that hold the two lines of text.
    let rows: Vec<u16> = (0..30)
        .filter(|row| {
            (0..80)
                .map(|column| buffer[(column, *row)].symbol().to_owned())
                .collect::<String>()
                .contains("beta two")
        })
        .collect();
    assert_eq!(rows.len(), 1, "the line with the second match");
    assert_eq!(highlighted(rows[0]), 4, "the other match is highlighted");
}

#[test]
fn the_bar_takes_the_mouse_and_paste() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');

    app.handle_input(InputEvent::Paste("pasted".to_owned()));
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(text(&app), TEXT);
    assert!(app.state().find().is_some());
}

#[test]
fn find_from_the_palette_and_its_key_is_listed() {
    let mut app = app_with(TEXT);

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "find in file");

    let entry = app.state().palette().unwrap().visible().next().unwrap();
    assert_eq!(entry.title, "Search: Find in File");
    assert_eq!(entry.keys.as_deref(), Some("ctrl+f"));
    press(&mut app, KeyCode::Enter);
    assert!(app.state().find().is_some());
}

#[test]
fn find_does_nothing_with_no_open_file() {
    let mut app = App::default();
    ctrl(&mut app, 'w');

    ctrl(&mut app, 'f');

    assert!(app.state().find().is_none());
}

#[test]
fn a_tiny_terminal_does_not_panic() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    for (width, height) in [(1, 1), (5, 2), (20, 3), (40, 4)] {
        screen(&mut app, width, height);
    }
}

// ---- whole words and replacing

fn open_replace(app: &mut App) {
    ctrl(app, 'r');
}

/// The bar open with `query` typed, then `replacement` in the row below.
fn replace_bar(app: &mut App, query: &str, replacement: &str) {
    ctrl(app, 'f');
    type_str(app, query);
    // Opens the replacement row and moves the typing to it.
    ctrl(app, 'r');
    type_str(app, replacement);
}

#[test]
fn alt_w_switches_whole_word_and_the_bar_shows_it() {
    let mut app = app_with("cat concat cat_x\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "cat");
    assert!(screen(&mut app, 120, 20).contains("1 of 3"));

    alt(&mut app, 'w');

    let screen = screen(&mut app, 120, 20);
    assert!(screen.contains("1 of 1"), "{screen}");
    assert!(screen.contains("[x] word (alt+w)"), "{screen}");
}

#[test]
fn a_narrow_bar_shows_the_switches_in_a_shorter_form() {
    let mut app = app_with("cat\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "cat");
    alt(&mut app, 'w');

    let screen = screen(&mut app, 70, 20);

    assert!(
        screen.contains("[x] word") || screen.contains("[x]w"),
        "{screen}"
    );
    assert!(screen.contains("1 of 1"), "{screen}");
}

#[test]
fn ctrl_r_opens_the_bar_with_a_row_for_the_replacement() {
    let mut app = app_with(TEXT);

    open_replace(&mut app);

    let screen = screen(&mut app, 120, 20);
    assert!(screen.contains("Find:"), "{screen}");
    assert!(screen.contains("Replace:"), "{screen}");
}

#[test]
fn typing_after_ctrl_r_goes_to_the_replacement_and_tab_goes_back_to_the_query() {
    let mut app = app_with(TEXT);
    open_replace(&mut app);

    type_str(&mut app, "new");
    press(&mut app, KeyCode::Tab);
    type_str(&mut app, "beta");

    let screen = screen(&mut app, 120, 20);
    assert!(screen.contains("Replace: new"), "{screen}");
    assert!(screen.contains("Find: beta"), "{screen}");
}

#[test]
fn enter_in_the_replacement_replaces_the_current_match_and_selects_the_next() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "BETA");

    press(&mut app, KeyCode::Enter);

    assert_eq!(text(&app), "alpha BETA\nBeta gamma\ndelta beta\n");
    assert_eq!(
        selection(&app),
        (Position::new(1, 0), Position::new(1, 4)),
        "on the next match"
    );
}

#[test]
fn enter_in_the_replacement_goes_through_every_match_and_wraps() {
    let mut app = app_with("a a a\n");
    replace_bar(&mut app, "a", "b");

    for _ in 0..3 {
        press(&mut app, KeyCode::Enter);
    }

    assert_eq!(text(&app), "b b b\n");
    // Nothing is left to find, so there is nothing to replace.
    press(&mut app, KeyCode::Enter);
    assert_eq!(text(&app), "b b b\n");
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("nothing to replace")
    );
}

#[test]
fn a_replacement_that_holds_the_query_does_not_go_on_forever() {
    let mut app = app_with("a a\n");
    replace_bar(&mut app, "a", "aa");

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);

    assert_eq!(text(&app), "aa aa\n");
}

#[test]
fn enter_in_the_query_still_just_goes_to_the_next_match() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "X");
    press(&mut app, KeyCode::Tab);

    press(&mut app, KeyCode::Enter);

    assert_eq!(text(&app), TEXT);
    assert_eq!(selection(&app).0, Position::new(1, 0));
}

#[test]
fn alt_a_replaces_every_match_at_once() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "B");

    alt(&mut app, 'a');

    assert_eq!(text(&app), "alpha B\nB gamma\ndelta B\n");
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("replaced 3 occurrences")
    );
}

#[test]
fn replace_all_is_one_step_to_undo() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "B");
    alt(&mut app, 'a');
    press(&mut app, KeyCode::Esc);

    ctrl(&mut app, 'z');

    assert_eq!(text(&app), TEXT);
}

#[test]
fn a_single_replacement_is_undone_alone() {
    let mut app = app_with("a a\n");
    replace_bar(&mut app, "a", "b");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);

    ctrl(&mut app, 'z');

    assert_eq!(text(&app), "b a\n");
}

#[test]
fn replace_all_puts_the_cursor_at_the_first_replacement() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "B");

    alt(&mut app, 'a');

    assert_eq!(selection(&app), (Position::new(0, 6), Position::new(0, 6)));
}

#[test]
fn replace_all_with_one_match_says_occurrence_not_occurrences() {
    let mut app = app_with("one two\n");
    replace_bar(&mut app, "two", "2");

    alt(&mut app, 'a');

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("replaced 1 occurrence")
    );
    assert!(
        !app.state()
            .latest_notification()
            .unwrap()
            .contains("occurrences")
    );
}

#[test]
fn replace_all_with_no_match_says_so_and_changes_nothing() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "zzz", "B");

    alt(&mut app, 'a');

    assert_eq!(text(&app), TEXT);
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("nothing to replace")
    );
}

#[test]
fn an_empty_replacement_deletes_the_matches() {
    let mut app = app_with("a-b-c\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "-");
    ctrl(&mut app, 'r');

    alt(&mut app, 'a');

    assert_eq!(text(&app), "abc\n");
}

#[test]
fn with_regex_on_the_groups_can_be_used_in_the_replacement() {
    let mut app = app_with("john smith\nada lovelace\n");
    ctrl(&mut app, 'f');
    alt(&mut app, 'r');
    type_str(&mut app, r"(\w+) (\w+)");
    ctrl(&mut app, 'r');
    type_str(&mut app, "$2, $1");

    alt(&mut app, 'a');

    assert_eq!(text(&app), "smith, john\nlovelace, ada\n");
}

#[test]
fn whole_word_applies_to_replacing_too() {
    let mut app = app_with("cat concat cat\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "cat");
    alt(&mut app, 'w');
    ctrl(&mut app, 'r');
    type_str(&mut app, "dog");

    alt(&mut app, 'a');

    assert_eq!(text(&app), "dog concat dog\n");
}

#[test]
fn the_counts_follow_the_replacing() {
    let mut app = app_with("a a a\n");
    replace_bar(&mut app, "a", "b");

    press(&mut app, KeyCode::Enter);

    assert!(screen(&mut app, 120, 20).contains("1 of 2"));
}

#[test]
fn the_replacement_is_remembered_for_the_next_time() {
    let mut app = app_with(TEXT);
    replace_bar(&mut app, "beta", "BETA");
    press(&mut app, KeyCode::Esc);

    open_replace(&mut app);

    let screen = screen(&mut app, 120, 20);
    assert!(screen.contains("Replace: BETA"), "{screen}");
}

#[test]
fn escape_closes_the_bar_and_gives_both_rows_back() {
    let mut app = app_with(TEXT);
    let _ = screen(&mut app, 120, 20);
    let rows_before = app.state().editor().text_area().unwrap().height;
    open_replace(&mut app);
    let _ = screen(&mut app, 120, 20);
    assert_eq!(
        app.state().editor().text_area().unwrap().height,
        rows_before - 2
    );

    press(&mut app, KeyCode::Esc);
    let _ = screen(&mut app, 120, 20);

    assert_eq!(
        app.state().editor().text_area().unwrap().height,
        rows_before
    );
}

#[test]
fn ctrl_r_inside_the_bar_hides_the_replacement_row_again() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    ctrl(&mut app, 'r');
    assert!(screen(&mut app, 120, 20).contains("Replace:"));

    ctrl(&mut app, 'r');

    let screen = screen(&mut app, 120, 20);
    assert!(!screen.contains("Replace:"), "{screen}");
    assert!(screen.contains("Find:"), "{screen}");
}

#[test]
fn the_palette_has_replace_in_file() {
    use crate::event::action::Action;
    assert!(Action::palette_actions().contains(&Action::ReplaceInFile));
    assert!(Action::ReplaceInFile.title().is_some());
}

#[test]
fn replacing_with_an_emoji_counts_by_graphemes_so_the_next_match_is_found() {
    let mut app = app_with("a a a\n");
    replace_bar(&mut app, "a", "🙂");

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);

    assert_eq!(text(&app), "🙂 🙂 a\n");
}
