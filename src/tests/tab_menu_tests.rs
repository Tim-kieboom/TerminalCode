//! The right click menu on a tab, and pinned tabs.

use std::fs;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::initial_state;

struct Rig {
    app: App,
    _dir: tempfile::TempDir,
}

/// An app with `count` files open, `f1.txt` to `fN.txt`, the first in front.
fn rig(count: usize) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = (1..=count)
        .map(|n| {
            let path = dir.path().join(format!("f{n}.txt"));
            fs::write(&path, format!("text {n}\n")).unwrap();
            path
        })
        .collect();
    let mut state = initial_state(&files).unwrap();
    state.open_project(dir.path()).unwrap();
    let mut rig = Rig {
        app: App::new(state),
        _dir: dir,
    };
    rig.screen();
    rig
}

impl Rig {
    fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }

    fn press(&mut self, code: KeyCode) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    }

    /// The screen cell where `needle` starts.
    fn cell_of(&mut self, needle: &str) -> (u16, u16) {
        let screen = self.screen();
        for (row, line) in screen.lines().enumerate() {
            let line = line.trim_matches('"');
            if let Some(byte) = line.find(needle) {
                return (line[..byte].chars().count() as u16, row as u16);
            }
        }
        panic!("the screen does not show {needle:?}: {screen}");
    }

    fn right_click(&mut self, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    fn right_click_tab(&mut self, name: &str) {
        let (column, row) = self.cell_of(name);
        self.right_click(column, row);
    }

    /// Chooses the item at `index` of the open menu (separators do not count).
    fn choose(&mut self, index: usize) {
        for _ in 0..index {
            self.press(KeyCode::Down);
        }
        self.press(KeyCode::Enter);
    }

    fn names(&self) -> Vec<String> {
        self.app.state().workspace().tab_names_for_test()
    }

    fn front(&self) -> String {
        self.app.state().editor().display_name()
    }

    fn told(&self) -> String {
        self.app.state().notifications().texts().join("|")
    }
}

#[test]
fn a_right_click_on_a_tab_brings_it_to_the_front_and_opens_the_menu() {
    let mut rig = rig(4);

    rig.right_click_tab("f3.txt");

    assert_eq!(rig.front(), "f3.txt");
    let labels = rig.app.state().menu().unwrap().labels_for_test();
    assert_eq!(
        labels,
        [
            "Close",
            "Close Others",
            "Close to the Left",
            "Close to the Right",
            "Close All",
            "Pin Tab",
        ]
    );
}

#[test]
fn a_right_click_on_the_text_is_still_the_editor_menu() {
    let mut rig = rig(2);
    let (column, row) = rig.cell_of("text 1");

    rig.right_click(column, row);

    let labels = rig.app.state().menu().unwrap().labels_for_test();
    assert!(labels.contains(&"Paste"), "{labels:?}");
}

#[test]
fn close_closes_the_tab_that_was_clicked() {
    let mut rig = rig(4);
    rig.right_click_tab("f2.txt");

    rig.choose(0);

    assert_eq!(rig.names(), ["f1.txt", "f3.txt", "f4.txt"]);
}

#[test]
fn close_others_keeps_only_the_tab_that_was_clicked() {
    let mut rig = rig(4);
    rig.right_click_tab("f3.txt");

    rig.choose(1);

    assert_eq!(rig.names(), ["f3.txt"]);
}

#[test]
fn close_to_the_left_and_to_the_right_close_the_tabs_on_that_side() {
    let mut rig = rig(5);
    rig.right_click_tab("f3.txt");
    rig.choose(2);
    assert_eq!(rig.names(), ["f3.txt", "f4.txt", "f5.txt"]);

    rig.right_click_tab("f4.txt");
    rig.choose(3);

    assert_eq!(rig.names(), ["f3.txt", "f4.txt"]);
}

#[test]
fn close_to_the_left_of_the_first_tab_closes_nothing() {
    let mut rig = rig(3);
    rig.right_click_tab("f1.txt");

    rig.choose(2);

    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);
}

#[test]
fn close_all_closes_every_tab() {
    let mut rig = rig(3);
    rig.right_click_tab("f2.txt");

    rig.choose(4);

    assert!(rig.names().is_empty());
}

#[test]
fn pin_moves_the_tab_to_the_left_and_marks_it() {
    let mut rig = rig(4);
    rig.right_click_tab("f3.txt");

    rig.choose(5);

    assert_eq!(rig.names(), ["f3.txt", "f1.txt", "f2.txt", "f4.txt"]);
    assert_eq!(rig.front(), "f3.txt");
    let screen = rig.screen();
    assert!(screen.contains("◆ f3.txt"), "{screen}");
    assert!(!screen.contains("◆ f1.txt"), "{screen}");
    assert!(rig.told().contains("tab pinned"), "{}", rig.told());
}

#[test]
fn a_second_pin_goes_after_the_first_and_unpin_goes_after_the_pinned() {
    let mut rig = rig(4);
    rig.right_click_tab("f3.txt");
    rig.choose(5);
    rig.right_click_tab("f4.txt");
    rig.choose(5);
    assert_eq!(rig.names(), ["f3.txt", "f4.txt", "f1.txt", "f2.txt"]);

    rig.right_click_tab("f3.txt");
    let labels = rig.app.state().menu().unwrap().labels_for_test();
    assert!(labels.contains(&"Unpin Tab"), "{labels:?}");
    rig.choose(5);

    assert_eq!(rig.names(), ["f4.txt", "f3.txt", "f1.txt", "f2.txt"]);
    assert!(rig.told().contains("tab unpinned"), "{}", rig.told());
}

#[test]
fn pinned_tabs_are_left_alone_by_the_close_many_items() {
    let mut rig = rig(5);
    rig.right_click_tab("f2.txt");
    rig.choose(5);
    // Order: f2 (pinned), f1, f3, f4, f5. Click f4.
    rig.right_click_tab("f4.txt");
    rig.choose(1);
    assert_eq!(rig.names(), ["f2.txt", "f4.txt"]);

    rig.right_click_tab("f4.txt");
    rig.choose(2);
    assert_eq!(
        rig.names(),
        ["f2.txt", "f4.txt"],
        "the pinned one on the left stays"
    );

    rig.right_click_tab("f4.txt");
    rig.choose(4);
    assert_eq!(rig.names(), ["f2.txt"]);
}

#[test]
fn a_pinned_tab_needs_an_explicit_close() {
    let mut rig = rig(2);
    rig.right_click_tab("f1.txt");
    rig.choose(5);

    rig.right_click_tab("f1.txt");
    rig.choose(0);

    assert_eq!(rig.names(), ["f2.txt"]);
}

#[test]
fn unsaved_tabs_keep_the_warning_when_closed_from_the_menu() {
    let mut rig = rig(3);
    rig.right_click_tab("f1.txt");
    rig.press(KeyCode::Esc);
    rig.press(KeyCode::Char('x'));
    rig.right_click_tab("f2.txt");

    rig.choose(2);

    assert_eq!(rig.names(), ["f1.txt [+]", "f2.txt", "f3.txt"]);
    assert!(rig.told().contains("unsaved changes"), "{}", rig.told());
    rig.right_click_tab("f2.txt");
    rig.choose(2);
    assert_eq!(rig.names(), ["f2.txt", "f3.txt"]);
}

#[test]
fn the_new_actions_are_in_the_palette() {
    use crate::event::action::Action;

    for action in [
        Action::CloseTabsToLeft,
        Action::CloseTabsToRight,
        Action::TogglePinTab,
    ] {
        assert!(Action::palette_actions().contains(&action), "{action:?}");
        let title = action.title().unwrap();
        assert!(title.starts_with("Tab: "), "{title}");
    }
}
