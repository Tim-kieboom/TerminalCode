# TerminalCode — Design

A keyboard-driven TUI code editor in Rust (Ratatui), aiming for a Zed-like IDE feel in the terminal:
explorer, integrated terminal, tree-sitter highlighting, and an optional vim layer on top of a
normal-IDE editing core.

This is a restart (branch `redo`). Reasons: vim was not in the original core design, and error handling
used `anyhow` everywhere instead of typed errors.

## Goals / Non-goals

**0.1.0 goals**
- Normal IDE-style editing as the primary mode; vim motions as an optional layer.
- File explorer, tabs/splits, integrated terminal, tree-sitter syntax highlighting.
- Fully rebindable keymap (data, not code).
- Linux and Windows tier 1; macOS best-effort.

**Explicitly after 0.1.0**
- Debugger (DAP), LSP, Lua plugin system (LSP and debugger may be delivered as Lua plugins).
- Collaboration, remote dev, AI features.

**Constraint for 0.1.0 that keeps the above possible:** every user-visible operation goes through the
`Action` registry with serializable arguments. Lua will call the same registry later.

## Principles

1. **Single owner of state.** One task owns `AppState` and processes a single `Event` channel. All other
   tasks (input, PTY, file watcher, parse workers, timers) only send events.
2. **Edits are data.** All text changes are an `Edit` applied via `Buffer::apply`.
3. **Keymap is data.** Keys resolve to `Action`s through layered tables; no key matching in logic code.
4. **Core is UI-agnostic.** Display width, tabs, and rendering live only in the view layer.
5. **Typed errors.** Each module defines its own error enum with `thiserror`; errors wrap their
   parents/sources. `anyhow` is allowed only in `main.rs`.

## Components, layout, theme, plugins

- `ComponentKind` is a closed enum plus one open variant, `Plugin(PluginViewId)`. Component state lives in
  `AppState`; views are pure functions of state, theme and area.
- Layout is a tree of data in RON (`defaults/default_layout.ron`, embedded at compile time). `LayoutTree::from_ron`
  parses and validates, so a user settings file can reuse it after 0.1.0.
- Theme is named style slots in TOML (`defaults/default_theme.toml`, embedded the same way). Components ask for a slot,
  never a color; unknown slots fall back to the default style.
- Plugins push declarative `ViewNode` trees into their pane and never touch `Frame` or `Rect`.
- **Plugin state access: snapshots (decided).** The loop publishes immutable snapshots (ropey clones are O(1));
  plugins read them without round trips. Writes carry the buffer `version` they were based on so stale writes
  can be detected. What the host does on a stale write (reject or rebase) is decided when the plugin API is
  designed, post-0.1.0.

## Architecture

```
 input task ─┐
 pty tasks  ─┤
 fs watcher ─┼──► mpsc<Event> ──► App loop (owns AppState) ──► render (ratatui, dirty-flag, ≤60fps)
 parse jobs ─┤                         │
 timers     ─┘                         └─► spawns workers (parse, fs, pty) that reply with Events
```

### Event loop
- Tokio runtime. One `mpsc` channel for bulk events; input has its own high-priority channel, polled
  first with a biased `select!`, so Ctrl+C is never queued behind terminal output.
- Render only when state is dirty, capped near 60 fps. Never one render per event.
- PTY readers batch reads into one event and use a bounded channel for backpressure.
- Timers: vim `timeoutlen` for pending key sequences, notification expiry, cursor blink.

### Text core
- **Buffer:** `ropey::Rope` plus language, path, line-ending style, dirty flag, and a `version: u64`
  that increments on every edit.
- **Edit:** `{ start_byte, old_end_byte, new_end_byte, start_point, old_end_point, new_end_point, text }`.
  Single entry point `Buffer::apply(Edit)`. Tree-sitter incremental parsing and (later) LSP sync consume
  the same struct.
- **Undo:** history stores inverse edits grouped into **transactions**. A transaction is one logical
  change (typing burst, a vim operator, a paste). Cursor/selection state is saved with each
  transaction. Vim `u`/`.` and normal-editor Ctrl+Z share it. Undo tree/branching is out of scope for
  0.1.0 (linear history).
- **Position:** `(line, grapheme_index)` using `unicode-segmentation`. The buffer layer converts to and
  from byte/char offsets. Only the view layer knows display width (`unicode-width`) and tab expansion.
- **Cursor/Selection:** selections are the primary model (a cursor is an empty selection). Core cursor
  stores `desired_column` for Up/Down. Multi-cursor is not in 0.1.0 but the type is a list of
  selections from day one to avoid a later rewrite.
- **Known limit:** grapheme-index to byte conversion is O(line length); extremely long single-line files
  will be slow.

### Highlighting
- `tree-sitter` + `tree-sitter-highlight`, with grammars bundled for a starter set (Rust, TOML, JSON,
  Markdown, Python, JS/TS, C/C++, Bash).
- Parse jobs run off the loop and carry the buffer `version`. Stale results (version mismatch) are
  discarded and a new parse is queued. Old highlights remain until fresh ones arrive.

### Input and keymap
- Terminal key events are normalized into a `KeyChord` (`terminal::init` probes the kitty keyboard protocol
  at startup and enables `DISAMBIGUATE_ESCAPE_CODES` when available; `terminal::restore`, also called from the
  panic hook, pops it). Without kitty support (`KeyboardSupport::Legacy`) `defaults/default_keymap_legacy.toml`
  is layered on the defaults; it holds chords that legacy terminals send as other keys (ctrl+backspace -> ctrl+h).
  The same action must stay reachable on both: alt-based alternatives live in the main default file.
- **Keymap = tries of chord sequences per `Context`** (`global`, `editor`; `terminal` and vim modes join later).
  The active contexts are listed most specific first and the first context that knows a sequence answers.
  Bindings are TOML `[[binding]]` tables: `keys` ("ctrl+k ctrl+s" is a sequence), optional `context`, and an
  `action`; a binding with no `action` removes an earlier one. Layers apply in order: built-in defaults, then
  legacy fallbacks, then the user file (`<config dir>/terminalcode/keymap.toml`, via `dirs`). An invalid user
  file leaves the defaults in place and is reported in the status bar.
- **Sequences:** `keymap::Resolver` feeds chords through the trie. Result: run an action, wait (pending), or
  unbound. A broken sequence discards its old prefix and retries the new chord on its own, and printable
  discarded chords are typed, so a stray prefix never swallows text or a command. A pending sequence expires
  after 1 s (`App::expire_pending`, driven by a timer in the app loop): if the pending chords are themselves a
  binding it fires, otherwise it is dropped.
- **Keymap layers** (highest priority first): terminal-focus prefix layer, vim mode layer (when
  enabled), pane/context layer, global layer.
- **Vim layer:** a modal parser on top of the normal editor. It turns key sequences (`d2w`, `ci"`, `.`,
  registers, macros) into the same `Action`s the normal editor uses. Vim's "cursor sits on a character"
  semantics live in this layer, not the core. Scope for 0.1.0: normal/insert/visual (char and line),
  operators + motions + text objects, counts, `.` repeat, registers, `/` search. Visual-block, macros
  and ex commands beyond `:w :q :wq :e` are post-0.1.0 unless time allows.
- **Terminal focus:** when the integrated terminal is focused, all keys go to the shell except the IDE
  prefix chord, which makes the next key an IDE shortcut. Default prefix: `ctrl+g` (readline barely uses it;
  zellij uses it too). It is just a binding in a `terminal` context that starts a sequence, so it is
  rebindable; it is added with the terminal pane in M6.
- All keybinds are redesigned from scratch (the old `keybind_defaults.json` is discarded). User keymap
  file overrides defaults.

### UI layout
- Ratatui. Regions: sidebar (explorer), editor area (tabs, splits), bottom panel (terminal), status
  bar, notifications overlay, command palette / fuzzy finder overlay.
- Modal prompts (the unsaved-changes prompt on quit) live in `AppState` as `Option<...>` and are drawn last,
  over every component. While one is open it takes all keys; mouse and paste are ignored. Quitting with
  modified documents opens it; it lists them, lets each be saved or discarded alone or all at once, and
  quits when none are left. A save that fails keeps the file listed with the error.
- Notifications live in `AppState` (`Notifications`) and are drawn over the components and under the modals,
  stacked bottom right above the status bar. `Level::Info` expires (the app loop has a timer branch
  for the next expiry), `Level::Error` stays until a key press or mouse click, which is handled as usual; the
  dismissal runs before the key so an error that key causes is kept. Every user-visible message goes through
  `notify` / `notify_error`, never straight to the status bar.
- The command palette is a modal of the same kind. Its entries come from `Action::palette_actions()`, each with
  a hand-written title from `Action::title()` (an exhaustive match, so a new action must decide whether it is
  listed); actions with an argument are listed once per useful value. App builds the entries when it opens
  the palette, including the keys from `Keymap::keys_for`, so rendering needs nothing but `AppState`. Matching
  is `ui::fuzzy::score`, shared with the file finder later.
- Components are views over `AppState`; they emit `Action`s, they do not mutate state directly.

### Integrated terminal
- PTY via `portable-pty` (Unix PTY and Windows ConPTY). Emulation via `alacritty_terminal`.
- Multiple terminal tabs, scrollback, resize, alternate screen, bracketed paste.
- Default shell: `$SHELL` on Unix, PowerShell on Windows; configurable.

### Explorer
- Tree view over the project root, lazy-loaded, file watcher driven refresh, create/rename/delete/move,
  gitignore-aware.
- Implemented so far: the tree (`components/explorer`), directories read on expand on the UI thread (a huge
  directory can stall a frame; move to a worker if it shows up), listing filtered by the `ignore` crate
  (`.gitignore` honored even outside a repository, `.git` never listed, other dotfiles shown), directories first
  then case-insensitive names. The project root is the first directory on the command line, else the working
  directory.
- Keyboard focus is `AppState::focus` (`Editor` or `Explorer`) and picks the keymap contexts: `[Editor, Global]` or
  `[Explorer, Global]`. Printable keys are only typed into the editor when it has focus. `ctrl+b` toggles focus,
  `esc` returns to the editor, clicking either component focuses it. Opening a file from the explorer keeps focus
  in the tree; actions that open or switch tabs and panes (`Action::focuses_editor`) move it to the editor.
- Outside changes: `watcher::FsWatcher` (notify + debouncer) watches the explorer's open directories and the directories
  of open files, one directory at a time, and sends `Event::FilesChanged`. A `Buffer` remembers a hash of what the
  file held when it last read or wrote it (`Disk`), so it can tell its own saves from someone else's changes and a
  rewrite with identical contents from a real change. `Workspace::check_disk` reloads unmodified documents and flags
  modified ones (once per distinct change, `Document::warned`); `save_document` refuses to overwrite a changed file
  until it is asked a second time with nothing else done in between. The same check runs at save time, so a
  missed event cannot lose someone's change.
- `Workspace::open_path` never reads an open file twice: it switches to a tab that shows it (focused pane first,
  then others) or adds a view to the existing document.

### Search
- Fuzzy file finder (project files) and project-wide text search (ripgrep-style via `ignore` +
  `grep-searcher` or similar). Both are in 0.1.0.

### Config
- Settings go in a Zed-style `settings.json` (JSON with comments) layered defaults → user → project (project layer
  optional in 0.1.0); the keymap is its own TOML file (`keymap.toml`, already implemented). Indentation: the editor
  detects tabs vs 2-4 spaces from the file and falls back to 4 spaces; the fallback and the detection switch become
  settings later.
- Auto-indent: Enter copies the leading whitespace left of the cursor verbatim. Adding a level after an opening
  token waits for tree-sitter (M5). Paste, from either path, is always inserted verbatim.

### Errors
- Per-module `thiserror` enums wrapping sources. User-facing errors become notifications. No panics for
  recoverable failures. `anyhow` only at `main`.

## Platform support
| Platform | Status | Notes |
|---|---|---|
| Linux | Tier 1 | tested in CI and daily |
| Windows | Tier 1 | ConPTY, CRLF handling, Windows Terminal kitty support varies |
| macOS | Best-effort | CI builds, no promises on behavior |

CI (GitHub Actions) builds and runs `cargo test` and `clippy` on Linux and Windows; release binaries
per OS.

## Testing strategy
- Core (buffer, edits, undo, positions, vim parser, keymap resolution) is pure and unit-tested in
  sibling `tests.rs` files.
- The App loop takes `Event`s and exposes state, so it can be tested headless by feeding events.
- Render tests via Ratatui `TestBackend` for key components.
- Property tests for `Buffer::apply` + undo (apply then invert returns the original text).

## Module layout (proposed)
```
src/
  main.rs
  app/        # AppState, Event, loop, dispatch
  buffer/     # rope, Edit, transactions, positions, selections
  action/     # Action enum + registry
  keymap/     # KeyChord, layers, kitty detection, fallbacks, vim parser
  syntax/     # tree-sitter, parse jobs, highlight spans
  ui/         # layout, editor view, explorer, tabs, status, overlays
  terminal/   # pty + alacritty_terminal integration
  fs/         # watcher, project tree, search
  config/
```

## Open questions
- Default IDE prefix chord and the default keymap (decided in M1).
- Whether a project-wide search crate or shelling out to `rg` is used.
- Clipboard integration: system clipboard (`arboard`) vs OSC 52 (needed over SSH).
- Mouse support scope (click to place cursor, scroll, pane resize) for 0.1.0.
- Who tests Windows regularly beyond CI.

## Editor view notes
- `Editor` (buffer, selections, scroll) lives in `AppState`. Scroll offset is view state. Components implement
  `ui::Render`: `prepare(&mut self, placement)` runs first for every placement and adjusts size-dependent view
  state (the editor scrolls to keep the cursor visible); `render(&self, frame, &AppState, placement)` then
  draws read-only. `components::render` runs both passes.
- All unbound unmodified printable keys type text. Keymap actions are the only other way to change the buffer.
- Undo grouping: runs of typing, backspace or delete merge into one step. Movement, newline, replacing a selection and undo/redo end a run. No time-based or word-boundary break yet.
- Known gap: AltGr keys report Ctrl+Alt on some terminals (notably Windows) and are not typed yet.

## Theming later
`ratatui-css` on crates.io is an empty reserved name (0.0.0). `ratatui-style` (CSS cascade: selectors,
specificity, inheritance, pseudo-states, optional SCSS) is the real candidate. The `Theme::style(slot)` API is
the seam: slots can become selector paths (`editor .selection`) without touching components. Evaluate maturity
(single maintainer, 0.2.0) before adopting; keep TOML slots as the fallback.

## Clipboard
- `clipboard::Clipboard` owns an internal `Register { text, kind: Charwise | Linewise }` and a `System`
  (`arboard` native clipboard, else OSC 52 write-only through the terminal, else nothing).
- Copy/cut always fill the register and try to write the system clipboard; `last_written` remembers what we
  wrote (even if the write failed). Paste reads the system clipboard: text different from `last_written` came
  from another program and wins (charwise); same text, empty, or unreadable -> the register wins and keeps its kind.
  The first time the system clipboard is unreadable while a register exists, the status bar says so.
- Copy/cut with no selection take the whole line (including its line break). Linewise paste inserts above the
  current line and keeps the cursor on the same character. Pasted line breaks are converted to the buffer's style.
- Bracketed paste (`ctrl+shift+v`, middle click) arrives as `Event::Paste` and is inserted charwise; it never reads
  or changes the register or system clipboard. Each paste is one undo step and never merges with typing.
- Tests never touch the real clipboard: `App` defaults to `Clipboard::internal_only()`; `lib::run` installs
  `System::detect()`.

## Mouse
- `terminal::init` turns mouse capture on; `terminal::set_mouse_capture` switches it. With capture on the terminal's
  own selection needs a modifier (Shift in most terminals), which the status bar mentions when you toggle with
  `alt+m`. A `mouse` setting joins `settings.json` later (default on).
- The editor records where its text was drawn (`set_viewport`, called from `prepare`), so a screen cell maps back
  to a buffer position: undo the horizontal scroll, expand tabs and wide characters, and pick the nearest cell
  boundary (left half of a character -> before it). Below the last line is the end of the document.
- `mouse::ClickTracker` counts quick clicks on the same cell (400 ms) as single, double or triple; a fourth starts over.
  Double click selects the run of same-kind characters (word, punctuation or spaces); triple click the whole line
  including its break. A drag extends from the start of whatever the press selected, clamped to the text area.
- Scrolling (wheel) never moves the cursor. To keep the next frame from snapping the view back, the editor only
  scrolls to the cursor when the cursor, the text or the viewport changed (`take_view_change`).
- Deferred: auto-scroll while dragging beyond the edge (mouse events stop while the pointer is still, so it needs a
  repeating timer in the app loop).

## Workspace: documents, panes, tabs
- `components::workspace::Workspace` stores each file once as a `Document` (buffer + undo history + indent style) in a
  map by id. A pane is a strip of tabs; a tab is a `ViewState` (selections, scroll, mouse state) of one document.
  Panes sit in a binary split tree (`Axis::Horizontal` = side by side, `Vertical` = stacked); splitting halves the
  focused pane and the new pane starts with a copy of the active view.
- **Shared buffer, shared undo.** `Editor` (buffer + view) is unchanged for all editing logic. To run an action,
  `Workspace::with_editor` lends the document's buffer to an `Editor` together with the tab's `ViewState`, runs the
  closure, and puts both back. Reading goes through `EditorRef` (`&Buffer` + `&ViewState`), and rendering works from
  the same two references, so two panes can draw one document at once.
- **Cursors follow edits.** `Buffer` logs an `EditInfo` (start, old end, new end byte) for every applied edit,
  including undo/redo. Before running an action the workspace converts the other views' selections of the same
  document to byte offsets; afterwards it replays the log over them (`EditInfo::remap_byte`) and converts back.
  Rule: offsets at or before the edit start stay put (a cursor at an insertion point stays before the new text),
  offsets after the replaced range shift by the size change, offsets inside it collapse to the start. A view that
  did not make the edit does not scroll to its cursor because of it.
- **Typing bursts** never merge across views: the document remembers the last editing view and the next view to
  edit starts a new undo step.
- **Closing:** closing a tab whose document has no other view and unsaved changes needs a second close to discard;
  anything else in between cancels that. The last tab of a pane closes the pane; the last tab of the last pane
  leaves the editor area empty: a hint is shown, the app keeps running, editing actions answer "no open file
  (ctrl+n opens a new one)", and `ctrl+n` opens a tab again. Documents with no views are dropped.
- **Layout and mouse:** `Workspace::prepare` lays the pane tree out in the placement (each pane: one tab-bar row, the
  rest is the bordered editor), records pane, tab and text areas for hit-testing and directional focus, and lets each
  pane's active view scroll to its cursor. A press focuses the pane under it (and switches tab or places the cursor);
  drags and releases go to the focused pane; the wheel scrolls the pane under the pointer without moving focus;
  a middle press on a tab closes that tab (`close_tab_at`), whether or not it is active or its pane is focused.
- Command line: every path argument opens a tab (first one active).

## Component frames
- Every component is drawn inside a `PaneFrame` (`ui::pane_frame`): a border (`Off`, `Plain`, `Rounded`, `Double`,
  `Thick`), a title (`Hidden`, `Name`, `Text("...")`), a title alignment, and the theme slots that style the title
  and the border. The layout file sets them per component with `Framed(component: X, frame: (...))`; `Leaf(X)`
  keeps the component's default. Anything left out of a frame takes the default, and unknown fields or variants are
  errors (so a typo does not silently do nothing).
- Defaults: panes `Plain` + `Name`; the editor `Plain` + `Hidden` (its tab bar shows file names; `title: Name` adds the
  active file's name to the border); the status bar `Off` + `Hidden`.
- The resolved frame travels with the `Placement`, so each component reads `placement.frame`. `PaneFrame::inner`
  gives the content area exactly as `PaneFrame::block` draws it: a title costs a row even without a border.
- A focused editor pane uses `<border_slot>.focused` when the theme defines it, otherwise the plain slot. Style
  slots are plain theme names, so the same layout file can point two components at differently styled slots.
- `Off` is not spelled `None`: with implicit `Some` in the RON reader, a bare `None` means "field not set".

## Background and transparency
- A text-mode program only picks a color per cell; opacity and blur are the terminal's (kitty: `background_opacity`,
  `background_blur`; WezTerm, Ghostty and others have equivalents) and apply to the terminal's default background.
  With no `[background]` in the theme the editor paints no background at all, so those effects show through.
- `[background]` in `theme.toml` (a reserved table, not a style slot): `color` (a color, or `"transparent"` / `"reset"`)
  and `opacity` (0 to 1, integer or float). Opacity 0 or a transparent color paints nothing; opacity 1 paints the color;
  in between the color is blended per channel with the terminal's own background color, which the editor asks for once
  at startup (OSC 11, via `terminal-colorsaurus`, 500 ms timeout) and only when the theme needs it. If the terminal does
  not answer, the tint color is used as is. A blend needs a hex color, since a named ANSI color has no known RGB.
- Painting is one `Block` over the whole frame at the start of every draw (`components::paint_background`); slots with
  their own `background` (selection, tabs, status bar) draw over it.
- `blend = "terminal"` (the default) paints a solid blended color, so the terminal's translucency and blur do not show
  in those cells: terminals only make their *default* background translucent. `blend = "none"` paints the exact
  color instead, for terminals that can make one exact color translucent (kitty 0.39+:
  `transparent_background_colors #rrggbb@alpha` in kitty.conf); the startup status line shows that exact setting.
  To see the terminal's blur through the whole editor, paint nothing (no `[background]`, or `color = "transparent"`).
- A blend is not a blur and not true see-through: it gives a tinted look in terminals that cannot be made
  translucent. For real translucency leave the background transparent and configure the terminal.
