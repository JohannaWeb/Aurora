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
use std::time::{Duration, Instant};

pub(crate) fn run_browser(cli: CliOptions, identity: Identity) {
    let html = load_html(cli.input_url.as_deref(), &identity);
    let base_url = cli.input_url.clone();
    let viewport = viewport_size();
    let dom = Parser::new(&html).parse_document();
    // Establish parent back-pointers for the freshly parsed tree so DOM
    // connectivity/ancestor queries are O(depth); mutations maintain them after.
    crate::dom::reparent_subtree(&dom);

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
    runtime.fire_dom_content_loaded();
    pump_ready_work(runtime.as_mut());
    runtime.fire_load();
    pump_ready_work(runtime.as_mut());
    if env::var("AURORA_DEBUG_YOUTUBE").is_ok() {
        run_youtube_initial_navigation_probe(runtime.as_mut());
        pump_ready_work(runtime.as_mut());
    }
    Some(runtime)
}

/// Drive the JS event loop toward quiescence after a script or lifecycle step.
///
/// Real wall-clock time barely advances between iterations, so timers scheduled
/// with any delay would never come due against `Instant::now()` and time-deferred
/// boot work would stall (YouTube hydrates large parts of its tree through chained
/// `setTimeout`s). Instead we run a small event loop against a *virtual* clock that
/// only ever jumps forward to the next piece of scheduled work, firing due timers
/// and animation-frame callbacks until the loop is quiescent.
///
/// Animation frames are throttled to one batch per ~16ms virtual frame so that an
/// `requestAnimationFrame` render loop advances virtual time instead of spinning in
/// place. Two budgets guard against pages that never settle: `VIRTUAL_BUDGET` caps
/// how much page-time we simulate, and `REAL_BUDGET` caps wall-clock time so a slow
/// or runaway synchronous callback can't hang the boot.
fn pump_ready_work(runtime: &mut dyn crate::js_engine::JsRuntime) {
    const FRAME: Duration = Duration::from_millis(16);
    const VIRTUAL_BUDGET: Duration = Duration::from_millis(2000);
    const REAL_BUDGET: Duration = Duration::from_secs(5);

    let real_start = Instant::now();
    let virtual_start = real_start;
    let mut virtual_now = real_start;
    // Earliest virtual time at which the next animation-frame batch may run.
    let mut next_frame = real_start;

    loop {
        if real_start.elapsed() >= REAL_BUDGET {
            break;
        }
        if virtual_now.duration_since(virtual_start) > VIRTUAL_BUDGET {
            break;
        }

        let mut fired = runtime.tick(virtual_now);
        if runtime.has_animation_frame_callbacks() && virtual_now >= next_frame {
            fired |= runtime.drain_animation_frame_callbacks(virtual_now);
            next_frame = virtual_now + FRAME;
        }
        // Deliver MutationObserver records produced by the work above (and let
        // their callbacks' own mutations settle on the next iteration).
        fired |= runtime.deliver_mutation_records();
        if fired {
            continue;
        }

        // Nothing was ready at the current virtual time. Jump the clock to the
        // next scheduled timer or animation frame; if neither exists the loop
        // is quiescent and we're done.
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
            Some(deadline) if deadline > virtual_now => virtual_now = deadline,
            // Work is "due" but nothing fired (e.g. a frame gated by `next_frame`
            // we've already passed); nudge forward to avoid a tight spin.
            Some(_) => virtual_now += FRAME,
            None => break,
        }
    }
}

fn run_youtube_initial_navigation_probe(runtime: &mut dyn crate::js_engine::JsRuntime) {
    let _ = runtime.execute(
        r#"
        (function() {
            try {
                var app = document.querySelector && document.querySelector('ytd-app');
                var pageManager = document.querySelector && document.querySelector('ytd-page-manager');
                var beforeBrowse = document.querySelector && document.querySelector('ytd-browse');
                var data = typeof window.getInitialData === 'function' ? window.getInitialData() : null;
                if (pageManager && !pageManager.__aurora_probe_wrapped__) {
                    pageManager.__aurora_probe_wrapped__ = true;
                    ['updatePageData', 'performDataUpdate', 'setActivePage', 'attachPage', 'preparePage', 'renderPageLoadingState'].forEach(function(name) {
                        var original = pageManager[name];
                        if (typeof original !== 'function') return;
                        pageManager[name] = function() {
                            var first = arguments[0];
                            var page = first && (first.page || first.is || first.tagName || first.nodeName || first);
                            console.log('[yt-probe] pm.' + name + ' page=' + page + ' args=' + arguments.length);
                            try {
                                var result = original.apply(this, arguments);
                                console.log('[yt-probe] pm.' + name + ' returned ' + (result && typeof result.then === 'function' ? 'thenable' : typeof result));
                                return result;
                            } catch (e) {
                                console.log('[yt-probe] pm.' + name + ' threw ' + (e && (e.stack || e.message || e)));
                                throw e;
                            }
                        };
                    });
                }
                if (app && !app.__aurora_probe_wrapped__) {
                    app.__aurora_probe_wrapped__ = true;
                    ['onYtPageDataFetched', 'onYtNavigateStart', 'onYtNavigateFinish', 'onYtNavigateError'].forEach(function(name) {
                        var original = app[name];
                        if (typeof original !== 'function') return;
                        app[name] = function() {
                            console.log('[yt-probe] app.' + name + ' args=' + arguments.length);
                            try {
                                var result = original.apply(this, arguments);
                                console.log('[yt-probe] app.' + name + ' returned ' + (result && typeof result.then === 'function' ? 'thenable' : typeof result));
                                return result;
                            } catch (e) {
                                console.log('[yt-probe] app.' + name + ' threw ' + (e && (e.stack || e.message || e)));
                                throw e;
                            }
                        };
                    });
                    ['yt-navigate-start', 'yt-page-data-fetched', 'yt-navigate-finish', 'yt-navigate-error'].forEach(function(type) {
                        app.addEventListener(type, function(event) {
                            var detail = event && event.detail;
                            console.log('[yt-probe] app event ' + type +
                                ' detailKeys=' + (detail ? Object.keys(detail).join(',') : 'none') +
                                ' page=' + (detail && (detail.pageType || (detail.pageData && detail.pageData.page))));
                        });
                    });
                }
                console.log('[yt-probe] before app=' + !!app +
                    ' pm=' + !!pageManager +
                    ' browse=' + !!beforeBrowse +
                    ' getInitialData=' + !!data +
                    ' appListeners=' + (app && app.__ael ? Object.keys(app.__ael).join(',') : 'none') +
                    ' pmParent=' + (pageManager && pageManager.parentNode && (pageManager.parentNode.tagName || pageManager.parentNode.nodeName)));
                if (app && app.pageManagerAttachedPromise && app.pageManagerAttachedPromise.promise) {
                    try {
                        app.pageManagerAttachedPromise.promise.then(function() {
                            console.log('[yt-probe] pageManagerAttachedPromise resolved');
                        });
                    } catch (e) {
                        console.log('[yt-probe] pageManagerAttachedPromise then error ' + (e && (e.stack || e.message || e)));
                    }
                }
                if (app && app.loadDepsPromise && typeof app.loadDepsPromise.then === 'function') {
                    try {
                        app.loadDepsPromise.then(function() {
                            console.log('[yt-probe] loadDepsPromise resolved');
                        });
                    } catch (e) {
                        console.log('[yt-probe] loadDepsPromise then error ' + (e && (e.stack || e.message || e)));
                    }
                }
                if (app && typeof app.onYtPageManagerAttached === 'function') {
                    try {
                        app.onYtPageManagerAttached({
                            target: pageManager,
                            type: 'attached',
                            composedPath: function() { return [pageManager]; }
                        });
                        console.log('[yt-probe] direct onYtPageManagerAttached called');
                    } catch (e) {
                        console.log('[yt-probe] direct onYtPageManagerAttached error ' + (e && (e.stack || e.message || e)));
                    }
                }
                if (app && data && typeof app.loadData === 'function') {
                    try {
                        app.loadData(data);
                        console.log('[yt-probe] loadData called');
                    } catch (e) {
                        console.log('[yt-probe] loadData error ' + (e && (e.stack || e.message || e)));
                    }
                }
                if (app && data && typeof app.onYtPageDataFetched === 'function') {
                    try {
                        var fetchedDetail = { pageData: data };
                        app.onYtPageDataFetched(new CustomEvent('yt-page-data-fetched', {
                            detail: fetchedDetail,
                            bubbles: true,
                            composed: true
                        }), fetchedDetail);
                        console.log('[yt-probe] onYtPageDataFetched called');
                    } catch (e) {
                        console.log('[yt-probe] onYtPageDataFetched error ' + (e && (e.stack || e.message || e)));
                    }
                }
                if (pageManager && typeof pageManager.dispatchEvent === 'function') {
                    try {
                        pageManager.dispatchEvent(new CustomEvent('attached', {
                            bubbles: true,
                            composed: true,
                            detail: {}
                        }));
                        console.log('[yt-probe] attached dispatched from page-manager id=' + pageManager.id);
                    } catch (e) {
                        console.log('[yt-probe] attached error ' + (e && (e.stack || e.message || e)));
                    }
                }
                Promise.resolve().then(function() {
                    var pm = document.querySelector && document.querySelector('ytd-page-manager');
                    if (app && data && typeof app.onYtNavigateFinish === 'function') {
                        try {
                            var finishDetail = { pageData: data };
                            app.onYtNavigateFinish(new CustomEvent('yt-navigate-finish', {
                                detail: finishDetail,
                                bubbles: true,
                                composed: true
                            }), finishDetail);
                            console.log('[yt-probe] onYtNavigateFinish called');
                        } catch (e) {
                            console.log('[yt-probe] onYtNavigateFinish error ' + (e && (e.stack || e.message || e)));
                        }
                    }
                    if (app && data && data.endpoint && typeof app.handleNavigate === 'function') {
                        try {
                            app.handleNavigate({ command: data.endpoint });
                            console.log('[yt-probe] handleNavigate called');
                        } catch (e) {
                            console.log('[yt-probe] handleNavigate error ' + (e && (e.stack || e.message || e)));
                        }
                    }
                if (pm && data && typeof pm.updatePageData === 'function') {
                    try {
                        if (globalThis.__aurora_debug_youtube__) {
                            try {
                                var pmProto = Object.getPrototypeOf(pm);
                                var dumpNames = ['updatePageData', 'performDataUpdate', 'setActivePage', 'attachPage', 'preparePage', 'renderPageLoadingState', 'getCurrentPage', 'getCurrentData'];
                                for (var di = 0; di < dumpNames.length; di++) {
                                    var dn = dumpNames[di];
                                    var ownDesc = Object.getOwnPropertyDescriptor(pm, dn);
                                    var protoDesc = pmProto ? Object.getOwnPropertyDescriptor(pmProto, dn) : null;
                                    var fn = pm[dn];
                                    console.log('[yt-probe] pm.' + dn +
                                        ' own=' + (ownDesc ? (ownDesc.get ? 'getter' : typeof ownDesc.value) : 'none') +
                                        ' proto=' + (protoDesc ? (protoDesc.get ? 'getter' : typeof protoDesc.value) : 'none') +
                                        ' typeof=' + typeof fn +
                                        ' src=' + (typeof fn === 'function' ? String(fn).replace(/\s+/g, ' ').slice(0, 260) : String(fn)));
                                }
                                console.log('[yt-probe] pm props currentPageOwn=' + Object.prototype.hasOwnProperty.call(pm, 'currentPage') +
                                    ' currentPage=' + (pm.currentPage ? (pm.currentPage.is || pm.currentPage.tagName || pm.currentPage.nodeName) : String(pm.currentPage)) +
                                    ' dataOwn=' + Object.prototype.hasOwnProperty.call(pm, 'data') +
                                    ' dataType=' + typeof pm.data +
                                    ' keys=' + (pm.data && typeof pm.data === 'object' ? Object.keys(pm.data).slice(0, 12).join(',') : 'n/a'));
                            } catch (dumpError) {
                                console.log('[yt-probe] pm dump error ' + (dumpError && (dumpError.stack || dumpError.message || dumpError)));
                            }
                        }
                        var updateResult = pm.updatePageData(data);
                        console.log('[yt-probe] direct pm.updatePageData called');
                        if (updateResult && typeof updateResult.then === 'function') {
                                updateResult.then(function() {
                                    console.log('[yt-probe] direct pm.updatePageData resolved browse=' +
                                        !!(document.querySelector && document.querySelector('ytd-browse')) +
                                        ' pmChildren=' + (pm && pm.children ? pm.children.length : -1) +
                                        ' currentPage=' + (pm && pm.getCurrentPage && pm.getCurrentPage() ? pm.getCurrentPage().is : 'none'));
                                }, function(error) {
                                    console.log('[yt-probe] direct pm.updatePageData rejected ' + (error && (error.stack || error.message || error)));
                                });
                            }
                        } catch (e) {
                            console.log('[yt-probe] direct pm.updatePageData error ' + (e && (e.stack || e.message || e)));
                        }
                    }
                    console.log('[yt-probe] microtask browse=' +
                        !!(document.querySelector && document.querySelector('ytd-browse')) +
                        ' pmChildren=' + (pm && pm.children ? pm.children.length : -1));
                });
                setTimeout(function() {
                    var pm = document.querySelector && document.querySelector('ytd-page-manager');
                    var cp = pm && pm.getCurrentPage && pm.getCurrentPage();
                    console.log('[yt-probe] timeout0 browse=' +
                        !!(document.querySelector && document.querySelector('ytd-browse')) +
                        ' pmChildren=' + (pm && pm.children ? pm.children.length : -1) +
                        ' currentPage=' + (cp ? cp.is : 'none') +
                        ' cpParent=' + (cp && cp.parentNode ? (cp.parentNode.tagName || cp.parentNode.nodeName) : 'none') +
                        ' cpConnected=' + (cp ? cp.isConnected : 'n/a') +
                        ' pmRootKids=' + (pm && pm.root && pm.root.children ? pm.root.children.length : 'n/a') +
                        ' pmShadowKids=' + (pm && pm.shadowRoot && pm.shadowRoot.children ? pm.shadowRoot.children.length : 'n/a') +
                        ' pmQueryBrowse=' + !!(pm && pm.querySelector && pm.querySelector('ytd-browse')) +
                        ' dataPage=' + (pm && pm.getCurrentData && pm.getCurrentData() ? pm.getCurrentData().page : 'none'));
                }, 0);
                setTimeout(function() {
                    var pm = document.querySelector && document.querySelector('ytd-page-manager');
                    var cp = pm && pm.getCurrentPage && pm.getCurrentPage();
                    function deepFind(root, selector, seen) {
                        if (!root) return null;
                        if (!seen) {
                            try {
                                seen = new WeakSet();
                            } catch (e) {
                                seen = [];
                            }
                        }
                        try {
                            if (typeof seen.has === 'function') {
                                if (seen.has(root)) return null;
                                seen.add(root);
                            } else {
                                for (var si = 0; si < seen.length; si++) {
                                    if (seen[si] === root) return null;
                                }
                                seen.push(root);
                            }
                        } catch (e0) {}
                        try {
                            if (root.querySelector) {
                                var found = root.querySelector(selector);
                                if (found) return found;
                            }
                        } catch (e) {}
                        var kids = [];
                        try {
                            if (root.children) {
                                for (var i = 0; i < root.children.length; i++) kids.push(root.children[i]);
                            }
                        } catch (e2) {}
                        try { if (root.shadowRoot) kids.push(root.shadowRoot); } catch (e3) {}
                        try { if (root.root && root.root !== root.shadowRoot) kids.push(root.root); } catch (e4) {}
                        for (var k = 0; k < kids.length; k++) {
                            var nested = deepFind(kids[k], selector, seen);
                            if (nested) return nested;
                        }
                        return null;
                    }
                    var rich = deepFind(cp, 'ytd-rich-grid-renderer');
                    var two = deepFind(cp, 'ytd-two-column-browse-results-renderer');
                    var contents = deepFind(cp, '#contents');
                    function keys(obj) {
                        if (!obj || typeof obj !== 'object') return 'none';
                        try { return Object.keys(obj).slice(0, 12).join(','); } catch (e) { return 'err'; }
                    }
                    function len(value) {
                        return value && typeof value.length === 'number' ? value.length : 'n/a';
                    }
                    function describe(node) {
                        if (!node) return 'none';
                        var name = 'n/a';
                        var id = '';
                        var kids = 'n/a';
                        var ckids = 'n/a';
                        var connected = 'n/a';
                        try { name = node.tagName || node.nodeName || (node.constructor && node.constructor.name) || 'n/a'; } catch (e) {}
                        try { id = node.id || ''; } catch (e2) {}
                        try { kids = node.children ? node.children.length : 'n/a'; } catch (e3) {}
                        try { ckids = node.childNodes ? node.childNodes.length : 'n/a'; } catch (e4) {}
                        try { connected = node.isConnected; } catch (e5) {}
                        return name + '#' + id + ' kids=' + kids + ' childNodes=' + ckids + ' connected=' + connected;
                    }
                    function dataSummary(el) {
                        var data = null;
                        try { data = el && (el.data || el.__data && el.__data.data); } catch (e) {}
                        var contents = data && (data.contents || data.items || data.richGridRenderer && data.richGridRenderer.contents);
                        var shownItems = null;
                        try { shownItems = el && (el.shownItems || el.__data && el.__data.shownItems); } catch (e2) {}
                        var ownContents = null;
                        try { ownContents = el && (el.contents || el.__data && el.__data.contents); } catch (e3) {}
                        var stampDom = null;
                        var renderJobsMap = null;
                        var hasDataPath = null;
                        var deferredBindingTasks = null;
                        var componentsWithPropertyObservers = null;
                        try { stampDom = el && el.stampDom; } catch (e4) {}
                        try { renderJobsMap = el && el.renderJobsMap_; } catch (e5) {}
                        try { hasDataPath = el && el.hasDataPath_; } catch (e6) {}
                        try { deferredBindingTasks = el && el.deferredBindingTasks_; } catch (e7) {}
                        try { componentsWithPropertyObservers = el && el.componentsWithPropertyObservers_; } catch (e8) {}
                        return 'data=' + !!data +
                            ' dataKeys=' + keys(data) +
                            ' contentsLen=' + len(contents) +
                            ' shownItemsLen=' + len(shownItems) +
                            ' ownContentsLen=' + len(ownContents) +
                            ' dataChanged=' + (el ? typeof el.dataChanged : 'n/a') +
                            ' reflowContent=' + (el ? typeof el.reflowContent : 'n/a') +
                            ' stampDom=' + (stampDom ? Object.keys(stampDom).join(',') : 'none') +
                            ' renderJobsMap=' + (renderJobsMap ? Object.keys(renderJobsMap).join(',') : 'none') +
                            ' hasDataPath=' + (hasDataPath ? Object.keys(hasDataPath).join(',') : 'none') +
                            ' deferredBindingTasks=' + (deferredBindingTasks ? deferredBindingTasks.length : 'none') +
                            ' componentsWithPropertyObservers=' + (componentsWithPropertyObservers ? Object.keys(componentsWithPropertyObservers).join(',') : 'none');
                    }
                    console.log('[yt-probe] timeout100 browse=' +
                        !!(document.querySelector && document.querySelector('ytd-browse')) +
                        ' pmChildren=' + (pm && pm.children ? pm.children.length : -1) +
                        ' currentPage=' + (cp ? cp.is : 'none') +
                        ' cpParent=' + (cp && cp.parentNode ? (cp.parentNode.tagName || cp.parentNode.nodeName) : 'none') +
                        ' cpConnected=' + (cp ? cp.isConnected : 'n/a') +
                        ' pmRootKids=' + (pm && pm.root && pm.root.children ? pm.root.children.length : 'n/a') +
                        ' pmShadowKids=' + (pm && pm.shadowRoot && pm.shadowRoot.children ? pm.shadowRoot.children.length : 'n/a') +
                        ' pmQueryBrowse=' + !!(pm && pm.querySelector && pm.querySelector('ytd-browse')) +
                        ' dataPage=' + (pm && pm.getCurrentData && pm.getCurrentData() ? pm.getCurrentData().page : 'none') +
                        ' two=' + !!two +
                        ' twoConnected=' + (two ? two.isConnected : 'n/a') +
                        ' rich=' + !!rich +
                        ' richParent=' + (rich && rich.parentNode ? (rich.parentNode.id || rich.parentNode.tagName || rich.parentNode.nodeName) : 'none') +
                        ' richConnected=' + (rich ? rich.isConnected : 'n/a') +
                        ' richKids=' + (rich && rich.children ? rich.children.length : 'n/a') +
                        ' contents=' + !!contents +
                        ' contentsKids=' + (contents && contents.children ? contents.children.length : 'n/a') +
                        ' browseData{' + dataSummary(cp) + '}' +
                        ' twoData{' + dataSummary(two) + '}' +
                        ' richData{' + dataSummary(rich) + '}');
                    if (rich) {
                        console.log('[yt-probe] richTree self=' + describe(rich) +
                            ' parent=' + describe(rich.parentNode) +
                            ' root=' + describe(rich.root) +
                            ' shadow=' + describe(rich.shadowRoot) +
                            ' shady=' + describe(rich.__shady_shadowRoot));
                        try {
                            var roots = [rich.parentNode, rich.root, rich.shadowRoot, rich.__shady_shadowRoot];
                            for (var ri = 0; ri < roots.length; ri++) {
                                var rootNode = roots[ri];
                                if (!rootNode) continue;
                                var childInfo = [];
                                var list = rootNode.childNodes || rootNode.children || [];
                                for (var ci = 0; ci < Math.min(8, list.length || 0); ci++) {
                                    var cn = list[ci];
                                    childInfo.push((cn && (cn.tagName || cn.nodeName || cn.constructor && cn.constructor.name)) +
                                        '#' + (cn && cn.id ? cn.id : ''));
                                }
                                console.log('[yt-probe] richTree[' + ri + '] ' + describe(rootNode) + ' children=' + childInfo.join('|'));
                            }
                        } catch (e) {
                            console.log('[yt-probe] richTree error ' + (e && (e.stack || e.message || e)));
                        }
                    }
                    if (rich) {
                        console.log('[yt-probe] richMetrics clientWidth=' + rich.clientWidth +
                            ' offsetWidth=' + rich.offsetWidth +
                            ' containerWidth=' + rich.containerWidth +
                            ' elementsPerRow=' + rich.elementsPerRow +
                            ' isReflowing=' + rich.isReflowing);
                        try {
                            var richData = rich.data || (rich.__data && rich.__data.data);
                            var firstItem = richData && richData.contents && richData.contents[0];
                            var itemKey = firstItem ? Object.keys(firstItem).slice(0, 20).join(',') : 'none';
                            var nestedKey = firstItem && firstItem.richItemRenderer ? 'richItemRenderer' :
                                firstItem && firstItem.richSectionRenderer ? 'richSectionRenderer' :
                                firstItem && firstItem.continuationItemRenderer ? 'continuationItemRenderer' :
                                firstItem && firstItem.richShelfRenderer ? 'richShelfRenderer' : 'none';
                            var nested = firstItem && firstItem.richSectionRenderer ? firstItem.richSectionRenderer :
                                firstItem && firstItem.richItemRenderer ? firstItem.richItemRenderer :
                                firstItem && firstItem.continuationItemRenderer ? firstItem.continuationItemRenderer :
                                firstItem && firstItem.richShelfRenderer ? firstItem.richShelfRenderer : null;
                            var nestedKeys = nested ? Object.keys(nested).slice(0, 24).join(',') : 'none';
                            var nestedContentLen = nested && nested.contents && nested.contents.length;
                            var innerContent = nested && nested.content;
                            var innerKeys = innerContent ? Object.keys(innerContent).slice(0, 24).join(',') : 'none';
                            var nestedTitle = nested && nested.title && (nested.title.simpleText || (nested.title.runs && nested.title.runs[0] && nested.title.runs[0].text));
                            console.log('[yt-probe] richFirstItem keys=' + itemKey + ' nested=' + nestedKey +
                                ' nestedKeys=' + nestedKeys +
                                ' nestedContentLen=' + (nestedContentLen == null ? 'n/a' : nestedContentLen) +
                                ' innerKeys=' + innerKeys +
                                ' nestedTitle=' + (nestedTitle || 'none'));
                        } catch (e) {
                            console.log('[yt-probe] richFirstItem error ' + (e && (e.stack || e.message || e)));
                        }
                        try {
                            if (rich.data && rich.data.contents && (!rich.shownItems || rich.shownItems.length === 0)) {
                                if (typeof rich.dataChanged === 'function') {
                                    rich.dataChanged();
                                    console.log('[yt-probe] manual rich.dataChanged called');
                                } else if (typeof rich.reflowContent === 'function') {
                                    rich.reflowContent(true);
                                    console.log('[yt-probe] manual rich.reflowContent called');
                                }
                                Promise.resolve().then(function() {
                                    var contentsAgain = deepFind(cp, '#contents');
                                    console.log('[yt-probe] after manual rich shownItems=' +
                                        (rich.shownItems ? rich.shownItems.length : 'n/a') +
                                        ' contentsKids=' + (contentsAgain && contentsAgain.children ? contentsAgain.children.length : 'n/a') +
                                        ' containerWidth=' + rich.containerWidth +
                                        ' elementsPerRow=' + rich.elementsPerRow +
                                        ' isReflowing=' + rich.isReflowing);
                                });
                                setTimeout(function() {
                                    var contentsLater = deepFind(cp, '#contents');
                                    console.log('[yt-probe] after manual rich timeout shownItems=' +
                                        (rich.shownItems ? rich.shownItems.length : 'n/a') +
                                        ' contentsKids=' + (contentsLater && contentsLater.children ? contentsLater.children.length : 'n/a') +
                                        ' containerWidth=' + rich.containerWidth +
                                        ' elementsPerRow=' + rich.elementsPerRow +
                                        ' isReflowing=' + rich.isReflowing);
                                    if (rich.data && rich.data.contents && (!rich.shownItems || rich.shownItems.length === 0)) {
                                        try {
                                            rich.isReflowing = false;
                                            var forcedItems = rich.data.contents.slice ? rich.data.contents.slice() : rich.data.contents;
                                            if (typeof rich.set === 'function') rich.set('shownItems', forcedItems);
                                            else rich.shownItems = forcedItems;
                                            if (typeof rich.notifyPath === 'function') rich.notifyPath('shownItems', rich.shownItems || forcedItems);
                                            console.log('[yt-probe] manual rich.shownItems assigned len=' +
                                                (rich.shownItems ? rich.shownItems.length : 'n/a'));
                                        } catch (e) {
                                            console.log('[yt-probe] manual shownItems error ' + (e && (e.stack || e.message || e)));
                                        }
                                        try {
                                            var forcedContents = rich.data.contents.slice ? rich.data.contents.slice() : rich.data.contents;
                                            if (typeof rich.set === 'function') rich.set('contents', forcedContents);
                                            else rich.contents = forcedContents;
                                            if (typeof rich.notifyPath === 'function') rich.notifyPath('contents', rich.contents || forcedContents);
                                            console.log('[yt-probe] manual rich.contents assigned len=' +
                                                (rich.contents ? rich.contents.length : 'n/a'));
                                        } catch (e) {
                                            console.log('[yt-probe] manual contents error ' + (e && (e.stack || e.message || e)));
                                        }
                                        setTimeout(function() {
                                            var contentsAssigned = deepFind(cp, '#contents');
                                            console.log('[yt-probe] after shownItems assign shownItems=' +
                                                (rich.shownItems ? rich.shownItems.length : 'n/a') +
                                                ' contentsKids=' + (contentsAssigned && contentsAssigned.children ? contentsAssigned.children.length : 'n/a'));
                                        }, 0);
                                    }
                                }, 0);
                            }
                        } catch (e) {
                            console.log('[yt-probe] manual rich error ' + (e && (e.stack || e.message || e)));
                        }
                        try {
                            var dropdowns = document.querySelectorAll ? document.querySelectorAll('tp-yt-iron-dropdown') : null;
                            console.log('[yt-probe] dropdown count=' + (dropdowns ? dropdowns.length : 'n/a'));
                            for (var dx = 0; dropdowns && dx < Math.min(3, dropdowns.length); dx++) {
                                var dropdown = dropdowns[dx];
                                var dKeys = Object.keys(dropdown).slice(0, 40).join(',');
                                var dProto = [];
                                var proto = Object.getPrototypeOf(dropdown);
                                while (proto && dProto.length < 40) {
                                    try {
                                        var names = Object.getOwnPropertyNames(proto);
                                        for (var ni = 0; ni < names.length && dProto.length < 40; ni++) {
                                            if (dProto.indexOf(names[ni]) < 0) dProto.push(names[ni]);
                                        }
                                    } catch (e) {}
                                    proto = Object.getPrototypeOf(proto);
                                }
                                console.log('[yt-probe] dropdown[' + dx + '] keys=' + dKeys +
                                    ' opened=' + dropdown.opened +
                                    ' disabled=' + dropdown.disabled +
                                    ' verticalAlign=' + dropdown.verticalAlign +
                                    ' horizontalAlign=' + dropdown.horizontalAlign +
                                    ' focusTarget=' + dropdown.focusTarget +
                                    ' restoreFocusOnClose=' + dropdown.restoreFocusOnClose +
                                    ' opened=' + dropdown.opened +
                                    ' proto=' + dProto.join(','));
                            }
                        } catch (e) {
                            console.log('[yt-probe] dropdown probe error ' + (e && (e.stack || e.message || e)));
                        }
                    }
                }, 100);
            } catch (e) {
                console.log('[yt-probe] fatal ' + (e && (e.stack || e.message || e)));
            }
        })();
        "#,
    );
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
    #[cfg(feature = "engine-spidermonkey")]
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
