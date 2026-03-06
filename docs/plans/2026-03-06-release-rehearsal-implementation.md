# Release Rehearsal Workflow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a manual non-publishing rehearsal mode to the existing release workflow so maintainers can exercise raw archives, GUI bundles, and optional signing paths before cutting a real tag.

**Architecture:** Extend `.github/workflows/release.yml` rather than introducing a second release pipeline. Keep the current tag-driven publish path intact, add `workflow_dispatch` inputs, resolve version/signing mode explicitly inside steps, and upload a merged rehearsal artifact bundle when the workflow is run manually.

**Tech Stack:** GitHub Actions YAML, shell scripting, Python for small event-payload parsing, Markdown maintainer docs.

---

### Task 1: Add manual release-rehearsal trigger and version resolution

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `.github/workflows/release.yml`

**Step 1: Write the failing workflow check**

Run:

```bash
ruby -e 'require "yaml"; data = YAML.load_file(".github/workflows/release.yml"); on = data.fetch("on"); abort("missing workflow_dispatch") unless on.key?("workflow_dispatch")'
```

Expected: FAIL because the workflow is tag-only today.

**Step 2: Write the minimal implementation**

Update `.github/workflows/release.yml` to:

- add `workflow_dispatch`,
- require a `version` input,
- add optional `macos_signing_required` and `windows_signing_required` inputs,
- resolve the effective package version in the build job from either the manual input or the tag name,
- fail clearly if a manual run does not provide a usable version.

Use event-payload parsing in shell/Python rather than relying on brittle context assumptions across trigger types.

**Step 3: Re-run the workflow check**

Run:

```bash
ruby -e 'require "yaml"; data = YAML.load_file(".github/workflows/release.yml"); on = data.fetch("on"); abort("missing workflow_dispatch") unless on.key?("workflow_dispatch"); text = File.read(".github/workflows/release.yml"); abort("missing rehearsal version input") unless text.include?("version:"); puts "workflow dispatch ok"'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release rehearsal trigger"
```

### Task 2: Add per-run signing overrides and rehearsal finalization

**Files:**
- Modify: `.github/workflows/release.yml`
- Test: `.github/workflows/release.yml`

**Step 1: Write the failing workflow check**

Run:

```bash
ruby -e 'text = File.read(".github/workflows/release.yml"); abort("missing macOS override input") unless text.include?("macos_signing_required"); abort("missing rehearsal artifact upload") unless text.include?("rehearsal-dist")'
```

Expected: FAIL because manual override/finalization behavior is not yet implemented.

**Step 2: Write the minimal implementation**

Update `.github/workflows/release.yml` so that:

- signing resolution prefers manual `workflow_dispatch` inputs over repository variables,
- the final aggregation job always downloads raw + GUI artifacts and generates `SHA256SUMS`,
- manual runs upload `dist/*` as a merged artifact such as `rehearsal-dist`,
- the GitHub Release publish step runs only for tag-push releases.

Keep real tag behavior functionally equivalent to today.

**Step 3: Re-run the workflow check**

Run:

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml"); text = File.read(".github/workflows/release.yml"); abort("missing macOS override input") unless text.include?("macos_signing_required"); abort("missing windows override input") unless text.include?("windows_signing_required"); abort("missing rehearsal artifact upload") unless text.include?("rehearsal-dist"); abort("missing push-only publish guard") unless text.include?("github.event_name == 'push'"); puts "rehearsal finalize ok"'
```

Expected: PASS.

**Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release rehearsal finalization"
```

### Task 3: Document manual rehearsal usage

**Files:**
- Modify: `docs/RELEASING.md`

**Step 1: Write the failing documentation check**

Run:

```bash
rg -n 'workflow_dispatch|rehearsal|rehearsal-dist|macos_signing_required|windows_signing_required' docs/RELEASING.md
```

Expected: FAIL because rehearsal mode is not documented yet.

**Step 2: Write the minimal implementation**

Update `docs/RELEASING.md` to describe:

- how to manually trigger a rehearsal run,
- that `version` is required,
- what the signing override inputs do,
- that manual rehearsals upload merged artifacts and checksums but do not publish a GitHub Release.

**Step 3: Re-run the documentation check**

Run:

```bash
rg -n 'workflow_dispatch|rehearsal|rehearsal-dist|macos_signing_required|windows_signing_required' docs/RELEASING.md
```

Expected: PASS.

**Step 4: Commit**

```bash
git add docs/RELEASING.md
git commit -m "docs: explain release rehearsal workflow"
```

### Task 4: Run final verification

**Files:**
- Modify if needed: any files above to fix validation issues

**Step 1: Run workflow validation**

Run:

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml"); puts "workflow ok"'
```

Expected: PASS.

**Step 2: Run targeted structure checks**

Run:

```bash
ruby -e 'text = File.read(".github/workflows/release.yml"); abort("missing workflow_dispatch") unless text.include?("workflow_dispatch"); abort("missing version input") unless text.include?("version:"); abort("missing rehearsal-dist") unless text.include?("rehearsal-dist"); abort("missing push-only publish guard") unless text.include?("github.event_name == 'push'"); puts "release rehearsal structure ok"'
```

Expected: PASS.

**Step 3: Re-run workspace verification**

Run:

```bash
cargo test --workspace --quiet
```

Expected: PASS.

**Step 4: Commit any last fixes**

```bash
git add .
git commit -m "chore: finalize release rehearsal workflow"
```
