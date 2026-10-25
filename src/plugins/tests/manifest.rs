use std::fs;
use std::path::Path;

use trcode_plugin_api::{Abi, AbiError, RunPattern};

use crate::plugins::manifest::{Host, Installed, MANIFEST_FILE, ManifestError, read, scan};

use super::*;

const HOST: Host<'static> = Host {
    abi: Abi::new(1, 2),
    version: "0.7.1",
};

fn manifest_text(extra: &str) -> String {
    format!(
        "id = \"viewer\"\nname = \"Viewer\"\nversion = \"0.1.0\"\nabi = \"1.2\"\nwasm = \"viewer.wasm\"\n{extra}"
    )
}

/// A plugin folder called `folder` with `text` as its manifest and a module.
fn plugin(root: &Path, folder: &str, text: &str) -> std::path::PathBuf {
    let dir = root.join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(MANIFEST_FILE), text).unwrap();
    fs::write(dir.join("viewer.wasm"), b"\0asm").unwrap();
    dir
}

fn read_text(text: &str) -> Result<Installed, ManifestError> {
    let root = tempfile::tempdir().unwrap();
    let dir = plugin(root.path(), "viewer", text);
    read(&dir, HOST)
}

#[test]
fn a_good_manifest_is_read_with_its_capabilities_in_a_fixed_order() {
    let installed = read_text(&manifest_text(
        "capabilities = [\"git.write\", \"git.read\", \"git.write\"]\n[[action]]\nid = \"stage\"\ntitle = \"Stage\"\nkeys = \"s\"\n",
    ))
    .unwrap();

    assert_eq!(installed.id(), "viewer");
    assert_eq!(
        installed.capabilities,
        vec![Capability::GitRead, Capability::GitWrite],
        "each once, always in the same order"
    );
    assert_eq!(installed.manifest.actions.len(), 1);
}

#[test]
fn a_plugin_that_asks_for_nothing_asks_for_no_capability() {
    assert!(
        read_text(&manifest_text(""))
            .unwrap()
            .capabilities
            .is_empty()
    );
}

#[test]
fn the_module_is_read_from_the_plugins_folder() {
    let root = tempfile::tempdir().unwrap();
    let dir = plugin(root.path(), "viewer", &manifest_text(""));
    let installed = read(&dir, HOST).unwrap();

    assert_eq!(installed.module().unwrap(), b"\0asm");
}

#[test]
fn a_missing_module_is_told() {
    let root = tempfile::tempdir().unwrap();
    let dir = plugin(root.path(), "viewer", &manifest_text(""));
    fs::remove_file(dir.join("viewer.wasm")).unwrap();
    let installed = read(&dir, HOST).unwrap();

    assert!(matches!(
        installed.module(),
        Err(ManifestError::ModuleUnreadable(_))
    ));
}

#[test]
fn a_manifest_that_is_not_toml_or_lacks_a_part_is_invalid() {
    for text in ["", "id = ", "id = \"viewer\"\n", "not toml at all"] {
        assert!(
            matches!(read_text(text), Err(ManifestError::Invalid(_))),
            "{text:?}"
        );
    }
}

#[test]
fn a_manifest_that_cannot_be_read_is_told() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("viewer");
    fs::create_dir_all(&dir).unwrap();

    assert!(matches!(
        read(&dir, HOST),
        Err(ManifestError::Unreadable(_))
    ));
}

#[test]
fn an_id_that_is_not_the_name_of_the_folder_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let dir = plugin(root.path(), "other-name", &manifest_text(""));

    assert_eq!(
        read(&dir, HOST),
        Err(ManifestError::IdIsNotTheFolder {
            id: "viewer".into(),
            folder: "other-name".into()
        })
    );
}

#[test]
fn an_id_that_would_confuse_a_name_is_refused() {
    for id in [
        "",
        "Has Capitals",
        "a:b",
        "a.b",
        "a/b",
        "-start",
        "ünï",
        "a b",
    ] {
        let root = tempfile::tempdir().unwrap();
        let text = manifest_text("").replace("id = \"viewer\"", &format!("id = \"{id}\""));
        let dir = plugin(root.path(), "viewer", &text);
        assert_eq!(
            read(&dir, HOST),
            Err(ManifestError::BadId(id.to_owned())),
            "{id:?}"
        );
    }
}

#[test]
fn a_module_path_that_leaves_the_folder_is_refused() {
    for path in ["../x.wasm", "/etc/passwd", "a/../../x.wasm", ""] {
        let text = manifest_text("").replace("viewer.wasm", path);
        assert_eq!(
            read_text(&text),
            Err(ManifestError::BadModulePath(path.to_owned())),
            "{path:?}"
        );
    }
}

#[test]
fn a_module_in_a_folder_of_the_plugin_is_fine() {
    let text = manifest_text("").replace("viewer.wasm", "bin/viewer.wasm");

    assert!(read_text(&text).is_ok());
}

#[test]
fn a_later_plugin_api_is_refused_with_what_to_do() {
    let text = manifest_text("").replace("abi = \"1.2\"", "abi = \"2.0\"");

    let error = read_text(&text).unwrap_err();

    assert!(matches!(error, ManifestError::Abi(AbiError::TooNew { .. })));
    assert!(error.to_string().contains("update TerminalCode"), "{error}");
}

#[test]
fn an_earlier_major_plugin_api_is_refused_and_an_earlier_minor_is_fine() {
    let old = Host {
        abi: Abi::new(2, 0),
        version: "0.7.1",
    };
    let root = tempfile::tempdir().unwrap();
    let dir = plugin(root.path(), "viewer", &manifest_text(""));
    assert!(matches!(
        read(&dir, old),
        Err(ManifestError::Abi(AbiError::TooOld { .. }))
    ));

    let text = manifest_text("").replace("abi = \"1.2\"", "abi = \"1.0\"");
    assert!(read_text(&text).is_ok());
}

#[test]
fn a_version_of_the_plugin_api_that_does_not_read_is_refused() {
    let text = manifest_text("").replace("abi = \"1.2\"", "abi = \"one\"");

    assert!(matches!(
        read_text(&text),
        Err(ManifestError::Abi(AbiError::Unreadable(_)))
    ));
}

#[test]
fn a_plugin_that_needs_a_newer_editor_is_refused_naming_both() {
    let error = read_text(&manifest_text("requires = \"0.8.0\"\n")).unwrap_err();

    assert_eq!(
        error,
        ManifestError::NeedsNewerEditor {
            required: "0.8.0".into(),
            have: "0.7.1".into()
        }
    );
}

#[test]
fn the_editor_version_a_plugin_needs_is_compared_as_numbers() {
    for (requires, fine) in [
        ("0.7.1", true),
        ("0.7.0", true),
        ("0.7", true),
        ("0", true),
        ("0.7.2", false),
        ("0.10.0", false),
        ("1", false),
    ] {
        let result = read_text(&manifest_text(&format!("requires = \"{requires}\"\n")));
        assert_eq!(result.is_ok(), fine, "{requires}");
    }
}

#[test]
fn a_requirement_that_is_not_a_version_is_invalid() {
    for requires in ["", "latest", "0.7.x", "1.2.3.4", ">=0.7"] {
        let result = read_text(&manifest_text(&format!("requires = \"{requires}\"\n")));
        assert!(
            matches!(result, Err(ManifestError::Invalid(_))),
            "{requires:?}: {result:?}"
        );
    }
}

#[test]
fn a_capability_this_version_does_not_have_is_refused_naming_it() {
    let error = read_text(&manifest_text(
        "capabilities = [\"git.read\", \"net.fetch\"]\n",
    ))
    .unwrap_err();

    assert_eq!(error, ManifestError::UnknownCapability("net.fetch".into()));
}

#[test]
fn actions_must_have_distinct_ids_without_a_colon() {
    let two = "[[action]]\nid = \"a\"\n[[action]]\nid = \"a\"\n";
    assert_eq!(
        read_text(&manifest_text(two)),
        Err(ManifestError::DuplicateAction("a".into()))
    );
    let colon = "[[action]]\nid = \"a:b\"\n";
    assert_eq!(
        read_text(&manifest_text(colon)),
        Err(ManifestError::BadActionId("a:b".into()))
    );
    let empty = "[[action]]\nid = \"\"\n";
    assert_eq!(
        read_text(&manifest_text(empty)),
        Err(ManifestError::BadActionId(String::new()))
    );
}

#[test]
fn scanning_lists_every_folder_with_a_manifest_good_or_not_in_order() {
    let root = tempfile::tempdir().unwrap();
    plugin(root.path(), "viewer", &manifest_text(""));
    plugin(root.path(), "broken", "nonsense");
    fs::create_dir_all(root.path().join("no-manifest")).unwrap();
    fs::write(root.path().join("a-file.txt"), "x").unwrap();
    let mut late = manifest_text("").replace("viewer", "zeta");
    late = late.replace("zeta.wasm", "viewer.wasm");
    plugin(root.path(), "zeta", &late);

    let found = scan(root.path(), HOST);

    let names: Vec<_> = found
        .iter()
        .map(|found| match found {
            Ok(installed) => (installed.id().to_owned(), true),
            Err((folder, _)) => (folder.clone(), false),
        })
        .collect();
    assert_eq!(
        names,
        vec![
            ("broken".to_owned(), false),
            ("viewer".to_owned(), true),
            ("zeta".to_owned(), true)
        ]
    );
}

#[test]
fn a_plugins_folder_that_is_not_there_has_no_plugins() {
    let root = tempfile::tempdir().unwrap();

    assert!(scan(&root.path().join("nothing"), HOST).is_empty());
}

#[test]
fn this_editor_is_what_it_says_it_is() {
    assert_eq!(Host::THIS.abi, Abi::CURRENT);
    assert_eq!(Host::THIS.version, env!("CARGO_PKG_VERSION"));
}

// --- process.run: the commands a plugin lists ---

#[test]
fn the_commands_of_a_plugin_that_runs_commands_are_read_as_patterns() {
    let installed = read_text(&manifest_text(
        "capabilities = [\"process.run\"]\nrun = [\"cargo nextest *\", \"git status\"]\n",
    ))
    .unwrap();

    assert_eq!(installed.capabilities, vec![Capability::ProcessRun]);
    let shown: Vec<&str> = installed.run.iter().map(RunPattern::as_str).collect();
    assert_eq!(shown, vec!["cargo nextest *", "git status"]);
}

#[test]
fn a_plugin_that_asks_to_run_commands_must_list_them() {
    let error = read_text(&manifest_text("capabilities = [\"process.run\"]\n")).unwrap_err();

    assert_eq!(error, ManifestError::RunWithoutCommands);
}

#[test]
fn a_plugin_that_lists_commands_must_ask_for_the_capability() {
    let error = read_text(&manifest_text("run = [\"cargo nextest *\"]\n")).unwrap_err();

    assert_eq!(error, ManifestError::CommandsWithoutCapability);
}

#[test]
fn a_command_that_is_not_a_pattern_names_itself_and_says_why() {
    let error = read_text(&manifest_text(
        "capabilities = [\"process.run\"]\nrun = [\"cargo nextest *\", \"/bin/sh -c *\"]\n",
    ))
    .unwrap_err();

    match error {
        ManifestError::BadRunPattern { pattern, reason } => {
            assert_eq!(pattern, "/bin/sh -c *");
            assert!(reason.contains("not a program name"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plugin_without_commands_has_none() {
    assert!(read_text(&manifest_text("")).unwrap().run.is_empty());
}
