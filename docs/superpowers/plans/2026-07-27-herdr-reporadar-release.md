# Herdr RepoRadar Release Implementation Plan

1. Add `scripts/package-release.sh` with a stable interface for assembling one
   target bundle from an already-built binary. Preserve the plugin runtime
   layout, source fallback, executable permissions, and deterministic asset
   naming.
2. Add `tests/release_package.sh` to exercise the packaging script with a fake
   executable and inspect the resulting archive. Run this test from normal CI.
3. Add `.github/workflows/release.yml` triggered by `v*` tags. Validate Cargo,
   plugin, and tag versions before running formatting, Clippy, and tests.
4. Build `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and
   `aarch64-apple-darwin` on native GitHub-hosted runners. Upload one compressed
   bundle per matrix entry.
5. Merge the three artifacts on Ubuntu, generate `SHA256SUMS`, and create or
   update the tag release with GitHub CLI and generated notes.
6. Expand README installation documentation with tagged source installation,
   GitHub CLI and browser binary installation, checksum verification, supported
   targets, upgrades, development-link recovery, and maintainer release steps.
7. Validate workflow YAML, shell syntax, package contents, Rust formatting,
   Clippy, tests, release build, and dependency advisories locally.
8. Commit and push the implementation to `main`, create and push annotated tag
   `v0.1.1`, monitor the Release workflow until all jobs complete, and inspect
   the published assets and checksums.
9. Download the macOS ARM64 release into a temporary directory, verify it,
   install it through `herdr plugin link`, and start a fresh RepoRadar pane.
10. Close the release pane, install the tagged source through
    `herdr plugin install`, start another fresh pane, then restore the local
    development link and verify its final source and process.
