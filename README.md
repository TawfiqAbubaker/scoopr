# Scoopr

Scoopr is a small Rust plugin for [Herdr](https://herdr.dev) that turns terminal scrollback into a fast, fuzzy-selectable list. Select a URL, path, command, log line, hash, quote, or other useful text, then copy it or insert it back into the pane that opened Scoopr.

## Requirements

- Herdr 0.7.0 or newer
- Rust and Cargo (only needed to build or install from source)
- A terminal that supports OSC 52 clipboard requests for the `Tab` copy action

Scoopr supports macOS and Linux. It does not require tmux.

## Install from a checkout

From a checkout of the repository, build the release binary and link it into Herdr:

```sh
cargo build --release
herdr plugin link .
herdr plugin action list --plugin scoopr
```

After publishing, replace the checkout step with your repository's clone URL.

The repository's Herdr manifest builds the release binary and runs that binary at runtime, so Cargo is not needed in Herdr's runtime `PATH`.

To update a linked checkout after pulling changes:

```sh
cargo build --release
herdr plugin link .
```

## Configure a keybinding

Plugin manifests do not define global keybindings. Add a binding to `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+shift+c"
type = "plugin_action"
command = "scoopr.open"
description = "Scoop text from current tab"
```

Reload Herdr after changing the configuration:

```sh
herdr server reload-config
```

You can also invoke the action directly:

```sh
herdr plugin action invoke scoopr.open
```

To have Scoopr add its recommended binding automatically, run its setup action:

```sh
herdr plugin action invoke scoopr.setup
herdr server reload-config
```

Setup is idempotent. It creates a `config.toml.scoopr.bak` backup before editing, refuses to overwrite an existing `prefix+shift+c` binding, and marks the block so it can be removed later:

```sh
herdr plugin action invoke scoopr.remove-setup
herdr server reload-config
```

## Use the picker

Scoopr opens with the scrollback from every pane in the current tab. It keeps the originating pane, so the result is returned to the right place even though the picker runs in a popup.

- `Tab` — copy the selected result through OSC 52
- `Enter` — insert the selected result into the originating pane
- `Ctrl-S` — cycle through tab, workspace, and server scopes
- `Ctrl-F` — choose a candidate filter
- `Up` / `Down` — move through matches
- `Left` / `Right` — pan across long candidates
- `Esc` or `Ctrl-C` — cancel
- Type to search; `Backspace` deletes a character

Filters are `all`, `word`, `line`, `path`, `url`, `hash`, and `quote`. Search treats straight and typographic quotation marks as equivalent while preserving the original text in copied and inserted results.

## Development

Run the formatter, tests, and checks locally:

```sh
cargo fmt -- --check
cargo test
cargo check
```

The `extract` subcommand is useful for inspecting candidate extraction without opening a Herdr popup:

```sh
printf 'visit https://example.com\n' | cargo run -- extract
```

## Permissions and data handling

Scoopr reads pane output through the Herdr CLI when the picker is opened. It may read panes in the selected tab, workspace, or server scope. Selected text is either sent back to the originating pane or emitted as an OSC 52 clipboard request. Scoopr does not make network requests or persist pane content.

## License

Scoopr is released under the MIT License. See [LICENSE](LICENSE).

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) before submitting a change. Please do not include terminal logs containing credentials, tokens, customer data, or other private information.
