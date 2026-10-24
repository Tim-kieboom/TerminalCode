use serde_json::json;

use crate::{ArtifactFinder, Kind, TargetId, Targets, build_args, run_env};

fn targets(extra: serde_json::Value) -> Targets {
    let mut cli = json!({
        "name": "cli", "kind": ["bin"], "crate_types": ["bin"], "src_path": "/p/app/src/main.rs",
    });
    if !extra.is_null() {
        cli["required-features"] = extra;
    }
    let json = json!({ "packages": [
        { "name": "app", "manifest_path": "/p/app/Cargo.toml", "targets": [
            cli,
            { "name": "demo", "kind": ["example"], "crate_types": ["bin"],
              "src_path": "/p/app/examples/demo.rs" } ] },
    ]});
    Targets::from_metadata(json.to_string().as_bytes()).unwrap()
}

fn target<'a>(targets: &'a Targets, kind: Kind, name: &str) -> &'a crate::Target {
    targets.get(&TargetId::new("app", kind, name)).unwrap()
}

#[test]
fn a_binary_is_built_by_the_path_of_its_package_with_cargo_rendering_the_errors() {
    let targets = targets(serde_json::Value::Null);

    let args = build_args(target(&targets, Kind::Bin, "cli"), false);

    assert_eq!(
        args,
        [
            "build",
            "--message-format=json-render-diagnostics",
            "--color",
            "always",
            "--manifest-path",
            "/p/app/Cargo.toml",
            "--bin",
            "cli"
        ]
    );
}

#[test]
fn an_example_is_built_with_example_and_release_and_the_features_it_needs() {
    let targets = targets(json!(["a", "b"]));

    let example = build_args(target(&targets, Kind::Example, "demo"), true);
    assert_eq!(example[6..], ["--example", "demo", "--release"]);
    let needing = build_args(target(&targets, Kind::Bin, "cli"), false);
    assert_eq!(needing[6..], ["--bin", "cli", "--features", "a,b"]);
}

#[test]
fn the_program_hears_where_its_package_is_and_its_name() {
    let targets = targets(serde_json::Value::Null);

    let env = run_env(target(&targets, Kind::Bin, "cli"));

    assert_eq!(
        env,
        [
            ("CARGO_MANIFEST_DIR".to_owned(), "/p/app".to_owned()),
            ("CARGO_PKG_NAME".to_owned(), "app".to_owned()),
        ]
    );
}

fn artifact(name: &str, kind: &str, executable: Option<&str>) -> String {
    json!({
        "reason": "compiler-artifact",
        "package_id": "path+file:///p/app#0.1.0",
        "target": { "name": name, "kind": [kind], "crate_types": ["bin"] },
        "executable": executable,
        "fresh": false,
    })
    .to_string()
}

fn finder(kind: Kind) -> ArtifactFinder {
    ArtifactFinder::new(&TargetId::new("app", kind, "cli"))
}

#[test]
fn the_executable_is_the_one_of_the_artifact_of_the_target() {
    let mut finder = finder(Kind::Bin);
    let lines = [
        artifact("serde", "lib", None),
        artifact("cli", "lib", None),
        artifact("cli", "bin", Some("/p/target/debug/cli")),
        json!({ "reason": "build-finished", "success": true }).to_string(),
    ];

    finder.feed(lines.join("\n").as_bytes());

    assert_eq!(
        finder.finish().as_deref(),
        Some(std::path::Path::new("/p/target/debug/cli"))
    );
}

#[test]
fn it_does_not_matter_how_the_output_is_cut_into_pieces() {
    let text = format!(
        "{}\n{}\n",
        artifact("cli", "bin", Some("/t/cli")),
        "not json"
    );
    for cut in [1, 7, 30, text.len() - 1] {
        let mut finder = finder(Kind::Bin);
        finder.feed(&text.as_bytes()[..cut]);
        finder.feed(&text.as_bytes()[cut..]);
        assert_eq!(
            finder.finish().as_deref(),
            Some(std::path::Path::new("/t/cli")),
            "{cut}"
        );
    }
}

#[test]
fn an_example_is_not_found_among_binaries_and_the_reverse() {
    let mut examples = finder(Kind::Example);
    let mut bins = finder(Kind::Bin);
    let line = artifact("cli", "bin", Some("/t/cli"));

    examples.feed(line.as_bytes());
    bins.feed(line.as_bytes());

    assert_eq!(examples.finish(), None);
    assert!(bins.finish().is_some());
}

#[test]
fn a_build_that_made_nothing_has_no_executable() {
    let mut finder = finder(Kind::Bin);
    finder.feed(artifact("cli", "bin", None).as_bytes());
    finder.feed(b"\nwarning: not a message\n{\"reason\":\n");

    assert_eq!(finder.finish(), None);
}

#[test]
fn the_last_artifact_counts_and_a_last_line_without_a_newline_is_read() {
    let mut finder = finder(Kind::Bin);
    finder.feed(artifact("cli", "bin", Some("/old/cli")).as_bytes());
    finder.feed(b"\n");
    finder.feed(artifact("cli", "bin", Some("/new/cli")).as_bytes());

    assert_eq!(
        finder.finish().as_deref(),
        Some(std::path::Path::new("/new/cli"))
    );
}
