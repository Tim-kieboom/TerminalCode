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

fn titles(app: &App) -> Vec<&'static str> {
    app.state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| entry.title)
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
