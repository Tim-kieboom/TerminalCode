use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::components::{ComponentKind, PluginViewId};
use crate::ui::pane_frame::*;
use crate::ui::theme::Theme;

fn spec(border: Option<Border>, title: Option<Title>) -> FrameSpec {
    FrameSpec {
        border,
        title,
        ..FrameSpec::default()
    }
}

/// Draws `frame` into a 20 x 5 terminal and returns the backend buffer.
fn draw(
    frame: &PaneFrame,
    theme: &Theme,
    title: Option<&str>,
    focused: bool,
) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
    terminal
        .draw(|f| {
            let block = frame.block(theme, title, focused);
            f.render_widget(block, f.area());
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().to_owned())
        .collect()
}

fn frame_with(border: Border) -> PaneFrame {
    PaneFrame::resolve(
        &spec(Some(border), Some(Title::Hidden)),
        &ComponentKind::Explorer,
    )
}

#[test]
fn panes_default_to_a_plain_border_and_their_name() {
    for kind in [
        ComponentKind::Explorer,
        ComponentKind::Terminal,
        ComponentKind::Plugin(PluginViewId::new("dbg")),
    ] {
        let frame = PaneFrame::default_for(&kind);

        assert_eq!(frame.border(), Border::Plain);
        assert_eq!(frame.title(), &Title::Name);
    }
}

#[test]
fn the_editor_defaults_to_a_border_without_a_title() {
    let frame = PaneFrame::default_for(&ComponentKind::Editor);

    assert_eq!(frame.border(), Border::Plain);
    assert!(!frame.shows_title());
}

#[test]
fn the_status_bar_defaults_to_no_border_and_no_title() {
    let frame = PaneFrame::default_for(&ComponentKind::StatusBar);

    assert_eq!(frame.border(), Border::Off);
    assert!(!frame.shows_title());
}

#[test]
fn a_spec_overrides_only_what_it_names() {
    let spec = FrameSpec {
        border: Some(Border::Rounded),
        title_align: Some(TitleAlign::Center),
        ..FrameSpec::default()
    };

    let frame = PaneFrame::resolve(&spec, &ComponentKind::Terminal);

    assert_eq!(frame.border(), Border::Rounded);
    assert_eq!(frame.title_align(), TitleAlign::Center);
    assert_eq!(frame.title(), &Title::Name);
    assert_eq!(&*frame.title_slot(), "pane.title");
    assert_eq!(&*frame.border_slot(), "pane.border");
}

#[test]
fn title_text_follows_the_title_setting() {
    let name = PaneFrame::resolve(&FrameSpec::default(), &ComponentKind::Explorer);
    let hidden = PaneFrame::resolve(&spec(None, Some(Title::Hidden)), &ComponentKind::Explorer);
    let text = PaneFrame::resolve(
        &spec(None, Some(Title::Text("Files".into()))),
        &ComponentKind::Explorer,
    );

    assert_eq!(name.title_text("Explorer"), Some("Explorer"));
    assert_eq!(hidden.title_text("Explorer"), None);
    assert_eq!(text.title_text("Explorer"), Some("Files"));
}

#[test]
fn inner_leaves_room_for_a_border() {
    let area = Rect::new(2, 3, 10, 6);

    assert_eq!(frame_with(Border::Plain).inner(area), Rect::new(3, 4, 8, 4));
    assert_eq!(frame_with(Border::Thick).inner(area), Rect::new(3, 4, 8, 4));
}

#[test]
fn without_a_border_the_content_gets_the_whole_area() {
    let area = Rect::new(2, 3, 10, 6);

    assert_eq!(frame_with(Border::Off).inner(area), area);
}

#[test]
fn a_title_without_a_border_still_costs_a_row() {
    let frame = PaneFrame::resolve(
        &spec(Some(Border::Off), Some(Title::Name)),
        &ComponentKind::Explorer,
    );

    assert_eq!(frame.inner(Rect::new(0, 0, 10, 6)), Rect::new(0, 1, 10, 5));
}

#[test]
fn inner_agrees_with_what_the_block_draws() {
    for border in [
        Border::Off,
        Border::Plain,
        Border::Rounded,
        Border::Double,
        Border::Thick,
    ] {
        for title in [Title::Hidden, Title::Name] {
            let frame =
                PaneFrame::resolve(&spec(Some(border), Some(title)), &ComponentKind::Explorer);
            let area = Rect::new(1, 1, 12, 7);
            let shown = frame.title_text("x");

            assert_eq!(
                frame.inner(area),
                frame.block(&Theme::default(), shown, false).inner(area),
                "{border:?} {:?}",
                frame.title()
            );
        }
    }
}

#[test]
fn each_border_type_draws_its_own_corners() {
    let theme = Theme::default();
    let corner = |border| {
        let buffer = draw(&frame_with(border), &theme, None, false);
        buffer[(0, 0)].symbol().to_owned()
    };

    assert_eq!(corner(Border::Plain), "┌");
    assert_eq!(corner(Border::Rounded), "╭");
    assert_eq!(corner(Border::Double), "╔");
    assert_eq!(corner(Border::Thick), "┏");
    assert_eq!(corner(Border::Off), " ");
}

#[test]
fn no_border_draws_no_lines_at_all() {
    let buffer = draw(&frame_with(Border::Off), &Theme::default(), None, false);

    for y in 0..5 {
        assert_eq!(row(&buffer, y).trim(), "");
    }
}

fn title_row(align: TitleAlign) -> String {
    let frame = PaneFrame::resolve(
        &FrameSpec {
            title: Some(Title::Text("Hi".into())),
            title_align: Some(align),
            ..FrameSpec::default()
        },
        &ComponentKind::Explorer,
    );
    let buffer = draw(&frame, &Theme::default(), frame.title_text("n"), false);
    row(&buffer, 0)
}

#[test]
fn titles_can_be_left_centered_or_right_aligned() {
    assert_eq!(title_row(TitleAlign::Left), "┌Hi────────────────┐");
    assert_eq!(title_row(TitleAlign::Center), "┌────────Hi────────┐");
    assert_eq!(title_row(TitleAlign::Right), "┌────────────────Hi┐");
}

#[test]
fn a_hidden_title_leaves_the_border_unbroken() {
    let buffer = draw(&frame_with(Border::Plain), &Theme::default(), None, false);

    assert_eq!(row(&buffer, 0), "┌──────────────────┐");
}

#[test]
fn title_and_border_use_the_slots_the_frame_names() {
    let theme = Theme::from_toml(
        r#"
        ["files.title"]
        fg = "red"

        ["files.border"]
        fg = "green"
        "#,
    )
    .unwrap();
    let frame = PaneFrame::resolve(
        &FrameSpec {
            title_slot: Some("files.title".into()),
            border_slot: Some("files.border".into()),
            ..FrameSpec::default()
        },
        &ComponentKind::Explorer,
    );

    let buffer = draw(&frame, &theme, Some("Files"), false);

    assert_eq!(buffer[(1, 0)].fg, Color::Red);
    assert_eq!(buffer[(0, 2)].fg, Color::Green);
}

#[test]
fn an_unknown_slot_just_means_no_styling() {
    let frame = PaneFrame::resolve(
        &FrameSpec {
            border_slot: Some("nonexistent".into()),
            ..FrameSpec::default()
        },
        &ComponentKind::Explorer,
    );

    let buffer = draw(&frame, &Theme::default(), Some("x"), false);

    assert_eq!(buffer[(0, 2)].fg, Color::Reset);
}

#[test]
fn a_focused_frame_uses_the_focused_variant_of_its_border_slot() {
    let theme = Theme::from_toml(
        r#"
        ["pane.border"]
        fg = "red"

        ["pane.border.focused"]
        fg = "green"
        "#,
    )
    .unwrap();
    let frame = PaneFrame::default_for(&ComponentKind::Explorer);

    let unfocused = draw(&frame, &theme, None, false);
    let focused = draw(&frame, &theme, None, true);

    assert_eq!(unfocused[(0, 2)].fg, Color::Red);
    assert_eq!(focused[(0, 2)].fg, Color::Green);
}

#[test]
fn a_focused_frame_falls_back_to_the_plain_slot_without_a_focused_variant() {
    let theme = Theme::from_toml(
        r#"
        ["pane.border"]
        fg = "red"
        "#,
    )
    .unwrap();
    let frame = PaneFrame::default_for(&ComponentKind::Explorer);

    let focused = draw(&frame, &theme, None, true);

    assert_eq!(focused[(0, 2)].fg, Color::Red);
}
