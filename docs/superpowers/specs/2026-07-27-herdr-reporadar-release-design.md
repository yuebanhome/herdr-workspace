# Herdr RepoRadar Release Design

## Purpose

RepoRadar releases should provide versioned, checksummed binaries that can be
downloaded and linked without compiling Rust locally, while preserving Herdr's
native GitHub installation path for users who prefer source builds.

Herdr 0.7.5 does not resolve GitHub Release assets during
`herdr plugin install`. That command clones the requested repository revision
and runs the build command from `herdr-plugin.toml`. Therefore the project will
support two explicit installation paths:

- `herdr plugin install yuebanhome/herdr-workspace --ref vX.Y.Z -y` clones the
  tagged source and builds it locally;
- a user can download the matching platform bundle from GitHub Releases,
  extract it, and run `herdr plugin link <bundle> --enabled` without compiling.

## Release Trigger And Version Contract

The release workflow runs only for pushed tags matching `v*`. It receives
write permission for repository contents because it creates or updates the
corresponding GitHub Release. Pull requests and normal branch pushes never run
the privileged release job.

Before building, a validation job parses both package manifests and requires:

1. the Git tag to equal `v` followed by the Cargo package version;
2. `Cargo.toml` and `herdr-plugin.toml` to contain the same version;
3. formatting, Clippy with warnings denied, and all tests to pass with the lock
   file enforced.

A mismatched tag or manifest version fails before any release asset is built.

## Supported Artifacts

Each release publishes three native archives:

- `x86_64-unknown-linux-gnu` on an x86_64 Ubuntu runner;
- `aarch64-unknown-linux-gnu` on an ARM64 Ubuntu runner;
- `aarch64-apple-darwin` on an Apple Silicon macOS runner.

Intel macOS is intentionally unsupported. This matches the current deployment
need and avoids publishing an unneeded binary.

Every archive is named
`herdr-reporadar-vX.Y.Z-<target>.tar.gz` and extracts into a single
`herdr-reporadar/` directory. The directory preserves the paths expected by the
plugin manifest:

```text
herdr-reporadar/
  Cargo.lock
  Cargo.toml
  LICENSE
  README.md
  herdr-plugin.toml
  scripts/open.sh
  src/
  target/release/herdr-reporadar
```

Including the small Rust source tree keeps the bundle internally complete if a
future Herdr command chooses to execute the declared build step. The prebuilt
binary remains at `target/release/herdr-reporadar`, so the current
`herdr plugin link` path starts immediately without compiling.

## Workflow Architecture

The workflow contains three boundaries:

- `validate` checks versions and runs the complete quality gate once;
- `build` is a three-entry matrix that installs Rust 1.88, builds the selected
  target with `--release --locked`, assembles the bundle, and uploads the tarball
  as an Actions artifact;
- `release` downloads all build artifacts, produces `SHA256SUMS`, and creates or
  updates the GitHub Release with generated release notes.

The release step uses the GitHub CLI already present on hosted runners. On a
retry, an existing release is reused and its assets are uploaded with clobber
semantics, making recovery from a partially failed run deterministic.

Official GitHub actions are used for checkout and artifact transfer. The
workflow does not run downloaded project scripts with release credentials, and
the write-capable job only consumes artifacts produced by the validated commit.

## Documentation And Operation

The README will document both installation paths with copy-pasteable commands,
the supported target table, checksum verification, upgrade behavior, recovery
to a local development link, and the release procedure. Binary installation
instructions will cover both GitHub CLI download and direct browser download,
then show extraction and `herdr plugin link --enabled` from the resulting
directory.

Source installation documentation will use:

```bash
herdr plugin install yuebanhome/herdr-workspace --ref vX.Y.Z -y
```

Binary installation documentation will use the target-specific archive and
verify its entry in `SHA256SUMS` before linking it. The commands must not assume
that the user has a Rust toolchain.

The release procedure is:

1. update both manifests to the intended version and merge the change;
2. create an annotated `vX.Y.Z` tag on the release commit;
3. push the tag;
4. wait for the Release workflow and verify all three archives plus
   `SHA256SUMS`.

For the first deployment, the existing `0.1.1` manifests and commit will be
tagged as `v0.1.1` after the workflow lands on `main`.

## Local Installation Regression

After GitHub publishes `v0.1.1`, the release is tested on the local Apple
Silicon machine with Herdr 0.7.5. The regression uses a temporary directory and
performs the same steps documented for users:

1. download the macOS ARM64 archive and `SHA256SUMS` from GitHub Release;
2. validate the archive checksum;
3. extract the bundle and verify the binary architecture and executable bit;
4. link the extracted bundle with `herdr plugin link <bundle> --enabled`;
5. invoke `herdr-reporadar.open` and verify that one functioning right-side pane
   opens from the released binary;
6. install the tagged source with
   `herdr plugin install yuebanhome/herdr-workspace --ref v0.1.1 -y` and verify
   the action again;
7. restore the development checkout with `herdr plugin link <repo> --enabled`
   and verify the final plugin source and pane process.

Existing RepoRadar panes are closed between installation modes so no old
process can make a broken installation appear healthy. Temporary release files
are removed after the test.

## Failure Handling

No GitHub Release is published if validation or any platform build fails. A
release job failure leaves build artifacts available in the workflow run for
diagnosis. Rerunning the workflow safely replaces assets on the same tag.

Checksums are generated only after all platform archives have been downloaded
into the release job. Archive paths and executable permissions are verified in
the build job before upload.

## Acceptance

The release delivery is complete when:

- the workflow syntax and local packaging logic are tested;
- a pushed `v0.1.1` tag completes all three builds;
- GitHub Release `v0.1.1` contains three target archives and `SHA256SUMS`;
- an Apple Silicon bundle downloads, verifies, extracts, and exposes an
  executable `target/release/herdr-reporadar`;
- the extracted bundle is accepted by `herdr plugin link <bundle> --enabled`;
- the linked release bundle opens a new RepoRadar pane using its packaged
  binary;
- `herdr plugin install yuebanhome/herdr-workspace --ref v0.1.1 -y` remains a
  documented and locally tested source-build alternative;
- the README installation commands match the commands used by the successful
  local regression.
