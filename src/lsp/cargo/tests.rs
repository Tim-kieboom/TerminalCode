use std::fs;
use std::path::Path;

use super::*;

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn package(name: &str) -> String {
    format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n")
}

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn a_crate_includes_itself() {
    let dir = temp();
    write(dir.path(), "Cargo.toml", &package("a"));

    assert!(includes(dir.path(), dir.path()));
}

#[test]
fn a_member_named_in_the_workspace_is_included() {
    let dir = temp();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/api\"]\n",
    );
    write(dir.path(), "crates/api/Cargo.toml", &package("api"));

    assert!(includes(dir.path(), &dir.path().join("crates/api")));
}

#[test]
fn members_given_by_a_pattern_are_included_and_excluded_ones_are_not() {
    let dir = temp();
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/old\"]\n",
    );
    write(dir.path(), "crates/a/Cargo.toml", &package("a"));
    write(dir.path(), "crates/old/Cargo.toml", &package("old"));

    assert!(includes(dir.path(), &dir.path().join("crates/a")));
    assert!(!includes(dir.path(), &dir.path().join("crates/old")));
}

#[test]
fn a_crate_below_the_project_that_nothing_names_is_not_included() {
    let dir = temp();
    write(dir.path(), "Cargo.toml", &package("outer"));
    write(dir.path(), "lib/inner/Cargo.toml", &package("inner"));

    assert!(!includes(dir.path(), &dir.path().join("lib/inner")));
}

#[test]
fn a_workspace_without_members_takes_in_no_crate_below_it() {
    let dir = temp();
    write(dir.path(), "Cargo.toml", "[workspace]\n");
    write(dir.path(), "crates/a/Cargo.toml", &package("a"));

    assert!(!includes(dir.path(), &dir.path().join("crates/a")));
}

#[test]
fn a_path_dependency_is_included_also_through_another_crate() {
    let dir = temp();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\n[dependencies]\nmid = { path = \"lib/mid\" }\n",
    );
    write(
        dir.path(),
        "lib/mid/Cargo.toml",
        "[package]\nname = \"mid\"\nversion = \"0.1.0\"\n[dependencies]\nleaf = { path = \"../leaf\" }\n",
    );
    write(dir.path(), "lib/leaf/Cargo.toml", &package("leaf"));

    assert!(includes(dir.path(), &dir.path().join("lib/mid")));
    assert!(includes(dir.path(), &dir.path().join("lib/leaf")));
}

#[test]
fn dev_and_target_specific_path_dependencies_count_too() {
    let dir = temp();
    write(
        dir.path(),
        "Cargo.toml",
        "[package]\nname = \"o\"\nversion = \"0.1.0\"\n[dev-dependencies]\na = { path = \"a\" }\n[target.'cfg(unix)'.dependencies]\nb = { path = \"b\" }\n",
    );
    write(dir.path(), "a/Cargo.toml", &package("a"));
    write(dir.path(), "b/Cargo.toml", &package("b"));

    assert!(includes(dir.path(), &dir.path().join("a")));
    assert!(includes(dir.path(), &dir.path().join("b")));
}

#[test]
fn a_dependency_cycle_ends() {
    let dir = temp();
    write(
        dir.path(),
        "a/Cargo.toml",
        "[package]\nname = \"a\"\nversion = \"0.1.0\"\n[dependencies]\nb = { path = \"../b\" }\n",
    );
    write(
        dir.path(),
        "b/Cargo.toml",
        "[package]\nname = \"b\"\nversion = \"0.1.0\"\n[dependencies]\na = { path = \"../a\" }\n",
    );

    assert!(!includes(&dir.path().join("a"), &dir.path().join("c")));
}

#[test]
fn a_manifest_that_is_not_valid_includes_nothing_but_itself() {
    let dir = temp();
    write(dir.path(), "Cargo.toml", "this is not toml [");
    write(dir.path(), "inner/Cargo.toml", &package("inner"));

    assert!(!includes(dir.path(), &dir.path().join("inner")));
}

#[test]
fn patterns_match_by_characters() {
    assert!(matches("*", "anything"));
    assert!(matches("a*c", "abbbc"));
    assert!(matches("a?c", "abc"));
    assert!(!matches("a?c", "ac"));
    assert!(!matches("a*c", "abd"));
}
