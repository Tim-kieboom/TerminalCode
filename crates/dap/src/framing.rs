//! The base protocol: each message is `Content-Length: N`, a blank line, and
//! N bytes of JSON, on a stream both sides read and write. The Debug Adapter Protocol frames
//! its messages as the Language Server Protocol does (this is the framing of `trcode-lsp`,
//! kept here so that the two crates do not depend on each other).

use std::io::{self, BufRead, Read, Write};

use serde_json::Value;
use thiserror::Error;

/// The biggest message accepted. A server can send a lot at once (a big
/// answer), but not this much; a length past it is a broken stream.
pub const MAX_BODY: usize = 64 * 1024 * 1024;
/// The longest header line accepted.
const MAX_HEADER_LINE: u64 = 8 * 1024;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("a message header is not valid: {0}")]
    Header(String),
    #[error("a message has no Content-Length header")]
    NoLength,
    #[error("a message of {0} bytes is over the limit of {MAX_BODY} bytes")]
    TooLarge(usize),
    #[error("a message body is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the stream ended in the middle of a message")]
    Truncated,
}

/// Reads the next message, or `None` when the stream ended between messages.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, FrameError> {
    let Some(length) = read_headers(reader)? else {
        return Ok(None);
    };
    if length > MAX_BODY {
        return Err(FrameError::TooLarge(length));
    }
    // Read what is there instead of reserving `length` first: a length that
    // lies must not cost memory.
    let mut body = Vec::new();
    let read = reader.take(length as u64).read_to_end(&mut body)?;
    if read < length {
        return Err(FrameError::Truncated);
    }
    Ok(Some(serde_json::from_slice(&body)?))
}

/// The `Content-Length` of the next message. `None` is a clean end of the
/// stream: nothing at all came before it.
fn read_headers(reader: &mut impl BufRead) -> Result<Option<usize>, FrameError> {
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = Vec::new();
        let read = reader
            .by_ref()
            .take(MAX_HEADER_LINE + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return match first {
                true => Ok(None),
                false => Err(FrameError::Truncated),
            };
        }
        first = false;
        if !line.ends_with(b"\n") {
            return match read as u64 > MAX_HEADER_LINE {
                true => Err(FrameError::Header("a header line is too long".to_owned())),
                false => Err(FrameError::Truncated),
            };
        }

        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return length.map(Some).ok_or(FrameError::NoLength);
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(FrameError::Header(format!("\"{line}\" has no colon")));
        };
        // Other headers (Content-Type) are allowed and mean nothing to us.
        if name.trim().eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            length = Some(value.parse().map_err(|_| {
                FrameError::Header(format!("Content-Length \"{value}\" is not a number"))
            })?);
        }
    }
}

/// Writes `message` with its header and flushes.
pub fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
