# Releasing satex

## Overview

satex releases are automated via GitHub Actions. The release process involves cutting a new version, building binaries for multiple platforms, and publishing them to GitHub Releases.

## Cutting a Release

To cut a new release, use the `scripts/release.sh` script:

```bash
scripts/release.sh <patch|minor|major> "<release-title>"
```

The script will:
1. Verify the working tree is clean
2. Read the current version from Cargo.toml
3. Bump the version according to the specified type (patch, minor, or major)
4. Update Cargo.toml and Cargo.lock
5. Generate release notes in `doc/release-notes/<version>.md` with commits since the previous release

The script will print two commands you must run manually:

```bash
git commit -am 'Bump version to X.Y.Z'
git tag -a vX.Y.Z -m "Release vX.Y.Z"
```

Pushing the tag (`git push origin vX.Y.Z`) starts the release workflow: it checks that the tag matches `Cargo.toml`, builds every target, attaches the archives and a `SHA256SUMS` file, and uses `doc/release-notes/X.Y.Z.md` as the notes (GitHub's generated notes when that file is missing). A tag with a `-` (`v1.2.0-rc1`) is published as a prerelease.

## CI Process

### On Push or Pull Request

The CI workflow (`.github/workflows/ci.yml`) runs on every push and pull request:
- Installs stable Rust with clippy and rustfmt
- Caches Cargo dependencies
- Installs TeX Live (required for tests)
- Builds the release binary
- Runs all tests
- Runs clippy with strict warnings

### On Release Tag

When you push a tag matching `v*` (e.g., `v0.1.0`), the Release workflow (`.github/workflows/release.yml`) is triggered:

1. Builds binaries for all platforms in parallel:
   - x86_64-unknown-linux-gnu
   - aarch64-unknown-linux-gnu (using `cross`)
   - x86_64-apple-darwin
   - aarch64-apple-darwin
   - x86_64-pc-windows-msvc

2. For each platform:
   - Compiles the release binary
   - Strips the binary (Unix platforms only)
   - Packages it with README.md, SEMANTICS.md, and LICENSE (if present)
   - Creates a tarball (Unix) or zip file (Windows)
   - Uploads the archive

3. Creates a GitHub release:
   - Tags the release as prerelease if the version contains `-rc` or `-beta`
   - Uses release notes from `doc/release-notes/<version>.md` if it exists
   - Otherwise uses generated release notes from commit messages

## Artifacts

Built binaries are published to GitHub Releases and named:
- `satex-<version>-<target>.tar.gz` (Unix platforms)
- `satex-<version>-<target>.zip` (Windows)

Each archive contains the satex binary and documentation files.

## Release Notes

Release notes are stored in `doc/release-notes/<version>.md`. This file is created automatically by the release script and should contain a title (heading) and a list of commits since the previous release. You can edit it before pushing your commit if needed.
