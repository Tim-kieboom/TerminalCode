use std::fs;

use crate::file::{FileEntry, Patch};
use crate::{FILE, Kind, LaunchFile, TargetId};

fn id(text: &str) -> TargetId {
    text.parse().unwrap()
}

fn problems(file: &LaunchFile) -> Vec<String> {
    file.problems().iter().map(ToString::to_string).collect()
}

#[test]
fn an_entry_with_a_target_changes_what_it_names_and_one_with_a_command_adds_one() {
    let file = LaunchFile::parse(
        r#"
[[run]]
target = "trcode:bin:trcode"
args = ["--recent"]
env = { RUST_LOG = "debug" }
cwd = "demo"
release = true

[[run]]
name = "Dev server"
command = "npm"
args = ["run", "dev"]
"#,
    );

    assert!(file.problems().is_empty(), "{:?}", problems(&file));
    let [(1, first), (2, second)] = file.entries() else {
        panic!("{:?}", file.entries());
    };
    let FileEntry::Override { target, patch } = first else {
        panic!("{first:?}");
    };
    assert_eq!(*target, TargetId::new("trcode", Kind::Bin, "trcode"));
    assert_eq!(patch.args.as_deref(), Some(&["--recent".to_owned()][..]));
    assert_eq!(patch.env.as_ref().unwrap()["RUST_LOG"], "debug");
    assert_eq!(patch.cwd.as_deref(), Some("demo"));
    assert_eq!(patch.release, Some(true));
    assert_eq!(patch.name, None);
    let FileEntry::Command {
        name,
        program,
        args,
        env,
        cwd,
    } = second
    else {
        panic!("{second:?}");
    };
    assert_eq!((name.as_str(), program.as_str()), ("Dev server", "npm"));
    assert_eq!(args, &["run", "dev"]);
    assert!(env.is_empty());
    assert_eq!(*cwd, None);
}

#[test]
fn an_entry_that_names_only_a_target_changes_nothing_yet_is_fine() {
    let file = LaunchFile::parse("[[run]]\ntarget = \"a:bin:a\"\n");

    assert!(file.problems().is_empty());
    assert_eq!(
        file.entries(),
        [(
            1,
            FileEntry::Override {
                target: id("a:bin:a"),
                patch: Patch::default()
            }
        )]
    );
}

#[test]
fn no_file_text_and_no_run_entries_are_no_entries_and_no_problem() {
    for text in ["", "# nothing here\n", "[other]\nkey = 1\n"] {
        let file = LaunchFile::parse(text);
        assert!(
            file.entries().is_empty() && file.problems().is_empty(),
            "{text}"
        );
    }
}

#[test]
fn text_that_is_not_toml_is_one_problem_for_the_whole_file() {
    let file = LaunchFile::parse("[[run\nnope");

    assert!(file.entries().is_empty());
    let [problem] = file.problems() else {
        panic!("{:?}", problems(&file));
    };
    assert_eq!(problem.entry(), None);
    assert!(problem.message().contains("not valid TOML"), "{problem}");
}

#[test]
fn run_that_is_not_a_list_of_tables_is_a_problem() {
    let file = LaunchFile::parse("run = 3\n");

    assert_eq!(
        problems(&file),
        ["`run` must be a list of `[[run]]` tables"]
    );
}

#[test]
fn an_entry_that_is_wrong_is_said_with_its_number_and_the_others_still_count() {
    let file = LaunchFile::parse(
        r#"
[[run]]
target = "a:bin:a"

[[run]]
name = "no command, no target"

[[run]]
target = "a:bin:a"
command = "x"

[[run]]
name = "Fine"
command = "ls"
"#,
    );

    assert_eq!(file.entries().len(), 2);
    assert_eq!(file.entries()[0].0, 1);
    assert_eq!(
        file.entries()[1].0,
        4,
        "numbers count the entries that are wrong"
    );
    let said = problems(&file);
    assert_eq!(said.len(), 2);
    assert!(
        said[0].starts_with("entry 2: an entry needs a `target`"),
        "{said:?}"
    );
    assert!(
        said[1].starts_with("entry 3: an entry has a `target` or a `command`"),
        "{said:?}"
    );
}

#[test]
fn a_misspelled_key_is_a_problem_and_not_ignored() {
    let file = LaunchFile::parse("[[run]]\ntarget = \"a:bin:a\"\nargss = [\"x\"]\n");

    assert!(file.entries().is_empty());
    let said = problems(&file);
    assert!(
        said[0].starts_with("entry 1: unknown field `argss`"),
        "{said:?}"
    );
}

#[test]
fn values_of_the_wrong_type_are_problems() {
    let file = LaunchFile::parse("[[run]]\ntarget = \"a:bin:a\"\nargs = \"not a list\"\n");

    assert!(file.entries().is_empty());
    assert_eq!(file.problems().len(), 1);
}

#[test]
fn a_command_needs_a_name_a_program_and_no_release() {
    let file = LaunchFile::parse(
        r#"
[[run]]
command = "ls"

[[run]]
name = "x"
command = ""

[[run]]
name = ""
command = "ls"

[[run]]
name = "y"
command = "ls"
release = true
"#,
    );

    assert!(file.entries().is_empty());
    assert_eq!(
        problems(&file),
        [
            "entry 1: a command needs a `name`",
            "entry 2: `command` is empty",
            "entry 3: a command needs a `name`",
            "entry 4: `release` is for a target, not a command",
        ]
    );
}

#[test]
fn a_target_that_is_not_an_identity_is_a_problem_naming_it() {
    let file = LaunchFile::parse(
        "[[run]]\ntarget = \"just-a-name\"\n[[run]]\ntarget = \"a:test:x\"\n[[run]]\ntarget = \"a:bin:a\"\nname = \"\"\n",
    );

    let said = problems(&file);
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(
        said[0].contains("`just-a-name` is not `package:kind:name`"),
        "{said:?}"
    );
    assert!(
        said[1].contains("`test` is not a kind of target here"),
        "{said:?}"
    );
    assert_eq!(said[2], "entry 3: `name` is empty");
}

#[test]
fn a_project_without_the_file_has_nothing_and_no_problem() {
    let dir = tempfile::tempdir().unwrap();

    let file = LaunchFile::load(dir.path());

    assert_eq!(file, LaunchFile::default());
}

#[test]
fn the_file_is_read_from_the_trcode_folder_of_the_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".trcode")).unwrap();
    fs::write(
        dir.path().join(FILE),
        "[[run]]\nname = \"Hi\"\ncommand = \"echo\"\n",
    )
    .unwrap();

    let file = LaunchFile::load(dir.path());

    assert_eq!(file.entries().len(), 1);
    assert!(file.problems().is_empty());
}

#[test]
fn a_file_that_cannot_be_read_is_a_problem_and_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    // A folder where the file should be.
    fs::create_dir_all(dir.path().join(FILE)).unwrap();

    let file = LaunchFile::load(dir.path());

    assert!(file.entries().is_empty());
    let [problem] = file.problems() else {
        panic!("{:?}", problems(&file));
    };
    assert!(problem.message().contains("could not be read"), "{problem}");
}

#[test]
fn an_open_entry_has_a_name_and_the_text_to_open() {
    let file = LaunchFile::parse("[[run]]\nname = \"Preview\"\nopen = \"${file}\"\n");

    assert!(file.problems().is_empty(), "{:?}", problems(&file));
    assert_eq!(
        file.entries(),
        [(
            1,
            FileEntry::Open {
                name: "Preview".to_owned(),
                target: "${file}".to_owned()
            }
        )]
    );
}

#[test]
fn an_open_entry_that_is_not_right_is_a_problem_and_the_others_still_count() {
    let file = LaunchFile::parse(
        r#"
[[run]]
open = "${file}"

[[run]]
name = "a"
open = ""

[[run]]
name = "b"
open = "${dir}/x.html"

[[run]]
name = "c"
open = "x.html"
command = "xdg-open"

[[run]]
name = "d"
open = "x.html"
args = ["y"]

[[run]]
name = "fine"
open = "docs/index.html"
"#,
    );

    assert_eq!(
        problems(&file),
        [
            "entry 1: an `open` entry needs a `name`",
            "entry 2: `open` is empty",
            "entry 3: `${dir}` is not known; only `${file}` and `${workspaceFolder}` are",
            "entry 4: an entry with `open` has no `command`",
            "entry 5: an entry with `open` has no `args`",
        ]
    );
    assert_eq!(file.entries().len(), 1);
}

#[test]
fn the_placeholders_are_put_in_the_text_and_a_missing_file_is_told() {
    use crate::{NoFile, Places, expand};
    let with_file = Places {
        file: Some("/p/a.html"),
        workspace_folder: "/p",
    };
    let untitled = Places {
        file: None,
        workspace_folder: "/p",
    };

    assert_eq!(expand("${file}", &with_file), Ok("/p/a.html".to_owned()));
    assert_eq!(
        expand("${workspaceFolder}/web:${file}", &with_file),
        Ok("/p/web:/p/a.html".to_owned())
    );
    assert_eq!(expand("${file}", &untitled), Err(NoFile));
    assert_eq!(
        expand("${workspaceFolder}/web", &untitled),
        Ok("/p/web".to_owned()),
        "the folder is there without a file"
    );
}

#[test]
fn a_command_may_use_the_placeholders_in_its_arguments_variables_and_folder() {
    let file = LaunchFile::parse(
        r#"
[[run]]
name = "Serve"
command = "python"
args = ["serve.py", "${file}"]
env = { ROOT = "${workspaceFolder}" }
cwd = "${workspaceFolder}/web"

[[run]]
name = "Bad"
command = "python"
args = ["${nope}"]

[[run]]
target = "a:bin:a"
cwd = "${home}"
"#,
    );

    assert_eq!(file.entries().len(), 1);
    assert_eq!(
        problems(&file),
        [
            "entry 2: `${nope}` is not known; only `${file}` and `${workspaceFolder}` are",
            "entry 3: `${home}` is not known; only `${file}` and `${workspaceFolder}` are",
        ]
    );
}

#[test]
fn the_template_for_a_new_file_is_valid_and_changes_nothing() {
    let file = LaunchFile::parse(crate::TEMPLATE);

    assert!(file.problems().is_empty(), "{:?}", problems(&file));
    assert!(file.entries().is_empty());
}

#[test]
fn each_example_of_the_template_is_valid_once_it_is_not_commented() {
    // An example line is a table header or a `key = value`; the rest is prose.
    let uncommented: Vec<&str> = crate::TEMPLATE
        .lines()
        .map(|line| match line.strip_prefix("# ") {
            Some(rest) if rest.starts_with("[[") || rest.contains(" = ") => rest,
            _ => line,
        })
        .collect();
    let file = LaunchFile::parse(&uncommented.join("\n"));

    assert!(file.problems().is_empty(), "{:?}", problems(&file));
    assert_eq!(file.entries().len(), 3);
}
