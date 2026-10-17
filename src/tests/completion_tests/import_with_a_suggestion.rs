use super::*;

#[test]
fn a_suggestion_with_an_import_writes_both_and_one_undo_takes_both_back() {
    let answer: Answer = Arc::new(|_| {
        json!([{
            "label": "HashMap",
            "kind": 22,
            "textEdit": {
                "range": { "start": { "line": 1, "character": 11 }, "end": { "line": 1, "character": 15 } },
                "newText": "HashMap",
            },
            "additionalTextEdits": [{
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "newText": "use std::collections::HashMap;\n",
            }],
        }])
    });
    let source = "fn f() {\n    let m: Has";
    let mut rig = Rig::start(provider(), source, answer);

    rig.type_text("h");
    rig.wait_for_box();
    rig.press(KeyCode::Tab);

    assert_eq!(
        rig.text(),
        "use std::collections::HashMap;\nfn f() {\n    let m: HashMap"
    );
    rig.app.state_mut().edit(|editor| editor.undo().unwrap());
    assert_eq!(rig.text(), format!("{source}h"));
}
