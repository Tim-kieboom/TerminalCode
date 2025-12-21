use std::ops::{Index, IndexMut};

/// Unified text buffer for single-line and multi-line use.
///
/// Provides uniform `Index`/`IndexMut`/`as_slice()`/`as_mut_slice()` access to both `[String; 1]` and `Vec<String>`
/// via enum variants. Enables shared logic across UI
#[derive(Debug, Clone)]
pub enum TextBuffer {
    Single([String; 1]),
    Multi(Vec<String>),
}
impl TextBuffer {
    /// Creates a single-line buffer
    pub fn new_single_line(line: String) -> Self {
        Self::Single([line])
    }

    /// Creates a multi-line buffer
    pub fn new_multi_line(lines: Vec<String>) -> Self {
        Self::Multi(lines)
    }

    /// Clears buffer contents, ensuring at least one empty line exists.
    ///
    /// Single-line: clears the line. Multi-line: clears vector + adds empty line.
    pub fn clear(&mut self) {
        match self {
            TextBuffer::Single(line) => line[0].clear(),
            TextBuffer::Multi(items) => {
                items.clear();
                items.push(String::new());
            }
        }
    }

    /// Safe immutable access to line by index.
    pub fn get(&self, i: usize) -> Option<&String> {
        match self {
            TextBuffer::Single(line) => line.get(i),
            TextBuffer::Multi(items) => items.get(i),
        }
    }

    /// Returns number of lines in buffer.
    pub const fn line_count(&self) -> usize {
        match self {
            TextBuffer::Single(_) => 1,
            TextBuffer::Multi(items) => items.len(),
        }
    }

    /// Converts to immutable slice
    pub const fn as_slice(&self) -> &[String] {
        match self {
            TextBuffer::Single(line) => line,
            TextBuffer::Multi(items) => items.as_slice(),
        }
    }

    /// Converts to mutable slice
    pub const fn as_mut_slice(&mut self) -> &mut [String] {
        match self {
            TextBuffer::Single(line) => line,
            TextBuffer::Multi(items) => items.as_mut_slice(),
        }
    }

    /// Attempts to get mutable `Some(&mut Vec<String>)` reference (returns `None` for Single).
    ///
    /// Used by multi-line only operations that need `Vec` methods like `push/pop`.
    pub const fn try_as_vec_mut(&mut self) -> Option<&mut Vec<String>> {
        match self {
            TextBuffer::Single(_) => None,
            TextBuffer::Multi(items) => Some(items),
        }
    }
}

impl Index<usize> for TextBuffer {
    type Output = String;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            TextBuffer::Single(line) => &line[index],
            TextBuffer::Multi(items) => &items[index],
        }
    }
}
impl IndexMut<usize> for TextBuffer {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match self {
            TextBuffer::Single(line) => &mut line[index],
            TextBuffer::Multi(items) => &mut items[index],
        }
    }
}
impl AsRef<[String]> for TextBuffer {
    fn as_ref(&self) -> &[String] {
        self.as_slice()
    }
}
impl AsMut<[String]> for TextBuffer {
    fn as_mut(&mut self) -> &mut [String] {
        self.as_mut_slice()
    }
}
