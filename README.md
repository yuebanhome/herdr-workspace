# herdr-reporadar

A compact, read-only Git change radar for AI-driven multi-repository Herdr
workspaces.

RepoRadar recursively discovers repositories below the current workspace,
keeps clean repositories visible but muted, highlights repositories with local
changes, and expands dirty repositories to show their changed files. It does
not browse, diff, edit, stage, or commit code.

## Development

Requirements: Rust 1.88 or newer, Git, and Herdr 0.7 or newer.

```bash
cargo build --release
herdr plugin link "$PWD" --enabled
herdr plugin action invoke herdr-reporadar.open
```

Invoke the action again to focus the existing pane. The pane refreshes every
two seconds and immediately when it regains focus. Use arrow keys or `j`/`k` to
move, Enter or Space to expand a dirty repository, `r` to refresh, and `q` to
close the pane. Repository rows also respond to mouse clicks and the wheel.

To bind the action in Herdr's `config.toml`:

```toml
[[keys.command]]
key = "prefix+g"
type = "plugin_action"
command = "herdr-reporadar.open"
description = "Open RepoRadar"
```

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## License

MIT
