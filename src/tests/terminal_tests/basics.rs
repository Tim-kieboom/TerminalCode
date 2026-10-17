use super::*;

#[test]
fn the_terminal_is_hidden_until_it_is_asked_for() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");

    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
    assert!(!rig.app.state().terminal().is_running());

    rig.toggle();

    assert!(rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn showing_it_the_first_time_starts_a_shell_whose_output_is_drawn() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");

    rig.toggle();

    rig.wait_for_text("hello");
    assert!(rig.app.state().terminal().is_running());
}

#[test]
fn toggling_again_hides_the_pane_and_keeps_the_same_shell() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");
    rig.toggle();
    rig.wait_for_text("hello");
    let pid = rig.app.state().terminal().session().unwrap().process_id();

    rig.toggle();
    assert!(!rig.screen().contains("hello"), "{}", rig.screen());
    rig.toggle();

    assert!(rig.screen().contains("hello"));
    assert_eq!(
        rig.app.state().terminal().session().unwrap().process_id(),
        pid
    );
}

// Prints the directory with `pwd`; Git's sh on Windows writes it as /c/Users/..., so Windows has its own test with cmd.exe (windows_tests.rs).
#[cfg(unix)]
#[test]
fn the_shell_starts_in_the_project_root() {
    let (mut rig, dir) = rig("pwd; sleep 30");

    rig.toggle();

    let root = dir.path().canonicalize().unwrap();
    rig.wait_for_text(root.to_str().unwrap());
}

#[test]
fn colors_reach_the_screen() {
    let (mut rig, _dir) = rig("printf '\\033[31mred\\033[0m'; sleep 30");
    rig.toggle();
    rig.wait_for_text("red");

    let buffer = rig.terminal.backend().buffer().clone();
    let cell = buffer
        .content
        .iter()
        .find(|cell| cell.symbol() == "r" && cell.fg == Color::Indexed(1));

    assert!(cell.is_some(), "no red `r` on screen:\n{}", rig.screen());
}

#[test]
fn the_shell_has_the_size_of_the_pane() {
    let (mut rig, _dir) = rig("stty size; sleep 30");

    rig.toggle();

    let (rows, columns) = rig.app.state().terminal().size().unwrap();
    assert!(rows > 3 && columns > 20, "{rows}x{columns}");
    rig.wait_for_text(&format!("{rows} {columns}"));
}

#[test]
fn resizing_the_window_resizes_the_shell() {
    let (mut rig, _dir) = rig("trap 'stty size' WINCH; stty size; while true; do sleep 0.1; done");
    rig.toggle();
    let (rows, columns) = rig.app.state().terminal().size().unwrap();
    rig.wait_for_text(&format!("{rows} {columns}"));

    rig.terminal.backend_mut().resize(80, 30);
    rig.terminal
        .resize(ratatui::layout::Rect::new(0, 0, 80, 30))
        .unwrap();
    rig.draw();

    let (_, narrower) = rig.app.state().terminal().size().unwrap();
    assert!(narrower < columns);
    rig.wait_for_text(&format!("{rows} {narrower}"));
}

#[test]
fn a_hidden_pane_keeps_the_size_its_shell_last_had() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    let size = rig.app.state().terminal().size();

    rig.toggle();
    rig.draw();

    assert_eq!(rig.app.state().terminal().size(), size);
}

#[test]
fn a_shell_that_ends_is_reported_once_and_toggling_starts_another() {
    let (mut rig, _dir) = rig("exit 3");
    rig.toggle();

    rig.wait_for("the notification", |rig| {
        rig.notifications().contains("code 3")
    });
    assert!(!rig.app.state().terminal().is_running());

    // Hide, show: a new shell, which ends again.
    rig.toggle();
    rig.toggle();
    rig.wait_for("a second notification", |rig| {
        rig.notifications().contains("code 3") && rig.app.state().terminal().session().is_some()
    });
}

#[test]
fn without_an_event_loop_there_is_no_shell_and_a_message_says_so() {
    let mut app = App::new(AppState::default());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('`'),
        KeyModifiers::CONTROL,
    )));
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();

    assert!(!app.state().terminal().is_running());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("event loop")
    );
}

#[test]
fn the_palette_offers_to_toggle_the_terminal() {
    use crate::event::action::Action;

    assert!(Action::palette_actions().contains(&Action::ToggleTerminal));
    assert_eq!(
        Action::ToggleTerminal.title(),
        Some("View: Toggle Terminal")
    );
}

#[test]
fn the_editor_gets_the_space_back_while_the_terminal_is_hidden() {
    let (mut rig, _dir) = rig("sleep 30");
    let tall = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();

    rig.toggle();
    let short = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();
    rig.toggle();
    let tall_again = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();

    assert!(short < tall, "{short} vs {tall}");
    assert_eq!(tall, tall_again);
}

#[test]
fn showing_the_terminal_gives_it_the_keyboard() {
    let (mut rig, _dir) = rig("sleep 30");

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn toggling_from_the_editor_with_the_pane_already_shown_moves_the_keyboard_into_it() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert!(rig.screen().contains("Terminal"), "still shown");
}

#[test]
fn typed_keys_reach_the_shell() {
    let (mut rig, _dir) = rig("cat");
    rig.toggle();

    rig.type_text("hello there");
    rig.press(KeyCode::Enter, KeyModifiers::NONE);

    rig.wait_for("the echo", |rig| {
        rig.screen().matches("hello there").count() >= 2
    });
}

#[test]
fn special_keys_are_sent_as_the_bytes_a_terminal_sends() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Up, KeyModifiers::NONE);
    rig.press(KeyCode::Char('c'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Enter, KeyModifiers::NONE);
    rig.press(KeyCode::Delete, KeyModifiers::NONE);

    rig.wait_for_text("^[[A^C^M^[[3~");
}

#[test]
fn keys_the_editor_binds_go_to_the_shell_while_the_terminal_has_the_keyboard() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('q'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('p'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('s'), KeyModifiers::CONTROL);

    rig.wait_for_text("^Q^P^S");
    assert!(!rig.app.should_quit());
}

#[test]
fn ctrl_b_twice_sends_a_literal_ctrl_b() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);

    rig.wait_for_text("^B");
}

#[test]
fn ctrl_b_alone_sends_nothing() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    std::thread::sleep(Duration::from_millis(300));
    rig.wait_for("a redraw", |_| true);

    assert!(!rig.screen().contains("^B"), "{}", rig.screen());
}

#[test]
fn behind_the_prefix_e_goes_to_the_editor_and_x_to_the_explorer() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Explorer);

    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(rig.app.state().terminal().is_running(), "the shell goes on");
}

#[test]
fn behind_the_prefix_t_hides_the_pane_and_returns_the_keyboard_to_the_editor() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn behind_the_prefix_a_binding_of_the_keymap_works() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('p'), KeyModifiers::CONTROL);

    assert!(rig.app.state().finder().is_some(), "the file finder opened");
}

#[test]
fn behind_the_prefix_a_binding_with_more_keys_goes_on_through_the_keymap() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    // `ctrl+k b` hides and shows the explorer; ctrl+k alone is half a binding.
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('k'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('b'), KeyModifiers::NONE);
    rig.draw();

    assert!(!rig.screen().contains("Explorer"), "{}", rig.screen());
}

#[test]
fn an_unbound_key_behind_the_prefix_cancels_it_and_says_so() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('z'), KeyModifiers::NONE);
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(
        rig.notifications().contains("ctrl+b z is not bound"),
        "{}",
        rig.notifications()
    );
    assert!(
        !rig.screen().contains("zok"),
        "the z did not reach the shell"
    );
}

#[test]
fn escape_behind_the_prefix_cancels_it_quietly() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Esc, KeyModifiers::NONE);
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert_eq!(rig.notifications(), "");
    assert!(!rig.screen().contains("^["), "the escape was not sent");
}

#[test]
fn the_prefix_gives_up_silently_after_a_second() {
    let (mut rig, _dir) = rig_showing_bytes();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let deadline = rig.app.pending_deadline().expect("the prefix is waiting");

    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now()) + Duration::from_millis(20),
    );
    rig.app.expire_pending();
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert_eq!(rig.notifications(), "");
    assert_eq!(rig.app.pending_deadline(), None);
}

#[test]
fn the_status_bar_shows_that_the_prefix_is_waiting() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.draw();
    assert!(rig.screen().contains("ctrl+b ..."), "{}", rig.screen());

    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    rig.draw();
    assert!(!rig.screen().contains("ctrl+b ..."), "{}", rig.screen());
}

#[test]
fn a_plain_paste_goes_to_the_shell_with_carriage_returns() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.app.handle_input(InputEvent::Paste("a\nb".to_owned()));

    rig.wait_for_text("a^Mb");
    assert_eq!(
        rig.app.state().editor().buffer().text(),
        "",
        "not into the editor"
    );
}

#[test]
fn a_program_that_asked_for_bracketed_paste_gets_one() {
    let (mut rig, _dir) = rig("printf '\\033[?2004h'; stty raw -echo; cat -v");
    rig.toggle();
    rig.wait_for("the mode", |rig| {
        rig.app
            .state()
            .terminal()
            .session()
            .is_some_and(|session| session.with_screen(|screen| screen.bracketed_paste()))
    });
    std::thread::sleep(Duration::from_millis(200));

    rig.app.handle_input(InputEvent::Paste("a\nb".to_owned()));

    rig.wait_for_text("^[[200~a");
    rig.wait_for_text("^[[201~");
}

// A Unix terminal in raw mode stops reading, which fills its input; ConPTY takes input into a console buffer that does not fill the same way, so this cannot be set up there.
#[cfg(unix)]
#[test]
fn a_paste_into_a_shell_that_is_not_reading_reports_how_much_went_through() {
    let (mut rig, _dir) = rig("stty raw -echo; sleep 30");
    rig.toggle();
    rig.wait_for("the shell", |rig| rig.app.state().terminal().is_running());
    std::thread::sleep(Duration::from_millis(300));

    let start = Instant::now();
    rig.app
        .handle_input(InputEvent::Paste("p".repeat(8 * 1024 * 1024)));

    assert!(
        start.elapsed() < Duration::from_millis(500),
        "{:?}",
        start.elapsed()
    );
    assert!(
        rig.notifications().contains("not reading"),
        "{}",
        rig.notifications()
    );
}

#[test]
fn clicking_the_pane_gives_it_the_keyboard_and_clicking_the_editor_takes_it_back() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig("printf hello; sleep 30");
    rig.toggle();
    rig.wait_for_text("hello");
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("hello"))
        .unwrap();
    let column = line.chars().position(|c| c == 'h').unwrap() as u16;
    let click = |column: u16, row: u16| {
        InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app.handle_input(click(column, row as u16));
    assert_eq!(rig.app.state().focus(), Focus::Terminal);

    rig.app.handle_input(click(60, 2));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn the_cursor_is_where_the_shells_is_while_the_pane_has_the_keyboard() {
    let (mut rig, _dir) = rig("printf abc; sleep 30");
    rig.toggle();
    rig.wait_for_text("abc");
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("abc"))
        .unwrap();
    // The backend's text form wraps every line in a quote.
    let start = line.chars().position(|c| c == 'a').unwrap() as u16 - 1;

    let cursor = rig.terminal.get_cursor_position().unwrap();

    assert_eq!((cursor.x, cursor.y), (start + 3, row as u16));
}

#[test]
fn typing_after_the_shell_ended_says_so_instead_of_failing_silently() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });

    rig.type_text("x");

    assert!(
        rig.notifications().contains("has ended"),
        "{}",
        rig.notifications()
    );
}

#[test]
fn shift_page_up_scrolls_into_the_history_and_says_how_far() {
    let (mut rig, _dir) = rig_with_history();
    assert!(!rig.screen().contains('↑'));

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    rig.draw();

    let (rows, _) = rig.app.state().terminal().size().unwrap();
    assert!(
        rig.screen().contains(&format!("↑{}", rows - 1)),
        "{}",
        rig.screen()
    );
    assert!(!rig.screen().contains("100"), "the last line scrolled off");
}

#[test]
fn shift_page_down_comes_back_and_the_indicator_goes() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);

    rig.press(KeyCode::PageDown, KeyModifiers::SHIFT);
    rig.draw();

    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
    assert!(rig.screen().contains("100"));
}

#[test]
fn shift_home_and_shift_end_jump_to_the_ends() {
    let (mut rig, _dir) = rig_with_history();

    rig.press(KeyCode::Home, KeyModifiers::SHIFT);
    rig.draw();
    assert!(rig.screen().contains('↑'), "{}", rig.screen());
    assert!(
        rig.screen().contains("\n") && rig.screen().contains(" 1 "),
        "{}",
        rig.screen()
    );

    rig.press(KeyCode::End, KeyModifiers::SHIFT);
    rig.draw();
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn the_scroll_keys_are_not_sent_to_the_shell() {
    let (mut rig, _dir) = rig_with_history();

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    rig.press(KeyCode::Char('c'), KeyModifiers::CONTROL);
    rig.wait_for_text("^C");

    assert!(!rig.screen().contains("^[[5;2~"));
}

#[test]
fn typing_brings_the_view_back_to_the_present() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::Home, KeyModifiers::SHIFT);

    rig.press(KeyCode::Char('x'), KeyModifiers::CONTROL);

    rig.wait_for_text("^X");
    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn a_paste_brings_the_view_back_too() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::Home, KeyModifiers::SHIFT);

    rig.app.handle_input(InputEvent::Paste("pasted".to_owned()));

    rig.wait_for_text("pasted");
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn the_wheel_over_the_pane_scrolls_the_history_three_lines_a_notch() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig_with_history();
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("100"))
        .unwrap();
    let column = line.chars().position(|c| c == '1').unwrap() as u16;
    let wheel = |kind| {
        InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app.handle_input(wheel(MouseEventKind::ScrollUp));
    rig.app.handle_input(wheel(MouseEventKind::ScrollUp));
    rig.draw();
    assert!(rig.screen().contains("↑6"), "{}", rig.screen());

    rig.app.handle_input(wheel(MouseEventKind::ScrollDown));
    rig.draw();
    assert!(rig.screen().contains("↑3"), "{}", rig.screen());
}

#[test]
fn the_wheel_elsewhere_does_not_scroll_the_terminal() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig_with_history();

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 70,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }));
    rig.draw();

    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn new_output_does_not_pull_a_scrolled_view_back() {
    let (mut rig, _dir) = rig("seq 1 60; sleep 1; seq 61 70; sleep 30");
    rig.toggle();
    rig.wait_for_text("60");
    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    let received = rig
        .app
        .state()
        .terminal()
        .session()
        .unwrap()
        .bytes_received();

    rig.wait_for("more output", |rig| {
        rig.app
            .state()
            .terminal()
            .session()
            .unwrap()
            .bytes_received()
            > received
    });
    rig.draw();

    assert!(rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn a_full_screen_program_gets_the_scroll_keys_and_has_no_indicator() {
    let (mut rig, _dir) = rig("printf '\\033[?1049h\\033[2Jalt'; stty raw -echo; cat -v");
    rig.toggle();
    rig.wait_for_text("alt");
    std::thread::sleep(Duration::from_millis(200));

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);

    rig.wait_for_text("^[[5;2~");
    assert!(!rig.screen().contains('↑'));
}
