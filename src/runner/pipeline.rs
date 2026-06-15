use super::cli::{CliOptions, env_f32};
use super::fixtures::demo_html;
use super::images::{load_images, load_svgs};
use super::scripts::extract_scripts;
use crate::blitz_document::BlitzDocument;
use crate::css::Stylesheet;
use crate::html::Parser;
use crate::identity::Identity;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::time::Instant;

pub(crate) fn run_browser(cli: CliOptions, identity: Identity) {
    let html = load_html(cli.input_url.as_deref(), &identity);
    let base_url = cli.input_url.clone();
    let viewport = viewport_size();
    let dom = Parser::new(&html).parse_document();

    let mut runtime = run_scripts(&dom, base_url.as_deref(), &identity);

    let mut stylesheet = Stylesheet::from_dom(&dom, base_url.as_deref(), &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let style_tree = StyleTree::from_dom(&dom, &stylesheet);
    let content_viewport = ViewportSize {
        width: viewport.width,
        height: (viewport.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
    };
    let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);
    let image_cache = load_images(layout.root(), base_url.as_deref(), &identity);
    let svg_cache = load_svgs(layout.root(), base_url.as_deref(), &identity);
    let media_cache = crate::MediaCache::load(layout.root(), base_url.as_deref(), &identity);

    let stylesheet_rc = Rc::new(RefCell::new(stylesheet));
    let viewport_rc = Rc::new(RefCell::new(viewport));
    let layout_rc = Rc::new(RefCell::new(layout));
    if let Some(runtime) = runtime.as_mut() {
        runtime.set_shared_state(
            layout_rc.clone(),
            stylesheet_rc.clone(),
            viewport_rc.clone(),
        );
        runtime.clear_dirty_bits();
    }

    print_debug_output(&cli, &dom, &style_tree, &layout_rc.borrow());
    let _ = crate::font::get_glyph_metrics('A');

    let content_w = viewport.width as u32;
    let content_h = (viewport.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0) as u32;
    // Build the live renderer snapshot from the post-script DOM, not the
    // original HTML source, so bootstrap mutations (custom elements, template
    // stamping, connectedCallback writes, etc.) are visible on the first paint.
    let blitz_doc =
        build_hydrated_blitz_doc(&dom, base_url.as_deref(), &identity, content_w, content_h);
    if env::var("AURORA_DEBUG_RENDER").is_ok() {
        match &blitz_doc {
            Some(blitz_doc) => eprintln!(
                "[render] hydrated first paint: {}",
                blitz_doc.debug_summary()
            ),
            None => eprintln!("[render] Blitz renderer disabled for initial document"),
        }
    }

    maybe_open_window(
        dom,
        stylesheet_rc,
        base_url,
        identity,
        viewport_rc,
        layout_rc,
        image_cache,
        svg_cache,
        media_cache,
        runtime,
        blitz_doc,
    );
}

fn load_html(input_url: Option<&str>, identity: &Identity) -> String {
    match input_url {
        Some(url) => match crate::fetch::fetch_html(url, identity) {
            Ok(html) => html,
            Err(error) => {
                eprintln!("Failed to fetch {url}: {error}");
                std::process::exit(1);
            }
        },
        None => demo_html().to_string(),
    }
}

fn viewport_size() -> ViewportSize {
    ViewportSize {
        width: env_f32("AURORA_VIEWPORT_WIDTH").unwrap_or(1440.0),
        height: env_f32("AURORA_VIEWPORT_HEIGHT").unwrap_or(1024.0),
    }
}

// Scripts larger than this are skipped as a memory/time safety bound.
// SpiderMonkey JITs real-world bundles fine, so these budgets are sized for
// modern multi-MB sites (e.g. YouTube) rather than an interpreter-only engine.
const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024; // 16 MB per script
const MAX_TOTAL_SCRIPT_BYTES: usize = 32 * 1024 * 1024; // 32 MB across all scripts

fn run_scripts(
    dom: &crate::dom::NodePtr,
    base_url: Option<&str>,
    identity: &Identity,
) -> Option<Box<dyn crate::js_engine::JsRuntime>> {
    let scripts = extract_scripts(dom);
    if scripts.is_empty() {
        return None;
    }

    let total = scripts.len();
    log::info!("[JS] processing {} scripts", total);

    // Fetch all external scripts in parallel, preserving order for execution.
    let fetched: Vec<Option<String>> = {
        let handles: Vec<_> = scripts
            .iter()
            .map(|script| {
                let base = base_url.map(str::to_string);
                let identity = identity.clone();
                let source = script.source.clone();
                let is_url = script.is_url;
                std::thread::spawn(move || fetch_script(source, is_url, base.as_deref(), &identity))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(None))
            .collect()
    };

    let mut runtime: Box<dyn crate::js_engine::JsRuntime> =
        crate::js_engine::create_runtime(crate::js_engine::EngineKind::from_env(), dom)
            .unwrap_or_else(|e| {
                log::warn!("[JS] {e}; falling back to SpiderMonkey");
                crate::js_engine::create_runtime(crate::js_engine::EngineKind::SpiderMonkey, dom)
                    .expect("SpiderMonkey backend is always available")
            });
    let mut total_script_bytes = 0usize;
    for (script, content) in scripts.iter().zip(fetched.into_iter()) {
        let Some(content) = content else { continue };
        if total_script_bytes + content.len() > MAX_TOTAL_SCRIPT_BYTES {
            eprintln!(
                "JS: skipping script ({} KB, over {}KB total limit)",
                content.len() / 1024,
                MAX_TOTAL_SCRIPT_BYTES / 1024
            );
            continue;
        }
        total_script_bytes += content.len();
        runtime.set_current_script(Some(&script.node));
        if let Err(e) = runtime.execute(&content) {
            crate::logging::track_js_exception(&e);
        }
        runtime.set_current_script(None);
        pump_ready_work(runtime.as_mut());
    }
    runtime.fire_dom_content_loaded();
    pump_ready_work(runtime.as_mut());
    runtime.fire_load();
    pump_ready_work(runtime.as_mut());
    Some(runtime)
}

fn pump_ready_work(runtime: &mut dyn crate::js_engine::JsRuntime) {
    for _ in 0..8 {
        let now = Instant::now();
        let mut fired = runtime.tick(now);
        fired |= runtime.drain_animation_frame_callbacks(now);
        if !fired && !runtime.has_ready_work(now) {
            break;
        }
    }
}

pub fn fetch_script(
    source: String,
    is_url: bool,
    base_url: Option<&str>,
    identity: &Identity,
) -> Option<String> {
    if !is_url {
        return if source.len() <= MAX_SCRIPT_BYTES {
            Some(source)
        } else {
            eprintln!(
                "JS: skipping inline script ({} KB, over limit)",
                source.len() / 1024
            );
            None
        };
    }

    let base = base_url?;
    let full_url = match crate::fetch::resolve_relative_url(base, &source) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("Failed to resolve script URL {source}: {e}");
            return None;
        }
    };

    log::info!(target: "aurora::net", "[NET] GET {} (script)", full_url);
    let content = match crate::fetch::fetch_string(&full_url, identity) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to fetch script {full_url}: {e}");
            return None;
        }
    };

    if content.len() > MAX_SCRIPT_BYTES {
        eprintln!(
            "JS: skipping {} ({} KB, over {}KB limit)",
            full_url,
            content.len() / 1024,
            MAX_SCRIPT_BYTES / 1024
        );
        return None;
    }

    Some(content)
}

fn print_debug_output(
    cli: &CliOptions,
    dom: &crate::dom::NodePtr,
    style_tree: &StyleTree,
    layout: &LayoutTree,
) {
    if cli.debug_dom {
        println!("{}", dom.borrow());
    }
    if cli.debug_style {
        println!("{style_tree}");
    }
    if cli.debug_layout {
        println!("{layout}");
    }
}

fn maybe_open_window(
    dom: crate::dom::NodePtr,
    stylesheet: Rc<RefCell<Stylesheet>>,
    base_url: Option<String>,
    identity: Identity,
    viewport: Rc<RefCell<ViewportSize>>,
    layout: Rc<RefCell<LayoutTree>>,
    images: crate::ImageCache,
    svgs: crate::SvgCache,
    media: crate::MediaCache,
    runtime: Option<Box<dyn crate::js_engine::JsRuntime>>,
    blitz_doc: Option<BlitzDocument>,
) {
    let has_screenshot = env::var("AURORA_SCREENSHOT").is_ok();
    let is_headless = env::var("AURORA_HEADLESS").is_ok();
    let has_display = env::var("DISPLAY").is_ok() || env::var("WAYLAND_DISPLAY").is_ok();

    if has_screenshot || (!is_headless && has_display) {
        let window_input = crate::window::WindowInput {
            dom,
            stylesheet,
            base_url,
            identity,
            viewport,
            layout,
            images,
            svgs,
            media,
            runtime,
            blitz_doc,
            needs_reflow: false,
        };
        if let Err(error) = crate::window::open(window_input) {
            eprintln!("Window disabled: {error}");
            eprintln!(
                "Set AURORA_SCREENSHOT=/path/output.png for file output or AURORA_HEADLESS=1 to skip window creation."
            );
        }
    } else if !is_headless && !has_display {
        eprintln!("No display server detected; skipping window creation.");
        eprintln!("Set AURORA_SCREENSHOT=/path/output.png for file output.");
    } else {
        eprintln!("Headless mode: skipping window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html::Parser;

    #[test]
    #[cfg(feature = "engine-sm")]
    fn hydrated_blitz_doc_reflects_dom_mutations_before_first_paint() {
        let html = "<html><body><div id='root'>before</div></body></html>";
        let dom = Parser::new(html).parse_document();
        let mut runtime = crate::js_sm::SmRuntime::new(dom.clone());

        runtime
            .execute(
                r#"
                document.getElementById("root").textContent = "after";
                "#,
            )
            .unwrap();

        let blitz_doc =
            build_hydrated_blitz_doc(&dom, None, &headless_identity(), 800, 600).unwrap();
        let summary = blitz_doc.debug_summary();

        assert!(summary.contains("text_len=5"), "{summary}");
        assert!(summary.contains("nodes=5"), "{summary}");
        assert!(summary.contains("elements=4"), "{summary}");
    }

    fn headless_identity() -> Identity {
        Identity::new(
            "did:headless:test",
            "Headless",
            crate::identity::IdentityKind::Agent,
            [
                crate::identity::Capability::ReadWorkspace,
                crate::identity::Capability::NetworkAccess,
            ],
        )
    }
}

pub(crate) fn build_hydrated_blitz_doc(
    dom: &crate::dom::NodePtr,
    base_url: Option<&str>,
    identity: &Identity,
    content_w: u32,
    content_h: u32,
) -> Option<BlitzDocument> {
    let hydrated_html = crate::dom::serialize_outer_html(dom);
    BlitzDocument::try_from_html(&hydrated_html, base_url, identity, content_w, content_h)
}
