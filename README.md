# TerminalCode

A lightweight, keyboard-driven CLI IDE built with Rust + Ratatui.

## ✨ Features

- **Multi-window interface**: Text editor, file tree, fuzzy finder, shell, file creator
- **Syntax highlighting**: Syntect-powered with theme support
- **File operations**: Load/save, create files/dirs, change project root
- **Integrated terminal**: Cross-platform shell execution
- **Live fuzzy search**: Ctrl+P file finder (max depth 3)

## 🚀 Quick Start
```
git clone <repo>
cd terminalcode
cargo run
```


1. Opens in current directory
2. `*` appears in header when unsaved changes
3. Use **Ctrl+P** → type → **Enter** to open files
4. **Alt+1** for file tree navigation

## 🏗️ Architecture

```
Session (owns Terminal + WindowStack)
↓
WindowKind (enum delegation via macro)
↓ LookupBar TextEditor FileTree Shell etc.
↓ WindowsControl trait
↓ Cursor + TextBuffer + ScrollableView
```


**Key Design Patterns:**
- `Rc<RefCell<>>` SharedContext (avoids double-borrow panics)
- `TextBuffer` enum (Single/Multi-line unification)
- `impl_window_for_enum!` macro (~200 LOC saved per window)
- `SessionEvent` pipeline (input → window → session)

## 📁 File Structure
```
src/
├── session.rs # Main event loop + window stack
├── context.rs # SharedContext + FileContext
├── window/ # All UI components
│ ├── mod.rs # WindowKind enum + traits
│ ├── text_editor.rs # Syntax-highlighted editor
│ ├── lookup_bar.rs # Ctrl+P fuzzy finder
│ ├── filetree.rs # Hierarchical browser
│ └── command_prompt.rs # Integrated shell
├── utils/ # Shared primitives
│ ├── cursor.rs # Universal cursor type
│ ├── text_buffer.rs # Single/Multi-line unification
│ ├── scrollable_view.rs # Smart viewport mgmt
│ └── syntaxer.rs # Syntect integration
└── key_controller.rs # InputEvent → SessionEvent
```

## 🛠️ Tech Stack

| Crate | Purpose |
|-------|---------|
| [Ratatui](https://ratatui.rs/) | Terminal UI |
| [Crossterm](https://crates.io/crates/crossterm) | Input handling |
| [Syntect](https://crates.io/crates/syntect) | Syntax highlighting |
| [fuzzy-matcher](https://crates.io/crates/fuzzy-matcher) | Live search |
| [anyhow](https://crates.io/crates/anyhow) | Error handling |
| [walkdir](https://crates.io/crates/walkdir) | File traversal |

## ⚡ Performance

- **Zero-copy rendering** via viewport slicing
- **Live syntax** (per-keystroke highlighting)
- **Fast fuzzy** (SkimMatcherV2 + depth-limited walks)
---