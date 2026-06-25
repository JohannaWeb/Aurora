use super::cli::{CliOptions, env_f32};
use super::event_loop::{EventLoopPhase, run_event_loop_turn};
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
use std::time::{Duration, Instant};

pub(crate) fn run_browser(cli: CliOptions, identity: Identity) {
    let html = load_html(cli.input_url.as_deref(), &identity);
    let base_url = cli.input_url.clone();
    let viewport = viewport_size();
    let dom = Parser::new(&html).parse_document();
    // Establish parent back-pointers for the freshly parsed tree so DOM
    // connectivity/ancestor queries are O(depth); mutations maintain them after.
    crate::dom::reparent_subtree(&dom);

    let content_viewport = ViewportSize {
        width: viewport.width,
        height: (viewport.height - crate::window::BROWSER_CHROME_HEIGHT).max(1.0),
    };
    let content_w = content_viewport.width as u32;
    let content_h = content_viewport.height as u32;
    // Build the live renderer document before script execution and share it
    // with V8 so DOM mutations can be applied directly to Blitz as they happen.
    let blitz_doc =
        BlitzDocument::try_from_dom(&dom, base_url.as_deref(), &identity, content_w, content_h)
            .map(|doc| Rc::new(RefCell::new(doc)));

    let mut runtime = run_scripts(&dom, base_url.as_deref(), &identity, blitz_doc.clone());

    let mut stylesheet = Stylesheet::from_dom(&dom, base_url.as_deref(), &identity);
    stylesheet.merge(Stylesheet::user_agent_stylesheet());
    let sync_legacy_layout = blitz_doc.is_none()
        || cli.debug_style
        || cli.debug_layout
        || matches!(
            env::var("AURORA_LEGACY_LAYOUT_SYNC").as_deref(),
            Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
        );
    let (style_tree, layout, image_cache, svg_cache, media_cache) = if sync_legacy_layout {
        let style_tree = StyleTree::from_dom(&dom, &stylesheet);
        let layout = LayoutTree::from_style_tree_with_viewport(&style_tree, content_viewport);
        let image_cache = load_images(layout.root(), base_url.as_deref(), &identity);
        let svg_cache = load_svgs(layout.root(), base_url.as_deref(), &identity);
        let media_cache = crate::MediaCache::load(layout.root(), base_url.as_deref(), &identity);
        (
            Some(style_tree),
            layout,
            image_cache,
            svg_cache,
            media_cache,
        )
    } else {
        (
            None,
            LayoutTree::placeholder(content_viewport),
            crate::ImageCache::default(),
            crate::SvgCache::default(),
            crate::MediaCache::default(),
        )
    };

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

    print_debug_output(&cli, &dom, style_tree.as_ref(), &layout_rc.borrow());
    let _ = crate::font::get_glyph_metrics('A');

    if env::var("AURORA_DEBUG_RENDER").is_ok() {
        match &blitz_doc {
            Some(blitz_doc) => {
                let mut blitz_doc = blitz_doc.borrow_mut();
                eprintln!(
                    "[render] hydrated first paint: {}",
                    blitz_doc.debug_summary()
                );
                // TEMP: exercise the same paint the live window does, to see if
                // it panics (→ white page) or produces an empty scene.
                let mut scene = vello::Scene::new();
                let paint_result = blitz_doc.paint_to_scene(&mut scene, content_w, content_h);
                let enc = scene.encoding();
                eprintln!(
                    "[render] paint_to_scene result={paint_result:?} encoding: paths={} draw_tags={} path_data={}",
                    enc.n_paths,
                    enc.draw_tags.len(),
                    enc.path_data.len()
                );
                eprintln!("[render] layout sizes: {}", blitz_doc.debug_layout_sizes());
            }
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
// V8 runs real-world bundles fine, so these budgets are sized for modern
// multi-MB sites (e.g. YouTube) rather than an interpreter-only engine.
const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024; // 16 MB per script
const MAX_TOTAL_SCRIPT_BYTES: usize = 32 * 1024 * 1024; // 32 MB across all scripts

fn run_scripts(
    dom: &crate::dom::NodePtr,
    base_url: Option<&str>,
    identity: &Identity,
    render_document: Option<Rc<RefCell<BlitzDocument>>>,
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

    let mut runtime: Box<dyn crate::js_engine::JsRuntime> = crate::js_engine::create_runtime(
        crate::js_engine::EngineKind::from_env(),
        dom,
        render_document,
    )
    .expect("V8 backend is required for JavaScript execution");
    if let Some(url) = base_url {
        // `{url:?}` produces a quoted, escaped string literal valid in JS.
        let _ = runtime.execute(&format!(
            "if (typeof __aurora_set_location__ === 'function') __aurora_set_location__({url:?});"
        ));
    }
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
    log_youtube_debug_state(runtime.as_mut(), "after-scripts");
    runtime.fire_dom_content_loaded();
    pump_ready_work(runtime.as_mut());
    log_youtube_debug_state(runtime.as_mut(), "after-domcontentloaded");
    runtime.fire_load();
    pump_ready_work(runtime.as_mut());
    log_youtube_debug_state(runtime.as_mut(), "after-load");
    let native_ce_reactions = runtime.native_custom_element_reactions_enabled();
    apply_polymer_bindings(runtime.as_mut(), native_ce_reactions);
    pump_ready_work(runtime.as_mut());
    drive_content_bearing_initial_navigation(runtime.as_mut());
    pump_ready_work(runtime.as_mut());
    apply_polymer_bindings(runtime.as_mut(), native_ce_reactions);
    pump_ready_work(runtime.as_mut());
    log_youtube_debug_state(runtime.as_mut(), "after-polymer-bindings");
    Some(runtime)
}

/// Drive the JS event loop toward quiescence after a script or lifecycle step.
///
/// This intentionally uses wall-clock `Instant::now()` instead of teleporting a
/// virtual clock to each scheduled deadline. Timers and rAF callbacks therefore
/// run only when real time has advanced far enough, which keeps ordering closer
/// to a browser event loop and avoids simulating an SPA to quiescence in a burst.
fn pump_ready_work(runtime: &mut dyn crate::js_engine::JsRuntime) {
    const FRAME: Duration = Duration::from_millis(16);
    let real_budget = Duration::from_millis(
        env::var("AURORA_BOOT_REAL_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(500),
    );

    let real_start = Instant::now();
    let mut next_frame = real_start;

    loop {
        let now = Instant::now();
        if now.duration_since(real_start) >= real_budget {
            break;
        }

        // Drive one event-loop turn through explicit, canonically-ordered phases
        // (Phase 7). The headless pump implements the JS-relevant phases; style,
        // layout, paint, and idle callbacks are window-loop concerns left as
        // no-ops here so the full turn ordering stays explicit.
        let raf_due = runtime.has_animation_frame_callbacks() && now >= next_frame;
        let fired_phases = run_event_loop_turn(|phase| match phase {
            EventLoopPhase::RunTask => runtime.tick(now),
            // V8 drains microtasks at the end of each task, so this is implicit.
            EventLoopPhase::MicrotaskCheckpoint => false,
            EventLoopPhase::MutationObserverDelivery => runtime.deliver_mutation_records(),
            EventLoopPhase::ResizeObserverDelivery => false,
            EventLoopPhase::RequestAnimationFrame => {
                raf_due && runtime.drain_animation_frame_callbacks(now)
            }
            EventLoopPhase::StyleAndLayout => runtime.perform_style_and_layout(),
            EventLoopPhase::Paint => runtime.perform_paint(),
            EventLoopPhase::IdleCallbacks => false,
        });
        if raf_due {
            next_frame = now + FRAME;
        }
        if !fired_phases.is_empty() {
            continue;
        }

        let next_timer = runtime.next_deadline();
        let next_raf = runtime
            .has_animation_frame_callbacks()
            .then_some(next_frame);
        let next = match (next_timer, next_raf) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        match next {
            Some(deadline) if deadline > now => {
                let remaining = real_budget.saturating_sub(now.duration_since(real_start));
                let sleep_for = deadline
                    .duration_since(now)
                    .min(remaining)
                    .min(Duration::from_millis(4));
                if sleep_for.is_zero() {
                    break;
                }
                std::thread::sleep(sleep_for);
            }
            Some(_) => std::thread::yield_now(),
            None => break,
        }
    }
}

/// Sweep the rendered tree to install binding hooks on renderers that Polymer
/// stamped natively (bypassing our upgrade pipeline). Without this, their
/// `[[…]]` text/attribute annotations render as literal template text.
fn apply_polymer_bindings(
    runtime: &mut dyn crate::js_engine::JsRuntime,
    native_ce_reactions: bool,
) {
    let native_ce_reactions = if native_ce_reactions { "true" } else { "false" };
    let script = format!(
        r#"
        (function() {{
            try {{
                if (!{native_ce_reactions} && typeof globalThis.__aurora_connect_sweep__ === 'function') {{
                    var c = globalThis.__aurora_connect_sweep__(document.body);
                    if (globalThis.__aurora_debug_youtube__) {{
                        console.log('[yt-bind] connect sweep fired connectedCallback on ' + c + ' element(s)');
                    }}
                }}
                if (typeof globalThis.__aurora_sweep_bindings__ === 'function') {{
                    var n = globalThis.__aurora_sweep_bindings__(document.body);
                    if (globalThis.__aurora_debug_youtube__) {{
                        console.log('[yt-bind] sweep installed hooks on ' + n + ' element(s)');
                    }}
                }}
            }} catch (e) {{
                if (globalThis.__aurora_debug_youtube__) console.log('[yt-bind] sweep error ' + e);
            }}
        }})();
        "#
    );
    let _ = runtime.execute(&script);
}

/// Recover YouTube's first page transition only when the inline application
/// data contains real content. Kevlar normally drives this through a private
/// navigation state machine that does not fire in Aurora. Calling
/// `ytd-page-manager.updatePageData` is the narrow public equivalent, but it
/// must not run for the logged-out homepage's feed-nudge-only payload: doing so
/// replaces the useful shell with an empty browse page.
fn drive_content_bearing_initial_navigation(runtime: &mut dyn crate::js_engine::JsRuntime) {
    let _ = runtime.execute(
        r#"
        (function() {
            try {
                var pageManager = document.querySelector && document.querySelector('ytd-page-manager');
                if (!pageManager && document.querySelector) {
                    var app = document.querySelector('ytd-app');
                    var appController = app && (app.polymerController || app);
                    pageManager = appController && appController.$ &&
                        appController.$['page-manager'];
                }
                var getInitialData = window.getInitialData;
                if (!pageManager || pageManager.__aurora_initial_navigation_driven__ ||
                    typeof pageManager.updatePageData !== 'function' ||
                    typeof getInitialData !== 'function') return;

                var data = getInitialData();
                if (!data) return;

                var visited = [];
                var budget = 50000;
                var contentKeys = {
                    videoRenderer: true,
                    gridVideoRenderer: true,
                    compactVideoRenderer: true,
                    playlistVideoRenderer: true,
                    reelItemRenderer: true
                };
                function hasRenderableContent(value, depth) {
                    if (!value || typeof value !== 'object' || depth > 30 || --budget <= 0) return false;
                    if (visited.indexOf(value) >= 0) return false;
                    visited.push(value);
                    var keys;
                    try { keys = Object.keys(value); } catch (e) { return false; }
                    for (var i = 0; i < keys.length; i++) {
                        if (contentKeys[keys[i]]) return true;
                    }
                    for (var j = 0; j < keys.length; j++) {
                        if (hasRenderableContent(value[keys[j]], depth + 1)) return true;
                    }
                    return false;
                }

                var player = globalThis.ytInitialPlayerResponse;
                var contentBearing = !!(player && player.videoDetails) ||
                    data.page === 'watch' || data.page === 'search' ||
                    hasRenderableContent(data, 0);
                if (!contentBearing) {
                    if (globalThis.__aurora_debug_youtube__) {
                        console.log('[yt-nav] skipped initial navigation: payload has no renderable content');
                    }
                    return;
                }

                // If the private state machine succeeded on its own, leave its
                // live page alone. The driver is a recovery path, not a second
                // navigation.
                if (document.querySelector('ytd-watch-flexy,ytd-browse,ytd-search')) return;

                pageManager.__aurora_initial_navigation_driven__ = true;
                var attempts = 0;
                function settleRenderers() {
                    attempts++;
                    var all = document.querySelectorAll ? document.querySelectorAll('*') : [];
                    for (var i = 0; i < all.length; i++) {
                        var el = all[i];
                        var elData = el.data || (el.__data && el.__data.data);
                        if (!elData || !elData.contents || !elData.contents.length) continue;
                        var shown = el.shownItems || (el.__data && el.__data.shownItems);
                        if (shown && shown.length) continue;
                        try {
                            if (typeof el.dataChanged === 'function') el.dataChanged();
                            else if (typeof el.reflowContent === 'function') el.reflowContent(true);
                        } catch (e) {}
                    }
                    if (attempts < 5) setTimeout(settleRenderers, 0);
                }

                var result = pageManager.updatePageData(data);
                if (globalThis.__aurora_debug_youtube__) {
                    console.log('[yt-nav] drove content-bearing initial page=' + (data.page || 'unknown'));
                }
                if (result && typeof result.then === 'function') {
                    result.then(settleRenderers, function() {});
                } else {
                    settleRenderers();
                }
            } catch (error) {
                if (globalThis.__aurora_debug_youtube__) {
                    console.log('[yt-nav] initial navigation failed ' + error);
                }
            }
        })();
        "#,
    );
}

fn log_youtube_debug_state(runtime: &mut dyn crate::js_engine::JsRuntime, label: &str) {
    if !matches!(
        env::var("AURORA_DEBUG_YOUTUBE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    ) {
        return;
    }

    let _ = runtime.execute(&format!(
        r#"
        (function() {{
            function bool(v) {{ return v ? 'yes' : 'no'; }}
            function count(n) {{ return n && n.childNodes ? n.childNodes.length : 'n/a'; }}
            function keys(o) {{
                try {{ return o ? Object.keys(o).slice(0, 12).join(',') : ''; }}
                catch (e) {{ return 'err:' + e; }}
            }}
            function nodeState(selector) {{
                var el = document.querySelector(selector);
                if (!el) return selector + '=missing';
                return selector +
                    '=exists upgraded=' + bool(el.__ce_upgraded__) +
                    ' connected=' + bool(el.__ce_connected__) +
                    ' connectFailed=' + bool(el.__ce_connect_failed__) +
                    ' children=' + count(el) +
                    ' shadowChildren=' + count(el.shadowRoot) +
                    ' dataKeys=' + keys(el.data || el.__data || el.__dataHost || null);
            }}
            var pr = globalThis.ytInitialPlayerResponse;
            var ytcfgType = typeof globalThis.ytcfg;
            console.log('[yt-probe] {label} ytInitialPlayerResponse=' + bool(pr) +
                ' type=' + typeof pr +
                ' videoDetails=' + bool(pr && pr.videoDetails) +
                ' playability=' + (pr && pr.playabilityStatus ? pr.playabilityStatus.status : 'n/a') +
                ' keys=' + keys(pr));
            console.log('[yt-probe] {label} ytcfg=' + ytcfgType +
                ' app=' + nodeState('ytd-app'));
            console.log('[yt-probe] {label} flexy=' + nodeState('ytd-watch-flexy'));
            var app = document.querySelector('ytd-app');
            var appController = app && (app.polymerController || app);
            var pageManager = appController && appController.$ && appController.$['page-manager'];
            var pageController = pageManager && pageManager.polymerController;
            console.log('[yt-probe] {label} page-manager=' + bool(pageManager) +
                ' lazy=' + (pageManager ? typeof pageManager.lazyPrepareCriticalPages : 'missing') +
                ' controller=' + bool(pageController) +
                ' controllerLazy=' + (pageController ? typeof pageController.lazyPrepareCriticalPages : 'missing') +
                ' own=' + keys(pageManager) +
                ' controllerOwn=' + keys(pageController));
            console.log('[yt-probe] {label} stamped player=' +
                bool(document.querySelector('ytd-player,#movie_player,ytw-player-with-controls')) +
                ' metadata=' + bool(document.querySelector('ytd-watch-metadata')) +
                ' primary=' + bool(document.querySelector('#primary,ytd-watch-grid')) +
                ' secondary=' + bool(document.querySelector('ytd-watch-next-secondary-results-renderer,#secondary')) +
                ' comments=' + bool(document.querySelector('ytd-comments,ytd-comments-entry-point-header-renderer')));
        }})();
        "#
    ));
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
    style_tree: Option<&StyleTree>,
    layout: &LayoutTree,
) {
    if cli.debug_dom {
        println!("{}", dom.borrow());
    }
    if cli.debug_style {
        if let Some(style_tree) = style_tree {
            println!("{style_tree}");
        }
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
    blitz_doc: Option<Rc<RefCell<BlitzDocument>>>,
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
            blitz_snapshot_dirty: false,
            pending_snapshot_rebuild_reason: None,
            pending_snapshot_rebuild_source: None,
            snapshot_rebuild_count: 0,
            consecutive_snapshot_rebuilds: 0,
            last_snapshot_rebuild_reason: None,
            last_snapshot_rebuild_source: None,
            last_snapshot_rebuild_op_id: None,
            #[cfg(debug_assertions)]
            snapshot_rebuild_events: std::collections::VecDeque::new(),
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
    use crate::js_engine::JsRuntime;

    #[test]
    fn hydrated_blitz_doc_reflects_dom_mutations_before_first_paint() {
        let html = "<html><body><div id='root'>before</div></body></html>";
        let dom = Parser::new(html).parse_document();
        let mut runtime = crate::js_v8::V8Runtime::new(dom.clone());

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

    #[test]
    fn initial_navigation_driver_skips_empty_home_and_drives_content_once() {
        let dom = Parser::new("<html><body><ytd-page-manager></ytd-page-manager></body></html>")
            .parse_document();
        let mut runtime = crate::js_v8::V8Runtime::new(dom);
        runtime
            .execute(
                r#"
                globalThis.__navCalls = 0;
                globalThis.__initialData = {
                    page: 'browse',
                    contents: { feedNudgeRenderer: { title: 'Sign in' } }
                };
                window.getInitialData = function() { return globalThis.__initialData; };
                document.querySelector('ytd-page-manager').updatePageData = function(data) {
                    globalThis.__navCalls++;
                    globalThis.__navigatedData = data;
                };
                "#,
            )
            .unwrap();

        drive_content_bearing_initial_navigation(&mut runtime);
        assert_eq!(runtime.eval_to_string("__navCalls"), Ok("0".into()));

        runtime
            .execute(
                r#"
                globalThis.__initialData = {
                    page: 'search',
                    contents: { sectionListRenderer: { contents: [
                        { itemSectionRenderer: { contents: [
                            { videoRenderer: { videoId: 'content-id' } }
                        ] } }
                    ] } }
                };
                "#,
            )
            .unwrap();
        drive_content_bearing_initial_navigation(&mut runtime);
        drive_content_bearing_initial_navigation(&mut runtime);

        assert_eq!(runtime.eval_to_string("__navCalls"), Ok("1".into()));
        assert_eq!(
            runtime.eval_to_string("__navigatedData.page + ':' + __navigatedData.contents.sectionListRenderer.contents[0].itemSectionRenderer.contents[0].videoRenderer.videoId"),
            Ok("search:content-id".into())
        );
    }

    #[test]
    fn initial_navigation_driver_accepts_watch_player_data() {
        let dom = Parser::new("<html><body><ytd-page-manager></ytd-page-manager></body></html>")
            .parse_document();
        let mut runtime = crate::js_v8::V8Runtime::new(dom);
        runtime
            .execute(
                r#"
                globalThis.__navCalls = 0;
                globalThis.ytInitialPlayerResponse = { videoDetails: { videoId: 'watch-id' } };
                window.getInitialData = function() { return { page: 'watch' }; };
                document.querySelector('ytd-page-manager').updatePageData = function() {
                    globalThis.__navCalls++;
                };
                "#,
            )
            .unwrap();

        drive_content_bearing_initial_navigation(&mut runtime);
        assert_eq!(runtime.eval_to_string("__navCalls"), Ok("1".into()));
    }

    #[test]
    fn initial_navigation_driver_finds_page_manager_in_app_shadow_map() {
        let dom = Parser::new("<html><body><ytd-app></ytd-app></body></html>").parse_document();
        let mut runtime = crate::js_v8::V8Runtime::new(dom);
        runtime
            .execute(
                r#"
                globalThis.__navCalls = 0;
                globalThis.__initialData = {
                    page: 'search',
                    contents: { videoRenderer: { videoId: 'shadow-content' } }
                };
                window.getInitialData = function() { return globalThis.__initialData; };
                const app = document.querySelector('ytd-app');
                const pageManager = document.createElement('ytd-page-manager');
                pageManager.updatePageData = function(data) {
                    globalThis.__navCalls++;
                    globalThis.__navigatedData = data;
                };
                app.attachShadow({ mode: 'open' }).appendChild(pageManager);
                app.$ = { 'page-manager': pageManager };
                "#,
            )
            .unwrap();

        assert_eq!(
            runtime.eval_to_string("String(document.querySelector('ytd-page-manager'))"),
            Ok("null".into())
        );
        drive_content_bearing_initial_navigation(&mut runtime);
        assert_eq!(runtime.eval_to_string("__navCalls"), Ok("1".into()));
        assert_eq!(
            runtime.eval_to_string("__navigatedData.contents.videoRenderer.videoId"),
            Ok("shadow-content".into())
        );
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
    BlitzDocument::try_from_dom(dom, base_url, identity, content_w, content_h)
}
