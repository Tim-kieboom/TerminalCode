//! Plugins in the app: a plugin draws into its pane, and one that goes wrong
//! is told about and stopped while the editor keeps taking keys.

use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::ui::layout::LayoutTree;

const LAYOUT: &str = r#"Row([
    Pane(view: Editor),
    Fixed(30, Pane(view: Plugin("hello"))),
])"#;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn rig() -> Rig {
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    let (tx, events) = mpsc::channel(1024);
    Rig {
        app: App::new(state).with_events(tx),
        events,
        terminal: Terminal::new(TestBackend::new(100, 20)).unwrap(),
    }
}

impl Rig {
    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn type_text(&mut self, text: &str) {
        for letter in text.chars() {
            let key = KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE);
            self.app.handle_input(InputEvent::Key(key));
        }
    }

    /// Handles what the plugin thread says until `done` holds.
    fn until(&mut self, what: &str, done: impl Fn(&mut Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn notifications(&self) -> Vec<String> {
        let notifications = self.app.state().notifications();
        notifications
            .texts()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }
}

fn plugin(body: &str, calls: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 2)
            {body}
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32) {calls}))"#
    ))
    .unwrap()
}

const SAYS_HI: &str =
    r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"plugin says hi","slot":"x"}]}}"#;

fn saying_hi() -> Vec<u8> {
    plugin(
        &format!("(data (i32.const 0) \"{}\")", SAYS_HI.replace('"', "\\\"")),
        &format!("(call $send (i32.const 0) (i32.const {}))", SAYS_HI.len()),
    )
}

fn looping() -> Vec<u8> {
    plugin("", "(loop $forever (br $forever))")
}

#[test]
fn a_plugin_draws_into_its_pane() {
    let mut rig = rig();

    rig.app.load_plugin("hello", saying_hi());

    rig.until("the plugin's view", |rig| {
        rig.screen().contains("plugin says hi")
    });
}

#[test]
fn a_plugin_that_loops_is_stopped_with_a_message_while_the_editor_takes_keys() {
    let mut rig = rig();

    rig.app.load_plugin("hello", looping());
    // The plugin is spinning on its own thread; typing goes through at once.
    rig.type_text("still typing");
    assert_eq!(rig.app.state().editor().buffer().text(), "still typing");

    rig.until("the plugin to be switched off", |rig| {
        rig.notifications()
            .iter()
            .any(|text| text.contains("plugin hello is off"))
    });
    let notifications = rig.notifications();
    assert!(
        notifications
            .iter()
            .any(|text| text.contains("plugin hello restarted: it ran out of budget")),
        "{notifications:?}"
    );
    rig.type_text("!");
    assert_eq!(rig.app.state().editor().buffer().text(), "still typing!");
}

#[test]
fn a_plugin_that_cannot_run_is_told_once_and_leaves_its_pane_alone() {
    let mut rig = rig();

    rig.app.load_plugin("hello", b"nonsense".to_vec());

    rig.until("the message", |rig| !rig.notifications().is_empty());
    assert!(rig.notifications()[0].starts_with("plugin hello cannot run: "));
    assert!(!rig.screen().contains("plugin says hi"));
}

#[test]
fn a_refresh_reaches_the_plugin_and_it_draws_again() {
    let mut rig = rig();
    rig.app.load_plugin("hello", saying_hi());
    rig.until("the first view", |rig| {
        rig.screen().contains("plugin says hi")
    });
    let version = rig
        .app
        .state()
        .plugin_view(&crate::ui::PluginViewId::new("hello"))
        .unwrap()
        .version();

    rig.app.refresh_plugin("hello");

    rig.until("a newer view", |rig| {
        rig.app
            .state()
            .plugin_view(&crate::ui::PluginViewId::new("hello"))
            .is_some_and(|view| view.version() > version)
    });
}
