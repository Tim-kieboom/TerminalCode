# TerminalCode — Design

A keyboard-driven TUI code editor in Rust (Ratatui), aiming for a Zed-like IDE feel in the terminal:
explorer, integrated terminal, tree-sitter highlighting, and an optional vim layer on top of a
normal-IDE editing core.

This is a restart (branch `redo`). Reasons: vim was not in the original core design, and error handling
used `anyhow` everywhere instead of typed errors.

## Goals / Non-goals

**0.1.0 goals**
- Normal IDE-style editing as the primary mode; vim motions as an optional layer, shipped after 0.1.0.
- File explorer, tabs/splits, integrated terminal, tree-sitter syntax highlighting.
- Fully rebindable keymap (data, not code).
- Linux and Windows tier 1; macOS best-effort.

**Explicitly after 0.1.0**
- Debugger (DAP), LSP (a generic client in the core, Rust first; see "LSP client"), WebAssembly plugin system (the debugger may be delivered as a plugin; see "Plugin runtime").
- Collaboration, remote dev, AI features.

**Constraint for 0.1.0 that keeps the above possible:** every user-visible operation goes through the
`Action` registry with serializable arguments. Plugins will call the same registry later.

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
- Layout is a tree of data in RON (`crates/ui/defaults/default_layout.ron`, embedded at compile time). `LayoutTree::from_ron`
  parses and validates, so a user settings file can reuse it after 0.1.0.
- **Bars.** A `SideBar(views: [...])` or `BottomBar(views: [...])` layout node holds a list of views (Explorer,
  Terminal, plugin views) and shows one at a time; `Pane(view: Explorer)` and `Pane(view: Terminal)` still work and
  are a bar of one view. A bar is not a `ComponentKind`: resolving the layout turns it into the placement of the view
  it shows (`Placement::kind`), so every component keeps drawing itself, and a bar with several views also gives the
  placement `bar: Some(BarTabs { views, active })`. Its frame (`PaneFrame::with_tab_row`) keeps the title row empty and
  `ui::bar::draw_tabs` draws the labels there (`Explorer - Git`, slots `bar.tab.active` / `bar.tab.inactive`);
  `ui::bar::tab_rects` is the one place that knows where each label is, for drawing and for clicks. A bar is named by
  its first view; `AppState::bar_views` remembers which view each bar shows. Every view keeps its own `Hideable`
  flag and its state while it is not the one shown (the terminal's shells keep running); `is_visible` is the flag,
  `is_showing` is flag and shown-by-its-bar, and hiding the shown view hides the bar. `set_shown(view, true)` makes
  the bar show that view, which is how the toggles, the remembered state and the startup do it. A click on a tab
  shows the view and gives it the keyboard (a middle click goes to the next view; "Bar: Next View" / "Bar:
  Previous View" do the same from the keyboard for the bar the keyboard is in). A view is named once in a layout, and
  the editor and the status bar cannot be in a bar. The Errors view is the second view of the default BottomBar (`components/errors`): `ErrorsView` keeps only the selection and the scroll, and the rows (`ErrorRow`) are made from `AppState::diagnostics` and the open documents at every draw and every key, so they cannot go stale. It has its own `Focus::Errors`, keymap context `errors` and `Action::Errors(ErrorsCommand)`; the label of its tab gets the count in `AppState::placements` (`BarTabs::labels`). Git is meant to be a second view of the SideBar.
- **Main menu.** `MenuBar` (a component, `Fixed(1, Pane(view: MenuBar))` at the top of the default layout, hidden with "View: Toggle Menu Bar" and remembered like the other panes) is a row with the `≡` icon (plain three lines, since a gear needs a font that has it). A click on the icon, `f10` or "View: Open Menu" opens the main menu, a `Menu` made with `Menu::on_app`: it keeps the keyboard where it was and gives it back on close, and with the row hidden it starts at the top left of the screen. `Menu` entries are an `Item`, a `Group` (opens a menu to the right, level with its row, one level deep; the open submenu has the keys: right or enter opens, left or `esc` closes it first) or a `Separator`; `Menu::key` and `Menu::click` return a `Step`, so the app only runs what comes out. The menu is built in `App::main_menu_entries` from the same `Action`s as the palette, with `keys_for` for the key beside each.
- **Open File and Open Folder** (`app/open.rs`) use the name prompt (`Purpose::OpenFile` / `OpenFolder`). A file opens through `Workspace::open_path`. A folder cannot be swapped under a running editor (the explorer, the watcher, the finder, the language servers and the shells all hold the root), so Open Folder sets `App::restart_in` and quits (with the unsaved-files prompt; its cancel clears the field); `App::run` and `lib::run` return the folder, and `main` ends the terminal and starts `trcode -- <folder>` again (`exec` on Unix). A re-root of the running editor would keep the tabs but is a change in six places; it is not planned.
- **Open Recent** (`persist/recent.rs`, `app/recent.rs`): `Recent` is two lists (files, folders; latest first, deduplicated, at most `MOST` = 10) in `recent.json` beside `state.json`, written with `saved_state::write_atomically`. `App::remember_recent` runs after every input and event (like `remember_panes`): it notes the project folder once and each open document's file the first time it is seen, so every way of opening a file counts and switching tabs does not reorder the list. The Open Recent group is built when the main menu opens, after dropping what is gone from the disk.
- Theme is named style slots in TOML (`crates/ui/defaults/default_theme.toml`, embedded the same way). Components ask for a slot,
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
  stores `desired_column` for Up/Down. `Selections` is a list kept sorted with overlapping
  selections joined (a cursor on the edge of a selection, or two cursors in one place, join too); one is primary.
  **Several cursors** (`components/editor/multi.rs`): `ctrl+alt+up/down` add a cursor above or below the primary one
  (it becomes the primary one), `esc` goes back to the primary one. Motions, selecting by motion, typing, a line break,
  the four deletes, copy, cut, paste and undo/redo act on every cursor; each of them plans one change per cursor against the text as it
  is, applies them last first in one transaction (so one `ctrl+z` takes back the whole burst, and typing bursts still
  merge into one step) and puts a cursor after each change (`Editor::edit_each`); deletes that run into each other are
  joined. **Clipboard with several cursors:** copy joins the selections with line breaks, and a bare cursor gives its whole line (once when cursors share a line); the register is linewise only when no cursor has a selection, otherwise charwise. Cut removes the selections and the bare cursors' lines. Paste with as many lines as cursors gives line n to cursor n; any other text goes whole to every cursor (a copy of `pub ` pastes at all of them). A linewise register with only bare cursors goes above each cursor's line (above each distinct line when every cursor gets all of it) and every cursor stays on its character; otherwise it is plain text replacing each selection. Every other operation (indent, outdent, replace, completion, go to) acts on the primary cursor and
  leaves one cursor, because it replaces the selection list with a single one. A line break with several cursors only
  copies the indentation (no smart indent after a bracket), and a closing bracket does not dedent. The primary cursor is
  the terminal's own cursor; the others are blocks in the `cursor.extra` theme slot (swapped colours when a theme has
  none), and every selection is highlighted. `alt+click` adds a cursor (the word on a double click, the line on a
  triple click) and `alt+drag` a selection (`ViewState::drag_base` holds the selections it is added to); a plain click
  puts the selection list back to one.
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
  panic hook, pops it). After mouse reporting is switched off, `restore` sends a status request (`CSI 5 n`) and
  reads the tty (`src/terminal/drain.rs`, Unix only) until the answer (`CSI 0 n`) or 50 ms pass: the answer comes after every
  mouse report sent earlier, so none is left for the shell to print as text at its prompt (decided: the handshake directly,
  not a plain drain, because the leak is a race that cannot be reproduced on demand; a terminal that never answers costs
  50 ms once; `relaunch_in` runs after `restore`, so the new program starts clean too). Without kitty support (`KeyboardSupport::Legacy`) `defaults/default_keymap_legacy.toml`
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
  prefix chord, which makes the next key an IDE shortcut. The prefix that shipped is `ctrl+b` (see "Terminal keys" below), not the `ctrl+g` first
  planned here; `ctrl+g` goes to a line of the open file.
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
- Deleting (`app/popups/delete.rs`, `files/removal.rs`): the selection goes to the OS trash. The project root cannot be
  deleted. Open tabs of the deleted path close; if any has unsaved changes a `Confirm` popup asks first. If the
  trash move fails (e.g. on a mount without a trash), a second `Confirm` offers permanent delete. In a `Confirm`
  only `y` says yes; Enter, Esc and everything else cancel. Tests swap the trash for a stand-in
  (`App::with_trash`) so they never touch the real one.
- The name prompt (`components/name_prompt.rs`; also Open File and Folder, Move, Go to line and New Theme) has a text cursor: `left`/`right` by grapheme (a flag or a letter with its accent is one step), `home`/`end`, `ctrl+left`/`ctrl+right` by word (letters, digits and `_`; dots and slashes separate), `delete`, backspace at the cursor, `ctrl+u` clears, and a click on the text puts the cursor there (the nearer edge of a wide character). A name longer than the box scrolls to keep the cursor in view (`NamePrompt::place` finds the box and the scroll before each draw, like the menus, so drawing keeps no state). Decided: only this prompt for now; the finder, the find box and the project search have their own inputs, still without a movable cursor.
- Indent Line and Outdent Line (`ctrl+]`, `ctrl+[`; `Editor::indent_line` / `outdent_line` in `indent.rs`) move every line a cursor is on or a selection touches by one level, each line once however many cursors share it, and keep every cursor on its text (a selection's start at column 0 stays, so it covers the new indent); a blank line inside a block of several lines stays blank, the blank line of a cursor is moved with the rest. `Tab` and `Shift+Tab` run the same code (`shift_lines`) on the primary selection alone. `Tab` and `Shift+Tab` are unchanged. Legacy terminals (decided: fallbacks in `default_keymap_legacy.toml`): `ctrl+]` arrives as `ctrl+5` (crossterm maps 0x1d), `ctrl+[` is the escape byte, and `alt+[` would be the start of a CSI sequence, so there `ctrl+5` and `alt+]` indent and `alt+\` outdents.
- Toggle Line Comment (`ctrl+/`; `Editor::toggle_comment` in `components/editor/comment.rs`): the marker comes from `Language::comment` (`//` Rust and JSON including `.jsonc`, `#` TOML and Nix, `<!-- -->` around the text of the line for Markdown; a file with no known language, or an untitled one, gets a notification instead). The lines are those of every cursor and selection, each once; blank lines inside a block are left out and do not decide, the blank line of a cursor is commented with the rest (or alone, as `// `); if every line with text has the marker it is taken away (with one space after it when there is one), otherwise it is put at the smallest indentation followed by one space. Cursors keep their text (a cursor at the start of the code goes along, one at the end of a Markdown line stays before the `-->`). One undo step. Legacy terminals: `ctrl+/` is byte 0x1f and arrives as `ctrl+7`. A doc comment `///` counts as commented and loses `//`, as in VS Code.
- Creating (`app/popups/new_entry.rs`, `files/entries.rs`, `components/name_prompt.rs`): `a` / `shift+a` open a one-line
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
  directory can stall a frame; move to a worker if it shows up), listing by the `ignore` crate
  (`.gitignore` honored even outside a repository; what it leaves out is listed too, dimmed and openable; `.git` never listed, other dotfiles shown), directories first
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
- `settings.json` first slice (built, `src/settings/`): `<config dir>/terminalcode/settings.json`, JSON with `//` and
  `/* */` comments and trailing commas (a small stripper turns them into spaces, so error line numbers match the file).
  One setting so far, `theme` (the file name of an installed theme, or `null`; see "Built-in themes are templates"); the vim setting waits for the vim layer, which is after 0.1.0. Left-out settings keep their defaults. A setting with the
  wrong type makes the whole file unusable (reported with the setting's name, the defaults are used, and a reload keeps
  what is applied); a name that is not a setting, or is set twice (the last counts), is reported and the rest applies. `Config: Open Settings` makes the
  file from `defaults/settings.jsonc`. No project layer yet.
- The `theme` setting is the starting theme, not a store for the palette's choice: `Theme: ...` lasts for the run only
  and its notification says so and names the setting (rewriting a hand-edited JSONC file would lose its comments). A
  reload (the file was saved) applies a setting only when the file changed it, so a palette choice survives saving
  `theme.toml`, but editing `theme` in `settings.json` and saving applies at once.
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
- The five biggest test files in `src/tests/` (terminal, lsp, config_files, completion, disk) are folders: `mod.rs` holds the imports and helpers and each file holds the tests of one section (for example `lsp_tests/diagnostics.rs`).
- Top-level modules are grouped: `files/` (entries, paths, removal, rootsave, watcher) and `persist/` (saved_state, recent); `cli`, `error`, `startup` and `terminal` stay at the top.
- **Workspace.** The repository is a Cargo workspace (root package `terminal_code`, binary `trcode`, plus `crates/*`), to enforce the layering with crate boundaries and so that CI compiles only the crates that changed. Decided: leaf crates first, in this order of dependence: `trcode-ids`, `trcode-buffer`, `trcode-files`, `trcode-pty`, `trcode-lsp`, `trcode-syntax`, `trcode-clipboard`, `trcode-jumplist` and `trcode-ui` (done, in that order), and `trcode-plugin-api` (the messages of plugins, which authors depend on, so it has no other dependency than `serde`; see "Plugin runtime"); the knot of `app`, `components`, `ui`, `event`, `config` and `error` stays in the root package until its cycles are broken. A crate depends only on crates below it. `trcode-syntax` asks the theme for styles through its `CaptureStyles` trait, which the root's `Theme` implements. The root keeps its old paths with `use trcode_buffer as buffer;` in `lib.rs`, so `crate::buffer::X` still works. Items a crate exports are `pub`; test-only helpers the root's tests need are not `#[cfg(test)]` (cfg(test) code is not built when another crate depends on it); the fake servers of `trcode-lsp` (`lsp::test_support`) and the in-memory clipboard of `trcode-clipboard` (`clipboard::Memory`) are `test-support` features that the root enables as dev-dependencies.
- `src/ui/` depends on nothing above it: `ComponentKind` and `PluginViewId` (the vocabulary of a layout) live in `ui/component_kind.rs`, the `Render` trait takes the state as a parameter (`Render<AppState>` in the app; `()` in `ui`'s own tests), and `bar::draw_tabs` takes the theme, not `AppState`. A check: `grep -rE "crate::(app|components|event|config|keymap|settings|error)" src/ui` finds nothing. `ui` (about 2k lines: theme, layout, backdrop, cursor, frames) is the crate `trcode-ui`; it depends on `trcode-syntax` only to implement `CaptureStyles` for `Theme`, and carries the default theme and layout files in `crates/ui/defaults/`. `components` and `app` stay as one package with their module boundaries.
- The popup views (menu, palette, name prompt, confirm, quit prompt, root prompt, definition list, diagnostic popup and the find, finder and search boxes) are in `src/components/popups/`; what each does with keys and mouse is in `src/app/popups/` under the same name.
- `app/lsp.rs` keeps the servers' life (the `Lsp` struct, starting one for the file in front, the sync, the status and "LSP: Show Server of This File"); the diagnostics are in `app/diagnostics.rs`, and what asks a server for completion, hover and definition is in `app/completion.rs`, `app/hover.rs` and `app/definition.rs` beside what handles the answers.
- `src/lsp/` is grouped: `protocol/` (framing, message, connection, handshake, position, uri, replies, error), `session/` (launch, manager, supervisor, sync, progress, config, cargo, language, diff) and `features/` (completion, completion_state, snippet, definition, hover, docs, diagnostics); `lsp/mod.rs` re-exports what the app uses and names each file at the `lsp` level, so `crate::lsp::hover` and the like still work inside the client.
- `app::state::AppState` is split by area (`components`, `popups`, `notices`, `lsp` next to `state.rs`).
- `components::workspace::Workspace` (split by concern: `documents`, `tabs`, `panes`, `mouse`, `editing`, `highlights`, with the ids in `src/ids.rs`) stores each file once as a `Document` (buffer + undo history + indent style) in a
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
- Command line (`src/cli.rs`): every path argument opens a tab (first one active). `-h`/`--help` and `-v`/`--version` count only as the first argument, print and exit before the terminal is touched; `--` first makes everything after it a path.

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
- A pane can override it: `[background.editor]`, `explorer`, `terminal`, `statusbar` and `popup` (one table for every
  box) take the same `color` / `opacity` / `blend` keys and inherit each key they leave out from the screen's table
  (`LayerSpec::merged_with`, so a pane that only sets `color` keeps the screen's opacity; `color = "transparent"` lets
  the terminal show in that pane). They are merged key by key across theme layers like `[background]`. The tables are
  in the theme file, not the layout, so each theme picks shades that suit it. `ui::backdrop::paint_pane` paints a
  pane's table over the screen's just before the component draws, and `backdrop::clear` takes the `popup` table for
  boxes (`Theme::pane_color`). A tinted pane makes the theme ask for the terminal's background like the screen does.
- Painting is one `Block` over the whole frame at the start of every draw (`components::paint_background`); slots with
  their own `background` (selection, tabs, status bar) draw over it.
- A box drawn over the text (palette, finder, search, menus, prompts, completion, hover, notifications) starts from
  `ui::backdrop::clear`: it wipes the area and paints the same background again; a plain `Clear` would leave the
  terminal's default there. The terminal pane copies only the colors a program set: a cell with the default
  background keeps the painted one, so the shell's default looks like the theme.
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
  lookup, and `crates/ui/defaults/default_theme.toml` has the `syntax.*` slots. It builds with just a C compiler (`cc`);
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
  paste; (D) scrollback and the alternate screen; (E) mouse passthrough and terminal tabs. Windows (ConPTY) could not
  be tested here at the time; CI does it now.
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
  `portable-pty` and `vt100` crates only; Windows (ConPTY) is tested on `windows-latest`, see "ConPTY" below.
- Step A2 is done (`src/pty/tests/flood_tests.rs`): a `probe` stands in for the app between two events: it hands a
  byte to the shell and, when the screen changed, looks at every cell. While a
  shell prints scrolling lines (`yes | head -c N`) the slowest key took about 9 µs and the slowest frame about
  2.7 ms in a debug build (0.3 ms in release); 100 MB parses in about 1 s in release and about 12 s in debug, so
  the 100 MB test is `#[ignore]`d (`cargo test --release -- --ignored`) and the ordinary suite runs the same test
  with 10 MB. Also tested: one wake-up per drawn frame during the flood, keys reaching a shell that is busy printing,
  and an 8 MB paste into a raw-mode shell that is not reading (the write returns with the rest
  dropped; drawing, further keys and the child's output stay live). Decided: no test asserts how long a key, a frame
  or the paste took (a shared runner stretches any limit and made them flaky); a call that blocks for good is a test
  that never ends, which nextest kills and reports.
- Step B is done. The terminal pane (`components/terminal`, `TerminalPane`) is hidden by default (`Hideable::new_hidden`)
  and toggled with `` ctrl+` `` (`toggle_terminal`, "View: Toggle Terminal" in the palette). Showing it starts the
  shell if there is none or it ended: in the project root, with the size the pane has when shown, worked out from
  the layout and the screen as of the last draw (`AppState::terminal_size_now`), so a shell that asks for its size
  straight away already gets the right one. Each frame `prepare` takes the dirty flag (the next change wakes the
  app again) and resizes the shell if the pane's text area changed; while hidden the pane keeps its last size.
  `Event::TerminalChanged` (sent with `blocking_send`, because a lost wake-up leaves the screen stale) redraws and
  reports the end of a shell once, with its exit code. `components/terminal/render.rs` copies the `vt100` cells into
  the frame buffer: colors (default/indexed/RGB), bold/dim/italic/underline/inverse, wide characters take two
  cells, combining marks stay with their letter. The pane only draws so far: no focus, no keys (step C), no
  cursor, no scrollback (step D).
- Step C is done. `Focus::Terminal`: `` ctrl+` `` shows the pane and gives it the keyboard, gives the keyboard to a pane
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
  editor, the prefix, the status bar, pastes, mouse focus, and the cursor position.
- Step D is done. Scrollback comes from `vt100` (10,000 lines, the normal screen only): `Session::scroll_by`,
  `scroll_to_top`, `scroll_to_bottom` and `scrollback_offset` move the view through the history (clamped at both
  ends); `vt100` keeps the view on the same lines when output arrives, so a scrolled view does not jump. In the pane,
  `shift+pageup` / `shift+pagedown` move a page less one line, `shift+home` / `shift+end` jump to the oldest line and
  the present, and the wheel over the pane moves three lines a notch; typing or pasting brings the view back to the
  present, new output does not. A scrolled view shows ` ↑N ` (reversed) in the bottom right corner and hides the
  cursor. The alternate screen (`ESC [ ? 1049 h`, what vim, less and htop use) is a screen of its own without
  history: the scroll keys go to the program as `ESC [ 5 ; 2 ~` and so on, the wheel does nothing there (until mouse
  passthrough, step E), and the normal screen and its history come back untouched when the program leaves it.

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
the embedded `crates/ui/defaults/default_theme.toml`; there is no user theme file.
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

## Extra highlight patterns (built)
- The grammar's own highlight query is not enough for a VS Code look, so `Language::highlights` wraps it with the
  editor's patterns: `before` the grammar's, which win where both capture exactly the same text (the first pattern
  wins a tie), and `after`, which only fill in what the grammar leaves uncolored. Only Rust has any so far
  (`RUST_BEFORE`, `RUST_AFTER` in `src/syntax/language.rs`).
- Before: a called method (`x.foo()`, `x.foo::<T>()`) is `@function.method`, because the grammar captures the same text as `@property` first and wins the tie; `if else match loop while for in break continue return yield` are `@keyword.control` (the grammar has one
  `@keyword` for all of them), and integer and float literals are `@number` (the grammar calls them
  `@constant.builtin`). After: a path segment is `@namespace` (the grammar already takes capitalized ones for
  types), and any identifier nothing else captured is `@variable`.
- Theme slots `syntax.keyword.control` (purple), `syntax.namespace` (teal), `syntax.variable` (light blue) and the
  existing `syntax.number`; a capture whose slots are missing falls back to its parent (`syntax.keyword`) or is left
  plain, as before. Nix already had a `@variable` capture, which the new slot now colors.
- The toggle chord (`` ctrl+` `` by default, bound to `toggle_terminal`) is the second chord the terminal keeps for the
  editor, besides `ctrl+b`. From the editor it shows the pane and gives it the keyboard (or only gives the keyboard
  to a pane that is already shown); with the keyboard in the pane, the same chord hides the pane and puts the
  keyboard back in the editor, and the shell never sees it. `handle_terminal_key` asks the keymap whether the chord
  alone is bound to `toggle_terminal` (`Keyboard::action_for`), so a rebound key works the same, and calls
  `toggle_terminal` directly (going through `run_action` would first move the focus to the editor and turn the
  hide into a re-focus). `ctrl+b t` still does the same. The tests' rig presses `` ctrl+` `` to toggle.
- Step E is done. Mouse reporting: when the program asked for the mouse (`vt100` tracks `?1000h`, `?1002h`, `?1003h`
  and the encodings `?1006h` SGR and `?1005h` UTF-8), mouse events over the pane's text are sent to it as xterm
  reports (`pty/mouse.rs`: `ESC [ < b ; x ; y M` / `m` for SGR, `ESC [ M` plus three bytes otherwise; positions
  count from the text area's top left; wheel notches are buttons 64..67; motion is only sent as far as the mode
  asked; the default encoding drops positions past 223) instead of acting on the pane, and shift keeps the
  event for the editor (the usual escape hatch). Without a request a click gives the pane the keyboard and the wheel
  scrolls the history, and every other mouse event over the pane is kept away from the editor (a drag that started in
  the pane used to select text in it). Tabs: `TerminalPane` holds up to 16 tabs, each a shell, and a tab bar
  (`tab.bar` / `tab.active` / `tab.inactive`) takes the top row of the pane's text; all shells get the text area's size. Behind the prefix `c` opens a tab, `n` / `p` step through them,
  `1`..`9` pick one, `w` closes the active one (the last one hides the pane); a click on a tab picks it and a middle
  click closes it; palette: Terminal: New / Next / Previous / Close Tab. A shell that ended stays as a tab marked
  `(ended)` until it is closed, each one reported once with its number; showing the pane when every shell has ended
  drops them and starts one.
- The tab bar is always shown while the pane has a tab (it took a row only from the second tab on at first, which
  left the " + " button with nowhere noticeable to be). A green, bold " + " (theme slot `tab.add`, over `tab.bar`) sits
  right after the last tab, or at the end of the bar when the tabs fill it; a left click on it is the same as
  `ctrl+b c`: a new tab, with the keyboard. Every shell therefore has one row less than the pane's inner height from
  the start, and opening a second tab no longer resizes the first.

### ConPTY (Windows terminal), found by running the suite on `windows-latest`
- ConPTY writes `ESC [ 6 n` (where is the cursor?) before it starts the shell and holds the shell back until the
  terminal answers. Without an answer the session got 4 bytes, showed nothing and never ran the shell, so every
  terminal test failed. `src/pty/replies.rs` answers it with `ESC [ row ; column R` from the screen's cursor, as of
  the moment of the query, also when the query is split over two reads. Other queries (device attributes and so on)
  are not answered yet.
- ConPTY keeps the terminal open after the shell exits, so the reader never sees the end. A watcher thread in
  `session.rs` polls the child every 100 ms; when it is gone it waits for the output to go quiet (25 ms, up to ten
  times), marks the session ended and wakes the app. Unix still ends through the reader.
- `canonicalize` on Windows gives `\\?\C:\...`, which `cmd.exe` cannot use as a working directory. The shell is started
  in the path without the prefix (`paths::without_verbatim_prefix`).
- ConPTY redraws its screen instead of passing the output through, so the lines of scrollback it leaves differ by a
  few from a Unix terminal's, and its input never "fills" the way a raw-mode Unix terminal's does. The tests that
  depend on exact history counts, on a shell that stops reading, or on `pwd` (Git's `sh` writes `/c/Users/...`) run on
  Unix only; the Windows tests use `cmd.exe` (`src/pty/tests/windows_tests.rs`).
- CI runs the tests with `cargo nextest` (`.config/nextest.toml`): a test that hangs is killed after a minute and
  reported, and a job cannot hold a runner for more than 20 minutes.

## LSP client (plan, decided before building; first slice is diagnostics)

**Where it lives.** A generic client in the core, not a plugin (this replaces the earlier idea that language servers
arrive as plugins). Rust is the first, built-in configuration. A plugin may later add servers or UI on top of it.

**First slice and what "done" means.** Diagnostics from `rust-analyzer`, shown on the right lines. The on-type
diagnostics (syntax errors, unresolved names, most type mismatches) must show as you type; the `cargo check` ones
(borrow checker and so on) show when the server pushes them after a save. Nothing else is promised: no hover, go to
definition or completion yet, and no diagnostics list for files that are not open (they show when the file is opened).
Proof: a scripted fake server in CI on all three platforms, and one ignored test that starts the real `rust-analyzer`
on a small temporary crate with a type error and checks the line (run by hand; CI never runs the real server).

**Configuration** (`settings.json`):

```jsonc
"language_servers": {
    "rust": {
        "command": "rust-analyzer",
        "args": [],
        "extensions": ["rs"],
        "root_files": ["Cargo.toml"],
        "initialization_options": {}   // raw JSON, passed to the server as it is
    }
}
```

- The servers there are come from the default settings file (`defaults/settings.jsonc`: Rust, Nix, JSON), which is the base layer: the user's `language_servers` is laid over it by name and by field. An entry for a server that is in the defaults overrides only the fields it has and keeps the rest (`"rust": { "format_on_save": true }` is enough), a list such as `args` is replaced and not added to, `"rust": null` turns a server off, and an entry with another name adds one (it needs its `command` and `extensions`). Nothing is built into the code: a file from before, with `"language_servers": {}`, keeps every default server. (Before the layering an entry replaced a built-in server whole.)
- A file is matched by extension. The server's root is the **outermost** ancestor folder, at or below the project
  root, that holds one of the `root_files`; none found means the project root. One server per root, so a Cargo
  workspace with many members is one server. For Cargo (`src/lsp/session/cargo.rs`) the outermost folder only counts when its
  `Cargo.toml` takes the file's crate in: a workspace member (`members` with `*` and `?` patterns, minus `exclude`) or a
  path dependency (also `dev-`, `build-` and `target.*` ones, followed through other crates up to 8 deep); otherwise
  the next folder in that does, at the end the crate's own folder. Decided after reproducing it with the real
  rust-analyzer: a crate that only sits in a folder below the root is not checked at all (no diagnostics, hover or
  definition) although the status bar says the server runs. "LSP: Show Server of This File" says which server a file
  has (name, root, state, whether the server was told to open it) or the first thing that is missing.
- Left out on purpose: per-server environment variables and server-specific settings other than
  `initialization_options` (that one is the escape hatch, for example `"cargo": {"features": "all"}`).

**Position encoding: UTF-16 only.** LSP positions are `(line, character)` in UTF-16 code units. We negotiate nothing
and always convert between UTF-16 offsets and buffer bytes, so there is one code path and it works with every server.
The conversion walks the line (O(line length), the same class as layout and the documented long-line limit). Tested on
the fake server with an emoji, a combining accent, a tab and CRLF. If a real file makes it slow, measure and revisit.

**Failure policy** (in the spirit of the terminal's): a broken server never blocks typing, and the user is told once.
- Spawn fails, or the process exits at once (the `rustup` shim for a missing component does this): reported once with
  the reason (the first line of stderr), the status bar shows the server as off, and it is not tried again until the
  settings change or the project is reopened.
- A crash while running: restarted automatically, at most 3 times in a short window, with a notice each time; after
  that it stays off and says so.
- Slow start or indexing: the status bar shows the server's own progress text, so a missing diagnostic does not look
  like "no errors".

**Staying on the right lines while you type.** Diagnostics arrive for the document version the server last saw, and
the buffer may be several versions newer. Each diagnostic's range is mapped through every edit made after that version
(the same mechanism as syntax colors: `map_spans` and the buffer's edit log), a diagnostic whose range an edit deletes
into is dropped, and the next push replaces the set. After a reload from disk, or when the server's version is older
than the history kept, all of them are dropped and a fresh push is awaited. This needs a short, version-bounded edit
history per document.

**Display.** The message is written **after the code on the line**, like nvim's virtual text (a first version marked the
gutter with a dot, which read as a breakpoint): the first line of the worst diagnostic that starts on the line, two columns
after the code, in the color of its theme slot (`editor.diagnostic.error`, `.warning`, `.info`, `.hint`), with ` (+N)` for the
others that start there. It is cut with `…` to the edge of the text, left out when fewer than 8 columns are left (a long
line) or when the line has scrolled out of sight, and said once for a diagnostic over several lines, on the line it starts
on. Under the text each diagnostic is about there is a line, in the slot's new `underline` key (only the color, so the code
keeps its own syntax color, and a terminal that cannot color underlines draws it in the text color); a worse severity is
drawn over a milder one where they meet. The status bar shows only how the server is doing (a first version also repeated
the cursor line's message there; it only duplicated the inline text, so it was dropped). A hover popup arrives with the
`f7` opens a box next to the cursor with the whole text of the diagnostics on its line, for reading a message that was cut
or has more lines than the one shown (a compiler's notes and help); it opens on demand only, scrolls with the arrow and page
keys and closes with any other key. `f8` and `shift+f8` go to the next and previous diagnostic, round the file.

**Sync.** `didOpen` when a server runs and a matching document is open (all open documents it serves, not only the
one in front), `didChange`, `didSave` after a save and `didClose` when the document goes away or has another name.
Built (this replaced the first idea of replaying the buffer's edit log, which has byte ranges but not the replaced text
and has one consumer already, the syntax worker): the client keeps a copy of each document as the server has it (a rope
clone, which shares its text) and sends the **difference between that copy and the text now** as one incremental change,
the common start and end kept and the middle replaced, never splitting a character or a `\r\n`. It does not depend on
how the text got there (typing, undo, replace all, a reload from disk), so it cannot disagree with the text, and a burst
of typing between two frames is one message because the sync runs once before each frame. Positions are converted with
the rope's UTF-16 counts in logarithmic time. A server that started again, or whose settings changed, is told
everything again (each run of a server has a number). Measured: an edit in 10 MB is worked out in about 5 ms and the
first `didOpen` of 10 MB takes about 17 ms (release build).

### Completion (decided after a grill; not built)
- **What the user sees:** while typing, a box of suggestions appears next to the cursor, like most IDEs. `Tab` writes the highlighted one, `Up` and `Down` move the highlight, `Esc` closes the box, and `Enter` closes it and is a plain newline. `Tab` only accepts while the box is showing a match; with nothing matching the box is closed and `Tab` indents as always.
- **When it opens:** automatically from the first identifier character, and after the trigger characters the server names (`.`, `::`). There is no manual key in the first version. Inside comments and strings the server decides: no rule of our own, the box shows where the server has something to say.
- **Rows:** the name on the left, a one-letter kind tag next to it (`f` function, `s` struct, ...), the server's short `detail` dimmed on the right; at most 8 rows, scrolling. Above the list (below it when there is no room above the cursor, and cut to what fits) a small box shows the documentation of the highlighted item, as plain text (markdown marks taken out), at most 8 lines, only when the item has any; there is no hover yet.
- **Matching and order:** prefix only (`pu` finds `push`, `psh` finds nothing; case does not matter, so `hash` finds `HashMap`), compared with the server's `filterText`, ranked by its `sortText`. The list from the first request is kept and filtered locally as the word grows, so there is no flicker and no request per key. When the server marks a list `isIncomplete`, a new request goes out on each further character instead (and for any list when the word was shortened below what it was asked for, since a list is only complete for longer words); the old filtered list stays visible until the new one arrives, and an answer for text that has since changed is dropped.
- **What is written:** the item's own text edit (or its `insertText`, else its label), the server sends snippets (the client declares `snippetSupport`), so a call comes with its brackets: `push(${1:value})` is written `push(value)` with `value` selected (typing replaces it, backspace leaves `push()`), `len()$0` puts the cursor after the brackets, `HashMap<$0>` inside the angle brackets. Only the first tab stop is kept (no jumping between stops; the other placeholders stay as text with their defaults); a choice gives its first alternative, a variable its default or nothing, and the lines after the first of a multi-line snippet are indented like the line the item goes into. The item's `additionalTextEdits` (the auto-import `use` line) are applied in the same transaction, so one `ctrl+z` undoes both. Ranges are UTF-16 as everywhere else and are moved through whatever was typed since the request was made.
- **Failure:** a server that is not running, still starting, or does not offer completion shows no box; there is no message.
- **Done when:** typing `Vec::ne` in a real Rust file shows `new` in the box and `Tab` writes it; typing `HashMap` and pressing `Tab` adds the `use` line, and one `ctrl+z` removes both edits.
- **Order of building:** (1) protocol: declare completion support, send `textDocument/completion`, parse the items and the incomplete flag, with the fake server; (2) the model: prefix filter, `sortText` order, applying an item as one transaction with the import edit, mapped through later typing; no UI; (3) triggering and the box: opening and closing rules, `Tab`/`Up`/`Down`/`Esc`, stale answers dropped; (4) one check by hand against the real `rust-analyzer`. CI tests use the fake server; the real-server test is ignored and run by hand, like the diagnostics one.
- **Found when checking against the real `rust-analyzer` (step 4), and changed because of it:** a server offers the items that are not imported yet (`HashMap`) only to a client that says it can resolve `additionalTextEdits` later (`resolveSupport`), so the client now declares that, and picking an item sends `completionItem/resolve` for it (after the servers were told the text as it is, waiting at most 2 s) and writes the `use` line that comes back with the word as one undo step; when the server does not answer in time the name is written without the import and a message says so. rust-analyzer says `resolveProvider: false` and resolves anyway, so an item that carries `data` is resolved whatever the server says (and every item of a server that says `true`). The client also declares `labelDetailsSupport`, since without it the server writes the import path into the label (`HashMap(use std::collections::HashMap)`). Another thing the real server showed: it answers `[]` or a list for a word it has not indexed yet, so a box can be empty in the first seconds after the server starts; nothing is done about that.

### Hover (decided with the user; built)
- **What the user sees:** the mouse pointer resting on a word for 400 ms asks the server (`textDocument/hover`) about it, and its answer comes in a small box hanging under the pointer (above when there is no room), as plain text (markdown made plain by `lsp/features/docs.rs`, rules between parts are blank lines), at most 14 lines, cut with `…`. `f6` (palette "Hover: Show Documentation at Cursor") does the same for the word at the text cursor, or the word just before it, with the box under the cursor.
- **When it goes away:** the pointer moves to another word or off the text (moving inside the same word keeps it), any key (`Esc` does nothing else then), a click, the wheel, an edit of the text, another tab or document, or a popup or the box of suggestions opening. An answer that comes after any of that is dropped.
- **How:** `Moved` events are looked at after the terminal pane, the palette and the menus had their turn, so a terminal app that wants mouse motion still gets it. The pane under the pointer need not be the focused one (`Workspace::text_at_cell` changes nothing). The loop wakes at `hover_deadline` like it does for notifications. The servers are told the text as it is before the request goes out, and the answer is looked for when the server makes a sound (every answer wakes the loop), like the box of suggestions. A server that does not offer `hoverProvider` is not asked. No message is shown when the server has nothing to say.
- **Not done:** go to definition (clicking or `ctrl+click` on the word), a box that can be scrolled or selected from, and a delay setting (400 ms is a constant).
- **Pinning and scrolling (asked for after the first version):** `f6` with a box up from the pointer *pins* it: it stays when the pointer moves on or the wheel turns elsewhere, and goes with a key (`esc` only closes it; any other key closes it and does its work) or a click; `f6` on a pinned box closes it. Without a box `f6` asks about the word at the cursor and the answer is pinned from the start. The text is no longer cut at 14 lines: the box shows 14 lines at a time, says which (`1-14/59` on its bottom edge), and scrolls: the wheel over it (3 lines a notch; the box also stays while the pointer is on it, so it can be reached), and, while it is pinned, the arrow keys (a line) and the page keys (a page) instead of moving the cursor. The documentation box beside the suggestions scrolls the same way: the wheel over it, or `pgup`/`pgdn` while the suggestions are open and it has more than it shows (otherwise they are the editor's, which closes the suggestions by moving the cursor); another highlighted item starts at the top. `TextScroll` (`components/scroll_text.rs`) holds the offset and where the box was drawn, for both.

## Go to definition and the jump list (decided after a grill; built)
- **Go to definition:** `f12` on the word at the cursor, and `ctrl+click` on a word, ask the server (`textDocument/definition`; answers may be one location, a list of them, or links with a target range). One location: the cursor goes there, opening the file in a tab when it is not open. Several (a trait method has one for each implementation, a macro can point at itself and at its expansion): a small list to pick from, each row the path, the line number and the text of the line; up and down, `enter` to go, `esc` to close, like the other popups. No answer: a short message says so.
- **Files outside the project** (the standard library sources under `~/.rustup`, dependencies under `~/.cargo/registry`): opened as read-only tabs, so they cannot be edited by accident (edits are refused, undo still works), and still served by the project's server, so hover and go to definition keep working inside them. That is not certain to work with rust-analyzer, so it is checked against the real one before anything relies on it.
- **The jump list** (the cursor-history item of the backlog, built together with this): `alt+,` goes back and `alt+.` forward (not `alt+left` and `alt+right`, which are word motions where terminals cannot report `ctrl+arrows`). One stack for everything. What records a place: go to definition, a search or finder hit, go to line, `f8` and `shift+f8` to a diagnostic, opening or switching to another file, and a mouse click more than a screen away from the cursor. Plain keyboard movement, typing and paging record nothing. A new jump after going back drops what was ahead. At most 100 entries; not saved between runs.
- **Entries move with edits:** a place is kept as a document and a byte offset and is moved through every edit of that document, the way the selections of other tabs are, so it stays on the same text. An edit that takes the text the place was on puts it at the start of what was taken.
- **An entry whose tab was closed:** back reopens the file from disk and puts the cursor on the place. When the file is gone the entry is skipped, going on to the next older one, with a short message that one was skipped.
- **Order of building:** (1) the jump list on its own (entries, moving with edits, back and forward, the limit, closed files), with tests; (2) recording the jumps that exist already, and `alt+,` and `alt+.`; (3) the protocol: `textDocument/definition` request and answer (all three shapes), with the fake server; (4) `f12` and `ctrl+click` with one location, files outside the project read-only; (5) the list for several locations; (6) the check against the real `rust-analyzer`: a definition in the project, one in the standard library, and a jump from inside it. Done when `ctrl+click` on `Vec::new` in Geonl opens the standard library source read-only at the right line, `f12` there goes on to the next definition, and `alt+,` twice brings you back to where you were.
- **How it came out:** the jump list is `src/jumplist/` (a pure list) and `src/app/jumps.rs` (what records and walks it: the place from before each input is recorded when the input was a jump, and the places move through the edits the workspace reports); go to definition is `src/lsp/features/definition.rs` and `src/app/definition.rs`; the list is `src/components/definition_list.rs`. A file outside the project is locked by `Buffer::lock`, which the file permissions never clear and saving refuses, and it is served by the server that pointed at it (`borrowed` in `app/lsp.rs`), not by one started for its own folder. Checked against the real `rust-analyzer`: `Vec::new` goes to the standard library source, and a jump from inside it goes on. Not done: the list reads the text of each line from the file on disk, not from an open buffer with unsaved edits; a go to definition from the explorer or on `ctrl+click` in an unfocused pane works on the pane under the pointer but is not checked on Windows terminals, where `alt+,` and `ctrl+click` may not arrive as sent.

## Rename symbol (decided after a grill; built)
- **What the user sees:** `f2` in the editor (palette "Language: Rename Symbol"; `f2` in the explorer still renames a file) asks the server `textDocument/prepareRename`; no symbol there is a message and no prompt. Otherwise the name prompt (`components/popups/name_prompt.rs`) opens on the server's placeholder (or the word), `enter` sends `textDocument/rename`, and the answer is applied. A server without `prepareProvider` skips the first question and starts from the word under the cursor. An empty name stays in the prompt; the same name says "unchanged" and asks nothing.
- **All or nothing** (`app/rename/apply.rs`). (1) Everything is checked before anything is written: only text edits are supported, so a `createFile`, `renameFile` or `deleteFile` refuses the rename naming the operation (renaming a Rust module moves its file, so that fails for now); every URI must be a `file:` URI under the project root after canonicalizing (a symlink out of the project is outside; a symlink inside is written through); a file must exist, be writable and, when it is not open, be valid UTF-8; the text at every edit range must equal the old name (`r#name` counts), otherwise the server worked from another text; edits must not overlap; an open document's buffer version must equal the version it had when the server was asked (a keystroke while the server works aborts, naming the file). (2) Files that are not open are read into the same rope as an open document, the server's UTF-16 positions become bytes with the one conversion the editor uses (`lsp::from_lsp`), the edits are made from the end, and the text (line endings and BOM as they were) is written to a temporary file beside the original (`buffer::StagedWrite`). (3) The temporary files are put in place one by one; a failure here can still tear, and the sticky error names the files already changed (no rollback: it would need a backup of every file and could fail itself). (4) Open documents get their edits last, each as one undo step (`Editor::replace_ranges` through `Workspace::edit_document`, which finds a tab of the document whether or not it is in front), the cursor kept on its text, and they are left unsaved: a dirty open file is fine, since the server's positions are against its buffer. The notification says how many places in how many files changed and how many open files are unsaved.
- **Why the content check and not mtime and size:** the client has no earlier mtime for a file it never read, and a rename keeps the byte count, so size misses exactly the edit it makes. The text at each range is exact and needs no baseline.
- **Waits:** `prepareRename` 10 s, `rename` 60 s (a server may have much to look through); nothing blocks input while the server works, the version check is what keeps a typed key from being overwritten.
- **Not done:** `renameFile` (module renames), through the drag and drop move (`explorer/drag.rs`) so open tabs follow; saving the open files for the user; using `StagedWrite` for the root save and the quit prompt's save.

## Code actions (decided after a grill; built)
- **What the user sees:** `ctrl+.` (legacy terminals: `alt+enter`; palette "Language: Code Action") asks the server (`textDocument/codeAction`) for the cursor or the selection, with the problems that touch it as the context (so a quick fix such as "import `HashMap`" is offered). The answer is a box like the definition list (`components/popups/action_list.rs`): a row for each action with its kind, numbers 1 to 9, up and down, `enter` or a number does it, `esc` or a click outside closes. Actions the server marks disabled are left out; no actions is a message, no box.
- **Picking:** an action with neither edit nor command is first completed with `codeAction/resolve` (only the resolved action names the files it changes). Its edit is made, then its command is run through `workspace/executeCommand`.
- **The edit** goes through the applier of rename (`app/rename/apply.rs`, `Check::Action`): only text edits (a file operation refuses the action), only files under the project root, writable, no overlapping edits, an open document's version equal to what it was when the server was asked. There is no check of the old text (an import is an insertion at an empty range), and files that are not open are never written: they are opened first. Edits are one undo step per document and the documents are left unsaved.
- **Files the edit names that are not open:** the server's positions for them come from what it read from disk, and the client has nothing to compare (no version was sent, an insertion has no old text). So they are opened as tabs, the server is told with `didOpen`, and `textDocument/codeAction` is asked again for the same range and problems, against the text the server now has. The tab the user was in stays in front, and a notification names the tabs opened. An edit that names only open files is applied at once, with no second round (the common case).
- **Finding the picked action in the second answer:** the same kind and exact title; with several, the one whose edit names the same files; exactly one is done (resolved first if it needs it). None, or several left, shows the second list again with "the actions changed after opening the files; pick again". Never a guess. **At most one re-ask:** if the second edit names yet another closed file, it is opened too and the list is shown again, never a third request. Tabs opened this way stay open.
- **Not done:** a lightbulb mark in the gutter for a line that has actions; disabled actions shown with their reason; `workspace/applyEdit` requests a command makes of the client are declined (`protocol/replies.rs`), so a command that works by asking for an edit changes nothing; a result of a command is not shown.

## Format on save (decided after a grill; built)
- **The setting and the commands:** `format_on_save`, a field of a server in `language_servers` (per language: on by default for Rust, off for the others, `"rust": { "format_on_save": false }` turns it off; the global setting of the first version was dropped before it was released), makes a save format the document first with its language server (`textDocument/formatting`; for Rust that is `rustfmt` through `rust-analyzer`, so it follows the project's `rustfmt.toml`; not `cargo fmt`, which rewrites files behind the buffer's back). "Language: Format Document" (`alt+shift+f` in the editor) formats without saving, and "Language: Format Selection" (palette) formats the whole lines the selection touches (`textDocument/rangeFormatting`; without a selection, or with a server that cannot, it says so).
- **Saving becomes asynchronous** only for a document the setting applies to: a file with a server that formats or is starting (`app/formatting.rs`). Everything else, with the setting off, a file with no server, a server that does not format (said once per server), still saves synchronously as before. `ctrl+s`, `save_all` (`ctrl+k a`) and the quit prompt's save share the path; the root save needs none, since it comes after a normal save that has formatted the text already.
- **Bounds:** a server that is starting is waited for at most 10 s from the save (the end of `initialize` is "ready", not the end of indexing: an answer of nothing or an error saves unformatted with that said), an answer at most 2 s. When either runs out, or the server errors or has no formatter, the file is saved as it is and a message says why ("saved a.rs without formatting: the language server was not ready"). A document with a target is found from its file, since the sync only looks at documents while a server runs.
- **A change while the format is in flight:** the version of the text that was sent is compared to the buffer's each step; a difference drops the format and saves what is there now ("the file changed while formatting"), at once, not at the end of the wait.
- **Applying:** the edits that would not change the text are left out (a formatted file adds no undo step and arms nothing, so "save again to overwrite" still works); the rest go through the applier of rename (`Check::Format`: open documents only, one undo step, the cursor kept on its text, and files outside the project are formatted too).
- **Several files:** `save_all` and the quit prompt ask for every format at once (they share the deadlines, which count from each save), each file is written as its answer is applied; none waits for another.
- **Quitting:** `ctrl+q` while a save waits for its formatting waits for the saves (the loop keeps running) and then quits through the normal flow (a file that could not be saved is asked about); a second `ctrl+q` skips the formatting, writes the files as they are and goes on. The quit prompt's save does the same: it quits when its saves are done.
- **The status bar** says `formatting…` while a format or a save that formats waits. **`esc` while a quit waits** gives up the quit only: the saves already running go on, nothing is skipped, and the editor stays open (closing the quit prompt does the same for the quit that follows its saves).

## New settings reach existing files (decided after a grill; built)
- **The problem:** `settings.json` is copied from the default once, so a setting a new version adds is missing from every file that exists, and the user had to add it by hand or reset the file. The one exception to "trcode never writes `settings.json`": it adds missing entries, and never changes or removes anything in the file (`settings/update.rs`).
- **What is added:** live entries with the default value and the comments the default file has, inserted after the last entry: the `//` lines directly above the key in `defaults/settings.jsonc` plus its `"key": value,` line, taken from there as they are (the one source of truth; a setting whose default spans several lines needs `text` in the table). A comma is added after the last entry when it has none, a `}` that shares its line with the last entry moves to a line of its own, and an empty `{}` gets the entries inside. A written value is the user's value from then on: a later change of a default does not reach it.
- **Which are new:** a table (`INTRODUCED`) gives the version that introduced each setting, and `state.json` keeps `settings_version`, the newest version whose settings were added. A start, and every reload, adds the settings introduced after it that the file does not have, and stores the newest version of the table. A setting the user deleted stays deleted: it was introduced before the stored version. A file with no stored version (every file from before this) counts as 0.6.1, so only settings introduced after it are added; a stored version newer than the table (a downgrade) adds nothing and is not lowered. No settings file adds nothing and remembers nothing; the version is remembered once the file exists. "Reset" of `state.json` keeps the version, or deleted settings would come back.
- **When it refuses:** the file does not parse (the usual warning says what is wrong); it is not one `{ ... }`; a comment follows the last entry (the entries would go between); the file is open in a tab with unsaved edits; the file or its folder cannot be written (a config folder managed by Nix). Then nothing is written, the stored version is not advanced (the next start or reload tries again), the defaults still apply, and an error says "settings.json was not updated with the new settings (reason); the defaults are used". The text is written to a temporary file and put in place (`buffer::StagedWrite`), and what was made must read back as settings with every new key in it, or nothing is written.
- **Reload:** our own write is made before the files are read and before the change is noted as applied, and the second pass finds nothing to add, so it does not loop; a refused write does not trigger a reload.
- **The guard:** a test fails when a key of `defaults/settings.jsonc` is not in the table (it would never reach existing files), when a table key is not in the file, when a version of the table is not a version, or when a setting newer than 0.6.1 has no extractable block. A new setting therefore needs: the default file entry with its comment, a table entry with the version of the release that ships it, and the field in `Settings`.

## Git (decided after a grill; not built)

- **Core first, UI by plugin later.** The core owns the status model, the passive marks and every git action; the Git pane (lists, stage, commit, branch picker) is an official plugin that comes after the plugin runtime and only reads the model and calls actions. The passive marks stay in the core because they paint core components (explorer rows, tab titles, status bar) that a plugin cannot reach: `git.*` theme slots, status letter, folders take the strongest status inside them, branch in the status bar.
- **The `git` command line, not a library.** The user's own `git`, so config, hooks, credential helpers and the ssh agent just work. Status is `git status --porcelain=v2 -z`. Read-only calls run with `GIT_OPTIONAL_LOCKS=0` and `--no-optional-locks`: a background refresh must not take `index.lock` and fail the user's `git commit` in the terminal pane. Only the actions write.
- **Actions are registry actions with typed arguments** (`git.stage(path)`, `git.commit(message)`, `git.switch_branch(name)`). The action validates its arguments (a path outside the repository, an empty message: a typed error), not the caller. Palette and keys collect arguments with core prompts; the plugin passes them directly. This needs arguments on the Action registry, which plugins need anyway. Safety lives in one place: discard asks first, a failed hook shows its output, remote operations can be cancelled, credentials go through `GIT_ASKPASS` and are never stored.
- **Slow repositories.** A refresh (debounced `Event::FilesChanged`, and after our own actions) keeps the old marks while it runs, times out after 5 s, runs one at a time, and backs off (the next waits twice the last duration). It never switches the feature off for the session.
- **Scope of the first version.** The repository root is `git rev-parse --show-toplevel` from the project root; the project may be a subfolder of it and paths are translated. Several repositories under the project: off, with one info message. Submodules are one modified folder. Linked worktrees are not handled.

## Git pane with buttons (decided after a grill; not built)

Goal: from the Git pane, stage files, commit, fetch, pull and push with buttons. It spans the core (the actions), the plugin API (1.4) and the Git plugin (`Tim-kieboom/trcode-plugin-git`, release 0.2.0).

- **Composed views (plugin API 1.4, additive).** A new `ViewNode::Column` of parts, top to bottom: `Buttons` (a row of labelled buttons, each with an id, shown dim when disabled), the existing `Rows` list (it takes the space that is left and scrolls), and an `Input` line (the commit message) under it. A click on a button is a new message `ToPlugin::Press { button }` (the click on a row stays `Click { row }`); a click in the input places the cursor. Only the input part can take raw keys, as today (`esc` is the editor's). Old plugins and the old single-node views keep working; the golden JSON tests get a 1.4 shape. A plugin that is older than the editor never sees the new nodes.
- **Actions in the core (todo steps 4 and 5, first slice), each validating its own arguments, with typed errors.** `git.stage_all`, `git.unstage_all`, `git.commit(message)` (an empty or whitespace message is refused; `git commit -m`; a hook that fails gives its output), `git.fetch`, `git.pull`, `git.push`. A plugin asks for them as `git_commit`, `git_fetch`... requests and the capability `git.write` covers the local ones; `git.remote` is a new capability for fetch, pull and push, because they reach the network (the user is asked once, as for the others, and a plugin that has only `git.write` is refused with `denied`).
- **Remote commands.** The user's own `git`, off the UI thread, `GIT_TERMINAL_PROMPT=0` and no askpass: a command that needs a password or passphrase fails at once with git's own text, shown as a sticky error that says to authenticate once in the terminal pane or to set up a credential helper or an ssh agent (credential helpers and ssh agents work as they do on the command line). No secret ever passes through the editor. A remote command has no 5 s read timeout: it runs until it ends, is cancelled (`esc` while it runs kills the child process) or reaches 5 minutes. One remote command at a time; the buttons of the others are disabled while one runs, and the pane says what runs (`pushing...`). When it ends the status is read again at once.
- **Pull is `git pull --ff-only`.** It only moves forward. When the branches have diverged it refuses with git's text and changes nothing, so a button never starts a merge or a rebase with conflicts (that is done in the terminal, where merge or rebase is chosen). **Push** with no upstream runs `git push -u <remote> <branch>` when there is exactly one remote, and otherwise says there are several and which. Force push does not exist. Ahead and behind counts come from `status --branch` (already read) and show next to the branch, and on the buttons (`Pull (2)`, `Push (1)`).
- **Commit box.** One line (the existing `Input` node): enter or the Commit button commits, with `git commit -m`. The Commit button is disabled while nothing is staged. A multi-line message is a follow-up (a text area node and `ctrl+enter`).
- **Adding files.** Stage and unstage a file stay `s` and `u` on a row (and a click on the letter of the row is not new: a click selects, `enter` opens). New: `Stage All` and `Unstage All` buttons, and the keys `a` (stage all) and `A` (unstage all).
- **Buttons have keys too.** Commit `c` (moves to the commit box), Fetch `f`, Pull `p`, Push `P`; every button is also an action in the palette, `git: Push`, so a keyboard-only user is not left out.
- **Not in this slice.** Discard changes, branch switching and creating, a multi-line message, force push, conflict resolution, a diff view.

- **Step 1, built** (plugin API 1.4): `WireView::Buttons` (buttons with an id, a label and `enabled`), `WireView::Column` (parts one under the other: a buttons row and a line to type into take a row each, `Lines` as many as it has, `Rows` and `List` share what is left, the odd row going to the first; parts that do not fit are dropped from the bottom; a column in a column is not drawn), and `active` on `Input` (written only when it is false, so the 1.3 shape is unchanged; a line that is the whole view is always active). `ToPlugin::Press { button }` for an enabled button and `ToPlugin::InputClick { column }` (the character of the text under the click) are new; a click on a row is still `Click { row }`, now in whichever part is `Rows`. The editor decides where a click goes with `ViewNode::leaves`, `button_at` and `input_column_at` in `trcode-ui`, the same functions the drawing uses. The keys are the plugin's text only while the column has an active line (`ViewNode::is_input`); otherwise they are the plugin's declared keys as before. Buttons are drawn as ` Label ` in the slots `bar.tab.active` and `bar.tab.inactive`, which every theme has. The golden JSON tests pin the 1.4 shapes. A 1.3 editor reads a column as an unknown view (a line that says so), as the other additive changes do.
- **Step 2, built** (still plugin API 1.4): the requests `git_stage_all`, `git_unstage_all` and `git_commit { message }` (all need `git.write`) and the refusal `nothing_staged`. In the core, `Git::stage_all` (`git add -A`), `Git::unstage_all` (`git reset -q`, which works before the first commit) and `Git::commit`: a message that is empty after trimming, or has a NUL, is a `BadArgument` before git runs; it then asks `git diff --cached --quiet` and a clean index is `ActionError::NothingStaged` (a commit that has nothing staged but changed files says so, it does not commit them); the message is passed as the argument of `-m`, so whatever it looks like is only a message; a failure (a hook that says no, no name or email) is `Failed` with up to 12 lines (600 characters) of what git and the hook wrote to stderr and stdout, joined with ` / `, and nothing is committed. What git shows is read again at once after each, like stage and unstage. Not done: a commit during a merge with nothing else staged (refused as nothing staged), amend, signing with a passphrase prompt.
- **Step 3, built** (still plugin API 1.4): the capability `git.remote` (`Capability::GitRemote`, asked about once like the others, with the words "uses the network"), the requests `git_fetch`, `git_pull`, `git_push` and `git_cancel`, and the refusals `busy`, `cancelled` and `auth_required { detail }`. In the core (`src/git/remote.rs`, `Git::run_remote`): `fetch`, `pull` (`--ff-only`) and `push` (a branch with no upstream is pushed with `-u` to the one remote; none, several or a detached `HEAD` is a `BadArgument` that says why). They never ask for a secret: `GIT_TERMINAL_PROMPT=0`, empty `GIT_ASKPASS` and `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE=never`, and on Unix the child gets a session of its own (`setsid`), so `ssh` cannot open `/dev/tty` of the editor; what git prints when it wanted a secret (`terminal prompts disabled`, `Authentication failed`, `Permission denied (publickey)`, `Host key verification failed`...) is `ActionError::NeedsAuthentication` and the sticky error ends with the hint to authenticate once in the terminal pane or to set up a credential helper or an ssh agent. A command runs until it ends, is stopped (the child and everything it started are killed, as a group on Unix) or reaches five minutes. One at a time (`busy`). `esc` in the pane of the plugin that started it stops it (and goes no further), and so does `git_cancel` (answered `done` whether one ran or not); the stopped one is answered `cancelled` and is not shown as an error. Every failure of a change to the repository that is git's own (a hook that says no, a pull that cannot fast-forward, no network) is shown as a sticky error by the editor, because a plugin has no way to show a message; a refusal of an argument (`bad_argument`, `nothing_staged`) is for the plugin to show. After each command what git shows is read again at once, so ahead and behind (already in the summary) follow. Not done: progress lines from git, a choice of remote, force push.
- **Step 4, built** (the Git plugin 0.2.0, `abi = "1.4"`, capabilities `git.read`, `git.write`, `git.remote`, `editor.open`): a column of the buttons (Commit, Stage All, Unstage All, Fetch, Pull (n), Push (n), and Cancel while a command runs), the files, and the message line. Only what can be done now is enabled (Commit needs something staged, Pull an upstream, Push a commit to push or a branch with no upstream, Stage All something unstaged); during a fetch, pull or push the other remote buttons wait and the branch row says `main · pushing…`. Keys: `a`, `shift+a`, `c`, `f`, `p`, `shift+p` (and `esc` stops a running command in the editor). The commit button with no message moves to the message; `enter` in the message commits; a refused commit keeps the text. The files part is unchanged. **A row of buttons wraps** (host side, in `trcode-ui`: `button_cells(buttons, width)` gives line, offset and width; a column gives the buttons the lines they need) because the default Explorer bar is 30 columns and the six buttons are about 52 wide. The real WebAssembly module runs in the editor's tests now (the rustup toolchain has `rust-lld`; the Nix one the project was started on did not): `real_plugin_tests` has the buttons, stage all with `a` then `c`, a message and `enter` committing, and `shift+p` pushing to a bare repository.
- **Releasing 0.2.0 without cutting 0.1.0 off.** The index may list a plugin more than once, once for each version (format 1 is unchanged: the ids need not be unique), and an editor offers, for each id, the newest version it can run (`Index::entry`: the newest version that is `Ready` for it, else the newest, so the list says what is needed); the list has one row for each id. So `plugins/index.json` lists Git 0.1.0 (plugin API 1.3) and 0.2.0 (1.4): an editor with API 1.4 offers 0.2.0 as an update to 0.1.0, and one with 1.3 still installs 0.1.0. Editors 0.7.x, which look at the first entry of an id only for installing, show two rows for Git (0.1.0 installable, 0.2.0 greyed with what it needs) and install 0.1.0 from either; they are not offered the update. The 0.2.0 entry is the hash GitHub gives for the uploaded `git.wasm`.

## Git marks (built)

- **Status model** (`src/git/`): `Status` (branch, entries) read by `git status --porcelain=v2 --branch -z`; `Marks` is built from it once and answers "what shows beside this path": a color (`git.untracked`, `git.added`, `git.modified`, `git.deleted`, `git.conflict`, strongest wins on a folder) and a letter. A path is looked up under the repository root as it is, and by where it really is (`canonical`) when it is spelled another way.
- **Gutter** (`git/lines.rs`, `git/diff.rs`): the base is the **index** (`git show :0:path`), not `HEAD`, so a staged change loses its bar (like Zed). Only the hash of each line is kept; the diff is Myers on those hashes after the common start and end are cut off, capped at 500 edits (past it the stretch is one modified hunk). It is worked out in `prepare` for documents on screen, keyed by (buffer version, base number). A file git has no entry for (not in the index) is asked with `git check-ignore`: ignored means no marks, otherwise every line is new. When the index text is asked again: the first time, and when the file's `Change` in the status differs from the one it was asked in (so a `git add` in the terminal is noticed at the next refresh).
- **Not done:** a click on a bar, `HEAD` as the base, a letter for folders.

## Plugin runtime (decided after a grill; the first slice is built)

This replaces the earlier plan of Lua plugins.

- **Language: WebAssembly, run by `wasmi`** (a pure-Rust interpreter with fuel metering; small and quick to build, and fast enough for a pane that redraws on a key). The author we design for is the one who builds and distributes a plugin (a checksum, any language that compiles to WASM, a hard sandbox), not the one who edits a script in a config folder. Luau can be added later as a second language on the same message API. LuaJIT is out: its compiled traces do not call the instruction hook, so no budget holds.
- **Interface: core WASM and a message ABI**, not WIT components (which need `wasmtime`, several MB and long builds, and a component model that is still changing). The host gives a few functions (`send`, `log`, `alloc`); messages are JSON structs from a small `plugin-api` crate that authors depend on. Bindings for other languages are written by hand when someone needs them.
- **Compatibility.** JSON (not `postcard` or `bincode`: they are positional, so an added field or variant would break every shipped plugin and the abi version would have to change on every addition). Additive changes only within an abi major version; the manifest has an `abi` version; a plugin whose major is newer than the host's is refused with a message. Every enum in `plugin-api` has an `Unknown` catch-all (`#[serde(other)]`) and is `#[non_exhaustive]`, so an old plugin ignores a variant it does not know; from the plugin, an unknown `ViewNode` type is dropped or drawn as a placeholder and an unknown action name is a typed error the plugin can read. Neither crashes the plugin or the host.
- **Threads and snapshots.** The runtime is on a thread of its own, so a slow or hung plugin never freezes the editor (and the keys that disable it). It reads immutable snapshots and sends back declarative `ViewNode` trees and action calls (see "Components, layout, theme, plugins").
- **Pushed summary, pulled pages.** An event carries only `{generation, branch, counts}`; the plugin pulls the rows it draws (`git.entries(section, offset, limit)`). A pull is always answered from the latest status and the reply is stamped with its generation; the plugin starts over when the stamp changed. To do when the API is written: make the check hard to forget (a page object that remembers its generation).
- **Budget and failure.** A fuel budget for each callback: the default is about 50 times the slowest callback of the Git viewer on a 10 000-entry status, a plugin may ask for more in its manifest up to a cap the host sets, and loading has a larger one. A memory limit and API calls that never block are the other two caps. A trap cannot be resumed, so the instance is thrown away and a fresh one is started and sent the current summary; three traps within 60 s disable the plugin, and the counter resets after a quiet minute. A restart shows one notification naming the plugin; a disable stays in the Plugins window with its reason.
- **Capabilities.** The manifest lists what the plugin uses (`git.read`, `git.write`, `editor.open`, `notify`, ...). The host checks every call against it and refuses the rest with a typed error (the check is in the host, never in the plugin). The list is shown and asked once, when the user enables the plugin in the Plugins window. Official plugins declare like any other.
- **Keys.** Both models: named actions declared in the manifest (default keys, a `when` context) go through the user's keymap file and the palette, so they can be rebound; raw key events only reach a plugin inside a text-input node it returned (a commit message box), and `esc` or a keymap action the host keeps for itself always ends it, so a plugin cannot trap the keyboard.
- **`trcode-plugin-api`** (`crates/plugin-api`): the one place that says what a message looks like, for the editor and for authors: `ToPlugin` (`start`, `refresh`), `FromPlugin` (`set_view`), `WireView` (`lines`, `list`), `WireLine`, `Abi` (`1.0`) with `Abi::check`, `MAX_MESSAGE`, and `to_bytes` and `from_bytes` that enforce the size. Enums are `#[non_exhaustive]` with an `Unknown` catch-all and structs are `#[non_exhaustive]` with a `new`, so adding to them is additive for authors' code too. `Abi::check`: the same major version and a minor that is not later is accepted; a later minor or major is refused ("update TerminalCode"); an earlier major is refused too ("ask its author for an update"), since a major bump means something old plugins rely on is gone. The shapes of ABI 1.0 are written out byte for byte in the crate's tests and must never change; a change that breaks one needs a new major.
- **Built so far** (`src/plugins/`): the runtime thread, the instance with fuel and a memory limit, the trap policy, the messages `start`, `refresh` and `set_view`, and the app drawing a plugin's view. The host gives a plugin only `host.send`; there is no `log` yet and nothing loads plugins outside the tests. Fuel numbers are placeholders until the Git viewer is measured.
- **Keys, built** (plugin API 1.1): `Focus::Plugin` with `AppState::focused_plugin`; a plugin pane gets the keyboard from `focus_plugin_view`, a click, and gives it back with `esc` (`focus_editor`, bound in the `plugin` context). Declared actions are `Action::Plugin("name:id")`; their default keys live in the private context `PluginDefaults`, which the editor replaces with the keys of the plugin that has the keyboard (so two plugins can both declare `s`), and the user's `plugin` context wins over it. A plugin's `WireView::Input` turns raw mode on while its pane has the keyboard: `text` for characters and pastes, `key` by name for the rest (a character with `ctrl` or `alt` too), no keymap lookup at all, not even `ctrl+q`; only `esc` is the editor's: it ends the raw mode and sends `input_ended`. If the plugin still shows the input afterwards the keys go to it again and `esc` ends them again, so a plugin can never hold the keyboard against `esc`. Not built: composition of views (an input beside a list), mouse messages, `when` contexts.
- **Git for plugins, built** (plugin API 1.2): the app gives the plugin thread an immutable copy of the state of the repository (`Runtime::publish_git`, with a generation that grows with each state that differs), so reading git costs the UI nothing and needs no round trip. `git.read`: `git_changed` (a summary: branch, commit, upstream, ahead and behind, the number of files in each section) is pushed after start, after a restart and at every change; `git_entries` pulls a page of a section (`staged`, `changes`, `untracked`, `conflicts`; at most 500) and is answered on the plugin thread from the latest copy with its generation. `git.write`: `git_stage` and `git_unstage` go from the plugin thread (which checks the capability) to the app, which runs the core action in the background; the arguments are checked by the action (`src/git/actions.rs`), not by the caller, and refusals are typed (`denied` naming the capability, `no_repository`, `bad_argument`, `failed` with git's words). Questions and answers wait in a queue behind what the app sends, so no plugin can keep the others waiting. Capabilities are passed when a plugin is loaded; the manifest and the prompt come with step 5.
- **Manifest, consent and the list, built.** A plugin is a folder `<config>/plugins/<id>/` with `manifest.toml` and its module. The editor refuses a manifest it cannot honour with a typed message that says what to do (see `ManifestError`): API newer than the editor's or an earlier major, `requires` newer than this editor, an unknown capability (a plugin built for a later API asks for names this version lacks), duplicate or malformed action ids, an id that is not the folder name, a module path that leaves the folder. **Consent** is a decision per plugin in `state.json`: `enabled` and the capability names the user was shown; the question is asked when a plugin is turned on and has no record, or asks for more than the record has; `y` is the only yes, and a no is remembered so the next start does not ask again. What the plugin may do is exactly the capabilities in its manifest; a plugin that asks for less than it was allowed needs no question. The decision lives in `state.json`, not in `settings.json`, because turning a plugin on is a decision with a question, which has to go through the editor (a name in a settings file would run code without it). A reset of the saved state forgets the decisions. The list ("Plugins: Manage") reuses the menu popup: `[x] Name version` and a status; `enter` toggles.
- **The Git pane, built** (its own repository, <https://github.com/Tim-kieboom/trcode-plugin-git>; it was `plugins/git` here; plugin API 1.3): a plugin written in Rust against `trcode-plugin-api`: the `Plugin` trait (messages in, messages out, no WebAssembly in it, so it is tested natively) and `export_plugin!` for the module. It holds the summary and the pages it has, keeps a selected file counted over the groups in the order they are shown, and sends the editor a `rows` view: the branch, then a window of 80 places (headings and files) around the selected file, so the size of a message does not grow with the repository; it asks for 100 files at a time, only for the pages the window touches. `rows` is a view of lines made of spans, with a selected row: the editor highlights it (`explorer.selected`, or the inactive one when the pane lacks the keyboard) and scrolls so it is on screen, centred once it is past the first half (`rows_scroll`); a click on a row is `click` with the line's number. `open_file` (capability `editor.open`) is checked like any path (inside the repository, a file) and leaves the keyboard where it is. The module needs `wasm32-unknown-unknown` and a linker; CI builds it and runs the tests that load it.
- **First slice, with no git:** the host and a plugin pane, with two test plugins built from Rust in CI: one draws a line (seen in a `TestBackend` render), one loops forever (trapped, restarted, disabled after three traps, while the editor keeps taking keys).
- **Module and crate.** The runtime is in the root package (it needs the Action registry and `AppState`, which are there), and `src/git/` stays a module in it rather than `crates/git`: nothing below would use it, and its layering is already clean (`grep -rE "crate::(app|components|event)" src/git` finds nothing).

## Plugin manager and official plugins (decided after a grill; built except the plugin's own repo)

- **Trust.** Option B: the editor fetches an index of the official plugins from a file in a repository of its own, `Tim-kieboom/trcode-plugins` (`index.json` on `main`; it began as `plugins/index.json` in the editor's repository and moved out so that a plugin can be listed or updated without touching or releasing the editor), downloads the module the index names and checks its SHA-256 against the index. The anchor is therefore GitHub (whoever controls that repo controls what the editor installs), which is accepted while the author owns the repo; the checksum catches a damaged or swapped download, not a repo that was taken over. Rejected: **pinning the hashes in the editor's release** (safe, but a plugin could then only change with the editor, so a runtime fetch would add nothing over embedding the module), **embedding the modules** in the editor (the same), and **a signed index now** (the work of a key and its guarding for one author; signing is the far-future item that also allows plugins from anyone). Fetching is HTTPS only, from `github.com`, `raw.githubusercontent.com`, `objects.githubusercontent.com` and `release-assets.githubusercontent.com` (release assets redirect there; GitHub moved them from the second to the last).
- **Trust on first use.** The first install of a version pins its module hash in `.trust.json` in the plugins folder (`src/plugins/trust.rs`); the same version later must come with the same hash, or the install is refused with "a published version must not change" and nothing is written. Removing a plugin keeps its pins; a pin goes only by editing the file, and a file that does not read stops installs instead of being replaced. This catches a release asset or index entry rewritten after you installed it; it does not help the very first install, and a module is re-checked each time it is read to run: for a folder the installer made it must have the hash in `installed.json` and the pin for its version, or the plugin is not started ("its module is not the one that was installed"). A plugin folder the user made is not checked.
- **Extra sources.** `plugin_sources` in `settings.json` lists more indexes (the same format, the same download rules: https and the hosts of GitHub only), so a plugin can be offered without a commit to the editor's repository. Their entries are marked with the source's URL (`Entry.source`, set by the editor and never read from an index, so an index cannot claim to be the official one or another source); the list shows `owner/repo` of the source in the tag and detail. A name is taken by the first index that lists it, official first, then the sources in the order of the setting: a source cannot replace an official plugin, and what was left out is said under the list. Installing from a source asks first (source, address, the first 16 digits of the module hash) and the first install pins the hash like any other; `installed.json` keeps the source, and an update is offered only from that same source. A source's index is not cached, so offline its plugins vanish from the list (the installed ones stay).
- **The index.** `"format": 1`; an entry has `id`, `name`, `description`, `version`, the URLs of the manifest and of the module, the module's `sha256`, `abi` (the plugin API it needs) and `requires` (the oldest TerminalCode). A newer format major says to update TerminalCode; unknown fields are ignored. The editor embeds no copy of the index: the last fetch is cached in the config folder, so offline the last list is shown, and with no fetch yet the list is empty (installing needs the network anyway). A `plugin_sources` entry equal to the official address is not fetched twice. An entry that needs a newer editor or API is shown greyed with what it needs. A failed fetch shows the cache marked offline with the reason; a hash that does not match stops the install and writes nothing. An entry is published by hand: the plugin repo's release workflow prints the entry (version, URLs, hash) and it is pasted into `index.json` of trcode-plugins (a token that edits the thing the editor trusts is not worth it for one plugin that rarely releases).
- **The plugin repos.** The Git plugin is a public repo of its own. It depends on `trcode-plugin-api` by git tag of the editor repo (the crate is not published to crates.io until the API settles: it went from 1.0 to 1.3 in a week), and its tag is bumped by hand when the API changes.
- **Installing.** `ureq` with `rustls`, 30 seconds, `esc` cancels (a thread, progress as events). Folders the manager makes have an `installed.json` (the entry, the version, the hash of the module); only those are updated or removed. A folder without it is local (a plugin the user built) and is never touched: installing over it says to remove it by hand. An update is downloaded next to the old folder, checked and swapped in by a rename, so a failure keeps the old plugin; a running plugin is asked about first. Remove deletes the folder and keeps what the user decided in `state.json`, so a reinstall comes back as it was.
- **Placement.** Fixed places: the Explorer's bar, the bottom bar, or a popup. The choice is in `state.json` and is applied to the parsed layout in memory; `layout.ron` is never written (a text edit of a file the user edits by hand is fragile). A plugin that `layout.ron` names itself stays where the file puts it. A placement only takes effect while the plugin is installed and on. A layout without such a bar offers only the popup. The popup is a new surface: a plugin view over the editor, opened by the action `plugins: open <name>` (palette, and a binding of the user's), closed by `esc` unless the plugin is typing into a line; it has the keyboard like the palette.
- **The flow and its stops.** Download, then the question about what the plugin may do (already built), then where to put it. A stop at the download writes nothing. A no to the question leaves the plugin installed and off. A cancelled placement leaves it installed and on and "not placed", until "Place..." in the manager asks again (it also moves a placed plugin).
- **What was built, and where it differs from the plan.** `src/plugins/index.rs` (the index and the cache `plugin-index.json` in the config folder), `src/plugins/install.rs` (fetch, install, remove; the policy of hosts), `src/app/plugin_list.rs` and `src/components/popups/plugin_list.rs` (the manager popup), `src/app/plugin_place.rs` (placement and the plugin popup). Differences: the download runs on a thread that is itself run by a thread, so `esc` is heard while a server says nothing (the inner one ends at the 30 s limit and writes nowhere); every redirect is checked against the policy (`ureq` follows none by itself); an update is built in `plugins/.staging/<id>/` (a folder whose name starts with a dot is never listed) and swapped in by renames; the manager popup is a list with keys (`enter`, `i`, `x`, `space`, `p`, `r`) and not a menu, and questions (what it may do, where, update, remove) are the existing popups; the question about where is not asked while other plugins wait for their answer to what they may do. Placement: `AppState` keeps the layout of the file (`base_layout`) and the effective one (`layout`) that has the views of placed plugins added at the end of the bar that holds the Explorer or the Terminal (`LayoutTree::with_view_in_bar_of`); a `Pane(view: Explorer)` is not a bar, so a layout of panes offers only the popup. The popup is `Popup::PluginView`: it keeps the focus on the plugin, `esc` closes it (after the plugin's line to type into, if it has one), a click outside closes it.
- **The Git plugin's repository.** `Tim-kieboom/trcode-plugin-git` depends on `trcode-plugin-api` by the tag `plugin-api-1.3` of this repository (a new tag when the API changes, bumped by hand in its `Cargo.toml` with `abi` in its manifest). Its CI runs its own tests, builds the module, and runs this repository's `real_plugin` tests against it (`TRCODE_PLUGIN_GIT_DIR`); this repository's CI does the same against the plugin's `main`. A tag `vX.Y.Z` there makes a release (the module and the manifest) and prints the entry for `plugins/index.json` in the run's summary. The index entry is pasted by hand.

## Save as root (Linux; built)
- A save that fails because the user may not write the file (`FileError::ReadOnly`) or its folder (`PermissionDenied` from the atomic write) opens a masked password prompt ("Save /etc/hosts as root"); a file the app locked on purpose (the standard library source opened by go to definition) never does. The text goes to a temporary file as the user (mode 0600, removed on every path), then one `sudo -S -k -p '' sh -c SCRIPT sh SRC DST` call makes a temporary name next to the target (`mktemp`), copies the owner and mode of the file it replaces (a new file gets `root:root 0644`) and moves it over the target; the password goes to `sudo` on its input, never in an argument, and `-k` makes `sudo` ask every time. It runs on a thread of its own (a wrong password makes `sudo` pause) and answers with `Event::RootSaved`. A wrong password empties the prompt and asks again until `Esc`; "not in the sudoers" and "no sudo" are told once and then every save says so at once; keys do nothing while `sudo` works; text typed after the save began keeps the buffer modified (the saved version is compared). The password lives in `Secret`, which overwrites its bytes when dropped.
- A file that is read-only to the user is locked against edits, so nothing could be saved: the palette action `File: Edit Anyway (save as root)` lifts that lock for the buffer (`Buffer::unlock_for_root`; the file permissions do not lock it again), and the save then asks for the password. The quit prompt does not ask for a password: it says to close it and save with `ctrl+s`.
- Not done: `doas`, a root save over a bind-mounted file (the inode is replaced), and saving all modified files as root in one go.

## Settings, remembered state and reset (decided after a grill)
- What is hidden between runs is remembered in `state.json` in the config folder: one global file, not per project, and not
  `settings.json`, which the user edits by hand and trcode only ever adds new settings to (see "New settings" below). The remembered state wins over the layout
  file's "starts hidden"; the layout value is only the starting point when nothing is remembered for that pane.
- It is written on every toggle (a crash or a closed window loses nothing), through a temp file and a rename. A failed
  write (read-only config folder, a managed `~/.config`) is one notification per run, not one per toggle. A corrupt file
  is ignored with a notification and the layout is used.
- Reset to default exists for the theme, keymap, layout and settings files and for the remembered state: confirm, then
  the old file is renamed to `NAME.bak` (an older backup is overwritten) and the built-in default is written.
- `sequence_timeout`: when a chord times out its first key is dropped, not sent on; the status bar shows the pending keys
  while it waits. 200 to 5000 ms, `0` waits forever.
- `notification_seconds` is the lifetime of plain messages only; errors stay until dismissed.
- `hide_ignored` hides ignored entries in the explorer only. Correction after building it: the file finder and project
  search already leave ignored files out (they follow `.gitignore`), so the explorer is the only place that lists
  them; with `hide_ignored` on, an ignored file can only be opened by naming it on the command line or from a
  language server jump.

## Built-in themes are templates (decided after a grill)
- The four built-in themes (Dark, Light, SolDark, SolLight) are not themes to choose. They are templates for
  `Theme: New...`, which copies one to `themes/NAME.toml`, chooses it and opens it. `Theme: Choose` and the `theme` setting
  only know the files in `themes/`.
- One built-in theme stays as a hidden fallback (the dark one, never listed, called "default" in messages): the floor under
  every installed theme (a theme file may leave slots out), what the UI shows with no `theme` (`null`, the default) and what it
  falls back to when a theme file is missing or broken. The `theme.toml` overlay still goes over whatever is chosen.
- `"theme": "dark"` (or `light`, `soldark`, `sollight`) with no theme file of that name is an unknown theme like any other:
  one warning at startup, naming the theme and `Theme: New...`, and the fallback. Nothing is written into the config folder
  for the user and nothing guesses which template was meant.
- `Config: Open Theme` opens the theme in use; with the fallback in use there is no file, so it says how to make a theme.
  It no longer creates `themes/dark.toml`. Removed: the `Theme: Dark/Light/SolDark/SolLight` palette entries (`UseTheme`).
