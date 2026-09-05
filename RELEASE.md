# Release checklist

1. Update the version in `Cargo.toml` and `herdr-plugin.toml` together.
2. Run `cargo fmt -- --check`, `cargo test`, and `cargo check`.
3. Build a release binary with `cargo build --release` on each supported platform you intend to publish.
4. Verify the manifest, action list, popup opening, copy, insert, scope switching, cancellation, and the `extract` command.
5. Review the diff for secrets, local paths, generated files, and terminal logs.
6. Create a tagged release matching the package version, for example `v0.1.0`.
7. Publish the repository URL and install instructions in the release notes.
