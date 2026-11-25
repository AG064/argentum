# About this site

## No tracking

This site does not track you. It has:

- no analytics, counters or tracking pixels
- no cookies
- no fonts, scripts, styles or images loaded from other hosts

Everything the pages need is served from the same place as the pages. The search
box searches an index that is part of the site, in your browser. The only things
the site remembers are the color theme you pick and whether the sidebar is open,
in your browser's local storage. They never leave your device.

A check in CI fails the build if a page loads anything from another host,
mentions a known analytics service, or contains an address on any host other than
github.com, ag064.eu and this site's own host.

## How it is made

The pages are Markdown files in
[`docs/site/src`](https://github.com/AG064/argentum/tree/development/docs/site/src),
built with mdBook 0.5.4. The screenshots come from
`cargo run -p argentum-ui --example ui_snapshot`, which draws the interface
without opening a window.

To build the site yourself:

```sh
mdbook build docs/site
python scripts/check-docs-site.py docs/site/book
```

Corrections are welcome as a pull request. Every page has an edit link at the top.
