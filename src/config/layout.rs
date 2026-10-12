use std::path::Path;

use super::{ConfigWarning, read_user_file};
use crate::ui::layout::LayoutTree;

/// A layout plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedLayout {
    pub(crate) layout: LayoutTree,
    pub(crate) warning: Option<ConfigWarning>,
}

/// The user's layout file if there is a valid one, else the built-in layout.
/// A missing file is normal; an unreadable or invalid one is reported (with
/// the place in the tree for a layout error) and the built-in layout is used.
/// The built-in layout itself is not allowed to fail: it panics.
pub(crate) fn load_layout(user_file: Option<&Path>) -> LoadedLayout {
    let fallback = |warning| LoadedLayout {
        layout: LayoutTree::default(),
        warning,
    };
    let Some(path) = user_file else {
        return fallback(None);
    };

    let source = match read_user_file(path) {
        Ok(Some(source)) => source,
        Ok(None) => return fallback(None),
        Err(error) => return fallback(Some(ConfigWarning::unusable(path, error, "layout"))),
    };

    match LayoutTree::from_ron(&source) {
        Ok(layout) => LoadedLayout {
            layout,
            warning: None,
        },
        Err(error) => fallback(Some(ConfigWarning::unusable(path, error, "layout"))),
    }
}
