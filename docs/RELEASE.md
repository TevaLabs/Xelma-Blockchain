# Release ritual

This document describes how maintainers cut a release with consistent change
notes. Automated changelog generation is provided by
[`.github/workflows/changelog.yml`](../.github/workflows/changelog.yml) (Issue
#191).

## Prerequisites

- Admin/maintainer access to the repository
- [`gh`](https://cli.github.com/) authenticated locally (optional, for dry runs)
- Contract and bindings CI green on `main`
- **Emergency Drill Gate green** — the claims-only `pause → claims → resume`
  drill is a required check. See
  [`EMERGENCY_DRILL.md`](EMERGENCY_DRILL.md) §4.

## Required gates

The following are blocking. `CI Success` (`ci-success`) fails if any of them is
red, and the release must not be cut while one is missing.

| Gate | Check name | Manual equivalent |
|---|---|---|
| Emergency drill | `Emergency Drill Gate` | `./scripts/emergency_drill_gate.sh` |
| WASM size | `WASM Size Gate` | `./scripts/check_wasm_size.sh` |
| Workspace tests | `Rust Tests` | `cargo test --workspace --locked` |

The emergency drill is a separate required check rather than a step buried in
`Rust Tests` so a regression in the incident-mode `pause → claims → resume`
path is impossible to overlook at review time. To re-run every gate against the
exact ref you are about to tag, use **Actions → CI → Run workflow**
(`workflow_dispatch`).

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

### 0. Verify the release gates

1. Confirm `Emergency Drill Gate`, `WASM Size Gate` and `CI Success` are green
   on the release commit.
2. Re-run them on the release ref if needed: **Actions → CI → Run workflow**.
3. Run the drill gate locally as a second pair of eyes:
   ```bash
   ./scripts/emergency_drill_gate.sh
   ```
4. Work through the release checklist in
   [`EMERGENCY_DRILL.md`](EMERGENCY_DRILL.md) §4. It is the authoritative
   release checklist for incident-mode behaviour; the items marked **gate**
   block the release.

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
- Attach (or link) the `emergency-drill-log` artifact and record the drill
  result in the release notes.

## Local dry run

```bash
chmod +x .github/scripts/generate-changelog.sh
.github/scripts/generate-changelog.sh 0.2.0
cat CHANGELOG.draft.md
```

The script sorts entries by PR number for deterministic output across runs.
