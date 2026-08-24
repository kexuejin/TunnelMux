# Promo copy — English (Show HN / Reddit / dev.to)

## Suggested titles

- Show HN: TunnelMux — a GUI-first local tunnel console for cloudflared/ngrok
- TunnelMux: open-source desktop app to expose local services, with access gates — works great for remote access to DeepSeek Harness / Ollama / Open WebUI

## Show HN post body

TunnelMux is a GUI-first local tunnel control console I built in Rust + Tauri.

**The pain:** local sharing turns into a pile of ad-hoc `cloudflared`/`ngrok` commands, route scripts and half-broken demos the moment your project is “frontend + API + docs + callback endpoint” instead of one localhost port.

**What it does:**

- One desktop app to create tunnels, start/stop them, and manage services per tunnel
- cloudflared quick tunnels or named tunnels; ngrok with reserved domains
- Multi-service host/path routing with health checks
- Service access gates: a global default code plus per-service inherit / custom / public modes (route-scoped cookies)
- Built-in updater: checks GitHub Releases, verifies SHA256, installs the raw archive, then “Restart Now”
- English / 简体中文 UI
- DeepSeek / SPA preset that mounts loopback-protected apps like DeepSeek Harness under a path with correct Host/Origin stripping and response path rewriting

**A concrete use case:** you want remote access to DeepSeek Harness or Open WebUI running on your machine. Add a service, apply the preset, and `https://your-domain/deepseek` maps to `http://127.0.0.1:3080` — with root `/` staying closed and the route behind an access code.

**Install (macOS/Linux):**

```bash
curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
```

Or grab the `.dmg` / `.msi` / `.deb` from the Releases page.

Rust + Tauri, one daemon + one API behind both the GUI and the CLI. Feedback is very welcome — especially on the route/gate model and the updater.

## Reddit cross-post (r/selfhosted, r/rust, r/webdev, r/locallama)

**Title:** TunnelMux — open-source desktop app to expose local services (cloudflared/ngrok) with access gates; perfect for remote access to DeepSeek Harness / Ollama / Open WebUI

**Body:** Same core as above, trimmed to 4-6 bullets + the DeepSeek Harness use case + install command. Ask a specific question at the end (e.g. “what’s missing for you to switch from your current tunnel setup?”) to drive comments.

## dev.to / blog variant

Write it as a 6-step tutorial: 1) install TunnelMux → 2) create tunnel → 3) start → 4) add service → 5) apply DeepSeek/SPA preset → 6) share protected URL. Screenshot each step. End with “star the repo if this saved you time”.

## Show HN A/B variants

### Title A (feature-led)
Show HN: TunnelMux — a GUI-first tunnel console for cloudflared/ngrok (built with Rust + Tauri)

### Title B (pain-led)
Show HN: TunnelMux — stop juggling cloudflared/ngrok commands; expose local services and DeepSeek Harness from one desktop app

### Opening A (direct)
I got tired of copy-pasting `cloudflared` commands, so I built a desktop app around them. Meet TunnelMux — a GUI-first local tunnel console in Rust + Tauri. Start a tunnel, add services, share a protected URL. No shell history required.

### Opening B (use-case led)
Your local AI tooling — DeepSeek Harness, Ollama, Open WebUI — should be reachable from your phone or a teammate's browser. Here's the open-source app I use to do that safely: TunnelMux. One click to expose a local port, with per-route access codes and root `/` closed by default.

### Closing CTA A (feedback)
If you manage local tunnels by hand today, I'd love to hear what your setup looks like and what's missing before you'd switch. Repo: https://github.com/kexuejin/TunnelMux

### Closing CTA B (trust)
I tried to keep the security story boring and safe: per-route access codes, root `/` closed by default, and correct Host/Origin handling for loopback-protected apps like DeepSeek Harness. What would make you trust this with your own stack?

### A/B rules of thumb
- Post **B** on r/selfhosted and r/locallama (use-case resonates). Post **A** on r/rust and Show HN (build + tech resonates).
- Keep the body under ~40 lines on Show HN; cut the feature list to 4 bullets for Reddit.
- Reply to the first 5 comments within the first hour — early replies strongly affect ranking.
