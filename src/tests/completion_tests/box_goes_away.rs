use super::*;

#[test]
fn a_letter_nothing_matches_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec", "vector"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text("x");

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_character_that_is_not_part_of_a_word_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.type_text(" ");

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn an_answer_for_a_word_that_is_over_is_dropped() {
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let wait = std::sync::Mutex::new(wait);
    let answer: Answer = Arc::new(move |_| {
        let _ = wait.lock().unwrap().recv();
        items(&["vec"])
    });
    let mut rig = Rig::start(provider(), "", answer);
    rig.type_text("v");
    rig.type_text(" ");

    release.send(()).unwrap();
    rig.pump_until(|_| true);
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn backspacing_to_the_start_of_the_word_closes_a_box_that_a_trigger_did_not_open() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.press(KeyCode::Backspace);
    rig.frame();

    assert!(rig.app.state().completion_box().is_none());
}

#[test]
fn a_click_closes_the_box() {
    let mut rig = Rig::with_answer(items(&["vec"]));
    rig.type_text("v");
    rig.wait_for_box();

    rig.app
        .handle_input(InputEvent::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 50,
            row: 10,
            modifiers: KeyModifiers::NONE,
        }));

    assert!(rig.app.state().completion_box().is_none());
}
