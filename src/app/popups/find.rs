//! Find in the open file.

use crossterm::event::KeyEvent;

use super::SavedQuery;
use crate::app::App;
use crate::components::find::{self, Find};

impl App {
    /// Opens the find bar for the open file. A selected word becomes the query;
    /// otherwise the last one is used. The first match from the cursor is
    /// selected.
    pub(in crate::app) fn open_find(&mut self) {
        let (query, origin) = {
            let editor = self.state.editor();
            let selection = *editor.selections().primary();
            let seeded = Find::seed(editor.buffer(), selection.start(), selection.end());
            (
                seeded.unwrap_or_else(|| self.remembered.find.query.clone()),
                selection.start(),
            )
        };
        let find = Find::open(
            self.state.editor().buffer(),
            origin,
            query,
            self.remembered.find.options,
        );
        self.state.workspace_mut().set_find_bar(true);
        self.select_found(&find);
        self.remembered.find = SavedQuery::new(find.query(), find.options());
        self.state.open_find(find);
    }

    pub(in crate::app) fn handle_find_key(&mut self, key: KeyEvent) {
        let Some(command) = find::Command::from_key(key) else {
            return;
        };
        let Some(mut find) = self.state.take_find() else {
            return;
        };
        find.refresh(self.state.editor().buffer());
        match command {
            find::Command::Insert(c) => find.insert(self.state.editor().buffer(), c),
            find::Command::Backspace => find.backspace(self.state.editor().buffer()),
            find::Command::ClearQuery => find.clear_query(self.state.editor().buffer()),
            find::Command::ToggleCase => find.toggle_case(self.state.editor().buffer()),
            find::Command::ToggleRegex => find.toggle_regex(self.state.editor().buffer()),
            find::Command::Next => find.next(),
            find::Command::Previous => find.previous(),
            find::Command::Close => {
                self.remembered.find = SavedQuery::new(find.query(), find.options());
                self.state.workspace_mut().set_find_bar(false);
                self.needs_redraw = true;
                return;
            }
        }
        self.select_found(&find);
        self.remembered.find = SavedQuery::new(find.query(), find.options());
        self.state.open_find(find);
        self.needs_redraw = true;
    }

    /// Selects the match the find bar is on, so it shows and the cursor is there.
    fn select_found(&mut self, find: &Find) {
        let Some(found) = find.current() else {
            return;
        };
        self.state
            .edit(|editor| editor.select_range(found.start_position(), found.end_position()));
    }
}
