use std::ffi::OsString;
use std::path::Path;

use super::{arguments, open, system_opener};

#[test]
fn the_path_comes_last_after_what_the_opener_needs_first() {
    let given = arguments(&["a", "b"], Path::new("/p/-x.html"));

    assert_eq!(
        given,
        [OsString::from("a"), "b".into(), "/p/-x.html".into()]
    );
}

#[test]
fn the_system_has_an_opener() {
    let (program, _) = system_opener();

    assert!(!program.is_empty());
}

#[test]
fn a_program_that_is_not_there_is_an_error() {
    let result = open("/nonexistent/opener", &[], Path::new("/p/a.html"));

    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
}

#[cfg(unix)]
#[test]
fn a_program_that_starts_is_a_success_whatever_it_does_after() {
    // `false` starts and ends with an error: that is not looked at.
    assert!(open("false", &[], Path::new("/p/a.html")).is_ok());
}
