<img width="1916" height="821" alt="ChatGPT Image Sep 5, 2026, 08_08_41 PM" src="https://github.com/user-attachments/assets/ab1bf1d8-69c4-4992-a8cf-da63d9defb3d" />

<div align="center">

# Scoopr

### Fastest Herdr plugin to find & copy anything in your terminal history.

Scoopr is a fuzzy-searchable scrollback picker for [Herdr](https://herdr.dev). Find a URL, path, command, log line, hash, quote, or anything else you remember seeing — then copy it or send it straight back to the pane that opened the picker.

Heavily inspired by [extrakto](https://github.com/laktos/extrakto), Scoopr brings that fast, fuzzy terminal-text workflow from tmux to Herdr.

<img alt="Rust" src="https://img.shields.io/badge/Rust-self--contained_crate-orange?logo=rust&logoColor=white">
<img alt="Herdr" src="https://img.shields.io/badge/Herdr-%E2%89%A5%200.7-5865a3">
<img alt="Platforms" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-supported-2ea44f">
<img alt="License" src="https://img.shields.io/badge/license-MIT-blue">

<br><br>

<code>prefix + shift + c</code> &nbsp;→&nbsp; search &nbsp;→&nbsp; <code>Enter</code> to insert

</div>

## Why Scoopr?

Terminal scrollback is full of useful things that are awkward to recover: a command from ten minutes ago, a long URL, a temporary path, a commit hash, or a line from a build log. Scoopr turns that scrollback into a small, focused command palette.

- **Fuzzy search** across terminal content, with ranking tuned for useful matches.
- **Structured filters** for words, lines, paths, URLs, hashes, and quotes.
- **Three scopes** — current space, current tab, or the whole server.
- **Direct insertion** back into the originating pane, or clipboard copy via OSC 52.
- **Popup-native** and ephemeral, with no database and no background service.

## Install

### Published plugin

```sh
herdr plugin install TawfiqAbubaker/scoopr -y
herdr plugin action invoke scoopr.setup
herdr server reload-config
```

`setup` adds the recommended `prefix+shift+c` shortcut and creates Scoopr’s starter settings file.

To update:

```sh
herdr plugin uninstall scoopr
herdr plugin install TawfiqAbubaker/scoopr -y
```

### From a checkout

```sh
git clone https://github.com/TawfiqAbubaker/scoopr.git
cd scoopr
cargo build --release
herdr plugin link .
```

The Herdr manifest builds and runs the release binary for you. Cargo is only needed when building or updating the linked plugin.

## Configure

Scoopr uses two configuration layers:

1. Herdr’s global config defines the keybinding.
2. Scoopr’s plugin config controls the picker.

### Keybinding

Plugin manifests do not define global keybindings. Add this to `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+shift+c"
type = "plugin_action"
command = "scoopr.open"
description = "Scoop text from current tab"
```

Then reload Herdr:

```sh
herdr server reload-config
```

You can also launch Scoopr directly:

```sh
herdr plugin action invoke scoopr.open
```

### Picker settings

Find Scoopr’s config directory with:

```sh
herdr plugin config-dir scoopr
```

Edit its `config.toml`:

```toml
[behavior]
default_scope = "tab"       # tab, space, server
default_filter = "all"      # all, word, line, path, url, hash, quote

[keys]
copy = "tab"
insert = "enter"
cycle_scope = "ctrl+s"
open_filter = "ctrl+f"
cancel = "esc"

[popup]
width = "80%"
height = "80%"
```

Missing settings use these defaults. Shortcut values support named keys such as `tab`, `enter`, `esc`, `backspace`, `up`, `down`, `left`, and `right`, plus single characters with `ctrl+`, `alt+`, or `shift+` modifiers.

The setup action is idempotent. It backs up the Herdr config before editing, refuses to overwrite an existing `prefix+shift+c` binding, and marks its managed block so it can be removed later:

```sh
herdr plugin action invoke scoopr.remove-setup
herdr server reload-config
```

## Use the picker

Open Scoopr from any Herdr pane. It starts at the current **tab** and preserves the originating pane, so an inserted result goes back to the right place.

| Key | Action |
| --- | --- |
| `Tab` | Copy the selected result through OSC 52 |
| `Enter` | Insert the selected result into the originating pane |
| `Ctrl-S` | Cycle through `tab → space → server` |
| `Ctrl-F` | Choose a candidate filter |
| `↑` / `↓` | Move through matches |
| `←` / `→` | Pan across long candidates |
| `Esc` / `Ctrl-C` | Cancel |
| Type / `Backspace` | Search and edit the query |

When the current space contains only one tab, the space scope is redundant and the cycle becomes `tab → server`.

Available filters are `all`, `word`, `line`, `path`, `url`, `hash`, and `quote`. Search treats straight and typographic quotation marks as equivalent while preserving the original text when copying or inserting.

## Development

Run the local checks:

```sh
cargo fmt -- --check
cargo test
cargo check
```

Inspect candidate extraction without opening a popup:

```sh
printf 'visit https://example.com\n' | cargo run -- extract
```

## Permissions and data handling

Scoopr reads pane output through the Herdr CLI when the picker opens. Depending on the selected scope, it may read panes in the current tab, space, or server. Selected text is either sent back to the originating pane or emitted as an OSC 52 clipboard request.

Scoopr does not make network requests, persist pane content, or run a background process.

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) before submitting changes. Please do not include terminal logs containing credentials, tokens, customer data, or other private information.

## License

Scoopr is released under the MIT License. See [LICENSE](LICENSE).
