use ratatui::Frame;

use crate::state::AppState;
use crate::ui::layout::Placement;
use crate::ui::{Hideable, HideableKind, Render};

#[derive(Debug, Default, PartialEq, Eq)]
struct Popup(Vec<u32>);

impl Render for Popup {
    fn render(&self, _: &mut Frame, _: &AppState, _: &Placement) {}
}

#[test]
fn it_starts_hidden() {
    let popup: Hideable<Popup> = Hideable::default();

    assert_eq!(popup.hideable_kind(), HideableKind::Hide);
    assert!(popup.try_get().is_none());
}

#[test]
fn a_shown_popup_gives_access_to_its_value() {
    let popup = Hideable::new_show(Popup(vec![1]));

    assert_eq!(popup.hideable_kind(), HideableKind::Show);
    assert_eq!(popup.try_get(), Some(&Popup(vec![1])));
}

#[test]
fn a_hidden_popup_keeps_its_value_for_when_it_is_shown_again() {
    let mut popup = Hideable::new_show(Popup(vec![1, 2]));

    popup.set_kind(HideableKind::Hide);
    assert!(popup.try_get().is_none());
    assert_eq!(
        popup.node,
        Popup(vec![1, 2]),
        "hiding does not forget the state"
    );

    popup.set_kind(HideableKind::Show);
    assert_eq!(popup.try_get(), Some(&Popup(vec![1, 2])));
}

#[test]
fn the_shown_value_can_be_changed_in_place_but_not_while_hidden() {
    let mut popup = Hideable::new_show(Popup(vec![1]));

    popup.try_get_mut().unwrap().0.push(2);
    assert_eq!(popup.try_get(), Some(&Popup(vec![1, 2])));

    popup.set_kind(HideableKind::Hide);
    assert!(popup.try_get_mut().is_none());
}

#[test]
fn take_hides_it_and_hands_back_the_value() {
    let mut popup = Hideable::new_show(Popup(vec![7]));

    assert_eq!(popup.take(), Some(Popup(vec![7])));

    assert_eq!(popup.hideable_kind(), HideableKind::Hide);
    assert_eq!(popup.take(), None, "taking twice finds nothing");
}

#[test]
fn showing_again_after_a_take_starts_from_the_new_value() {
    let mut popup = Hideable::new_show(Popup(vec![1]));
    popup.take();

    popup = Hideable::new_show(Popup(vec![2]));

    assert_eq!(popup.try_get(), Some(&Popup(vec![2])));
}

#[test]
fn is_shown_and_toggle_follow_the_kind() {
    let mut popup = Hideable::new_show(Popup(vec![1]));
    assert!(popup.is_shown());

    popup.toggle();
    assert!(!popup.is_shown());
    assert_eq!(popup.hideable_kind(), HideableKind::Hide);
    assert_eq!(
        popup.node,
        Popup(vec![1]),
        "toggling does not touch the value"
    );

    popup.toggle();
    assert!(popup.is_shown());
}
