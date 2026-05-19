# Wikipedia Render Analysis
## Target: https://en.wikipedia.org/wiki/Rust_(programming_language)

Current status: page fetches and produces a screenshot (`tests/screenshots/wikipedia-rust.png`),
but the render is mostly bare text — the visual structure is missing because it all comes from
external CSS that Aurora does not yet load.

---

## What the page actually contains

Measured from a live fetch (588 KB HTML):

| Metric | Count |
|---|---|
| HTML bytes | 588,864 |
| `<span>` elements | 3,725 |
| `<a>` links | 1,908 |
| `<sup>` footnote markers | 385 |
| `<code>` inline code | 175 |
| `<li>` list items | 672 |
| `<pre>` code blocks | 29 |
| `<h2>` / `<h3>` sections | 13 / 29 |
| `<table>` elements | 13 |
| `<img>` elements | 28 |
| External stylesheets | 2 (MediaWiki bundles) |
| External scripts | 1 (MediaWiki startup) |

---

## External CSS — the main blocker

Wikipedia loads its entire visual design from two bundled `load.php` URLs:

```
/w/load.php?modules=ext.cite.styles|ext.pygments|skins.vector.icons,styles|skins.vector.search.codex.styles|...&only=styles&skin=vector-2022
/w/load.php?modules=site.styles&only=styles&skin=vector-2022
```

These are dynamically generated CSS bundles containing hundreds of rules for layout,
typography, colors, icons, and component styles. Without them, every element falls back
to browser defaults — which is exactly what our current screenshot shows.

**This is the single highest-priority fix.** Almost every other visual deficiency is a
consequence of external CSS not being applied.

---

## CSS features required (by priority)

### P0 — Without these, the page looks broken

| Feature | Where used | Aurora status |
|---|---|---|
| External CSS fetching + applying | MediaWiki skin | Not implemented |
| `display: none` | Collapsed nav, hidden elements (15 inline) | Partially works |
| `float: left / right` | Infobox, image thumbnails (3 inline) | Not implemented |
| `display: table / table-cell` | Infobox, data tables (3 inline) | Not implemented |
| `border-spacing` | All tables (11 inline) | Not implemented |
| `<sup>` vertical alignment | 385 footnote markers | Not implemented |
| `<pre>` / `<code>` block styling | 29 code blocks, 175 inline | Parsing ok, styling missing |

### P1 — Visual structure

| Feature | Where used | Aurora status |
|---|---|---|
| `display: flex` | Navigation, search bar (5 inline) | Taffy supports flex — CSS class rules missing |
| `max-width` | Content column, infobox (3 inline) | Not implemented |
| CSS custom properties (`--mw-file-upright`) | File sizing | Not implemented |
| `position: sticky` | TOC sidebar | Not implemented |
| `position: absolute` | Dropdowns, tooltips (1 inline) | Not implemented |
| `background-color` on table headers | Infobox header row | Partially works (inline only) |
| `white-space: nowrap` | Nav links (1 inline) | Not implemented |

### P2 — Polish

| Feature | Where used | Aurora status |
|---|---|---|
| `border-radius` | Buttons, infobox | Not implemented |
| `box-shadow` | Cards, modals | Not implemented |
| `font-size` percentages / `em` on nested elements | Body text scaling | Partially works |
| `line-height` | Body text | Partially works |
| Pseudo-elements `::before` / `::after` | Icons, bullets | Not implemented |
| `background-image: url()` | Vector skin icons | Not implemented |

---

## Image loading — Wikimedia rate limiting

All 28 images returned HTTP 429 in our test run. Wikimedia rate-limits user agents
that don't look like real browsers. Images fall into two categories:

**Protocol-relative URLs** — `//upload.wikimedia.org/...` — these need to be resolved
as `https://`, not `http://`. The fetch module needs to handle `//`-prefixed URLs as
HTTPS (they currently resolve relative to the page URL which is also HTTPS, so this
may already work — needs verification).

**User-Agent** — Aurora sends `Mozilla/5.0... Aurora/0.1`. Wikimedia may reject non-browser
agents. A closer browser UA string or adding a `Referer: https://en.wikipedia.org/` header
may help.

**SVG images** — Wikipedia inlines many SVG references as pre-rasterised PNGs via their
CDN (e.g. `250px-Rust_programming_language_black_logo.svg.png`). These are plain PNG
responses, so no SVG rendering is required for the thumbnails.

---

## HTML features that need work

### `<sup>` — footnote superscripts
385 occurrences. Currently rendered inline at normal baseline. Needs vertical-align and
font-size-reduction (typically `font-size: 0.75em; vertical-align: super`).

### `<pre>` code blocks
29 blocks. Wikipedia uses ext.pygments for syntax highlighting — each token gets a
`<span class="...">` with a color class defined in the external CSS. Without the CSS
these appear as plain monospace text with no colors, but still readable.

### `<cite>` / `<bdi>`
`<cite>` (141 occurrences) and `<bdi>` (17) are parsed but their semantic styling
(italic for cite, bidi isolation for bdi) depends on external CSS or UA stylesheet.

### Definition lists `<dl>` / `<dt>` / `<dd>`
Used in the infobox. Need indentation and layout rules.

### `<figure>` / `<figcaption>`
Image thumbnails use `<figure>` with a float and `<figcaption>` below. Needs float
layout plus block formatting after.

---

## Table layout — infobox and navboxes

Wikipedia has 13 tables. The key ones:

**Infobox** (`class="infobox vcard"`) — floated right, fixed-width, two-column key/value
layout. Requires:
- `float: right`
- `display: table` with `table-cell` columns
- `border-spacing: 3px`
- `vertical-align: top` on cells
- Background color on header rows

**Navboxes** (`class="navbox"`) — collapsible navigation tables at the bottom. These are
styled entirely by external CSS classes. Currently show as unstyled `<table>` elements.
Their collapse/expand behavior requires JS (Boa integration) plus `display:none` toggling.

---

## JavaScript

One script loaded: the MediaWiki startup module. It lazy-loads the rest of JS on demand.
The main features that need JS:
- Collapsible navboxes and sections
- Table of contents highlighting on scroll
- References preview on hover

Boa currently executes inline scripts. The MediaWiki module system is complex and unlikely
to work fully, but the base page content renders without JS.

---

## What a "good enough" Wikipedia render needs (ordered roadmap)

```
1. External CSS loading
   - Fetch linked <link rel="stylesheet"> URLs
   - Parse and apply the rules (the entire MediaWiki skin)

2. Float layout
   - float: left / right on block elements
   - Clear floats (overflow: hidden on parent, or explicit <br clear>)

3. CSS table display
   - display: table / table-row / table-cell
   - border-spacing, vertical-align on cells

4. <sup> / <sub> rendering
   - vertical-align: super/sub
   - font-size reduction

5. max-width / min-width in layout
   - Content column is max-width: 960px centred

6. CSS custom properties (basic --var: value; and var(--var) resolution)

7. Wikimedia image fetching
   - Add Referer header or closer UA string
   - Handle protocol-relative URLs (//upload.wikimedia.org)

8. position: absolute (tooltips, dropdowns)
   position: sticky (TOC sidebar)

9. Pseudo-elements ::before / ::after
   - MediaWiki uses these for bullet styles, icons, cite brackets

10. background-image: url() for icon sprites
```

---

## Current screenshot

`tests/screenshots/wikipedia-rust.png` (1440×900, 55 KB)

The small file size (55 KB vs the ~300 KB expected for a styled page) confirms that the
render is mostly white background with unstyled text — headings and paragraphs are present
but all MediaWiki visual structure is absent. This is the expected baseline before external
CSS is implemented.
