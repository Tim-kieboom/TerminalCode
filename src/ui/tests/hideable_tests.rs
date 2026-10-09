use std::cell::Cell;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::app::state::AppState;
use crate::components::ComponentKind;
use crate::ui::layout::Placement;
use crate::ui::{Hideable, Render};

/// Counts how often it is prepared and drawn.
#[derive(Debug, Default)]
struct Counting {
    prepared: u32,
    rendered: Cell<u32>,
}

impl Render for Counting {
    fn prepare(&mut self, _: &Placement) {
        self.prepared += 1;
    }

    fn render(&self, _: &mut ratatui::Frame, _: &AppState, _: &Placement) {
        self.rendered.set(self.rendered.get() + 1);
    }
}

fn placement() -> Placement {
    Placement::new(ComponentKind::Explorer, Rect::new(0, 0, 10, 5))
}

fn hidden<T>(node: T) -> Hideable<T> {
    let mut hideable = Hideable::new_show(node);
    hideable.hide();
    hideable
}

fn draw(hideable: &mut Hideable<Counting>) {
    let state = AppState::default();
    let placement = placement();
    let mut terminal = Terminal::new(TestBackend::new(10, 5)).unwrap();
    hideable.prepare(&placement);
    terminal
        .draw(|frame| hideable.render(frame, &state, &placement))
        .unwrap();
}

#[test]
fn it_holds_any_value_not_only_one_that_can_be_drawn() {
    // `u32` is not a `Render`: the bound is only on drawing.
    let mut number = Hideable::new_show(7_u32);

    *number.node_mut() += 1;

    assert_eq!(*number.node(), 8);
}

#[test]
fn it_is_shown_when_made_and_hidden_once_hidden() {
    assert!(Hideable::new_show(1).is_shown());
    assert!(!hidden(1).is_shown());
}

#[test]
fn show_hide_and_toggle_change_whether_it_is_shown() {
    let mut popup = hidden(1);

    popup.show();
    assert!(popup.is_shown());
    popup.show();
    assert!(popup.is_shown(), "showing twice is fine");

    popup.hide();
    assert!(!popup.is_shown());
    popup.hide();
    assert!(!popup.is_shown(), "hiding twice is fine");

    popup.toggle();
    assert!(popup.is_shown());
    popup.toggle();
    assert!(!popup.is_shown());
}

#[test]
fn hiding_keeps_the_value_for_when_it_is_shown_again() {
    let mut popup = Hideable::new_show(vec![1, 2]);

    popup.hide();
    popup.node_mut().push(3);
    popup.show();

    assert_eq!(popup.node(), &vec![1, 2, 3]);
}

#[test]
fn the_value_is_reachable_while_hidden() {
    let popup = hidden(String::from("kept"));

    assert_eq!(popup.node(), "kept");
}

#[test]
fn a_shown_component_is_prepared_and_drawn() {
    let mut shown = Hideable::new_show(Counting::default());

    draw(&mut shown);

    assert_eq!(shown.node().prepared, 1);
    assert_eq!(shown.node().rendered.get(), 1);
}

#[test]
fn a_hidden_component_is_neither_prepared_nor_drawn() {
    let mut popup = hidden(Counting::default());

    draw(&mut popup);

    assert_eq!(popup.node().prepared, 0);
    assert_eq!(popup.node().rendered.get(), 0);
}

#[test]
fn a_component_hidden_and_shown_again_is_drawn_again() {
    let mut popup = Hideable::new_show(Counting::default());
    draw(&mut popup);

    popup.hide();
    draw(&mut popup);
    assert_eq!(popup.node().rendered.get(), 1, "not drawn while hidden");

    popup.show();
    draw(&mut popup);
    assert_eq!(popup.node().rendered.get(), 2);
    assert_eq!(popup.node().prepared, 2);
}
