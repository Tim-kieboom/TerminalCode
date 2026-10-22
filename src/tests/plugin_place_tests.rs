//! Where a plugin's view goes: a bar next to the Explorer's or the Terminal's, or a
//! popup. The choice is kept in `state.json` and put on the layout in memory.

use std::fs;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus, Popup};
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;
use crate::persist::saved_state::{self, Place, PluginRecord, SavedState};
use crate::plugins::fixtures::{Echo, First};
use crate::ui::layout::LayoutTree;
use crate::ui::{ComponentKind, PluginViewId};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    config: tempfile::TempDir,
}

fn hello() -> ComponentKind {
    ComponentKind::Plugin(PluginViewId::new("hello"))
}

fn record(enabled: bool, place: Option<Place>) -> PluginRecord {
    PluginRecord {
        enabled,
        capabilities: Vec::new(),
        place,
        file_types: Vec::new(),
        own_thread: false,
        run: Vec::new(),
    }
}

fn rig_with(layout: &str, first: First, record: Option<PluginRecord>) -> Rig {
    let config = tempfile::tempdir().unwrap();
    let dir = config.path().join("plugins/hello");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        "id = \"hello\"\nname = \"Hello\"\nversion = \"0.1.0\"\nabi = \"1.2\"\nwasm = \"plugin.wasm\"\n",
    )
    .unwrap();
    fs::write(dir.join("plugin.wasm"), Echo::new(first).build()).unwrap();

    let mut saved = SavedState::default();
    if let Some(record) = record {
        saved.set_plugin("hello", record);
    }
    let mut state = AppState::new(Editor::new(Buffer::from_text("")));
    state.set_layout(LayoutTree::from_ron(layout).unwrap());
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(saved)
        .with_events(tx);
    Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(120, 30)).unwrap(),
        config,
    }
}

fn rig(record: Option<PluginRecord>) -> Rig {
    rig_with(&default_layout(), First::Echo, record)
}

fn default_layout() -> String {
    "Col([Row([Fixed(30, SideBar(views: [Explorer])), Col([Pane(view: Editor), \
     Fixed(8, BottomBar(views: [Terminal]))])])])"
        .to_owned()
}

impl Rig {
    fn screen(&mut self) -> String {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        self.terminal.backend().to_string()
    }

    fn wait_for_view(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while self
            .app
            .state()
            .plugin_view(&PluginViewId::new("hello"))
            .is_none()
        {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            assert!(Instant::now() < deadline, "the plugin never showed a view");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    fn click(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    fn state_file_exists(&self) -> bool {
        saved_state::path_in(self.config.path()).exists()
    }

    fn remembered(&self) -> SavedState {
        saved_state::load(Some(self.config.path())).0
    }

    fn bar_of_hello(&self) -> Option<Vec<ComponentKind>> {
        self.app
            .state()
            .layout()
            .bar_views(&hello())
            .map(<[_]>::to_vec)
    }

    fn notification(&self) -> Option<String> {
        self.app.state().latest_notification().map(str::to_owned)
    }
}

#[test]
fn a_plugin_placed_in_the_explorers_bar_is_a_view_of_it() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();
    assert_eq!(rig.bar_of_hello(), None, "nothing placed yet");

    rig.app.place_plugin("hello", Place::Explorer);

    assert_eq!(
        rig.bar_of_hello(),
        Some(vec![ComponentKind::Explorer, hello()])
    );
    assert_eq!(
        rig.remembered().plugin("hello").unwrap().place,
        Some(Place::Explorer)
    );
    // The plugin's view is what the bar shows, and the layout file is as it was.
    assert!(rig.screen().contains("start"));
    assert!(!rig.app.state().base_layout().contains(&hello()));
}

#[test]
fn a_plugin_placed_in_the_bottom_bar_joins_the_terminals_bar() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();

    rig.app.place_plugin("hello", Place::Bottom);

    assert_eq!(
        rig.bar_of_hello(),
        Some(vec![ComponentKind::Terminal, hello()])
    );
}

#[test]
fn a_placement_is_applied_when_the_app_starts() {
    let mut rig = rig(Some(record(true, Some(Place::Bottom))));
    rig.wait_for_view();

    assert_eq!(
        rig.bar_of_hello(),
        Some(vec![ComponentKind::Terminal, hello()])
    );
}

#[test]
fn a_plugin_that_is_off_or_not_placed_has_no_tab() {
    let off = rig(Some(record(false, Some(Place::Explorer))));
    assert_eq!(off.bar_of_hello(), None);

    let mut unplaced = rig(Some(record(true, None)));
    unplaced.wait_for_view();
    assert_eq!(unplaced.bar_of_hello(), None);
}

#[test]
fn turning_a_placed_plugin_off_takes_its_tab_away_and_on_gives_it_back() {
    let mut rig = rig(Some(record(true, Some(Place::Explorer))));
    rig.wait_for_view();
    assert!(rig.bar_of_hello().is_some());

    rig.app.run_action(Action::TogglePlugin("hello".into()));
    assert_eq!(rig.bar_of_hello(), None, "off");
    assert_eq!(
        rig.remembered().plugin("hello").unwrap().place,
        Some(Place::Explorer),
        "the place is kept"
    );

    rig.app.run_action(Action::TogglePlugin("hello".into()));
    assert!(rig.bar_of_hello().is_some(), "on again, where it was");
}

#[test]
fn a_new_layout_gets_the_placed_views_again() {
    let mut rig = rig(Some(record(true, Some(Place::Explorer))));
    rig.wait_for_view();

    rig.app
        .state_mut()
        .set_layout(LayoutTree::from_ron(&default_layout()).unwrap());

    assert_eq!(
        rig.bar_of_hello(),
        Some(vec![ComponentKind::Explorer, hello()])
    );
}

#[test]
fn a_plugin_the_layout_file_names_stays_where_the_file_puts_it() {
    let layout = r#"Row([Pane(view: Editor), Fixed(40, Pane(view: Plugin("hello")))])"#;
    let mut rig = rig_with(layout, First::Echo, Some(record(true, Some(Place::Popup))));
    rig.wait_for_view();

    rig.app.place_plugin("hello", Place::Explorer);

    assert!(rig.notification().unwrap().contains("your layout file"));
    assert_eq!(rig.bar_of_hello(), None);
    assert!(rig.app.state().layout().contains(&hello()));
}

#[test]
fn a_layout_without_bars_offers_the_side_pane_and_the_popup() {
    let layout = "Row([Fixed(30, Pane(view: Explorer)), Pane(view: Editor)])";
    let mut rig = rig_with(layout, First::Echo, Some(record(true, None)));
    rig.wait_for_view();

    assert_eq!(rig.app.available_places(), [Place::Side, Place::Popup]);
    rig.app.place_plugin("hello", Place::Explorer);
    assert!(rig.notification().unwrap().contains("no Explorer's bar"));
    assert!(
        !rig.state_file_exists(),
        "nothing was chosen, so nothing is written"
    );
}

#[test]
fn a_layout_with_both_bars_offers_all_four_places() {
    let rig = rig(None);
    assert_eq!(
        rig.app.available_places(),
        [Place::Explorer, Place::Bottom, Place::Side, Place::Popup]
    );
}

// --- the popup ---

#[test]
fn a_plugin_placed_in_a_popup_opens_over_the_editor_and_has_the_keyboard() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();

    rig.app.place_plugin("hello", Place::Popup);

    assert!(matches!(rig.app.state().popup(), Popup::PluginView(_)));
    assert_eq!(rig.app.state().focus(), Focus::Plugin);
    let screen = rig.screen();
    assert!(screen.contains(" hello "), "the frame names it\n{screen}");
    assert!(screen.contains("start"), "its view is in it\n{screen}");
}

#[test]
fn esc_closes_the_popup_and_gives_the_keyboard_back() {
    let mut rig = rig(Some(record(true, Some(Place::Popup))));
    rig.wait_for_view();
    rig.app.open_plugin_popup("hello");
    assert!(matches!(rig.app.state().popup(), Popup::PluginView(_)));

    rig.press(KeyCode::Esc);

    assert!(matches!(rig.app.state().popup(), Popup::None));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.screen().contains(" hello "));
}

#[test]
fn a_click_outside_the_popup_closes_it_and_one_inside_does_not() {
    let mut rig = rig(Some(record(true, Some(Place::Popup))));
    rig.wait_for_view();
    rig.app.open_plugin_popup("hello");
    rig.screen();
    let area = rig
        .app
        .state()
        .plugin_area(&PluginViewId::new("hello"))
        .unwrap();

    rig.click(area.x + 3, area.y + 2);
    assert!(matches!(rig.app.state().popup(), Popup::PluginView(_)));

    rig.click(0, 0);
    assert!(matches!(rig.app.state().popup(), Popup::None));
}

#[test]
fn esc_ends_the_typing_in_a_popup_before_it_closes_it() {
    let mut rig = rig_with(
        &default_layout(),
        First::EchoInInput,
        Some(record(true, Some(Place::Popup))),
    );
    rig.wait_for_view();
    rig.app.open_plugin_popup("hello");
    rig.app.state_mut().focus_plugin(PluginViewId::new("hello"));
    assert!(
        rig.app.state().plugin_input(),
        "its view is a line to type into"
    );

    rig.press(KeyCode::Esc);
    assert!(matches!(rig.app.state().popup(), Popup::PluginView(_)));
    assert!(!rig.app.state().plugin_input());

    rig.press(KeyCode::Esc);
    assert!(matches!(rig.app.state().popup(), Popup::None));
}

#[test]
fn the_open_action_of_a_plugin_that_is_not_placed_says_so() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();

    rig.app.open_plugin_popup("hello");

    assert!(matches!(rig.app.state().popup(), Popup::None));
    assert!(rig.notification().unwrap().contains("not placed"));
}

#[test]
fn the_open_action_of_a_plugin_in_a_bar_gives_it_the_keyboard_there() {
    let mut rig = rig(Some(record(true, Some(Place::Explorer))));
    rig.wait_for_view();

    rig.app.open_plugin_popup("hello");

    assert!(matches!(rig.app.state().popup(), Popup::None));
    assert_eq!(rig.app.state().focus(), Focus::Plugin);
}

#[test]
fn the_open_action_of_a_plugin_that_is_not_running_says_so() {
    let mut rig = rig(None);
    rig.app.open_plugin_popup("hello");
    assert!(rig.notification().unwrap().contains("not running"));
}

#[test]
fn turning_off_a_plugin_whose_popup_is_open_closes_it() {
    let mut rig = rig(Some(record(true, Some(Place::Popup))));
    rig.wait_for_view();
    rig.app.open_plugin_popup("hello");

    rig.app.run_action(Action::TogglePlugin("hello".into()));

    assert!(matches!(rig.app.state().popup(), Popup::None));
}

// --- what is written ---

#[test]
fn a_place_is_written_in_snake_case_and_an_unknown_one_reads_as_unknown() {
    let mut state = SavedState::default();
    state.set_plugin("a", record(true, Some(Place::Bottom)));
    let dir = tempfile::tempdir().unwrap();
    saved_state::save(dir.path(), &state).unwrap();
    let text = fs::read_to_string(saved_state::path_in(dir.path())).unwrap();
    assert!(text.contains(r#""place": "bottom""#), "{text}");

    let from_the_future = text.replace("bottom", "sidebar_right");
    fs::write(saved_state::path_in(dir.path()), from_the_future).unwrap();
    let (read, warning) = saved_state::load(Some(dir.path()));
    assert!(warning.is_none());
    assert_eq!(read.plugin("a").unwrap().place, Some(Place::Unknown));
}

#[test]
fn a_record_without_a_place_writes_none() {
    let mut state = SavedState::default();
    state.set_plugin("a", record(true, None));
    let dir = tempfile::tempdir().unwrap();
    saved_state::save(dir.path(), &state).unwrap();
    assert!(
        !fs::read_to_string(saved_state::path_in(dir.path()))
            .unwrap()
            .contains("place")
    );
}

// --- the side pane, right of the editor ---

/// The column the text `needle` starts at on the screen, in the first row it is on.
fn column_of(screen: &str, needle: &str) -> Option<usize> {
    screen
        .lines()
        .find_map(|line| line.find(needle).map(|at| line[..at].chars().count()))
}

#[test]
fn a_plugin_placed_in_the_side_pane_is_right_of_the_editor() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();

    rig.app.place_plugin("hello", Place::Side);

    assert_eq!(rig.bar_of_hello(), Some(vec![hello()]));
    assert_eq!(
        rig.remembered().plugin("hello").unwrap().place,
        Some(Place::Side)
    );
    let screen = rig.screen();
    // The editor's line number is left of the plugin's frame, and the plugin is
    // right of the Explorer.
    let editor = column_of(&screen, "  1 ").unwrap();
    let plugin = column_of(&screen, "┌hello").expect(&screen);
    assert!(editor < plugin, "{screen}");
    assert!(column_of(&screen, "Explorer").unwrap() < editor, "{screen}");
    // The terminal's bar still spans under both.
    assert!(screen.contains("start"), "{screen}");
    // The layout file is as it was.
    assert!(!rig.app.state().base_layout().contains(&hello()));
    assert_eq!(rig.app.state().base_layout().right_bar_anchor(), None);
}

#[test]
fn a_placement_in_the_side_pane_is_applied_when_the_app_starts() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    assert_eq!(rig.bar_of_hello(), Some(vec![hello()]));
}

#[test]
fn the_side_pane_gives_way_on_a_narrow_screen_and_the_manager_says_so() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    let wide = rig.screen();
    assert!(wide.contains("┌hello"));

    rig.terminal = Terminal::new(TestBackend::new(90, 30)).unwrap();
    rig.app.handle_input(InputEvent::Resize(90, 30));
    let narrow = rig.screen();
    assert!(!narrow.contains("┌hello"), "{narrow}");

    rig.app.run_action(Action::ShowPlugins);
    let listed = rig.screen();
    assert!(listed.contains("side pane"), "{listed}");
    assert!(listed.contains("hidden: needs 100 columns"), "{listed}");
}

#[test]
fn a_second_plugin_in_the_side_pane_shares_its_bar() {
    use crate::ui::layout::LayoutTree;
    let base = LayoutTree::from_ron(&default_layout()).unwrap();
    let first = hello();
    let second = ComponentKind::Plugin(PluginViewId::new("other"));
    let one = base.with_view_beside_editor(&first, 40).unwrap();
    let two = one.with_view_beside_editor(&second, 40).unwrap();
    assert_eq!(
        two.bar_views(&second),
        Some(&[first.clone(), second.clone()][..])
    );
    assert_eq!(two.right_bar_anchor(), Some(first));
    // Not twice.
    assert!(two.with_view_beside_editor(&second, 40).is_none());
}

#[test]
fn a_menu_for_where_names_the_side_pane() {
    let mut rig = rig(Some(record(true, None)));
    rig.wait_for_view();
    rig.app.run_action(Action::ShowPlugins);
    rig.press(KeyCode::Char('p'));
    let screen = rig.screen();
    assert!(
        screen.contains("Put hello in the side pane (right of the editor)"),
        "{screen}"
    );
}

// --- moving the lines between panes ---

use crossterm::event::{MouseButton as Button, MouseEventKind as Kind};

impl Rig {
    fn mouse(&mut self, kind: Kind, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    /// Where the pane of `hello` starts, after drawing.
    fn side_column(&mut self) -> u16 {
        self.screen();
        self.app
            .state()
            .placements(self.app.state().screen())
            .into_iter()
            .find(|p| p.kind == hello())
            .unwrap()
            .area
            .x
    }
}

#[test]
fn dragging_the_line_beside_the_editor_resizes_the_side_pane_and_it_is_remembered() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    let before = rig.side_column();
    assert_eq!(rig.app.state().side_percent(), 40);

    // Take hold of the border between the editor and the pane, and move it left.
    rig.mouse(Kind::Down(Button::Left), before, 5);
    rig.mouse(Kind::Drag(Button::Left), before - 20, 5);
    rig.mouse(Kind::Up(Button::Left), before - 20, 5);

    let after = rig.side_column();
    assert!(
        after < before,
        "the pane is wider: {after} against {before}"
    );
    assert!(rig.app.state().side_percent() > 40);
    assert_eq!(
        rig.remembered().side_percent(),
        Some(rig.app.state().side_percent())
    );
    // The layout file is as it was.
    assert_eq!(rig.app.state().base_layout().right_bar_anchor(), None);
}

#[test]
fn the_side_pane_never_gets_less_than_15_percent_or_more_than_70() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    let at = rig.side_column();
    rig.mouse(Kind::Down(Button::Left), at, 5);
    rig.mouse(Kind::Drag(Button::Left), 119, 5);
    assert_eq!(rig.app.state().side_percent(), 15);
    rig.mouse(Kind::Drag(Button::Left), 0, 5);
    assert_eq!(rig.app.state().side_percent(), 70);
    rig.mouse(Kind::Up(Button::Left), 0, 5);
}

#[test]
fn a_press_that_is_not_on_the_line_does_not_move_it() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    let at = rig.side_column();
    rig.mouse(Kind::Down(Button::Left), at - 10, 5);
    rig.mouse(Kind::Drag(Button::Left), at - 30, 5);
    rig.mouse(Kind::Up(Button::Left), at - 30, 5);
    assert_eq!(rig.app.state().side_percent(), 40);
    assert_eq!(rig.remembered().side_percent(), None);
}

#[test]
fn the_palette_commands_make_the_side_pane_wider_and_narrower_in_steps_of_five() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    rig.screen();
    rig.app.run_action(Action::SidePaneWider);
    assert_eq!(rig.app.state().side_percent(), 45);
    rig.app.run_action(Action::SidePaneNarrower);
    rig.app.run_action(Action::SidePaneNarrower);
    assert_eq!(rig.app.state().side_percent(), 35);
    assert_eq!(rig.remembered().side_percent(), Some(35));

    // Without a pane there, it says so.
    let mut none = rig_with(&default_layout(), First::Echo, None);
    none.app.run_action(Action::SidePaneWider);
    assert_eq!(
        none.notification().as_deref(),
        Some("there is no pane right of the editor")
    );
}

#[test]
fn a_remembered_width_is_applied_when_the_app_starts() {
    let config = tempfile::tempdir().unwrap();
    let mut state = SavedState::default();
    state.set_plugin("hello", record(true, Some(Place::Side)));
    state.set_side_percent(55);
    saved_state::save(config.path(), &state).unwrap();
    let (loaded, _) = saved_state::load(Some(config.path()));
    let mut ui = AppState::new(Editor::new(Buffer::from_text("")));
    ui.set_layout(LayoutTree::from_ron(&default_layout()).unwrap());
    let app = App::new(ui).with_saved_state(loaded);
    assert_eq!(app.state().side_percent(), 55);
}

#[test]
fn the_line_between_two_editor_panes_is_dragged_with_the_mouse() {
    let mut rig = rig(None);
    rig.app.run_action(Action::SplitRight);
    rig.screen();
    let areas = rig.app.state().workspace().pane_areas();
    let divider = areas[1].x;
    rig.mouse(Kind::Down(Button::Left), divider, 5);
    rig.mouse(Kind::Drag(Button::Left), divider + 20, 5);
    rig.mouse(Kind::Up(Button::Left), divider + 20, 5);
    rig.screen();
    let after = rig.app.state().workspace().pane_areas();
    assert!(
        after[0].width > areas[0].width,
        "{after:?} against {areas:?}"
    );
    assert_eq!(
        after[0].width + after[1].width,
        areas[0].width + areas[1].width
    );

    // The keyboard does it too.
    rig.app.run_action(Action::PaneShrink);
    rig.screen();
    let again = rig.app.state().workspace().pane_areas();
    assert!(again[1].width < after[1].width);
}

// --- showing and hiding the side pane ---

fn chord(rig: &mut Rig, keys: &[(KeyCode, KeyModifiers)]) {
    for (code, modifiers) in keys {
        rig.app
            .handle_input(InputEvent::Key(KeyEvent::new(*code, *modifiers)));
    }
}

#[test]
fn toggling_the_side_pane_gives_the_space_back_and_takes_it_again_and_is_remembered() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    let editor_width = |rig: &mut Rig| {
        rig.screen();
        rig.app
            .state()
            .placements(rig.app.state().screen())
            .into_iter()
            .find(|p| p.kind == ComponentKind::Editor)
            .unwrap()
            .area
            .width
    };
    let with = editor_width(&mut rig);
    assert!(rig.screen().contains("┌hello"));

    rig.app.run_action(Action::ToggleSidePane);

    assert!(!rig.screen().contains("┌hello"), "the pane is gone");
    assert!(editor_width(&mut rig) > with, "and the editor has its room");
    assert_eq!(rig.remembered().side_shown(), Some(false));

    rig.app.run_action(Action::ToggleSidePane);
    assert!(rig.screen().contains("┌hello"));
    assert_eq!(editor_width(&mut rig), with);
    assert_eq!(rig.remembered().side_shown(), Some(true));
}

#[test]
fn ctrl_k_p_toggles_it() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    assert!(rig.screen().contains("┌hello"));
    chord(
        &mut rig,
        &[
            (KeyCode::Char('k'), KeyModifiers::CONTROL),
            (KeyCode::Char('p'), KeyModifiers::NONE),
        ],
    );
    assert!(!rig.screen().contains("┌hello"));
}

#[test]
fn hiding_the_pane_takes_the_keyboard_back_and_a_remembered_hide_applies_at_start() {
    let mut rig = rig(Some(record(true, Some(Place::Side))));
    rig.wait_for_view();
    rig.app.run_action(Action::FocusPluginView("hello".into()));
    assert_eq!(rig.app.state().focus(), Focus::Plugin);
    rig.app.run_action(Action::ToggleSidePane);
    assert_eq!(rig.app.state().focus(), Focus::Editor);

    let mut saved = SavedState::default();
    saved.set_plugin("hello", record(true, Some(Place::Side)));
    saved.set_side_shown(false);
    let mut ui = AppState::new(Editor::new(Buffer::from_text("")));
    ui.set_layout(LayoutTree::from_ron(&default_layout()).unwrap());
    let app = App::new(ui).with_saved_state(saved);
    assert!(!app.state().side_pane_shown());
}

#[test]
fn with_no_pane_there_it_says_where_to_put_one() {
    let mut rig = rig_with(&default_layout(), First::Echo, None);
    rig.app.run_action(Action::ToggleSidePane);
    let note = rig.notification().unwrap();
    assert!(
        note.contains("there is no pane right of the editor"),
        "{note}"
    );
    assert!(rig.remembered().side_shown().is_none());
}

#[test]
fn a_right_bar_of_the_layout_file_is_toggled_too() {
    let layout = "Col([Row([Fixed(30, SideBar(views: [Explorer])), Row([Pane(view: Editor), \
                  Percent(40, RightBar(views: [Plugin(\"hello\")]))])]), \
                  Fixed(8, BottomBar(views: [Terminal]))])";
    let mut rig = rig_with(layout, First::Echo, Some(record(true, None)));
    rig.wait_for_view();
    assert!(rig.screen().contains("┌hello"));
    rig.app.run_action(Action::ToggleSidePane);
    assert!(!rig.screen().contains("┌hello"));
}
