use super::*;

#[test]
fn a_shell_that_cannot_start_leaves_no_empty_pane_and_the_keyboard_in_the_editor() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert!(!pane_shown(&rig), "the pane stayed open with no tab");
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert_eq!(rig.app.state().terminal().tab_count(), 0);
    assert!(rig.app.state().notifications().has_errors());
    assert!(
        rig.notifications().contains(MISSING_SHELL),
        "{}",
        rig.notifications()
    );
}

#[test]
fn typing_after_a_failed_start_goes_to_the_editor() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));
    rig.toggle();

    rig.type_text("hi");

    assert_eq!(rig.app.state().editor().buffer().text(), "hi");
}

#[test]
fn a_new_terminal_tab_that_cannot_start_leaves_the_pane_and_focus_as_they_were() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.new_tab_from_the_palette();

    assert!(!pane_shown(&rig));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn a_shell_that_cannot_start_falls_back_to_the_platform_shell_and_the_notice_stays() {
    let (mut rig, _dir) = rig_with_shells(Shell::program(MISSING_SHELL), Shell::script("sleep 30"));

    rig.toggle();

    assert!(pane_shown(&rig));
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert_eq!(rig.app.state().terminal().tab_count(), 1);
    assert!(
        rig.app.state().notifications().has_errors(),
        "the notice must be sticky"
    );
    let notice = rig.notifications();
    assert!(
        notice.contains(MISSING_SHELL) && notice.contains("started `sh` instead"),
        "{notice}"
    );
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn after_a_fallback_the_next_tab_uses_it_without_another_notice() {
    let (mut rig, _dir) = rig_with_shells(Shell::program(MISSING_SHELL), Shell::script("sleep 30"));
    rig.toggle();
    assert!(
        rig.app.state().notifications().has_errors(),
        "the notice stays until a key is pressed"
    );
    rig.prefixed('c');

    assert_eq!(rig.app.state().terminal().tab_count(), 2);
    assert!(
        !rig.app.state().notifications().has_errors(),
        "the notice was dismissed by the key and not made again: {}",
        rig.notifications()
    );
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn a_working_shell_starts_with_no_notice() {
    let (mut rig, _dir) = rig_with_shells(Shell::script("sleep 30"), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert_eq!(rig.app.state().terminal().tab_count(), 1);
    assert!(rig.app.state().notifications().len() == 0);
}

#[test]
fn a_shell_that_is_already_the_platform_shell_is_not_tried_twice() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert_eq!(rig.app.state().notifications().len(), 1);
}
