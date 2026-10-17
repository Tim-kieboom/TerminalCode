//! The language a file is called when a server is told about it. The extension
//! says it, except for JSON files that allow comments: the JSON server flags
//! those unless it is told `jsonc`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::lsp::sync::language_id;
use crate::paths::canonical;

/// The settings file of the config folder is JSON with comments; so are the
/// files below, wherever they are.
const SETTINGS_FILE: &str = "settings.json";
const PROJECT_FOLDER: &str = ".vscode";
const PROJECT_FILES: [&str; 3] = ["jsconfig.json", "devcontainer.json", ".devcontainer.json"];

/// What a server is told the file at `path` is.
///
/// In order: an entry of the server's `language_ids`, which goes by the bare file
/// name; a JSON file that allows comments (the settings file in `config_dir`
/// by its full path, the files of a project by name); then the extension.
pub(crate) fn language_id_of(
    path: &Path,
    language_ids: &BTreeMap<String, String>,
    config_dir: Option<&Path>,
) -> Option<String> {
    let extension = path.extension()?.to_str()?;
    let name = path.file_name()?.to_str()?;
    if let Some(id) = language_ids.get(name) {
        return Some(id.clone());
    }
    if extension.eq_ignore_ascii_case("json") && allows_comments(path, name, config_dir) {
        return Some("jsonc".to_owned());
    }
    Some(language_id(extension))
}

fn allows_comments(path: &Path, name: &str, config_dir: Option<&Path>) -> bool {
    let lower = name.to_ascii_lowercase();
    let in_config_dir =
        config_dir.is_some_and(|dir| canonical(path) == canonical(&dir.join(SETTINGS_FILE)));
    let in_project_folder = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|folder| folder == PROJECT_FOLDER);
    in_config_dir
        || in_project_folder
        || lower.starts_with("tsconfig")
        || PROJECT_FILES.contains(&lower.as_str())
}
