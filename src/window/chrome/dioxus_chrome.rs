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

/// Live node counts harvested from the real DOM, plus the active page title.
/// This is what the chrome's "telemetry" fields report — nothing is faked.
struct DomTelemetry {
    total: usize,
    elements: usize,
    text: usize,
    title: Option<String>,
}

/// Walk the real DOM (light children + parsed `<template>` contents) and tally
/// node counts, capturing the first non-empty `<title>` text as the page title.
fn dom_telemetry(root: &NodePtr) -> DomTelemetry {
    fn walk(node: &NodePtr, t: &mut DomTelemetry, in_title: bool) {
        let b = node.borrow();
        match &*b {
            Node::Document { children, .. } => {
                t.total += 1;
                for c in children {
                    walk(c, t, false);
                }
            }
            Node::Element(el) => {
                t.total += 1;
                t.elements += 1;
                let is_title = el.tag_name.eq_ignore_ascii_case("title");
                for c in &el.children {
                    walk(c, t, is_title);
                }
                if let Some(tc) = &el.template_contents {
                    walk(tc, t, false);
                }
            }
            Node::Text(txt) => {
                t.total += 1;
                t.text += 1;
                if in_title && t.title.is_none() {
                    let s = txt.content.trim();
                    if !s.is_empty() {
                        t.title = Some(s.to_string());
                    }
                }
            }
        }
    }
    let mut t = DomTelemetry {
        total: 0,
        elements: 0,
        text: 0,
        title: None,
    };
    walk(root, &mut t, false);
    t
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

/// Data model for the chrome. Pulled out of the old hand-coded painters so the
/// chrome's content is now data-driven rather than baked into draw calls.
#[derive(Props, Clone, PartialEq)]
pub struct ChromeProps {
    pub url: String,
    pub version: String,
    pub tab_label: String,
    pub diagnostics: String,
    pub identity_initials: String,
}

impl ChromeProps {
    /// Build props from the real render state: node counts come from walking the
    /// live DOM, the tab label from the page's `<title>` (or URL host), and the
    /// identity chip from the active `Identity`.
    pub fn from_render_state(url: &str, dom: &NodePtr, identity: &Identity) -> Self {
        let t = dom_telemetry(dom);

        let tab_label = t
            .title
            .as_deref()
            .map(|s| truncate(s, 22))
            .unwrap_or_else(|| truncate(&url_host(url), 22));

        // Quiet, real engine-debug counter (no fabricated mem/gpu/tab figures).
        let diagnostics = format!("{} nodes · {} text", t.total, t.text);

        ChromeProps {
            url: chrome_display_url(url),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tab_label,
            diagnostics,
            identity_initials: identity_initials(&identity.name),
        }
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

#[allow(non_snake_case)]
fn Chrome(props: ChromeProps) -> Element {
    rsx! {
        div { class: "chrome",
            // ── row 1: brand + active tab ............ engine · identity ──
            div { class: "logo-box" }
            span { class: "brand", "AURORA" }
            span { class: "ver", "{props.version}" }
            span { class: "tab", "{props.tab_label}" }
            span { class: "id", "{props.identity_initials}" }

            // ── row 2: nav + url ............ live counters ──
            span { class: "nav nav-back", "‹" }
            span { class: "nav nav-fwd", "›" }
            span { class: "nav nav-reload", "↻" }
            div { class: "urlbar" }
            span { class: "tls", "TLS" }
            span { class: "url", "{props.url}" }
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
.tab {
    left: 150px; top: 7px; height: 22px;
    background: rgb(255,231,240); color: rgb(105,54,76);
    font-size: 12px; padding: 4px 0 0 12px;
}
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
.urlbar {
    left: 96px; top: 42px; right: 180px; height: 24px;
    background: rgb(255,249,251); border: 1px solid rgb(240,214,225);
}
.tls {
    left: 106px; top: 46px; font-size: 10px; color: rgb(198,87,133);
}
.url { left: 146px; top: 45px; font-size: 13px; color: rgb(138,100,117); }
.diag { right: 16px; top: 47px; width: 170px; font-size: 11px; color: rgb(184,160,173); }
"#;

/// Render the chrome component to a complete HTML document string.
pub fn chrome_html(props: ChromeProps) -> String {
    let mut vdom = VirtualDom::new_with_props(Chrome, props);
    vdom.rebuild_in_place();
    let body = dioxus_ssr::render(&vdom);
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><style>{CHROME_CSS}</style></head><body>{body}</body></html>"
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

impl ChromeRenderer {
    pub fn paint(
        &mut self,
        scene: &mut vello::Scene,
        width: u32,
        url: &str,
        dom: &NodePtr,
        identity: &Identity,
    ) {
        let html = chrome_html(ChromeProps::from_render_state(url, dom, identity));
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
