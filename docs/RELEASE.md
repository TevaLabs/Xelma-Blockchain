# Release ritual

This document describes how maintainers cut a release with consistent change
notes. Automated changelog generation is provided by
[`.github/workflows/changelog.yml`](../.github/workflows/changelog.yml) (Issue
#191).

## Prerequisites

- Admin/maintainer access to the repository
- [`gh`](https://cli.github.com/) authenticated locally (optional, for dry runs)
- Contract and bindings CI green on `main`

## Label convention

Merged pull requests are bucketed deterministically:

| Label(s) | Changelog section |
| -------- | ----------------- |
| `security`, `changelog:security` | **Security** |
| `bug`, `fix`, `changelog:fixed` | **Fixed** |
| `enhancement`, `feature`, `changelog:added` | **Added** |
| (none / other) | **Added** (default) |

Security-labeled PRs always appear under **Security**, even if they also carry
other labels.

## Maintainer workflow

### 1. Preview the draft

1. Open **Actions → Changelog → Run workflow**.
2. Set **mode** to `preview`.
3. Optionally set **version** (defaults to `Unreleased`).
4. Download the `changelog-draft` artifact and review Added / Fixed / Security
   sections.

Fix any mis-bucketed PRs by adjusting labels on the merged PR, then re-run
preview until the draft looks correct.

### 2. Publish the release section

1. Run the workflow again with **mode** `publish` and **version** set (e.g.
   `0.2.0`).
2. The workflow replaces the `[Unreleased]` block in [`CHANGELOG.md`](../CHANGELOG.md)
   with a dated `## [version]` section and restores an empty `[Unreleased]`
   stub.
3. Verify the commit on `main`.

### 3. Tag and validate bindings (existing flow)

1. Bump `bindings/package.json` version and add a matching entry in
   `bindings/CHANGELOG.md` if publishing the npm package.
2. Push an annotated tag `vX.Y.Z`.
3. The **Release Bindings** workflow validates WASM build, parity, and changelog
   entries (`release-bindings.yml`).

### 4. Communicate

- Link the new `CHANGELOG.md` section in the GitHub Release notes.
- Announce breaking contract or binding changes explicitly in **Added** or
  **Security** as appropriate.

## Local dry run

```bash
chmod +x .github/scripts/generate-changelog.sh
.github/scripts/generate-changelog.sh 0.2.0
cat CHANGELOG.draft.md
```

The script sorts entries by PR number for deterministic output across runs.
