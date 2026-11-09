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
