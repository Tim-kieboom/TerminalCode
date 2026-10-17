use super::*;

#[test]
fn turning_the_server_off_in_the_settings_ends_it_and_clears_the_status() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": null } }"#,
    );

    assert_eq!(rig.status(), None);
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn changing_the_server_in_the_settings_starts_the_new_one_for_the_file_in_front() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );
    rig.wait_for_status(Some("ra-multiplex"));

    assert_eq!(rig.launcher.launched(), 2);
    assert_eq!(rig.launcher.specs()[1].command, "ra-multiplex");
}

#[test]
fn a_server_that_was_off_gets_another_try_after_the_settings_change() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(
        dir.path(),
        &file,
        FakeLauncher::new(|number| if number == 0 { None } else { stays() }),
    );
    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "rust-analyzer", "extensions": ["rs"], "args": ["--log-file", "x"] } } }"#,
    );
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 2);
    assert_eq!(rig.launcher.specs()[1].args, ["--log-file", "x"]);
}

#[test]
fn a_reload_that_leaves_the_servers_alone_does_not_restart_them() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(&mut rig, &config, r#"{ "tab_size": 2 }"#);

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer"));
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn the_initialization_options_from_the_settings_reach_the_launch() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let mut rig = config_app(dir.path(), &file, FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "rust-analyzer", "extensions": ["rs"],
            "initialization_options": { "cargo": { "features": "all" } } } } }"#,
    );
    rig.wait_for_status(Some("rust-analyzer"));

    let specs = rig.launcher.specs();
    assert_eq!(
        specs[1].initialization_options,
        serde_json::json!({ "cargo": { "features": "all" } })
    );
}
