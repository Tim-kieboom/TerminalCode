use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use serde_json::{Value, json};

use super::*;
use crate::lsp::parse_completion;

fn open_box(items: usize) -> CompletionBox {
    let list = Value::Array(
        (0..items)
            .map(|n| json!({ "label": format!("item{n:02}") }))
            .collect(),
    );
    let list = parse_completion(&list).unwrap();
    let completion = Completion::new(list, &Rope::from_str("i"), 0, 1);
    CompletionBox::new(completion, DocumentId::new(1), Rope::from_str("i"), false)
}

/// The lines of the box that show an item, each with whether it is highlighted:
/// its background differs from the one most rows have.
fn drawn_rows(completion_box: &CompletionBox) -> Vec<(String, bool)> {
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).unwrap();
    let theme = Theme::default();
    terminal
        .draw(|frame| completion_box.render(frame, &theme, Position::new(5, 2)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let lines: Vec<(String, Option<Color>)> = (0..area.height)
        .map(|y| {
            let line: String = (0..area.width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect();
            let background = (0..area.width)
                .find(|&x| buffer[(x, y)].symbol() == "i")
                .map(|x| buffer[(x, y)].bg);
            (line, background)
        })
        .filter(|(line, _)| line.contains("item"))
        .collect();
    let plain = lines
        .iter()
        .map(|(_, background)| *background)
        .max_by_key(|background| {
            lines
                .iter()
                .filter(|(_, other)| other == background)
                .count()
        })
        .flatten();
    lines
        .into_iter()
        .map(|(line, background)| (line, background != plain))
        .collect()
}

fn highlighted(completion_box: &CompletionBox) -> Vec<String> {
    drawn_rows(completion_box)
        .into_iter()
        .filter(|(_, marked)| *marked)
        .map(|(line, _)| line)
        .collect()
}

#[test]
fn a_row_is_highlighted_before_the_list_scrolls() {
    let completion_box = open_box(20);

    assert_eq!(highlighted(&completion_box).len(), 1);
}

#[test]
fn a_row_is_highlighted_in_the_frame_where_the_selection_leaves_the_rows_shown() {
    let mut completion_box = open_box(20);
    drawn_rows(&completion_box);

    for _ in 0..=MOST_ROWS {
        completion_box.completion_mut().select_next();
    }

    let marked = highlighted(&completion_box);
    assert_eq!(marked.len(), 1, "one row is highlighted: {marked:?}");
    assert!(marked[0].contains(&format!("item{:02}", MOST_ROWS + 1)));
}
