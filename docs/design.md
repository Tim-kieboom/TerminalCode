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
- `ui::Hideable<T>` wraps a component that can be hidden: it keeps the value while hidden (reachable through
  `node()` / `node_mut()`, never a public field), has no `Default` (starting shown is spelled out), needs no
  bound on `T`, and implements `Render` for `T: Render` by drawing nothing while hidden. `AppState` holds the
  explorer, the status bar and every plugin view as `Hideable`s (`AppComponents` has a hand-written `Default`), and the popups are one `Popup` enum (`None`, or the one popup that is open: opening one closes the other, so two
  can never be open; key routing and drawing are a `match` on it). `LayoutTree::resolve_visible`
  asks `AppState::is_visible` per component: a hidden one is left out and the space goes to its siblings, a split
  with nothing visible disappears. A hidden explorer is skipped by mouse hit-testing, and hiding it moves the
  keyboard to the editor.
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
- Deleting (`app/popups/delete.rs`, `removal.rs`): the selection goes to the OS trash. The project root cannot be
  deleted. Open tabs of the deleted path close; if any has unsaved changes a `Confirm` popup asks first. If the
  trash move fails (e.g. on a mount without a trash), a second `Confirm` offers permanent delete. In a `Confirm`
  only `y` says yes; Enter, Esc and everything else cancel. Tests swap the trash for a stand-in
  (`App::with_trash`) so they never touch the real one.
- Creating (`app/popups/new_entry.rs`, `entries.rs`, `components/name_prompt.rs`): `a` / `shift+a` open a one-line
  name prompt for the selected folder (the parent of a selected file). The name may have several parts; existing
  entries are never touched (`create_new`, `create_dir`), and names that leave the folder (`..`, absolute) are
  refused. A made file opens in the editor; either kind is revealed in the tree (`Explorer::reveal`).
- Renaming (same prompt, `Purpose::Rename`): `entries::rename` uses `renamore::rename_exclusive_fallback`, so an
  existing entry is never replaced. Before the move the app notes which open documents are inside the entry and
  where; right after it, in the same handler, it points them at the new paths (`Workspace::set_document_paths`,
  which also bumps the documents version so the watcher re-syncs). A late event for the old path therefore names
  no open file and does nothing.
- Moving (`m`, `Purpose::Move`): the same path as rename with the prompt holding the path from the project root
  (`entries::move_to`); an existing folder as destination means "into it". A folder into itself is refused before
  any folder is made.
- Context menu (`shift+f10`, `Popup::Menu`, `components/menu.rs`): a generic list of (label, action) entries with
  separators, opened just below the selected row (above it near the bottom of the screen). It keeps the path it was
  opened on; picking an item checks that the path still exists, reveals it so it is the explorer's selection, checks
  the selection really is that path, and only then runs the ordinary action. A vanished path gives an error and
  nothing runs, so a watcher reload while the menu is open cannot retarget it. The project folder only gets New File
  and New Folder. A right click in the explorer selects the row under the pointer (the project folder on empty
  space) and opens the menu there; the menu places itself before each draw (`Menu::place`) and keeps its area, so
  clicks are tested against what is on screen. A left click on an item runs it; on the frame or a separator it does
  nothing. A press outside closes the menu and is then handled as if the menu were not there (so a right click
  reopens it elsewhere); the wheel only closes it. Each item shows the keys bound to its action in the explorer (the shortest, as the palette does), right-aligned.
- Implemented so far: the tree (`components/explorer`), directories read on expand on the UI thread (a huge
  directory can stall a frame; move to a worker if it shows up), listing filtered by the `ignore` crate
  (`.gitignore` honored even outside a repository, `.git` never listed, other dotfiles shown), directories first
  then case-insensitive names. The project root is the first directory on the command line, else the working
  directory.
- Keyboard focus is `AppState::focus` (`Editor` or `Explorer`) and picks the keymap contexts: `[Editor, Global]` or
  `[Explorer, Global]`. Printable keys are only typed into the editor when it has focus. `ctrl+b` toggles focus,
  `esc` returns to the editor, clicking either component focuses it. Opening a file from the explorer keeps focus
  in the tree; actions that open or switch tabs and panes (`Action::focuses_editor`) move it to the editor.
- The file finder is the first background worker: `components/finder/scan.rs` walks the project on a std thread and
  sends `Event::FinderBatch` / `Event::FinderDone` tagged with a scan number, so batches of a finder that was closed
  or reopened are ignored; dropping the finder's `ScanHandle` stops the walk, and the bounded event channel slows
  the walk down if the UI falls behind. `ui::fuzzy::Query` is the shared matcher (lowercased once per query).
  Measured at 200,000 files: about 10 ms per keystroke in release builds.
- Project search (`components/search`) is the second background worker. `worker::start` compiles the pattern up front
  (so an invalid regex is an immediate error, not a dead search), then runs on a std thread: it sleeps 150 ms and
  quits if its `SearchHandle` was dropped meanwhile, which debounces typing without a timer in the app loop; it
  walks with the same rules as the explorer, searches each file with `grep-searcher` (binary files skipped), and
  sends `SearchBatch` / `SearchDone` tagged with a search number. A `Hit` is shaped for the list (indent dropped,
  long lines cut around the match, column in graphemes). It reads files from disk, so unsaved edits are not
  searched. 17,740 files in the cargo registry: a miss takes about 0.2 s after the debounce, 2,860 hits about 1 s,
  streaming.
- Find in the open file (`components/find`) is a modal like the others, but it reserves a row: while it is open
  `Workspace::set_find_bar(true)` makes the focused pane's editor area one row shorter and the bar draws in the
  row left, so the scrolling logic keeps the current match above it without knowing about the bar. Matches are
  found by the `regex` crate line by line (the query is escaped unless regex is on), stored as grapheme columns,
  and drawn by the editor from `Find::matches_on_line`; the current match is the selection. Matches are found
  again when the buffer version changed (an outside reload) at the next key.
- Outside changes: `watcher::FsWatcher` (notify + debouncer) watches the explorer's open directories and the directories
  of open files, one directory at a time, and sends `Event::FilesChanged`. A `Buffer` remembers a hash of what the
  file held when it last read or wrote it (`Disk`), so it can tell its own saves from someone else's changes and a
  rewrite with identical contents from a real change. `Workspace::check_disk` reloads unmodified documents and flags
  modified ones (once per distinct change, `Document::warned`); `save_document` refuses to overwrite a changed file
  until it is asked a second time with nothing else done in between. The same check runs at save time, so a
  missed event cannot lose someone's change.
- A report is cheap to apply however many paths it names: the explorer reloads only the open directories the paths
  are in (`Explorer::refresh_paths`; the merge is a hash lookup, collapsed directories are skipped and re-read when
  opened), and `check_disk` resolves only the paths whose file name is an open document's. `FsWatcher::forget`
  drops the watch of a directory the report names itself (inotify loses a watch with its directory), and the next
  `watch_only` starts a fresh one; a directory that does not exist yet is not an error and is retried.
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
  app/        # the single owner of the state: the event loop and the plumbing (mod.rs)
                # App holds the state and a few small structs, each owning one concern and its invariants:
                # Keyboard (keymap, half-typed sequence, deadline), MouseInput, Watching, Background,
                # Remembered (last queries); clipboard is its own type
    keys.rs     # which bindings apply, typing, running an Action
    mouse.rs    # clicks, drags and the wheel in the editor panes and tabs
    panels.rs   # the explorer and plugin views: show, hide, keyboard focus, explorer mouse
    editing.rs  # clipboard, save, close tab
    watching.rs # watched directories and what to do with a report of changed files
    popups/     # one file per popup: open it, handle its keys, act on what it returns
    background.rs # where background work sends its results, and the numbers that tell its runs apart
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
- Every component is drawn inside a `PaneFrame` (`ui::pane_frame`): border `sides` (a list of `Top`, `Right`, `Bottom`,
  `Left` or `All`; empty for no border), a border `border` style (any ratatui `BorderType`: `Plain`, `Rounded`,
  `Double`, `Thick`, the dashed forms, `QuadrantInside`, `QuadrantOutside`), a title (`Hidden`, `Name`,
  `Text("...")`), a title alignment, and the theme slots that style the title and the border. The layout file sets
  them in `Pane(view: X, sides: .., border: .., ..)`; every field is optional and takes the component's default
  when left out, and unknown fields or variants are errors (so a typo does not silently do nothing).
- Defaults: panes all sides, `Plain` and `Name`; the editor all sides, `Plain` and `Hidden` (its tab bar shows file
  names; `title: Name` adds the active file's name to the border); the status bar no sides and `Hidden`. A style
  alone adds no edge: `Pane(view: StatusBar, border: Double)` is still unboxed, it needs `sides: [All]` too.
- The resolved frame travels with the `Placement`, so each component reads `placement.frame`. `PaneFrame::inner`
  gives the content area exactly as `PaneFrame::block` draws it: a title costs a row even without a border.
- A focused editor pane uses `<border_slot>.focused` when the theme defines it, otherwise the plain slot. Style
  slots are plain theme names, so the same layout file can point two components at differently styled slots.

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

## Syntax highlighting (M5 plan, decided before building)
- Grammars are compiled in: Rust, TOML, JSON, Markdown, Nix (RON was dropped again, see step E; the drop order if size hurts is in `docs/todo.md`). Loading
  them at runtime from shared libraries is post-0.1.0.
- Each document owns a highlighter (panes sharing a document share it). `Buffer::apply` already returns the edit;
  the document forwards it to its highlighter, which keeps the edits made since the tree it holds was parsed.
  A reload, or a path change that switches the language, drops the tree and parses in full.
- A worker thread parses. At most one parse is pending and it always starts from the newest text (a rope clone is
  cheap), with the stored edits applied to the old tree first, so reparsing is incremental. Any result is
  accepted, whatever version it parsed, and mapped forward through the edits made since; `version` only guards
  against results arriving out of order. A steady typist on a big file therefore never starves the colors.
- Highlights are flat spans of (byte range, style), not tree nodes, because the renderer wants them per
  line; the theme turns capture names into styles when the highlighter is made (a changed theme needs a new
  one), and captures it has no style for are left out so the capture around them shows through. They cover only the visible range plus a margin of a few screens (the query runs with
  `QueryCursor::set_byte_range`), so mapping and rebuilding scale with the screen, not the file. The worker is
  told which range(s) to query; a scroll outside the cached range asks for a new query on the existing tree.
  One document can be open in several panes at different scroll positions, so the ranges are the union of every
  pane's visible range plus margin: overlapping ones are merged, and at most 8 are kept.
- Until a result arrives, the old spans are mapped through each edit: an edit before a span shifts it, inside it
  grows or shrinks it, across it cuts it; an insert exactly at a boundary stays outside the span.
- A capture name from a grammar's `highlights.scm` (`function.builtin`) resolves to a theme slot by longest prefix:
  `syntax.function.builtin`, then `syntax.function`, then `syntax`. A capture the theme has no slot for gets no
  span at all. The default dark and light themes define a short fixed list (keyword, function, type, string,
  number, comment, constant, operator, punctuation, property). Grammars without a `HIGHLIGHTS_QUERY` constant
  would get a `highlights.scm` vendored under `defaults/` (none of the final languages needed one).
- Build order, one commit each: (A) a synchronous `Highlighter` for Rust that turns text plus a byte range into
  spans, with the capture→slot mapping and default `syntax.*` theme; (B) spans drawn in the editor, the document
  owning the highlighter and reparsing synchronously on change; (C) mapping spans through edits and `Tree::edit`
  for incremental reparse; (D) the worker (coalescing, range list, late results mapped forward); (E) the other
  grammars, Markdown injection and the vendored queries. No cross-check test between the worker and the
  synchronous path; the worker is tested on its own.
- Step A is done: `src/syntax` has `Language` (Rust), `Highlighter` (`parse` from scratch, `spans(text, range)`)
  and `flatten`, which turns the nesting captures of a query into non-overlapping spans (innermost wins, first
  pattern wins a tie, cut to the range, same-style neighbours joined). `Theme::syntax_style` is the longest-prefix
  lookup, and `defaults/default_theme.toml` has the `syntax.*` slots. It builds with just a C compiler (`cc`);
  nothing Windows-specific has been tried yet.
- Step B is done: each `Document` owns a `DocumentSyntax` (`src/syntax/document.rs`) that picks the language from the
  buffer's path, parses in full whenever the buffer version changed and recomputes spans for the ranges asked
  for. `Workspace::refresh_highlights` runs after layout every frame: per pane, the visible lines plus one screen
  above and below, merged across panes (`syntax::merge_ranges`, capped at 8). The editor draws a span's style under
  the selection and find-match styles (`Style::patch`, so a selection keeps the syntax color of its text). A grammar
  that cannot be set up is reported once as a notification. This is still synchronous: a 7 MB Rust file takes
  about 0.9 s per reparse in a release build (3 s in debug), on every keystroke. That is what steps C and D remove.
- Step C is done. `Workspace::with_editor` hands the edit log it already drains for the other views to the
  document's `DocumentSyntax::record_edits`, which keeps the edits since the tree was parsed and moves the spans
  through each one at once (`syntax::mapping::map_spans`: before an edit stays, after it shifts, an insert at a
  span's edge stays outside, an edit inside grows or shrinks the span, new text over a whole span takes its color,
  a span an edit cuts into keeps what is left, one it covers is dropped). The next update calls `Tree::edit` for
  each and reparses from the old tree, but only when every change since the last parse is accounted for: each edit
  adds one to the buffer version, so `parsed + pending == version` says so, and a reload or any unrecorded change
  breaks it and gets a full parse. More than 10,000 pending edits are dropped the same way. Measured on 7 MB of
  Rust in a release build: the first parse 0.9 s, a keystroke afterwards ≈ 0.12 s (down from 0.9 s). What is left
  is copying the text into a `String` and the UI thread doing it; step D moves the work to a worker and reads the
  rope in chunks.
- Step D is done. One worker thread (`syntax/worker.rs`, started by `App::with_events`, which is why highlighting
  needs the event channel) keeps a `Highlighter` and its tree per document, keyed by the document id. A `Job`
  carries a rope clone (`Buffer::snapshot`, cheap), the version, the version it continues from (`base`) with the
  edits between, and the byte ranges. Jobs for one document are coalesced while the worker is busy (`coalesce`: a
  job that continues the one it replaces takes over its edits and its base, otherwise it stands alone), so there is
  never more than one parse queued per document. The worker reparses from its tree when its parsed version is the
  job's base and the edit count matches the version gap, parses from scratch otherwise, and only re-runs the query
  when the version did not change (a scroll). Text goes to tree-sitter and the query in rope chunks, never as one
  `String`. The answer travels as `Event::Highlighted(Output)`. The document side (`DocumentSyntax`) keeps a log of
  edits since the version its spans describe: `record_edits` moves the spans through each at once, `accept` moves
  an answer forward through the edits made since the text it describes, ignores one older than the spans on
  screen or from a version it has no record of, and trims the log. A change nobody reported (a reload) breaks the
  `base + log.len() == version` check and clears the spans and starts over; a pile of more than 10,000 edits does
  the same. Closing a document sends the worker a `forget`. A grammar that cannot be set up comes back as a failed
  `Output`, reported once as a notification. Measured on 7 MB of Rust in release: asking and recording an edit cost
  the UI a few microseconds; the worker takes ≈ 0.9 s for the first parse and ≈ 0.12 s for each keystroke after.
- Step E is done: TOML, JSON, Markdown and Nix join Rust (`Language::from_path` picks by extension: `rs`, `toml`,
  `json`, `md`/`markdown`, `nix`). All of them ship their own `HIGHLIGHTS_QUERY`, so nothing had to be written by
  hand. RON was tried and dropped: `tree-sitter-ron` 0.2 on crates.io depends on `tree-sitter` 0.20, whose `Language`
  is a different type, and two tree-sitter C runtimes in one binary are not safe; making it work meant vendoring its
  parser. If it comes back, vendor it rebuilt against `tree-sitter-language` (its `LANGUAGE` as a `LanguageFn`). Markdown has two
  grammars: the block grammar is parsed and kept like any other tree; for the `inline` nodes that touch the range
  being highlighted, a fresh parser with the inline grammar is run over just those nodes (`set_included_ranges`)
  and its captures are added to the outer ones before flattening, so the cost scales with what is on screen and
  nothing inline is kept between queries. Fenced code blocks are not highlighted in their own language yet. The
  default theme gained slots for the new captures (`number`, `boolean`, `string.escape`, `string.special.key`,
  `text.title`/`literal`/`uri`/`reference`/`strong`/`emphasis`). The release binary grew from 8.3 MB to 10.7 MB with
  all grammars (Linux); the Windows build has not been tried.

## Terminal focus (M6 plan, decided before building)
- While the terminal pane has the keyboard, every key goes to the shell except one reserved chord, `ctrl+b`.
  Pressing it twice (`ctrl+b ctrl+b`) sends a literal `ctrl+b` to the shell. The global bindings (`ctrl+s`,
  `ctrl+q`, `ctrl+p`, `ctrl+w`, `ctrl+n`, ...) do not apply there: they go to the shell, so shell habits (readline,
  history, flow control) keep working. The `ctrl+k` prefix that shipped, and the `ctrl+g` the docs mention, are not
  used for the terminal.
- After the prefix: a short fixed table moves focus (editor, explorer, terminal toggle, cancel), and any other key
  is looked up in the normal keymap, so a global binding works behind the prefix (`ctrl+b ctrl+p` opens the finder).
  What an unknown key or a timeout does is still to be decided.
- An unknown key after the prefix cancels it: nothing goes to the shell and a notification says the chord is not
  bound. A timeout cancels it silently. While the prefix is pending the status bar shows `ctrl+b …`.
- The emulator is `vt100` (scrollback, alternate screen, bracketed paste, mouse modes are enough for now), not
  `alacritty_terminal`; the PTY is `portable-pty`.
- PTY output is parsed on the reader thread, which owns the `vt100` parser behind a mutex and feeds it in slices of
  a few KB, releasing the lock between slices. The UI takes the lock only to draw. The reader sends one dirty
  wake-up and stays quiet until the UI has drawn, so a flood becomes one redraw per frame, and backpressure is the
  kernel's PTY buffer. Test: a headless program prints 100 MB while the test feeds keystrokes and asserts each key
  is handled within 50 ms.
- Keystrokes and pastes reach the child through a writer thread fed by a bounded channel, so a child that is not
  reading stdin cannot block the UI thread on a PTY write; resizing the PTY goes through the same thread. When the
  channel is full the sender waits at most 20 ms; if the channel is still full the rest of the paste is dropped
  and a notification says how much went through. The flood test also pastes several MB into a child that is not
  reading and asserts the UI stays responsive.
- The terminal pane is toggled like the explorer (hidden by default). The shell starts the first time it is shown,
  in the project root, and keeps its last PTY size while the pane is hidden, so toggling does not resize it.
- Build order, one commit each: (A) a `Terminal` session with no UI: spawn through `portable-pty` in a directory,
  reader thread feeding `vt100`, writer thread behind the bounded channel with the 20 ms rule, tested with `sh -c`
  and scripted output; (A2) the flood tests (100 MB of output with keys within 50 ms, a several-MB paste into a
  child that is not reading), written before any UI depends on the design; (B) the pane draws the screen, the
  toggle, spawn on first show, PTY size following the pane, one wake-up per drawn frame; (C) key encoding, the
  `ctrl+b` prefix with its fixed table and keymap fallthrough, the pending indicator in the status bar, bracketed
  paste; (D) scrollback and the alternate screen; (E) mouse passthrough and terminal tabs. Windows (ConPTY) cannot
  be tested here.
- Step A is done: `src/pty` has `Session` (spawn a `Shell` in a directory through `portable-pty`, `TERM` set to
  `xterm-256color`), with no UI. The reader thread reads 8 KB at a time and feeds `vt100` (10,000 lines of
  scrollback) under a mutex it holds only per slice, then calls the wake-up once until `take_dirty` is called
  (the draw's side of the one-wake-per-frame rule); the end of the shell also wakes. `write` cuts input into 4 KB
  chunks for a 64-chunk bounded queue (`pty::queue`, own `Mutex`+`Condvar`, since `std::mpsc` has no send timeout)
  that a writer thread drains, waits at most 20 ms for room per chunk and reports `Written { sent, complete }`.
  `resize` changes the screen at once and hands the size to the writer thread, which applies it in order with the
  input. Dropping the session kills and reaps the shell. A terminal in line mode throws away input that does not
  fit instead of blocking the writer, so a child that "is not reading" only makes writes block once it has set raw
  mode (what a shell's line editor does); the not-reading tests use `stty raw -echo`. Needs the `cc`-free
  `portable-pty` and `vt100` crates only; Windows (ConPTY) is untested.
- Step A2 is done (`src/pty/tests/flood_tests.rs`): a `probe` stands in for the app between two events: it hands a
  byte to the shell and, when the screen changed, looks at every cell, timing both against the 50 ms limit. While a
  shell prints scrolling lines (`yes | head -c N`) the slowest key took about 9 µs and the slowest frame about
  2.7 ms in a debug build (0.3 ms in release); 100 MB parses in about 1 s in release and about 12 s in debug, so
  the 100 MB test is `#[ignore]`d (`cargo test --release -- --ignored`) and the ordinary suite runs the same test
  with 10 MB. Also tested: one wake-up per drawn frame during the flood, keys reaching a shell that is busy printing,
  and an 8 MB paste into a raw-mode shell that is not reading (the write returns inside the limit with the rest
  dropped; drawing, further keys and the child's output stay live). The probe was checked against a deliberately
  bad reader that holds the screen lock for 80 ms now and then: the frame limit fails at 83 ms.
- Step B is done. The terminal pane (`components/terminal`, `TerminalPane`) is hidden by default (`Hideable::new_hidden`)
  and toggled with `ctrl+k t` (`toggle_terminal`, "View: Toggle Terminal" in the palette). Showing it starts the
  shell if there is none or it ended: in the project root, with the size the pane has when shown, worked out from
  the layout and the screen as of the last draw (`AppState::terminal_size_now`), so a shell that asks for its size
  straight away already gets the right one. Each frame `prepare` takes the dirty flag (the next change wakes the
  app again) and resizes the shell if the pane's text area changed; while hidden the pane keeps its last size.
  `Event::TerminalChanged` (sent with `blocking_send`, because a lost wake-up leaves the screen stale) redraws and
  reports the end of a shell once, with its exit code. `components/terminal/render.rs` copies the `vt100` cells into
  the frame buffer: colors (default/indexed/RGB), bold/dim/italic/underline/inverse, wide characters take two
  cells, combining marks stay with their letter. The pane only draws so far: no focus, no keys (step C), no
  cursor, no scrollback (step D).
- Step C is done. `Focus::Terminal`: `ctrl+k t` shows the pane and gives it the keyboard, gives the keyboard to a pane
  that is shown but not focused, and hides a focused pane (the keyboard goes back to the editor); a click in the
  pane focuses it, a click elsewhere takes the keyboard away; the focused pane has the focused border and the shell's
  cursor. While it has the keyboard `handle_key` sends everything to `pty::encode_key` (xterm bytes: control
  letters and punctuation, alt as an ESC prefix, arrows/Home/End as `ESC [ A` or `ESC O A` when the program is in
  application-cursor mode, `ESC [ 1 ; m A` with modifiers, Insert/Delete/Page keys and F1-F12 in xterm's forms)
  except `ctrl+b`. Behind it (`terminal_keys.rs`): `ctrl+b` sends a literal 0x02, `esc` cancels, `e` / `x` / `t` move
  focus to the editor / explorer or hide the pane, and any other chord goes through the keymap in the editor and
  global contexts (so `ctrl+b ctrl+p` opens the finder and a half-typed `ctrl+b ctrl+k` goes on to `b`). An unbound
  key cancels with a "ctrl+b X is not bound" notification, and one second of silence cancels quietly; while it waits
  the status bar shows the choices. Pastes go to the shell: bracketed (`ESC [ 200 ~ ... ESC [ 201 ~`, with any end
  marker inside the text removed) when the program asked for it, otherwise with line breaks as carriage returns;
  a write that does not fit says how many bytes went through. A shell that has ended says so when typed at.
  The 16 key tests, 19 app tests: keys reach the shell, `ctrl+q` / `ctrl+p` / `ctrl+s` go to the shell instead of the

## Layout syntax v2 (plan, decided before building)
Goal: less nesting to read. The tree shape is the thing to see, so the file says only what differs from the default.
- **Containers:** `Row([..])` (side by side) and `Col([..])` (stacked) replace `Split(direction:, children:)`; no
  `(size:, node:)` tuples and no single-child root split.
- **Sizes wrap a node:** `Fixed(30, node)`, `Percent(50, node)`. A bare node is `Fill`. `Fill(..)` is not written.
- **One leaf:** `Pane(view: Explorer, sides: [Right], border: QuadrantInside, title: Hidden)` replaces `Framed` and
  `Leaf`. RON cannot mix positional and named fields, so every field is named; `component` is renamed `view`
  (it matches `Plugin("view.id")`). The `frame: (...)` block is flattened into the pane. Every field is optional.
- **Borders split in two:** `sides` is a list of `Top`, `Right`, `Bottom`, `Left` (or `All`; empty = no border) and
  `border` is any ratatui `BorderType` (`Plain`, `Rounded`, `Double`, `Thick`, the dashed forms,
  `QuadrantInside`, `QuadrantOutside`). `Off`, `TopOnly` and `RightOnly` are removed (they hard-coded a style).
- **Defaults stay in the component**, for `sides` and `border` alike: the status bar has no sides, the others
  `All`; panes `Plain`, the editor `Plain` with a hidden title.
- Example:

```ron
Col([
    Row([
        Fixed(30, Pane(view: Explorer, sides: [Right], border: QuadrantInside, title: Hidden)),
        Col([
            Pane(view: Editor, sides: [], title: Hidden),
            Fixed(12, Pane(view: Terminal, sides: [Top], border: LightDoubleDashed)),
            Fixed(1, Pane(view: StatusBar)),
        ]),
    ]),
])
```
- **Format stays RON** (enums map directly; TOML nests badly, KDL would add a dependency and a third syntax).
- **Hard break:** the old syntax fails to load; there are no saved layouts outside this repo.
- **Validation at load time**, as data, not types: the enum allows nonsense (`Fixed(Fixed(..))`, a size on the root,
  two bare siblings, which would share space silently), so the loader rejects it. Errors carry a tree path, e.g.
  `root > Col[0] > Row[1]: two children have no size`; no line numbers (RON reports them only for syntax errors).
  Two bare siblings is an error, not an equal split, so a forgotten `Fixed(1, ..)` cannot hand half the screen
  to the status bar. Hiding a component is checked at resolve time and is unaffected.
- **Failure behaviour:** the embedded file failing is a startup panic (the `default_layout_is_valid` test catches
  it); a user layout file (`<config dir>/terminalcode/layout.ron`, next to `keymap.toml`, loaded by
  `config::load_layout`) that fails falls back to the embedded one and the error, with the file name and the tree
  path, goes through `notify_error`, like every user-visible message. A missing file is normal and silent.

## Theme palette v2 (plan, decided before building)
Goal: do not write the same color in every slot, and let a user restyle without a rebuild. Today the theme is only
the embedded `defaults/default_theme.toml`; there is no user theme file.
- **`[palette]`** is a table of named colors (hex or a color name). Entries are literals only: a palette entry cannot
  refer to another, so there are no cycles and no resolution order.
- **References:** a slot's `text` and `background`, and `[background].color`, may be `"@name"`. The `@` keeps it apart
  from color names such as `"red"`. A name the palette lacks is a load error naming the slot and the entry, not a
  silent default (a typo would otherwise show up as an unstyled slot nobody can trace). A literal color in a slot
  still works and is how one slot differs from its role.
- **Semantic roles, one flat namespace.** UI: `text`, `muted`, `faint`, `surface`, `accent`, `accent_bg`,
  `accent_text`, `selection`, `info`, `error`, `warning`. Syntax hues: `red`, `orange`, `yellow`, `green`, `teal`,
  `cyan`, `blue`, `purple`, `pink`, `gold`, plus `comment` and `punctuation`. Flat, so `@green` works in
  any slot and changing `green` retints strings and everything else that uses it. `error` and `red` are two entries
  that may hold the same value.
- **User file:** `<config dir>/terminalcode/theme.toml`, next to `keymap.toml` and `layout.ron`, layered over the
  built-in theme. The palette is merged first and `@name` is resolved after, so redefining `accent` retints every
  slot that uses it. A user slot merges key by key with the default slot: `text` and `background` replace;
  `modifiers` replaces the whole list (a union could never remove `bold`). To remove something the default sets,
  `text` or `background` = `"reset"` (the terminal's own color) and `modifiers = []`. Slots the file does not
  mention keep the default. `[background]` merges key by key as well.
- **Failure:** the embedded theme failing is a startup panic (a test catches it). A user file that is unreadable,
  has a syntax error, a bad color or an unknown `@name` falls back to the whole built-in theme, with one
  `notify_error` naming the file; a missing file is normal and silent. Plugins that ask for a slot the theme lacks
  still get the plain style.
- **Same loader shape as the layout:** `config::load_theme` / `config::user_theme_path`, built on the file-reading
  and fallback code of `load_layout` and `load_keymap`.

## Opening the user's config files (decided and built)
- `Action::OpenConfig(Theme | Keymap | Layout)`, in the command palette (F1) as `Config: Open Theme`, `Config: Open
  Keymap` and `Config: Open Layout`, and bindable in `keymap.toml` as `{ open_config = "theme" }`. It opens the
  file in a tab. If the file does not exist it is first made (with the directory) from the built-in default, written
  with `create_new` so an existing file is never overwritten, and a notification says so. A copy of the whole
  default is the start on purpose: it shows every slot, binding or pane there is to change.
- The files are read at startup and by `Config: Reload` (below); the notification for a new file says
  that an edit applies on the next start or reload. (Reloading when a file is saved is not built.)
- The directory is `App::config_dir`, set at startup from `config::user_config_dir()` and `None` otherwise, so a
  test app never touches the real config directory; without one the action says so.
- **Reload** (`Action::Reload`, palette `Config: Reload`): reads `theme.toml`, `layout.ron` and `keymap.toml` again
  and applies them without restarting; later it will restart the plugins too. A missing file means the built-in one,
  an invalid file is reported and replaced by the built-in one, the same as at startup (so removing a user file and
  reloading goes back to the default). Open files, tabs and the terminal are untouched. What it does: the theme is
  replaced (the terminal background learned at startup is kept, asking again would fight the input loop for stdin);
  the layout is replaced and the keyboard goes back to the editor if the focused component is not in it; the keymap
  is replaced and a half-typed sequence dropped; the syntax worker is started again with the new theme and every
  document asks it for its colors afresh. Nothing is bound to it by default.
