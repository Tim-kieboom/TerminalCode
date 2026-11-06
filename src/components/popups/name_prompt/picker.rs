//! The list under the Open File and Open Folder prompts: the entries of the folder typed so
//! far, matched against the name being typed. What reads the folder, and when, is the app's
//! (`app/path_prompt.rs`); this is what the prompt keeps of it and does with keys.

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{NamePrompt, Purpose};
use crate::components::popups::path_list::{self, Entry, Listing, Match};
use crate::ui::theme::Theme;

/// How many rows the list shows at most.
const MOST_ROWS: usize = 10;

/// What the prompt knows of the folder it is in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct Picker {
    /// The folder the list is of, or is being read for.
    wanted: Option<PathBuf>,
    /// The number of the read that is waited for; an answer to another is dropped.
    request: u64,
    /// What the folder held when it was read; kept while the next folder is being read.
    listing: Option<Listing>,
    /// Why the folder could not be read.
    error: Option<String>,
    /// The entries that match the name being typed, best first.
    shown: Vec<Match>,
    /// How many entries matched, of which `shown` has the best.
    total: usize,
    /// The highlighted row of `shown`: nothing until a name is typed or a key moves it.
    selected: Option<usize>,
    /// Files are left out (Open Folder).
    dirs_only: bool,
}

impl NamePrompt {
    /// Whether this prompt shows a list of the folder.
    pub(crate) fn has_list(&self) -> bool {
        self.picker.is_some()
    }

    pub(super) fn picker_for(purpose: &Purpose) -> Option<Picker> {
        match purpose {
            Purpose::OpenFile => Some(Picker::default()),
            Purpose::OpenFolder => Some(Picker {
                dirs_only: true,
                ..Picker::default()
            }),
            _ => None,
        }
    }

    /// The part of the name before its last separator, as typed (with the separator).
    pub(crate) fn typed_folder(&self) -> &str {
        path_list::split_typed(&self.name).0
    }

    /// The part of the name after its last separator: what the list is matched against.
    pub(crate) fn partial(&self) -> &str {
        path_list::split_typed(&self.name).1
    }

    /// Whether the folder `dir` is not the one the list is of (or being read for), so it is to
    /// be read.
    pub(crate) fn needs_listing_of(&self, dir: &Path) -> bool {
        self.picker
            .as_ref()
            .is_some_and(|picker| picker.wanted.as_deref() != Some(dir))
    }

    /// A read of `dir` starts: the number to answer with. The list shows what it had until
    /// the answer comes.
    pub(crate) fn begin_listing(&mut self, dir: PathBuf) -> u64 {
        let Some(picker) = self.picker.as_mut() else {
            return 0;
        };
        picker.request += 1;
        picker.wanted = Some(dir);
        picker.request
    }

    /// The answer to read number `request`. Dropped when another read began since; otherwise
    /// the list is of the folder, or the error says why not. Whether it was taken.
    pub(crate) fn finish_listing(
        &mut self,
        request: u64,
        dir: PathBuf,
        result: Result<Vec<Entry>, String>,
    ) -> bool {
        let Some(picker) = self.picker.as_mut().filter(|p| p.request == request) else {
            return false;
        };
        match result {
            Ok(entries) => {
                picker.listing = Some(Listing { dir, entries });
                picker.error = None;
            }
            Err(message) => {
                picker.listing = None;
                picker.error = Some(message);
            }
        }
        self.refresh_matches();
        true
    }

    /// Matches the entries against the name being typed again. The highlight starts on the
    /// first match once a name is typed, and nowhere before.
    pub(crate) fn refresh_matches(&mut self) {
        let partial = self.partial().to_owned();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let (shown, total) = match (&picker.error, &picker.listing) {
            (None, Some(listing)) => {
                path_list::matching(&listing.entries, &partial, picker.dirs_only)
            }
            _ => (Vec::new(), 0),
        };
        picker.selected = (!partial.is_empty() && !shown.is_empty()).then_some(0);
        picker.shown = shown;
        picker.total = total;
    }

    fn entry_at(&self, row: usize) -> Option<&Entry> {
        let picker = self.picker.as_ref()?;
        let found = picker.shown.get(row)?;
        picker.listing.as_ref()?.entries.get(found.index)
    }

    /// The highlighted entry.
    pub(crate) fn highlighted(&self) -> Option<&Entry> {
        self.entry_at(self.picker.as_ref()?.selected?)
    }

    /// The highlight one row down; from nothing, onto the first.
    pub(crate) fn select_next(&mut self) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let last = picker.shown.len().saturating_sub(1);
        picker.selected = match picker.selected {
            _ if picker.shown.is_empty() => None,
            None => Some(0),
            Some(row) => Some((row + 1).min(last)),
        };
    }

    /// The highlight one row up; off the first it goes to nothing when no name is typed, else
    /// it stays.
    pub(crate) fn select_previous(&mut self) {
        let typed = !self.partial().is_empty();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        picker.selected = match picker.selected {
            Some(0) if !typed => None,
            Some(0) => Some(0),
            Some(row) => Some(row - 1),
            None => None,
        };
    }

    /// Replaces the name being typed (after the last separator) with `text`.
    fn set_partial(&mut self, text: &str) {
        let keep = self.typed_folder().len();
        self.name.truncate(keep);
        self.name.push_str(text);
        self.cursor = self.name.len();
    }

    /// Goes into the folder called `name`: it is written with a separator after it, and the
    /// list is of what is in it.
    fn go_into(&mut self, name: &str) {
        self.set_partial(&format!("{name}/"));
    }

    /// `tab`: writes as much of the name as the entries that start with it share; when that is
    /// no more than is typed, the highlighted entry (a folder with a separator after it); and
    /// with none highlighted, it highlights the first.
    pub(crate) fn complete(&mut self) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let partial = self.partial().to_owned();
        let names: Vec<&str> = (0..picker.shown.len())
            .filter_map(|row| self.entry_at(row))
            .map(|entry| entry.name.as_str())
            .collect();
        let start = path_list::common_start(&names, &partial);
        if start.len() > partial.len() {
            self.set_partial(&start);
            return;
        }
        if let Some(entry) = self.highlighted().cloned() {
            match entry.is_dir {
                true => self.go_into(&entry.name),
                false => self.set_partial(&entry.name),
            }
        } else if let Some(picker) = self.picker.as_mut()
            && !picker.shown.is_empty()
        {
            picker.selected = Some(0);
        }
    }

    /// What `enter` does with the highlight: into a folder (and the answer is `false`: nothing
    /// is opened yet; but for Open Folder a folder whose whole name is typed is the one meant),
    /// or onto the file highlighted (it is written as the name, and the answer
    /// is `true`); with nothing highlighted what was typed is what is opened.
    pub(crate) fn enter(&mut self) -> bool {
        match self.highlighted().cloned() {
            // Open Folder with the whole name of the folder typed: that is the one meant.
            Some(entry)
                if entry.is_dir
                    && self.purpose == Purpose::OpenFolder
                    && entry.name == self.partial() =>
            {
                true
            }
            Some(entry) if entry.is_dir => {
                self.go_into(&entry.name);
                false
            }
            Some(entry) => {
                self.set_partial(&entry.name);
                true
            }
            None => true,
        }
    }

    /// Takes the name back one folder when the character before the cursor is a separator:
    /// the separator and the name before it go. Whether it did.
    pub(super) fn step_up(&mut self) -> bool {
        if self.picker.is_none() {
            return false;
        }
        let before = &self.name[..self.cursor];
        let Some(rest) = before.strip_suffix(std::path::is_separator) else {
            return false;
        };
        let start = rest.rfind(std::path::is_separator).map_or(0, |at| at + 1);
        self.name.replace_range(start..self.cursor, "");
        self.cursor = start;
        true
    }

    /// How many rows the list takes under the text.
    pub(super) fn list_height(&self) -> u16 {
        let Some(picker) = self.picker.as_ref() else {
            return 0;
        };
        let rows = picker.shown.len().min(MOST_ROWS);
        let note = usize::from(picker.error.is_some() || picker.total > rows);
        u16::try_from(rows + note).unwrap_or(u16::MAX)
    }

    /// The row of the list at a screen cell, when it is on one.
    pub(crate) fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        let list = self.layout.list;
        let picker = self.picker.as_ref()?;
        let inside = column >= list.x && column < list.right() && row >= list.y;
        let index = usize::from(row.checked_sub(list.y)?);
        (inside && index < picker.shown.len().min(MOST_ROWS)).then_some(index)
    }

    /// Highlights row `row`.
    pub(crate) fn select_row(&mut self, row: usize) {
        if let Some(picker) = self.picker.as_mut() {
            picker.selected = Some(row);
        }
    }

    /// Draws the rows under the text, in `area`.
    pub(super) fn render_list(&self, frame: &mut Frame, theme: &Theme, area: Rect) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let dim = theme.style("pane.border");
        let hit = theme.style("finder.match");
        let chosen = theme.style("list.selected");
        let rows = picker.shown.len().min(MOST_ROWS);
        for (index, found) in picker.shown.iter().take(rows).enumerate() {
            let Some(entry) = self.entry_at(index) else {
                continue;
            };
            let Some(row_area) = row_in(area, index) else {
                break;
            };
            let base = match picker.selected == Some(index) {
                true => chosen,
                false => Style::default(),
            };
            let line = Line::from(name_spans(entry, found, base, hit));
            frame.render_widget(Paragraph::new(line).style(base), row_area);
        }
        let note = match (&picker.error, picker.total > rows) {
            (Some(message), _) => Some((message.clone(), theme.style("notification.error"))),
            (None, true) => Some((format!(" +{} more", picker.total - rows), dim)),
            _ => None,
        };
        if let (Some((text, style)), Some(row_area)) = (note, row_in(area, rows)) {
            frame.render_widget(Paragraph::new(text).style(style), row_area);
        }
    }
}

fn row_in(area: Rect, index: usize) -> Option<Rect> {
    let offset = u16::try_from(index).ok()?;
    (offset < area.height).then_some(Rect {
        y: area.y + offset,
        height: 1,
        ..area
    })
}

/// The name of an entry (a folder with a `/` after it), with the letters the match used in the
/// style `hit`.
fn name_spans(entry: &Entry, found: &Match, base: Style, hit: Style) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(" ", base)];
    for (at, c) in entry.name.chars().enumerate() {
        let style = match found.positions.contains(&at) {
            true => base.patch(hit),
            false => base,
        };
        spans.push(Span::styled(c.to_string(), style));
    }
    if entry.is_dir {
        spans.push(Span::styled("/", base));
    }
    spans
}
