//! The text helpers the components share: cutting a string to a number of
//! columns, wrapping it into lines, shortening a path, and placing a box next to
//! a cursor. Widths are display columns, not characters or bytes.

use std::path::{Path, PathBuf};

use ratatui::layout::{Position, Rect};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The start of `text` that fits in `width` columns.
pub(crate) fn head_fitting(text: &str, width: usize) -> &str {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        used += c.width().unwrap_or(0);
        if used > width {
            return &text[..index];
        }
    }
    text
}

/// `text` cut to at most `width` columns.
pub(crate) fn truncate(text: &str, width: usize) -> String {
    head_fitting(text, width).to_owned()
}

/// `text` cut to `width` columns, with `…` where it was cut.
pub(crate) fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let columns = character.width().unwrap_or(0);
        if used + columns + 1 > width {
            break;
        }
        out.push(character);
        used += columns;
    }
    out.push('…');
    out
}

/// `text` broken into lines of at most `width` columns: at a space where there
/// is one, and in the middle of a word that is longer than a line. Line breaks
/// in `text` stay, and so do the spaces at the start of a line (a compiler
/// indents what it quotes).
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for source in text.split('\n') {
        let source = source.trim_end_matches('\r').trim_end();
        let mut current = String::new();
        let mut used = 0;
        // Where the line can be broken: after the last space, and how wide the text before it is.
        let mut space: Option<(usize, usize)> = None;
        for character in source.chars() {
            let columns = character.width().unwrap_or(0);
            // A space may hang over the edge: it is trimmed where the line breaks.
            if used + columns > width && used > 0 && character != ' ' {
                match space.take() {
                    Some((at, before)) => {
                        let rest = current.split_off(at);
                        out.push(current.trim_end().to_owned());
                        current = rest;
                        used -= before;
                    }
                    None => {
                        out.push(std::mem::take(&mut current));
                        used = 0;
                    }
                }
            }
            current.push(character);
            used += columns;
            if character == ' ' && !current.trim().is_empty() {
                space = Some((current.len(), used));
            }
        }
        out.push(current);
    }
    out
}

/// `text` broken into lines of at most `width` columns, words joined by one
/// space: any run of white space becomes a single space and the indent is lost.
/// For prose; use [`wrap_text`] for text whose spaces count.
pub(crate) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word;
            while word.width() > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let (head, tail) = split_at_width(word, width);
                lines.push(head.to_owned());
                word = tail;
            }
            let needed = match line.is_empty() {
                true => word.width(),
                false => line.width() + 1 + word.width(),
            };
            if needed > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// `text` cut into lines of at most `width` columns: at spaces when it can, in
/// the middle of a word when a word is longer than a line. The indent of the
/// first line (lists, code, tables) is kept; a space that ends a line is dropped.
pub(crate) fn wrap_indented(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut used = 0;
    for word in text.split_inclusive(' ') {
        let word_width: usize = word
            .trim_end()
            .chars()
            .filter_map(UnicodeWidthChar::width)
            .sum();
        if used + word_width.min(width) > width && used > 0 {
            out.push(current.trim_end().to_owned());
            current.clear();
            used = 0;
        }
        for c in word.chars() {
            let w = c.width().unwrap_or(0);
            // A space that does not fit ends the line and is not carried to the next.
            if c == ' ' && used + w > width {
                continue;
            }
            if used + w > width {
                out.push(std::mem::take(&mut current));
                used = 0;
            }
            current.push(c);
            used += w;
        }
    }
    out.push(current.trim_end().to_owned());
    out
}

/// `path` as short as is useful: relative to the project when it is in it, else
/// the last three parts.
pub(crate) fn short_path(path: &Path, project: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(project) {
        return slashes(&relative.display().to_string());
    }
    let parts: Vec<_> = path.components().collect();
    let skip = parts.len().saturating_sub(3);
    let kept: PathBuf = parts.into_iter().skip(skip).collect();
    match skip {
        0 => slashes(&kept.display().to_string()),
        _ => format!("…/{}", slashes(&kept.display().to_string())),
    }
}

/// Where a box of `width` by `height` goes on `screen` for a cursor at `anchor`:
/// under the cursor's line, above it when it would run off the bottom, and
/// always inside the screen.
pub(crate) fn place(screen: Rect, anchor: Position, width: u16, height: u16) -> Rect {
    let x = anchor
        .x
        .min(screen.right().saturating_sub(width))
        .max(screen.x);
    let below = anchor.y.saturating_add(1);
    let y = if below + height <= screen.bottom() {
        below
    } else if anchor.y >= screen.y + height {
        anchor.y - height
    } else {
        screen.bottom().saturating_sub(height).max(screen.y)
    };
    Rect {
        x,
        y,
        width: width.min(screen.width),
        height: height.min(screen.height),
    }
}

/// Splits `text` after as many characters as fit in `width` columns (at least
/// one, so the caller always makes progress).
fn split_at_width(text: &str, width: usize) -> (&str, &str) {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        let cell = c.width().unwrap_or(0);
        if used + cell > width && index > 0 {
            return text.split_at(index);
        }
        used += cell;
    }
    (text, "")
}

/// `path` with `/` between the parts, on every system.
fn slashes(path: &str) -> String {
    path.replace('\\', "/")
}
