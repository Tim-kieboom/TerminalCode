//! The Debug pane: the call stack on the left and the variables of the chosen frame on the right,
//! a view of a bar (the bottom one in the default layout) with its own keys and selection.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use serde::Deserialize;
use unicode_width::UnicodeWidthStr;

use super::model::{RowKind, Status, VarRow};
use crate::app::state::{AppState, Focus};
use crate::components::list_cursor::{ListCursor, Step};
use crate::components::text::{fit, short_path};
use crate::ui::layout::Placement;

/// What the Debug pane can be asked to do with its rows. Going on and stepping are actions of
/// their own (`debug_continue`, `debug_step_over`, ...), which the pane has plain keys for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DebugCommand {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    /// Chooses the frame (and shows where it is), or opens or closes the variable.
    Open,
    /// Opens the variable.
    Expand,
    /// Closes the variable.
    Collapse,
    /// The next of the three lists.
    Switch,
    /// Pins the variable under the selection as a watch.
    Pin,
    /// Takes the watch under the selection away.
    Remove,
    /// Gives the keyboard back to the editor.
    Leave,
}

/// The three lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Section {
    #[default]
    Stack,
    Variables,
    Watches,
}

/// How many rows each list has.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Counts {
    pub(crate) frames: usize,
    pub(crate) variables: usize,
    pub(crate) watches: usize,
}

impl Counts {
    pub(crate) fn of(data: &super::model::DebugData) -> Self {
        Self {
            frames: data.frames.len(),
            variables: data.tree.rows().len(),
            watches: data.watches.rows().len(),
        }
    }
}

/// The move of the selection a command asks for, when it is one.
fn step_of(command: DebugCommand) -> Option<Step> {
    Some(match command {
        DebugCommand::Up => Step::Up,
        DebugCommand::Down => Step::Down,
        DebugCommand::PageUp => Step::PageUp,
        DebugCommand::PageDown => Step::PageDown,
        DebugCommand::First => Step::First,
        DebugCommand::Last => Step::Last,
        _ => return None,
    })
}

/// What the pane remembers of how it is used and drawn.
#[derive(Debug, Default)]
pub(crate) struct DebugView {
    section: Section,
    stack: ListCursor,
    variables: ListCursor,
    watches: ListCursor,
    /// Whether the next draw scrolls to keep the selections in view; the wheel turns it off.
    follow: bool,
    area: Rect,
    stack_body: Rect,
    variables_body: Rect,
    watches_body: Rect,
}

impl DebugView {
    pub(crate) fn section(&self) -> Section {
        self.section
    }

    pub(crate) fn area(&self) -> Rect {
        self.area
    }

    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains(Position::new(column, row))
    }

    /// The frame (from 0) the stack's selection is on.
    pub(crate) fn selected_frame(&self) -> usize {
        self.stack.selected()
    }

    /// The row of the variables the selection is on.
    pub(crate) fn selected_variable(&self) -> usize {
        self.variables.selected()
    }

    /// The row of the watches the selection is on.
    pub(crate) fn selected_watch(&self) -> usize {
        self.watches.selected()
    }

    /// Puts the selection of the stack on the frame the session chose.
    pub(crate) fn select_frame(&mut self, frame: usize) {
        self.stack.select(frame, usize::MAX);
        self.follow = true;
    }

    /// Moves in the list that has the keys, among `counts` rows.
    pub(crate) fn apply(&mut self, command: DebugCommand, counts: Counts) {
        match command {
            DebugCommand::Switch => {
                self.section = match self.section {
                    Section::Stack => Section::Variables,
                    Section::Variables => Section::Watches,
                    Section::Watches => Section::Stack,
                };
            }
            DebugCommand::Pin | DebugCommand::Remove => {}
            _ => {
                let (cursor, count, body) = match self.section {
                    Section::Stack => (&mut self.stack, counts.frames, self.stack_body),
                    Section::Variables => {
                        (&mut self.variables, counts.variables, self.variables_body)
                    }
                    Section::Watches => (&mut self.watches, counts.watches, self.watches_body),
                };
                let page = usize::from(body.height).saturating_sub(1).max(1);
                match step_of(command) {
                    Some(step) => cursor.step(step, count, page),
                    None => cursor.select(cursor.selected(), count),
                }
            }
        }
        self.follow = true;
    }

    /// Scrolls the list under the pointer without moving its selection.
    pub(crate) fn scroll_by(&mut self, column: u16, row: u16, rows: isize, counts: Counts) {
        self.follow = false;
        let position = Position::new(column, row);
        let (cursor, count, body) = if self.watches_body.contains(position) {
            (&mut self.watches, counts.watches, self.watches_body)
        } else if self.variables_body.contains(position) {
            (&mut self.variables, counts.variables, self.variables_body)
        } else {
            (&mut self.stack, counts.frames, self.stack_body)
        };
        cursor.scroll_by(rows, count, usize::from(body.height));
    }

    /// Selects the row under a screen cell, in the list it is in, and says which list and row.
    pub(crate) fn select_at(
        &mut self,
        column: u16,
        row: u16,
        counts: Counts,
    ) -> Option<(Section, usize)> {
        let position = Position::new(column, row);
        let (section, cursor, body, count) = if self.stack_body.contains(position) {
            (
                Section::Stack,
                &mut self.stack,
                self.stack_body,
                counts.frames,
            )
        } else if self.variables_body.contains(position) {
            (
                Section::Variables,
                &mut self.variables,
                self.variables_body,
                counts.variables,
            )
        } else if self.watches_body.contains(position) {
            (
                Section::Watches,
                &mut self.watches,
                self.watches_body,
                counts.watches,
            )
        } else {
            return None;
        };
        if !cursor.select_row(usize::from(row - body.y), count) {
            return None;
        }
        self.section = section;
        self.follow = true;
        Some((section, cursor.selected()))
    }

    /// Takes where the pane is drawn and keeps its selections and scrolls sensible.
    pub(crate) fn prepare(&mut self, area: Rect, body: Rect, counts: Counts) {
        self.area = area;
        let (_, stack, vars, watches) = layout(body);
        self.stack_body = stack;
        self.variables_body = vars;
        self.watches_body = watches;
        let follow = self.follow;
        let lists = [
            (&mut self.stack, counts.frames, stack),
            (&mut self.variables, counts.variables, vars),
            (&mut self.watches, counts.watches, watches),
        ];
        for (cursor, count, rect) in lists {
            cursor.clamp(count, usize::from(rect.height));
            if follow {
                cursor.reveal(usize::from(rect.height));
            }
        }
    }

    /// Draws the frame and the two lists.
    pub(crate) fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let theme = state.theme();
        let data = state.debug();
        let focused = state.focus() == Focus::Debug;
        let title = placement.frame.title_text(placement.kind.title());
        let block = placement.frame.block(theme, title, focused);
        let inner = block.inner(placement.area);
        frame.render_widget(block, placement.area);

        if data.frames.is_empty() && data.watches.rows().is_empty() {
            let hint = hint(&data.status);
            frame.render_widget(
                Paragraph::new(hint).style(theme.style("errors.empty")),
                inner,
            );
            return;
        }
        let live = state.has_keyboard(Focus::Debug);
        let (header, stack_area, vars_area, watch_area) = layout(inner);
        frame.render_widget(
            Paragraph::new(status_line(&data.status)).style(theme.style("tab.inactive")),
            header,
        );
        let selected_style = |section: Section| {
            let slot = match live && self.section == section {
                true => "errors.selected",
                false => "errors.selected.inactive",
            };
            theme.style(slot)
        };

        let project = state.explorer().root_path();
        let width = usize::from(stack_area.width);
        let lines: Vec<Line> = data
            .frames
            .iter()
            .enumerate()
            .skip(self.stack.scroll())
            .take(usize::from(stack_area.height))
            .map(|(index, frame)| {
                let place = match (&frame.path, project) {
                    (Some(path), Some(project)) => {
                        format!("  {}:{}", short_path(path, project), frame.line + 1)
                    }
                    _ => String::new(),
                };
                let mark = if index == data.selected_frame {
                    "▶ "
                } else {
                    "  "
                };
                let text = fit(&format!("{mark}{}{place}", frame.name), width);
                let line = Line::from(Span::raw(text));
                match index == self.stack.selected() {
                    true => line.style(selected_style(Section::Stack)),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), stack_area);

        let width = usize::from(vars_area.width);
        let rows = data.tree.rows();
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(self.variables.scroll())
            .take(usize::from(vars_area.height))
            .map(|(index, row)| {
                let line = variable_line(theme, row, width);
                match index == self.variables.selected() {
                    true => line.style(selected_style(Section::Variables)),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), vars_area);

        let width = usize::from(watch_area.width);
        let rows = data.watches.rows();
        let lines: Vec<Line> = match rows.is_empty() {
            true => vec![Line::from(Span::styled(
                fit("watches: right-click a name in the code", width),
                theme.style("errors.empty"),
            ))],
            false => rows
                .iter()
                .enumerate()
                .skip(self.watches.scroll())
                .take(usize::from(watch_area.height))
                .map(|(index, row)| {
                    let line = variable_line(theme, row, width);
                    match index == self.watches.selected() {
                        true => line.style(selected_style(Section::Watches)),
                        false => line,
                    }
                })
                .collect(),
        };
        frame.render_widget(Paragraph::new(lines), watch_area);
    }
}

/// The areas of the pane: a line at the top that says where the session is, then the stack
/// (left), the variables and the watches (right) with a column between each.
fn layout(inner: Rect) -> (Rect, Rect, Rect, Rect) {
    let header_height = u16::from(inner.height > 2);
    let header = Rect {
        height: header_height,
        ..inner
    };
    let body = Rect {
        y: inner.y + header_height,
        height: inner.height - header_height,
        ..inner
    };
    let (stack, variables, watches) = split(body);
    (header, stack, variables, watches)
}

/// The areas of the stack (left), the variables and the watches (right) in `body`, with a
/// column between each.
fn split(body: Rect) -> (Rect, Rect, Rect) {
    let gap = u16::from(body.width > 30);
    let room = body.width.saturating_sub(2 * gap);
    let stack = room * 3 / 10;
    let watches = room * 3 / 10;
    let variables = room - stack - watches;
    let at = |x: u16, width: u16| Rect {
        x: body.x + x,
        width,
        ..body
    };
    (
        at(0, stack),
        at(stack + gap, variables),
        at(stack + gap + variables + gap, watches),
    )
}

/// What the top line says about the session.
fn status_line(status: &Status) -> String {
    match status {
        Status::Idle => String::new(),
        Status::Starting => "starting".to_owned(),
        Status::Running => "running".to_owned(),
        Status::Stopped {
            reason,
            description: Some(description),
        } => format!("stopped ({reason}): {description}"),
        Status::Stopped { reason, .. } => format!("stopped ({reason})"),
        Status::Ended(why) => format!("ended: {why}"),
    }
}

/// What the pane says when there is nothing to show.
fn hint(status: &Status) -> String {
    match status {
        Status::Starting => "starting the debugger...".to_owned(),
        Status::Running => "the program is running; it can be inspected when it stops".to_owned(),
        Status::Ended(why) => why.clone(),
        _ => "no debug session: ctrl+k g debugs the active run configuration; F9 sets a breakpoint"
            .to_owned(),
    }
}

/// One row of the variables: an arrow for what has children, the name, the value, the type.
pub(super) fn variable_line(
    theme: &crate::ui::theme::Theme,
    row: &VarRow,
    width: usize,
) -> Line<'static> {
    let indent = "  ".repeat(row.depth);
    if let RowKind::More { .. } = row.kind {
        let text = fit(&format!("{indent}  {}", row.value), width);
        return Line::from(Span::styled(text, theme.style("tab.inactive")));
    }
    let arrow = match (row.has_children(), row.expanded) {
        (false, _) => "  ",
        (true, true) => "▾ ",
        (true, false) => "▸ ",
    };
    let loading = if row.loading { " …" } else { "" };
    let head = format!("{indent}{arrow}{}", row.name);
    let value = match row.value.is_empty() {
        true => String::new(),
        false => format!(" = {}", row.value),
    };
    let kind = row
        .type_name
        .as_deref()
        .map(|name| format!("  {name}"))
        .unwrap_or_default();
    let room = width.saturating_sub(head.width());
    let value = fit(&format!("{value}{loading}"), room);
    let used = head.width() + value.width();
    let kind = fit(&kind, width.saturating_sub(used));
    Line::from(vec![
        Span::raw(head),
        Span::raw(value),
        Span::styled(kind, theme.style("tab.inactive")),
    ])
}
