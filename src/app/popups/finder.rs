//! The file finder.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;
use crate::app::state::Focus;
use crate::buffer::Position;
use crate::components::finder::{self, Finder, Target};
use crate::components::palette;
use crate::ui::layout::Axis;

impl App {
    /// Opens the file finder on the project and starts the walk that feeds it.
    pub(in crate::app) fn open_finder(&mut self) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("the file finder needs the event loop, which is not running");
            return;
        };
        let root = self.project_root();
        let scan = self.background.next_scan();
        let handle = finder::start_scan(root.clone(), scan, events);
        let recent = self.recent_files_under(&root);
        self.state
            .open_finder(Finder::new(root, scan, Some(handle)).with_recent(&recent));
    }

    /// The files opened lately that are in the project, latest first, as paths
    /// under `root` (a file opened by a relative path is found by where it is).
    fn recent_files_under(&self, root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let canonical_root = crate::paths::canonical(root);
        self.state
            .workspace()
            .recent_paths()
            .iter()
            .filter_map(|path| {
                if path.starts_with(root) {
                    return Some(path.clone());
                }
                let relative = crate::paths::canonical(path)
                    .strip_prefix(&canonical_root)
                    .ok()?
                    .to_path_buf();
                Some(root.join(relative))
            })
            .collect()
    }

    pub(in crate::app) fn handle_finder_key(&mut self, key: KeyEvent) {
        // Alt+Enter opens the file in a pane of its own, beside the current one.
        let in_split = key.kind != KeyEventKind::Release
            && key.code == KeyCode::Enter
            && key.modifiers.contains(KeyModifiers::ALT);
        let command = match in_split {
            true => Some(palette::Command::Run),
            false => palette::Command::from_key(key),
        };
        let Some(command) = command else {
            return;
        };
        let Some(mut finder) = self.state.take_finder() else {
            return;
        };
        let mut keep_open = true;
        let mut chosen = None;
        match command {
            palette::Command::Insert(c) => finder.insert(c),
            palette::Command::Backspace => finder.backspace(),
            palette::Command::ClearQuery => finder.clear_query(),
            palette::Command::Previous => finder.select_previous(),
            palette::Command::Next => finder.select_next(),
            palette::Command::Cancel => keep_open = false,
            palette::Command::Run => {
                chosen = finder.selected_target();
                keep_open = chosen.is_none();
            }
        }
        if keep_open {
            self.state.open_finder(finder);
        }
        self.needs_redraw = true;
        let Some(target) = chosen else {
            return;
        };
        self.open_target(target, in_split);
    }

    /// Opens what the finder chose, and goes to the line and column it named.
    /// The editor gets the keyboard: unlike the explorer, the finder is for
    /// getting to a file.
    fn open_target(&mut self, target: Target, in_split: bool) {
        self.state.set_focus(Focus::Editor);
        let opened = match in_split {
            true => self
                .state
                .workspace_mut()
                .open_path_in_split(&target.path, Axis::Horizontal),
            false => self.state.workspace_mut().open_path(&target.path),
        };
        if let Err(error) = opened {
            self.state.notify_error(error.to_string());
            return;
        }
        let Some(line) = target.line else {
            return;
        };
        self.state.edit(|editor| {
            let at = Position::new(line - 1, target.column.map_or(0, |column| column - 1));
            let at = editor.buffer().clamp_position(at);
            editor.go_to(at);
        });
    }
}
