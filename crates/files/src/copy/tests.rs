use std::fs;

use crate::entries::EntryError;

use super::{copy_into, free_name};

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    fs::create_dir(dir.path().join("other")).unwrap();
    fs::write(dir.path().join("a.txt"), "alpha").unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("src/deep/x.rs"), "x").unwrap();
    dir
}

#[test]
fn a_file_is_copied_under_its_own_name_into_another_folder() {
    let dir = tree();

    let copy = copy_into(&dir.path().join("a.txt"), &dir.path().join("other")).unwrap();

    assert_eq!(copy, dir.path().join("other/a.txt"));
    assert_eq!(fs::read_to_string(copy).unwrap(), "alpha");
    assert!(dir.path().join("a.txt").exists(), "the original stays");
}

#[test]
fn a_name_that_is_taken_gets_copy_and_then_a_number_before_the_extension() {
    let dir = tree();
    let from = dir.path().join("a.txt");

    let first = copy_into(&from, dir.path()).unwrap();
    let second = copy_into(&from, dir.path()).unwrap();
    let third = copy_into(&from, dir.path()).unwrap();

    assert_eq!(first, dir.path().join("a copy.txt"));
    assert_eq!(second, dir.path().join("a copy 2.txt"));
    assert_eq!(third, dir.path().join("a copy 3.txt"));
}

#[test]
fn a_folder_or_a_name_with_no_extension_gets_the_words_at_the_end() {
    let dir = tree();
    fs::write(dir.path().join(".env"), "k=v").unwrap();
    fs::create_dir(dir.path().join("v1.2")).unwrap();

    let folder = copy_into(&dir.path().join("src"), dir.path()).unwrap();
    let dotfile = copy_into(&dir.path().join(".env"), dir.path()).unwrap();
    let dotted = copy_into(&dir.path().join("v1.2"), dir.path()).unwrap();

    assert_eq!(folder, dir.path().join("src copy"));
    assert_eq!(dotfile, dir.path().join(".env copy"));
    assert_eq!(
        dotted,
        dir.path().join("v1.2 copy"),
        "not a file: no extension"
    );
}

#[test]
fn a_folder_is_copied_with_everything_in_it() {
    let dir = tree();

    let copy = copy_into(&dir.path().join("src"), &dir.path().join("other")).unwrap();

    assert_eq!(copy, dir.path().join("other/src"));
    assert_eq!(
        fs::read_to_string(copy.join("main.rs")).unwrap(),
        "fn main() {}"
    );
    assert_eq!(fs::read_to_string(copy.join("deep/x.rs")).unwrap(), "x");
}

#[test]
fn nothing_is_replaced() {
    let dir = tree();
    fs::write(dir.path().join("other/a.txt"), "mine").unwrap();

    let copy = copy_into(&dir.path().join("a.txt"), &dir.path().join("other")).unwrap();

    assert_eq!(copy, dir.path().join("other/a copy.txt"));
    assert_eq!(
        fs::read_to_string(dir.path().join("other/a.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn a_folder_cannot_be_copied_into_itself_or_into_a_folder_in_it() {
    let dir = tree();
    let src = dir.path().join("src");

    for into in [src.clone(), src.join("deep")] {
        let error = copy_into(&src, &into).unwrap_err();
        assert!(matches!(error, EntryError::IntoItself(_)), "{error}");
    }
    assert!(!src.join("src").exists());
    assert!(!src.join("deep/src").exists());
}

#[test]
fn a_folder_can_be_copied_next_to_itself() {
    let dir = tree();

    let copy = copy_into(&dir.path().join("src"), dir.path()).unwrap();

    assert_eq!(copy, dir.path().join("src copy"));
    assert!(copy.join("deep/x.rs").exists());
}

#[cfg(unix)]
#[test]
fn a_link_is_copied_as_a_link_and_is_not_followed() {
    use std::os::unix::fs::symlink;
    let dir = tree();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("big.bin"), "secret").unwrap();
    symlink(outside.path(), dir.path().join("src/out")).unwrap();
    symlink("main.rs", dir.path().join("src/rel")).unwrap();
    symlink("/nonexistent/there", dir.path().join("src/dangling")).unwrap();

    let copy = copy_into(&dir.path().join("src"), &dir.path().join("other")).unwrap();

    for name in ["out", "rel", "dangling"] {
        let made = copy.join(name);
        assert!(
            fs::symlink_metadata(&made)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{name}"
        );
        assert_eq!(
            fs::read_link(&made).unwrap(),
            fs::read_link(dir.path().join("src").join(name)).unwrap(),
            "{name} points where the original does"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_that_is_copied_to_a_taken_name_is_numbered_too() {
    use std::os::unix::fs::symlink;
    let dir = tree();
    symlink("a.txt", dir.path().join("l")).unwrap();

    let copy = copy_into(&dir.path().join("l"), dir.path()).unwrap();

    assert_eq!(copy, dir.path().join("l copy"));
}

#[cfg(unix)]
#[test]
fn a_failure_half_way_names_what_failed_and_leaves_what_was_done() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tree();
    let locked = dir.path().join("src/locked.txt");
    fs::write(&locked, "no").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&locked).is_ok() {
        return; // running as root: nothing is unreadable
    }

    let error = copy_into(&dir.path().join("src"), &dir.path().join("other")).unwrap_err();

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(error.to_string().contains("locked.txt"), "{error}");
    assert!(dir.path().join("other/src").is_dir(), "what was done stays");
}

#[test]
fn a_file_that_is_not_there_is_an_error_naming_it() {
    let dir = tree();

    let error = copy_into(&dir.path().join("gone.txt"), dir.path()).unwrap_err();

    assert!(error.to_string().contains("gone.txt"), "{error}");
}

#[test]
fn a_free_name_is_the_plain_one_when_nothing_is_there() {
    let dir = tree();

    assert_eq!(
        free_name(dir.path(), "new.txt".as_ref(), true),
        Some(dir.path().join("new.txt"))
    );
}
