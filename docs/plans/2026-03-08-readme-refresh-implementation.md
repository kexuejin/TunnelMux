# README Refresh Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rewrite the repository README into a more compelling GUI-first product document, add Chinese entry support, and include one real local GUI screenshot plus macOS install guidance.

**Architecture:** Keep `README.md` as the English primary document, add `README.zh-CN.md` as the Chinese counterpart, and store a real GUI screenshot under `docs/images/`. Reuse existing docs for deeper technical detail while moving the README toward a landing-page structure.

**Tech Stack:** Markdown, local GUI screenshot asset, repository docs

---

### Task 1: Capture a real GUI screenshot

**Files:**
- Create: `docs/images/gui-home.png`

**Step 1: Launch the local GUI and arrange a representative state**
- Use the current release or debug GUI locally with a realistic home screen.

**Step 2: Capture one screenshot**
- Capture the main GUI page only.
- Prefer a state showing:
  - tunnel summary
  - public URL card
  - services list

**Step 3: Save it into the repository**
- Save as `docs/images/gui-home.png`

**Step 4: Verify file exists**
Run:
```bash
ls -lah docs/images/gui-home.png
```

Expected: screenshot file exists.

### Task 2: Rewrite `README.md`

**Files:**
- Modify: `README.md`

**Step 1: Write the failing expectation**
- Re-read the current README and identify missing sections:
  - language switch
  - GUI screenshot
  - explicit pain points
  - macOS install FAQ

**Step 2: Rewrite the structure**
- Add:
  - language switch
  - stronger hero
  - pain points
  - screenshot
  - GUI-first value section
  - installation
  - macOS FAQ

**Step 3: Verify links and image paths**
Run:
```bash
rg -n "README.zh-CN.md|docs/images/gui-home.png|Releasing" README.md
```

Expected: all references present.

### Task 3: Add `README.zh-CN.md`

**Files:**
- Create: `README.zh-CN.md`

**Step 1: Mirror the top-level README structure in Simplified Chinese**
- Keep the Chinese version useful, not machine-like.
- Include the same screenshot reference.

**Step 2: Add language switch links**
- Chinese README links back to `README.md`
- English README links to `README.zh-CN.md`

**Step 3: Verify file exists and links render plainly**
Run:
```bash
ls -lah README.zh-CN.md
rg -n "README.md|README.zh-CN.md" README.md README.zh-CN.md
```

Expected: files and cross-links exist.

### Task 4: Add macOS install FAQ details

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Optional: `docs/RELEASING.md`

**Step 1: Add practical launch guidance**
- Cover:
  - right-click → Open
  - System Settings → Privacy & Security → Open Anyway
  - `xattr` guidance only when necessary
- Explicitly say to do this only when the source is trusted.

**Step 2: Verify wording is concise and actionable**
- Ensure the steps are short and skimmable.

### Task 5: Final verification

**Files:**
- Verify only

**Step 1: Check all new doc references**
Run:
```bash
rg -n "gui-home.png|README.zh-CN.md|无法验证开发者|App is damaged" README.md README.zh-CN.md docs/RELEASING.md
```

Expected: all required references present.

**Step 2: Inspect git diff**
Run:
```bash
git diff -- README.md README.zh-CN.md docs/RELEASING.md docs/images/gui-home.png
```

Expected: only the intended doc and asset changes are present.

