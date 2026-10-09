# TODO — path to 0.1.0

See `design.md` for decisions. Milestones are ordered; each ends in something runnable.

## M0 — Foundation
- [x] Decide what to keep from the old code (git history) vs. delete; clean the working tree
- [x] New crate skeleton, module layout from design.md, `thiserror` error enums per module
- [x] CI: GitHub Actions matrix (Linux + Windows): build, `cargo test`, `clippy`, `fmt --check`
- [x] Tokio runtime, `Event` enum, single-owner `AppState`, input channel + bulk channel
- [x] Ratatui terminal setup/teardown with panic-safe restore
- [x] Dirty-flag render loop capped at ~60 fps
- [x] Headless test harness: feed `Event`s, assert state

## M1 — Text core
- [x] `Position` (line, grapheme) and conversions to byte/char (`unicode-segmentation`)
- [x] `Buffer` on `ropey`, `version`, line endings (LF/CRLF) detection
- [x] `Edit` struct and `Buffer::apply`
- [x] Transactions + linear undo/redo (stores inverse edits and selection state)
- [x] Typing-burst coalescing for normal-editor undo (runs of typing, backspace or delete form one step; moves, newline, selection edits and undo/redo break a run; no time-based break yet)
- [x] Selections (list of ranges, one used), `desired_column`
- [x] Property tests: apply + invert == identity, undo/redo, position round-trip
- [x] Property tests for grapheme movement over emoji/CJK/combining marks
- [x] File open/save (atomic write, encoding: UTF-8 only in 0.1.0, clear error otherwise)

## M2 — Action + keymap
- [x] `Action` enum with serializable args and a plugin hole (grows as features land)
- [x] `KeyChord`, crossterm normalization, string parsing
- [x] Kitty keyboard protocol probe (`terminal::init`) + legacy fallback keymap layer
- [x] Layered keymap (contexts, defaults -> legacy -> user file, unbind support)
- [x] Default keymap + fallbacks written; IDE prefix chord chosen (`ctrl+g`, bound in M6 with the terminal pane)
- [x] Key-sequence resolver with timeout (needed for vim and prefix chords)

## M3 — Editor UI
- [x] Editor view: gutter, scrolling, selection highlight, cursor, display-width/tab handling in the view layer only
- [x] Normal-IDE editing: typing, delete, line/document motion, shift-select, undo/redo, save
- [x] Word motion (ctrl/alt+arrows), delete word, select all
- [x] Copy/cut/paste: internal register (charwise or linewise) mirrored to the system clipboard; whole-line copy/cut with no selection; linewise paste goes above the cursor line; bracketed paste is a separate path
- [x] Auto-indent on Enter (copies the leading whitespace left of the cursor, verbatim), Tab/Shift+Tab with the indent style detected from the file (tabs, or 2-4 spaces; fallback 4 spaces), block indent/outdent of selected lines, page up/down (+ shift to select)
- [x] Mouse: click, drag, wheel (shift+wheel and horizontal wheel scroll sideways), shift+click extends, double click selects a word run, triple click the line; on by default, `alt+m` (`toggle_mouse`) switches it at runtime
- [ ] Mouse follow-ups: auto-scroll while dragging past the edge (needs a repeating timer), middle-click paste, right-click menu in the editor (the explorer has one, and the menu component is generic), click in the gutter selects the line, wheel goes to the pane under the pointer once there are several panes, `mouse` setting in `settings.json`, Shift+click may be taken by the terminal itself (kitty uses shift for its own selection) so consider alt+click for extending
- [x] Status bar (file, modified marker, line/column, messages)
- [x] Tabs and splits: documents are shared by id, every pane has its own tab strip, splits (right/down) show the same document in a new pane with shared buffer, shared undo and cursors that follow the other pane's edits; ctrl+n new file, ctrl+w close tab (asks before discarding the last view of a modified document, pane closes with its last tab; closing the very last tab leaves an empty editor area and the app keeps running), ctrl+pageup/pagedown switch tabs, ctrl+k right/down split, alt+h/j/k/l and ctrl+k o move focus, click focuses a pane or switches a tab, middle-click on a tab closes it (same unsaved-changes confirmation, any tab, focus stays), the wheel scrolls the pane under the pointer, several files on the command line open as tabs
- [x] Quit with unsaved changes asks first: a modal lists every modified document; `ctrl+s` saves and `ctrl+d` discards the selected one (up/down to choose), `s` saves all and quits, `d` discards all and quits, `esc` cancels; each entry has a rule under it; a file that cannot be saved (no path, I/O error) stays listed with the error and can still be discarded; the app quits once no document is left to decide on
- [ ] Tabs/panes follow-ups: open a path that is already open by switching to it, overflow markers in a crowded tab bar, close other/all tabs, move a tab between panes, resize panes (drag the divider), close pane without closing its tabs one by one, reopen closed tab, save all, per-pane tab history
- [x] Clipboard (arboard, OSC 52 write-only fallback, one-time notice when the system clipboard is unreadable)
- [x] Per-component frames in the layout file: border type, title text, title alignment and title/border theme slots (`Framed(component: X, frame: (...))`)
- [x] `[background]` in theme.toml: color + opacity, blended with the terminal background (OSC 11 query) or transparent so the terminal's own opacity/blur shows
- [x] Notifications: info messages (saved, mouse on, hints) time out after 4 s, errors are sticky until the next key press or click (the key is still handled; the error a key causes survives that key); stacked bottom right above the status bar, newest at the bottom, same message replaces itself, 20 kept (the oldest info message goes first, so a flood of them cannot push out a sticky error; if not all fit on screen the errors are drawn first), long text wraps; `notification.info` / `notification.error` theme slots; the status bar no longer carries messages
- [ ] Notification follow-ups: `[+N more]` when the stack does not fit, a message history (palette entry), per-notification dismiss and click-to-dismiss, lifetime as a setting
- [x] Hide and show parts of the screen: the explorer, the status bar and each plugin view are `Hideable` (a hidden one takes no space, its neighbors in the split share it, and it keeps its state); `ctrl+k b` toggles the explorer (showing it gives it the keyboard, hiding it takes the keyboard back to the editor), `ctrl+k s` the status bar, `{ toggle_plugin_view = "id" }` a plugin's view by its id in a keymap, and "View: Toggle Explorer" / "View: Toggle Status Bar" are in the palette; `ctrl+b` on a hidden explorer shows and focuses it
- [ ] Hide follow-ups: remember what is hidden between runs (settings.json), a palette entry per plugin view, hide the terminal pane once it exists, give the layout file a way to start with something hidden, a hidden pane could leave a thin strip to click
- [x] Command palette (`f1`, `alt+p`; not `ctrl+shift+p`, which kitty and Windows Terminal keep for themselves): fuzzy search over hand-written action titles ("File: Save"), actions with an argument expanded per value ("Pane: Focus Left"), the bound keys shown beside each entry, enter runs it, esc closes; every action that has a title is listed explicitly in `Action::palette_actions`
- [ ] Palette follow-ups: argument prompts (open file path, go to line), save as for untitled files, recently used first, mouse selection, paste into the query

## M4 — Explorer + search
- [x] Project tree, lazy load, gitignore-aware: root = first directory argument or the working directory; `ctrl+b` gives it the keyboard (`esc` or a click in the editor takes it back); up/down/home/end/pageup/pagedown move, right/left expand, collapse and step in/out, enter opens a file (focus stays in the tree) or toggles a directory, `f5` refreshes, click and wheel work; indent guides (a line under each directory's chevron down its children, theme slot `explorer.guide`); opening an already open file switches to its tab; `Explorer` keymap context
- [ ] Explorer follow-ups: highlight the file of the active tab, reveal the active file, dim ignored files instead of hiding them (setting), hidden-file toggle, `alt+h` from the leftmost pane into the explorer, read directories off the UI thread, symlink loops
- [x] File watcher: directories the explorer has open and directories that hold open files are watched (not recursively, so hidden `target/` trees cost nothing; 150 ms debounce) and arrive as `Event::FilesChanged`; the explorer refreshes itself; an open document with no edits reloads (cursors clamped, undo history dropped, info message), one with edits keeps its text and gets a sticky error, and saving over a file that changed on disk asks first (save again to overwrite; any other action in between cancels it; also in the quit prompt); ; a report refreshes only the open directories it names (a 30,000-file directory refreshes in 71 ms, was 8 s), closed directories are read again when opened, and a watched directory that was deleted or replaced is watched again
- [ ] Watcher follow-ups: a document reload keeps undo history (diff-based edit) instead of dropping it, detect a changed file when the editor regains focus or the tab is switched to (for a missed event), show "changed on disk" in the tab, watch limits (inotify) fall back to polling, project-wide watch for new files in collapsed directories, rename/move handling
- [x] Delete (`delete` in the explorer, or the palette): to the OS trash (`trash` crate); unsaved tabs of it are asked about first and tabs of deleted files close; when the trash fails a prompt offers permanent delete (`y` confirms, Enter and every other key cancel)
- [x] Create: `a` (file) and `shift+a` (folder) in the explorer, or the palette, ask for a name in the selected folder (beside a selected file); `a/b/c.rs` makes the folders; `create_new`/`create_dir` so a collision is an error and the prompt stays open; a new file opens in the editor, a new folder is revealed
- [x] Rename: `r` / `f2` in the explorer (or the palette) asks for the new name, starting from the old one; `renamore::rename_exclusive_fallback` (atomic no-replace, an `exists()` check first where the filesystem lacks it); open tabs, unsaved ones included, follow the new path, the watcher re-syncs, a late `FilesChanged` for the old path is harmless (tested, no ignore window)
- [x] Move: `m` in the explorer (or the palette) asks for the new path from the project root, starting from the current one; an existing folder means "into it" (like `mv`), missing folders are made, nothing is replaced, a folder cannot move into itself, nothing outside the project; tabs follow as for rename
- [x] Explorer right-click menu: (done: `shift+f10`, right click, clicking items, outside-click rules, the generic `components/menu.rs`, captured path, vanished-path check; items show their keys) right-click selects the row under the pointer first (empty space targets the project root, where rename and delete are left out), then lists new file, new folder, rename, and delete last behind a separator; no move entry (see drag and drop); up/down/enter/esc work too, every item acts on the path captured when the menu opened (not the selection at click time, so a watcher reload cannot retarget it; one up-front existence check, a vanished path gives an error notification and does nothing, so create never recreates a deleted folder), built as a generic popup (items = label, action, key hint looked up in the keymap, separator flag) so an editor menu can reuse it, a left click outside dismisses it and acts on what it hit, a right click outside reopens it there, wheel and esc dismiss, and `shift+f10` opens it on the selected row (anchored under that row)
- [ ] Explorer drag and drop: drag a file or folder onto a folder to move it (same rules as `m`: collisions refused, open tabs follow); needs a drop-target highlight and a drag threshold so a click never moves anything
- [ ] Rename that only changes case on a case-insensitive filesystem (NTFS, macOS): the exclusive rename sees the name as taken
- [x] Fuzzy file finder (`ctrl+p`, palette: "File: Go to File"): a thread walks the project with the explorer's rules (gitignore honored, `.git` skipped, other hidden files kept, links to files listed, links to directories not followed) and sends batches over the event channel, so the finder opens at once and ranks what has arrived while you type; a name match beats a scattered path match, shorter paths win ties; only the best 200 are kept; a longer query only re-ranks the files that matched the shorter one; enter opens the file and the editor gets the keyboard; the footer counts files and says "indexing..." until the walk is done; ; file names are matched as text but opened by their real path, so a `\` in a Unix name or a name that is not valid Unicode opens correctly
- [ ] Finder follow-ups: recently opened files first, highlight the matched characters, open in a split, `file:line` queries, a fallback scan when the project has more than ~1M files
- [x] Project-wide text search (`alt+f`, palette: "Search: Find in Project"; `ctrl+shift+f` is kitty's and Windows Terminal's): literal by default, `alt+c` toggles match case (ignored by default), `alt+r` toggles regex; searches as you type (a search waits 150 ms and is dropped if a newer one replaced it); one row per matching line, `path:line  text` with the match marked, in path order, streaming in; up/down/pageup/pagedown choose, enter opens the file with the cursor on the match and the editor gets the keyboard; stops at 5000 results (at most 200 per file) and says so; an invalid regex is explained in the footer; ignored files, `.git` and binary files are skipped; reopening restores the last query and options; uses `grep-searcher` + `grep-regex` (no `rg` needed)
- [x] Find in the open file (`ctrl+f`, palette: "Search: Find in File"): a bar takes the bottom row of the focused pane, every match is highlighted (`editor.match` theme slot) and the current one is selected, the bar shows "3 of 12" (or "no results", or why a regex is invalid); typing searches from where the cursor was, enter/down/`f3` go to the next match and shift+enter/up/shift+`f3` to the previous (both wrap), `alt+c` toggles match case and `alt+r` regex (same meaning as the project search; literal and case-insensitive by default), esc closes and leaves the match selected; a selected word seeds the query, otherwise the last query comes back; capped at 10,000 matches
- [ ] Find follow-ups: replace (and replace all), whole-word toggle, multi-line patterns, search within the selection, keep the bar open while editing and clicking in the text, the match count of other panes showing the same file, a scrollbar-style overview of matches, `ctrl+g` go to line
- [ ] Search follow-ups: replace, search only the open file/folder/selection, include/exclude globs, search unsaved buffers (it reads the disk), keep the results open while editing (a results pane), group results under file headers, next/previous result without reopening, whole-word toggle, multi-line patterns

## M5 — Syntax highlighting
- [x] tree-sitter integration, grammars compiled into the binary: Rust, TOML (`tree-sitter-toml-ng`), JSON, Markdown (`tree-sitter-md`, block and inline grammars), Nix. Binary 8.3 MB → 10.7 MB (release, Linux). If the binary size or the Windows build starts to hurt, drop in this order: TOML, JSON. RON was tried and dropped (its crate pulls in a second, incompatible tree-sitter runtime; see `docs/design.md`). Not done: fenced code blocks in Markdown are not highlighted in their own language, the Windows build is untested
- [x] Parse worker (done; a keystroke costs the UI ≈ 4 µs, the worker answers a 7 MB Rust file in ≈ 0.12 s, the first parse takes ≈ 0.9 s; why the incremental parse still takes 0.12 s is worth a benchmark): at most one parse pending, always started from the newest text (coalescing); any result is accepted and its highlights are mapped forward through the edits made since the text it parsed, so a steady typist on a big file still sees colors; `version` only guards against results arriving out of order. Until a result arrives the old highlights are mapped through each `Edit
- [x] Incremental reparse from `Edit`
- [ ] Smart indent: add a level after an opening token (`{`, `(`, `[`, `:`) and dedent on a closing one, using the syntax tree to skip strings and comments (Enter copies indentation verbatim until then)
- [ ] Highlight theme as data; a light theme (the default one has the `syntax.*` slots; `syntax.*` slots with longest-prefix fallback: `syntax.function.builtin` → `syntax.function` → `syntax`; unthemed captures get no span)

## M6 — Integrated terminal
- [ ] `portable-pty` spawn (Unix + ConPTY), shell selection
- [ ] `alacritty_terminal` grid, render into Ratatui
- [ ] Batched PTY output events, bounded channel, backpressure test (`cat` huge file keeps input responsive)
- [ ] Resize, scrollback, alt screen, bracketed paste, mouse reporting passthrough
- [ ] Multiple terminal tabs
- [ ] Focus handling: all keys to shell except IDE prefix chord

## M7 — Vim layer
- [ ] Mode state machine: normal / insert / visual / visual-line
- [ ] Motions, operators, text objects, counts
- [ ] Registers, `.` repeat, `/` search + `n`/`N`
- [ ] Minimal ex commands: `:w :q :wq :e`
- [ ] Config toggle for vim on/off; mode indicator in the status bar
- [ ] Vim parser tests (table-driven: keys in, edits out)

## M8 — Config + polish
- [ ] `settings.json` (Zed-style: JSON with comments, user file layered over defaults; project override file optional). Settings: theme, shell, vim on/off, indent fallback width/style and an option to turn off indent detection from the file, tab display width, sequence timeout. Needs a JSONC parser (or comment stripping) and typed errors shown as notifications. Keymap stays in `keymap.toml` unless we decide to move it.
- [ ] Mouse: pane focus by click (needs several panes)
- [ ] Error handling pass: every recoverable error surfaces as a notification; no `unwrap` outside tests
- [ ] Performance pass: large file open (10 MB), long lines, fast scroll
- [ ] Benchmarks (`criterion`): parse and highlight on a large file, mapping highlights through edits, buffer edits and undo, rope line access, explorer reload of a big directory, finder and project search on a big tree, render of a full frame; run in CI or on demand to catch regressions
- [ ] Manual test pass on Windows Terminal, one kitty-capable and one non-kitty terminal on Linux

## M9 — Release 0.1.0
- [ ] README (install, keybinds, config), license
- [ ] Release CI: binaries for Linux and Windows (macOS best-effort), checksums
- [ ] Known-limits section (long single-line files, UTF-8 only, no undo tree, no LSP/debugger)
- [ ] Tag `v0.1.0`

## Post-0.1.0 backlog
- [ ] Load tree-sitter grammars at runtime from shared libraries (`.so` / `.dll`), so languages can be added without a rebuild; needs a place to find them, a version check against the `tree-sitter` ABI, and per-platform shipping
- [ ] adding usefull keybinds like (move line `alt+up/down` cursor go back `alt+left/right` multicursor `ctrl+alt+up/down`)
- [ ] LSP client (possibly as a Lua plugin)
- [ ] Lua plugin runtime on top of the Action registry
- [ ] Debugger via DAP (e.g. Rust through `lldb-dap`), possibly as a Lua plugin
- [ ] Multi-cursor, undo tree, visual-block, vim macros, git integration

## Open questions (resolve before the milestone that needs them)
- [ ] Clipboard strategy details (M3)
- [x] Project search implementation: the `grep-searcher` crates, no `rg` dependency (M4)
- [ ] Mouse scope (M8)
