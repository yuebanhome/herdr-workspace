# herdr-reporadar

A compact, read-only Git change radar for AI-driven multi-repository Herdr
workspaces.

RepoRadar recursively discovers repositories below the current workspace,
keeps clean repositories visible but muted, highlights repositories with local
changes, and expands dirty repositories to show their changed files. It does
not browse, diff, edit, stage, or commit code.

## Install From Source

Herdr clones the tagged source and builds it locally. This requires Git and
Rust 1.88 or newer.

```bash
herdr plugin install yuebanhome/herdr-workspace --ref v0.1.2 -y
herdr plugin action invoke herdr-reporadar.open
```

The one-time action initializes the current workspace immediately when Herdr is
already running. After that, RepoRadar opens automatically for every workspace
and follows the active tab without taking focus. Herdr also restores missing
sidebars on server startup and on the next workspace or tab focus. The manual
action remains available to move RepoRadar into the current tab and focus it.

## Install A Prebuilt Bundle

Prebuilt releases do not require Rust. Published targets are:

| System | Target |
| --- | --- |
| Linux x86_64 | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` |
| macOS Apple Silicon | `aarch64-apple-darwin` |

With GitHub CLI on Apple Silicon macOS:

```bash
release_version=v0.1.2
release_target=aarch64-apple-darwin
install_root="${XDG_DATA_HOME:-$HOME/.local/share}/herdr/plugins/reporadar-$release_version"
mkdir -p "$install_root"
gh release download "$release_version" \
  --repo yuebanhome/herdr-workspace \
  --pattern "herdr-reporadar-$release_version-$release_target.tar.gz" \
  --pattern SHA256SUMS \
  --dir "$install_root"
cd "$install_root"
grep "herdr-reporadar-$release_version-$release_target.tar.gz" SHA256SUMS \
  | shasum -a 256 -c -
tar -xzf "herdr-reporadar-$release_version-$release_target.tar.gz"
herdr plugin link "$install_root/herdr-reporadar" --enabled
herdr plugin action invoke herdr-reporadar.open
```

Without GitHub CLI, download the target archive and `SHA256SUMS` from the
[GitHub Release](https://github.com/yuebanhome/herdr-workspace/releases). Put
both files in one directory and set `archive` to the downloaded archive name.
On macOS, verify it with:

```bash
archive=herdr-reporadar-v0.1.2-aarch64-apple-darwin.tar.gz
grep -F "$archive" SHA256SUMS | shasum -a 256 -c -
```

On Linux, use `grep -F "$archive" SHA256SUMS | sha256sum -c -` instead. Then
extract the archive and link the extracted `herdr-reporadar` directory as
shown above.

To upgrade, close the running RepoRadar pane with `q`, download the new version
to a new persistent directory, and link that directory. Run the `open` action
once to initialize the current workspace immediately; later workspace and tab
focus events are automatic. To return to a local development checkout:

```bash
cargo build --release --locked
herdr plugin link "$PWD" --enabled
```

## Development

Requirements: Rust 1.88 or newer, Git, and Herdr 0.7.5 or newer.

```bash
cargo build --release
herdr plugin link "$PWD" --enabled
herdr plugin action invoke herdr-reporadar.open
```

The pane refreshes every two seconds for small workspaces, five seconds above
25 repositories, and ten seconds above 100. Repository discovery runs every 30
seconds and immediately on focus or manual refresh. Use arrow keys or `j`/`k`
to move, Enter or Space to expand a dirty repository, `r` to refresh, and `q`
to close the pane. Repository rows also respond to mouse clicks and the wheel.

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
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
bash tests/release_package.sh
```

## Publishing

Keep the versions in `Cargo.toml` and `herdr-plugin.toml` identical, then tag
the release commit. The tag must be exactly `v` followed by that version.

```bash
git tag -a v0.1.2 -m "RepoRadar v0.1.2"
git push origin v0.1.2
```

The Release workflow validates the tag, runs the quality gate, builds all three
targets, publishes the bundles and `SHA256SUMS`, and can safely be rerun for a
partially failed release.

## License

MIT
