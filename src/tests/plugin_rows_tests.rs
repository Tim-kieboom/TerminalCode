//! A plugin's rows: drawn with the selected one highlighted and kept on screen,
//! and a click on one reaches the plugin as the number of the row.

use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::plugins::fixtures::{First, echo};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::{ViewNode, rows_scroll};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn rig() -> Rig {
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(
        LayoutTree::from_ron(
            r#"Row([Pane(view: Editor), Fixed(40, Pane(view: Plugin("hello")))])"#,
        )
        .unwrap(),
    );
    let (tx, events) = mpsc::channel(1024);
    let mut rig = Rig {
        app: App::new(state).with_events(tx),
        events,
        terminal: Terminal::new(TestBackend::new(100, 16)).unwrap(),
    };
    rig.app
        .load_plugin("hello", echo(First::EchoInRows), vec![], &[]);
    rig.wait_for_view();
    rig.draw();
    rig
}

impl Rig {
    fn draw(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn wait_for_view(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self
            .app
            .state()
            .plugin_view(&PluginViewId::new("hello"))
            .is_none()
        {
            assert!(Instant::now() < deadline, "no view");
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn id() -> PluginViewId {
        PluginViewId::new("hello")
    }

    /// The text of the last row, which is where the plugin shows what it heard.
    fn heard(&self) -> String {
        let view = self.app.state().plugin_view(&Self::id()).unwrap();
        let ViewNode::Rows { rows, .. } = view.content() else {
            panic!("rows");
        };
        rows.last().unwrap().spans[0].text.to_string()
    }

    fn wait_for_news(&mut self, before: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.heard() == before {
            assert!(Instant::now() < deadline, "the plugin heard nothing");
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.heard()
    }

    fn click(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// The first row shown, and how many rows the pane shows.
    fn window(&self) -> (usize, usize) {
        let body = self.app.state().plugin_body(&Self::id()).unwrap();
        let height = usize::from(body.height);
        (rows_scroll(Some(40), 41, height), height)
    }
}

#[test]
fn the_rows_are_drawn_from_the_one_that_keeps_the_selected_row_in_the_middle() {
    let mut rig = rig();
    let (top, height) = rig.window();
    assert!(top > 0, "there are more rows than fit");

    let screen = rig.draw();

    assert!(screen.contains(&format!("row {top}")), "{screen}");
    assert!(
        screen.contains(&format!("row {}", top + height - 2)),
        "{screen}"
    );
    assert!(!screen.contains("row 0 "), "scrolled away: {screen}");
}

#[test]
fn the_selected_row_is_highlighted() {
    let mut rig = rig();
    let (top, height) = rig.window();
    let body = rig.app.state().plugin_body(&Rig::id()).unwrap();
    rig.draw();
    let selected = rig.app.state().theme().style("explorer.selected.inactive");

    let buffer = rig.terminal.backend().buffer();
    let last_row = body.y + (height - 1) as u16;
    let cell = &buffer[(body.x, last_row)];
    let above = &buffer[(body.x, last_row - 1)];

    assert_eq!(cell.style().bg, selected.bg, "the selected row");
    assert_ne!(above.style().bg, selected.bg, "the row above it");
    assert!(top + height > 40);
}

#[test]
fn the_highlight_is_the_stronger_one_while_the_pane_has_the_keyboard() {
    let mut rig = rig();
    let body = rig.app.state().plugin_body(&Rig::id()).unwrap();
    rig.app
        .run_action(crate::event::action::Action::FocusPluginView(
            "hello".into(),
        ));
    rig.draw();
    let focused = rig.app.state().theme().style("explorer.selected");
    let (_, height) = rig.window();

    let buffer = rig.terminal.backend().buffer();
    let cell = &buffer[(body.x, body.y + (height - 1) as u16)];

    assert_eq!(cell.style().bg, focused.bg);
}

#[test]
fn a_click_on_a_row_tells_the_plugin_which_row_it_is() {
    let mut rig = rig();
    let (top, height) = rig.window();
    let body = rig.app.state().plugin_body(&Rig::id()).unwrap();
    let before = rig.heard();

    rig.click(body.x + 2, body.y + 1);

    assert_eq!(
        rig.wait_for_news(&before),
        format!(r#"{{"type":"click","row":{}}}"#, top + 1)
    );
    assert_eq!(
        rig.app.state().focus(),
        Focus::Plugin,
        "it has the keyboard"
    );
    assert!(height > 2);
}

#[test]
fn a_click_on_the_last_shown_row_is_the_last_row() {
    let mut rig = rig();
    let (_, height) = rig.window();
    let body = rig.app.state().plugin_body(&Rig::id()).unwrap();
    let before = rig.heard();

    rig.click(body.x, body.y + (height - 1) as u16);

    assert_eq!(rig.wait_for_news(&before), r#"{"type":"click","row":40}"#);
}

#[test]
fn a_click_on_the_frame_of_the_pane_gives_it_the_keyboard_and_no_row() {
    let mut rig = rig();
    let area = rig.app.state().plugin_area(&Rig::id()).unwrap();
    let before = rig.heard();

    rig.click(area.x, area.y);

    assert_eq!(rig.app.state().focus(), Focus::Plugin);
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }
    assert_eq!(rig.heard(), before, "nothing was sent");
}

#[test]
fn a_click_outside_the_pane_does_not_reach_the_plugin() {
    let mut rig = rig();
    let before = rig.heard();

    rig.click(2, 5);

    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }
    assert_eq!(rig.heard(), before);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}
