# dev.to / 掘金 — 6-step tutorial (English)

## Title

Expose Local Services (and DeepSeek Harness) from One Desktop App — a TunnelMux Guide

## Cover note

Attach the social preview (`.github/social-preview.png`) or a fresh GUI screenshot as the cover image.

## Intro (2–3 sentences)

Local development rarely means a single localhost port anymore — it means a frontend, an API, docs, and maybe a local AI server like DeepSeek Harness or Open WebUI. TunnelMux is a free, open-source (Rust + Tauri) desktop app that puts that whole workflow behind one GUI. This guide walks through the happy path in six steps.

## Prerequisites

- macOS / Windows / Linux
- Either `cloudflared` or `ngrok` installed (or let TunnelMux prompt you to install it)
- A local service to expose, e.g. `http://127.0.0.1:3000`

## Step 1 — Install TunnelMux

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Or download the native installer (macOS `.dmg`, Windows `.msi`, Linux `.deb`) from the GitHub Releases page.

<!-- screenshot: TunnelMux main window after first launch -->

## Step 2 — Create your first tunnel

Click **Create Tunnel**. Give it a name (e.g. `main`), pick a provider, and leave the gateway target at its default. The GUI stores the profile locally so you can edit it later.

<!-- screenshot: tunnel creation form -->

## Step 3 — Start the tunnel

Click **Start Tunnel**. When it’s running you’ll see a public URL and the tunnel state turn green. This is the URL you’ll share (after adding services).

<!-- screenshot: running tunnel with public URL -->

## Step 4 — Add a service

Click **Add Service**, then fill in:

- **Local Service URL** — e.g. `http://127.0.0.1:3000`
- **Public Path** — e.g. `/app`
- Leave **Service enabled** on

Save it. The service now shows up on the card with its exposure, health, and access-gate status.

<!-- screenshot: service editor drawer -->

## Step 5 — Use the DeepSeek / SPA preset (for loopback-protected apps)

If your upstream is a mounted SPA that checks the Host/Origin — DeepSeek Harness is a good example — click **DeepSeek / SPA Preset** in the service editor. It sets:

- a path mount (`/deepseek`)
- Host forwarding **off** (so the app sees a loopback Host, not your public domain)
- response path rewriting **on**
- a reminder that root `/` stays closed unless another service exposes it

This is what makes `https://your-domain/deepseek` map cleanly to `http://127.0.0.1:3080`.

<!-- screenshot: DeepSeek preset applied -->

## Step 6 — Share a protected URL

Set a default service access code under **Settings → Default service access**, or choose per-service **inherit / custom / public**. Now opening the public route asks for the code once and stores a route-scoped cookie — other routes are untouched.

Use **Test** on the service card to verify the public route and upstream health before you share the link.

<!-- screenshot: access gate page on the public URL -->

## Recap

1. Install → 2. Create tunnel → 3. Start → 4. Add service → 5. Apply preset → 6. Share a protected URL.

If this saved you time, a ⭐ on the repo helps other developers find it: https://github.com/kexuejin/TunnelMux

## Notes for the author

- Replace every `<!-- screenshot: ... -->` with a real image (aim for 1600px wide).
- Use the `deepseek` route example only if it’s honest for your audience; otherwise substitute any local web app.
