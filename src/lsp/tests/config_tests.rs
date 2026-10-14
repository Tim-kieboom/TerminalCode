use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::lsp::config::root_for;
use crate::lsp::{ConfigError, LanguageServers, ServerConfig};

fn servers(setting: Value) -> LanguageServers {
    LanguageServers::with_settings(setting).unwrap()
}

fn error_of(setting: Value) -> String {
    match LanguageServers::with_settings(setting) {
        Ok(servers) => panic!("should not be accepted, got {servers:?}"),
        Err(error) => error.to_string(),
    }
}

fn names(servers: &LanguageServers) -> Vec<&str> {
    servers.names().collect()
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

// ---- what is configured

#[test]
fn with_no_setting_rust_analyzer_is_there_for_rust() {
    let servers = servers(json!({}));

    let rust = servers.get("rust").unwrap();
    assert_eq!(names(&servers), ["rust"]);
    assert_eq!(rust.command, "rust-analyzer");
    assert_eq!(rust.args, Vec::<String>::new());
    assert_eq!(rust.extensions, ["rs"]);
    assert_eq!(rust.root_files, ["Cargo.toml"]);
    assert_eq!(rust.initialization_options, Value::Null);
    assert_eq!(servers, LanguageServers::default());
}

#[test]
fn an_entry_with_a_new_name_adds_a_server() {
    let servers = servers(json!({
        "python": { "command": "pyright-langserver", "args": ["--stdio"], "extensions": ["py"],
                    "root_files": ["pyproject.toml", "setup.py"] },
    }));

    assert_eq!(names(&servers), ["python", "rust"]);
    let python = servers.get("python").unwrap();
    assert_eq!(python.command, "pyright-langserver");
    assert_eq!(python.args, ["--stdio"]);
    assert_eq!(python.root_files, ["pyproject.toml", "setup.py"]);
}

#[test]
fn an_entry_named_rust_replaces_the_built_in_one_whole() {
    let servers = servers(json!({
        "rust": { "command": "my-analyzer", "extensions": ["rs", "rlib"] },
    }));

    let rust = servers.get("rust").unwrap();
    assert_eq!(rust.command, "my-analyzer");
    assert_eq!(rust.extensions, ["rs", "rlib"]);
    assert_eq!(
        rust.root_files,
        Vec::<String>::new(),
        "nothing is kept of the built in one"
    );
}

#[test]
fn null_turns_a_server_off() {
    let servers = servers(json!({ "rust": null }));

    assert_eq!(names(&servers), Vec::<&str>::new());
    assert!(servers.for_file(Path::new("/p/a.rs")).is_none());
}

#[test]
fn null_for_a_server_that_does_not_exist_is_fine() {
    assert_eq!(names(&servers(json!({ "nothing": null }))), ["rust"]);
}

#[test]
fn the_options_go_through_as_json_of_any_shape() {
    let options = json!({ "cargo": { "features": "all" }, "list": [1, "two", null] });
    let servers = servers(json!({
        "rust": { "command": "rust-analyzer", "extensions": ["rs"], "initialization_options": options },
    }));

    assert_eq!(servers.get("rust").unwrap().initialization_options, options);
}

#[test]
fn extensions_are_taken_with_or_without_a_dot_and_in_any_case() {
    let servers = servers(json!({
        "x": { "command": "x", "extensions": [".Foo", "BAR", " baz "] },
    }));

    assert_eq!(servers.get("x").unwrap().extensions, ["foo", "bar", "baz"]);
}

#[test]
fn a_command_is_trimmed() {
    let servers = servers(json!({ "x": { "command": "  fish  ", "extensions": ["f"] } }));

    assert_eq!(servers.get("x").unwrap().command, "fish");
}

// ---- what is refused

#[test]
fn the_setting_must_be_an_object() {
    for setting in [json!([]), json!("rust"), json!(3), json!(null)] {
        assert_eq!(
            LanguageServers::with_settings(setting),
            Err(ConfigError::NotAnObject)
        );
    }
}

#[test]
fn an_entry_that_is_not_an_object_names_the_entry() {
    let message = error_of(json!({ "rust": "rust-analyzer" }));

    assert!(message.contains("\"rust\""), "{message}");
}

#[test]
fn an_entry_without_a_command_or_extensions_says_which_is_missing() {
    let message = error_of(json!({ "x": { "extensions": ["x"] } }));
    assert!(
        message.contains("\"x\"") && message.contains("command"),
        "{message}"
    );

    let message = error_of(json!({ "x": { "command": "x" } }));
    assert!(
        message.contains("\"x\"") && message.contains("extensions"),
        "{message}"
    );
}

#[test]
fn a_word_that_is_not_a_field_is_refused_so_a_typo_is_not_silent() {
    let message = error_of(json!({ "x": { "command": "x", "extensions": ["x"], "arg": [] } }));

    assert!(
        message.contains("\"x\"") && message.contains("arg"),
        "{message}"
    );
}

#[test]
fn fields_of_the_wrong_type_are_refused_with_the_entry_name() {
    for entry in [
        json!({ "command": 1, "extensions": ["x"] }),
        json!({ "command": "x", "extensions": "x" }),
        json!({ "command": "x", "extensions": ["x"], "args": "--stdio" }),
        json!({ "command": "x", "extensions": ["x"], "root_files": [1] }),
    ] {
        let message = error_of(json!({ "x": entry }));

        assert!(message.contains("\"x\""), "{entry}: {message}");
    }
}

#[test]
fn an_empty_command_empty_extensions_or_bad_names_are_refused() {
    for (entry, needle) in [
        (json!({ "command": "  ", "extensions": ["x"] }), "command"),
        (json!({ "command": "x", "extensions": [] }), "extensions"),
        (json!({ "command": "x", "extensions": [""] }), "extension"),
        (json!({ "command": "x", "extensions": ["."] }), "extension"),
        (
            json!({ "command": "x", "extensions": ["a/b"] }),
            "extension",
        ),
        (
            json!({ "command": "x", "extensions": ["x"], "root_files": [""] }),
            "root_files",
        ),
        (
            json!({ "command": "x", "extensions": ["x"], "root_files": ["a/b"] }),
            "root_files",
        ),
    ] {
        let message = error_of(json!({ "x": entry }));

        assert!(message.contains(needle), "{entry}: {message}");
    }
}

#[test]
fn one_bad_entry_refuses_the_whole_setting_so_nothing_half_applies() {
    let result = LanguageServers::with_settings(json!({
        "good": { "command": "g", "extensions": ["g"] },
        "bad": { "command": "" , "extensions": ["b"] },
    }));

    assert!(result.is_err());
}

// ---- which server a file gets

fn two_servers() -> LanguageServers {
    servers(json!({
        "zig": { "command": "zls", "extensions": ["zig", "zon"] },
        "also_rs": { "command": "other", "extensions": ["rs"] },
    }))
}

#[test]
fn a_file_gets_the_server_for_its_extension() {
    let servers = two_servers();

    assert_eq!(servers.for_file(Path::new("/p/a.zig")).unwrap().0, "zig");
    assert_eq!(servers.for_file(Path::new("/p/a.zon")).unwrap().0, "zig");
}

#[test]
fn the_extension_is_matched_in_any_case() {
    assert_eq!(
        two_servers().for_file(Path::new("/p/A.ZIG")).unwrap().0,
        "zig"
    );
}

#[test]
fn files_with_no_server_get_none() {
    let servers = two_servers();

    for path in ["/p/a.txt", "/p/Makefile", "/p/.zig", "/p/a."] {
        assert!(servers.for_file(Path::new(path)).is_none(), "{path}");
    }
}

#[test]
fn when_two_servers_want_an_extension_the_first_by_name_gets_it() {
    assert_eq!(
        two_servers().for_file(Path::new("/p/a.rs")).unwrap().0,
        "also_rs"
    );
}

// ---- where its project is

fn rust_server() -> ServerConfig {
    LanguageServers::default().get("rust").unwrap().clone()
}

fn canonical(path: &Path) -> std::path::PathBuf {
    crate::paths::without_verbatim_prefix(&crate::paths::canonical(path))
}

#[test]
fn the_outermost_root_file_up_to_the_project_root_is_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("Cargo.toml"));
    touch(&project.join("crates/a/Cargo.toml"));
    touch(&project.join("crates/a/src/lib.rs"));

    let root = root_for(
        &rust_server(),
        &project.join("crates/a/src/lib.rs"),
        &project,
    );

    assert_eq!(root, Some(canonical(&project)));
}

#[test]
fn without_a_workspace_the_crate_is_its_own_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("a/Cargo.toml"));
    touch(&project.join("a/src/lib.rs"));
    touch(&project.join("b/Cargo.toml"));
    touch(&project.join("b/src/lib.rs"));
    let server = rust_server();

    let a = root_for(&server, &project.join("a/src/lib.rs"), &project);
    let b = root_for(&server, &project.join("b/src/lib.rs"), &project);

    assert_eq!(a, Some(canonical(&project.join("a"))));
    assert_eq!(b, Some(canonical(&project.join("b"))));
}

#[test]
fn a_root_file_above_the_project_root_is_not_looked_at() {
    let dir = tempfile::tempdir().unwrap();
    touch(&dir.path().join("Cargo.toml"));
    let project = dir.path().join("proj");
    touch(&project.join("src/lib.rs"));

    let root = root_for(&rust_server(), &project.join("src/lib.rs"), &project);

    assert_eq!(
        root,
        Some(canonical(&project)),
        "the project root, not the folder above it"
    );
}

#[test]
fn with_no_root_file_anywhere_the_project_root_is_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("src/lib.rs"));

    let root = root_for(&rust_server(), &project.join("src/lib.rs"), &project);

    assert_eq!(root, Some(canonical(&project)));
}

#[test]
fn a_server_with_no_root_files_always_has_the_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("deep/er/Cargo.toml"));
    touch(&project.join("deep/er/a.rs"));
    let mut server = rust_server();
    server.root_files.clear();

    let root = root_for(&server, &project.join("deep/er/a.rs"), &project);

    assert_eq!(root, Some(canonical(&project)));
}

#[test]
fn any_of_several_root_files_counts() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("pkg/setup.py"));
    touch(&project.join("pkg/src/a.py"));
    let servers = servers(json!({
        "python": { "command": "p", "extensions": ["py"], "root_files": ["pyproject.toml", "setup.py"] },
    }));

    let (key, _) = servers
        .resolve(&project.join("pkg/src/a.py"), &project)
        .unwrap();

    assert_eq!(key.name, "python");
    assert_eq!(key.root, canonical(&project.join("pkg")));
}

#[test]
fn a_file_outside_the_project_gets_the_nearest_root_file_or_no_server() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    fs::create_dir_all(&project).unwrap();
    touch(&dir.path().join("other/Cargo.toml"));
    touch(&dir.path().join("other/src/x.rs"));
    touch(&dir.path().join("lonely/y.rs"));
    let server = rust_server();

    let with = root_for(&server, &dir.path().join("other/src/x.rs"), &project);
    let without = root_for(&server, &dir.path().join("lonely/y.rs"), &project);

    assert_eq!(with, Some(canonical(&dir.path().join("other"))));
    assert_eq!(without, None);
}

#[test]
fn a_path_spelled_with_dots_gives_the_same_root() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("Cargo.toml"));
    touch(&project.join("src/lib.rs"));
    let server = rust_server();

    let plain = root_for(&server, &project.join("src/lib.rs"), &project);
    let dotted = root_for(
        &server,
        &project.join("src/../src/./lib.rs"),
        &project.join("."),
    );

    assert_eq!(plain, dotted);
}

#[test]
fn the_key_names_the_server_and_its_root_so_one_workspace_is_one_key() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    touch(&project.join("Cargo.toml"));
    touch(&project.join("crates/a/Cargo.toml"));
    touch(&project.join("crates/a/src/lib.rs"));
    touch(&project.join("crates/b/Cargo.toml"));
    touch(&project.join("crates/b/src/lib.rs"));
    let servers = LanguageServers::default();

    let (a, _) = servers
        .resolve(&project.join("crates/a/src/lib.rs"), &project)
        .unwrap();
    let (b, _) = servers
        .resolve(&project.join("crates/b/src/lib.rs"), &project)
        .unwrap();

    assert_eq!(a, b);
    assert_eq!(a.name, "rust");
}

#[test]
fn a_file_no_server_wants_resolves_to_nothing() {
    let dir = tempfile::tempdir().unwrap();

    let servers = LanguageServers::default();
    let resolved = servers.resolve(&dir.path().join("notes.txt"), dir.path());

    assert!(resolved.is_none());
}

// ---- language_ids

#[test]
fn language_ids_are_read_from_an_entry() {
    let servers = servers(json!({
        "json": { "command": "vscode-json-language-server", "extensions": ["json"],
                  "language_ids": { "settings.json": "jsonc" } }
    }));

    let ids = &servers.get("json").unwrap().language_ids;
    assert_eq!(ids.get("settings.json").map(String::as_str), Some("jsonc"));
}

#[test]
fn an_entry_without_language_ids_has_none() {
    assert!(
        servers(json!({}))
            .get("rust")
            .unwrap()
            .language_ids
            .is_empty()
    );
}

#[test]
fn a_language_id_name_with_a_path_or_an_empty_id_is_refused() {
    for ids in [
        json!({ "a/settings.json": "jsonc" }),
        json!({ "": "jsonc" }),
        json!({ "settings.json": "" }),
    ] {
        let error = error_of(json!({
            "j": { "command": "x", "extensions": ["json"], "language_ids": ids }
        }));
        assert!(error.contains("language_ids"), "{error}");
    }
}

#[test]
fn a_misspelled_language_ids_is_still_refused() {
    let error = error_of(json!({
        "j": { "command": "x", "extensions": ["json"], "language_id": {} }
    }));

    assert!(error.contains("language_id"), "{error}");
}
