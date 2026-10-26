//! Keys, the palette and the mouse with plugins: the actions a plugin declares,
//! the keys that work in its pane, and the line to type into that takes every key
//! but `esc`. The plugins here show the last message they were sent as their view,
//! so a test sees exactly what reached them.

use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
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
use crate::event::action::Action;
use crate::keymap::Keymap;
use crate::plugin_api::ActionDecl;
use crate::plugins::fixtures::{First, echo};
use crate::terminal::KeyboardSupport;
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::view::ViewNode;

const LAYOUT: &str = r#"Row([
    Pane(view: Editor),
    Fixed(40, Pane(view: Plugin("hello"))),
    Fixed(40, Pane(view: Plugin("other"))),
])"#;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn rig_with(keymap: Keymap) -> Rig {
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    let (tx, events) = mpsc::channel(1024);
    Rig {
        app: App::new(state).with_keymap(keymap).with_events(tx),
        events,
        terminal: Terminal::new(TestBackend::new(140, 20)).unwrap(),
    }
}

fn rig() -> Rig {
    rig_with(Keymap::defaults(KeyboardSupport::Enhanced))
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

    /// What a plugin shows: the text of its line, or the line to type into.
    fn view(&self, plugin: &str) -> Option<String> {
        let view = self.app.state().plugin_view(&PluginViewId::new(plugin))?;
        match view.content() {
            ViewNode::Lines(lines) => lines.first().map(|line| line.text.to_string()),
            ViewNode::Input { value, .. } => Some(value.to_string()),
            _ => None,
        }
    }

    fn version(&self, plugin: &str) -> Option<u64> {
        let view = self.app.state().plugin_view(&PluginViewId::new(plugin))?;
        Some(view.version())
    }

    fn wait_until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.pump();
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Loads a plugin and waits for the view it shows when started.
    fn load(&mut self, name: &str, wasm: Vec<u8>, actions: Vec<ActionDecl>) {
        self.app.load_plugin(name, wasm, actions, &[]);
        self.wait_until("the plugin's first view", |rig| rig.view(name).is_some());
    }

    /// Waits for the view of `plugin` to be one more than it was and returns it.
    fn after(&mut self, plugin: &str, before: Option<u64>) -> String {
        self.wait_until("a new view", |rig| rig.version(plugin) > before);
        self.view(plugin).unwrap()
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn press_char(&mut self, letter: char) {
        self.press(KeyCode::Char(letter), KeyModifiers::NONE);
    }

    fn focus(&mut self, plugin: &str) {
        self.app.run_action(Action::FocusPluginView(plugin.into()));
    }

    /// Presses a key and returns what `plugin` showed after hearing of it.
    fn send(&mut self, plugin: &str, code: KeyCode, modifiers: KeyModifiers) -> String {
        let before = self.version(plugin);
        self.press(code, modifiers);
        self.after(plugin, before)
    }

    fn editor_text(&self) -> String {
        self.app.state().editor().buffer().text()
    }
}

fn stage() -> Vec<ActionDecl> {
    vec![ActionDecl::new("stage", "Stage File").with_keys("s")]
}

const START: &str = r#"{"type":"start"}"#;
const STAGE: &str = r#"{"type":"action","action":"stage"}"#;

#[test]
fn a_plugin_is_sent_start_when_it_is_loaded() {
    let mut rig = rig();

    rig.load("hello", echo(First::Echo), stage());

    assert_eq!(rig.view("hello").as_deref(), Some(START));
}

// --- the palette ---

#[test]
fn a_declared_action_is_in_the_palette_and_runs_from_it() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());
    let before = rig.version("hello");

    rig.press(KeyCode::F(1), KeyModifiers::NONE);
    for letter in "stage".chars() {
        rig.press_char(letter);
    }
    let screen = rig.draw();
    assert!(screen.contains("hello: Stage File"), "{screen}");
    rig.press(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(rig.after("hello", before), STAGE);
}

#[test]
fn the_palette_shows_the_keys_a_plugin_declared() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());

    rig.press(KeyCode::F(1), KeyModifiers::NONE);
    for letter in "stage".chars() {
        rig.press_char(letter);
    }

    let screen = rig.draw();
    let row = screen
        .lines()
        .find(|row| row.contains("hello: Stage File"))
        .expect("the entry");
    assert!(row.contains('s'), "{row}");
}

#[test]
fn an_action_that_no_plugin_declared_says_so() {
    let mut rig = rig();

    rig.app.run_action(Action::Plugin("hello:nothing".into()));

    assert_eq!(
        rig.app.state().latest_notification(),
        Some("no plugin action named hello:nothing")
    );
}

// --- the keys of a plugin's pane ---

#[test]
fn the_keys_a_plugin_declared_work_while_its_pane_has_the_keyboard() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());
    rig.focus("hello");

    let shown = rig.send("hello", KeyCode::Char('s'), KeyModifiers::NONE);

    assert_eq!(shown, STAGE);
    assert_eq!(rig.editor_text(), "", "the editor was not typed into");
}

#[test]
fn the_same_key_types_into_the_editor_when_the_editor_has_the_keyboard() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());

    rig.press_char('s');

    assert_eq!(rig.editor_text(), "s");
    rig.pump();
    assert_eq!(
        rig.view("hello").as_deref(),
        Some(START),
        "nothing was sent"
    );
}

#[test]
fn another_plugins_keys_are_not_in_force() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());
    rig.load(
        "other",
        echo(First::Echo),
        vec![ActionDecl::new("drop", "Drop").with_keys("s")],
    );

    rig.focus("hello");
    assert_eq!(
        rig.send("hello", KeyCode::Char('s'), KeyModifiers::NONE),
        STAGE
    );
    rig.pump();
    assert_eq!(rig.view("other").as_deref(), Some(START));

    rig.focus("other");
    assert_eq!(
        rig.send("other", KeyCode::Char('s'), KeyModifiers::NONE),
        r#"{"type":"action","action":"drop"}"#
    );
}

#[test]
fn the_user_can_bind_a_plugin_action_to_another_key() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml(
            r#"[[binding]]
keys = "x"
context = "plugin"
action = { plugin = "hello:stage" }"#,
        )
        .unwrap();
    let mut rig = rig_with(keymap);
    rig.load("hello", echo(First::Echo), stage());
    rig.focus("hello");

    assert_eq!(
        rig.send("hello", KeyCode::Char('x'), KeyModifiers::NONE),
        STAGE
    );
}

#[test]
fn the_user_can_take_a_plugins_key_away_for_another_action() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml(
            r#"[[binding]]
keys = "s"
context = "plugin"
action = "focus_editor""#,
        )
        .unwrap();
    let mut rig = rig_with(keymap);
    rig.load("hello", echo(First::Echo), stage());
    rig.focus("hello");

    rig.press_char('s');

    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn a_key_a_plugin_declared_that_cannot_be_read_is_reported() {
    let mut rig = rig();

    rig.app.load_plugin(
        "hello",
        echo(First::Echo),
        vec![ActionDecl::new("stage", "Stage").with_keys("ctrl+nonsense+key")],
        &[],
    );
    rig.wait_until("the first view", |rig| rig.view("hello").is_some());
    rig.focus("hello");

    let notice = rig.app.state().latest_notification().unwrap().to_owned();
    assert!(
        notice.starts_with("a key of a plugin cannot be used"),
        "{notice}"
    );
}

#[test]
fn esc_goes_back_to_the_editor() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());
    rig.focus("hello");
    assert_eq!(rig.app.state().focus(), Focus::Plugin);

    rig.press(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    rig.press_char('a');
    assert_eq!(rig.editor_text(), "a");
}

#[test]
fn focusing_a_view_that_does_not_exist_says_so() {
    let mut rig = rig();

    rig.focus("nothing");

    assert_eq!(
        rig.app.state().latest_notification(),
        Some("no plugin view named nothing")
    );
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn a_click_on_a_plugin_pane_gives_it_the_keyboard() {
    let mut rig = rig();
    rig.load("hello", echo(First::Echo), stage());
    rig.draw();
    let area = rig
        .app
        .state()
        .plugin_area(&PluginViewId::new("hello"))
        .expect("the pane was drawn");

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x + 2,
        row: area.y + 2,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(rig.app.state().focus(), Focus::Plugin);
    assert_eq!(
        rig.app.state().focused_plugin(),
        Some(&PluginViewId::new("hello"))
    );
    assert_eq!(
        rig.send("hello", KeyCode::Char('s'), KeyModifiers::NONE),
        STAGE,
        "its keys are in force"
    );
}

// --- a line to type into ---

fn typing_rig() -> Rig {
    let mut rig = rig();
    rig.load("hello", echo(First::EchoInInput), stage());
    rig.focus("hello");
    assert!(
        rig.app.state().plugin_input(),
        "its view is a line to type into"
    );
    rig
}

#[test]
fn typed_characters_go_to_the_plugin_as_text() {
    let mut rig = typing_rig();

    let shown = rig.send("hello", KeyCode::Char('a'), KeyModifiers::NONE);
    assert_eq!(shown, r#"{"type":"text","text":"a"}"#);
    let shown = rig.send("hello", KeyCode::Char('B'), KeyModifiers::SHIFT);
    assert_eq!(shown, r#"{"type":"text","text":"B"}"#);

    assert_eq!(rig.editor_text(), "", "the editor got nothing");
}

#[test]
fn a_key_that_would_run_an_action_goes_to_the_plugin_as_text_in_a_line_to_type_into() {
    let mut rig = typing_rig();

    // `s` is bound to an action in this pane, and typing it is text here.
    let shown = rig.send("hello", KeyCode::Char('s'), KeyModifiers::NONE);

    assert_eq!(shown, r#"{"type":"text","text":"s"}"#);
}

#[test]
fn keys_that_are_not_text_go_to_the_plugin_by_name() {
    let mut rig = typing_rig();
    let none = KeyModifiers::NONE;
    let key = |name: &str, ctrl: bool, alt: bool, shift: bool| {
        format!(r#"{{"type":"key","key":"{name}","ctrl":{ctrl},"alt":{alt},"shift":{shift}}}"#)
    };

    assert_eq!(
        rig.send("hello", KeyCode::Backspace, none),
        key("backspace", false, false, false)
    );
    assert_eq!(
        rig.send("hello", KeyCode::Enter, none),
        key("enter", false, false, false)
    );
    assert_eq!(
        rig.send("hello", KeyCode::Left, none),
        key("left", false, false, false)
    );
    assert_eq!(
        rig.send("hello", KeyCode::Home, KeyModifiers::SHIFT),
        key("home", false, false, true)
    );
    assert_eq!(
        rig.send("hello", KeyCode::BackTab, KeyModifiers::SHIFT),
        key("tab", false, false, true)
    );
    assert_eq!(
        rig.send("hello", KeyCode::PageDown, none),
        key("page_down", false, false, false)
    );
}

#[test]
fn a_chord_goes_to_the_plugin_with_its_modifiers_and_no_global_binding_runs() {
    let mut rig = typing_rig();

    let ctrl_s = rig.send("hello", KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert_eq!(
        ctrl_s,
        r#"{"type":"key","key":"s","ctrl":true,"alt":false,"shift":false}"#
    );
    let ctrl_q = rig.send("hello", KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert_eq!(
        ctrl_q,
        r#"{"type":"key","key":"q","ctrl":true,"alt":false,"shift":false}"#
    );
    assert!(!rig.app.should_quit(), "only esc is the editor's");
}

#[test]
fn a_key_with_no_name_in_the_protocol_is_dropped() {
    let mut rig = typing_rig();
    let before = rig.version("hello");

    rig.press(KeyCode::F(5), KeyModifiers::NONE);
    rig.pump();
    std::thread::sleep(Duration::from_millis(100));
    rig.pump();

    assert_eq!(rig.version("hello"), before);
}

#[test]
fn pasted_text_goes_to_the_plugin_as_text() {
    let mut rig = typing_rig();
    let before = rig.version("hello");

    rig.app
        .handle_input(InputEvent::Paste("pasted words".to_owned()));

    assert_eq!(
        rig.after("hello", before),
        r#"{"type":"text","text":"pasted words"}"#
    );
    assert_eq!(rig.editor_text(), "");
}

#[test]
fn esc_ends_the_typing_and_the_plugin_is_told_every_time() {
    let mut rig = typing_rig();

    // This plugin shows a line to type into again whatever it hears, so it keeps
    // asking for the keys; esc still always ends it.
    for _ in 0..3 {
        let shown = rig.send("hello", KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(shown, r#"{"type":"input_ended"}"#);
        assert_eq!(
            rig.app.state().focus(),
            Focus::Plugin,
            "the first esc ends the typing, not the focus"
        );
    }
}

#[test]
fn when_the_plugin_stops_showing_a_line_to_type_into_the_keys_are_the_editors_again() {
    let mut rig = rig();
    rig.load("hello", echo(First::InputThenLines), stage());
    rig.focus("hello");
    assert!(rig.app.state().plugin_input());

    // Any message is answered with lines, which is not a line to type into.
    let shown = rig.send("hello", KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(shown, r#"{"type":"text","text":"x"}"#);
    assert!(!rig.app.state().plugin_input());

    // Now `s` runs the plugin's action instead of being text.
    assert_eq!(
        rig.send("hello", KeyCode::Char('s'), KeyModifiers::NONE),
        STAGE
    );
    rig.press(KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn the_cursor_of_a_line_to_type_into_is_drawn_when_the_pane_has_the_keyboard() {
    let mut rig = typing_rig();
    rig.draw();

    let cursor = rig.terminal.get_cursor_position().unwrap();
    let area = rig
        .app
        .state()
        .plugin_area(&PluginViewId::new("hello"))
        .unwrap();

    assert!(
        cursor.x > area.x && cursor.x < area.x + area.width && cursor.y > area.y,
        "{cursor:?} in {area:?}"
    );
}
