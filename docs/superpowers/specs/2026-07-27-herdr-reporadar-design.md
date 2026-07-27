# Herdr RepoRadar Design

## Purpose

`herdr-reporadar` is a read-only change radar for AI-driven, multi-repository
workspaces. It answers two questions without turning into a code browser:

1. Which repositories in the current Herdr workspace have local changes?
2. Which files changed in a selected repository?

The plugin does not show diffs, preview files, edit files, stage changes, or
manage commits.

## User Experience

The plugin opens as a narrow split on the right of the active Herdr pane. Its
header shows the current workspace path and repository/change totals.

Repositories are listed alphabetically so their positions remain stable across
refreshes. Each repository row contains:

- a hollow, muted indicator for a clean repository;
- a filled warning indicator for a repository with changes;
- the repository directory name;
- the number of changed files when non-zero;
- the current branch or detached `HEAD` name on a muted second line.

Activating a dirty repository expands it in place and displays changed files
with concise Git status markers such as `M`, `A`, `D`, `R`, `U`, and `?`.
Activating it again collapses the file list. Clean repositories remain visible
but cannot expand into an empty section.

Keyboard controls are `j`/Down and `k`/Up for navigation, Enter/Space for
expand or collapse, `r` for immediate refresh, and `q`/Escape for closing the
pane. Mouse clicks select and toggle repository rows; the wheel scrolls the
list. A short footer exposes only the essential key hints.

## Architecture

The plugin is a Rust binary launched by a `herdr-plugin.toml` pane entrypoint.
It has four internal boundaries:

- `discovery`: recursively finds Git repositories below the workspace root;
- `git`: invokes the system Git executable and parses stable porcelain output;
- `app`: owns selection, expansion, scrolling, refresh timing, and scan state;
- `ui`: renders the compact terminal interface and maps visible rows for mouse
  interaction.

The system Git executable is authoritative for repository semantics. This
avoids libgit2 behavior drift and naturally supports normal repositories,
worktrees, submodules, and repository-specific Git configuration. No
repository-controlled hooks or commands are executed.

The pane derives its root from `HERDR_PLUGIN_CONTEXT_JSON`, falling back to the
current directory for direct execution and tests. A small launcher action opens
the pane to the right without stealing focus. Repeated invocation focuses the
existing RepoRadar pane instead of creating duplicates in the same workspace.

## Discovery And Status Flow

Repository discovery walks directories without following symbolic links. A
directory is a repository when it contains a `.git` directory or `.git` file.
The walker never descends into `.git` internals and honors ignore files to avoid
scanning generated trees such as `node_modules` and `target`. The workspace
root itself is included when it is a repository, and nested repositories remain
independent entries.

For each discovered repository, a bounded worker pool executes a read-only
porcelain status command. The result contains the branch, changed file paths,
and index/worktree status. The dirty count includes staged, unstaged,
untracked, conflicted, and renamed files. Commits ahead of an upstream do not
make an otherwise clean repository dirty.

An initial scan begins immediately. The UI remains responsive while scans run,
then replaces the repository snapshot atomically. Automatic refresh runs every
two seconds, and `r` requests an immediate scan. Only one scan may run at a
time; repeated triggers coalesce. Selection and expanded repositories survive
refreshes by repository path.

## Error Handling

A failed repository status command does not abort the workspace scan. The row
is retained with an error indicator and a short status-unavailable message.
The previous successful snapshot remains visible until a complete new snapshot
arrives. An invalid or unavailable workspace root produces a centered error
message while keeping refresh and quit controls usable.

Git stderr is bounded before display, paths are rendered lossily when they are
not valid UTF-8, and terminal control characters are sanitized before drawing.
The plugin never writes to repositories.

## Packaging

The first release supports macOS and Linux with Herdr 0.7 or newer. The
manifest builds the release binary with Cargo. A pane entrypoint and an
idempotent `open` action are included. Local development uses
`herdr plugin link <path>` after `cargo build --release`.

## Testing And Acceptance

Unit tests cover nested repository discovery, normal and unusual porcelain
records, status aggregation, stable ordering, selection preservation, and
viewport behavior. Renderer tests use Ratatui's test backend at narrow and tall
sizes. Integration tests create temporary Git repositories and verify staged,
unstaged, untracked, deleted, and renamed files through the real Git executable.

The delivery is accepted when:

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test --all-targets` pass;
- the plugin links successfully in Herdr 0.7.5;
- invoking the action opens one right-side RepoRadar pane;
- a multi-repository fixture shows clean repositories muted and dirty
  repositories highlighted with correct counts;
- clicking or activating a dirty repository reveals its changed files;
- changing a fixture repository appears without restarting the pane.

