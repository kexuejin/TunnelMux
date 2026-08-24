# TunnelMux

GUI-first local tunnel control console for developers who are tired of juggling `cloudflared`, `ngrok`, route scripts, and half-broken local demos.

Start a tunnel, expose local services — APIs, docs, webhooks, and AI tools like DeepSeek Harness, Ollama, or Open WebUI — behind one desktop app, with multi-service routing, health checks, and per-route access gates.

## Install

=== "macOS / Linux"

    ```bash
    curl -fsSL https://raw.githubusercontent.com/kexuejin/TunnelMux/main/scripts/install.sh | bash
    ```

=== "Native installers"

    Download the latest `.dmg`, `.msi`, or `.deb` from [GitHub Releases](https://github.com/kexuejin/TunnelMux/releases).

## Quick start

1. Create a tunnel profile (`cloudflared` or `ngrok`)
2. Start the tunnel
3. Add a local service (for example `http://127.0.0.1:3000`)
4. Share the public URL

## Highlights

- Desktop GUI + CLI sharing one daemon and one API
- Multi-service host/path routing with health checks
- Service access gates: default code plus per-service inherit / custom / public
- DeepSeek / SPA preset for loopback-protected apps
- In-app updater with SHA256 verification
- English / 简体中文 UI

## Documentation

- [Architecture](ARCHITECTURE.md)
- [API](API.md)
- [Integration](INTEGRATION.md)
- [Integration templates](INTEGRATION-TEMPLATES.md)
- [Roadmap](ROADMAP.md)
- [Releasing](RELEASING.md)

!!! tip "Local AI tooling"
    Mount DeepSeek Harness, Ollama, or Open WebUI behind one clickable path with the **DeepSeek / SPA Preset**, and keep root `/` closed by default.

> If TunnelMux saves you time, [star the repo](https://github.com/kexuejin/TunnelMux) — it helps more developers find it.
