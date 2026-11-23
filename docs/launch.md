# launch.toml: run configurations

`.trcode/launch.toml` is the file in a project that tells TerminalCode what **Run** (`f5`) starts. It is optional. A Cargo project works without it: every binary and example is found by itself. The file is for changing those, and for adding commands of your own (a dev server, a script, a preview of an HTML file).

The file belongs to the project and can be checked in, so a team shares it.

## Creating and opening it

Open the command palette and run **Run: Open Launch File**. If the project has no file yet, one is made from a template (`.trcode/` too) with examples that are all commented out, so a freshly made file changes nothing. A file that exists is opened as it is. Saving it works the list out again at once.

## Running

| Key | What it does |
|---|---|
| `f5` | Run the active configuration (saves all modified files first) |
| `ctrl+k r` | Choose the configuration from a picker, and run it |

The active configuration is remembered per project and shown in the status bar as `▶ name`. With none chosen, `f5` runs the first one that can be run. If the chosen one has disappeared (a renamed binary), Run says so and does not pick another.

Each configuration has its own terminal tab, which is reused. A configuration that is still running asks before it is stopped and run again.

## The list of configurations

The picker shows, in this order:

1. The **detected targets**: the binaries and examples `cargo metadata` finds, grouped by package. Tests and benches are not listed.
2. The **commands and opens** of the file.
3. Entries of the file for a target that no longer exists. They stay in the list, greyed out, with "target not found".

When cargo is missing or the manifest does not parse, the list holds only the file's entries, with a note that says why.

## Entries

The file is a list of `[[run]]` tables. There are three kinds, told apart by which key they have.

### 1. Change a detected target: `target`

```toml
[[run]]
target = "trcode:bin:trcode"
args = ["--recent"]
env = { RUST_LOG = "debug" }
cwd = "demo"
release = true
```

`target` is `package:kind:name`, where kind is `bin` or `example` (for example `trcode:bin:trcode` or `trcode-lsp:example:demo`). The picker's second column shows the package.

Only the fields that are named change; the rest stay as detected.

| Key | Meaning |
|---|---|
| `name` | The label in the picker |
| `args` | Arguments of the program (a list of strings) |
| `env` | Environment variables, a table of strings |
| `cwd` | The folder to run in |
| `release` | Build with `--release` |

A target can be changed once. If two entries name the same one, the first counts and the second is reported as a problem.

### 2. A command of your own: `command`

```toml
[[run]]
name = "Dev server"
command = "npm"
args = ["run", "dev"]
cwd = "web"
env = { PORT = "3000" }
```

`name` (unique) and `command` are required. `args`, `env` and `cwd` are optional. `release` and `target` are not allowed here.

Nothing goes through a shell: `command` is a program looked up on `PATH` and started directly with `args`. If you want a shell, ask for one:

```toml
[[run]]
name = "Build and serve"
command = "sh"
args = ["-c", "make && ./serve"]
```

There is no build step: the command starts straight away in its terminal tab.

### 3. Open a file: `open`

```toml
[[run]]
name = "Preview"
open = "${file}"
```

Opens a file with the system's program for it (a browser for `.html`), with **no terminal tab**. `${file}` is the file of the active tab; `open = "docs/index.html"` is a fixed path from the project.

`open` needs a `name` and cannot be combined with `target`, `command`, `args`, `env`, `cwd` or `release`. It starts the system opener (`xdg-open`, `open`, or the Windows file handler) and does not wait for it. If the active tab is untitled, nothing runs: save it first.

## Placeholders

These can be used in `args`, in the **values** of `env`, in `cwd`, and in `open`:

| Placeholder | Stands for |
|---|---|
| `${file}` | The whole path of the file in the active tab |
| `${workspaceFolder}` | The project folder |

Nothing else with `${...}` is allowed. An unknown one is a problem in that entry. If `${file}` is used while the active tab is untitled, the run stops with "not running: the open file is untitled; save it first". `${workspaceFolder}` always works.

A relative `cwd` is from the project folder, so `${workspaceFolder}/web` and `web` are the same.

## Problems in the file

A mistake in one entry is reported in a notification (`.trcode/launch.toml: entry 2: ...`) and **that entry is left out**; the rest still count. A file that is not valid TOML gives one problem and no entries. Unknown keys are refused, which catches typos like `arg =`.

Things that are reported:

- an entry with neither `target`, `command` nor `open`
- `target` together with `command`
- `open` with any of `target`, `command`, `args`, `env`, `cwd`, `release`
- a `command` or `open` without a `name`, or with an empty value
- `release` on a command
- two commands or opens with the same `name`
- an unknown `${...}` placeholder
- a `target` that is not `package:kind:name`

## Trust

A project that you clone can bring its own `launch.toml`, and it decides what starts when you press `f5`. So anything the file decides (a command, an `open`, or a target the file changes, even by one field) only runs in a folder you trust. The first time, TerminalCode asks "Trust this folder?". A yes goes on with the run and is remembered; a no drops it.

A target as cargo found it, untouched by the file, needs no trust: it is what `cargo run` would do in your own project.

## Debugging

`ctrl+k g` starts the active configuration under the debugger, and the launch arguments (`args`, `env`, `cwd`, with the placeholders filled in) are used there too. An `open` entry cannot be debugged.

## Running a target: what happens

1. Modified files that have a path are saved (untitled buffers are left alone). If one cannot be saved, the run stops.
2. A detected target is built with `cargo build` (with `--release` and the target's `required-features` when they apply). The compiler's errors are shown in the terminal tab. A failed build starts nothing.
3. The built executable is started directly in the terminal tab, in the project folder or the entry's `cwd`, with the `args` and `env` of the entry. For a target, `CARGO_MANIFEST_DIR` and `CARGO_PKG_NAME` are set, as `cargo run` does.

A command of the file skips step 2.

## A full example

```toml
# Run the editor with debug logging and a file to open
[[run]]
target = "trcode:bin:trcode"
args = ["${file}"]
env = { RUST_LOG = "debug" }

# A bin that should always be built optimised
[[run]]
target = "bench-tool:bin:bench"
release = true

# A front-end dev server
[[run]]
name = "Web dev server"
command = "npm"
args = ["run", "dev"]
cwd = "web"

# View the current HTML file in a browser
[[run]]
name = "Preview"
open = "${file}"
```

See also "Run configurations and a debugger" in `design.md` for the reasoning behind these choices.
