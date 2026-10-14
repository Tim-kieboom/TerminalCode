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

    /// The selections of one document, kept in order with overlapping ones
    /// joined. `primary` is an index into `items`; an empty list gives one
    /// cursor at the start.
    pub(crate) fn from_items(items: Vec<Selection>, primary: usize) -> Self {
        if items.is_empty() {
            return Self::default();
        }
        let primary = primary.min(items.len() - 1);
        let mut tagged: Vec<(Selection, bool)> = items
            .into_iter()
            .enumerate()
            .map(|(at, selection)| (selection, at == primary))
            .collect();
        tagged.sort_by_key(|(selection, _)| (selection.start(), selection.end()));

        let mut joined: Vec<(Selection, bool)> = Vec::with_capacity(tagged.len());
        for (selection, is_primary) in tagged {
            match joined.last_mut() {
                Some((last, last_primary)) if touches(last, &selection) => {
                    *last = union(last, &selection);
                    *last_primary |= is_primary;
                }
                _ => joined.push((selection, is_primary)),
            }
        }
        let primary = joined.iter().position(|(_, is_primary)| *is_primary);
        Self {
            primary: primary.unwrap_or(0),
            items: joined.into_iter().map(|(selection, _)| selection).collect(),
        }
    }

    pub(crate) fn primary(&self) -> &Selection {
        &self.items[self.primary]
    }

    pub(crate) fn primary_index(&self) -> usize {
        self.primary
    }

    /// Every selection, earliest first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.items.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn is_single(&self) -> bool {
        self.items.len() == 1
    }

    /// These selections with `selection` added, and it the primary one.
    pub(crate) fn with_added(&self, selection: Selection) -> Self {
        let mut items = self.items.clone();
        items.push(selection);
        let primary = items.len() - 1;
        Self::from_items(items, primary)
    }

    /// Only the primary selection.
    pub(crate) fn collapsed(&self) -> Self {
        Self::single(*self.primary())
    }
}

/// Whether two selections, the first starting no later than the second, run
/// into each other: they overlap, or one of them is a cursor on the other's
/// edge or at the same place.
fn touches(first: &Selection, second: &Selection) -> bool {
    second.start() < first.end()
        || (second.start() == first.end() && (second.is_empty() || first.is_empty()))
}

/// The selection covering both, running the way `first` does.
fn union(first: &Selection, second: &Selection) -> Selection {
    let (start, end) = (first.start(), first.end().max(second.end()));
    let backward = first.head() < first.anchor();
    let (anchor, head) = if backward { (end, start) } else { (start, end) };
    Selection::new(anchor, head).with_desired_column(first.desired_column())
}

impl Default for Selections {
    fn default() -> Self {
        Self::single(Selection::cursor(Position::default()))
    }
}
