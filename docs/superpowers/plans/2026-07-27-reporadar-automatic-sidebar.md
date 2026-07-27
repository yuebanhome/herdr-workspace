# RepoRadar Automatic Sidebar Implementation Plan

## Goal

Make RepoRadar a self-healing workspace sidebar in Herdr 0.7.5: one process
per workspace, visible in the active tab, automatically created or moved by
native lifecycle hooks without stealing focus. Ship and fully validate this as
`v0.1.2`.

## 1. Add Structured Host Queries

Extend `src/host.rs` and `src/main.rs` with narrowly scoped JSON commands for
the shell lifecycle layer:

- enumerate workspace IDs and each workspace's active tab;
- inspect workspace and pane-list responses using explicit workspace and tab
  identifiers;
- list exact-title RepoRadar candidates and non-Radar target panes;
- extract pane launch `cwd`, worktree checkout paths, and opened/moved pane
  identifiers;
- verify `pane process-info` output identifies a live `herdr-reporadar`
  process;
- validate workspace, tab, and pane identifiers before they enter command
  arguments or lock paths.

Keep all JSON handling in Rust. Add unit tests for every accepted Herdr response
shape and for malformed, missing, and deceptive data.

## 2. Implement Workspace Reconciliation

Add `scripts/reconcile.sh` as the shared layout implementation. It will accept
an explicit workspace, optional preferred tab and target pane, and a focus
policy. It will:

1. validate identifiers;
2. acquire an atomic session-and-workspace lock under
   `HERDR_PLUGIN_STATE_DIR`;
3. recover dead-owner locks and stop waiting after two seconds;
4. re-query the workspace and active tab after acquiring the lock;
5. verify title-filtered candidates through `pane process-info`;
6. retain a candidate already in the active tab or the lexically smallest
   verified pane, then close only verified duplicates with
   `herdr plugin pane close`;
7. select a live non-Radar target in the active tab;
8. move the keeper with `--no-focus`, or open a new right split rooted at the
   worktree checkout or target pane launch `cwd`;
9. resize the pane and either preserve user focus or focus the pane for the
   manual action.

State races receive at most three attempts, 100 ms apart. A vanished
workspace/tab is a successful no-op. Traps remove only locks owned by the
current reconciler.

## 3. Wire Native Lifecycle Hooks

Add `scripts/auto-open.sh`:

- startup mode parses `workspace list` through the Rust helper and reconciles
  restored workspaces sequentially;
- event mode reconciles only the explicit workspace and tab from Herdr's event
  context;
- malformed or incomplete event context is logged and exits without mutating
  an ambient workspace.

Update `scripts/open.sh` to delegate to the reconciler and request focus.
Register `[[startup]]`, `workspace.created`, `workspace.focused`, and
`tab.focused` in `herdr-plugin.toml`. Do not register pane-close or exit events.

## 4. Build Regression Coverage

Replace the single launcher test with a stateful fake-Herdr integration suite
covering:

- startup enumeration across multiple workspaces;
- create, no-op, and cross-tab move behavior;
- automatic no-focus and manual focus behavior;
- stable root selection from checkout or pane launch `cwd`;
- title collision rejection through process verification;
- deterministic duplicate healing through plugin-safe close;
- simultaneous event convergence on one pane;
- stale-lock recovery, live-lock timeout, and session-scoped lock names;
- target, tab, and workspace disappearance during reconciliation;
- manifest lifecycle registration and shell syntax.

Run formatting, Clippy with warnings denied, all tests, release-package tests,
Actionlint, and RustSec audit. Review the final diff for unsafe ambient state,
unbounded waits, focus theft, duplicate creation, shell injection, and package
omissions; fix every finding and repeat the quality gate.

## 5. Live Herdr Acceptance

Using controlled temporary workspaces and tabs on Herdr 0.7.5, verify:

- a newly focused workspace gains RepoRadar automatically;
- switching tabs moves the same RepoRadar pane ID;
- opening a workspace or tab needs no manual action;
- automatic operations preserve the user's focused pane;
- rapid focus events converge on one process per workspace;
- closing with `q` stays closed until the next focus event, then repairs;
- the current multi-repository rendering and clean-repository dimming remain
  intact.

Record the original plugin installation, layouts, and focus before testing and
restore the development link and original focus afterward.

## 6. Release `v0.1.2`

Bump `Cargo.toml`, `Cargo.lock`, `herdr-plugin.toml`, README commands, and
release examples to `0.1.2`. Ensure release packages include every lifecycle
script. Commit and push `main`, create and push annotated tag `v0.1.2`, and
monitor the GitHub workflow until the Release is complete.

Verify the published Release contains Linux x86_64, Linux ARM64, macOS ARM64,
and `SHA256SUMS`; validate checksums, archive contents, and binary
architectures. Perform real macOS ARM64 prebuilt and tagged-source installs,
repeat automatic workspace/tab acceptance in each mode, then restore the local
development link. Persist the final behavior and installation procedure in
Nowledge Mem only after all release checks pass.
