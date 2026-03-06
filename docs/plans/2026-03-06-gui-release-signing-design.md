# GUI Release Signing and Notarization Design

**Date:** 2026-03-06
**Status:** Approved for implementation

## Context

TunnelMux now publishes native GUI installers through GitHub Releases:

- macOS: `.dmg`
- Windows: `.msi`
- Linux: `.deb`

That closes the first installer-distribution gap, but the current release story still has an obvious trust gap:

- macOS direct-download apps should be code-signed and notarized,
- Windows installers should be code-signed,
- current documentation explicitly warns that GUI installers are unsigned.

The previous GUI-native-bundles iteration intentionally deferred signing, notarization, and updater work to keep the first release tractable. That was the right call, but now the next highest-value follow-up is to make the release pipeline *ready* for trusted distribution without destabilizing the current release path.

The repository already has a clean place to extend:

- `.github/workflows/release.yml` already has a dedicated `gui_bundle` job,
- `crates/tunnelmux-gui/tauri.conf.json` already contains bundle metadata and icons,
- `docs/RELEASING.md` already distinguishes signed work as future scope,
- the current workflow can already produce deterministic installer artifacts.

## Goals

- Prepare TunnelMux for signed GUI releases on macOS and Windows.
- Preserve the current raw archive flow and Linux `.deb` packaging.
- Keep local development and PR validation working without release credentials.
- Add explicit CI preflight checks so missing signing prerequisites fail clearly when signed mode is requested.
- Keep the feature incremental: release-signing readiness first, automatic trusted distribution second.

## Non-Goals

- No Tauri auto-updater or release-channel work in this iteration.
- No Linux package signing in this iteration.
- No certificate purchase, account provisioning, or external vendor onboarding automation.
- No forced migration to signed releases for every workflow run.
- No redesign of the existing archive release path.

## User-Confirmed Scope

This iteration is **GUI release signing + notarization design with CI pre-wiring**.

That means:

- add repo-level seams for signing configuration,
- add workflow logic that can run in unsigned mode or signed mode,
- document the exact secrets/variables needed to turn signed mode on,
- keep the repository releasable before credentials are provisioned,
- avoid promising auto-update or store-distribution behavior.

## Approaches Considered

### 1. Keep installers unsigned and document manual post-processing

**Pros**
- Zero CI complexity.
- No additional secrets or release setup.
- No platform-specific signing maintenance.

**Cons**
- Does not reduce end-user trust warnings.
- Keeps release quality dependent on undocumented manual work.
- Leaves the new GUI installer path feeling unfinished.

**Decision:** Rejected.

### 2. Require fully signed installers immediately on every tagged release

**Pros**
- Strongest distribution guarantees.
- Forces release hygiene quickly.
- Makes unsigned releases impossible once implemented.

**Cons**
- Blocks release work until every credential is provisioned correctly.
- Harder to validate safely in-repo before the secrets exist.
- Couples rollout timing to external certificate/vendor setup.

**Decision:** Rejected for this iteration.

### 3. Add staged signed-release support with explicit CI modes

**Pros**
- Keeps today’s unsigned release path working.
- Lets maintainers enable signing platform-by-platform when credentials are ready.
- Creates a clear migration path from “unsigned but functional” to “signed by default”.
- Matches the requested scope: design plus CI pre-wiring.

**Cons**
- Requires a small amount of release-mode logic in CI.
- Public releases may remain unsigned until maintainers enable the mode.

**Decision:** Recommended.

## Recommended Design

### Release Policy Model

The release workflow should support two states per GUI platform:

1. **Unsigned mode**
   - current behavior remains intact,
   - bundle job still builds `.dmg`, `.msi`, and `.deb`,
   - release docs remain truthful while credentials are not configured.

2. **Signed mode**
   - enabled explicitly by repository variables,
   - CI validates that required secrets/variables are present,
   - macOS builds use code signing + notarization,
   - Windows builds use code signing,
   - missing prerequisites fail early with readable messages.

This keeps signed distribution opt-in until operators are ready.

### Platform Strategy

#### macOS

Use the Tauri-supported signing and notarization path for direct-download macOS apps:

- code signing identity from `APPLE_SIGNING_IDENTITY`,
- CI certificate import using `APPLE_CERTIFICATE` and `APPLE_CERTIFICATE_PASSWORD`,
- notarization via App Store Connect API credentials,
- a workflow-generated `APPLE_API_KEY_PATH` file from a repository secret.

For macOS direct downloads, the desired outcome is a signed and notarized `.dmg` produced by the existing `gui_bundle` matrix entry.

Preferred authentication path:

- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- repository secret containing the private key material, written to `APPLE_API_KEY_PATH` at runtime

This avoids Apple ID app-specific-password flow and keeps automation account-oriented.

#### Windows

Use Tauri’s `bundle.windows.signCommand` integration and the documented Azure Trusted Signing flow.

Recommended Windows strategy:

- keep the base `tauri.conf.json` unsigned-safe,
- generate a temporary Windows-only Tauri config overlay in CI,
- populate `bundle.windows.signCommand` with the Trusted Signing CLI command,
- authenticate via Azure environment secrets.

That yields a repo that is neutral by default but signing-ready in CI.

#### Linux

Keep Linux `.deb` packaging unchanged in this iteration.

Rationale:

- Linux package signing is distribution-channel-specific,
- it is not required to close the macOS/Windows trust gap,
- adding it now would broaden scope beyond the current release need.

### Configuration Shape

The committed base config should remain safe for local unsigned builds.

Environment-specific signing details should be injected at release time, not hardcoded into the repository. The cleanest split is:

- base GUI bundle settings live in `crates/tunnelmux-gui/tauri.conf.json`,
- Windows signing command is written into a temporary config overlay during CI,
- macOS notarization credentials stay entirely in workflow environment variables,
- helper scripts handle validation and temporary-file generation where that keeps YAML readable.

This avoids leaking environment assumptions into the normal developer workflow.

### CI Flow Changes

The existing `gui_bundle` job should gain a small signed-release preparation layer.

For each matrix row:

1. determine whether signing is required for this platform,
2. validate the exact required variables/secrets,
3. prepare any temporary files or config overlays,
4. invoke the existing Tauri bundle step,
5. collect artifacts using the existing bundle-collection helper.

Desired behavior:

- unsigned mode: continue bundling with minimal overhead,
- signed mode: fail before the expensive build if prerequisites are missing,
- publish job remains unchanged except for documentation truthfulness.

### Secrets and Variables Model

The repository should separate **toggle/configuration** from **secret material**.

Suggested repository variables:

- `GUI_MACOS_SIGNING_REQUIRED`
- `GUI_WINDOWS_SIGNING_REQUIRED`
- `WINDOWS_TRUSTED_SIGNING_ENDPOINT`
- `WINDOWS_TRUSTED_SIGNING_ACCOUNT`
- `WINDOWS_TRUSTED_SIGNING_PROFILE`

Suggested repository secrets:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_PRIVATE_KEY`
- `AZURE_CLIENT_ID`
- `AZURE_CLIENT_SECRET`
- `AZURE_TENANT_ID`

The key design principle is that toggles and non-secret account/profile metadata can be visible, but credential material stays in secrets.

### Verification Strategy

This feature cannot be fully end-to-end verified locally without real credentials, so verification should be split into two layers.

#### Local verification

Local and branch validation should cover:

- helper-script syntax,
- helper-script fixture-based output checks,
- workflow structure checks,
- unsigned bundle smoke checks still passing.

#### CI verification

Credential-backed verification should happen only in GitHub Actions:

- signed macOS `.dmg` build succeeds,
- notarization completes,
- signed Windows `.msi` build succeeds,
- disabled signing mode still produces unsigned assets.

This keeps the repository honest about what can and cannot be proven offline.

## Risks and Mitigations

### Risk: Partial credential rollout creates confusing release states

**Mitigation:** use explicit per-platform signing toggles and document them in `docs/RELEASING.md`.

### Risk: Workflow YAML becomes brittle

**Mitigation:** move repeated signing-preparation logic into small scripts and validate them independently.

### Risk: macOS notarization adds long or flaky release time

**Mitigation:** keep notarization scoped to macOS release jobs only, fail early on missing inputs, and document the expectation clearly.

### Risk: Windows signing vendor choice becomes a hidden architectural dependency

**Mitigation:** contain the choice behind `bundle.windows.signCommand` and repository variables so the rest of the release pipeline stays stable.

## Rollout Order

1. add design and implementation plan,
2. add CI/helper seams while keeping unsigned mode default,
3. update maintainer docs with exact setup steps,
4. provision real credentials in the repository,
5. enable platform signing toggles,
6. verify first signed tagged release.

## Out of Scope Follow-Ups

Once this work lands, the next adjacent follow-ups become:

- flipping signed mode on by default,
- Linux package-signing strategy,
- Tauri updater metadata and release-channel policy,
- storefront/distribution-channel packaging decisions.
