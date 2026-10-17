//! The one popup that has the keyboard: opening, reading and taking it back.

use super::{AppState, Popup};
use crate::components::popups::confirm::Confirm;
use crate::components::popups::definition_list::DefinitionList;
use crate::components::popups::diagnostic_popup::DiagnosticPopup;
use crate::components::popups::find::Find;
use crate::components::popups::finder::Finder;
use crate::components::popups::menu::Menu;
use crate::components::popups::name_prompt::NamePrompt;
use crate::components::popups::palette::Palette;
use crate::components::popups::quit_prompt::QuitPrompt;
use crate::components::popups::root_prompt::RootPrompt;
use crate::components::popups::search::Search;

impl AppState {
    /// The popup that is open, which has all the keys.
    pub(crate) fn popup(&self) -> &Popup {
        &self.popup
    }

    /// The unsaved-changes prompt, while it is open.
    #[cfg(test)]
    pub(crate) fn quit_prompt(&self) -> Option<&QuitPrompt> {
        match &self.popup {
            Popup::QuitPrompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Opens the prompt, closing any other popup.
    pub(crate) fn open_quit_prompt(&mut self, prompt: QuitPrompt) {
        self.popup = Popup::QuitPrompt(prompt);
    }

    /// Closes the quit_prompt and hands it back; any other popup stays open.
    pub(crate) fn take_quit_prompt(&mut self) -> Option<QuitPrompt> {
        if !matches!(self.popup, Popup::QuitPrompt(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::QuitPrompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn confirm(&self) -> Option<&Confirm> {
        match &self.popup {
            Popup::Confirm(confirm) => Some(confirm),
            _ => None,
        }
    }

    /// Opens the question, closing any other popup.
    pub(crate) fn open_confirm(&mut self, confirm: Confirm) {
        self.popup = Popup::Confirm(confirm);
    }

    /// Closes the question and hands it back; any other popup stays open.
    pub(crate) fn take_confirm(&mut self) -> Option<Confirm> {
        if !matches!(self.popup, Popup::Confirm(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Confirm(confirm) => Some(confirm),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn menu(&self) -> Option<&Menu> {
        match &self.popup {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    /// The open context menu, to place it before it is drawn.
    pub(crate) fn menu_mut(&mut self) -> Option<&mut Menu> {
        match &mut self.popup {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn open_menu(&mut self, menu: Menu) {
        self.popup = Popup::Menu(menu);
    }

    /// Opens the password prompt of a save as root.
    pub(crate) fn open_root_prompt(&mut self, prompt: RootPrompt) {
        self.popup = Popup::RootPassword(prompt);
    }

    pub(crate) fn root_prompt_mut(&mut self) -> Option<&mut RootPrompt> {
        match &mut self.popup {
            Popup::RootPassword(prompt) => Some(prompt),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn root_prompt(&self) -> Option<&RootPrompt> {
        match &self.popup {
            Popup::RootPassword(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Closes the password prompt, which wipes the password.
    pub(crate) fn close_root_prompt(&mut self) {
        if matches!(self.popup, Popup::RootPassword(_)) {
            self.popup = Popup::None;
        }
    }

    /// Opens the list of places a server named for a definition.
    pub(crate) fn open_definition_list(&mut self, list: DefinitionList) {
        self.popup = Popup::Definitions(list);
    }

    pub(crate) fn definition_list_mut(&mut self) -> Option<&mut DefinitionList> {
        match &mut self.popup {
            Popup::Definitions(list) => Some(list),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn definition_list(&self) -> Option<&DefinitionList> {
        match &self.popup {
            Popup::Definitions(list) => Some(list),
            _ => None,
        }
    }

    /// Closes the list of definitions, giving it back.
    pub(crate) fn take_definition_list(&mut self) -> Option<DefinitionList> {
        if !matches!(self.popup, Popup::Definitions(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Definitions(list) => Some(list),
            _ => None,
        }
    }

    /// Opens the box with the diagnostics of a line, closing any other popup.
    pub(crate) fn open_diagnostic_popup(&mut self, popup: DiagnosticPopup) {
        self.popup = Popup::Diagnostics(popup);
    }

    pub(crate) fn diagnostic_popup(&self) -> Option<&DiagnosticPopup> {
        match &self.popup {
            Popup::Diagnostics(popup) => Some(popup),
            _ => None,
        }
    }

    /// Closes the box with the diagnostics; any other popup stays open.
    pub(crate) fn close_diagnostic_popup(&mut self) {
        if matches!(self.popup, Popup::Diagnostics(_)) {
            self.popup = Popup::None;
        }
    }

    /// Closes the menu and hands it back; any other popup stays open.
    pub(crate) fn take_menu(&mut self) -> Option<Menu> {
        if !matches!(self.popup, Popup::Menu(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn name_prompt(&self) -> Option<&NamePrompt> {
        match &self.popup {
            Popup::NewEntry(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// The open name prompt, to place it before it is drawn.
    pub(crate) fn name_prompt_mut(&mut self) -> Option<&mut NamePrompt> {
        match &mut self.popup {
            Popup::NewEntry(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub(crate) fn open_name_prompt(&mut self, prompt: NamePrompt) {
        self.popup = Popup::NewEntry(prompt);
    }

    /// Closes the prompt and hands it back; any other popup stays open.
    pub(crate) fn take_name_prompt(&mut self) -> Option<NamePrompt> {
        if !matches!(self.popup, Popup::NewEntry(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::NewEntry(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Find in the open file, while its bar is open.
    pub(crate) fn find(&self) -> Option<&Find> {
        match &self.popup {
            Popup::Find(find) => Some(find),
            _ => None,
        }
    }

    pub(crate) fn open_find(&mut self, find: Find) {
        self.popup = Popup::Find(find);
    }

    /// Closes the find and hands it back; any other popup stays open.
    pub(crate) fn take_find(&mut self) -> Option<Find> {
        if !matches!(self.popup, Popup::Find(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Find(find) => Some(find),
            _ => None,
        }
    }

    /// The project search, while it is open.
    #[cfg(test)]
    pub(crate) fn search(&self) -> Option<&Search> {
        match &self.popup {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    pub(crate) fn search_mut(&mut self) -> Option<&mut Search> {
        match &mut self.popup {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    pub(crate) fn open_search(&mut self, search: Search) {
        self.popup = Popup::Search(search);
    }

    /// Closes the search and hands it back; any other popup stays open.
    pub(crate) fn take_search(&mut self) -> Option<Search> {
        if !matches!(self.popup, Popup::Search(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    /// The file finder, while it is open.
    #[cfg(test)]
    pub(crate) fn finder(&self) -> Option<&Finder> {
        match &self.popup {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    pub(crate) fn finder_mut(&mut self) -> Option<&mut Finder> {
        match &mut self.popup {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    pub(crate) fn open_finder(&mut self, finder: Finder) {
        self.popup = Popup::Finder(finder);
    }

    /// Closes the finder and hands it back; any other popup stays open.
    pub(crate) fn take_finder(&mut self) -> Option<Finder> {
        if !matches!(self.popup, Popup::Finder(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    /// The command palette, while it is open.
    #[cfg(test)]
    pub(crate) fn palette(&self) -> Option<&Palette> {
        match &self.popup {
            Popup::Palette(palette) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn open_palette(&mut self, palette: Palette) {
        self.popup = Popup::Palette(palette);
    }

    /// Closes the palette and hands it back; any other popup stays open.
    pub(crate) fn take_palette(&mut self) -> Option<Palette> {
        if !matches!(self.popup, Popup::Palette(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Palette(palette) => Some(palette),
            _ => None,
        }
    }
}
