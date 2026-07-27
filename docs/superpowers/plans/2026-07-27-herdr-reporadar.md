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

