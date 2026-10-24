use std::fs;

use serde_json::{Value, json};

use crate::{IdError, Kind, MetadataError, TargetId, Targets};

fn target(name: &str, kind: &str, crate_type: &str) -> Value {
    json!({
        "name": name,
        "kind": [kind],
        "crate_types": [crate_type],
        "src_path": format!("/p/{name}.rs"),
    })
}

fn package(name: &str, targets: Vec<Value>) -> Value {
    json!({
        "name": name,
        "manifest_path": format!("/p/{name}/Cargo.toml"),
        "targets": targets,
    })
}

fn metadata(packages: Vec<Value>) -> Vec<u8> {
    json!({ "packages": packages, "workspace_root": "/p" })
        .to_string()
        .into_bytes()
}

fn ids(targets: &Targets) -> Vec<String> {
    targets.iter().map(|t| t.id().to_string()).collect()
}

#[test]
fn only_programs_that_can_be_started_are_targets() {
    let json = metadata(vec![package(
        "app",
        vec![
            target("app", "lib", "lib"),
            target("app", "bin", "bin"),
            target("demo", "example", "bin"),
            // An example that is a library cannot be run.
            target("shapes", "example", "lib"),
            target("it", "test", "bin"),
            target("speed", "bench", "bin"),
            target("build-script-build", "custom-build", "bin"),
        ],
    )]);

    let targets = Targets::from_metadata(&json).unwrap();

    assert_eq!(ids(&targets), ["app:bin:app", "app:example:demo"]);
}

#[test]
fn packages_keep_cargos_order_and_each_lists_its_binaries_before_its_examples_by_name() {
    let json = metadata(vec![
        package(
            "zeta",
            vec![
                target("b-example", "example", "bin"),
                target("z", "bin", "bin"),
                target("a", "bin", "bin"),
            ],
        ),
        package("alpha", vec![target("alpha", "bin", "bin")]),
    ]);

    let targets = Targets::from_metadata(&json).unwrap();

    assert_eq!(
        ids(&targets),
        [
            "zeta:bin:a",
            "zeta:bin:z",
            "zeta:example:b-example",
            "alpha:bin:alpha"
        ]
    );
}

#[test]
fn a_name_shared_by_packages_or_kinds_is_not_one_target() {
    let json = metadata(vec![
        package(
            "one",
            vec![
                target("server", "bin", "bin"),
                target("server", "example", "bin"),
            ],
        ),
        package("two", vec![target("server", "bin", "bin")]),
    ]);

    let targets = Targets::from_metadata(&json).unwrap();

    assert_eq!(targets.len(), 3);
    let two = TargetId::new("two", Kind::Bin, "server");
    let found = targets.get(&two).unwrap();
    assert_eq!(found.manifest().to_str(), Some("/p/two/Cargo.toml"));
    assert_eq!(found.source().to_str(), Some("/p/server.rs"));
}

#[test]
fn the_features_a_target_needs_are_kept() {
    let mut needs = target("cli", "bin", "bin");
    needs["required-features"] = json!(["cli", "extra"]);
    let json = metadata(vec![package("app", vec![needs])]);

    let targets = Targets::from_metadata(&json).unwrap();

    let found = targets.iter().next().unwrap();
    assert_eq!(found.required_features(), ["cli", "extra"]);
}

#[test]
fn a_workspace_without_programs_has_no_targets() {
    let json = metadata(vec![package("lib", vec![target("lib", "lib", "lib")])]);
    assert!(Targets::from_metadata(&json).unwrap().is_empty());
    assert!(Targets::from_metadata(b"{}").unwrap().is_empty());
}

#[test]
fn text_that_is_not_metadata_is_an_error() {
    let error = Targets::from_metadata(b"not json").unwrap_err();
    assert!(matches!(error, MetadataError::Unreadable(_)), "{error}");
}

#[test]
fn an_identity_is_package_kind_name_and_reads_back() {
    let id: TargetId = "trcode-lsp:example:demo".parse().unwrap();

    assert_eq!(id, TargetId::new("trcode-lsp", Kind::Example, "demo"));
    assert_eq!(id.to_string(), "trcode-lsp:example:demo");
    assert_eq!(
        (id.package(), id.kind(), id.name()),
        ("trcode-lsp", Kind::Example, "demo")
    );
}

#[test]
fn a_text_that_is_not_an_identity_says_why() {
    for text in ["", "app", "app:bin", "app:bin:x:y", ":bin:x", "app:bin:"] {
        let error = text.parse::<TargetId>().unwrap_err();
        assert_eq!(error, IdError::Shape(text.to_owned()), "{text}");
    }
    let error = "app:test:x".parse::<TargetId>().unwrap_err();
    assert_eq!(error, IdError::Kind("test".to_owned()));
}

#[test]
fn a_binary_sorts_before_an_example_and_the_flags_are_cargos() {
    assert!(Kind::Bin < Kind::Example);
    assert_eq!(Kind::Bin.flag(), "--bin");
    assert_eq!(Kind::Example.flag(), "--example");
}

#[test]
fn a_program_that_is_not_there_is_cargo_missing() {
    let dir = tempfile::tempdir().unwrap();

    let error = Targets::fetch_with("trcode-no-such-cargo", dir.path()).unwrap_err();

    assert!(matches!(error, MetadataError::CargoMissing), "{error}");
}

#[test]
fn cargo_finds_a_binary_and_an_example_the_manifest_does_not_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"found\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src/bin")).unwrap();
    fs::create_dir_all(dir.path().join("examples")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("src/bin/second.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("examples/demo.rs"), "fn main() {}\n").unwrap();

    let targets = Targets::fetch(dir.path()).unwrap();

    assert_eq!(
        ids(&targets),
        ["found:bin:found", "found:bin:second", "found:example:demo"]
    );
}

#[test]
fn a_folder_that_is_not_a_project_is_cargos_own_words() {
    let dir = tempfile::tempdir().unwrap();

    let error = Targets::fetch(dir.path()).unwrap_err();

    let MetadataError::Failed(message) = &error else {
        panic!("{error}");
    };
    assert!(message.contains("Cargo.toml"), "{message}");
}

#[test]
fn this_repository_has_the_editor_among_its_targets() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let targets = Targets::fetch(&root).unwrap();

    let editor = TargetId::new("terminal_code", Kind::Bin, "trcode");
    assert!(targets.get(&editor).is_some(), "{:?}", ids(&targets));
}
