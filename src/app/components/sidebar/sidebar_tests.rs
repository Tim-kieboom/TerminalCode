use super::{SideBar, SideBarSelect};
use crate::StartupArgs;

fn sidebar() -> SideBar {
    SideBar::new(&StartupArgs::new(".".into()))
}

#[test]
fn new_selects_explorer() {
    assert_eq!(sidebar().select, SideBarSelect::Explorer);
}

#[test]
fn switch_tab_cycles_forward() {
    let mut sidebar = sidebar();
    sidebar.switch_tab(1);
    assert_eq!(sidebar.select, SideBarSelect::Debugger);
    sidebar.switch_tab(1);
    assert_eq!(sidebar.select, SideBarSelect::Explorer);
}

#[test]
fn switch_tab_cycles_backward() {
    let mut sidebar = sidebar();
    sidebar.switch_tab(-1);
    assert_eq!(sidebar.select, SideBarSelect::Debugger);
}

#[test]
fn switch_tab_multiple_steps_wraps() {
    let mut sidebar = sidebar();
    sidebar.switch_tab(3);
    assert_eq!(sidebar.select, SideBarSelect::Debugger);
}
