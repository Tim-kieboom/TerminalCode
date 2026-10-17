use serde_json::{Value, json};

use crate::replies::answer;

#[test]
fn requests_that_only_need_agreement_are_answered_with_null() {
    for method in [
        "window/workDoneProgress/create",
        "client/registerCapability",
        "client/unregisterCapability",
        "workspace/workspaceFolders",
        "workspace/diagnostic/refresh",
        "workspace/semanticTokens/refresh",
        "workspace/inlayHint/refresh",
        "workspace/codeLens/refresh",
        "window/showMessageRequest",
    ] {
        assert_eq!(answer(method, &Value::Null), Ok(Value::Null), "{method}");
    }
}

#[test]
fn a_configuration_request_gets_one_null_for_each_item_asked() {
    let params = json!({ "items": [{ "section": "a" }, { "section": "b" }, {}] });

    assert_eq!(
        answer("workspace/configuration", &params),
        Ok(json!([null, null, null]))
    );
    assert_eq!(
        answer("workspace/configuration", &Value::Null),
        Ok(json!([]))
    );
}

#[test]
fn an_edit_a_server_wants_applied_is_declined_with_a_reason() {
    let answered = answer("workspace/applyEdit", &json!({})).unwrap();

    assert_eq!(answered["applied"], json!(false));
    assert!(
        answered["failureReason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
}

#[test]
fn a_document_a_server_wants_shown_is_declined() {
    assert_eq!(
        answer("window/showDocument", &json!({})),
        Ok(json!({ "success": false }))
    );
}

#[test]
fn anything_else_is_a_method_not_found_that_names_the_method() {
    let error = answer("custom/whatever", &Value::Null).unwrap_err();

    assert_eq!(error.code, -32601);
    assert!(error.message.contains("custom/whatever"), "{error}");
}
