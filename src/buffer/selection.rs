use super::position::Position;

/// A range of text with a direction. A cursor is a selection whose anchor
/// and head coincide.
///
/// `desired_column` remembers the column vertical movement is aiming for, so
/// moving through a short line and back lands in the original column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selection {
    anchor: Position,
    head: Position,
    desired_column: Option<usize>,
}

impl Selection {
    pub(crate) fn new(anchor: Position, head: Position) -> Self {
        Self {
            anchor,
            head,
            desired_column: None,
        }
    }

    pub(crate) fn cursor(at: Position) -> Self {
        Self::new(at, at)
    }

    pub(crate) fn with_desired_column(self, desired_column: Option<usize>) -> Self {
        Self {
            desired_column,
            ..self
        }
    }

    pub(crate) fn anchor(&self) -> Position {
        self.anchor
    }

    pub(crate) fn head(&self) -> Position {
        self.head
    }

    pub(crate) fn desired_column(&self) -> Option<usize> {
        self.desired_column
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The earlier end, regardless of direction.
    pub(crate) fn start(&self) -> Position {
        self.anchor.min(self.head)
    }

    /// The later end, regardless of direction.
    pub(crate) fn end(&self) -> Position {
        self.anchor.max(self.head)
    }
}

/// One or more selections, one of them primary. Never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selections {
    items: Vec<Selection>,
    primary: usize,
}

impl Selections {
    pub(crate) fn single(selection: Selection) -> Self {
        Self {
            items: vec![selection],
            primary: 0,
        }
    }

    pub(crate) fn primary(&self) -> &Selection {
        &self.items[self.primary]
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
}

impl Default for Selections {
    fn default() -> Self {
        Self::single(Selection::cursor(Position::default()))
    }
}
