use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::Line;

use super::*;
use crate::ui::theme::ThemeVariant;

fn list(count: usize) -> PickList<String> {
    PickList::new((0..count).map(|n| format!("row {n}")).collect())
}

fn draw(list: &PickList<String>, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let theme = Theme::built_in(ThemeVariant::Dark);
    terminal
        .draw(|frame| {
            list.draw(frame, &theme, "Things", 60, |text, _, _| {
                Line::raw(text.clone())
            });
        })
        .unwrap();
    terminal
}

#[test]
fn a_step_of_one_goes_round_the_ends_and_a_page_stops_at_them() {
    let mut list = list(5);
    list.step(-1);
    assert_eq!(list.selected(), 4);
    list.step(1);
    assert_eq!(list.selected(), 0);
    list.page(1);
    assert_eq!(list.selected(), 4);
    list.page(-1);
    assert_eq!(list.selected(), 0);
}

#[test]
fn an_empty_list_ignores_steps() {
    let mut list = list(0);
    list.step(1);
    list.page(1);
    assert_eq!(list.selected(), 0);
    assert!(list.selected_entry().is_none());
}

#[test]
fn select_index_refuses_a_row_that_is_not_there() {
    let mut list = list(3);
    assert!(list.select_index(2));
    assert!(!list.select_index(3));
    assert_eq!(list.selected(), 2);
}

#[test]
fn selecting_clamps_to_the_last_row() {
    assert_eq!(list(3).selecting(9).selected(), 2);
}

#[test]
fn a_click_selects_the_row_under_it_and_the_border_is_not_a_row() {
    let mut list = list(5);
    draw(&list, 80, 24);
    assert!(list.contains(40, 6));
    // The box starts at a fifth of the screen height; its first row is one below.
    let top = 24 / 5;
    assert!(!list.select_at(40, top), "the border");
    assert!(list.select_at(40, top + 3));
    assert_eq!(list.selected(), 2);
    assert!(!list.select_at(0, 0));
    assert!(!list.contains(0, 0));
}

#[test]
fn the_scroll_follows_a_selection_past_the_last_row_shown() {
    let mut list = list(30);
    list.select_index(20);
    let terminal = draw(&list, 80, 24);
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("row 20"));
    assert!(!text.contains("row 0 "));
}

#[test]
fn a_screen_too_small_for_the_box_draws_nothing_and_clicks_miss() {
    let mut list = list(5);
    draw(&list, 80, 4);
    assert!(!list.contains(40, 2));
    assert!(!list.select_at(40, 2));
}
