#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Check the built documentation site.

1. Privacy: no page loads anything from another host (scripts, styles, fonts,
   images, frames, CSS url() or @import), and no page or asset mentions a known
   analytics or tracking service. Every http(s) address in a page, link or
   not, must be on an allowed host: github.com, ag064.eu or the project's
   Pages host.
2. Links: every relative link points at a file in the site, and every #fragment
   at an id in that page. Links to this repository on GitHub
   (github.com/AG064/argentum/blob|tree|edit/development/PATH) must name a path
   that exists in the checkout. Other external links are not fetched here.

Usage: python scripts/check-docs-site.py docs/site/book
"""
from __future__ import annotations

import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

REPO_ROOT = Path(__file__).resolve().parent.parent
REPO_LINK = re.compile(
    r"^https://github\.com/AG064/argentum/(?:blob|tree|edit)/development/([^#?]*)"
)
TRACKERS = re.compile(
    r"google-analytics|googletagmanager|gtag\(|plausible\.io|matomo|piwik|"
    r"hotjar|segment\.(?:com|io)|mixpanel|clarity\.ms|cloudflareinsights|"
    r"umami|posthog|fathom|statcounter|doubleclick|facebook\.net|"
    r"fonts\.googleapis|fonts\.gstatic|use\.typekit|cdnjs|jsdelivr|unpkg",
    re.IGNORECASE,
)
ALLOWED_HOSTS = {"github.com", "ag064.eu", "ag064.github.io"}
ADDRESS = re.compile(r"""https?://([A-Za-z0-9.-]+\.[A-Za-z]{2,})""")
CSS_REMOTE = re.compile(r"""(?:url\(\s*['"]?|@import\s+['"]?)\s*(?:https?:)?//""", re.I)
# Tags whose attribute makes the browser fetch something on page load.
LOADING = {
    "script": ("src",),
    "link": ("href",),
    "img": ("src", "srcset"),
    "iframe": ("src",),
    "source": ("src", "srcset"),
    "video": ("src", "poster"),
    "audio": ("src",),
    "embed": ("src",),
    "object": ("data",),
    "use": ("href", "xlink:href"),
}


def is_remote(value: str) -> bool:
    value = value.strip().lower()
    return value.startswith(("http:", "https:", "//"))


class Page(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.ids: set[str] = set()
        self.links: list[str] = []
        self.loads: list[tuple[str, str]] = []
        self.inline_styles: list[str] = []

    def handle_starttag(self, tag, attrs):
        attrs = {k: (v or "") for k, v in attrs}
        for key in ("id", "name"):
            if attrs.get(key):
                self.ids.add(attrs[key])
        if tag == "a" and "href" in attrs:
            self.links.append(attrs["href"])
        for key in LOADING.get(tag, ()):
            if key in attrs:
                # rel="canonical" and similar do not load anything, but there are
                # none in this site, so every link href is treated as a load.
                for part in attrs[key].split(","):
                    self.loads.append((tag, part.strip().split(" ")[0]))
        if "style" in attrs:
            self.inline_styles.append(attrs["style"])


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    site = Path(sys.argv[1]).resolve()
    if not (site / "index.html").is_file():
        print(f"error: {site} has no index.html; build the site first")
        return 2

    errors: list[str] = []
    pages: dict[Path, Page] = {}
    for path in sorted(site.rglob("*.html")):
        page = Page()
        text = path.read_text(encoding="utf-8")
        page.feed(text)
        pages[path] = page
        rel = path.relative_to(site)
        for tag, value in page.loads:
            if is_remote(value):
                errors.append(f"{rel}: <{tag}> loads from another host: {value}")
        for style in page.inline_styles:
            if CSS_REMOTE.search(style):
                errors.append(f"{rel}: inline style loads from another host")
        for match in re.finditer(r"<style[^>]*>(.*?)</style>", text, re.S | re.I):
            if CSS_REMOTE.search(match.group(1)):
                errors.append(f"{rel}: <style> loads from another host")
        for match in ADDRESS.finditer(text):
            host = match.group(1).lower().rstrip(".")
            if host not in ALLOWED_HOSTS and not host.endswith(".ag064.eu"):
                errors.append(f"{rel}: address on a host that is not allowed: {host}")
        if TRACKERS.search(text):
            errors.append(f"{rel}: mentions a tracking or third-party asset host: "
                          f"{TRACKERS.search(text).group(0)}")

    for path in sorted(site.rglob("*")):
        if path.suffix.lower() in {".css", ".js"} and path.is_file():
            text = path.read_text(encoding="utf-8", errors="replace")
            rel = path.relative_to(site)
            if path.suffix.lower() == ".css" and CSS_REMOTE.search(text):
                errors.append(f"{rel}: stylesheet loads from another host")
            hit = TRACKERS.search(text)
            if hit:
                errors.append(f"{rel}: mentions {hit.group(0)}")

    checked = 0
    for path, page in pages.items():
        rel = path.relative_to(site)
        for href in page.links:
            if not href or href.startswith(("mailto:", "javascript:")):
                continue
            repo = REPO_LINK.match(href)
            if repo:
                checked += 1
                target = REPO_ROOT / unquote(repo.group(1))
                if not target.exists():
                    errors.append(f"{rel}: repository link to a missing path: {href}")
                continue
            if is_remote(href):
                continue
            checked += 1
            parts = urlsplit(href)
            if parts.path:
                target = (path.parent / unquote(parts.path)).resolve()
                if target.is_dir():
                    target = target / "index.html"
            else:
                target = path
            if not target.exists():
                errors.append(f"{rel}: broken link: {href}")
                continue
            if parts.fragment and target.suffix == ".html":
                ids = pages[target].ids if target in pages else set()
                if unquote(parts.fragment) not in ids:
                    errors.append(f"{rel}: missing anchor: {href}")

    for error in errors:
        print(f"error: {error}")
    print(f"{len(pages)} pages, {checked} links checked, {len(errors)} problems")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
