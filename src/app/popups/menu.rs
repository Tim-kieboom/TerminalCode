//! The context menu on an explorer row. Its items run the same actions as the
//! keys do, on the path the menu was opened on.

use std::fs;
use std::path::Path;

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use crate::app::App;
use crate::app::keys::{EDITOR_CONTEXTS, EXPLORER_CONTEXTS};
use crate::app::state::Focus;
use crate::components::menu_bar::MenuBar;
use crate::components::popups::menu::{self, Entry, Menu};
use crate::config::ConfigFile;
use crate::event::action::Action;
use crate::ui::ComponentKind;

impl App {
    /// Opens the menu on what has the keyboard: the explorer's selected row,
    /// just below it, or the editor's cursor.
    pub(in crate::app) fn open_context_menu(&mut self) {
        if self.state.focus() == Focus::Explorer {
            let anchor = self.state.explorer().selected_cell();
            self.open_menu_at(anchor);
            return;
        }
        let Some(anchor) = self.state.editor().cursor_cell() else {
            self.state.notify("the cursor is not on screen");
            return;
        };
        self.state.set_focus(Focus::Editor);
        self.open_editor_menu(anchor);
    }

    /// A right click on the editor text: moves the cursor there (unless it is in
    /// the selection) and opens the menu. Returns whether the click was on text.
    pub(in crate::app) fn right_click_editor(&mut self, column: u16, row: u16) -> bool {
        self.state.set_focus(Focus::Editor);
        match self.state.workspace_mut().context_press(column, row) {
            Ok(true) => {
                self.open_editor_menu(Position::new(column, row));
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.state.notify_error(error.to_string());
                true
            }
        }
    }

    fn open_editor_menu(&mut self, anchor: Position) {
        self.state
            .open_menu(Menu::on_editor(self.editor_entries(), anchor));
    }

    /// A right click in the explorer: selects the row under the pointer (the
    /// project folder when there is none) and opens the menu there.
    pub(in crate::app) fn right_click_explorer(&mut self, column: u16, row: u16) {
        self.state.set_focus(Focus::Explorer);
        self.state.explorer_mut().select_at_or_root(column, row);
        self.open_menu_at(Position::new(column, row));
    }

    /// Opens the menu on the selected row, hanging from `anchor`.
    fn open_menu_at(&mut self, anchor: Position) {
        let explorer = self.state.explorer();
        let Some(row) = explorer.selected_row() else {
            self.state.notify("there is nothing here to open a menu on");
            return;
        };
        let target = row.path.clone();
        let is_root = explorer.root_path() == Some(target.as_path());
        self.state
            .open_menu(Menu::new(self.explorer_entries(is_root), anchor, target));
    }

    pub(in crate::app) fn handle_menu_key(&mut self, key: KeyEvent) {
        let Some(command) = menu::Command::from_key(key) else {
            return;
        };
        let Some(mut menu) = self.state.take_menu() else {
            return;
        };
        self.needs_redraw = true;
        match menu.key(command) {
            menu::Step::Stay => self.state.open_menu(menu),
            menu::Step::Run(action) => self.run_menu_action(&menu, action),
            menu::Step::Close | menu::Step::Outside => {}
        }
    }

    /// A mouse event while the menu is open. Returns whether the menu used
    /// it up: a press outside only closes the menu and is then handled as if
    /// the menu had not been there, and the wheel just closes it.
    pub(in crate::app) fn handle_menu_mouse(&mut self, event: MouseEvent) -> bool {
        let Some(mut menu) = self.state.take_menu() else {
            return false;
        };
        let position = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::Down(_) if !menu.contains(position) => return false,
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {}
            MouseEventKind::Down(MouseButton::Left) => match menu.click(position) {
                menu::Step::Run(action) => self.run_menu_action(&menu, action),
                _ => self.state.open_menu(menu),
            },
            _ => self.state.open_menu(menu),
        }
        true
    }

    /// Runs a menu item: on the path the menu was opened on, or, for a menu on
    /// the editor, on the editor.
    fn run_menu_action(&mut self, menu: &Menu, action: Action) {
        match menu.target() {
            Some(target) => self.run_on_target(target, action),
            None if menu.keeps_focus() => self.run_action(action),
            None => {
                self.state.set_focus(Focus::Editor);
                self.run_action(action);
            }
        }
    }

    /// Runs `action` on `target`, which the menu was opened on. The actions
    /// work on the explorer's selection, so the target is selected first; if
    /// it is gone, or no longer in the tree, nothing runs, and the selection
    /// never stands in for it.
    fn run_on_target(&mut self, target: &Path, action: Action) {
        if fs::symlink_metadata(target).is_err() {
            self.state
                .notify_error(format!("{} no longer exists", name_of(target)));
            return;
        }
        if let Err(error) = self.state.explorer_mut().reveal(target) {
            self.state.notify_error(error.to_string());
        }
        let selected = self.state.explorer().selected_row();
        if selected.map(|row| row.path.as_path()) != Some(target) {
            self.state
                .notify_error(format!("{} is no longer in the tree", name_of(target)));
            return;
        }
        self.run_action(action);
    }
}

/// What the menu on the editor text offers, with the keys each item has there.
impl App {
    fn editor_entries(&self) -> Vec<Entry> {
        let item = |label: &'static str, action: Action| {
            let keys = self.keyboard.keys_for(&action, &EDITOR_CONTEXTS);
            Entry::Item {
                label: label.into(),
                action,
                keys,
            }
        };
        vec![
            item("Cut", Action::Cut),
            item("Copy", Action::Copy),
            item("Paste", Action::Paste),
            Entry::Separator,
            item("Select All", Action::SelectAll),
            Entry::Separator,
            item("Find in File", Action::FindInFile),
            item("Save", Action::Save),
        ]
    }
}

/// What the menu on an explorer row offers, with the keys each item has in
/// the explorer. The project folder itself cannot be renamed or deleted, so
/// it only gets the entries that add to it.
impl App {
    fn explorer_entries(&self, is_root: bool) -> Vec<Entry> {
        let item = |label: &'static str, action: Action| {
            let keys = self.keyboard.keys_for(&action, &EXPLORER_CONTEXTS);
            Entry::Item {
                label: label.into(),
                action,
                keys,
            }
        };
        let mut entries = vec![
            item("New File", Action::CreateFile),
            item("New Folder", Action::CreateFolder),
        ];
        if !is_root {
            entries.extend([
                item("Rename", Action::Rename),
                Entry::Separator,
                item("Delete", Action::DeleteSelected),
            ]);
        }
        entries
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// What the menu on a tab offers. The tab was brought to the front by the click,
/// so the items work on the tab in front.
impl App {
    fn tab_entries(&self) -> Vec<Entry> {
        let item = |label: &'static str, action: Action| {
            let keys = self.keyboard.keys_for(&action, &EDITOR_CONTEXTS);
            Entry::Item {
                label: label.into(),
                action,
                keys,
            }
        };
        let pin = match self.state.workspace().active_tab_is_pinned() {
            true => item("Unpin Tab", Action::TogglePinTab),
            false => item("Pin Tab", Action::TogglePinTab),
        };
        vec![
            item("Close", Action::CloseTab),
            item("Close Others", Action::CloseOtherTabs),
            item("Close to the Left", Action::CloseTabsToLeft),
            item("Close to the Right", Action::CloseTabsToRight),
            item("Close All", Action::CloseAllTabs),
            Entry::Separator,
            pin,
        ]
    }

    /// A right click on a tab: brings the tab to the front and opens the menu
    /// just below the strip. Returns whether the click was on a tab.
    pub(in crate::app) fn right_click_tab(&mut self, column: u16, row: u16) -> bool {
        if !self.state.workspace_mut().tab_context_press(column, row) {
            return false;
        }
        self.state.set_focus(Focus::Editor);
        let entries = self.tab_entries();
        self.state
            .open_menu(Menu::on_editor(entries, Position::new(column, row)));
        true
    }
}

/// The main menu: the icon in the top row, `f10` or the palette opens it.
impl App {
    /// Opens the main menu under the icon, or at the top left when the top row is
    /// hidden. The keyboard stays where it was, and closing the menu leaves it there.
    pub(in crate::app) fn open_main_menu(&mut self) {
        let screen = self.state.screen();
        let icon = self
            .state
            .placements(screen)
            .iter()
            .find(|placement| placement.kind == ComponentKind::MenuBar)
            .map(|placement| MenuBar::icon_rect(placement.area));
        let entries = self.main_menu_entries();
        let menu = match icon {
            Some(icon) => Menu::on_app(entries, Position::new(icon.x, icon.y)),
            // No row to hang from: the menu starts at the top left.
            None => {
                Menu::on_app(entries, Position::new(screen.x, screen.y)).starting_on_anchor_row()
            }
        };
        self.state.open_menu(menu);
        self.needs_redraw = true;
    }

    /// What the main menu offers. Every item is an action the palette has too, with
    /// the keys it has now.
    fn main_menu_entries(&mut self) -> Vec<Entry> {
        let recent = self.recent_entries();
        let item = |label: &'static str, action: Action| {
            let keys = self.keyboard.keys_for(&action, &EDITOR_CONTEXTS);
            Entry::Item {
                label: label.into(),
                action,
                keys,
            }
        };
        let group = |label: &'static str, entries: Vec<Entry>| Entry::Group {
            label: label.into(),
            entries,
        };
        vec![
            group(
                "File",
                vec![
                    item("New File", Action::NewFile),
                    item("Open File", Action::OpenFile),
                    item("Open Folder", Action::OpenFolder),
                    group("Open Recent", recent),
                    Entry::Separator,
                    item("Save", Action::Save),
                    item("Save All", Action::SaveAll),
                ],
            ),
            group(
                "View",
                vec![
                    item("Toggle Explorer", Action::ToggleExplorer),
                    item("Toggle Terminal", Action::ToggleTerminal),
                    item("Toggle Status Bar", Action::ToggleStatusBar),
                    item("Toggle Menu Bar", Action::ToggleMenuBar),
                    item("Show Errors", Action::ShowErrors),
                    item("Toggle Mouse", Action::ToggleMouse),
                    Entry::Separator,
                    item("Plugins", Action::ShowPlugins),
                ],
            ),
            group(
                "Settings",
                vec![
                    item("Open Settings", Action::OpenConfig(ConfigFile::Settings)),
                    item("Open Keymap", Action::OpenConfig(ConfigFile::Keymap)),
                    item("Open Layout", Action::OpenConfig(ConfigFile::Layout)),
                    item("Open Theme", Action::OpenConfig(ConfigFile::Theme)),
                    item("Reload", Action::Reload),
                    Entry::Separator,
                    item("Choose Theme", Action::ChooseTheme),
                    item("New Theme", Action::NewTheme),
                ],
            ),
            Entry::Separator,
            item("Quit", Action::Quit),
        ]
    }

    /// Shows or hides the top row. A layout without one has nothing to show, and
    /// saying so beats remembering a hidden row that was never there.
    pub(in crate::app) fn toggle_menu_bar(&mut self) {
        if !self.state.layout().contains(&ComponentKind::MenuBar) {
            self.state.notify(
                "the layout has no menu bar: add Fixed(1, Pane(view: MenuBar)) at the top of layout.ron",
            );
            return;
        }
        self.state.toggle_menu_bar();
    }

    /// A click on the menu icon opens the main menu. Returns whether it was one.
    pub(in crate::app) fn handle_menu_bar_mouse(&mut self, event: MouseEvent) -> bool {
        if event.kind != MouseEventKind::Down(MouseButton::Left) || self.modal_open() {
            return false;
        }
        let at = Position::new(event.column, event.row);
        let on_icon = self
            .state
            .placements(self.state.screen())
            .iter()
            .filter(|placement| placement.kind == ComponentKind::MenuBar)
            .any(|placement| MenuBar::icon_rect(placement.area).contains(at));
        if on_icon {
            self.open_main_menu();
        }
        on_icon
    }
}
