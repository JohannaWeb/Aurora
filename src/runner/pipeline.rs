use super::cli::{env_f32, CliOptions};
use super::fixtures::demo_html;
use super::images::load_images;
use super::scripts::extract_scripts;
use crate::css::Stylesheet;
use crate::html::Parser;
use crate::layout::{LayoutTree, ViewportSize};
use crate::style::StyleTree;
use opus::domain::Identity;
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

    maybe_open_window(
        dom,
        stylesheet_rc,
        base_url,
        identity,
        viewport_rc,
        layout_rc,
        image_cache,
        runtime,
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
        width: env_f32("AURORA_VIEWPORT_WIDTH").unwrap_or(1200.0),
        height: env_f32("AURORA_VIEWPORT_HEIGHT").unwrap_or(900.0),
    }
}

fn run_scripts(
    dom: &crate::dom::NodePtr,
    base_url: Option<&str>,
    identity: &Identity,
) -> Option<crate::js_boa::BoaRuntime> {
    let scripts = extract_scripts(dom);
    if scripts.is_empty() {
        return None;
    }

    println!("Boa: Processing {} scripts...", scripts.len());
    let mut runtime = crate::js_boa::BoaRuntime::new(Rc::clone(dom));
    for (source, is_url) in scripts {
        let Some(script_content) = script_content(source, is_url, base_url, identity) else {
            continue;
        };
        if let Err(e) = runtime.execute(&script_content) {
            eprintln!("JS Error: {}", e);
        }
    }
    Some(runtime)
}

fn script_content(
    source: String,
    is_url: bool,
    base_url: Option<&str>,
    identity: &Identity,
) -> Option<String> {
    if !is_url {
        return Some(source);
    }

    let base = base_url?;
    match crate::fetch::resolve_relative_url(base, &source) {
        Ok(full_url) => {
            println!("Boa: Fetching external script: {}", full_url);
            match crate::fetch::fetch_string(&full_url, identity) {
                Ok(content) => Some(content),
                Err(e) => {
                    eprintln!("Failed to fetch script {}: {}", full_url, e);
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to resolve script URL {}: {}", source, e);
            None
        }
    }
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
    runtime: Option<crate::js_boa::BoaRuntime>,
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
            runtime,
        };
        if let Err(error) = crate::window::open(window_input) {
            eprintln!("Window disabled: {error}");
            eprintln!("Set AURORA_SCREENSHOT=/path/output.png for file output or AURORA_HEADLESS=1 to skip window creation.");
        }
    } else if !is_headless && !has_display {
        eprintln!("No display server detected; skipping window creation.");
        eprintln!("Set AURORA_SCREENSHOT=/path/output.png for file output.");
    } else {
        eprintln!("Headless mode: skipping window");
    }
}
