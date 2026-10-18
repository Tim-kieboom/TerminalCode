//! A plugin's column of buttons, rows and a line to type into: drawn part by part,
//! and a click reaches the plugin as what was clicked.

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
use crate::ui::view::{ViewNode, button_cells};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn id() -> PluginViewId {
    PluginViewId::new("hello")
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
        .load_plugin("hello", echo(First::EchoInColumn), vec![], &[]);
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

    fn pump(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.app.handle_event(event);
        }
    }

    fn wait_for_view(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.app.state().plugin_view(&id()).is_none() {
            assert!(Instant::now() < deadline, "no view");
            self.pump();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The text of the line to type into, where the plugin shows what it heard.
    fn heard(&self) -> String {
        let view = self.app.state().plugin_view(&id()).unwrap();
        let ViewNode::Column(parts) = view.content() else {
            panic!("a column");
        };
        let Some(ViewNode::Input { value, .. }) = parts.last() else {
            panic!("the line to type into is last");
        };
        value.to_string()
    }

    fn wait_for_news(&mut self, before: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.heard() == before {
            assert!(Instant::now() < deadline, "the plugin heard nothing");
            self.pump();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.heard()
    }

    /// Whether the plugin hears nothing more for a moment.
    fn stays_quiet(&mut self, before: &str) -> bool {
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            self.pump();
            std::thread::sleep(Duration::from_millis(10));
        }
        self.heard() == before
    }

    fn click(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// The area of part `index` of the column (buttons, rows, input).
    fn part(&self, index: usize) -> ratatui::layout::Rect {
        let body = self.app.state().plugin_body(&id()).unwrap();
        let view = self.app.state().plugin_view(&id()).unwrap();
        view.content().leaves(body)[index].0
    }
}

#[test]
fn the_column_is_drawn_as_buttons_then_rows_then_the_line_to_type_into() {
    let mut rig = rig();

    let screen = rig.draw();

    assert!(screen.contains(" Commit "), "{screen}");
    assert!(screen.contains(" Push "), "{screen}");
    assert!(screen.contains("row 0"), "{screen}");
    assert!(screen.contains("row 1"), "{screen}");
    let (buttons, rows, input) = (rig.part(0), rig.part(1), rig.part(2));
    assert!(
        buttons.y < rows.y && rows.y < input.y,
        "{buttons:?} {rows:?} {input:?}"
    );
    assert_eq!(
        input.bottom(),
        rig.app.state().plugin_body(&id()).unwrap().bottom()
    );
}

#[test]
fn a_disabled_button_is_drawn_in_the_dim_style_and_an_enabled_one_in_the_active_one() {
    let mut rig = rig();
    rig.draw();
    let buttons = rig.part(0);
    let theme = rig.app.state().theme();
    let (active, inactive) = (
        theme.style("bar.tab.active"),
        theme.style("bar.tab.inactive"),
    );
    let cells = {
        let view = rig.app.state().plugin_view(&id()).unwrap();
        let ViewNode::Column(parts) = view.content() else {
            panic!()
        };
        let ViewNode::Buttons(buttons) = &parts[0] else {
            panic!()
        };
        button_cells(buttons)
    };
    let buffer = rig.terminal.backend().buffer();

    let commit = &buffer[(buttons.x + cells[0].0 + 1, buttons.y)];
    let push = &buffer[(buttons.x + cells[1].0 + 1, buttons.y)];

    assert_eq!(commit.fg, active.fg.unwrap_or(commit.fg));
    assert_eq!(push.fg, inactive.fg.unwrap_or(push.fg));
    assert_ne!(commit.style(), push.style());
}

#[test]
fn a_click_on_a_button_reaches_the_plugin_as_its_id() {
    let mut rig = rig();
    let before = rig.heard();
    let buttons = rig.part(0);

    rig.click(buttons.x + 2, buttons.y);

    assert_eq!(
        rig.wait_for_news(&before),
        r#"{"type":"press","button":"commit"}"#
    );
    assert_eq!(
        rig.app.state().focus(),
        Focus::Plugin,
        "the pane has the keys"
    );
}

#[test]
fn a_click_on_a_button_that_cannot_be_pressed_tells_the_plugin_nothing() {
    let mut rig = rig();
    let before = rig.heard();
    let buttons = rig.part(0);

    // " Commit " is 8 wide, a gap, then " Push " from column 9.
    rig.click(buttons.x + 10, buttons.y);

    assert!(rig.stays_quiet(&before));
}

#[test]
fn a_click_in_the_gap_between_buttons_tells_the_plugin_nothing() {
    let mut rig = rig();
    let before = rig.heard();
    let buttons = rig.part(0);

    rig.click(buttons.x + 8, buttons.y);

    assert!(rig.stays_quiet(&before));
}

#[test]
fn a_click_on_a_row_of_the_rows_part_is_the_number_of_the_row() {
    let mut rig = rig();
    let before = rig.heard();
    let rows = rig.part(1);

    rig.click(rows.x + 1, rows.y + 1);

    assert_eq!(rig.wait_for_news(&before), r#"{"type":"click","row":1}"#);
}

#[test]
fn a_click_below_the_last_row_tells_the_plugin_nothing() {
    let mut rig = rig();
    let before = rig.heard();
    let rows = rig.part(1);
    assert!(rows.height > 2, "there is room under the two rows");

    rig.click(rows.x + 1, rows.y + 2);

    assert!(rig.stays_quiet(&before));
}

#[test]
fn a_click_in_the_line_to_type_into_is_the_character_it_is_on() {
    let mut rig = rig();
    let before = rig.heard();
    let input = rig.part(2);

    // The line is `> ` and the text; the 4th column is the 2nd character after
    // the prompt.
    rig.click(input.x + 2 + 3, input.y);

    assert_eq!(
        rig.wait_for_news(&before),
        r#"{"type":"input_click","column":3}"#
    );
}

#[test]
fn an_idle_line_leaves_the_keys_to_the_editor() {
    let mut rig = rig();
    let input = rig.part(2);

    rig.click(input.x + 3, input.y);

    assert_eq!(rig.app.state().focus(), Focus::Plugin);
    assert!(
        !rig.app.state().plugin_input(),
        "the line says it is not active, so the keys are not the plugin's text"
    );
}
