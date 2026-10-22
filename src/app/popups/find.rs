//! Find in the open file, and replacing what it finds.

use crossterm::event::KeyEvent;

use super::SavedQuery;
use crate::app::App;
use crate::app::state::Focus;
use crate::buffer::Position;
use crate::components::editor::Replacement;
use crate::components::popups::find::{self, Find};
use unicode_segmentation::UnicodeSegmentation;

impl App {
    /// Opens the find bar for the open file. A selected word becomes the query;
    /// otherwise the last one is used. The first match from the cursor is
    /// selected.
    pub(in crate::app) fn open_find(&mut self) {
        self.open_find_bar(false);
    }

    /// Like [`App::open_find`], with the row for the replacement shown and
    /// given the typing.
    pub(in crate::app) fn open_replace(&mut self) {
        self.open_find_bar(true);
    }

    fn open_find_bar(&mut self, with_replacement: bool) {
        let (query, origin) = {
            let editor = self.state.editor();
            let selection = *editor.selections().primary();
            let seeded = Find::seed(editor.buffer(), selection.start(), selection.end());
            (
                seeded.unwrap_or_else(|| self.remembered.find.query.clone()),
                selection.start(),
            )
        };
        let mut find = Find::open(
            self.state.editor().buffer(),
            origin,
            query,
            self.remembered.find.options,
        );
        find.set_replacement(self.remembered.find.replacement.clone());
        if with_replacement {
            find.toggle_replace();
        }
        self.state
            .workspace_mut()
            .set_find_bar_rows(find.bar_rows());
        self.select_found(&find);
        self.remember_find(&find);
        self.state.open_find(find);
    }

    /// Whether the keys go to the find bar: it is open, was not left by a click, and the
    /// editor is where the keyboard is.
    pub(in crate::app) fn find_has_keys(&self) -> bool {
        self.state.find_focused() && self.state.focus() == Focus::Editor
    }

    /// Closes the find bar; the match that was found stays selected.
    pub(in crate::app) fn close_find(&mut self) {
        if let Some(find) = self.state.take_find() {
            self.remember_find(&find);
        }
        self.state.set_find_focused(false);
        self.state.workspace_mut().set_find_bar_rows(0);
        self.needs_redraw = true;
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
            find::Command::ToggleWord => find.toggle_word(self.state.editor().buffer()),
            find::Command::Next => find.next(),
            find::Command::Previous => find.previous(),
            find::Command::Confirm if find.replacing() => self.replace_current(&mut find),
            find::Command::Confirm => find.next(),
            find::Command::ReplaceAll => self.replace_all(&mut find),
            find::Command::ToggleReplace => find.toggle_replace(),
            find::Command::SwitchField => find.switch_field(),
            find::Command::Close => {
                self.remember_find(&find);
                self.state.set_find_focused(false);
                self.state.workspace_mut().set_find_bar_rows(0);
                self.needs_redraw = true;
                return;
            }
        }
        self.state
            .workspace_mut()
            .set_find_bar_rows(find.bar_rows());
        self.select_found(&find);
        self.remember_find(&find);
        // The keys stay where they were: in the bar.
        self.state.open_find(find);
        self.needs_redraw = true;
    }

    /// F3 and shift+F3, from the editor: the next or previous match from the cursor, wherever
    /// the user moved it. With no find open the bar opens, on the last query.
    pub(in crate::app) fn find_step(&mut self, forward: bool) {
        if self.state.find().is_none() {
            self.open_find();
            return;
        }
        self.state.refresh_find();
        let selection = *self.state.editor().selections().primary();
        let Some(mut find) = self.state.take_find() else {
            return;
        };
        find.step_from(selection.start(), selection.end(), forward);
        self.select_found(&find);
        self.remember_find(&find);
        self.state.put_find(find);
        self.needs_redraw = true;
    }

    fn remember_find(&mut self, find: &Find) {
        self.remembered.find = SavedQuery::new(find.query(), find.options());
        self.remembered.find.replacement = find.replacement().to_owned();
    }

    /// Selects the match the find bar is on, so it shows and the cursor is there.
    fn select_found(&mut self, find: &Find) {
        let Some(found) = find.current() else {
            return;
        };
        self.state
            .edit(|editor| editor.select_range(found.start_position(), found.end_position()));
    }

    /// Puts the replacement in place of the current match and goes on to the next.
    fn replace_current(&mut self, find: &mut Find) {
        let Some(replacement) = find.current_replacement(self.state.editor().buffer()) else {
            self.state.notify("nothing to replace");
            return;
        };
        let end = after(&replacement);
        let result = self
            .state
            .edit(|editor| editor.replace_all_of(std::slice::from_ref(&replacement), end));
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
            return;
        }
        find.continue_from(self.state.editor().buffer(), end);
    }

    /// Replaces every match, as one step to undo.
    fn replace_all(&mut self, find: &mut Find) {
        let replacements = find.all_replacements(self.state.editor().buffer());
        let (count, capped) = find.match_count();
        let Some(first) = replacements.first() else {
            self.state.notify("nothing to replace");
            return;
        };
        let cursor = first.start;
        if let Err(error) = self
            .state
            .edit(|editor| editor.replace_all_of(&replacements, cursor))
        {
            self.state.notify_error(error.to_string());
            return;
        }
        find.continue_from(self.state.editor().buffer(), cursor);
        let noun = match replacements.len() {
            1 => "occurrence",
            _ => "occurrences",
        };
        let more = match capped {
            true => format!(" ({count}+ found; run it again for the rest)"),
            false => String::new(),
        };
        self.state
            .notify(format!("replaced {} {noun}{more}", replacements.len()));
    }
}

/// Where the text after `replacement` has been put ends, counted in graphemes.
fn after(replacement: &Replacement) -> Position {
    let start = replacement.start;
    match replacement.text.rsplit_once('\n') {
        Some((before, last)) => Position::new(
            start.line + before.matches('\n').count() + 1,
            last.graphemes(true).count(),
        ),
        None => Position::new(
            start.line,
            start.column + replacement.text.graphemes(true).count(),
        ),
    }
}
