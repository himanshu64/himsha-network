# HIMSHA Network — Website Handoff

Marketing site for **HIMSHA Network** (educational, open-source, ZK-proven Bitcoin
programmability layer). Static HTML/CSS/JS — no build step required to run, one
optional Python script to regenerate the docs pages.

> **Status:** complete and verified at desktop + mobile. Ships as static files —
> deploy the `website/` directory as-is (GitHub Pages, Netlify, any static host).

---

## 1. Before you launch (required) ✅

Two find-and-replace passes + two tokens:

1. **Domain / org** — replace `your-org.github.io/himsha-network` and the repo slug
   `your-org/himsha-network` with your real values. They appear in:
   `index.html`, `tutorials.html`, `bitcoin-indexer-docker.html`,
   `bitcoin-indexer-k8s.html`, `robots.txt`, `sitemap.xml`, `llms.txt`,
   `assets/js/main.js` (the Ask form `data-repo`), and `build-docs.py`.
   ```bash
   cd website
   grep -rl 'your-org' . | xargs sed -i '' 's#your-org#YOUR_GH_ORG#g'   # macOS sed
   python3 build-docs.py   # regenerate the two doc pages after the change
   ```
2. **Search Console** — in `index.html`, replace
   `REPLACE_WITH_GOOGLE_SEARCH_CONSOLE_TOKEN` (and optionally
   `REPLACE_WITH_BING_TOKEN`) with the tokens from Google Search Console /
   Bing Webmaster Tools. (Or verify by DNS and delete the meta tags.)
3. Submit `sitemap.xml` in Search Console.

---

## 2. Run / preview locally

The site works opened directly as a **file://** URL (i18n dictionaries are embedded
in JS, no fetch needed). To mirror production exactly, serve over HTTP:

```bash
cd website
python3 -m http.server 8000
# → http://localhost:8000/index.html
# language test: http://localhost:8000/index.html?lang=ja
```

---

## 3. File map

```
website/
├── index.html                     # Landing page (nav, hero, why, how, programs,
│                                   #   use-cases, sdks, quickstart, docs, FAQ, ask, cta)
├── tutorials.html                 # Hand-written tutorials (sidebar + content)
├── bitcoin-indexer-docker.html    # GENERATED from ../docs/bitcoin-indexer-docker.md
├── bitcoin-indexer-k8s.html       # GENERATED from ../docs/bitcoin-indexer-k8s-terraform.md
├── build-docs.py                  # Renders the two docs above (run after editing the .md)
├── robots.txt                     # AI crawlers allowed + sitemap ref
├── sitemap.xml                    # incl. hreflang alternates for 10 locales
├── llms.txt                       # GEO: factual project summary for LLMs/answer engines
└── assets/
    ├── css/
    │   ├── main.css               # Design system + all components + motion (shared)
    │   ├── tutorials.css          # tutorials-only layout
    │   └── docs.css               # generated-docs layout (sticky TOC, prose)
    ├── js/
    │   ├── main.js                # nav, mobile menu, SDK tabs+indicator, particles,
    │   │                          #   scroll-reveal, Ask form, stat count-up, progress bar
    │   ├── i18n.js                # i18n engine + EMBEDDED dictionaries (source of truth)
    │   ├── docs.js                # docs TOC scroll-spy + mobile toggle
    │   └── tutorials.js           # tutorials tabs
    ├── i18n/*.json                # 10 locale files — REFERENCE COPIES (see §6)
    └── img/                       # favicons, logo, apple-touch-icon
```

`tutorials.html` and the generated docs pages share `assets/css/main.css`
(tokens + nav + buttons + code blocks), so design changes there propagate everywhere.

---

## 4. Design system (in `main.css :root`)

- **Brand:** Bitcoin orange `--orange #F7931A` / `--orange-2 #FFB347` on near-black.
- **Type:** Inter (body) · JetBrains Mono (code/labels) · **Orbitron** (`--font-display`,
  the "robotic" face) on headings, the HIMSHA wordmark, and stat numbers.
- **Motion curves:** `--ease-out` (enter/exit), `--ease-in-out` (on-screen movement),
  `--ease-drawer`. Use these, not the weak built-in easings.
- **Surfaces/lines/shadows/radii:** all tokenized — never hardcode hex in components.

---

## 5. What's built

| Area | Notes |
| --- | --- |
| **Landing redesign** | Swiss/technical, SVG icons (no emoji), two-column hero with a live "ZK receipt → VERIFIED" card |
| **Docs pages** | `bitcoin-indexer-docker.html` + `-k8s.html` rendered from `../docs/*.md` |
| **Use Cases** | Bento grid; each card tagged with the programs it composes |
| **FAQ (AEO)** | Visible accordion mirrored by `FAQPage` JSON-LD |
| **SEO** | canonical, robots, OG, Twitter cards, `robots.txt`, `sitemap.xml` |
| **GEO** | `llms.txt` + Organization/WebSite/SoftwareSourceCode JSON-LD |
| **i18n** | 10 locales (en, es, fr, de, zh-Hans, ja, hi, te, ta, kn) + switcher + hreflang |
| **Ask a question** | Opens a prefilled GitHub Discussion (Q&A) or Issue (`question` label) — no backend |
| **Motion** | Hero stagger, press feedback, scroll-stagger, glow/shimmer, SDK sliding indicator, stat count-up, scroll progress bar — all gated by `prefers-reduced-motion` |
| **Mobile header** | Right-aligned hamburger → X morph, animated menu, full-width Get Started CTA, body-scroll lock + Escape/resize close |

---

## 6. i18n — how it works & how to edit

- **Source of truth = `assets/js/i18n.js`** (the embedded `DICTS` object). It is read
  directly, so it works on `file://`. The `assets/i18n/*.json` files are reference
  copies for tooling — **if you change a string, update `i18n.js`** (and the JSON if you
  want them in sync).
- Translatable elements carry `data-i18n="key"`. English is the in-HTML baseline;
  switching to another locale swaps `textContent`, `<title>`, meta description, and
  `<html lang>`. Choice persists in `localStorage` + `?lang=` URL.
- **Add a string:** add `data-i18n="x.y"` in `index.html`, add `"x.y": "..."` to every
  locale in `DICTS`.
- **Add a language:** add `{code,name}` to `LOCALES` and a `DICTS[code]` block in
  `i18n.js`; add an `hreflang` line to `index.html` head + `sitemap.xml`.
- Only the marketing chrome is localized (nav, hero, section titles, FAQ heading, Ask,
  CTA). Dense technical body copy stays English by design.

---

## 7. Regenerating the docs pages

The two `bitcoin-indexer-*.html` files are **generated** — don't hand-edit them.
Edit the markdown in `../docs/`, then:

```bash
cd website && python3 build-docs.py
```

Requires Python `markdown` (3.10+). The script injects the shared nav, a sticky TOC
from the `##` headings, SEO meta, and the prose layout.

---

## 8. Notes & gotchas

- **Headless screenshots:** Chrome headless clamps the viewport to ~500px min width, so
  "390px" captures are really 500px crops — test true small screens on a device.
- **Reduced motion:** every animation collapses to instant/none under
  `prefers-reduced-motion: reduce`; keep that intact when adding motion.
- **Marketing:** `../marketing/twitter-60-day-content.md` holds the brand kit + a
  Day 1–60 X/Twitter calendar (swap the placeholder `@HIMSHAnetwork` handle).

---

## 9. Suggested next steps

- Replace placeholders (§1) and deploy.
- Add a real `og:image` / Twitter card image (1200×630) instead of the touch icon.
- Optional: swipe-to-dismiss on the mobile menu (velocity-based).
- Optional: localize the FAQ answers (currently English).
