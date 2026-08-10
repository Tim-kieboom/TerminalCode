use std::{fs, path::Path};

use super::{LaunchConfig, substitute_variables};

fn temp_project(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("launch_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".terminalcode")).unwrap();
    root
}

fn write_launch(root: &Path, content: &str) {
    fs::write(root.join(".terminalcode/launch.json"), content).unwrap();
}

fn cleanup(root: &Path) {
    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_parses_full_configuration() {
    let root = temp_project("full");
    write_launch(
        &root,
        r#"{
            "configurations": [
                {
                    "name": "Run App",
                    "program": "${workspaceFolder}/target/debug/app.exe",
                    "args": ["--flag", "value"],
                    "cwd": "${workspaceFolder}",
                    "env": { "RUST_LOG": "debug" }
                }
            ]
        }"#,
    );

    let launch = LaunchConfig::load(&root).unwrap().unwrap();
    let config = &launch.configurations()[0];

    let expected_program = format!("{}/target/debug/app.exe", root.display());
    assert_eq!(config.name(), "Run App");
    assert_eq!(config.program(), expected_program);
    assert_eq!(config.args(), &["--flag", "value"]);
    assert_eq!(
        config.cwd().unwrap().to_string_lossy(),
        root.to_string_lossy()
    );
    assert_eq!(config.env().get("RUST_LOG").unwrap(), "debug");

    cleanup(&root);
}

#[test]
fn load_missing_file_returns_none() {
    let root = temp_project("missing");
    let launch = LaunchConfig::load(&root).unwrap();
    assert!(launch.is_none());

    cleanup(&root);
}

#[test]
fn load_malformed_json_is_err() {
    let root = temp_project("malformed");
    write_launch(&root, "not json");

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().starts_with("at 1;"));

    cleanup(&root);
}

#[test]
fn load_top_level_non_object_is_err() {
    let root = temp_project("top_level");
    write_launch(&root, r#"[1, 2, 3]"#);

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().contains("expected a top-level JSON object"));

    cleanup(&root);
}

#[test]
fn load_missing_name_is_err() {
    let root = temp_project("no_name");
    write_launch(
        &root,
        r#"{
            "configurations": [
                { "program": "app.exe" }
            ]
        }"#,
    );

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().contains("requires a non-empty \"name\""));

    cleanup(&root);
}

#[test]
fn load_missing_program_is_err() {
    let root = temp_project("no_program");
    write_launch(
        &root,
        r#"{
            "configurations": [
                { "name": "Run" }
            ]
        }"#,
    );

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().contains("requires a non-empty \"program\""));

    cleanup(&root);
}

#[test]
fn load_args_not_array_is_err() {
    let root = temp_project("bad_args");
    write_launch(
        &root,
        r#"{
            "configurations": [
                { "name": "Run", "program": "app.exe", "args": "nope" }
            ]
        }"#,
    );

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().contains("args must be an array"));

    cleanup(&root);
}

#[test]
fn load_env_value_not_string_is_err() {
    let root = temp_project("bad_env");
    write_launch(
        &root,
        r#"{
            "configurations": [
                { "name": "Run", "program": "app.exe", "env": { "RUST_LOG": 5 } }
            ]
        }"#,
    );

    let err = LaunchConfig::load(&root).unwrap_err();
    assert!(err.to_string().contains("env values must be strings"));

    cleanup(&root);
}

#[test]
fn load_without_configurations_key_is_empty() {
    let root = temp_project("no_configs");
    write_launch(&root, r#"{ "version": "0.2.0" }"#);

    let launch = LaunchConfig::load(&root).unwrap().unwrap();
    assert!(launch.configurations().is_empty());

    cleanup(&root);
}

#[test]
fn load_empty_configurations_list_is_empty() {
    let root = temp_project("empty_list");
    write_launch(&root, r#"{ "configurations": [] }"#);

    let launch = LaunchConfig::load(&root).unwrap().unwrap();
    assert!(launch.configurations().is_empty());

    cleanup(&root);
}

#[test]
fn substitute_variables_replaces_workspace_folder() {
    let text = substitute_variables(
        "run in ${workspaceFolder} and ${workspaceFolder}/src",
        r"F:\proj",
    );
    assert_eq!(text, r"run in F:\proj and F:\proj/src");
}
