use super::*;

#[test]
fn detached_subtree_mutations_are_mirrored_on_insertion() {
    let dom = blank_dom();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    runtime.execute(r#"
        var host = document.createElement('div');
        host.setAttribute('data-state', 'old');
        host.removeAttribute('data-state');
        var root = host.attachShadow({mode: 'open'});
        var label = document.createElement('span');
        label.id = 'detached-label';
        var text = document.createTextNode('before');
        text.textContent = 'after';
        label.appendChild(text);
        root.appendChild(label);
        label.setAttribute('style', 'display:block;width:120px;height:30px');
        document.body.appendChild(host);
    "#).unwrap();
    assert_eq!(runtime.take_snapshot_rebuild_reason(), None);
    render_doc.borrow().validate_mirror_integrity().unwrap();
    runtime.perform_style_and_layout();
    assert_eq!(runtime.eval_to_string(r#"[
        label.textContent, host.hasAttribute('data-state'),
        label.getBoundingClientRect().width, label.getBoundingClientRect().height
    ].join('|')"#), Ok("after|false|120|30".into()));
}

#[test]
fn class_list_supports_iteration() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(runtime.eval_to_string(r#"(() => {
        const el = document.createElement('div');
        el.className = 'watch-flexy video-page';
        const classes = [];
        for (const name of el.classList) classes.push(name);
        return classes.join('|');
    })()"#), Ok("watch-flexy|video-page".into()));
}

#[test]
fn attributes_support_iteration() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(runtime.eval_to_string(r#"(() => {
        const el = document.createElement('div');
        el.setAttribute('alpha', '1');
        el.setAttribute('beta', '2');
        const attrs = [];
        for (const attr of el.attributes) attrs.push(attr.name + '=' + attr.value);
        return attrs.sort().join('|');
    })()"#), Ok("alpha=1|beta=2".into()));
}

#[test]
fn hidden_property_reflects_its_boolean_attribute() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(runtime.eval_to_string(r#"(() => {
        const el = document.createElement('div');
        el.hidden = true;
        const set = [el.hidden, el.hasAttribute('hidden'), el.getAttribute('hidden') === ''];
        el.hidden = false;
        set.push(el.hidden, el.hasAttribute('hidden'));
        return set.join('|');
    })()"#), Ok("true|true|true|false|false".into()));
}

#[test]
fn polymer_property_enablement_stamps_once() {
    let mut runtime = V8Runtime::new(blank_dom());
    assert_eq!(runtime.eval_to_string(r#"(() => {
        class PolymerElement extends HTMLElement {
            constructor() { super(); this.__dataEnabled = false; this.readyCount = 0; }
            _enableProperties() {
                if (this.__dataEnabled) return;
                this.__dataEnabled = true;
                this.ready();
            }
            ready() { this.readyCount++; this.appendChild(document.createElement('span')); }
            connectedCallback() { this._enableProperties(); }
        }
        customElements.define('polymer-once', PolymerElement);
        const el = document.createElement('polymer-once');
        document.body.appendChild(el);
        return [el.readyCount, el.childNodes.length].join('|');
    })()"#), Ok("1|1".into()));
}

#[test]
fn native_input_contents_are_not_dom_children() {
    let dom = Parser::new("<html><body><input type='file'><input type='submit' value='Go'></body></html>").parse_document();
    let render_doc = BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap();
    render_doc.validate_mirror_integrity().unwrap();
}

#[test]
fn shadow_slots_keep_the_composed_render_tree_in_sync() {
    let dom = blank_dom();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    for script in [
        "var host = document.createElement('div'); document.body.appendChild(host); var root = host.attachShadow({mode:'open'});",
        "var slot = document.createElement('slot'); root.appendChild(slot);",
        "slot.innerHTML = '<b>fallback</b>';",
        "host.innerHTML = '<span>slotted</span>';",
        "var extra = document.createElement('i'); extra.textContent = 'extra'; host.appendChild(extra);",
        "host.removeChild(extra);",
        "host.innerHTML = '';",
        "slot.textContent = 'new fallback';",
        "root.removeChild(slot);",
    ] {
        runtime.execute(script).unwrap();
        render_doc.borrow().validate_mirror_integrity().unwrap_or_else(|err| panic!("{script}: {err:?}"));
    }
}

#[test]
fn slots_inside_an_inserted_subtree_render_the_host_children() {
    let mut runtime = runtime_with_render_doc(blank_dom());
    runtime.execute(r#"
        var host = document.createElement('div');
        host.innerHTML = '<span slot="top" style="display:block;height:20px">top</span>';
        document.body.appendChild(host);
        var root = host.attachShadow({mode:'open'});
        var wrapper = document.createElement('div');
        wrapper.appendChild(document.createElement('slot')).setAttribute('name', 'top');
        root.appendChild(wrapper);
    "#).unwrap();
    runtime.perform_style_and_layout();
    let height = "String(host.firstChild.getBoundingClientRect().height)";
    assert_eq!(runtime.eval_to_string(height), Ok("20".into()));

    runtime.execute("root.removeChild(wrapper);").unwrap();
    runtime.perform_style_and_layout();
    assert_eq!(runtime.eval_to_string(height), Ok("0".into()));
}

#[test]
fn shadow_root_does_not_add_a_layout_box() {
    let mut runtime = runtime_with_render_doc(blank_dom());
    runtime.execute(r#"
        var host = document.createElement('div');
        host.setAttribute('style', 'display:flex; width:300px');
        document.body.appendChild(host);
        var root = host.attachShadow({mode:'open'});
        root.innerHTML = '<div style="width:100px;height:20px">one</div><div style="width:100px;height:20px">two</div>';
    "#).unwrap();
    runtime.perform_style_and_layout();
    assert_eq!(runtime.eval_to_string(r#"(() => {
        var a = root.firstChild.getBoundingClientRect();
        var b = root.lastChild.getBoundingClientRect();
        return [b.x - a.x, b.y - a.y].join('|');
    })()"#), Ok("100|0".into()));
}

#[test]
fn replacing_nested_children_keeps_render_mappings_valid() {
    let dom = blank_dom();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    runtime.execute("var host = document.createElement('div'); document.body.appendChild(host);").unwrap();
    for script in [
        "host.innerHTML = '<section><span>old</span></section>'; var old = host.firstChild;",
        "host.textContent = 'replacement';",
        "host.innerHTML = '<article><b>new</b></article>';",
        "host.appendChild(old);",
        "var fragment = document.createDocumentFragment(); while (host.firstChild) fragment.appendChild(host.firstChild);",
        "host.appendChild(fragment);",
        "old.textContent = 'reattached';",
        "host.innerHTML = '';",
    ] {
        runtime.execute(script).unwrap();
        render_doc.borrow().validate_mirror_integrity().unwrap_or_else(|err| panic!("{script}: {err:?}"));
    }
}

#[test]
fn root_element_client_size_is_the_viewport() {
    let dom = Parser::new("<html><body><p style='height:5000px'>tall</p></body></html>").parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    let size = "[document.documentElement.clientWidth, document.documentElement.clientHeight].join('x')";
    assert_eq!(runtime.eval_to_string(size), Ok("800x600".into()));
    render_doc.borrow_mut().set_viewport(1000, 700);
    assert_eq!(runtime.eval_to_string(size), Ok("1000x700".into()));
}

#[test]
fn image_loads_fire_load_and_error() {
    let dom = blank_dom();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    runtime
        .execute(
            r#"
        var log = [];
        function add(name, src) {
            var img = document.createElement('img');
            img.addEventListener('load', function(e) { log.push(name + ' load ' + e.isTrusted + ' ' + img.naturalWidth + 'x' + img.naturalHeight); });
            img.addEventListener('error', function() { log.push(name + ' error ' + img.complete + ' ' + img.naturalWidth); });
            img.onload = function() { log.push(name + ' onload'); };
            document.body.appendChild(img);
            img.src = src;
            return img;
        }
        // A 3x2 PNG, a GIF that is not one, and one whose src is removed again.
        var ok = add('ok', 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAADElEQVR4nGNgIA4AAABUAAG5uxhVAAAAAElFTkSuQmCC');
        add('broken', 'data:image/gif;base64,AAAA');
        add('removed', 'data:image/gif;base64,AAAA').removeAttribute('src');
        log.push('set ' + ok.complete);
        var empty = document.createElement('img');
        log.push('empty ' + empty.complete);
    "#,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while runtime.has_pending_loads() && std::time::Instant::now() < deadline {
        runtime.tick(crate::clock::now());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        runtime.eval_to_string("log.slice().sort().join(' | ')"),
        Ok("broken error true 0 | empty true | ok load true 3x2 | ok onload | set false".into())
    );
}

#[test]
fn intersection_observer_follows_layout_and_scrolling() {
    let dom = Parser::new(
        "<html><body style='margin:0'><div id='top' style='height:100px'></div>\
         <div style='height:2000px'></div><div id='low' style='height:200px'></div>\
         <div id='gone' style='display:none'></div></body></html>",
    )
    .parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    runtime
        .execute(
            r#"
        var log = [];
        var observer = new IntersectionObserver(function(entries, self) {
            entries.forEach(function(e) {
                log.push(e.target.id + ' ' + e.isIntersecting + ' ' + e.intersectionRatio + ' @' + e.boundingClientRect.top
                    + (self === observer ? '' : ' wrong observer'));
            });
        }, { threshold: [0, 0.5] });
        ['top', 'low', 'gone'].forEach(function(id) { observer.observe(document.getElementById(id)); });
        function takeLog() { return log.splice(0).join(', '); }
    "#,
        )
        .unwrap();
    let settle = |runtime: &mut V8Runtime, scroll_y: f64| {
        runtime.set_scroll_position(0.0, scroll_y);
        runtime.tick(crate::clock::now());
        runtime.drain_animation_frame_callbacks(crate::clock::now());
        runtime.eval_to_string("takeLog()").unwrap()
    };
    assert_eq!(
        settle(&mut runtime, 0.0),
        "top true 1 @0, low false 0 @2100, gone false 0 @0",
        "every target is reported once initially"
    );
    assert_eq!(settle(&mut runtime, 0.0), "");
    assert_eq!(settle(&mut runtime, 1650.0), "top false 0 @-1650, low true 0.75 @450");
    assert_eq!(settle(&mut runtime, 1700.0), "", "0.75 and 1 are past the same thresholds");
    runtime.execute("observer.unobserve(document.getElementById('low'))").unwrap();
    assert_eq!(settle(&mut runtime, 0.0), "top true 1 @0");
    assert_eq!(
        runtime.eval_to_string(
            "['intersectionRatio' in IntersectionObserverEntry.prototype, observer.rootMargin, observer.thresholds.join()].join('|')"
        ),
        Ok("true|0px 0px 0px 0px|0,0.5".into())
    );
}

#[test]
fn resize_observer_reports_layout_changes_in_depth_order() {
    let dom = Parser::new(
        "<html><body style='margin:0'>\
         <div id='outer' style='width:200px;height:100px;padding:10px;border:5px solid'>\
         <div id='inner' style='width:50px;height:20px'></div></div>\
         <div id='empty' style='display:none'></div></body></html>",
    )
    .parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    runtime
        .execute(
            r#"
        var log = [];
        var $ = function(id) { return document.getElementById(id); };
        function describe(e) {
            var r = e.contentRect, b = e.borderBoxSize[0];
            return e.target.id + ' ' + [r.x, r.y, r.width, r.height].join(',') + ' border ' + b.inlineSize + 'x' + b.blockSize;
        }
        var observer = new ResizeObserver(function(entries, self) {
            entries.forEach(function(e) { log.push(describe(e) + (self === observer ? '' : ' wrong observer')); });
        });
        observer.observe($('outer'));
        observer.observe($('empty'));
        var borders = new ResizeObserver(function(entries) {
            entries.forEach(function(e) { log.push('border-box ' + e.target.id); });
        });
        borders.observe($('inner'), { box: 'border-box' });
        window.addEventListener('error', function(e) { log.push('error ' + e.message); });
        function takeLog() { return log.splice(0).join(' | '); }
    "#,
        )
        .unwrap();
    let mut frame = |runtime: &mut V8Runtime, script: &str| {
        runtime.execute(script).unwrap();
        runtime.tick(crate::clock::now());
        runtime.eval_to_string("takeLog()").unwrap()
    };
    assert_eq!(
        frame(&mut runtime, ""),
        "outer 10,10,200,100 border 230x130 | border-box inner",
        "rendered, non-empty targets fire once on observe"
    );
    assert_eq!(frame(&mut runtime, ""), "", "nothing changed");
    assert_eq!(
        frame(&mut runtime, "$('outer').style.width = '300px'"),
        "outer 10,10,300,100 border 330x130"
    );
    assert_eq!(frame(&mut runtime, "$('outer').style.display = 'none'"), "outer 0,0,0,0 border 0x0 | border-box inner");
    assert_eq!(frame(&mut runtime, "$('outer').style.display = 'block'"), "outer 10,10,300,100 border 330x130 | border-box inner");

    // A callback that grows a deeper element is delivered in the same frame;
    // one that grows a shallower element waits for the next frame.
    let deeper = "var grow = new ResizeObserver(function(entries) { \
                      log.push('grow ' + entries[0].target.id); $('inner').style.height = '40px'; }); \
                  grow.observe($('outer'), { box: 'border-box' });";
    assert_eq!(frame(&mut runtime, deeper), "grow outer | border-box inner");
    runtime.execute("grow.disconnect(); borders.disconnect();").unwrap();
    let shallower = "var shrink = new ResizeObserver(function(entries) { \
                         log.push('shrink ' + entries[0].target.id); $('outer').style.width = '100px'; }); \
                     shrink.observe($('inner'));";
    assert_eq!(
        frame(&mut runtime, shallower),
        "shrink inner | error ResizeObserver loop completed with undelivered notifications."
    );
    assert_eq!(frame(&mut runtime, ""), "outer 10,10,100,100 border 130x130");

    runtime.execute("observer.disconnect(); shrink.disconnect()").unwrap();
    assert_eq!(frame(&mut runtime, "$('outer').style.width = '50px'"), "");
    assert_eq!(
        runtime.eval_to_string(
            "(() => { try { new ResizeObserver(function() {}).observe($('outer'), { box: 'margin-box' }); return 'no throw'; } \
             catch (e) { return e.constructor.name; } })()"
        ),
        Ok("TypeError".into())
    );
}

#[test]
fn page_script_scrolls_the_viewport_and_sees_scroll_events() {
    let dom = Parser::new("<html><body style='margin:0'><div style='height:2600px'></div></body></html>")
        .parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    runtime
        .execute(
            r#"
        var log = [];
        document.addEventListener('scroll', function(e) { log.push('document ' + e.bubbles + ' ' + scrollY); });
        window.addEventListener('scroll', function() { log.push('window ' + pageYOffset); });
        function takeLog() { return log.splice(0).join(', '); }
    "#,
        )
        .unwrap();
    let tick = |runtime: &mut V8Runtime| {
        runtime.tick(crate::clock::now());
        runtime.eval_to_string("takeLog()").unwrap()
    };

    runtime.execute("scrollTo(0, 500); scrollBy({top: 100}); scroll({left: 0})").unwrap();
    assert_eq!(runtime.eval_to_string("String(scrollY)"), Ok("600".into()));
    assert_eq!(runtime.take_scroll_request(), Some((0.0, 600.0)));
    assert_eq!(runtime.take_scroll_request(), None);
    assert_eq!(tick(&mut runtime), "document true 600, window 600", "one event per frame");
    assert_eq!(tick(&mut runtime), "");

    runtime.execute("scrollTo(0, 1e9)").unwrap();
    assert_eq!(runtime.eval_to_string("String(scrollY)"), Ok("2000".into()), "clamped to the page");
    runtime.set_scroll_position(0.0, 0.0);
    assert_eq!(runtime.eval_to_string("String(scrollY)"), Ok("2000".into()), "script wins until taken");
    assert_eq!(runtime.take_scroll_request(), Some((0.0, 2000.0)));
    tick(&mut runtime);

    runtime.set_scroll_position(0.0, 300.0);
    assert_eq!(tick(&mut runtime), "document true 300, window 300", "the window's scrolls fire too");
    runtime.set_scroll_position(0.0, 300.0);
    assert_eq!(tick(&mut runtime), "");
}

#[test]
fn client_rects_and_hit_tests_are_viewport_relative() {
    let dom = Parser::new(
        "<html><body style='margin:0'><div id='bar' style='position:fixed;top:0;left:0;width:100px;height:40px'></div>\
         <div style='height:1000px'></div><div id='box' style='height:100px'></div>\
         <div style='height:1500px'></div></body></html>",
    )
    .parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    let probe = r#"(() => {
        const top = (id) => document.getElementById(id).getBoundingClientRect().top;
        const hit = (x, y) => { const el = document.elementFromPoint(x, y); return el ? el.id || el.tagName : 'null'; };
        return [top('box'), top('bar'), hit(10, 10), hit(500, 1050 - scrollY), hit(10, -1), hit(10, 601),
            document.documentElement.scrollHeight, document.documentElement.clientHeight].join(' ');
    })()"#;
    assert_eq!(runtime.eval_to_string(probe), Ok("1000 0 bar null null null 2600 600".into()), "the box is below the viewport");
    runtime.set_scroll_position(0.0, 700.0);
    assert_eq!(
        runtime.eval_to_string(probe),
        Ok("300 0 bar box null null 2600 600".into()),
        "scrolled boxes move up; the fixed bar stays at the top"
    );
}

#[test]
fn inserted_scripts_run_in_order_and_fire_load() {
    let dom = blank_dom();
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc));
    runtime
        .execute(
            r#"
        var log = [];
        function js(code) { return 'data:text/javascript,' + encodeURIComponent(code); }
        function add(name, src, ordered) {
            var s = document.createElement('script');
            log.push(name + ' async ' + s.async);
            if (ordered) s.async = false;
            s.addEventListener('load', function(e) { log.push(name + ' load ' + e.isTrusted); });
            s.onerror = function() { log.push(name + ' error'); };
            if (src !== null) s.src = src;
            document.head.appendChild(s);
            return s;
        }
        add('one', js("log.push('one ran ' + (document.currentScript.id || '-'))"), true).id = 'first';
        add('two', js("log.push('two ran'); throw new Error('boom')"), true);
        add('bad', 'nope:', true);
        add('late', null, true).src = js("log.push('late ran')");
        var detached = document.createElement('script');
        detached.src = js("log.push('detached ran')");
        var box = document.createElement('div');
        box.innerHTML = '<script src="' + js("log.push('innerHTML ran')") + '"><\/script>';
        document.body.appendChild(box);
        log.push('inserted');
    "#,
        )
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while runtime.has_pending_loads() && std::time::Instant::now() < deadline {
        runtime.tick(crate::clock::now());
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // Moving a script that ran does not run it again.
    runtime.execute("document.body.appendChild(document.getElementById('first'))").unwrap();
    runtime.tick(crate::clock::now());
    assert_eq!(
        runtime.eval_to_string("log.join(' | ') + ' | ' + document.currentScript"),
        Ok("one async true | two async true | bad async true | late async true | inserted \
            | one ran first | one load true | two ran | two load true | bad error \
            | late ran | late load true | null"
            .into())
    );
}

#[test]
fn window_resize_updates_viewport_globals_events_and_match_media() {
    let dom = Parser::new(
        "<html><head><style>\
         #box { width: 100px; height: 20px; }\
         @media (min-width: 900px) { #box { width: 300px; } }\
         </style></head><body><div id='box'></div></body></html>",
    )
    .parse_document();
    crate::dom::reparent_subtree(&dom);
    let render_doc = Rc::new(RefCell::new(
        BlitzDocument::try_from_dom(&dom, None, &test_identity(), 800, 600).unwrap(),
    ));
    let mut runtime = V8Runtime::with_render_document(dom, Some(render_doc.clone()));
    runtime
        .execute(
            r#"
        var log = [];
        var wide = matchMedia('(min-width: 900px)');
        var portrait = matchMedia('(orientation: portrait)');
        var light = matchMedia('(prefers-color-scheme: light)');
        var multi = matchMedia('(max-width: 500px), (min-width: 900px) and (min-height: 700px)');
        var invalid = matchMedia('not all; { body {} }');
        log.push('init:' + innerWidth + 'x' + innerHeight + '/' + outerWidth + 'x' + outerHeight
            + '/screen:' + screen.width + 'x' + screen.height
            + '/vv:' + visualViewport.width + 'x' + visualViewport.height
            + '/wide:' + wide.matches + ':' + wide.media
            + '/portrait:' + portrait.matches
            + '/light:' + light.matches
            + '/multi:' + multi.matches
            + '/invalid:' + invalid.matches + ':' + invalid.media
            + '/box:' + document.getElementById('box').getBoundingClientRect().width);
        window.addEventListener('resize', function(e) {
            log.push('win:' + e.isTrusted + ':' + innerWidth + 'x' + innerHeight);
        });
        window.onresize = function() {
            log.push('onresize:' + innerWidth);
        };
        visualViewport.addEventListener('resize', function(e) {
            log.push('vv:' + e.isTrusted + ':' + visualViewport.width + 'x' + visualViewport.height);
        });
        visualViewport.onresize = function() {
            log.push('vvonresize:' + visualViewport.width);
        };
        wide.addEventListener('change', function(e) {
            log.push('wide-ev:' + (e instanceof MediaQueryListEvent) + ':' + e.isTrusted + ':' + e.matches + ':' + e.media);
        });
        wide.onchange = function(e) {
            log.push('wide-on:' + e.matches);
        };
        wide.addListener(function(e) {
            log.push('wide-legacy:' + e.matches);
        });
        portrait.addListener(function(e) {
            log.push('portrait:' + e.matches);
        });
    "#,
        )
        .unwrap();

    // Ticking without a resize must not fire resize or change events.
    runtime.tick(crate::clock::now());
    assert_eq!(
        runtime.eval_to_string("log.join(' | ')"),
        Ok("init:800x600/800x600/screen:800x600/vv:800x600/wide:false:(min-width: 900px)\
            /portrait:false/light:true/multi:false/invalid:false:not all/box:100"
            .into())
    );

    // Resize wider and taller: wide and multi flip true, portrait stays false, @media applies.
    render_doc.borrow_mut().set_viewport(1000, 750);
    runtime.tick(crate::clock::now());
    assert_eq!(
        runtime.eval_to_string("log.slice(1).join(' | ') + ' | box:' + document.getElementById('box').getBoundingClientRect().width + ' | multi:' + multi.matches"),
        Ok("win:true:1000x750 | onresize:1000 | vv:true:1000x750 | vvonresize:1000 \
            | wide-ev:true:true:true:(min-width: 900px) | wide-on:true | wide-legacy:true \
            | box:300 | multi:true"
            .into())
    );

    // Resize into portrait below 900px: wide flips false, portrait flips true.
    render_doc.borrow_mut().set_viewport(600, 900);
    runtime.tick(crate::clock::now());
    assert_eq!(
        runtime.eval_to_string("log.slice(8).join(' | ') + ' | box:' + document.getElementById('box').getBoundingClientRect().width"),
        Ok("win:true:600x900 | onresize:600 | vv:true:600x900 | vvonresize:600 \
            | wide-ev:true:true:false:(min-width: 900px) | wide-on:false | wide-legacy:false \
            | portrait:true | box:100"
            .into())
    );
}
