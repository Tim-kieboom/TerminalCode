use serde_json::json;

use crate::{ConfigId, LaunchFile, MetadataError, RunConfig, RunList, Targets, What};

fn targets() -> Targets {
    let target = |name: &str, kind: &str| {
        json!({
            "name": name, "kind": [kind], "crate_types": ["bin"],
            "src_path": format!("/p/{name}.rs"),
        })
    };
    let json = json!({ "packages": [
        { "name": "app", "manifest_path": "/p/app/Cargo.toml",
          "targets": [target("app", "bin"), target("demo", "example")] },
        { "name": "tool", "manifest_path": "/p/tool/Cargo.toml",
          "targets": [target("tool", "bin")] },
    ]});
    Targets::from_metadata(json.to_string().as_bytes()).unwrap()
}

fn list(file: &str) -> RunList {
    RunList::build(Ok(&targets()), &LaunchFile::parse(file))
}

fn names(list: &RunList) -> Vec<String> {
    list.iter().map(|config| config.name().to_owned()).collect()
}

fn config<'a>(list: &'a RunList, id: &str) -> &'a RunConfig {
    list.get(&ConfigId::from(id))
        .unwrap_or_else(|| panic!("{id}"))
}

#[test]
fn without_a_file_every_target_is_a_configuration_under_its_package() {
    let list = list("");

    assert_eq!(names(&list), ["app (bin)", "demo (example)", "tool (bin)"]);
    let app = config(&list, "app:bin:app");
    assert_eq!(app.package(), Some("app"));
    assert!(app.is_available() && app.unavailable().is_none());
    assert!(app.args().is_empty() && app.env().is_empty());
    assert_eq!((app.cwd(), app.release()), (None, false));
    assert!(matches!(app.what(), What::Cargo(target) if target.id().to_string() == "app:bin:app"));
    assert!(list.problems().is_empty() && list.note().is_none());
}

#[test]
fn an_entry_changes_only_the_fields_it_names() {
    let list = list(
        r#"
[[run]]
target = "app:bin:app"
args = ["--fast"]
release = true
"#,
    );

    let app = config(&list, "app:bin:app");
    assert_eq!(app.args(), ["--fast"]);
    assert!(app.release());
    assert_eq!(app.name(), "app (bin)", "the name was not named");
    assert!(app.env().is_empty() && app.cwd().is_none());
    // The others are as they were.
    assert!(config(&list, "tool:bin:tool").args().is_empty());
    assert_eq!(list.len(), 3, "a change adds no configuration");
}

#[test]
fn an_entry_can_give_a_name_an_environment_and_a_folder() {
    let list = list(
        r#"
[[run]]
target = "tool:bin:tool"
name = "Tool, verbose"
env = { A = "1", B = "2" }
cwd = "work"
"#,
    );

    let tool = config(&list, "tool:bin:tool");
    assert_eq!(tool.name(), "Tool, verbose");
    assert_eq!(tool.env().len(), 2);
    assert_eq!(tool.env()["B"], "2");
    assert_eq!(tool.cwd(), Some("work"));
}

#[test]
fn commands_of_the_file_come_after_the_targets_and_have_no_package() {
    let list = list(
        r#"
[[run]]
name = "Dev server"
command = "npm"
args = ["run", "dev"]
env = { PORT = "3000" }
"#,
    );

    assert_eq!(names(&list)[3..], ["Dev server"]);
    let command = config(&list, "command:Dev server");
    assert_eq!(command.package(), None);
    assert!(matches!(command.what(), What::Command(program) if program == "npm"));
    assert_eq!(command.args(), ["run", "dev"]);
    assert_eq!(command.env()["PORT"], "3000");
    assert!(command.is_available() && !command.release());
}

#[test]
fn an_entry_for_a_target_that_is_gone_stays_grey_at_the_end_with_what_it_set() {
    let list = list(
        r#"
[[run]]
target = "old:bin:old"
args = ["x"]

[[run]]
name = "Cmd"
command = "ls"

[[run]]
target = "app:example:gone"
name = "A name"
"#,
    );

    // The targets, the commands, then the ones that are missing, in the file's order.
    assert_eq!(
        names(&list),
        [
            "app (bin)",
            "demo (example)",
            "tool (bin)",
            "Cmd",
            "old (bin)",
            "A name"
        ]
    );
    let old = config(&list, "old:bin:old");
    assert!(!old.is_available());
    assert_eq!(old.unavailable(), Some("target not found"));
    assert_eq!(old.package(), Some("old"));
    assert_eq!(
        old.args(),
        ["x"],
        "what it said is kept for when the target comes back"
    );
    assert!(matches!(old.what(), What::Missing(id) if id.to_string() == "old:bin:old"));
}

#[test]
fn a_target_changed_twice_counts_the_first_time_and_says_so() {
    let list = list(
        r#"
[[run]]
target = "app:bin:app"
args = ["one"]

[[run]]
target = "app:bin:app"
args = ["two"]
"#,
    );

    assert_eq!(config(&list, "app:bin:app").args(), ["one"]);
    let said: Vec<String> = list.problems().iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        ["entry 2: `app:bin:app` is changed twice; the first entry counts"]
    );
}

#[test]
fn a_missing_target_changed_twice_is_one_grey_entry() {
    let list = list(
        "[[run]]\ntarget = \"x:bin:x\"\nargs = [\"1\"]\n[[run]]\ntarget = \"x:bin:x\"\nargs = [\"2\"]\n",
    );

    assert_eq!(list.len(), 4);
    assert_eq!(config(&list, "x:bin:x").args(), ["1"]);
    assert_eq!(list.problems().len(), 1);
}

#[test]
fn two_commands_with_one_name_are_one_command_and_a_problem() {
    let list =
        list("[[run]]\nname = \"Go\"\ncommand = \"a\"\n[[run]]\nname = \"Go\"\ncommand = \"b\"\n");

    assert_eq!(list.len(), 4);
    assert!(matches!(config(&list, "command:Go").what(), What::Command(p) if p == "a"));
    let said: Vec<String> = list.problems().iter().map(ToString::to_string).collect();
    assert_eq!(said, ["entry 2: there is a command called `Go` already"]);
}

#[test]
fn problems_of_the_file_come_with_the_list() {
    let list = list("[[run]]\nname = \"x\"\n");

    assert_eq!(list.len(), 3);
    assert_eq!(list.problems().len(), 1);
}

#[test]
fn without_cargo_the_list_is_the_files_entries_and_the_note_says_why() {
    let file = LaunchFile::parse(
        r#"
[[run]]
target = "app:bin:app"
release = true

[[run]]
name = "Cmd"
command = "ls"
"#,
    );

    let list = RunList::build(Err(&MetadataError::CargoMissing), &file);

    assert_eq!(
        list.note(),
        Some("cargo is not installed, or not on the PATH")
    );
    // The command can be run; the target cannot be known to be there.
    assert_eq!(names(&list), ["Cmd", "app (bin)"]);
    assert!(config(&list, "command:Cmd").is_available());
    assert!(!config(&list, "app:bin:app").is_available());
}

#[test]
fn a_configuration_keeps_its_id_when_others_come_and_go() {
    let before = list("");
    let after = list("[[run]]\nname = \"New\"\ncommand = \"ls\"\n");

    for id in ["app:bin:app", "tool:bin:tool"] {
        assert_eq!(config(&before, id).id(), config(&after, id).id());
    }
    assert_eq!(
        config(&after, "command:New").id().to_string(),
        "command:New"
    );
    assert!(after.get(&ConfigId::from("nothing")).is_none());
}

#[test]
fn an_empty_project_has_an_empty_list() {
    let list = RunList::build(Ok(&Targets::default()), &LaunchFile::default());

    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
}
