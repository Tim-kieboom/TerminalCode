use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::plugin_api::Abi;
use crate::plugins::index::{Entry, IndexError};
use crate::plugins::install::{
    DESCRIPTION_FILE, InstallError, MARKER_FILE, Policy, STAGING, Stage, fetch, fetch_index, hex,
    install, marker, remove, sibling,
};
use crate::plugins::manifest::{self, Host, Installed, MANIFEST_FILE, ManifestError};

const HOST: Host<'static> = Host {
    abi: Abi::CURRENT,
    version: "0.6.2",
};
const LOCAL: Policy = Policy::local(&["127.0.0.1"]);

pub(crate) enum Reply {
    Body(Vec<u8>),
    Redirect(String),
    Status(u16),
    /// Takes the request and never answers.
    Hang,
}

/// A server on this machine that answers the paths it was given; returns its address.
pub(crate) fn serve(routes: HashMap<&'static str, Reply>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                if stream.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            let text = String::from_utf8_lossy(&request).into_owned();
            let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let head = match routes.get(path.as_str()) {
                Some(Reply::Body(body)) => {
                    let mut out =
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len())
                            .into_bytes();
                    out.extend_from_slice(body);
                    out
                }
                Some(Reply::Redirect(to)) => {
                    format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\n\r\n")
                        .into_bytes()
                }
                Some(Reply::Hang) => {
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
                Some(Reply::Status(code)) => {
                    format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\n\r\n").into_bytes()
                }
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
            };
            let _ = stream.write_all(&head);
        }
    });
    address
}

fn manifest_text(version: &str) -> String {
    format!(
        "id = \"echo\"\nname = \"Echo\"\nversion = \"{version}\"\nabi = \"1.3\"\nwasm = \"echo.wasm\"\n"
    )
}

fn entry(address: &str, version: &str, module: &[u8]) -> Entry {
    Entry {
        id: "echo".into(),
        name: "Echo".into(),
        description: String::new(),
        version: version.into(),
        manifest: format!("{address}/manifest.toml"),
        module: format!("{address}/echo.wasm"),
        sha256: hex(&Sha256::digest(module)),
        abi: "1.3".into(),
        requires: None,
        readme: None,
        source: None,
    }
}

fn routes(version: &str, module: &[u8]) -> HashMap<&'static str, Reply> {
    HashMap::from([
        (
            "/manifest.toml",
            Reply::Body(manifest_text(version).into_bytes()),
        ),
        ("/echo.wasm", Reply::Body(module.to_vec())),
    ])
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

fn install_entry(entry: &Entry, dir: &Path) -> Result<Installed, InstallError> {
    install(entry, dir, HOST, LOCAL, &never(), |_| {})
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name != super::super::trust::TRUST_FILE)
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn only_https_from_the_hosts_of_github_is_allowed() {
    let github = Policy::GITHUB;
    for url in [
        "https://github.com/a/b/releases/download/v1/x.wasm",
        "https://raw.githubusercontent.com/a/b/main/index.json",
        "https://objects.githubusercontent.com/x?y=1",
        "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sp=r",
    ] {
        assert!(github.allows(url), "{url}");
    }
    for url in [
        "http://github.com/a",
        "https://evil.example/a",
        "https://github.com@evil.example/a",
        "https://github.com.evil.example/a",
        "https://github.com:8443/a",
        "file:///etc/passwd",
        "github.com/a",
        "ftp://github.com/a",
    ] {
        assert!(!github.allows(url), "{url}");
    }
}

#[test]
fn a_body_is_fetched_and_a_missing_one_says_so() {
    let address = serve(HashMap::from([("/a", Reply::Body(b"hello".to_vec()))]));
    assert_eq!(
        fetch(&format!("{address}/a"), LOCAL, 100, &never()).unwrap(),
        b"hello"
    );
    let error = fetch(&format!("{address}/missing"), LOCAL, 100, &never()).unwrap_err();
    assert!(matches!(error, InstallError::Status { status: 404, .. }));
}

#[test]
fn a_redirect_is_followed_only_to_an_allowed_place() {
    let address = serve(HashMap::from([
        ("/to", Reply::Body(b"there".to_vec())),
        ("/good", Reply::Redirect("/to".into())),
        ("/bad", Reply::Redirect("http://localhost:1/to".into())),
    ]));
    // A redirect with a relative place is not followed to a place unknown to the policy.
    let good = fetch(&format!("{address}/good"), LOCAL, 100, &never());
    assert!(good.is_err());
    let bad = fetch(&format!("{address}/bad"), LOCAL, 100, &never()).unwrap_err();
    assert!(matches!(bad, InstallError::ForbiddenUrl(_)), "{bad}");
}

#[test]
fn a_redirect_to_an_absolute_allowed_url_is_followed() {
    let listener_address = serve(HashMap::from([("/to", Reply::Body(b"there".to_vec()))]));
    let from = serve(HashMap::from([(
        "/go",
        Reply::Redirect(format!("{listener_address}/to")),
    )]));
    assert_eq!(
        fetch(&format!("{from}/go"), LOCAL, 100, &never()).unwrap(),
        b"there"
    );
}

#[test]
fn a_body_past_the_limit_is_refused() {
    let address = serve(HashMap::from([("/a", Reply::Body(vec![0; 5000]))]));
    let error = fetch(&format!("{address}/a"), LOCAL, 1000, &never()).unwrap_err();
    assert!(matches!(error, InstallError::TooLarge { limit: 1000, .. }));
}

#[test]
fn a_stop_before_or_during_the_download_ends_it() {
    let address = serve(HashMap::from([("/a", Reply::Body(vec![0; 5000]))]));
    let stopped = AtomicBool::new(true);
    let error = fetch(&format!("{address}/a"), LOCAL, 100_000, &stopped).unwrap_err();
    assert!(matches!(error, InstallError::Cancelled));
}

#[test]
fn an_install_puts_the_files_and_the_marker_in_place() {
    let module = b"\0asm-module";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    let entry = entry(&address, "1.0.0", module);

    let installed = install_entry(&entry, dir.path()).unwrap();

    assert_eq!(installed.id(), "echo");
    let folder = dir.path().join("echo");
    assert_eq!(fs::read(folder.join("echo.wasm")).unwrap(), module);
    assert!(folder.join(MANIFEST_FILE).is_file());
    assert_eq!(marker(&folder).unwrap().entry, entry);
    assert_eq!(marker(&folder).unwrap().sha256, entry.sha256);
    // The workshop is gone.
    assert_eq!(names(dir.path()), ["echo"]);
}

#[test]
fn the_stages_are_reported_in_order() {
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    let mut seen = Vec::new();
    install(
        &entry(&address, "1.0.0", module),
        dir.path(),
        HOST,
        LOCAL,
        &never(),
        |stage| seen.push(stage),
    )
    .unwrap();

    assert_eq!(
        &seen,
        &[
            Stage::Manifest,
            Stage::Module,
            Stage::Checking,
            Stage::Installing,
        ],
    )
}

#[test]
fn a_module_that_is_not_the_one_in_the_index_writes_nothing() {
    let address = serve(routes("1.0.0", b"the swapped module"));
    let dir = tempfile::tempdir().unwrap();
    let promised = entry(&address, "1.0.0", b"the module the index named");

    let error = install_entry(&promised, dir.path()).unwrap_err();

    assert!(matches!(error, InstallError::HashMismatch));
    assert!(
        error
            .to_string()
            .contains("does not match what the index promised")
    );
    assert!(names(dir.path()).is_empty());
}

#[test]
fn a_stop_during_an_install_writes_nothing() {
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    let stopped = AtomicBool::new(true);
    let error = install(
        &entry(&address, "1.0.0", module),
        dir.path(),
        HOST,
        LOCAL,
        &stopped,
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, InstallError::Cancelled));
    assert!(names(dir.path()).is_empty());
}

#[test]
fn a_manifest_that_is_not_the_entry_writes_nothing() {
    let module = b"m";
    let address = serve(routes("2.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    let error = install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap_err();
    assert!(
        matches!(error, InstallError::NotWhatTheIndexSaid(_)),
        "{error}"
    );
    assert!(names(dir.path()).is_empty());
}

#[test]
fn a_folder_of_the_users_own_is_never_touched() {
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    let mine = dir.path().join("echo");
    fs::create_dir(&mine).unwrap();
    fs::write(mine.join("mine.txt"), "mine").unwrap();

    let error = install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap_err();

    assert!(matches!(error, InstallError::Local(_)));
    assert!(error.to_string().contains("remove it by hand"));
    assert_eq!(fs::read_to_string(mine.join("mine.txt")).unwrap(), "mine");
    assert!(matches!(
        remove(dir.path(), "echo"),
        Err(InstallError::NotManaged(_))
    ));
    assert!(mine.is_dir());
}

#[test]
fn an_update_replaces_the_old_copy() {
    let dir = tempfile::tempdir().unwrap();
    let old = serve(routes("1.0.0", b"old"));
    install_entry(&entry(&old, "1.0.0", b"old"), dir.path()).unwrap();
    let new = serve(routes("1.1.0", b"newer"));

    let installed = install_entry(&entry(&new, "1.1.0", b"newer"), dir.path()).unwrap();

    assert_eq!(installed.manifest.version, "1.1.0");
    assert_eq!(
        fs::read(dir.path().join("echo/echo.wasm")).unwrap(),
        b"newer"
    );
    assert_eq!(names(dir.path()), ["echo"]);
}

#[test]
fn a_failed_update_keeps_the_old_copy() {
    let dir = tempfile::tempdir().unwrap();
    let old = serve(routes("1.0.0", b"old"));
    install_entry(&entry(&old, "1.0.0", b"old"), dir.path()).unwrap();
    let broken = serve(routes("1.1.0", b"not what was promised"));

    let error = install_entry(&entry(&broken, "1.1.0", b"newer"), dir.path()).unwrap_err();

    assert!(matches!(error, InstallError::HashMismatch));
    assert_eq!(fs::read(dir.path().join("echo/echo.wasm")).unwrap(), b"old");
    assert_eq!(
        marker(&dir.path().join("echo")).unwrap().entry.version,
        "1.0.0"
    );
}

#[test]
fn what_needs_a_newer_editor_is_not_downloaded() {
    let dir = tempfile::tempdir().unwrap();
    let mut wants_more = entry("http://127.0.0.1:1", "1.0.0", b"m");
    wants_more.requires = Some("9.0.0".into());
    let error = install_entry(&wants_more, dir.path()).unwrap_err();
    assert!(matches!(error, InstallError::Unavailable(_)), "{error}");
}

#[test]
fn removing_what_was_installed_deletes_the_folder() {
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();
    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();

    remove(dir.path(), "echo").unwrap();

    assert!(names(dir.path()).is_empty());
}

#[test]
fn an_id_that_leaves_the_folder_is_not_removed() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        remove(dir.path(), "../x"),
        Err(InstallError::NotManaged(_))
    ));
}

#[test]
fn the_index_is_fetched_and_kept() {
    let text = r#"{"format":1,"plugins":[]}"#;
    let address = serve(HashMap::from([(
        "/index.json",
        Reply::Body(text.as_bytes().to_vec()),
    )]));
    let config = tempfile::tempdir().unwrap();

    let index = fetch_index(
        &format!("{address}/index.json"),
        LOCAL,
        Some(config.path()),
        &never(),
    )
    .unwrap();

    assert!(index.entries.is_empty());
    assert_eq!(
        fs::read_to_string(config.path().join("plugin-index.json")).unwrap(),
        text
    );
}

#[test]
fn an_index_that_is_not_good_is_not_kept() {
    let address = serve(HashMap::from([(
        "/index.json",
        Reply::Body(br#"{"format":9}"#.to_vec()),
    )]));
    let config = tempfile::tempdir().unwrap();
    let error = fetch_index(
        &format!("{address}/index.json"),
        LOCAL,
        Some(config.path()),
        &never(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        InstallError::Index(IndexError::NewerFormat(9))
    ));
    assert!(names(config.path()).is_empty());
}

#[test]
fn the_workshop_is_not_a_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    // A half-made plugin left in the workshop by a crash is not listed.
    let stale = dir.path().join(STAGING).join("echo");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join(MANIFEST_FILE), manifest_text("1.0.0")).unwrap();
    assert!(manifest::scan(dir.path(), HOST).is_empty());

    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();
    assert_eq!(manifest::scan(dir.path(), HOST).len(), 1);
}

#[test]
fn a_stop_is_heard_while_the_server_says_nothing() {
    let address = serve(HashMap::from([("/a", Reply::Hang)]));
    let stopped = std::sync::Arc::new(AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&stopped);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();

    let error = fetch(&format!("{address}/a"), LOCAL, 100, &stopped).unwrap_err();

    assert!(matches!(error, InstallError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_version_that_changes_after_it_was_installed_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let first = b"the module";
    let address = serve(routes("1.0.0", first));
    install_entry(&entry(&address, "1.0.0", first), dir.path()).unwrap();
    remove(dir.path(), "echo").unwrap();

    // The same version, now with another module and a matching index hash.
    let second = b"a replaced module";
    let address = serve(routes("1.0.0", second));
    let error = install_entry(&entry(&address, "1.0.0", second), dir.path()).unwrap_err();

    assert!(matches!(error, InstallError::Untrusted(_)));
    assert!(error.to_string().contains("must not change"));
    assert!(!dir.path().join("echo").exists());
}

#[test]
fn a_reinstall_of_the_same_module_and_a_new_version_are_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let module = b"the module";
    let address = serve(routes("1.0.0", module));
    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();
    remove(dir.path(), "echo").unwrap();
    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();

    let newer = b"the newer module";
    let address = serve(routes("1.1.0", newer));
    install_entry(&entry(&address, "1.1.0", newer), dir.path()).unwrap();
}

#[test]
fn the_pins_file_is_not_a_plugin() {
    let dir = tempfile::tempdir().unwrap();
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();
    assert!(dir.path().join(super::super::trust::TRUST_FILE).exists());
    assert_eq!(manifest::scan(dir.path(), HOST).len(), 1);
}

#[test]
fn an_installed_module_is_read_only_while_it_is_the_one_installed() {
    let dir = tempfile::tempdir().unwrap();
    let module = b"the module";
    let address = serve(routes("1.0.0", module));
    let installed = install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();
    assert_eq!(installed.module().unwrap(), module);

    fs::write(installed.dir.join(&installed.manifest.wasm), b"swapped").unwrap();

    let error = installed.module().unwrap_err();
    assert!(matches!(error, ManifestError::ModuleChanged(_)), "{error}");
    assert!(error.to_string().contains("must not change"), "{error}");
}

#[test]
fn a_module_and_marker_swapped_together_still_meet_the_pin() {
    let dir = tempfile::tempdir().unwrap();
    let module = b"the module";
    let address = serve(routes("1.0.0", module));
    let installed = install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();

    // Someone rewrites the module and the marker's hash, but not the pins.
    let swapped = b"swapped";
    fs::write(installed.dir.join(&installed.manifest.wasm), swapped).unwrap();
    let mut marker = marker(&installed.dir).unwrap();
    marker.sha256 = hex(&Sha256::digest(swapped));
    fs::write(
        installed.dir.join(MARKER_FILE),
        serde_json::to_string(&marker).unwrap(),
    )
    .unwrap();

    assert!(match installed.module() {
        Err(ManifestError::ModuleChanged(_)) => true,
        _ => false,
    });
}

#[test]
fn a_plugin_of_the_users_own_is_not_checked() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("echo");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(MANIFEST_FILE), manifest_text("1.0.0")).unwrap();
    fs::write(folder.join("echo.wasm"), b"mine").unwrap();
    let found = manifest::scan(dir.path(), HOST);
    let own = found.into_iter().next().unwrap().unwrap();
    assert_eq!(own.module().unwrap(), b"mine");
}

#[test]
fn the_description_next_to_the_manifest_is_installed_with_the_plugin() {
    let module = b"m";
    let mut served = routes("1.0.0", module);
    served.insert("/description.md", Reply::Body(b"# What it is\n".to_vec()));
    let address = serve(served);
    let dir = tempfile::tempdir().unwrap();

    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("echo").join(DESCRIPTION_FILE)).unwrap(),
        "# What it is\n"
    );
}

#[test]
fn a_plugin_without_a_description_installs_without_one() {
    let module = b"m";
    let address = serve(routes("1.0.0", module));
    let dir = tempfile::tempdir().unwrap();

    install_entry(&entry(&address, "1.0.0", module), dir.path()).unwrap();

    assert!(!dir.path().join("echo").join(DESCRIPTION_FILE).exists());
}

#[test]
fn the_description_is_found_next_to_the_manifest_whatever_its_address() {
    assert_eq!(
        sibling(
            "https://github.com/a/b/releases/download/v1/manifest.toml",
            "description.md"
        ),
        "https://github.com/a/b/releases/download/v1/description.md"
    );
    assert_eq!(sibling("manifest.toml", "description.md"), "description.md");
}
