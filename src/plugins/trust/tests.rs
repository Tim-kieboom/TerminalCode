use super::*;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn the_first_install_of_a_version_pins_it() {
    let dir = tempfile::tempdir().unwrap();
    Pins::trust(dir.path(), "git", "0.1.0", A).unwrap();
    let pins = Pins::load(dir.path()).unwrap();
    assert_eq!(pins.check("git", "0.1.0", A), Ok(false));
}

#[test]
fn the_same_hash_again_and_a_new_version_are_fine() {
    let dir = tempfile::tempdir().unwrap();
    Pins::trust(dir.path(), "git", "0.1.0", A).unwrap();
    Pins::trust(dir.path(), "git", "0.1.0", A).unwrap();
    Pins::trust(dir.path(), "git", "0.2.0", B).unwrap();
    let pins = Pins::load(dir.path()).unwrap();
    assert_eq!(pins.check("git", "0.1.0", A), Ok(false));
    assert_eq!(pins.check("git", "0.2.0", B), Ok(false));
}

#[test]
fn a_version_that_changed_is_refused_and_keeps_its_pin() {
    let dir = tempfile::tempdir().unwrap();
    Pins::trust(dir.path(), "git", "0.1.0", A).unwrap();
    let error = Pins::trust(dir.path(), "git", "0.1.0", B).unwrap_err();
    assert!(matches!(error, TrustError::Changed { .. }));
    assert!(error.to_string().contains("must not change"));
    assert_eq!(
        Pins::load(dir.path()).unwrap().check("git", "0.1.0", A),
        Ok(false)
    );
}

#[test]
fn pins_of_one_plugin_do_not_touch_another() {
    let dir = tempfile::tempdir().unwrap();
    Pins::trust(dir.path(), "git", "0.1.0", A).unwrap();
    Pins::trust(dir.path(), "other", "0.1.0", B).unwrap();
}

#[test]
fn a_file_that_does_not_read_is_an_error_and_is_not_replaced() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(TRUST_FILE), "not json").unwrap();
    assert!(matches!(
        Pins::trust(dir.path(), "git", "0.1.0", A),
        Err(TrustError::File(_))
    ));
    assert_eq!(
        fs::read_to_string(dir.path().join(TRUST_FILE)).unwrap(),
        "not json"
    );
}
