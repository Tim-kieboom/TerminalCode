//! A right click on a plugin's rows: the plugin is told which row, and its answer, the items
//! of a menu, comes to the app as a notice.

use super::super::fixtures::{Echo, First, TYPE_AT};
use super::{Notice, rig};
use crate::plugin_api::{MAX_MENU_ITEMS, ToPlugin, WireMenuItem};

/// `context_click` is the only message that starts with `co` (`call` and `call_cancelled` have
/// an `a` after the `c`).
const AFTER_C: u32 = TYPE_AT + 1;

const MENU: &str = r#"{"type":"menu","items":[{"type":"action","action":"open"},{"type":"separator"},{"type":"action","action":"stage"}]}"#;

#[test]
fn the_plugin_is_told_the_row_and_its_menu_reaches_the_app() {
    let rig = rig();
    let echo = Echo::new(First::Echo).asks_on_byte(AFTER_C, b'o', MENU);
    rig.runtime.load("git", echo.build(), []);
    rig.next();

    rig.runtime.send("git", ToPlugin::ContextClick { row: 3 });

    // The plugin says what it heard, and its menu.
    let mut heard = None;
    let mut menu = None;
    for _ in 0..2 {
        match rig.next() {
            Notice::View { content, .. } => heard = Some(content),
            Notice::Menu { plugin, items } => menu = Some((plugin, items)),
            other => panic!("{other:?}"),
        }
    }
    let heard = format!("{heard:?}");
    assert!(heard.contains(r#"context_click\",\"row\":3"#), "{heard}");
    let (plugin, items) = menu.expect("a menu");
    assert_eq!(&*plugin, "git");
    assert_eq!(
        items,
        [
            WireMenuItem::action("open"),
            WireMenuItem::Separator,
            WireMenuItem::action("stage")
        ]
    );
}

#[test]
fn a_menu_has_at_most_so_many_items() {
    let rig = rig();
    let items: Vec<String> = (0..MAX_MENU_ITEMS + 3)
        .map(|n| format!(r#"{{"type":"action","action":"a{n}"}}"#))
        .collect();
    let menu = format!(r#"{{"type":"menu","items":[{}]}}"#, items.join(","));
    assert!(
        menu.len() < 1000,
        "the fixture has a kilobyte for it: {}",
        menu.len()
    );
    let echo = Echo::new(First::Echo).asks_on_byte(AFTER_C, b'o', &menu);
    rig.runtime.load("git", echo.build(), []);
    rig.next();

    rig.runtime.send("git", ToPlugin::ContextClick { row: 0 });

    let mut cut = None;
    for _ in 0..2 {
        if let Notice::Menu { items, .. } = rig.next() {
            cut = Some(items.len());
        }
    }
    assert_eq!(cut, Some(MAX_MENU_ITEMS));
}
