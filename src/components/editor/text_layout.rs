//! Turns buffer text into terminal cells. Tabs and wide characters only exist
//! here: the buffer and cursor work in graphemes.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Columns a tab advances to the next multiple of.
pub(super) const TAB_WIDTH: usize = 4;

/// One visible grapheme of a line, ready to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cell {
    /// What to draw: the grapheme, or spaces for a tab or a clipped wide char.
    pub(super) text: String,
    /// Index of the grapheme within its line.
    pub(super) grapheme: usize,
}

/// Number of decimal digits in `number`.
pub(super) fn digits(number: usize) -> usize {
    number.checked_ilog10().map_or(1, |log| log as usize + 1)
}

fn width_at(grapheme: &str, column: usize) -> usize {
    if grapheme == "\t" {
        return TAB_WIDTH - column % TAB_WIDTH;
    }
    grapheme.width()
}

/// Display column where the grapheme at `grapheme_column` starts.
pub(super) fn display_column(text: &str, grapheme_column: usize) -> usize {
    text.graphemes(true)
        .take(grapheme_column)
        .fold(0, |column, grapheme| column + width_at(grapheme, column))
}

/// Grapheme column of the cell boundary nearest to display column
/// `display_column`: a click in the left half of a character lands before it,
/// in the right half after it, and past the end of the line at the end.
pub(super) fn column_at_display(text: &str, display_column: usize) -> usize {
    let mut column = 0;
    for (index, grapheme) in text.graphemes(true).enumerate() {
        let width = width_at(grapheme, column);
        if display_column < column + width.div_ceil(2) {
            return index;
        }
        column += width;
    }
    text.graphemes(true).count()
}

/// The cells of `text` that fall in display columns `left..left + width`.
/// Zero-width graphemes are skipped. A wide grapheme or tab cut by an edge of
/// the window is replaced by spaces for its visible part.
pub(super) fn visible_cells(text: &str, left: usize, width: usize) -> Vec<Cell> {
    let right = left + width;
    let mut cells = Vec::new();
    let mut column = 0;

    for (index, grapheme) in text.graphemes(true).enumerate() {
        let cell_width = width_at(grapheme, column);
        let (start, end) = (column, column + cell_width);
        column = end;

        if cell_width == 0 || end <= left {
            continue;
        }
        if start >= right {
            break;
        }

        let clipped = start < left || end > right;
        let text = if grapheme == "\t" || clipped {
            " ".repeat(end.min(right) - start.max(left))
        } else {
            grapheme.to_owned()
        };
        cells.push(Cell {
            text,
            grapheme: index,
        });
    }
    cells
}
