# README Refresh Design

**Date:** 2026-03-08

**Scope**
- Reframe the README as a product-facing landing document instead of a pure engineering reference.
- Add explicit GUI-first positioning near the top.
- Add bilingual README support with English and Simplified Chinese entry points.
- Add one real local GUI screenshot.
- Add practical macOS install guidance for “App is damaged” / “cannot verify developer”.

## Problem

The current README explains what TunnelMux is, but it still reads more like a technical package overview than a product page.

That creates three issues:

- the GUI value is visible, but not emphasized enough
- the document does not lead with the real user pain points
- there is no clear Chinese entry point for Chinese-speaking users

## User Story

A user should land on the repository and understand within a few seconds:

1. why local tunnel workflows become painful in real projects
2. that TunnelMux has a GUI, not just a daemon and CLI
3. that TunnelMux helps with multi-service local exposure, diagnostics, and troubleshooting
4. how to install and what to do if macOS blocks launch

## Positioning

TunnelMux should be presented as:

**A developer-first local tunnel control console**

Not:
- a cloud platform
- a generic reverse proxy
- a niche wrapper around one provider

The README should explicitly speak to:
- vibe coding workflows
- local frontend + API + docs stacks
- exposing more than one local service
- debugging whether the daemon, tunnel, route, or local service is broken

## Content Structure

### 1. Language switch
- Add a compact language switch at the top:
  - `English | 简体中文`
- English README remains canonical root file: `README.md`
- Chinese README lives at: `README.zh-CN.md`

### 2. Hero
- One-sentence positioning
- One short paragraph about the pain of manual `cloudflared` / `ngrok` workflows
- One screenshot immediately below the hero section

### 3. Pain points
- Explicit bullets about:
  - multiple local services
  - ad-hoc tunnel scripts
  - route sprawl
  - hard-to-locate failures
  - team reproducibility

### 4. Product promise
- One short section translating the pain into the TunnelMux model:
  - one daemon
  - one GUI
  - one control plane

### 5. GUI-first capability section
- Elevate GUI above lower-level implementation details
- Clarify:
  - current tunnel
  - services
  - diagnostics
  - provider management

### 6. Installation and quick start
- Keep existing install instructions
- Make them secondary to the value proposition

### 7. macOS install FAQ
- Add a user-facing FAQ section with practical steps for:
  - “App is damaged”
  - “cannot verify developer”
- The tone should be operational and clear, not legalistic

## Screenshot Policy

- Use **one real local GUI screenshot**
- Prefer the main GUI home screen with:
  - current tunnel summary
  - public URL card
  - services list
- Store it under `docs/images/`

## Tone

- More product-facing than current README
- Still technical and developer-oriented
- Avoid exaggerated marketing language
- Prefer clear pain → solution → install flow

## Non-Goals

- no full documentation site redesign
- no separate marketing website
- no animated GIF capture requirement
- no dual-language duplication across all docs in this pass

