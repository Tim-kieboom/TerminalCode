# Git pane

The changed files of the repository in a pane beside the explorer, as a TerminalCode
plugin: the branch on top, then the files in groups (merge conflicts, staged changes,
changes, untracked files), each with a letter for what happened to it.

| Key | Action |
| --- | --- |
| `up`, `down` | move over the files |
| `enter`, a click | open the file in the editor |
| `s` | stage the file (also an untracked one) |
| `u` | unstage the file |
| `r` | read the files again |
| `esc` | back to the editor |

The keys can be changed in your keymap (`context = "plugin"`, action
`{ plugin = "git:stage" }`, and so on), and every action is in the command palette as
`git: ...`.

It asks to be allowed to read the state of the git repository, to change it (stage
and unstage) and to open files; you are asked when you turn it on.

## Installing it

It is built to WebAssembly and put in the plugins folder of the config folder
(`~/.config/terminalcode/plugins/git/` on Linux):

```sh
rustup target add wasm32-unknown-unknown
cargo build -p trcode-plugin-git --target wasm32-unknown-unknown --release

dir=~/.config/terminalcode/plugins/git
mkdir -p "$dir"
cp plugins/git/manifest.toml "$dir/"
cp target/wasm32-unknown-unknown/release/trcode_plugin_git.wasm "$dir/git.wasm"
```

Then turn it on with "Plugins: Manage" in the command palette, and show its pane by
putting it in the side bar of your `layout.ron`, next to the explorer:

```ron
SideBar(views: [Explorer, Plugin("git")], sides: [Right], border: QuadrantInside, title: Hidden)
```

(`Bar: Next View` and a click on the names at the top switch between the two.)

## Tests

The logic does not know about WebAssembly and is tested like any crate:
`cargo test -p trcode-plugin-git`. The module itself, run by the editor against a real
repository, is tested by `cargo test -p terminal_code real_plugin -- --ignored` after
building it as above.
