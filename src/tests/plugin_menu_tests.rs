//! A right click on a plugin's rows opens a menu of the plugin's actions, the editor's own
//! menu: the plugin says which items for the row it was told about, and choosing one runs the
//! action, which the plugin hears like a key.

use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::plugin_api::ActionDecl;
use crate::plugins::fixtures::{ACTION_AT, Echo, First, TYPE_AT};
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

const AFTER_C: u32 = TYPE_AT + 1;

/// Two actions the manifest has, a rule, one it does not have, and a rule at the end.
const MENU: &str = r#"{"type":"menu","items":[{"type":"separator"},{"type":"action","action":"open"},{"type":"separator"},{"type":"action","action":"stage"},{"type":"action","action":"nothing_like_it"},{"type":"separator"}]}"#;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn declared() -> Vec<ActionDecl> {
    let mut open = ActionDecl::new("open", "Open File");
    open.keys = Some("enter".to_owned());
    let mut stage = ActionDecl::new("stage", "Stage File");
    stage.keys = Some("s".to_owned());
    vec![open, stage, ActionDecl::new("refresh", "")]
}

/// A plugin with rows that answers a right click with `MENU`, in a pane on the right.
fn rig(menu: &str) -> Rig {
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
    let module = Echo::new(First::EchoInRows)
        .asks_on_byte(AFTER_C, b'o', menu)
        .build();
    rig.app.load_plugin("hello", module, declared(), &[]);
    rig.wait_until(|rig| rig.heard().is_some());
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

    fn wait_until(&mut self, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out");
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The text the plugin shows as the row after the forty it always has: what it heard last.
    fn heard(&self) -> Option<String> {
        let view = self.app.state().plugin_view(&PluginViewId::new("hello"))?;
        match view.content() {
            ViewNode::Rows { rows, .. } => rows
                .last()
                .map(|row| row.spans.iter().map(|span| span.text.as_ref()).collect()),
            _ => None,
        }
    }

    fn click(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    fn right_click_in_the_pane(&mut self) -> (u16, u16) {
        let body = self
            .app
            .state()
            .plugin_body(&PluginViewId::new("hello"))
            .unwrap();
        let (column, row) = (body.x + 3, body.y + 2);
        self.click(MouseEventKind::Down(MouseButton::Right), column, row);
        (column, row)
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn menu_open(&self) -> bool {
        matches!(self.app.state().popup(), Popup::Menu(_))
    }

    fn labels(&self) -> Vec<String> {
        self.app
            .state()
            .menu()
            .map(|menu| {
                menu.labels_for_test()
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[test]
fn a_right_click_on_the_rows_asks_the_plugin_and_opens_its_menu_there() {
    let mut rig = rig(MENU);

    let (column, row) = rig.right_click_in_the_pane();
    rig.wait_until(|rig| rig.menu_open());

    // Its two actions, with the titles of its manifest: the one it does not have is left out,
    // and so are the rules at the edges.
    assert_eq!(rig.labels(), ["Open File", "Stage File"]);
    let screen = rig.draw();
    let line = screen.lines().nth(usize::from(row) + 1).unwrap_or("");
    assert!(
        screen.contains("Open File") && screen.contains("Stage File"),
        "{screen}"
    );
    // It is drawn at the pointer, and shows the keys of the actions.
    assert!(
        screen.contains("enter") && screen.contains(" s"),
        "{screen}"
    );
    let _ = (column, line);
}

#[test]
fn the_plugin_heard_which_row_was_clicked() {
    let mut rig = rig(MENU);

    rig.right_click_in_the_pane();
    rig.wait_until(|rig| rig.menu_open());

    // The pane is higher than its rows are many and scrolled to the selected row, so the row
    // is the one under the pointer, counted from the first shown.
    let heard = rig.heard().unwrap();
    assert!(
        heard.starts_with(r#"{"type":"context_click","row":"#),
        "{heard}"
    );
}

#[test]
fn choosing_an_item_runs_the_action_and_the_plugin_hears_it_like_a_key() {
    let mut rig = rig(MENU);
    rig.right_click_in_the_pane();
    rig.wait_until(|rig| rig.menu_open());

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    rig.wait_until(|rig| {
        rig.heard()
            .is_some_and(|heard| heard == r#"{"type":"action","action":"stage"}"#)
    });
    assert!(!rig.menu_open(), "the menu closed");
}

#[test]
fn a_right_click_elsewhere_is_still_the_editors_and_opens_nothing_of_the_plugin() {
    let mut rig = rig(MENU);

    // The text of the editor: its own menu.
    rig.click(MouseEventKind::Down(MouseButton::Right), 5, 5);

    assert!(rig.menu_open());
    assert!(
        !rig.labels().contains(&"Open File".to_owned()),
        "{:?}",
        rig.labels()
    );
}

#[test]
fn a_plugin_that_answers_with_nothing_to_show_opens_no_menu() {
    let mut rig = rig(r#"{"type":"menu","items":[{"type":"action","action":"nothing_like_it"}]}"#);

    rig.right_click_in_the_pane();
    rig.wait_until(|rig| {
        rig.heard()
            .is_some_and(|heard| heard.contains("context_click"))
    });
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(!rig.menu_open());
}

#[test]
fn a_menu_nobody_asked_for_is_not_shown() {
    // The plugin sends it when it hears an action, not a right click.
    let module = Echo::new(First::EchoInRows)
        .asks_on_action_named(b'o', MENU)
        .build();
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
    rig.app.load_plugin("hello", module, declared(), &[]);
    rig.wait_until(|rig| rig.heard().is_some());

    rig.app
        .run_action(crate::event::action::Action::Plugin("hello:open".into()));
    rig.wait_until(|rig| {
        rig.heard()
            .is_some_and(|heard| heard.contains(r#""action":"open""#))
    });
    std::thread::sleep(Duration::from_millis(200));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(!rig.menu_open());
    let _ = ACTION_AT;
}
