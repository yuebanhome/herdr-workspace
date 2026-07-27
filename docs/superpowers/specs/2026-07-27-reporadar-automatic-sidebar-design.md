# RepoRadar Automatic Sidebar Design

## Problem

RepoRadar currently opens only when its `open` action is invoked. The pane is
owned by one Herdr tab, so it is absent after switching to another workspace or
tab and after opening a new Herdr session whose restored layout has no
RepoRadar pane.

The desired behavior is automatic: every workspace has one RepoRadar process,
and that pane is visible in the workspace's active tab without requiring a
manual action. Automatic reconciliation must not steal focus, duplicate panes,
or multiply repository scanning by the number of tabs.

## Behavioral Invariants

- Each Herdr workspace has at most one live RepoRadar pane.
- The pane lives in the workspace's active tab. Switching tabs moves the
  existing pane instead of starting another scanner.
- A missing pane is created automatically after server startup, workspace
  creation, workspace focus, or tab focus.
- Automatic creation and movement never take focus from the user's pane.
- The manual `open` action retains its current intent: reconcile the pane into
  the active tab and focus it.
- Different Herdr sessions are isolated even when both contain a workspace
  such as `w1`.
- A closed workspace, tab, or target pane is treated as a normal race rather
  than an error to retry indefinitely.
- Disabling the plugin is the persistent opt-out. Pressing `q` is a temporary
  close; the pane remains closed until the workspace or tab is focused again
  or the Herdr server restarts.

## Herdr Lifecycle Integration

The manifest will use Herdr 0.7.5's native plugin lifecycle surface:

| Trigger | Reconciliation scope |
| --- | --- |
| `[[startup]]` | Enumerate every restored workspace and reconcile its active tab |
| `workspace.created` | Reconcile the created workspace |
| `workspace.focused` | Reconcile the focused workspace and its active tab |
| `tab.focused` | Reconcile the owning workspace and move RepoRadar to that tab |
| manual `open` action | Reconcile the context workspace, then focus RepoRadar |

`tab.created` is intentionally not handled. A background tab does not need a
sidebar until it becomes active, while a newly created active tab emits
`tab.focused`. Pane and layout events are also excluded so RepoRadar's own
creation and movement cannot trigger a reconciliation loop.

Startup hooks run only when a Herdr server starts, not when a plugin is linked,
enabled, or installed into an already running server. Installation instructions
will continue to invoke the manual action once so the current workspace is
initialized immediately. All later lifecycle behavior is automatic.

## Components

### Manual launcher

`scripts/open.sh` remains the action entrypoint. It resolves the invocation
context, delegates layout work to the reconciler, and focuses the resulting
RepoRadar pane. It does not implement a second copy of pane discovery or
creation logic.

### Automatic hook

A dedicated automatic hook handles startup and manifest events. Startup mode
reads the structured `workspace list` response and reconciles workspaces
sequentially, which avoids exhausting Herdr's plugin-command concurrency limit.
Event mode reconciles only `HERDR_WORKSPACE_ID` and the tab supplied by the
event context.

### Workspace reconciler

The shared reconciler accepts an explicit workspace, optional preferred tab,
optional target pane, and a focus policy. It performs the following steps:

1. Validate all Herdr identifiers before using them in commands or lock paths.
2. Acquire a lock scoped by Herdr session and workspace.
3. Re-query the workspace, active tab, and panes after acquiring the lock.
4. Identify live RepoRadar candidates in the workspace.
5. Keep a candidate already in the active tab, otherwise keep the candidate
   with the lexically smallest pane ID, and safely close confirmed duplicates.
6. Select a live non-RepoRadar target in the active tab, preferring the event's
   focused pane when it still belongs to that tab.
7. Move the keeper into the active tab, or open a new split when no keeper
   exists.
8. Resize the right split to approximately 18 percent. Automatic callers leave
   the user's pane focused; the manual action focuses the reconciled RepoRadar.

Every command names the intended workspace, tab, or target pane explicitly.
The reconciler never relies on whichever workspace happens to be globally
focused when the asynchronous hook finally runs.

## Pane Identity And Root Selection

Herdr 0.7.5's public pane list does not expose plugin ownership. Exact
`RepoRadar` titles are therefore only candidate filters. A candidate must also
have a live foreground process whose process information identifies
`herdr-reporadar`. A duplicate is closed only through `herdr plugin pane close`;
the reconciler never applies the generic pane-close command to a title match.
This prevents an unrelated user pane named RepoRadar from being destroyed.

The initial scan root comes from the structured plugin context's
`workspace_cwd`. Startup recovery prefers a worktree checkout path when the
workspace exposes one, otherwise it uses the selected target pane's launch
`cwd`, not its mutable `foreground_cwd`. Moving an existing pane preserves its
original root and process.

The Rust binary will provide the small structured-JSON extraction operations
needed by the scripts. The scripts will not require `jq`, parse JSON with text
patterns, or evaluate values as shell source.

## Concurrency And Recovery

`workspace.created` and `workspace.focused` can arrive close together, and
separate Herdr clients can focus the same workspace concurrently. Locks are
therefore keyed by a portable checksum of `HERDR_SOCKET_PATH` plus the validated
workspace ID and stored under `HERDR_PLUGIN_STATE_DIR`.

Lock acquisition uses an atomic directory creation, records its owner PID,
recovers a lock whose owner is no longer alive, waits at most two seconds in
50-millisecond intervals, and always re-queries pane state after entry. The lock
is removed by normal, interrupt, and termination traps. Failure to acquire it
is logged and left for the next lifecycle event or manual action; it never
causes unbounded waiting.

Pane creation, pane movement, and process startup can race with target closure.
The reconciler retries state resolution at most three times with 100
milliseconds between attempts. It does not retry when the workspace or tab has
disappeared. Normal no-op reconciliations remain quiet, while actionable
failures are visible through Herdr's plugin command log.

The implementation does not subscribe to `pane.closed` or `pane.exited`.
Immediate respawn would turn an intentional `q` into a reopen loop and could
create an unbounded crash loop for a broken binary. The next focus or server
startup repairs a missing pane instead.

## Resource Model

There is one scanner per workspace, not one per tab. A workspace with many
tabs therefore pays the same Git scanning cost as a workspace with one tab.
Different workspaces intentionally keep separate scanners and roots, including
when two workspaces happen to point at the same directory.

Moving a pane must preserve plugin ownership, the running process, expansion
state, and workspace scan root. It may update the old and new tab layouts, but
must not move user panes or change the globally focused pane.

## Testing

Automated tests will cover:

- manifest registration of startup, workspace, and tab hooks;
- startup enumeration of multiple restored workspaces;
- creation in a workspace with no RepoRadar pane;
- no-op reconciliation when the pane is already in the active tab;
- movement from an inactive tab into the active tab;
- manual focus versus automatic no-focus behavior;
- simultaneous hooks converging on one pane;
- stale-lock recovery and session-scoped lock names;
- target-pane closure and workspace closure during reconciliation;
- rejection of an unrelated pane that merely has a RepoRadar title;
- deterministic, plugin-safe healing of duplicate RepoRadar panes;
- stable workspace-root selection when an agent changes its foreground
  directory.

Live Herdr 0.7.5 acceptance will use controlled temporary layouts and verify:

- switching between workspaces automatically exposes one right sidebar;
- switching between tabs moves the same pane ID rather than spawning another;
- creating a new workspace or tab requires no manual plugin action;
- the user's original pane remains focused after every automatic operation;
- rapid repeated focus events leave one RepoRadar process per workspace;
- a temporary `q` close is repaired on the next focus;
- the development plugin link and the user's original workspace focus are
  restored after testing.

## Release

This change will ship as `v0.1.2`. `Cargo.toml`, `herdr-plugin.toml`, README
installation examples, and release metadata must agree on that version. The
existing Release workflow must publish Linux x86_64, Linux ARM64, and macOS
ARM64 bundles plus `SHA256SUMS`.

Release acceptance requires successful CI and Release jobs, final-asset
checksum and architecture inspection, a real macOS ARM64 prebuilt installation,
a real `v0.1.2` tagged-source installation, automatic workspace/tab behavior in
both modes, and restoration of the local development link.

## Non-Goals

- Running one RepoRadar process per tab.
- A persistent user setting for temporary close beyond disabling the plugin.
- A supervised background daemon or custom event subscription loop.
- Sharing scan results between separate workspaces that use the same directory.
- Changing repository discovery, Git status semantics, or the TUI presentation.
