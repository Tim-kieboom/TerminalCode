use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::action::Action;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(code: KeyCode) -> InputEvent {
    key(code, KeyModifiers::NONE)
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_input(press(KeyCode::Char(c)));
    }
}

fn open(app: &mut App) {
    app.handle_input(press(KeyCode::F(1)));
}

fn titles(app: &App) -> Vec<&str> {
    app.state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| entry.title.as_str())
        .collect()
}

fn tab_count(app: &App) -> usize {
    app.state().workspace().tab_names().0.len()
}

#[test]
fn f1_and_alt_p_open_the_palette() {
    let mut app = App::default();
    open(&mut app);
    assert!(app.state().palette().is_some());

    app.handle_input(press(KeyCode::Esc));
    app.handle_input(key(KeyCode::Char('p'), KeyModifiers::ALT));
    assert!(app.state().palette().is_some());
}

#[test]
fn an_empty_query_lists_every_palette_action() {
    let mut app = App::default();

    open(&mut app);

    assert_eq!(titles(&app).len(), Action::palette_actions().len());
}

#[test]
fn entries_show_the_keys_bound_to_them() {
    let mut app = App::default();

    open(&mut app);

    let entry = |title| {
        app.state()
            .palette()
            .unwrap()
            .visible()
            .find(|entry| entry.title == title)
            .unwrap()
            .keys
            .clone()
    };
    assert_eq!(entry("File: Save").as_deref(), Some("ctrl+s"));
    assert_eq!(entry("Application: Command Palette").as_deref(), Some("f1"));
}

#[test]
fn typing_narrows_the_list_and_ranks_the_best_match_first() {
    let mut app = App::default();
    open(&mut app);

    type_str(&mut app, "focus left");

    assert_eq!(titles(&app), ["Pane: Focus Left"]);
    type_str(&mut app, "x");
    assert!(titles(&app).is_empty());
}

#[test]
fn backspace_and_ctrl_u_widen_the_list_again() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "zzz");
    assert!(titles(&app).is_empty());

    app.handle_input(press(KeyCode::Backspace));
    app.handle_input(press(KeyCode::Backspace));
    app.handle_input(press(KeyCode::Backspace));
    assert_eq!(titles(&app).len(), Action::palette_actions().len());

    type_str(&mut app, "undo");
    app.handle_input(key(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.state().palette().unwrap().query(), "");
}

#[test]
fn enter_runs_the_selected_action_and_closes_the_palette() {
    let mut app = App::default();
    let before = tab_count(&app);
    open(&mut app);
    type_str(&mut app, "file new");

    app.handle_input(press(KeyCode::Enter));

    assert!(app.state().palette().is_none());
    assert_eq!(tab_count(&app), before + 1);
}

#[test]
fn the_arrows_choose_another_match() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "pane split");
    assert_eq!(titles(&app), ["Pane: Split Right", "Pane: Split Down"]);

    app.handle_input(press(KeyCode::Down));
    app.handle_input(press(KeyCode::Down));
    assert_eq!(app.state().palette().unwrap().selected(), 1);
    app.handle_input(press(KeyCode::Enter));

    assert_eq!(app.state().workspace().pane_count(), 2);
}

#[test]
fn escape_closes_without_running_anything() {
    let mut app = App::default();
    let before = tab_count(&app);
    open(&mut app);
    type_str(&mut app, "file new");

    app.handle_input(press(KeyCode::Esc));

    assert!(app.state().palette().is_none());
    assert_eq!(tab_count(&app), before);
}

#[test]
fn enter_with_no_match_keeps_the_palette_open() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "zzz");

    app.handle_input(press(KeyCode::Enter));

    assert!(app.state().palette().is_some());
}

#[test]
fn the_palette_takes_the_keys_instead_of_the_editor() {
    let mut app = App::default();
    open(&mut app);

    type_str(&mut app, "abc");
    app.handle_input(key(KeyCode::Char('q'), KeyModifiers::CONTROL));

    assert_eq!(app.state().editor().buffer().text(), "");
    assert!(!app.should_quit());
}

#[test]
fn quit_from_the_palette_still_asks_about_unsaved_changes() {
    let mut app = App::default();
    type_str(&mut app, "edited");
    open(&mut app);
    type_str(&mut app, "quit");

    app.handle_input(press(KeyCode::Enter));

    assert!(!app.should_quit());
    assert!(app.state().palette().is_none());
    assert!(app.state().quit_prompt().is_some());
}

#[test]
fn every_palette_action_has_a_unique_title() {
    let titles: Vec<_> = Action::palette_actions()
        .iter()
        .map(|action| action.title().expect("palette actions have titles"))
        .collect();

    let mut sorted = titles.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), titles.len());
}

#[test]
fn the_palette_is_drawn_over_the_editor() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "save");
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();

    let screen = terminal.backend().to_string();
    assert!(screen.contains("Command Palette"), "{screen}");
    assert!(screen.contains("> save"), "{screen}");
    assert!(screen.contains("File: Save"), "{screen}");
    assert!(screen.contains("ctrl+s"), "{screen}");
}

// ---- recent actions, plugin views, the mouse and pasting

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use crate::app::state::AppState;
use crate::components::{ComponentKind, PluginViewId};
use crate::ui::layout::LayoutTree;
use crate::ui::view::{ViewLine, ViewNode};

/// Opens the palette, types `query` and runs the first match.
fn run(app: &mut App, query: &str) {
    open(app);
    type_str(app, query);
    app.handle_input(press(KeyCode::Enter));
}

fn first_title(app: &App) -> String {
    app.state()
        .palette()
        .unwrap()
        .visible()
        .next()
        .unwrap()
        .title
        .clone()
}

#[test]
fn an_action_that_was_run_comes_first_next_time() {
    let mut app = App::default();
    run(&mut app, "Toggle Status");

    open(&mut app);

    assert_eq!(first_title(&app), "View: Toggle Status Bar");
}

#[test]
fn the_latest_comes_before_the_one_before() {
    let mut app = App::default();
    run(&mut app, "Toggle Status");
    run(&mut app, "Theme: Light");

    open(&mut app);

    let titles = titles(&app);
    assert_eq!(&titles[..2], ["Theme: Light", "View: Toggle Status Bar"]);
}

#[test]
fn running_the_same_action_again_does_not_list_it_twice() {
    let mut app = App::default();
    run(&mut app, "Toggle Status");
    run(&mut app, "Toggle Status");

    open(&mut app);

    let count = titles(&app)
        .iter()
        .filter(|title| **title == "View: Toggle Status Bar")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn among_equal_matches_the_recent_one_wins() {
    let mut app = App::default();
    run(&mut app, "Tab: Previous");

    open(&mut app);
    type_str(&mut app, "tab:");

    assert_eq!(first_title(&app), "Tab: Previous");
}

#[test]
fn a_better_match_still_beats_a_recent_one() {
    let mut app = App::default();
    run(&mut app, "Theme: Light");

    open(&mut app);
    type_str(&mut app, "quit");

    assert_eq!(first_title(&app), "Application: Quit");
}

#[test]
fn closing_the_palette_without_running_remembers_nothing() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "Theme: Light");
    app.handle_input(press(KeyCode::Esc));

    open(&mut app);

    assert_ne!(first_title(&app), "Theme: Light");
}

#[test]
fn only_the_last_eight_actions_are_remembered() {
    let mut app = App::default();
    let queries = [
        "Theme: Light",
        "Theme: Dark",
        "Toggle Status",
        "Tab: Next",
        "Tab: Previous",
        "Pane: Focus Next",
        "Pane: Split Right",
        "Pane: Split Down",
        "Edit: Undo",
        "Selection: Select All",
    ];
    for query in queries {
        run(&mut app, query);
    }

    open(&mut app);

    let first_eight: Vec<String> = titles(&app).iter().take(8).map(|t| t.to_string()).collect();
    assert!(
        !first_eight.contains(&"Theme: Light".to_owned()),
        "{first_eight:?}"
    );
    assert!(
        first_eight.iter().any(|t| t.contains("Split Down")),
        "{first_eight:?}"
    );
}

fn app_with_plugin_views(ids: &[&str]) -> App {
    let layout = r#"Row([
    Pane(view: Editor),
    Fixed(20, Pane(view: Plugin("b.view"))),
])"#;
    let mut state = AppState::default();
    state.set_layout(LayoutTree::from_ron(layout).unwrap());
    let mut app = App::new(state);
    for id in ids {
        app.state_mut().set_plugin_view(
            PluginViewId::new(*id),
            ViewNode::Lines(vec![ViewLine {
                text: format!("text of {id}").into(),
                slot: "none".into(),
            }]),
        );
    }
    app
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn every_plugin_view_gets_an_entry_to_show_or_hide_it() {
    let mut app = app_with_plugin_views(&["b.view", "a.view"]);

    open(&mut app);
    type_str(&mut app, "view:");

    let titles = titles(&app);
    assert!(titles.contains(&"View: Toggle a.view"), "{titles:?}");
    assert!(titles.contains(&"View: Toggle b.view"), "{titles:?}");
}

#[test]
fn the_plugin_entries_come_after_the_built_in_ones_in_name_order() {
    let mut app = app_with_plugin_views(&["b.view", "a.view"]);

    open(&mut app);

    let titles = titles(&app);
    let a = titles
        .iter()
        .position(|t| *t == "View: Toggle a.view")
        .unwrap();
    let b = titles
        .iter()
        .position(|t| *t == "View: Toggle b.view")
        .unwrap();
    let last_built_in = titles
        .iter()
        .position(|t| *t == "Tab: Reopen Closed")
        .unwrap();
    assert!(a < b);
    assert!(last_built_in < a);
}

#[test]
fn running_a_plugin_entry_hides_the_view() {
    let mut app = app_with_plugin_views(&["b.view"]);
    assert!(draw(&mut app).contains("text of b.view"));

    run(&mut app, "Toggle b.view");

    assert!(!draw(&mut app).contains("text of b.view"));
    assert!(
        app.state()
            .plugin_view(&PluginViewId::new("b.view"))
            .is_some()
    );
}

#[test]
fn a_plugin_entry_shows_a_hidden_view_again() {
    let mut app = app_with_plugin_views(&["b.view"]);
    run(&mut app, "Toggle b.view");

    run(&mut app, "Toggle b.view");

    assert!(draw(&mut app).contains("text of b.view"));
}

#[test]
fn without_plugin_views_there_is_no_such_entry() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "Toggle b.view");

    assert!(titles(&app).is_empty() || !titles(&app).iter().any(|t| t.contains("b.view")));
    assert!(
        !ComponentKind::Plugin(PluginViewId::new("x"))
            .title()
            .is_empty()
    );
}

fn mouse(app: &mut App, kind: MouseEventKind, (column, row): (u16, u16)) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

/// Draws the app and finds the cell where `text` starts.
fn cell_of(app: &mut App, text: &str) -> (u16, u16) {
    cell_of_nth(app, text, 0)
}

/// Like [`cell_of`], for the `skip`-th place on the screen that has `text`,
/// reading from the top.
fn try_cell_of(app: &mut App, text: &str, skip: usize) -> Option<(u16, u16)> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        cell_of_nth(app, text, skip)
    }))
    .ok()
}

fn cell_of_nth(app: &mut App, text: &str, skip: usize) -> (u16, u16) {
    let mut skip = skip;
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    for y in 0..30 {
        let row: String = (0..100)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect();
        if let Some(byte) = row.find(text) {
            if skip == 0 {
                return (row[..byte].chars().count() as u16, y);
            }
            skip -= 1;
        }
    }
    panic!("{text} is not on the screen");
}

#[test]
fn clicking_an_entry_runs_it() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "Theme: Light");
    // The first is the query as typed; the entry is the second.
    let at = cell_of_nth(&mut app, "Theme: Light", 1);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), at);

    assert!(app.state().palette().is_none());
    assert_eq!(
        *app.state().theme(),
        crate::ui::theme::Theme::built_in(crate::ui::theme::ThemeVariant::Light)
    );
}

#[test]
fn a_clicked_entry_is_remembered_too() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "Theme: Light");
    // The first is the query as typed; the entry is the second.
    let at = cell_of_nth(&mut app, "Theme: Light", 1);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), at);

    open(&mut app);

    assert_eq!(first_title(&app), "Theme: Light");
}

#[test]
fn clicking_outside_the_palette_closes_it_and_runs_nothing() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "Theme: Light");
    let _ = draw(&mut app);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), (0, 29));

    assert!(app.state().palette().is_none());
    assert_eq!(*app.state().theme(), crate::ui::theme::Theme::default());
}

#[test]
fn clicking_the_query_line_leaves_the_palette_open() {
    let mut app = App::default();
    open(&mut app);
    let (column, row) = cell_of(&mut app, "> ");

    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        (column + 5, row),
    );

    assert!(app.state().palette().is_some());
}

#[test]
fn the_wheel_moves_the_selection() {
    let mut app = App::default();
    open(&mut app);
    let _ = draw(&mut app);

    mouse(&mut app, MouseEventKind::ScrollDown, (50, 10));
    mouse(&mut app, MouseEventKind::ScrollDown, (50, 10));
    assert_eq!(app.state().palette().unwrap().selected(), 2);

    mouse(&mut app, MouseEventKind::ScrollUp, (50, 10));
    assert_eq!(app.state().palette().unwrap().selected(), 1);

    for _ in 0..5 {
        mouse(&mut app, MouseEventKind::ScrollUp, (50, 10));
    }
    assert_eq!(app.state().palette().unwrap().selected(), 0);
}

#[test]
fn a_click_after_scrolling_picks_the_row_that_is_drawn_there() {
    let mut app = App::default();
    open(&mut app);
    let _ = draw(&mut app);
    for _ in 0..30 {
        mouse(&mut app, MouseEventKind::ScrollDown, (50, 10));
    }
    let selected = app.state().palette().unwrap().selected();
    let title = app
        .state()
        .palette()
        .unwrap()
        .selected_entry()
        .unwrap()
        .title
        .clone();
    // The selected row is the lowest one that has the title (a shorter title can
    // also be the start of another row above it).
    let mut at = cell_of(&mut app, &title);
    let mut skip = 1;
    while let Some(next) = try_cell_of(&mut app, &title, skip) {
        at = next;
        skip += 1;
    }
    assert!(selected >= 12);
    let above = app
        .state()
        .palette()
        .unwrap()
        .visible()
        .nth(selected - 1)
        .unwrap()
        .action
        .clone();

    // The selected row is the last one drawn; the one above it is the one before.
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        (at.0, at.1 - 1),
    );

    // What was run is remembered, whatever it did (it may have reopened the palette).
    assert_eq!(app.state().palette_recent()[0], above);
}

#[test]
fn other_buttons_and_movement_leave_the_palette_alone() {
    let mut app = App::default();
    open(&mut app);
    let _ = draw(&mut app);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Right), (50, 5));
    mouse(&mut app, MouseEventKind::Moved, (50, 6));

    assert!(app.state().palette().is_some());
}

#[test]
fn the_mouse_does_nothing_to_the_palette_when_it_is_not_open() {
    let mut app = App::default();
    let _ = draw(&mut app);

    mouse(&mut app, MouseEventKind::ScrollDown, (50, 10));

    assert!(app.state().palette().is_none());
}

// ---- pasting

#[test]
fn pasted_text_goes_into_the_query() {
    let mut app = App::default();
    open(&mut app);

    app.handle_input(InputEvent::Paste("toggle status".to_owned()));

    assert_eq!(app.state().palette().unwrap().query(), "toggle status");
    assert_eq!(first_title(&app), "View: Toggle Status Bar");
}

#[test]
fn only_the_first_line_of_a_paste_is_used() {
    let mut app = App::default();
    open(&mut app);

    app.handle_input(InputEvent::Paste("theme\r\nsecond line".to_owned()));

    assert_eq!(app.state().palette().unwrap().query(), "theme");
}

#[test]
fn control_characters_in_a_paste_are_dropped() {
    let mut app = App::default();
    open(&mut app);

    app.handle_input(InputEvent::Paste("th\u{7}em\u{1b}e".to_owned()));

    assert_eq!(app.state().palette().unwrap().query(), "theme");
}

#[test]
fn a_huge_paste_is_cut_short() {
    let mut app = App::default();
    open(&mut app);

    app.handle_input(InputEvent::Paste("a".repeat(10_000)));

    assert_eq!(app.state().palette().unwrap().query().len(), 200);
}

#[test]
fn a_paste_adds_to_what_was_typed() {
    let mut app = App::default();
    open(&mut app);
    type_str(&mut app, "tab");

    app.handle_input(InputEvent::Paste(": next".to_owned()));

    assert_eq!(app.state().palette().unwrap().query(), "tab: next");
}

/// An app with a project and an event channel, which the finder and the search
/// need for their background work (the receiver is kept so sending works).
fn app_in_project(
    root: &std::path::Path,
) -> (App, tokio::sync::mpsc::Receiver<crate::event::Event>) {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    (App::new(state).with_events(tx), rx)
}

#[test]
fn a_paste_into_the_find_bar_goes_into_its_query() {
    let mut app = App::default();
    app.handle_input(key(KeyCode::Char('f'), KeyModifiers::CONTROL));

    app.handle_input(InputEvent::Paste("needle".to_owned()));

    assert_eq!(app.state().find().unwrap().query(), "needle");
}

#[test]
fn a_paste_into_the_project_search_goes_into_its_query() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _events) = app_in_project(dir.path());
    app.handle_input(key(KeyCode::Char('f'), KeyModifiers::ALT));

    app.handle_input(InputEvent::Paste("needle".to_owned()));

    assert_eq!(app.state().search().unwrap().query(), "needle");
}

#[test]
fn a_paste_into_the_file_finder_goes_into_its_query() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _events) = app_in_project(dir.path());
    app.handle_input(key(KeyCode::Char('p'), KeyModifiers::CONTROL));

    app.handle_input(InputEvent::Paste("main".to_owned()));

    assert_eq!(app.state().finder().unwrap().query(), "main");
}

#[test]
fn a_paste_while_a_menu_is_open_does_nothing() {
    let mut app = App::default();
    let _ = draw(&mut app);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Right), (60, 5));
    assert!(app.state().menu().is_some());
    let text = app.state().editor().buffer().text();

    app.handle_input(InputEvent::Paste("y\ny".to_owned()));

    assert_eq!(app.state().editor().buffer().text(), text);
    assert!(app.state().menu().is_some());
}

#[test]
fn a_paste_with_no_popup_still_goes_into_the_editor() {
    let mut app = App::default();

    app.handle_input(InputEvent::Paste("pasted".to_owned()));

    assert_eq!(app.state().editor().buffer().text(), "pasted");
}

// ---- go to line

fn goto_app(text: &str) -> App {
    let mut app = App::default();
    app.handle_input(InputEvent::Paste(text.to_owned()));
    app
}

fn head(app: &App) -> (usize, usize) {
    let head = app.state().editor().selections().primary().head();
    (head.line, head.column)
}

fn ctrl_g(app: &mut App) {
    app.handle_input(key(KeyCode::Char('g'), KeyModifiers::CONTROL));
}

const FIVE_LINES: &str = "one\ntwo\nthree three\nfour\nfive";

#[test]
fn ctrl_g_asks_for_a_line_and_enter_goes_there() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);
    assert!(app.state().name_prompt().is_some());

    type_str(&mut app, "3");
    app.handle_input(press(KeyCode::Enter));

    assert!(app.state().name_prompt().is_none());
    assert_eq!(head(&app), (2, 0));
}

#[test]
fn a_column_after_a_colon_is_used() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);

    type_str(&mut app, "3:7");
    app.handle_input(press(KeyCode::Enter));

    assert_eq!(head(&app), (2, 6));
}

#[test]
fn a_line_past_the_end_goes_to_the_last_line_and_a_column_past_the_end_to_its_end() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);
    type_str(&mut app, "99:99");
    app.handle_input(press(KeyCode::Enter));

    assert_eq!(head(&app), (4, 4));
}

#[test]
fn zero_goes_to_the_first_line() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);
    type_str(&mut app, "0");
    app.handle_input(press(KeyCode::Enter));

    assert_eq!(head(&app), (0, 0));
}

#[test]
fn something_that_is_not_a_number_says_so_and_keeps_the_prompt() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);
    type_str(&mut app, "abc");

    app.handle_input(press(KeyCode::Enter));

    assert!(app.state().name_prompt().is_some());
    assert!(app.state().notifications().has_errors());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("abc is not a line number")
    );
}

#[test]
fn a_bad_number_can_be_corrected_in_the_prompt() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);
    type_str(&mut app, "x");
    app.handle_input(press(KeyCode::Enter));

    app.handle_input(press(KeyCode::Backspace));
    type_str(&mut app, "2");
    app.handle_input(press(KeyCode::Enter));

    assert_eq!(head(&app), (1, 0));
}

#[test]
fn escape_leaves_the_cursor_alone() {
    let mut app = goto_app(FIVE_LINES);
    let before = head(&app);
    ctrl_g(&mut app);
    type_str(&mut app, "1");

    app.handle_input(press(KeyCode::Esc));

    assert_eq!(head(&app), before);
    assert!(app.state().name_prompt().is_none());
}

#[test]
fn the_prompt_says_how_many_lines_there_are() {
    let mut app = goto_app(FIVE_LINES);
    ctrl_g(&mut app);

    let screen = draw(&mut app);

    assert!(screen.contains("Go to line (1-5"), "{screen}");
}

#[test]
fn go_to_line_is_in_the_palette() {
    let mut app = goto_app(FIVE_LINES);
    run(&mut app, "Go to Line");

    assert!(app.state().name_prompt().is_some());
}

#[test]
fn going_to_a_line_selects_nothing() {
    let mut app = goto_app(FIVE_LINES);
    app.handle_input(key(KeyCode::Char('a'), KeyModifiers::CONTROL));
    ctrl_g(&mut app);
    type_str(&mut app, "2");
    app.handle_input(press(KeyCode::Enter));

    let selection = *app.state().editor().selections().primary();
    assert!(selection.is_empty());
}
