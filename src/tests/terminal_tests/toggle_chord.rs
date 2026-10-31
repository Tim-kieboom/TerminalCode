use super::*;

#[test]
fn the_chord_that_opened_the_terminal_closes_it_and_the_editor_has_the_keyboard() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    assert_eq!(rig.app.state().focus(), Focus::Terminal);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.app.state().is_visible(&ComponentKind::Terminal));
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn the_toggle_chord_does_not_reach_the_shell() {
    let (mut rig, _dir) = rig("cat");
    rig.toggle();
    rig.type_text("ab");
    rig.wait_for_text("ab");

    rig.toggle();
    rig.toggle();

    // Had the chord gone to the shell it would have been echoed or have changed
    // the line; the shell still shows exactly what was typed before.
    assert!(rig.screen().contains("ab"), "{}", rig.screen());
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn from_the_editor_the_chord_gives_a_shown_pane_the_keyboard_without_hiding_it() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert!(rig.app.state().is_visible(&ComponentKind::Terminal));
}

#[test]
fn a_rebound_toggle_chord_closes_the_pane_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml("[[binding]]\nkeys = \"f11\"\naction = \"toggle_terminal\"\n")
        .unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_keymap(keymap)
        .with_events(tx)
        .with_terminal_shell(Shell::script("sleep 30"));
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
    };
    rig.draw();

    rig.press(KeyCode::F(11), KeyModifiers::NONE);
    rig.draw();
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    rig.press(KeyCode::F(11), KeyModifiers::NONE);

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.app.state().is_visible(&ComponentKind::Terminal));
}

#[test]
fn ctrl_b_c_opens_a_second_tab_with_its_own_shell_and_a_tab_bar() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    assert!(
        rig.screen().contains(" 1: sh "),
        "the tab bar shows with one tab"
    );

    rig.prefixed('c');

    assert_eq!(rig.terminal_tabs(), 2);
    assert_eq!(rig.active_tab(), 1);
    assert_ne!(rig.pid_of(0), rig.pid_of(1));
    assert!(rig.screen().contains(" 1: sh "), "{}", rig.screen());
    assert!(rig.screen().contains(" 2: sh "), "{}", rig.screen());
}

#[test]
fn every_tab_has_the_size_of_the_text_area_with_the_tab_bar_taking_a_row() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    let (rows_alone, columns) = rig.app.state().terminal().size().unwrap();

    rig.prefixed('c');
    rig.draw();

    let (rows, _) = rig.app.state().terminal().size().unwrap();
    assert_eq!(
        rows, rows_alone,
        "the bar was there already, so nothing resizes"
    );
    for tab in 0..2 {
        let size = rig
            .app
            .state()
            .terminal()
            .session_of(tab)
            .unwrap()
            .with_screen(|screen| screen.size());
        assert_eq!(size, (rows, columns), "tab {}", tab + 1);
    }
}

#[test]
fn each_tab_keeps_its_own_screen() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.wait_for_text("tab-");
    rig.prefixed('c');
    rig.wait_for("the second shell", |rig| {
        rig.app
            .state()
            .terminal()
            .session_of(1)
            .unwrap()
            .with_screen(|screen| screen.contents().contains("tab-"))
    });
    let text_of = |rig: &Rig, tab: usize| {
        rig.app
            .state()
            .terminal()
            .session_of(tab)
            .unwrap()
            .with_screen(|screen| screen.contents())
    };

    assert_ne!(
        text_of(&rig, 0),
        text_of(&rig, 1),
        "different shells, different pids"
    );
    rig.prefixed('1');
    rig.draw();
    let shown = rig.screen();
    assert!(shown.contains(text_of(&rig, 0).trim()), "{shown}");
}

#[test]
fn ctrl_b_n_and_p_go_between_tabs_and_digits_pick_one() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    rig.prefixed('c');
    assert_eq!(rig.active_tab(), 2);

    rig.prefixed('n');
    assert_eq!(rig.active_tab(), 0);
    rig.prefixed('p');
    assert_eq!(rig.active_tab(), 2);
    rig.prefixed('2');
    assert_eq!(rig.active_tab(), 1);
    rig.prefixed('9');
    assert_eq!(rig.active_tab(), 1, "there is no ninth tab");
}

#[test]
fn ctrl_b_w_closes_the_tab_and_stops_its_shell() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    let second = rig.pid_of(1);

    rig.prefixed('w');

    assert_eq!(rig.terminal_tabs(), 1);
    assert_eq!(rig.active_tab(), 0);
    assert_ne!(rig.pid_of(0), second);
    assert!(
        rig.screen().contains(" 1: sh "),
        "the bar stays with one tab"
    );
    assert!(!rig.screen().contains(" 2: sh "));
}

#[test]
fn closing_the_last_tab_hides_the_pane_and_returns_the_keyboard() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();

    rig.prefixed('w');

    assert_eq!(rig.terminal_tabs(), 0);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn a_tab_that_ended_is_marked_and_stays_until_it_is_closed() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });
    rig.prefixed('c');

    rig.wait_for("two ended tabs", |rig| {
        rig.screen().matches("(ended)").count() == 2
    });
    assert_eq!(rig.terminal_tabs(), 2);
}

#[test]
fn showing_the_pane_again_after_every_shell_ended_starts_over_with_one_tab() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });
    rig.prefixed('c');
    rig.wait_for("both to end", |rig| {
        !rig.app.state().terminal().any_running()
    });

    rig.toggle();
    rig.toggle();

    assert_eq!(rig.terminal_tabs(), 1);
}

#[test]
fn at_most_sixteen_tabs() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    for _ in 0..20 {
        rig.prefixed('c');
    }

    assert_eq!(rig.terminal_tabs(), 16);
    assert!(
        rig.notifications().contains("at most 16"),
        "{}",
        rig.notifications()
    );
}

#[test]
fn clicking_a_tab_picks_it_and_a_middle_click_closes_it() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(" 1: sh "))
        .unwrap();
    let first = line.find(" 1: sh ").unwrap() as u16;
    let click = |kind, column: u16| {
        InputEvent::Mouse(MouseEvent {
            kind,
            column: column - 1, // the text form of the backend starts with a quote
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app
        .handle_input(click(MouseEventKind::Down(MouseButton::Left), first + 2));
    assert_eq!(rig.active_tab(), 0);

    rig.app
        .handle_input(click(MouseEventKind::Down(MouseButton::Middle), first + 2));
    assert_eq!(rig.terminal_tabs(), 1);
}

#[test]
fn the_palette_has_the_tab_commands() {
    use crate::event::action::Action;

    for (action, title) in [
        (Action::TerminalNewTab, "Terminal: New Tab"),
        (Action::TerminalNextTab, "Terminal: Next Tab"),
        (Action::TerminalPreviousTab, "Terminal: Previous Tab"),
        (Action::TerminalCloseTab, "Terminal: Close Tab"),
    ] {
        assert!(Action::palette_actions().contains(&action));
        assert_eq!(action.title(), Some(title));
    }
}

#[test]
fn a_program_that_asked_for_the_mouse_gets_clicks_with_pane_relative_positions() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 4,
        y + 2,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Up(MouseButton::Left),
        x + 4,
        y + 2,
        KeyModifiers::NONE,
    ));

    rig.wait_for_text("^[[<0;5;3M^[[<0;5;3m");
}

#[test]
fn the_wheel_goes_to_such_a_program_instead_of_scrolling_the_history() {
    use crossterm::event::MouseEventKind;
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app
        .handle_input(mouse(MouseEventKind::ScrollUp, x, y, KeyModifiers::NONE));

    rig.wait_for_text("^[[<64;1;1M");
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn shift_keeps_the_mouse_for_the_editor() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y + 1,
        KeyModifiers::SHIFT,
    ));
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(!rig.screen().contains("^[[<"), "{}", rig.screen());
}

#[test]
fn a_program_that_did_not_ask_gets_no_mouse_reports() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_showing_bytes();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y + 1,
        KeyModifiers::NONE,
    ));
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(!rig.screen().contains("^[[<"), "{}", rig.screen());
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn a_drag_inside_the_pane_does_not_select_text_in_the_editor() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_showing_bytes();
    let (x, y) = rig.text_origin();
    rig.app
        .state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("hello").unwrap());
    let before = *rig.app.state().editor().selections().primary();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 3,
        y,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        x + 9,
        y + 2,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Up(MouseButton::Left),
        x + 9,
        y + 2,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.app.state().editor().selections().primary(), &before);
}

#[test]
fn the_plus_button_sits_right_after_the_last_tab_even_with_a_single_tab() {
    let (mut rig, _dir) = rig("sleep 30");
    assert!(!rig.screen().contains(" + "), "hidden pane, no button");

    rig.toggle();

    plus_button(&rig);
    assert!(rig.screen().contains(" 1: sh  + "), "{}", rig.screen());
    rig.prefixed('c');
    assert!(rig.screen().contains(" 2: sh  + "), "{}", rig.screen());
}

#[test]
fn clicking_the_plus_button_opens_a_new_tab_and_gives_it_the_keyboard() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('e');
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 2);
    assert_eq!(rig.active_tab(), 1);
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn the_plus_button_keeps_working_with_several_tabs() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    rig.draw();
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 3);
}

#[test]
fn only_a_click_on_the_plus_button_opens_a_tab() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Right),
        x,
        y,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x - 4,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 1);
}
