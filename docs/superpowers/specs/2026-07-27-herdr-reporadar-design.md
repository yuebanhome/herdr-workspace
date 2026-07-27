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
list. The footer reports repository totals without filling the compact pane
with usage instructions.

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

The pane derives its root from the action context's `workspace_cwd`, not the
focused pane's foreground directory. This keeps the radar anchored to the whole
workspace when an agent changes directory inside one repository. It falls back
to the pane directory and then the current directory only when Herdr does not
provide workspace context. A small launcher action opens the pane to the right
without stealing focus. Repeated invocation focuses the existing RepoRadar pane
instead of creating duplicates in the same workspace.

## Discovery And Status Flow

Repository discovery walks directories without following symbolic links. A
directory is a repository when it contains a `.git` directory or `.git` file.
The walker never descends into `.git` internals or common generated trees such
as `node_modules` and `target`. Parent ignore files do not hide independent
repositories from the workspace radar. The workspace root itself is included
when it is a repository, and nested repositories remain independent entries.

For each discovered repository, a bounded worker pool executes a read-only
porcelain status command with a ten-second process timeout. The result contains
the branch, changed file paths, and index/worktree status. Untracked directories
are expanded to individual files. The dirty count includes staged, unstaged,
untracked, conflicted, and renamed files. Commits ahead of an upstream do not
make an otherwise clean repository dirty. A timed-out repository remains in the
list with a status-unavailable error while other repositories finish normally.

An initial discovery begins immediately. The UI remains responsive while scans
run, then replaces the repository snapshot atomically. Normal refreshes reuse
the known repository paths; full recursive discovery runs on startup, on manual
refresh or focus, and every thirty seconds. The status interval is adaptive:
two seconds through 25 repositories, five seconds through 100, and ten seconds
above 100. Timer ticks never queue a second scan behind a slow one, while manual
requests coalesce into one pending full discovery. Selection and expanded
repositories survive refreshes by repository path.

Repository leaf names remain compact when unique. Duplicate leaf names are
replaced by their workspace-relative paths so every row stays identifiable.

## Error Handling

A failed repository status command does not abort the workspace scan. The row
is retained with an error indicator and a short status-unavailable message.
Directory traversal errors produce a visible partial-scan warning instead of
silently hiding repositories. The previous successful snapshot remains visible
until a complete new snapshot arrives. An invalid or unavailable workspace root
produces a centered error message while keeping refresh and quit controls usable.

Git stderr is bounded before display, paths are rendered lossily when they are
not valid UTF-8, and terminal control characters are sanitized before drawing.
The plugin never writes to repositories.

## Packaging

The first release supports macOS and Linux with Herdr 0.7 or newer. The
manifest builds the release binary with Cargo. A pane entrypoint and an
idempotent `open` action are included. Local development uses
`herdr plugin link <path>` after `cargo build --release`.

## Testing And Acceptance

Unit tests cover nested repository discovery, discovery warnings, normal and
unusual porcelain records, Git timeout behavior, status aggregation, stable
ordering, duplicate-name disambiguation, workspace-root precedence, selection
preservation, and viewport behavior. Renderer tests use Ratatui's test backend
at narrow and tall sizes. Integration tests create temporary Git repositories
and verify staged, unstaged, nested untracked, deleted, and renamed files through
the real Git executable.

The delivery is accepted when:

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
  `cargo test --all-targets` pass;
- the plugin links successfully in Herdr 0.7.5;
- invoking the action opens one right-side RepoRadar pane;
- a multi-repository fixture shows clean repositories muted and dirty
  repositories highlighted with correct counts;
- clicking or activating a dirty repository reveals its changed files;
- changing a fixture repository appears without restarting the pane.
