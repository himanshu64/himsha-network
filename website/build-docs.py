#!/usr/bin/env python3
"""Render HIMSHA Network markdown docs into styled HTML pages matching the site.

Usage: python3 build-docs.py
Reads ../docs/*.md and writes <name>.html into this directory.
"""
import html
import re
from pathlib import Path

import markdown

HERE = Path(__file__).resolve().parent
DOCS = HERE.parent / "docs"

# (source markdown, output html, eyebrow label)
PAGES = [
    ("bitcoin-indexer-docker.md", "bitcoin-indexer-docker.html", "Infrastructure · Docker"),
    ("bitcoin-indexer-k8s-terraform.md", "bitcoin-indexer-k8s.html", "Infrastructure · Kubernetes"),
]

NAV = """<nav class="nav" id="nav">
  <div class="nav-inner">
    <a class="nav-logo" href="index.html" aria-label="HIMSHA Network home">
      <svg width="28" height="28" viewBox="0 0 32 32" fill="none" aria-hidden="true">
        <defs><linearGradient id="navHimsha" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#F7931A"/><stop offset="1" stop-color="#FFB347"/></linearGradient></defs>
        <rect width="32" height="32" rx="7" fill="url(#navHimsha)"/>
        <rect x="8.4" y="8" width="3.3" height="16" rx="1.3" fill="#0a0a0b"/>
        <rect x="20.3" y="8" width="3.3" height="16" rx="1.3" fill="#0a0a0b"/>
        <rect x="8.4" y="13.2" width="15.2" height="2.5" rx="1" fill="#0a0a0b"/>
        <rect x="8.4" y="17.1" width="15.2" height="2.5" rx="1" fill="#0a0a0b"/>
      </svg>
      <span>HIMSHA</span>
    </a>
    <ul class="nav-links">
      <li><a href="index.html#features">Why HIMSHA</a></li>
      <li><a href="index.html#how-it-works">How It Works</a></li>
      <li><a href="index.html#programs">Programs</a></li>
      <li><a href="index.html#use-cases">Use Cases</a></li>
      <li><a href="index.html#sdks">SDKs</a></li>
      <li><a href="tutorials.html">Tutorials</a></li>
      <li><a href="index.html#docs">Docs</a></li>
    </ul>
    <div class="nav-cta">
      <a class="btn btn-ghost" href="https://github.com/your-org/himsha-network" target="_blank" rel="noopener">
        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/></svg>
        GitHub
      </a>
    </div>
    <button class="nav-hamburger" id="hamburger" aria-label="Open menu" aria-expanded="false">
      <span></span><span></span><span></span>
    </button>
  </div>
</nav>

<div class="mobile-menu" id="mobileMenu">
  <a href="index.html#features" class="mobile-link">Why HIMSHA</a>
  <a href="index.html#how-it-works" class="mobile-link">How It Works</a>
  <a href="index.html#programs" class="mobile-link">Programs</a>
  <a href="index.html#use-cases" class="mobile-link">Use Cases</a>
  <a href="index.html#sdks" class="mobile-link">SDKs</a>
  <a href="tutorials.html" class="mobile-link">Tutorials</a>
  <a href="index.html#docs" class="mobile-link">Docs</a>
  <a href="https://github.com/your-org/himsha-network" class="mobile-link" target="_blank" rel="noopener">GitHub &#8599;</a>
</div>"""

ARROW = ('<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" '
         'stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="12" x2="5" y2="12"/>'
         '<polyline points="12 19 5 12 12 5"/></svg>')
MENU_ICON = ('<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" '
             'stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"/>'
             '<line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="18" x2="21" y2="18"/></svg>')

PAGE_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta name="description" content="{desc}" />
  <title>{title} — HIMSHA Network</title>
  <link rel="canonical" href="https://your-org.github.io/himsha-network/{out}" />
  <meta name="robots" content="index, follow, max-image-preview:large" />
  <meta property="og:type" content="article" />
  <meta property="og:site_name" content="HIMSHA Network" />
  <meta property="og:title" content="{title} — HIMSHA Network" />
  <meta property="og:description" content="{desc}" />
  <meta property="og:url" content="https://your-org.github.io/himsha-network/{out}" />
  <meta name="twitter:card" content="summary_large_image" />
  <link rel="icon" type="image/svg+xml" href="assets/img/favicon.svg" />
  <link rel="icon" type="image/png" sizes="32x32" href="assets/img/favicon-32.png" />
  <link rel="icon" type="image/png" sizes="16x16" href="assets/img/favicon-16.png" />
  <link rel="shortcut icon" href="assets/img/favicon.ico" />
  <link rel="apple-touch-icon" sizes="180x180" href="assets/img/apple-touch-icon.png" />
  <meta name="theme-color" content="#F7931A" />
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700;800;900&family=JetBrains+Mono:wght@400;500;600;700&family=Orbitron:wght@400;500;600;700;800;900&display=swap" rel="stylesheet" />
  <link rel="stylesheet" href="assets/css/main.css" />
  <link rel="stylesheet" href="assets/css/docs.css" />
</head>
<body>

{nav}

<button class="doc-toc-toggle" id="tocToggle" aria-expanded="false">
  <span>On this page</span>
  {menu_icon}
</button>

<main class="doc-layout">
  <aside class="doc-sidebar" id="docSidebar">
    <a class="doc-back" href="index.html#docs">{arrow} Back to docs</a>
    <div class="doc-sidebar-head">On this page</div>
    <nav aria-label="Table of contents">
{toc}
    </nav>
  </aside>

  <div class="doc-content">
    <article class="doc-prose">
      <div class="doc-eyebrow">{eyebrow}</div>
{body}
    </article>
  </div>
</main>

<script src="assets/js/main.js"></script>
<script src="assets/js/docs.js"></script>
</body>
</html>
"""


def build_toc(toc_tokens):
    """Build sidebar links from level-2 headings (walking the heading tree)."""
    links = []

    def walk(tokens):
        for tok in tokens:
            if tok["level"] == 2:
                # toc_tokens["name"] is already HTML-escaped by the markdown toc extension
                name = tok["name"]
                links.append(f'      <a class="doc-toc-link" href="#{tok["id"]}">{name}</a>')
            walk(tok.get("children", []))

    walk(toc_tokens)
    return "\n".join(links)


def convert(md_text):
    md = markdown.Markdown(
        extensions=["extra", "toc", "sane_lists"],
        extension_configs={"toc": {"permalink": False}},
    )
    body = md.convert(md_text)
    return body, md.toc_tokens


def main():
    for src, out, eyebrow in PAGES:
        src_path = DOCS / src
        text = src_path.read_text(encoding="utf-8")

        # Pull the first H1 as the page title, strip it from body (rendered separately? keep in body)
        m = re.search(r"^#\s+(.+)$", text, re.MULTILINE)
        title = m.group(1).strip() if m else out
        # markdown strips trailing "—" weirdness fine
        title_plain = re.sub(r"\s+", " ", title)

        body, toc_tokens = convert(text)
        toc = build_toc(toc_tokens)

        page = PAGE_TEMPLATE.format(
            out=out,
            desc=html.escape(f"{title_plain} — HIMSHA Network documentation."),
            title=html.escape(title_plain),
            nav=NAV,
            menu_icon=MENU_ICON,
            arrow=ARROW,
            eyebrow=html.escape(eyebrow),
            toc=toc,
            body=body,
        )
        (HERE / out).write_text(page, encoding="utf-8")
        print(f"wrote {out}  ({len(body)} bytes html, {len(toc_tokens)} toc roots)")


if __name__ == "__main__":
    main()
