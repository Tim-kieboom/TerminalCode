use std::collections::BTreeMap;
use std::path::Path;

use crate::language_id_of;

fn ids(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, id)| ((*name).to_owned(), (*id).to_owned()))
        .collect()
}

fn id(path: &str) -> String {
    language_id_of(
        Path::new(path),
        &BTreeMap::new(),
        Some(Path::new("/cfg/trcode")),
    )
    .unwrap()
}

#[test]
fn the_settings_file_of_the_config_folder_is_jsonc() {
    assert_eq!(id("/cfg/trcode/settings.json"), "jsonc");
}

#[test]
fn a_settings_file_of_a_project_is_plain_json() {
    assert_eq!(id("/work/app/settings.json"), "json");
}

#[test]
fn the_files_that_allow_comments_in_a_project_are_jsonc_by_name() {
    for path in [
        "/p/.vscode/launch.json",
        "/p/.vscode/anything.json",
        "/p/tsconfig.json",
        "/p/packages/a/tsconfig.base.json",
        "/p/jsconfig.json",
        "/p/.devcontainer/devcontainer.json",
        "/p/.devcontainer.json",
    ] {
        assert_eq!(id(path), "jsonc", "{path}");
    }
}

#[test]
fn a_json_file_without_a_match_keeps_the_extension_rule() {
    assert_eq!(id("/p/package.json"), "json");
    assert_eq!(id("/p/data/tsconfig-notes.txt"), "txt");
    assert_eq!(id("/p/main.rs"), "rust");
}

#[test]
fn a_user_entry_goes_by_the_bare_file_name_and_wins_over_the_built_ins() {
    let ids = ids(&[("settings.json", "json"), ("package.json", "jsonc")]);
    let of = |path: &str| language_id_of(Path::new(path), &ids, Some(Path::new("/cfg/trcode")));

    assert_eq!(of("/cfg/trcode/settings.json").unwrap(), "json");
    assert_eq!(of("/anywhere/package.json").unwrap(), "jsonc");
    assert_eq!(of("/p/tsconfig.json").unwrap(), "jsonc");
}

#[test]
fn without_a_config_folder_only_the_project_names_match() {
    let none = BTreeMap::new();

    assert_eq!(
        language_id_of(Path::new("/cfg/trcode/settings.json"), &none, None).unwrap(),
        "json"
    );
    assert_eq!(
        language_id_of(Path::new("/p/tsconfig.json"), &none, None).unwrap(),
        "jsonc"
    );
}
