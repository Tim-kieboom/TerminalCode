use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::lsp::handshake::client_capabilities;
use crate::lsp::{LspError, initialize, shutdown};

use super::support::{PATIENCE, connect};

#[cfg(unix)]
const ROOT: &str = "/home/tim/project";
#[cfg(windows)]
const ROOT: &str = r"C:\Users\tim\project";

pub(super) fn root_for_tests() -> &'static str {
    ROOT
}

fn root() -> &'static Path {
    Path::new(ROOT)
}

#[test]
fn the_client_says_it_counts_in_utf_16_and_nothing_else() {
    assert_eq!(
        client_capabilities()["general"]["positionEncodings"],
        json!(["utf-16"])
    );
}

#[test]
fn the_client_says_only_what_it_uses() {
    let capabilities = client_capabilities();

    assert_eq!(
        capabilities["textDocument"]["publishDiagnostics"]["versionSupport"],
        json!(true)
    );
    assert_eq!(
        capabilities["textDocument"]["synchronization"]["didSave"],
        json!(true)
    );
    assert_eq!(capabilities["window"]["workDoneProgress"], json!(true));
    let completion = &capabilities["textDocument"]["completion"];
    assert_eq!(completion["contextSupport"], json!(true));
    assert_eq!(completion["completionItem"]["snippetSupport"], json!(true));
    assert_eq!(
        completion["completionItem"]["resolveSupport"]["properties"],
        json!(["additionalTextEdits"])
    );
    assert!(capabilities["textDocument"].get("hover").is_none());
}

#[test]
fn the_handshake_sends_initialize_and_then_initialized() {
    let rig = connect(|server| {
        server.start_up();
    });

    let server = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap();

    assert_eq!(server.name.as_deref(), Some("fake-server"));
    assert_eq!(server.capabilities["textDocumentSync"], json!(2));
    rig.server.finish();
}

#[test]
fn initialize_names_the_project_the_process_and_the_client() {
    let rig = connect(|server| {
        let params = server.start_up();

        assert_eq!(params["processId"], json!(std::process::id()));
        assert_eq!(params["clientInfo"]["name"], json!("trcode"));
        let uri = params["rootUri"].as_str().unwrap();
        assert!(
            uri.starts_with("file:///") && uri.ends_with("/project"),
            "{uri}"
        );
        assert_eq!(params["workspaceFolders"][0]["uri"], json!(uri));
        assert_eq!(params["workspaceFolders"][0]["name"], json!("project"));
        assert_eq!(params["capabilities"], client_capabilities());
    });

    initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap();

    rig.server.finish();
}

#[test]
fn initialization_options_go_to_the_server_as_they_are() {
    let options = json!({ "cargo": { "features": "all" }, "check": { "command": "clippy" } });
    let sent = options.clone();
    let rig = connect(move |server| {
        let params = server.start_up();

        assert_eq!(params["initializationOptions"], sent);
    });

    initialize(&rig.connection, root(), &options, PATIENCE).unwrap();

    rig.server.finish();
}

#[test]
fn no_options_means_the_field_is_not_sent_at_all() {
    let rig = connect(|server| {
        let params = server.start_up();

        assert!(params.get("initializationOptions").is_none());
    });

    initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap();

    rig.server.finish();
}

#[test]
fn a_server_with_no_name_is_fine() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("initialize");
        server.respond(id, json!({ "capabilities": {} }));
        server.expect_notification("initialized");
    });

    let server = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap();

    assert_eq!(server.name, None);
    rig.server.finish();
}

#[test]
fn a_server_that_counts_in_another_unit_is_refused() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("initialize");
        server.respond(
            id,
            json!({ "capabilities": { "positionEncoding": "utf-8" } }),
        );
    });

    let error = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap_err();

    assert!(
        matches!(error, LspError::Encoding(ref encoding) if encoding == "utf-8"),
        "{error}"
    );
    rig.server.finish();
}

#[test]
fn a_server_that_says_utf_16_is_accepted() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("initialize");
        server.respond(
            id,
            json!({ "capabilities": { "positionEncoding": "utf-16" } }),
        );
        server.expect_notification("initialized");
    });

    initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap();

    rig.server.finish();
}

#[test]
fn an_answer_with_no_capabilities_is_not_usable() {
    for answer in [json!({}), json!(null), json!({ "capabilities": 3 })] {
        let sent = answer.clone();
        let rig = connect(move |server| {
            let (id, _) = server.expect_request("initialize");
            server.respond(id, sent);
        });

        let error = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap_err();

        assert!(
            matches!(error, LspError::Unexpected { .. }),
            "{answer}: {error}"
        );
        rig.server.finish();
    }
}

#[test]
fn a_server_that_refuses_to_initialize_says_why() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("initialize");
        server.respond_error(id, -32002, "no Cargo.toml found");
    });

    let error = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap_err();

    assert!(error.to_string().contains("no Cargo.toml found"), "{error}");
    assert!(matches!(error, LspError::Rejected { .. }));
    rig.server.finish();
}

#[test]
fn a_server_that_never_answers_times_out_instead_of_hanging() {
    let rig = connect(|server| {
        server.expect_request("initialize");
        std::thread::sleep(Duration::from_millis(300));
    });

    let error = initialize(
        &rig.connection,
        root(),
        &Value::Null,
        Duration::from_millis(50),
    )
    .unwrap_err();

    assert!(matches!(error, LspError::Timeout { .. }), "{error}");
    rig.server.finish();
}

#[test]
fn a_server_that_exits_at_once_is_reported_as_closed() {
    let rig = connect(|_| {});

    let error = initialize(&rig.connection, root(), &Value::Null, PATIENCE).unwrap_err();

    assert!(matches!(error, LspError::Closed(_)), "{error}");
    rig.server.finish();
}

#[test]
fn a_root_that_is_not_absolute_is_refused_before_anything_is_sent() {
    let rig = connect(|server| {
        assert!(server.next().is_none(), "nothing should have been sent");
    });

    let error = initialize(
        &rig.connection,
        Path::new("relative/dir"),
        &Value::Null,
        PATIENCE,
    )
    .unwrap_err();

    assert!(matches!(error, LspError::NotAUri(_)), "{error}");
    drop(rig.connection);
    rig.server.finish();
}

#[test]
fn shutdown_asks_and_then_exits() {
    let rig = connect(|server| {
        let (id, _) = server.expect_request("shutdown");
        server.respond(id, Value::Null);
        server.expect_notification("exit");
    });

    shutdown(&rig.connection, PATIENCE).unwrap();

    rig.server.finish();
}

#[test]
fn exit_is_sent_even_when_the_server_does_not_answer_shutdown() {
    let rig = connect(|server| {
        server.expect_request("shutdown");
        server.expect_notification("exit");
    });

    let error = shutdown(&rig.connection, Duration::from_millis(50)).unwrap_err();

    assert!(matches!(error, LspError::Timeout { .. }), "{error}");
    rig.server.finish();
}
