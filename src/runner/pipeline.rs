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
    let blitz_doc = BlitzDocument::from_html(&html, base_url.as_deref(), &identity, content_w, content_h);

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
const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024; // 8 MB per script
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
    println!("JS: Processing {} scripts...", total);

    // Fetch all external scripts in parallel, preserving order for execution.
    let fetched: Vec<Option<String>> = {
        let handles: Vec<_> = scripts
            .into_iter()
            .map(|(source, is_url)| {
                let base = base_url.map(str::to_string);
                let identity = identity.clone();
                std::thread::spawn(move || fetch_script(source, is_url, base.as_deref(), &identity))
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or(None)).collect()
    };

    let mut runtime: Box<dyn crate::js_engine::JsRuntime> =
        Box::new(crate::js_sm::SmRuntime::new(Rc::clone(dom)));
    let mut total_bytes = 0usize;
    for content in fetched.into_iter().flatten() {
        total_bytes += content.len();
        if total_bytes > MAX_TOTAL_SCRIPT_BYTES {
            eprintln!(
                "JS: total script budget ({}KB) reached, skipping remaining scripts",
                MAX_TOTAL_SCRIPT_BYTES / 1024
            );
            break;
        }
        if let Err(e) = runtime.execute(&content) {
            eprintln!("JS Error: {}", e);
        }
    }
    Some(runtime)
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
            eprintln!("JS: skipping inline script ({} KB, over limit)", source.len() / 1024);
            None
        };
    }

    let base = base_url?;
    let full_url = match crate::fetch::resolve_relative_url(base, &source) {
        Ok(u) => u,
        Err(e) => { eprintln!("Failed to resolve script URL {source}: {e}"); return None; }
    };

    println!("JS: Fetching external script: {full_url}");
    let content = match crate::fetch::fetch_string(&full_url, identity) {
        Ok(c) => c,
        Err(e) => { eprintln!("Failed to fetch script {full_url}: {e}"); return None; }
    };

    if content.len() > MAX_SCRIPT_BYTES {
        eprintln!(
            "JS: skipping {} ({} KB, over {}KB limit)",
            full_url, content.len() / 1024, MAX_SCRIPT_BYTES / 1024
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
    blitz_doc: BlitzDocument,
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
            blitz_doc: Some(blitz_doc),
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
