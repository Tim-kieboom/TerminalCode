//! `file:` URIs, which is how a server names a file.

use std::path::{Path, PathBuf};

use crate::files::paths::without_verbatim_prefix;

/// The URI of the file or folder at `path`, which has to be absolute.
pub(crate) fn file_uri(path: &Path) -> Option<String> {
    let path = without_verbatim_prefix(path);
    let text = path.to_str()?;
    if cfg!(windows) {
        windows_file_uri(text)
    } else {
        text.starts_with('/')
            .then(|| format!("file://{}", encode(text)))
    }
}

/// The path a `file:` URI names, in the form this platform uses.
pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let text = path_from_uri(uri, cfg!(windows))?;
    Some(PathBuf::from(text))
}

/// A Windows path (`C:\a b\c` or `\\host\share\c`) as a URI. Compiled on every
/// platform so that it can be tested on every one.
pub(in crate::lsp) fn windows_file_uri(path: &str) -> Option<String> {
    let slashed = path.replace('\\', "/");
    if let Some(share) = slashed.strip_prefix("//") {
        let (host, rest) = share.split_once('/').unwrap_or((share, ""));
        return (!host.is_empty()).then(|| format!("file://{host}/{}", encode(rest)));
    }
    let bytes = slashed.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    drive.then(|| format!("file:///{}", encode(&slashed)))
}

/// The path part of a `file:` URI as text: for Windows `C:\...` or `\\host\...`,
/// otherwise `/...`. `None` for anything that is not a local file URI.
pub(in crate::lsp) fn path_from_uri(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let (authority, path) = match rest.find('/') {
        Some(slash) => rest.split_at(slash),
        None => (rest, ""),
    };
    let path = decode(path)?;
    if windows {
        if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
            return Some(format!(r"\\{authority}{}", path.replace('/', "\\")));
        }
        // `/C:/x` and the `/c%3A/x` that some editors make, both.
        let drive = path.strip_prefix('/')?;
        let bytes = drive.as_bytes();
        let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
        return has_drive.then(|| drive.replace('/', "\\"));
    }
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    path.starts_with('/').then_some(path)
}

/// Percent-encodes what a URI path does not allow. The slash, and the colon of
/// a drive letter, stay.
fn encode(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'/'
            | b':'
            | b'@'
            | b'!'
            | b'$'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b';'
            | b'=' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}
