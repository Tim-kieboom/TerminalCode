use std::ops::{Index, IndexMut};

#[derive(Debug, Clone)]
pub enum TextBuffer {
    Single([String; 1]),
    Multi(Vec<String>),
}
impl TextBuffer {
    pub fn new_single_line(line: String) -> Self {
        Self::Single([line])
    }

    pub fn new_multi_line(lines: Vec<String>) -> Self {
        Self::Multi(lines)
    }

    pub fn clear(&mut self) {
        match self {
            TextBuffer::Single(line) => line[0].clear(),
            TextBuffer::Multi(items) => {
                items.clear();
                items.push(String::new());
            }
        }
    }

    pub fn get(&self, i: usize) -> Option<&String> {
        match self {
            TextBuffer::Single(line) => line.get(i),
            TextBuffer::Multi(items) => items.get(i),
        }
    }

    pub const fn len(&self) -> usize {
        match self {
            TextBuffer::Single(_) => 1,
            TextBuffer::Multi(items) => items.len(),
        }
    }

    pub const fn as_slice<'a>(&'a self) -> &'a [String] {
        match self {
            TextBuffer::Single(line) => line,
            TextBuffer::Multi(items) => items.as_slice(),
        }
    }

    pub const fn as_mut_slice<'a>(&'a mut self) -> &'a mut [String] {
        match self {
            TextBuffer::Single(line) => line,
            TextBuffer::Multi(items) => items.as_mut_slice(),
        }
    }

    pub const fn try_as_vec_mut<'a>(&'a mut self) -> Option<&'a mut Vec<String>> {
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
