//! Browser chrome authored as a Dioxus component and rendered to HTML.
//!
//! This is the single source of truth for the chrome's structure and styling.
//! `chrome_html()` runs the component through `dioxus-ssr` to produce an HTML
//! document; both render paths consume that same HTML:
//!   - the live window parses it with Blitz and paints to a Vello scene,
//!   - the screenshot path parses it with Aurora's own layout engine and
//!     rasterizes it to an image.
//!
//! The layout is expressed with absolute positioning (no flexbox) so the two
//! independent layout engines agree on element placement.

use dioxus::prelude::*;

use super::display::chrome_display_url;
use crate::dom::{Node, NodePtr};
use crate::identity::Identity;
use crate::window::BROWSER_CHROME_HEIGHT;

/// Live node counts harvested from the real DOM.
/// This is what the chrome's "telemetry" fields report — nothing is faked.
struct DomTelemetry {
    total: usize,
    elements: usize,
    text: usize,
}

/// Walk the real DOM (light children + parsed `<template>` contents) and tally
/// node counts.
fn dom_telemetry(root: &NodePtr) -> DomTelemetry {
    fn walk(node: &NodePtr, t: &mut DomTelemetry) {
        let b = node.borrow();
        match &*b {
            Node::Document { children, .. } => {
                t.total += 1;
                for c in children {
                    walk(c, t);
                }
            }
            Node::Element(el) => {
                t.total += 1;
                t.elements += 1;
                for c in &el.children {
                    walk(c, t);
                }
                if let Some(tc) = &el.template_contents {
                    walk(tc, t);
                }
            }
            Node::Comment(_) => {
                t.total += 1;
            }
            Node::Text(_) => {
                t.total += 1;
                t.text += 1;
            }
        }
    }
    let mut t = DomTelemetry {
        total: 0,
        elements: 0,
        text: 0,
    };
    walk(root, &mut t);
    t
}

/// First non-empty `<title>` text, searched depth-first with early exit so
/// labelling background tabs doesn't walk their entire (possibly huge) DOMs.
fn page_title(root: &NodePtr) -> Option<String> {
    fn walk(node: &NodePtr, in_title: bool) -> Option<String> {
        let b = node.borrow();
        match &*b {
            Node::Document { children, .. } => children.iter().find_map(|c| walk(c, false)),
            Node::Element(el) => {
                let is_title = el.tag_name.eq_ignore_ascii_case("title");
                el.children
                    .iter()
                    .find_map(|c| walk(c, is_title))
                    .or_else(|| el.template_contents.as_ref().and_then(|tc| walk(tc, false)))
            }
            Node::Text(txt) if in_title => {
                let s = txt.content.trim();
                (!s.is_empty()).then(|| s.to_string())
            }
            _ => None,
        }
    }
    walk(root, false)
}

/// Tab-strip label for a page: its `<title>`, else its URL host.
pub(in crate::window) fn tab_label(url: &str, dom: &NodePtr) -> String {
    page_title(dom)
        .map(|s| truncate(&s, TAB_LABEL_MAX_CHARS))
        .unwrap_or_else(|| truncate(&url_host(url), TAB_LABEL_MAX_CHARS))
}

/// Resident set size in MB, read live from `/proc/self/statm` (Linux).
fn rss_mb() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page_size = 4096u64;
    Some(resident_pages * page_size / (1024 * 1024))
}

fn truncate(s: &str, max: usize) -> String {
    let mut out: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        out.push('…');
    }
    out
}

/// Host portion of a URL, for a tab label when the page has no `<title>`.
fn url_host(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// Tab-strip geometry, shared between the generated CSS and click hit-testing
/// so the strip that is painted and the strip that responds to clicks can never
/// drift apart. All values are CSS pixels within the chrome band.
const TAB_STRIP_LEFT: f64 = 150.0;
const TAB_WIDTH: f64 = 136.0;
const TAB_GAP: f64 = 6.0;
const TAB_TOP: f64 = 7.0;
const TAB_HEIGHT: f64 = 22.0;
/// Right-edge slice of each tab that acts as its close button.
const TAB_CLOSE_WIDTH: f64 = 18.0;
const NEW_TAB_WIDTH: f64 = 22.0;
const TAB_LABEL_MAX_CHARS: usize = 14;

/// URL bar geometry (row 2). Like the tab strip, these constants feed both the
/// generated CSS and the click hit-test.
const URLBAR_LEFT: f64 = 96.0;
const URLBAR_TOP: f64 = 42.0;
const URLBAR_HEIGHT: f64 = 24.0;
/// Space reserved right of the URL bar for the diagnostics readout.
const URLBAR_RIGHT_INSET: f64 = 180.0;

/// What a click inside the chrome band landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window) enum ChromeHit {
    ActivateTab(usize),
    CloseTab(usize),
    NewTab,
    UrlBar,
}

/// Hit-test a click at chrome-local coordinates against the tab strip and URL
/// bar. `width` is the window width, which fixes the URL bar's right edge.
pub(in crate::window) fn chrome_hit_test(
    x: f64,
    y: f64,
    tab_count: usize,
    width: f64,
) -> Option<ChromeHit> {
    if (URLBAR_TOP..URLBAR_TOP + URLBAR_HEIGHT).contains(&y)
        && (URLBAR_LEFT..(width - URLBAR_RIGHT_INSET).max(URLBAR_LEFT)).contains(&x)
    {
        return Some(ChromeHit::UrlBar);
    }
    if !(TAB_TOP..TAB_TOP + TAB_HEIGHT).contains(&y) {
        return None;
    }
    for i in 0..tab_count {
        let left = TAB_STRIP_LEFT + i as f64 * (TAB_WIDTH + TAB_GAP);
        if (left..left + TAB_WIDTH).contains(&x) {
            return Some(if x >= left + TAB_WIDTH - TAB_CLOSE_WIDTH {
                ChromeHit::CloseTab(i)
            } else {
                ChromeHit::ActivateTab(i)
            });
        }
    }
    let plus_left = TAB_STRIP_LEFT + tab_count as f64 * (TAB_WIDTH + TAB_GAP);
    if (plus_left..plus_left + NEW_TAB_WIDTH).contains(&x) {
        return Some(ChromeHit::NewTab);
    }
    None
}

/// Data model for the chrome. Pulled out of the old hand-coded painters so the
/// chrome's content is now data-driven rather than baked into draw calls.
#[derive(Props, Clone, PartialEq)]
pub struct ChromeProps {
    pub url: String,
    pub version: String,
    pub tabs: Vec<String>,
    pub active_tab: usize,
    pub diagnostics: String,
    pub identity_initials: String,
    /// `Some(buffer)` while the address bar has keyboard focus; the buffer is
    /// what the user has typed so far and is rendered in place of `url`.
    pub url_edit: Option<String>,
}

impl ChromeProps {
    /// Build single-tab props from the real render state (the screenshot path,
    /// which has no tab strip state). Node counts come from walking the live
    /// DOM and the identity chip from the active `Identity`.
    pub fn from_render_state(url: &str, dom: &NodePtr, identity: &Identity) -> Self {
        Self::from_tabs(url, dom, identity, vec![tab_label(url, dom)], 0)
    }

    /// Build props for a live window with a full tab strip. `dom` is the active
    /// tab's document (telemetry reports the page being shown); `tabs` are the
    /// pre-computed labels for every open tab.
    pub fn from_tabs(
        url: &str,
        dom: &NodePtr,
        identity: &Identity,
        tabs: Vec<String>,
        active_tab: usize,
    ) -> Self {
        let t = dom_telemetry(dom);

        // Quiet, real engine-debug counter (no fabricated mem/gpu/tab figures).
        let diagnostics = format!("{} nodes · {} text", t.total, t.text);

        ChromeProps {
            url: chrome_display_url(url),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tabs,
            active_tab,
            diagnostics,
            identity_initials: identity_initials(&identity.name),
            url_edit: None,
        }
    }
}

/// Editing view of the address-bar buffer: a trailing slice plus a caret, so
/// the end of a long URL (where typing happens) stays visible in the bar.
fn edit_display(buffer: &str) -> String {
    const MAX: usize = 90;
    let chars: Vec<char> = buffer.chars().collect();
    if chars.len() > MAX {
        let tail: String = chars[chars.len() - MAX..].iter().collect();
        format!("…{tail}|")
    } else {
        format!("{buffer}|")
    }
}

/// Up-to-two-letter initials from a display name (word initials, else prefix).
fn identity_initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let initials: String = if words.len() >= 2 {
        words
            .iter()
            .take(2)
            .filter_map(|w| w.chars().next())
            .collect()
    } else {
        name.chars().take(2).collect()
    };
    initials.to_uppercase()
}

// `rsx!` expands interpolated text and attributes such as "{label}" into
// `format!` calls, which clippy flags as useless. That is the idiomatic
// Dioxus spelling, so silence the lint here rather than rewrite the markup.
#[allow(non_snake_case, clippy::useless_format)]
fn Chrome(props: ChromeProps) -> Element {
    // Focused/idle address-bar classes share no properties (like the tab
    // strip) so neither render path depends on cascade order.
    let (urlbar_class, url_class, url_text) = match &props.url_edit {
        Some(buffer) => ("urlbar urlbar-on", "url url-edit", edit_display(buffer)),
        None => ("urlbar urlbar-off", "url url-view", props.url.clone()),
    };
    rsx! {
        div { class: "chrome",
            // ── row 1: brand + tab strip ............ engine · identity ──
            div { class: "logo-box" }
            span { class: "brand", "AURORA" }
            span { class: "ver", "{props.version}" }
            for (i, label) in props.tabs.iter().enumerate() {
                span {
                    class: if i == props.active_tab { "tab tab-on slot-{i}" } else { "tab tab-off slot-{i}" },
                    "{label}"
                }
                span { class: "tab-close close-slot-{i}", "×" }
            }
            span { class: "tab-new", "+" }
            span { class: "id", "{props.identity_initials}" }

            // ── row 2: nav + url ............ live counters ──
            span { class: "nav nav-back", "‹" }
            span { class: "nav nav-fwd", "›" }
            span { class: "nav nav-reload", "↻" }
            div { class: "{urlbar_class}" }
            span { class: "tls", "TLS" }
            span { class: "{url_class}", "{url_text}" }
            span { class: "diag", "{props.diagnostics}" }
        }
    }
}

/// CSS for the chrome. Two slim 36px bands; absolute positioning keeps Blitz and
/// Aurora's own (Taffy-backed) layout engine in agreement. One border weight,
/// one accent (pink), quiet greys for secondary/debug text.
const CHROME_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body { background: transparent; font-family: monospace; }
.chrome {
    position: absolute; left: 0; top: 0; right: 0; height: 72px;
    background: rgb(253,244,248);
    border-bottom: 1px solid rgb(240,214,225);
}
/* display:block keeps every chrome node out of inline flow so the Taffy-backed
   layout path (which supports absolute positioning) is selected over the legacy
   inline engine. */
.chrome span, .chrome div { position: absolute; display: block; }

/* row 1 ─ baseline at y=11, band 0-36 */
.logo-box { left: 14px; top: 12px; width: 12px; height: 12px; background: rgb(255,158,196); }
.brand { left: 34px; top: 11px; font-size: 13px; color: rgb(122,59,81); }
.ver { left: 112px; top: 13px; font-size: 11px; color: rgb(199,155,176); }
/* Tab strip. Positions (left/width) are generated per slot by `tab_slot_css`
   from the same constants the click hit-test uses. Active/inactive classes
   deliberately share no properties, so no cascade-order rules are needed. */
.tab { top: 7px; height: 22px; font-size: 12px; padding: 4px 0 0 12px; }
.tab-on { background: rgb(255,231,240); color: rgb(105,54,76); }
.tab-off { background: rgb(250,238,244); color: rgb(178,138,156); }
.tab-close { top: 11px; font-size: 11px; color: rgb(199,155,176); }
.tab-new { top: 10px; font-size: 14px; color: rgb(150,99,121); }
.id {
    right: 14px; top: 8px; height: 20px;
    background: rgb(255,158,196); color: rgb(255,255,255);
    font-size: 11px; padding: 3px 0 0 8px;
}

/* row 2 ─ baseline at y=46, band 36-72 */
.nav { top: 43px; font-size: 16px; color: rgb(150,99,121); }
.nav-back { left: 16px; }
.nav-fwd { left: 40px; }
.nav-reload { left: 64px; }
.urlbar { left: 96px; top: 42px; right: 180px; height: 24px; }
.urlbar-off { background: rgb(255,249,251); border: 1px solid rgb(240,214,225); }
.urlbar-on { background: rgb(255,255,255); border: 1px solid rgb(255,158,196); }
.tls {
    left: 106px; top: 46px; font-size: 10px; color: rgb(198,87,133);
}
.url { left: 146px; top: 45px; font-size: 13px; }
.url-view { color: rgb(138,100,117); }
.url-edit { color: rgb(105,54,76); }
.diag { right: 16px; top: 47px; width: 170px; font-size: 11px; color: rgb(184,160,173); }
"#;

/// Per-slot tab positions, generated as classes (rather than inline styles)
/// so both render paths only need class-based absolute positioning.
fn tab_slot_css(tab_count: usize) -> String {
    let mut css = String::new();
    for i in 0..tab_count {
        let left = TAB_STRIP_LEFT + i as f64 * (TAB_WIDTH + TAB_GAP);
        let close_left = left + TAB_WIDTH - TAB_CLOSE_WIDTH;
        css.push_str(&format!(
            ".slot-{i} {{ left: {left}px; width: {TAB_WIDTH}px; }}\n\
             .close-slot-{i} {{ left: {close_left}px; }}\n"
        ));
    }
    let plus_left = TAB_STRIP_LEFT + tab_count as f64 * (TAB_WIDTH + TAB_GAP) + 4.0;
    css.push_str(&format!(".tab-new {{ left: {plus_left}px; }}\n"));
    css
}

/// Render the chrome component to a complete HTML document string.
pub fn chrome_html(props: ChromeProps) -> String {
    let slot_css = tab_slot_css(props.tabs.len());
    let mut vdom = VirtualDom::new_with_props(Chrome, props);
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>{CHROME_CSS}{slot_css}</style></head><body>{body}</body></html>"
    )
}

/// The chrome's fixed height in CSS pixels.
pub const CHROME_HEIGHT: u32 = BROWSER_CHROME_HEIGHT as u32;

/// Live-window chrome renderer: parses the Dioxus-authored chrome HTML with
/// Blitz and paints it into the Vello scene. The parsed document is cached and
/// only rebuilt when the HTML or width actually changes (the chrome is static
/// across most frames), so steady-state frames just re-paint.
#[derive(Default)]
pub struct ChromeRenderer {
    cached_html: String,
    width: u32,
    doc: Option<crate::blitz_document::BlitzDocument>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{Capability, Identity, IdentityKind};

    const WIN_W: f64 = 1000.0;

    #[test]
    fn hit_test_resolves_tabs_close_buttons_and_new_tab() {
        let tab0_left = TAB_STRIP_LEFT;
        let tab1_left = TAB_STRIP_LEFT + TAB_WIDTH + TAB_GAP;
        let y = TAB_TOP + TAB_HEIGHT / 2.0;

        assert_eq!(
            chrome_hit_test(tab0_left + 10.0, y, 2, WIN_W),
            Some(ChromeHit::ActivateTab(0))
        );
        assert_eq!(
            chrome_hit_test(tab1_left + 10.0, y, 2, WIN_W),
            Some(ChromeHit::ActivateTab(1))
        );
        assert_eq!(
            chrome_hit_test(tab0_left + TAB_WIDTH - 5.0, y, 2, WIN_W),
            Some(ChromeHit::CloseTab(0))
        );
        assert_eq!(
            chrome_hit_test(tab1_left + TAB_WIDTH + TAB_GAP + 8.0, y, 2, WIN_W),
            Some(ChromeHit::NewTab)
        );
        // In the gap between tabs: nothing.
        assert_eq!(
            chrome_hit_test(tab0_left + TAB_WIDTH + 2.0, y, 2, WIN_W),
            None
        );
        // Outside the tab band vertically: nothing.
        assert_eq!(
            chrome_hit_test(tab0_left + 10.0, TAB_TOP + TAB_HEIGHT + 5.0, 2, WIN_W),
            None
        );
        // Left of the strip (brand area): nothing.
        assert_eq!(chrome_hit_test(50.0, y, 2, WIN_W), None);
    }

    #[test]
    fn hit_test_resolves_url_bar() {
        let y = URLBAR_TOP + URLBAR_HEIGHT / 2.0;

        assert_eq!(
            chrome_hit_test(URLBAR_LEFT + 40.0, y, 1, WIN_W),
            Some(ChromeHit::UrlBar)
        );
        // Left of the bar (nav buttons): nothing.
        assert_eq!(chrome_hit_test(URLBAR_LEFT - 10.0, y, 1, WIN_W), None);
        // Right of the bar, inside the diagnostics inset: nothing.
        assert_eq!(
            chrome_hit_test(WIN_W - URLBAR_RIGHT_INSET + 10.0, y, 1, WIN_W),
            None
        );
        // Row 1 above the bar is not the bar.
        assert_ne!(
            chrome_hit_test(URLBAR_LEFT + 40.0, TAB_TOP + 2.0, 1, WIN_W),
            Some(ChromeHit::UrlBar)
        );
    }

    #[test]
    fn edit_display_appends_caret_and_keeps_the_tail_visible() {
        assert_eq!(edit_display("example.com"), "example.com|");
        let long = "x".repeat(120);
        let shown = edit_display(&long);
        assert!(shown.starts_with('…'));
        assert!(shown.ends_with("x|"));
        assert_eq!(shown.chars().count(), 92); // ellipsis + 90-char tail + caret
    }

    #[test]
    fn chrome_html_renders_every_tab_with_positioned_slots() {
        let identity = Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace],
        );
        let dom =
            crate::html::Parser::new("<html><head><title>Alpha</title></head><body></body></html>")
                .parse_document();
        let props = ChromeProps::from_tabs(
            "https://example.com/",
            &dom,
            &identity,
            vec!["Alpha".into(), "Beta".into()],
            1,
        );
        let html = chrome_html(props);

        assert!(html.contains("tab tab-off slot-0"));
        assert!(html.contains("tab tab-on slot-1"));
        assert!(html.contains("Alpha"));
        assert!(html.contains("Beta"));
        assert!(html.contains(".slot-1 { left: 292px; width: 136px; }"));
        assert!(html.contains("tab-new"));
        // Idle address bar shows the page URL.
        assert!(html.contains("urlbar urlbar-off"));
        assert!(html.contains("example.com"));
    }

    #[test]
    fn chrome_html_renders_the_edit_buffer_when_the_bar_is_focused() {
        let identity = Identity::new(
            "did:aurora:test",
            "Aurora Test",
            IdentityKind::Agent,
            [Capability::ReadWorkspace],
        );
        let dom = crate::html::Parser::new("<html><body></body></html>").parse_document();
        let mut props = ChromeProps::from_render_state("https://example.com/", &dom, &identity);
        props.url_edit = Some("wikipedia.o".to_string());

        let html = chrome_html(props);

        assert!(html.contains("urlbar urlbar-on"));
        assert!(html.contains("url url-edit"));
        assert!(html.contains("wikipedia.o|"), "buffer + caret is rendered");
        assert!(
            !html.contains("example.com/"),
            "page URL is replaced while editing"
        );
    }

    #[test]
    fn tab_label_prefers_title_and_falls_back_to_host() {
        let titled = crate::html::Parser::new(
            "<html><head><title>  Page Title  </title></head><body></body></html>",
        )
        .parse_document();
        assert_eq!(tab_label("https://example.com/x", &titled), "Page Title");

        let untitled = crate::html::Parser::new("<html><body>hi</body></html>").parse_document();
        assert_eq!(tab_label("https://example.com/x", &untitled), "example.com");
    }
}

impl ChromeRenderer {
    pub fn paint(
        &mut self,
        scene: &mut vello::Scene,
        width: u32,
        props: ChromeProps,
        identity: &Identity,
    ) {
        let html = chrome_html(props);
        if self.doc.is_none() || html != self.cached_html || width != self.width {
            self.doc = crate::blitz_document::BlitzDocument::try_from_html(
                &html,
                None,
                identity,
                width,
                CHROME_HEIGHT,
            );
            self.cached_html = html;
            self.width = width;
        }
        if let Some(doc) = self.doc.as_mut() {
            let _paint_result = doc.paint_to_scene(scene, width, CHROME_HEIGHT);
        }
    }
}
