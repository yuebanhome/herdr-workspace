# Herdr RepoRadar Implementation Plan

1. Create the Rust crate, Herdr manifest, launcher, and minimal documentation.
2. Implement recursive repository discovery and porcelain status parsing with
   real-Git integration tests.
3. Implement application state, background refresh, selection preservation,
   and deterministic visible-row construction.
4. Implement Ratatui rendering plus keyboard and mouse input.
5. Add launcher idempotency and right-side pane integration.
6. Run formatting, linting, unit/integration tests, release build, local plugin
   linking, and live Herdr pane verification.

## Audit Remediation

1. Anchor launch context to `workspace_cwd` and test precedence over pane CWD.
2. Expand untracked directories into individual files and bound every Git child
   process with a timeout.
3. Separate frequent status refresh from low-frequency repository discovery,
   use adaptive intervals, and prevent timer-driven scan backlogs.
4. Surface traversal errors, disambiguate duplicate repository names, and
   harden terminal setup/cleanup.
5. Re-run live Herdr regression tests, dependency advisory checks, and a second
   source audit before committing and pushing.
