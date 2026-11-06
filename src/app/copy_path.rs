//! Copying the path of a file: of the explorer's selected entry, or of the open file when the
//! keyboard is elsewhere (the tab menu and the palette).

use std::path::Path;

use super::App;
use crate::app::state::Focus;
use crate::clipboard::Register;

impl App {
    /// Puts the path of the explorer's selection (or, with the keyboard elsewhere, of the open
    /// file) on the clipboard: `relative` to the project folder when it is in it, else whole.
    pub(super) fn copy_path(&mut self, relative: bool) {
        let path = match self.state.focus() {
            Focus::Explorer => self
                .state
                .explorer()
                .selected_row()
                .map(|row| row.path.clone()),
            _ => self.state.editor().buffer().path().map(Path::to_path_buf),
        };
        let Some(path) = path else {
            self.state.notify("there is no file to copy the path of");
            return;
        };
        self.copy_path_text(&path, relative);
    }

    /// Puts `path` on the clipboard as text, and says what was copied.
    pub(super) fn copy_path_text(&mut self, path: &Path, relative: bool) {
        let text = text_of(path, &self.project_root(), relative);
        self.set_register(Register::charwise(text.clone()));
        self.state.notify(format!("copied {text}"));
    }
}

/// The path as it is written to the clipboard: from `root` when `relative` and it is inside
/// (the project folder itself is `.`), else the whole path, without a Windows `\\?\` prefix.
pub(super) fn text_of(path: &Path, root: &Path, relative: bool) -> String {
    let whole = trcode_files::paths::without_verbatim_prefix(path);
    if !relative {
        return whole.display().to_string();
    }
    let root = trcode_files::paths::without_verbatim_prefix(root);
    match whole.strip_prefix(&root) {
        Ok(inside) if inside.as_os_str().is_empty() => ".".to_owned(),
        Ok(inside) => inside.display().to_string(),
        Err(_) => whole.display().to_string(),
    }
}

#[cfg(test)]
mod tests;
