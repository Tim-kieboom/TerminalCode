use std::path::PathBuf;

use super::{StartupArgs, Tabs};

fn new_tabs() -> Tabs {
    Tabs::new(&StartupArgs::new(PathBuf::from(".")))
}

#[test]
fn open_adds_and_activates_new_file() {
    let mut tabs = new_tabs();
    tabs.open("a.rs");
    tabs.open("b.rs");
    assert_eq!(tabs.files, vec!["a.rs", "b.rs"]);
    assert_eq!(tabs.active, 1);
}

#[test]
fn open_existing_activates_without_duplicate() {
    let mut tabs = new_tabs();
    tabs.open("a.rs");
    tabs.open("b.rs");
    tabs.open("a.rs");
    assert_eq!(tabs.files, vec!["a.rs", "b.rs"]);
    assert_eq!(tabs.active, 0);
}

#[test]
fn open_first_file_activates_it() {
    let mut tabs = new_tabs();
    tabs.open("main.rs");
    assert_eq!(tabs.active, 0);
}
