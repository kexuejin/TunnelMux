# TunnelMux — SEO & discovery guide

Run this checklist to make TunnelMux findable in Google / Bing search.

## Google Search Console

1. Open https://search.google.com/search-console and add a **URL prefix** property:
   `https://kexuejin.github.io/TunnelMux/`
2. Verify ownership with the **HTML file** method:
   - Download the `google<hash>.html` file that Google gives you.
   - Drop it into `docs/` (MkDocs copies it to the site root, so it will be served at
     `https://kexuejin.github.io/TunnelMux/google<hash>.html`).
   - Commit and push; the Docs workflow redeploys in about a minute.
   - Click **Verify** in Search Console.
3. After verification, open **Sitemaps** and submit:
   `https://kexuejin.github.io/TunnelMux/sitemap.xml`
4. Use **URL Inspection** on `https://kexuejin.github.io/TunnelMux/` and click **Request indexing**.

## Bing Webmaster Tools

1. Add the site at https://www.bing.com/webmasters.
2. Verify (file or meta-tag method — same idea as Google).
3. Submit the same sitemap URL.

## Status

- repo description + 20 topics — done
- README TOC + FAQ (People-Also-Ask style) — done
- social preview (.github/social-preview.png) — done
- docs site (MkDocs Material) + auto sitemap + robots.txt — done
- submit sitemap to Search Console / Bing — needs your Google / Microsoft account
- backlinks from dev.to / 掘金 / Show HN / Reddit posts linking the repo + docs site — launch posts (see promo/)
- release cadence + green CI (freshness signal) — v0.3.0 live, CI green
- optional: full Chinese docs site under /zh/ with its own sitemap entries
