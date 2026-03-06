# Release Rehearsal Workflow Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux now has a tag-driven release workflow that:

- builds raw platform archives,
- builds native GUI installers,
- optionally prepares macOS/Windows signed GUI bundles when repository variables and secrets are configured,
- publishes final assets to GitHub Releases.

That is enough for real tagged releases, but there is still a practical gap before maintainers can trust the signed-release path in production:

- there is no safe pre-release rehearsal mode,
- the current workflow only runs on `push` tags,
- the only way to fully exercise the packaging graph today is to cut a real release tag.

Now that the signing preflight exists, the next most valuable improvement is to provide a **non-publishing rehearsal path** that exercises the same asset graph before a real tag is created.

## Goals

- Allow maintainers to run the release pipeline manually without creating a real Git tag.
- Exercise the same raw archive + GUI bundle jobs used by real releases.
- Allow per-run signing-mode overrides for macOS and Windows rehearsals.
- Keep real tag publishing behavior unchanged.
- Produce merged rehearsal artifacts and checksums for download from workflow artifacts.

## Non-Goals

- No actual GitHub Release publication from rehearsal runs.
- No auto-tag creation or version bump automation.
- No new certificate provisioning logic.
- No refactor into a fully reusable multi-workflow release framework.

## Approaches Considered

### 1. Add a second standalone rehearsal workflow

**Pros**
- Keeps the tag-release workflow untouched.
- Easy to make manual-only.

**Cons**
- Duplicates the most critical release logic.
- Risks drift between rehearsal and real release behavior.
- Makes signing bugs harder to reason about because there are two pipelines.

**Decision:** Rejected.

### 2. Refactor release logic into a reusable workflow and add two callers

**Pros**
- Cleanest long-term architecture.
- Avoids duplication.
- Supports future reuse.

**Cons**
- Larger refactor of a critical workflow.
- More moving parts than needed for the next incremental gain.
- Harder to land quickly and verify locally.

**Decision:** Rejected for now.

### 3. Extend the existing release workflow with manual rehearsal mode

**Pros**
- Rehearsal uses the same workflow file as real release.
- Minimal conceptual surface area.
- Keeps the production publish path intact while adding a non-publishing branch.
- Supports per-run overrides without permanent repo-variable changes.

**Cons**
- Adds conditional logic to a critical workflow.
- Needs careful version and publish gating.

**Decision:** Recommended.

## Recommended Design

### Trigger Model

Extend `.github/workflows/release.yml` with `workflow_dispatch` in addition to tag push.

Manual runs should accept explicit inputs:

- `version` — required rehearsal version label such as `0.1.3-rc.1`
- `macos_signing_required` — optional per-run override
- `windows_signing_required` — optional per-run override

This makes rehearsals explicit and auditable while keeping tag pushes unchanged.

### Version Resolution

The workflow should derive the effective release version differently by trigger:

- tag push: `v0.2.0` → `0.2.0`
- manual rehearsal: use the provided `version` input exactly

The key requirement is that packaging steps do not accidentally treat the branch name as a release version.

### Signing Override Model

The existing repo-level signing toggles remain the default release policy:

- `GUI_MACOS_SIGNING_REQUIRED`
- `GUI_WINDOWS_SIGNING_REQUIRED`

Manual rehearsal runs can override those values for a single workflow run. That lets maintainers rehearse signed-mode behavior without permanently changing repository configuration.

Priority order:

1. manual input override, if provided,
2. repository variable fallback,
3. unsigned mode otherwise.

### Finalization Model

Real tag releases should continue to:

- download all build artifacts,
- generate `SHA256SUMS`,
- publish assets to GitHub Releases.

Manual rehearsal runs should instead:

- download all build artifacts,
- generate `SHA256SUMS`,
- upload the merged `dist/` directory as a workflow artifact,
- skip GitHub Release publication.

This keeps rehearsal output inspectable without touching the public release page.

### Safety Properties

The workflow must preserve these guarantees:

- no publish step runs on `workflow_dispatch`,
- manual rehearsals still fail early if signed mode is requested but credentials are missing,
- raw archive packaging and GUI bundle packaging use the same code path as real release,
- maintainers can inspect final merged artifacts before cutting a tag.

## Risks and Mitigations

### Risk: Conditional logic makes the release workflow harder to read

**Mitigation:** keep version/signing resolution explicit in named steps and document the behavior in `docs/RELEASING.md`.

### Risk: Manual input overrides accidentally mask repository policy

**Mitigation:** scope overrides only to the current workflow run and document precedence clearly.

### Risk: Rehearsal artifacts differ subtly from real release assets

**Mitigation:** reuse the same build jobs and only change the final publish step.

## Rollout Order

1. extend the workflow trigger and manual inputs,
2. add explicit version/signing-resolution steps,
3. gate publish behavior to tagged pushes only,
4. upload merged rehearsal artifacts for manual runs,
5. document rehearsal usage in `docs/RELEASING.md`.
