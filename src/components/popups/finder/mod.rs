//! The file finder: a fuzzy search over the project's files.
//!
//! The files come from a walk on another thread ([`scan`]) and arrive in
//! batches while the user is already typing; the best matches so far are
//! shown at once and re-ranked as more files arrive.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::fuzzy::Query;
use crate::ui::theme::Theme;

pub(crate) use scan::{ScanHandle, start as start_scan};

mod scan;
pub(crate) mod target;
#[cfg(test)]
mod tests;

/// Matches kept; the list never shows more than fits on screen anyway.
const MAX_RESULTS: usize = 200;
const WIDTH: u16 = 72;
const MAX_ROWS: u16 = 12;
/// Extra score for a query that also matches the file's own name, so
/// `main` finds `src/main.rs` before `domain/mapping.rs`.
const NAME_BONUS: i32 = 40;
/// Extra score for the file opened last; each one before it gets one point less.
const RECENT_BONUS: i32 = 30;

/// Whether the walk is still going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Walk {
    Running,
    Done { unreadable: usize },
}

/// Which files a changed query has to look at.
#[derive(Debug, Clone, Copy)]
enum Narrowing {
    /// The query only grew: files that did not match before still do not.
    FromCandidates,
    /// Characters were removed: any file may match now.
    FromAllFiles,
}

/// A file found by the walk. What is shown and matched is its path from the
/// project root as text, one name per component with `/` between them; what
/// is opened is the real path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct File {
    shown: String,
    name_start: usize,
    /// The real path from the root, kept only when `shown` cannot rebuild it,
    /// which is when the path is not valid Unicode (`shown` then has U+FFFD in
    /// place of what it could not show). A file name may well contain a `\` on
    /// Unix, and is not changed.
    exact: Option<PathBuf>,
}

impl File {
    fn new(path: &Path) -> Self {
        let names: Vec<_> = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect();
        let shown = names.join("/");
        let name_start = shown.len() - names.last().map_or(0, |name| name.len());
        let exact = path.to_str().is_none().then(|| path.to_path_buf());
        Self {
            shown,
            name_start,
            exact,
        }
    }

    /// The path to open, under `root`.
    fn path_in(&self, root: &Path) -> PathBuf {
        match &self.exact {
            Some(exact) => root.join(exact),
            None => root.join(&self.shown),
        }
    }

    fn name(&self) -> &str {
        &self.shown[self.name_start..]
    }

    fn directory(&self) -> &str {
        &self.shown[..self.name_start]
    }

    /// Which characters of the name and of the directory (counted in characters)
    /// `query` matched, for drawing them differently. A match in the name alone
    /// is shown there; otherwise the match across the whole path is split
    /// between the two.
    fn highlights(&self, query: &Query) -> (Vec<usize>, Vec<usize>) {
        if let Some(in_name) = query.positions(self.name()) {
            return (in_name, Vec::new());
        }
        let Some(in_path) = query.positions(&self.shown) else {
            return (Vec::new(), Vec::new());
        };
        let directory_len = self.directory().chars().count();
        let (directory, name): (Vec<usize>, Vec<usize>) = in_path
            .into_iter()
            .partition(|&index| index < directory_len);
        (
            name.into_iter()
                .map(|index| index - directory_len)
                .collect(),
            directory,
        )
    }

    /// How well `query` matches, or `None`.
    fn rank(&self, query: &Query) -> Option<i32> {
        let by_path = query.score(&self.shown)?;
        let by_name = query
            .score(self.name())
            .map_or(0, |score| score + NAME_BONUS);
        Some(by_path + by_name)
    }
}

#[derive(Debug, Clone, Copy)]
struct Match {
    index: usize,
    score: i32,
}

#[derive(Debug)]
pub(crate) struct Finder {
    root: PathBuf,
    /// Which walk this finder listens to; batches of any other are ignored.
    scan: u64,
    /// Stops the walk when the finder is dropped.
    _handle: Option<ScanHandle>,
    walk: Walk,
    files: Vec<File>,
    query: String,
    prepared: Query,
    /// Every file that matches the query, so a longer query only has to look
    /// at these.
    candidates: Vec<usize>,
    /// The best of the candidates, best first.
    matches: Vec<Match>,
    selected: usize,
    /// The files opened lately by their shown path, with how long ago: 0 is the
    /// latest.
    recent: HashMap<String, usize>,
    /// The line and column the query asks for, if it names one.
    place: Option<(usize, Option<usize>)>,
    /// The part of the query the files are matched against.
    matched_text: String,
}

impl Finder {
    pub(crate) fn new(root: PathBuf, scan: u64, handle: Option<ScanHandle>) -> Self {
        Self {
            root,
            scan,
            _handle: handle,
            walk: Walk::Running,
            files: Vec::new(),
            query: String::new(),
            prepared: Query::new(""),
            candidates: Vec::new(),
            matches: Vec::new(),
            selected: 0,
            recent: HashMap::new(),
            place: None,
            matched_text: String::new(),
        }
    }

    /// Lists the files in `recent` (paths under the project root, latest first)
    /// ahead of equal matches, and first while nothing is typed.
    pub(crate) fn with_recent(mut self, recent: &[PathBuf]) -> Self {
        self.recent = recent
            .iter()
            .filter_map(|path| path.strip_prefix(&self.root).ok())
            .enumerate()
            .map(|(rank, relative)| (File::new(relative).shown, rank))
            .collect();
        self
    }

    /// Files found so far.
    #[cfg(test)]
    pub(crate) fn file_count(&self) -> usize {
        self.files.len()
    }

    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub(crate) fn walk(&self) -> Walk {
        self.walk
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The matching paths, best first.
    #[cfg(test)]
    pub(crate) fn results(&self) -> Vec<&str> {
        self.matches
            .iter()
            .map(|found| self.files[found.index].shown.as_str())
            .collect()
    }

    /// The file the selection is on, as a path that can be opened.
    pub(crate) fn selected_path(&self) -> Option<PathBuf> {
        let found = self.matches.get(self.selected)?;
        Some(self.files[found.index].path_in(&self.root))
    }

    /// How well file `index` matches the query, with the bonus for having been
    /// opened lately.
    fn rank(&self, index: usize) -> Option<i32> {
        let file = &self.files[index];
        let score = file.rank(&self.prepared)?;
        let bonus = self.recent.get(&file.shown).map_or(0, |&ago| {
            (RECENT_BONUS - i32::try_from(ago).unwrap_or(RECENT_BONUS)).max(1)
        });
        Some(score + bonus)
    }

    /// What the selection asks for: the file, and the place in it the query names.
    pub(crate) fn selected_target(&self) -> Option<Target> {
        let path = self.selected_path()?;
        let (line, column) = match self.place {
            Some((line, column)) => (Some(line), column),
            None => (None, None),
        };
        Some(Target { path, line, column })
    }

    /// Takes a batch of files from the walk. A batch from another walk is
    /// ignored. The selection stays on the same file if it is still among the
    /// best matches.
    pub(crate) fn add_batch(&mut self, scan: u64, paths: Vec<PathBuf>) {
        if scan != self.scan {
            return;
        }
        let first_new = self.files.len();
        self.files.extend(paths.iter().map(|path| File::new(path)));

        let kept = self.matches.get(self.selected).map(|found| found.index);
        for index in first_new..self.files.len() {
            if let Some(score) = self.rank(index) {
                self.candidates.push(index);
                self.matches.push(Match { index, score });
            }
        }
        self.sort_matches();
        self.selected = kept
            .and_then(|index| self.matches.iter().position(|found| found.index == index))
            .unwrap_or(0);
    }

    /// The walk is over.
    pub(crate) fn finish(&mut self, scan: u64, unreadable: usize) {
        if scan == self.scan {
            self.walk = Walk::Done { unreadable };
        }
    }

    pub(crate) fn insert(&mut self, c: char) {
        self.query.push(c);
        self.refilter_after_typing();
    }

    pub(crate) fn backspace(&mut self) {
        self.query.pop();
        self.refilter(Narrowing::FromAllFiles);
    }

    pub(crate) fn clear_query(&mut self) {
        self.query.clear();
        self.refilter(Narrowing::FromAllFiles);
    }

    /// Whatever matches a longer text matches the shorter one too, so only the
    /// files that matched before are looked at again, unless the text matched
    /// against did not just grow (the typed `:` and digits after a name are a
    /// place, not part of it).
    fn refilter_after_typing(&mut self) {
        let grew = target::parse(&self.query)
            .text
            .starts_with(self.matched_text.as_str());
        let from = match grew {
            true => Narrowing::FromCandidates,
            false => Narrowing::FromAllFiles,
        };
        self.refilter(from);
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.matches.len().saturating_sub(1));
    }

    /// Ranks the files again for the new query and goes back to the top.
    fn refilter(&mut self, from: Narrowing) {
        let parsed = target::parse(&self.query);
        self.place = parsed.line.map(|line| (line, parsed.column));
        self.matched_text = parsed.text.to_owned();
        self.prepared = Query::new(parsed.text);
        let looked_at: Vec<usize> = match from {
            Narrowing::FromCandidates => std::mem::take(&mut self.candidates),
            Narrowing::FromAllFiles => (0..self.files.len()).collect(),
        };
        self.candidates.clear();
        self.matches.clear();
        for index in looked_at {
            if let Some(score) = self.rank(index) {
                self.candidates.push(index);
                self.matches.push(Match { index, score });
            }
        }
        self.sort_matches();
        self.selected = 0;
    }

    /// Best first (shorter paths win ties); keeps only the best
    /// [`MAX_RESULTS`].
    fn sort_matches(&mut self) {
        let files = &self.files;
        let key = |found: &Match| {
            (
                Reverse(found.score),
                files[found.index].shown.len(),
                found.index,
            )
        };
        if self.matches.len() > MAX_RESULTS {
            self.matches.select_nth_unstable_by_key(MAX_RESULTS, key);
            self.matches.truncate(MAX_RESULTS);
        }
        self.matches.sort_by_key(key);
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area(frame.area());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Go to File ", theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);
        if inner.height < 2 {
            return;
        }

        let input_area = Rect { height: 1, ..inner };
        let prompt = format!("> {}", self.query);
        frame.render_widget(Paragraph::new(prompt.as_str()), input_area);
        let cursor_x = input_area.x + prompt.width().min(usize::from(input_area.width)) as u16;
        frame.set_cursor_position(Position::new(
            cursor_x.min(input_area.right()),
            input_area.y,
        ));

        let footer_area = Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        };
        let dim = theme.style("pane.border");
        frame.render_widget(Paragraph::new(self.footer()).style(dim), footer_area);

        let list_area = Rect {
            y: inner.y + 1,
            height: inner.height.saturating_sub(2),
            ..inner
        };
        if self.matches.is_empty() {
            let message = match self.walk {
                Walk::Running => " looking for files...",
                Walk::Done { .. } => " no matching file",
            };
            frame.render_widget(Paragraph::new(message).style(dim), list_area);
            return;
        }
        let hit = theme.style("finder.match");
        let rows = self.matches.iter().map(|found| {
            let file = &self.files[found.index];
            row(file, file.highlights(&self.prepared), dim, hit)
        });
        let mut state = ListState::default().with_selected(Some(self.selected));
        let list = List::new(rows).highlight_style(theme.style("list.selected"));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    fn footer(&self) -> String {
        let counts = match self.walk {
            Walk::Running => format!(" indexing... {} files", self.files.len()),
            Walk::Done { unreadable: 0 } => format!(" {} files", self.files.len()),
            Walk::Done { unreadable } => {
                format!(" {} files, {unreadable} unreadable", self.files.len())
            }
        };
        match self.place {
            Some((line, Some(column))) => {
                format!("{counts}  \u{2192} line {line}, column {column}")
            }
            Some((line, None)) => format!("{counts}  \u{2192} line {line}"),
            None => counts,
        }
    }

    /// The box: horizontally centered, near the top.
    fn area(&self, screen: Rect) -> Rect {
        let width = WIDTH.min(screen.width);
        let rows = (self.matches.len().max(1) as u16).min(MAX_ROWS);
        let height = (rows + 4).min(screen.height);
        Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 8).min(screen.height - height),
            width,
            height,
        }
    }
}

/// A row: the file's name, then its directory dimmed, with the characters the
/// query matched drawn in the `hit` style.
fn row<'a>(
    file: &File,
    (in_name, in_directory): (Vec<usize>, Vec<usize>),
    dim: Style,
    hit: Style,
) -> ListItem<'a> {
    let mut spans = vec![Span::raw(" ")];
    spans.extend(marked(file.name(), &in_name, Style::default(), hit));
    if !file.directory().is_empty() {
        spans.push(Span::raw("  "));
        spans.extend(marked(file.directory(), &in_directory, dim, dim.patch(hit)));
    }
    ListItem::new(Line::from(spans))
}

/// `text` in pieces: the characters at `marked` (by position) in the `hit`
/// style, the rest in `base`.
fn marked<'a>(text: &str, marked: &[usize], base: Style, hit: Style) -> Vec<Span<'a>> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut run = String::new();
    let mut run_marked = false;
    for (index, c) in text.chars().enumerate() {
        let is_marked = marked.contains(&index);
        if is_marked != run_marked && !run.is_empty() {
            let style = if run_marked { hit } else { base };
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        run_marked = is_marked;
        run.push(c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, if run_marked { hit } else { base }));
    }
    spans
}

/// What the finder was asked to open: a file, and perhaps where in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) path: PathBuf,
    /// The line, counted from 1.
    pub(crate) line: Option<usize>,
    /// The column, counted from 1.
    pub(crate) column: Option<usize>,
}
