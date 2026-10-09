use crate::pty::Shell;

#[test]
fn a_shell_setting_names_the_program() {
    assert_eq!(
        Shell::from_setting(Some("/usr/bin/zsh")).program,
        "/usr/bin/zsh"
    );
}

#[test]
fn an_empty_or_missing_setting_falls_back_to_the_platform_shell() {
    let fallback = Shell::from_setting(None);

    assert!(fallback.program == "/bin/sh" || fallback.program == "cmd.exe");
    assert_eq!(Shell::from_setting(Some("  ")), fallback);
    assert_eq!(Shell::from_setting(Some("")), fallback);
}

#[test]
fn a_shell_starts_without_arguments() {
    assert!(Shell::detect().args.is_empty());
}
