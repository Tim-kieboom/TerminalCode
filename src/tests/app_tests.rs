use std::io;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::components::PluginViewId;
use crate::error::IdeError;
use crate::event::Event;
use crate::state::AppState;
use crate::ui::view::{ViewLine, ViewNode};

use super::super::app::*;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn ctrl_q() -> InputEvent {
    key(KeyCode::Char('q'), KeyModifiers::CONTROL)
}

fn plugin_text(text: &str) -> Event {
    Event::SetPluginView {
        id: PluginViewId::new("test.view"),
        content: ViewNode::Lines(vec![ViewLine {
            text: text.into(),
            slot: "none".into(),
        }]),
    }
}

fn test_sources() -> (Sources, mpsc::Sender<InputResult>, mpsc::Sender<Event>) {
    let (input_tx, input) = mpsc::channel(8);
    let (events_tx, events) = mpsc::channel(8);
    (Sources { input, events }, input_tx, events_tx)
}

#[test]
fn new_app_wants_a_first_draw() {
    assert!(App::default().needs_redraw());
}

#[test]
fn ctrl_q_quits() {
    let mut app = App::default();

    app.handle_input(ctrl_q());

    assert!(app.should_quit());
}

#[test]
fn plain_q_does_not_quit() {
    let mut app = App::default();

    app.handle_input(key(KeyCode::Char('q'), KeyModifiers::NONE));

    assert!(!app.should_quit());
}

#[test]
fn key_release_does_not_quit() {
    let mut app = App::default();
    let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    release.kind = KeyEventKind::Release;

    app.handle_input(InputEvent::Key(release));

    assert!(!app.should_quit());
}

#[test]
fn resize_requests_a_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    app.handle_input(InputEvent::Resize(80, 24));

    assert!(app.needs_redraw());
}

#[test]
fn plugin_view_event_updates_state_and_requests_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    app.handle_event(plugin_text("hello"));

    let view = app.state().plugin_view(&PluginViewId::new("test.view"));
    assert!(view.is_some());
    assert!(app.needs_redraw());
}

#[tokio::test]
async fn loop_draws_first_frame_then_quits_on_ctrl_q() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    input_tx.send(Ok(ctrl_q())).await.unwrap();

    let result = App::default().run(&mut terminal, sources).await;

    assert!(result.is_ok());
    let screen = terminal.backend().to_string();
    assert!(
        screen.contains("Explorer"),
        "first frame was not drawn:\n{screen}"
    );
}

#[tokio::test]
async fn loop_accepts_events_before_quit() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, events_tx) = test_sources();
    events_tx.send(plugin_text("hi")).await.unwrap();
    input_tx.send(Ok(ctrl_q())).await.unwrap();

    let result = App::default().run(&mut terminal, sources).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn loop_returns_input_errors() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    let err = io::Error::other("boom");
    input_tx.send(Err(err)).await.unwrap();

    let result = App::default().run(&mut terminal, sources).await;

    assert!(matches!(result, Err(IdeError::Io(_))));
}

#[tokio::test]
async fn loop_ends_when_input_closes() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    drop(input_tx);

    let result = App::default().run(&mut terminal, sources).await;

    assert!(result.is_ok());
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(key(code, modifiers));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn buffer_text(app: &App) -> String {
    app.state().editor().buffer().text()
}

#[test]
fn typing_inserts_text_into_the_editor() {
    let mut app = App::default();

    type_str(&mut app, "hi!");

    assert_eq!(buffer_text(&app), "hi!");
}

#[test]
fn shifted_letters_are_typed_as_uppercase() {
    let mut app = App::default();

    press(&mut app, KeyCode::Char('A'), KeyModifiers::SHIFT);

    assert_eq!(buffer_text(&app), "A");
}

#[test]
fn enter_backspace_and_tab_edit_the_text() {
    let mut app = App::default();

    type_str(&mut app, "ab");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);

    assert_eq!(buffer_text(&app), "ab\n   ");
}

#[test]
fn arrows_move_the_cursor_and_shift_arrows_select() {
    let mut app = App::default();
    type_str(&mut app, "abc");

    press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    press(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);

    assert_eq!(buffer_text(&app), "ac");
}

#[test]
fn ctrl_z_and_ctrl_y_undo_and_redo() {
    let mut app = App::default();
    type_str(&mut app, "ab");

    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "");

    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "ab");
}

#[test]
fn unbound_modified_keys_do_nothing() {
    let mut app = App::default();
    app.mark_drawn();

    press(&mut app, KeyCode::Char('l'), KeyModifiers::CONTROL);

    assert_eq!(buffer_text(&app), "");
    assert!(!app.needs_redraw());
}

#[test]
fn handled_keys_request_a_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    type_str(&mut app, "a");

    assert!(app.needs_redraw());
}

#[test]
fn ctrl_s_saves_and_reports_it_in_the_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    let editor =
        crate::components::editor::Editor::new(crate::buffer::Buffer::open_or_new(&path).unwrap());
    let mut app = App::new(AppState::new(editor));
    type_str(&mut app, "saved text");

    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved text");
    assert_eq!(app.state().latest_notification(), Some("saved note.txt"));
}

#[test]
fn saving_a_buffer_without_a_path_reports_an_error_and_keeps_running() {
    let mut app = App::default();
    type_str(&mut app, "x");

    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);

    let status = app.state().latest_notification().unwrap();
    assert!(status.contains("no file path"), "status was {status:?}");
    assert!(!app.should_quit());
}

#[test]
fn the_next_key_dismisses_an_error_and_is_still_handled() {
    let mut app = App::default();
    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(app.state().latest_notification().is_some());

    type_str(&mut app, "a");

    assert_eq!(app.state().latest_notification(), None);
    assert_eq!(buffer_text(&app), "a");
}

#[test]
fn ctrl_a_then_typing_replaces_everything() {
    let mut app = App::default();
    type_str(&mut app, "old text");

    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    type_str(&mut app, "new");

    assert_eq!(buffer_text(&app), "new");
}

#[test]
fn ctrl_arrows_move_by_word_and_ctrl_backspace_deletes_a_word() {
    let mut app = App::default();
    type_str(&mut app, "one two");

    press(&mut app, KeyCode::Left, KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Backspace, KeyModifiers::CONTROL);

    assert_eq!(buffer_text(&app), "two");
}

fn app_with_keymap(source: &str) -> App {
    let keymap = crate::keymap::Keymap::from_toml(source).unwrap();
    App::with_keymap(AppState::default(), keymap)
}

const SEQUENCE_KEYMAP: &str = r#"
    [[binding]]
    keys = "ctrl+k ctrl+i"
    action = { insert_text = "X" }

    [[binding]]
    keys = "ctrl+g"
    action = { insert_text = "G" }

    [[binding]]
    keys = "ctrl+g ctrl+g"
    action = { insert_text = "GG" }
"#;

#[test]
fn a_key_sequence_runs_its_action_after_the_last_chord() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "");
    assert!(app.pending_deadline().is_some());

    press(&mut app, KeyCode::Char('i'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "X");
    assert!(app.pending_deadline().is_none());
}

#[test]
fn a_broken_sequence_does_not_swallow_typed_text() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    type_str(&mut app, "ab");

    assert_eq!(buffer_text(&app), "ab");
    assert!(app.pending_deadline().is_none());
}

#[test]
fn a_pending_sequence_alone_changes_nothing_on_screen() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);
    app.mark_drawn();

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);

    assert!(!app.needs_redraw());
}

#[test]
fn expiring_an_ambiguous_prefix_runs_the_shorter_binding() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);
    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "");

    app.expire_pending();

    assert_eq!(buffer_text(&app), "G");
    assert!(app.pending_deadline().is_none());
}

#[test]
fn completing_an_ambiguous_prefix_runs_the_longer_binding() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);

    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('g'), KeyModifiers::CONTROL);

    assert_eq!(buffer_text(&app), "GG");
}

#[test]
fn expiring_a_pure_prefix_discards_it_silently() {
    let mut app = app_with_keymap(SEQUENCE_KEYMAP);
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);

    app.expire_pending();

    assert_eq!(buffer_text(&app), "");
    assert!(app.pending_deadline().is_none());
}

#[tokio::test]
async fn the_loop_fires_a_pending_sequence_when_its_timeout_passes() {
    let keymap = crate::keymap::Keymap::from_toml(SEQUENCE_KEYMAP).unwrap();
    let app = App::with_keymap(AppState::default(), keymap)
        .with_sequence_timeout(std::time::Duration::from_millis(20));
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();

    tokio::spawn(async move {
        input_tx
            .send(Ok(key(KeyCode::Char('g'), KeyModifiers::CONTROL)))
            .await
            .unwrap();
        // No further key: only the timeout can produce the "G".
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        input_tx.send(Ok(ctrl_q())).await.unwrap();
    });

    let result = app.run(&mut terminal, sources).await;

    assert!(result.is_ok());
    let screen = terminal.backend().to_string();
    assert!(screen.contains("  1 G"), "screen:\n{screen}");
}

fn app_with_clipboard(memory: crate::clipboard::Memory) -> App {
    let clipboard = crate::clipboard::Clipboard::new(crate::clipboard::System::Memory(memory));
    App::default().with_clipboard(clipboard)
}

fn ctrl(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::CONTROL);
}

#[test]
fn ctrl_c_ctrl_v_duplicates_the_selection() {
    let mut app = App::default();
    type_str(&mut app, "abc");
    press(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Left, KeyModifiers::SHIFT);

    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    ctrl(&mut app, 'v');

    assert_eq!(buffer_text(&app), "abcbc");
}

#[test]
fn ctrl_x_with_nothing_selected_cuts_the_line_and_ctrl_v_puts_it_back_above() {
    let mut app = App::default();
    type_str(&mut app, "one");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    type_str(&mut app, "two");

    ctrl(&mut app, 'x');
    assert_eq!(buffer_text(&app), "one");
    ctrl(&mut app, 'v');

    assert_eq!(buffer_text(&app), "two\none");
}

#[test]
fn paste_with_nothing_copied_says_so() {
    let mut app = App::default();

    ctrl(&mut app, 'v');

    assert_eq!(app.state().latest_notification(), Some("nothing to paste"));
    assert_eq!(buffer_text(&app), "");
}

#[test]
fn text_copied_in_another_program_is_pasted() {
    use crate::clipboard::Memory;
    let mut app = app_with_clipboard(Memory {
        content: Some("theirs".to_owned()),
        ..Memory::default()
    });

    ctrl(&mut app, 'v');

    assert_eq!(buffer_text(&app), "theirs");
}

#[test]
fn an_unreadable_system_clipboard_pastes_the_register_and_explains_once() {
    use crate::clipboard::Memory;
    let mut app = app_with_clipboard(Memory {
        readable: false,
        writable: false,
        ..Memory::default()
    });
    type_str(&mut app, "x");
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::End, KeyModifiers::NONE);

    ctrl(&mut app, 'v');
    assert_eq!(buffer_text(&app), "xx");
    let first = app.state().latest_notification().unwrap().to_owned();
    assert!(first.contains("not readable"), "{first}");

    ctrl(&mut app, 'v');
    assert_eq!(buffer_text(&app), "xxx");
    assert_eq!(
        app.state().notifications().len(),
        1,
        "the explanation is only given once"
    );
}

#[test]
fn bracketed_paste_inserts_text_without_touching_the_clipboard() {
    let mut app = App::default();
    type_str(&mut app, "ab");
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::End, KeyModifiers::NONE);

    app.handle_input(InputEvent::Paste("PASTED\r\ntext".to_owned()));

    assert_eq!(buffer_text(&app), "abPASTED\ntext");
    // The register still holds the earlier copy.
    ctrl(&mut app, 'v');
    assert_eq!(buffer_text(&app), "abPASTED\ntextab");
}

#[test]
fn bracketed_paste_requests_a_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    app.handle_input(InputEvent::Paste("x".to_owned()));

    assert!(app.needs_redraw());
}

#[test]
fn one_large_bracketed_paste_is_a_single_undo_step() {
    let mut app = App::default();

    app.handle_input(InputEvent::Paste("line\n".repeat(1000)));
    ctrl(&mut app, 'z');

    assert_eq!(buffer_text(&app), "");
}

#[test]
fn tab_indents_and_shift_tab_outdents() {
    let mut app = App::default();
    type_str(&mut app, "x");

    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(buffer_text(&app), "x   ");

    press(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert_eq!(buffer_text(&app), "x   ");
}

#[test]
fn enter_keeps_the_indentation_while_typing_code() {
    let mut app = App::default();

    type_str(&mut app, "if x:");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    type_str(&mut app, "a");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    type_str(&mut app, "b");

    assert_eq!(buffer_text(&app), "if x:\n    a\n    b");
}

#[test]
fn page_keys_move_the_cursor() {
    let mut app = App::default();
    type_str(&mut app, "a");
    for _ in 0..30 {
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    }

    press(&mut app, KeyCode::PageUp, KeyModifiers::NONE);

    let head = app.state().editor().selections().primary().head();
    assert!(head.line < 30, "cursor did not move: {head:?}");
}

fn mouse(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

/// Draws the whole layout once, so the editor knows where its text is.
fn draw(app: &mut App) -> ratatui::layout::Rect {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
        .unwrap();
    app.mark_drawn();
    app.state().editor().text_area().unwrap()
}

fn left_down(app: &mut App, column: u16, row: u16) {
    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        KeyModifiers::NONE,
    ));
}

fn head_of(app: &App) -> crate::buffer::Position {
    app.state().editor().selections().primary().head()
}

fn selection_of(app: &App) -> (crate::buffer::Position, crate::buffer::Position) {
    let selection = app.state().editor().selections().primary();
    (selection.start(), selection.end())
}

fn app_with_text(text: &str) -> App {
    let editor = crate::components::editor::Editor::new(crate::buffer::Buffer::from_text(text));
    App::new(AppState::new(editor))
}

#[test]
fn clicking_places_the_cursor_and_requests_a_redraw() {
    let mut app = app_with_text("hello\nworld");
    let area = draw(&mut app);

    left_down(&mut app, area.x + 2, area.y + 1);

    assert_eq!(head_of(&app), crate::buffer::Position::new(1, 2));
    assert!(app.needs_redraw());
}

#[test]
fn double_click_selects_a_word_and_triple_click_the_line() {
    let mut app = app_with_text("alpha beta\ngamma");
    let area = draw(&mut app);
    let (x, y) = (area.x + 7, area.y);

    left_down(&mut app, x, y);
    left_down(&mut app, x, y);
    assert_eq!(
        selection_of(&app),
        (
            crate::buffer::Position::new(0, 6),
            crate::buffer::Position::new(0, 10)
        )
    );

    left_down(&mut app, x, y);
    assert_eq!(
        selection_of(&app),
        (
            crate::buffer::Position::new(0, 0),
            crate::buffer::Position::new(1, 0)
        )
    );
}

#[test]
fn dragging_selects_and_typing_replaces_the_selection() {
    let mut app = app_with_text("hello world");
    let area = draw(&mut app);

    left_down(&mut app, area.x, area.y);
    app.handle_input(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        area.x + 5,
        area.y,
        KeyModifiers::NONE,
    ));
    app.handle_input(mouse(
        MouseEventKind::Up(MouseButton::Left),
        area.x + 5,
        area.y,
        KeyModifiers::NONE,
    ));
    type_str(&mut app, "bye");

    assert_eq!(buffer_text(&app), "bye world");
}

#[test]
fn shift_click_extends_the_selection() {
    let mut app = app_with_text("abcdef");
    let area = draw(&mut app);
    left_down(&mut app, area.x + 1, area.y);

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        area.x + 4,
        area.y,
        KeyModifiers::SHIFT,
    ));

    assert_eq!(
        selection_of(&app),
        (
            crate::buffer::Position::new(0, 1),
            crate::buffer::Position::new(0, 4)
        )
    );
}

#[test]
fn the_wheel_scrolls_without_moving_the_cursor_and_survives_the_next_frame() {
    let text = (0..200)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = app_with_text(&text);
    let area = draw(&mut app);

    app.handle_input(mouse(
        MouseEventKind::ScrollDown,
        area.x,
        area.y,
        KeyModifiers::NONE,
    ));
    app.handle_input(mouse(
        MouseEventKind::ScrollDown,
        area.x,
        area.y,
        KeyModifiers::NONE,
    ));
    draw(&mut app);

    assert_eq!(app.state().editor().scroll().top, 6);
    assert_eq!(head_of(&app), crate::buffer::Position::new(0, 0));
}

#[test]
fn shift_wheel_scrolls_sideways() {
    let mut app = app_with_text(&"x".repeat(300));
    let area = draw(&mut app);

    app.handle_input(mouse(
        MouseEventKind::ScrollDown,
        area.x,
        area.y,
        KeyModifiers::SHIFT,
    ));

    assert_eq!(app.state().editor().scroll().left, 6);
    assert_eq!(app.state().editor().scroll().top, 0);
}

#[test]
fn mouse_movement_without_a_button_does_not_redraw() {
    let mut app = app_with_text("hello");
    let area = draw(&mut app);

    app.handle_input(mouse(
        MouseEventKind::Moved,
        area.x,
        area.y,
        KeyModifiers::NONE,
    ));

    assert!(!app.needs_redraw());
}

#[test]
fn other_buttons_are_ignored() {
    let mut app = app_with_text("hello");
    let area = draw(&mut app);

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Right),
        area.x + 3,
        area.y,
        KeyModifiers::NONE,
    ));

    assert_eq!(head_of(&app), crate::buffer::Position::new(0, 0));
}

#[test]
fn alt_m_toggles_mouse_capture_and_tells_the_terminal_once() {
    let mut app = app_with_text("hello");
    assert_eq!(app.take_mouse_change(), None);

    press(&mut app, KeyCode::Char('m'), KeyModifiers::ALT);
    assert_eq!(app.take_mouse_change(), Some(false));
    assert_eq!(app.take_mouse_change(), None);
    assert_eq!(app.state().latest_notification(), Some("mouse off"));

    press(&mut app, KeyCode::Char('m'), KeyModifiers::ALT);
    assert_eq!(app.take_mouse_change(), Some(true));
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("mouse on")
    );
}

#[test]
fn with_the_mouse_off_clicks_do_nothing() {
    let mut app = app_with_text("hello");
    let area = draw(&mut app);
    press(&mut app, KeyCode::Char('m'), KeyModifiers::ALT);
    app.mark_drawn();

    left_down(&mut app, area.x + 3, area.y);

    assert_eq!(head_of(&app), crate::buffer::Position::new(0, 0));
    assert!(!app.needs_redraw());
}

fn tab_names(app: &App) -> Vec<String> {
    app.state().workspace().tab_names().0
}

#[test]
fn ctrl_n_opens_an_empty_tab_and_ctrl_page_keys_switch_tabs() {
    let mut app = App::default();
    type_str(&mut app, "first");

    ctrl(&mut app, 'n');
    assert_eq!(tab_names(&app).len(), 2);
    assert_eq!(buffer_text(&app), "");
    type_str(&mut app, "second");

    press(&mut app, KeyCode::PageUp, KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "first");
    press(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "second");
}

#[test]
fn ctrl_w_closes_a_tab_and_warns_before_discarding_changes() {
    let mut app = App::default();
    type_str(&mut app, "one");
    ctrl(&mut app, 'n');
    type_str(&mut app, "two");

    ctrl(&mut app, 'w');
    let status = app.state().latest_notification().unwrap().to_owned();
    assert!(status.contains("unsaved changes"), "{status}");
    assert_eq!(tab_names(&app).len(), 2);

    ctrl(&mut app, 'w');
    assert_eq!(tab_names(&app).len(), 1);
    assert_eq!(buffer_text(&app), "one");
}

#[test]
fn ctrl_k_right_splits_the_pane_and_the_new_pane_shares_the_document() {
    let mut app = App::default();
    type_str(&mut app, "shared");

    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);

    assert_eq!(app.state().workspace().pane_count(), 2);
    assert_eq!(app.state().workspace().document_count(), 1);
    assert_eq!(buffer_text(&app), "shared");
}

#[test]
fn ctrl_k_down_splits_below() {
    let mut app = App::default();

    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);

    assert_eq!(app.state().workspace().pane_count(), 2);
}

#[test]
fn typing_in_one_pane_shows_in_the_other_and_alt_hjkl_moves_focus() {
    let mut app = App::default();
    type_str(&mut app, "ab");
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    draw(&mut app);
    type_str(&mut app, "cd");

    press(&mut app, KeyCode::Char('h'), KeyModifiers::ALT);

    assert_eq!(buffer_text(&app), "abcd");
    // The cursor of the pane we came from was at the end; the left pane's was
    // at the end of "ab" and followed the insertion point rule.
    let head = app.state().editor().selections().primary().head();
    assert_eq!(head.column, 2);
}

#[test]
fn ctrl_k_o_cycles_through_panes() {
    let mut app = App::default();
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    draw(&mut app);
    let focused = app.state().workspace().focused_area();

    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);

    assert_ne!(app.state().workspace().focused_area(), focused);
}

#[test]
fn undo_after_typing_in_a_split_undoes_the_shared_history() {
    let mut app = App::default();
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    type_str(&mut app, "hello");

    ctrl(&mut app, 'z');

    assert_eq!(buffer_text(&app), "");
}

#[test]
fn closing_the_split_pane_returns_to_one_pane() {
    let mut app = App::default();
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);

    ctrl(&mut app, 'w');

    assert_eq!(app.state().workspace().pane_count(), 1);
}

#[test]
fn middle_clicking_a_tab_closes_it() {
    let mut app = App::default();
    type_str(&mut app, "first");
    ctrl(&mut app, 's');
    ctrl(&mut app, 'n');
    draw(&mut app);
    let first_tab = app.state().workspace().tab_areas()[0];

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Middle),
        first_tab.x + 1,
        first_tab.y,
        KeyModifiers::NONE,
    ));

    // "first" was modified (and saving a nameless buffer failed), so the
    // first middle click only warns.
    assert_eq!(tab_names(&app).len(), 2);
    let status = app.state().latest_notification().unwrap().to_owned();
    assert!(status.contains("unsaved changes"), "{status}");

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Middle),
        first_tab.x + 1,
        first_tab.y,
        KeyModifiers::NONE,
    ));
    assert_eq!(tab_names(&app).len(), 1);
}

#[test]
fn middle_clicking_a_clean_tab_closes_it_at_once() {
    let mut app = App::default();
    ctrl(&mut app, 'n');
    draw(&mut app);
    let first_tab = app.state().workspace().tab_areas()[0];

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Middle),
        first_tab.x + 1,
        first_tab.y,
        KeyModifiers::NONE,
    ));

    assert_eq!(tab_names(&app).len(), 1);
    assert!(app.needs_redraw());
}

#[test]
fn middle_click_outside_the_tab_bar_does_nothing() {
    let mut app = app_with_text("hello");
    let area = draw(&mut app);
    app.mark_drawn();

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Middle),
        area.x + 2,
        area.y,
        KeyModifiers::NONE,
    ));

    assert_eq!(tab_names(&app).len(), 1);
    assert_eq!(buffer_text(&app), "hello");
}

fn app_without_tabs() -> App {
    let mut app = App::default();
    ctrl(&mut app, 'w');
    assert!(!app.state().workspace().has_tabs());
    app
}

#[test]
fn closing_the_last_tab_leaves_the_app_running_with_an_empty_editor() {
    let app = app_without_tabs();

    assert!(!app.should_quit());
    assert_eq!(tab_names(&app).len(), 0);
}

#[test]
fn typing_with_no_open_file_explains_what_to_do() {
    let mut app = app_without_tabs();

    type_str(&mut app, "x");

    let status = app.state().latest_notification().unwrap().to_owned();
    assert!(status.contains("no open file"), "{status}");
    assert!(status.contains("ctrl+n"), "{status}");
    assert!(!app.state().workspace().has_tabs());
}

#[test]
fn editing_actions_do_nothing_without_a_tab_but_do_not_crash() {
    let mut app = app_without_tabs();

    ctrl(&mut app, 'z');
    ctrl(&mut app, 's');
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'v');
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut app, KeyCode::PageDown, KeyModifiers::NONE);

    assert!(!app.should_quit());
    assert!(!app.state().workspace().has_tabs());
}

#[test]
fn bracketed_paste_with_no_open_file_explains_too() {
    let mut app = app_without_tabs();

    app.handle_input(InputEvent::Paste("text".to_owned()));

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("no open file")
    );
}

#[test]
fn ctrl_n_brings_the_editor_back() {
    let mut app = app_without_tabs();

    ctrl(&mut app, 'n');
    type_str(&mut app, "again");

    assert_eq!(buffer_text(&app), "again");
}

#[test]
fn quit_and_pane_commands_still_work_without_a_tab() {
    let mut app = app_without_tabs();
    press(&mut app, KeyCode::PageDown, KeyModifiers::CONTROL);
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(app.state().workspace().pane_count(), 1);

    ctrl(&mut app, 'q');

    assert!(app.should_quit());
}

#[test]
fn the_status_bar_and_editor_area_show_the_empty_state() {
    let mut app = app_without_tabs();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
        .unwrap();

    let screen = terminal.backend().to_string();
    assert!(screen.contains("No open files"), "{screen}");
    assert!(screen.contains("no open files"), "{screen}");
}
