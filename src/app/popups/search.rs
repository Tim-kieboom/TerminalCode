//! The project search.

use crossterm::event::KeyEvent;

use super::SavedQuery;
use super::SavedResults;
use crate::app::App;
use crate::app::state::Focus;
use crate::components::popups::search::{self, Hit, Search};

impl App {
    /// Opens the project search with the last query and options, and runs it.
    pub(in crate::app) fn open_search(&mut self) {
        if !self.background.is_connected() {
            self.state
                .notify_error("the project search needs the event loop, which is not running");
            return;
        }
        let saved = &self.remembered.search;
        let (query, options) = (saved.query.clone(), saved.options);
        let mut search = Search::new(self.project_root(), query, options);
        self.restart_search(&mut search);
        self.state.open_search(search);
    }

    /// Stops the running search and starts one for what is typed now.
    fn restart_search(&mut self, search: &mut Search) {
        if search.query().is_empty() {
            search.idle();
            return;
        }
        let Some(events) = self.background.sender() else {
            search.invalid("no event loop".to_owned());
            return;
        };
        let id = self.background.next_search();
        let root = search.root().to_path_buf();
        match search::start_search(root, id, search.query(), search.options(), events) {
            Ok(handle) => search.begin(id, handle),
            Err(error) => search.invalid(error.to_string()),
        }
    }

    pub(in crate::app) fn handle_search_key(&mut self, key: KeyEvent) {
        let Some(command) = search::Command::from_key(key) else {
            return;
        };
        let Some(mut search) = self.state.take_search() else {
            return;
        };
        let mut keep_open = true;
        let mut changed = true;
        let mut chosen = None;
        match command {
            search::Command::Insert(c) => search.insert(c),
            search::Command::Backspace => search.backspace(),
            search::Command::ClearQuery => search.clear_query(),
            search::Command::ToggleCase => search.toggle_case(),
            search::Command::ToggleRegex => search.toggle_regex(),
            search::Command::ToggleWord => search.toggle_word(),
            search::Command::Previous => (changed, _) = (false, search.select_previous()),
            search::Command::Next => (changed, _) = (false, search.select_next()),
            search::Command::PageUp => (changed, _) = (false, search.page_up()),
            search::Command::PageDown => (changed, _) = (false, search.page_down()),
            search::Command::Cancel => (changed, keep_open) = (false, false),
            search::Command::Open => {
                changed = false;
                chosen = search.selected_hit().cloned();
                keep_open = chosen.is_none();
            }
        }
        if changed {
            self.restart_search(&mut search);
        }
        self.remembered.search = SavedQuery::new(search.query(), search.options());
        let root = search.root().to_path_buf();
        if keep_open {
            self.state.open_search(search);
        } else {
            // Closing keeps the results, so F4 and shift+F4 can step through them.
            let (hits, selected) = search.results();
            self.remembered.results = Some(SavedResults {
                root: root.clone(),
                hits: hits.to_vec(),
                index: selected,
                // A result that was opened is behind the next step; one that
                // was only highlighted is the next one.
                visited: chosen.is_some(),
            });
        }
        self.needs_redraw = true;
        let Some(hit) = chosen else {
            return;
        };
        self.open_hit(&root, &hit);
    }

    /// Opens the file of a search result with the cursor at the match. The
    /// editor gets the keyboard.
    fn open_hit(&mut self, root: &std::path::Path, hit: &Hit) {
        self.mark_jump();
        self.state.set_focus(Focus::Editor);
        let target = crate::buffer::Position::new(hit.line, hit.column);
        let opened = self.state.workspace_mut().open_path(&root.join(&hit.path));
        match opened {
            Ok(()) => self.state.edit(|editor| editor.go_to(target)),
            Err(error) => self.state.notify_error(error.to_string()),
        }
    }

    /// Goes to the result after (or before, for a negative `step`) the one last
    /// looked at in the last search, without opening the search again.
    pub(in crate::app) fn step_search_result(&mut self, step: isize) {
        let Some(saved) = self.remembered.results.as_mut() else {
            self.state
                .notify("no search results yet (alt+f searches the project)");
            return;
        };
        if saved.hits.is_empty() {
            self.state.notify("the last search found nothing");
            return;
        }
        let len = saved.hits.len() as isize;
        // From a result that was not opened yet, forward goes to it and
        // backward to the one before it.
        let from = match (saved.visited, step > 0) {
            (false, true) => saved.index as isize - 1,
            (false, false) => saved.index as isize,
            (true, _) => saved.index as isize,
        };
        saved.index = (from + step).rem_euclid(len) as usize;
        saved.visited = true;
        let (root, hit, index) = (
            saved.root.clone(),
            saved.hits[saved.index].clone(),
            saved.index,
        );
        self.open_hit(&root, &hit);
        self.state.notify(format!("result {} of {len}", index + 1));
    }
}
