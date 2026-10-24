use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

fn var(name: &str, reference: i64) -> Incoming {
    Incoming {
        name: name.to_owned(),
        value: "0".to_owned(),
        reference,
        ..Incoming::default()
    }
}

/// A box for a list `items` long, open, with the box not drawn yet.
fn list_of(items: usize) -> DebugHoverBox {
    let anchor = Position::new(0, 0);
    let mut hover = DebugHoverBox::valued("v", var("v", 1), anchor, DocumentId::new(1), 1, true);
    let read = hover.tree_mut().toggle(0).unwrap();
    let children = (0..items).map(|n| var(&format!("item{n}"), 0)).collect();
    hover.tree_mut().fill(read.row, 0, children);
    hover
}

fn draw(hover: &DebugHoverBox) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(60, 30)).unwrap();
    let theme = Theme::default();
    terminal.draw(|frame| hover.render(frame, &theme)).unwrap();
    terminal
}

/// The names of the rows on the screen, by the line each is on.
fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn the_selected_row_scrolls_into_view_when_the_keys_go_past_the_last_row_shown() {
    let mut hover = list_of(40);

    hover.move_by(MOST_LINES as isize + 5);

    let terminal = draw(&hover);
    let screen = screen_text(&terminal);
    let selected = &hover.tree().rows()[hover.selected()].name;
    assert!(
        screen.contains(selected.as_str()),
        "{selected} is selected but not on the screen"
    );
}

#[test]
fn the_selected_row_scrolls_back_into_view_going_up() {
    let mut hover = list_of(40);
    hover.move_by(30);

    hover.move_by(-30);

    let terminal = draw(&hover);
    assert!(screen_text(&terminal).contains("item0"));
}

#[test]
fn selecting_a_row_far_down_shows_it() {
    let mut hover = list_of(40);

    hover.select(35);

    let terminal = draw(&hover);
    let selected = &hover.tree().rows()[35].name;
    assert!(screen_text(&terminal).contains(selected.as_str()));
}
