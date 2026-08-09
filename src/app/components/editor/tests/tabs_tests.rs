use std::path::PathBuf;

use super::{StartupArgs, Tabs};
use crate::app::components::editor::file_content::FileContent;

fn new_tabs() -> Tabs {
    Tabs::new(&StartupArgs::new(PathBuf::from(".")))
}

fn file(name: &str) -> FileContent {
    FileContent::new(name, "")
}

fn names(tabs: &Tabs) -> Vec<&str> {
    tabs.files.iter().map(|f| f.name()).collect()
}

#[test]
fn open_adds_and_activates_new_file() {
    let mut tabs = new_tabs();
    tabs.open(file("a.rs"));
    tabs.open(file("b.rs"));
    assert_eq!(names(&tabs), vec!["a.rs", "b.rs"]);
    assert_eq!(tabs.active, 1);
}

#[test]
fn open_existing_activates_without_duplicate() {
    let mut tabs = new_tabs();
    tabs.open(file("a.rs"));
    tabs.open(file("b.rs"));
    tabs.open(file("a.rs"));
    assert_eq!(names(&tabs), vec!["a.rs", "b.rs"]);
    assert_eq!(tabs.active, 0);
}

#[test]
fn open_first_file_activates_it() {
    let mut tabs = new_tabs();
    tabs.open(file("main.rs"));
    assert_eq!(tabs.active, 0);
}

#[test]
fn switch_tab_wraps_forward() {
    let mut tabs = new_tabs();
    tabs.open(file("a.rs"));
    tabs.open(file("b.rs"));
    assert_eq!(tabs.active, 1);

    tabs.switch_tab(1);
    assert_eq!(tabs.active, 0);
}

#[test]
fn switch_tab_wraps_backward() {
    let mut tabs = new_tabs();
    tabs.open(file("a.rs"));
    tabs.open(file("b.rs"));
    tabs.switch_tab(-1);
    assert_eq!(tabs.active, 0);

    tabs.switch_tab(-1);
    assert_eq!(tabs.active, 1);
}

#[test]
fn switch_tab_is_noop_without_files() {
    let mut tabs = new_tabs();
    tabs.switch_tab(1);
    tabs.switch_tab(-1);
    assert_eq!(tabs.active, 0);
}
