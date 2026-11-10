//! What a server says to the user (`window/showMessage`): warnings and errors are shown once, with
//! the server's name; information is not.

use super::*;

/// A rig whose server sends each of `messages` (`window/showMessage` params) when it is told
/// about the document, the first time and again at every later `didChange`.
fn with_messages(messages: Vec<Value>) -> Rig {
    let (dir, file) = rust_project();
    write(&file, "fn main() {}\n");
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        let messages = messages.clone();
        let sent = sent.clone();
        Some(Box::new(move |session| {
            while let Some(message) = session.next() {
                if matches!(
                    method_of(&message),
                    Some("textDocument/didOpen" | "textDocument/didChange")
                ) {
                    for params in &messages {
                        session.notify("window/showMessage", params.clone());
                    }
                }
                let _ = sent.send(message);
            }
        }))
    });
    let mut rig = rig(dir.path(), std::slice::from_ref(&file), launcher);
    std::mem::forget(dir);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig
}

fn message(kind: u8, text: &str) -> Value {
    json!({ "type": kind, "message": text })
}

#[test]
fn an_error_of_the_server_is_shown_with_its_name_and_stays() {
    let mut rig = with_messages(vec![message(1, "the workspace could not be loaded")]);

    rig.wait_until("the message", |rig| {
        rig.texts()
            .iter()
            .any(|text| text.contains("could not be loaded"))
    });

    let told = rig.texts().join("|");
    assert!(
        told.contains("rust: the workspace could not be loaded"),
        "{told}"
    );
    assert!(
        rig.app.state().notifications().has_errors(),
        "an error stays"
    );
}

#[test]
fn a_warning_is_shown_too_and_information_is_not() {
    let mut rig = with_messages(vec![
        message(3, "just so you know"),
        message(4, "a log line"),
        message(2, "something to watch"),
    ]);

    rig.wait_until("the warning", |rig| {
        rig.texts()
            .iter()
            .any(|text| text.contains("something to watch"))
    });

    let told = rig.texts().join("|");
    assert!(
        !told.contains("just so you know") && !told.contains("a log line"),
        "{told}"
    );
    assert!(
        !rig.app.state().notifications().has_errors(),
        "a warning is not sticky"
    );
}

#[test]
fn a_message_that_comes_again_is_shown_once() {
    let mut rig = with_messages(vec![message(1, "the same trouble")]);
    rig.wait_until("the message", |rig| {
        rig.texts()
            .iter()
            .any(|text| text.contains("the same trouble"))
    });

    // A key takes an error that was seen away; another change makes the server say it again, and
    // it is not put up again.
    rig.type_text("x");
    rig.sync();
    std::thread::sleep(std::time::Duration::from_millis(300));
    rig.sync();

    let count = rig
        .texts()
        .iter()
        .filter(|text| text.contains("the same trouble"))
        .count();
    assert_eq!(count, 0, "shown once, and not again");
}

#[test]
fn a_missing_standard_library_says_what_to_do_about_it() {
    let mut rig = with_messages(vec![message(
        1,
        "Workspace `/p/Cargo.toml` has sysroot errors: cd \"/p\" && \"rustc\" \"--print\" \"sysroot\" failed",
    )]);

    rig.wait_until("the message", |rig| {
        rig.texts()
            .iter()
            .any(|text| text.contains("sysroot errors"))
    });

    let told = rig.texts().join("|");
    assert!(told.contains("put rustc on the PATH"), "{told}");
    assert!(told.contains("\"sysroot\""), "{told}");
}

#[test]
fn a_long_message_is_cut_and_only_its_first_lines_are_shown() {
    let long = format!(
        "first line\n\nsecond line\nthird line\n{}",
        "x".repeat(1000)
    );
    let mut rig = with_messages(vec![message(1, &long)]);

    rig.wait_until("the message", |rig| {
        rig.texts().iter().any(|text| text.contains("first line"))
    });

    let told = rig.texts().join("|");
    assert!(told.contains("first line second line"), "{told}");
    assert!(!told.contains("third line"), "{told}");
    assert!(told.len() < 500, "{} characters", told.len());
}
