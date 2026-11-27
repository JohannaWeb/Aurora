use super::V8Runtime;
use crate::html::Parser;

#[path = "../../codegen/webidl.rs"]
mod webidl;

fn runtime() -> V8Runtime {
    V8Runtime::new(
        Parser::new("<html><head></head><body><div id='parsed'></div></body></html>")
            .parse_document(),
    )
}

#[test]
fn generated_interfaces_brand_parsed_and_created_nodes() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const nodes = [
            [document, Document], [document.documentElement, HTMLHtmlElement],
            [document.head, HTMLHeadElement], [document.body, HTMLBodyElement],
            [document.getElementById('parsed'), HTMLDivElement],
            [document.createElement('div'), HTMLDivElement],
            [document.createElement('a'), HTMLAnchorElement],
            [document.createElement('video'), HTMLVideoElement],
            [document.createElement('audio'), HTMLAudioElement],
            [document.createElement('section'), HTMLElement],
            [document.createElement('notanelement'), HTMLUnknownElement],
            [document.createElement('x-undefined'), HTMLElement],
            [document.createTextNode('text'), Text], [document.createComment('note'), Comment],
            [document.createDocumentFragment(), DocumentFragment],
            [document.createElement('template').content, DocumentFragment],
            [document.createElement('div').attachShadow({mode:'open'}), ShadowRoot]
        ];
        return nodes.every(([node, Ctor]) => Object.getPrototypeOf(node) === Ctor.prototype
            && node instanceof Node && node instanceof EventTarget
            && Object.prototype.toString.call(node) === '[object ' + Ctor.name + ']');
    })()"#
        ),
        Ok("true".into())
    );
}

#[test]
fn generated_constructor_and_prototype_inheritance_matches_idl() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const chain = [[Node, EventTarget], [Element, Node], [HTMLElement, Element],
            [HTMLDivElement, HTMLElement], [HTMLMediaElement, HTMLElement],
            [HTMLVideoElement, HTMLMediaElement], [HTMLAudioElement, HTMLMediaElement],
            [ShadowRoot, DocumentFragment], [Text, CharacterData], [CDATASection, Text]];
        return chain.every(([child, parent]) => Object.getPrototypeOf(child) === parent
            && Object.getPrototypeOf(child.prototype) === parent.prototype
            && child.prototype.constructor === child
            && Function.prototype.toString.call(child).includes('[native code]'));
    })()"#
        ),
        Ok("true".into())
    );
    assert_eq!(runtime.eval_to_string(r#"(() => {
        const tag = Object.getOwnPropertyDescriptor(HTMLDivElement.prototype, Symbol.toStringTag);
        const iface = Object.getOwnPropertyDescriptor(window, 'HTMLDivElement');
        const prototype = Object.getOwnPropertyDescriptor(HTMLDivElement, 'prototype');
        return tag.value === 'HTMLDivElement' && !tag.writable && !tag.enumerable && tag.configurable
            && iface.writable && !iface.enumerable && iface.configurable
            && !prototype.writable && !prototype.configurable && HTMLDocument === Document;
    })()"#), Ok("true".into()));
}

#[test]
fn wrapper_prototypes_do_not_depend_on_replaced_globals() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const proto = HTMLDivElement.prototype;
        window.HTMLDivElement = function Fake() {};
        return Object.getPrototypeOf(document.createElement('div')) === proto;
    })()"#
        ),
        Ok("true".into())
    );
}

#[test]
fn native_html_constructor_preserves_custom_element_construction() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        class Example extends HTMLElement {
            constructor() { super(); this.setAttribute('constructed', 'yes'); }
        }
        customElements.define('x-example', Example);
        const node = new Example();
        return node instanceof Example && node instanceof HTMLElement && node instanceof Node
            && node.localName === 'x-example' && node.getAttribute('constructed') === 'yes';
    })()"#
        ),
        Ok("true".into())
    );
}

#[test]
fn document_has_a_single_wrapper() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const p = document.createElement('p');
        document.body.appendChild(p);
        return [
            document.documentElement.parentNode === document,
            document.body.ownerDocument === document,
            p.ownerDocument === document,
            document.body.parentNode.parentNode === document,
            document.nodeType, document.nodeName,
            Object.getPrototypeOf(document) === Document.prototype,
            typeof document.createElement, typeof document.getElementById,
            typeof document.elementFromPoint,
        ].join('|');
    })()"#
        ),
        Ok("true|true|true|true|9|#document|true|function|function|function".into())
    );
}

#[test]
fn generated_operation_brand_checks_and_converts_arguments() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const body = document.body, div = document.getElementById('parsed');
        const detached = document.createElement('p');
        const contains = Node.prototype.contains;
        const thrown = f => { try { f(); return 'none'; } catch (e) { return e.constructor.name; } };
        return [
            document.contains(div), body.contains(body), div.contains(body),
            body.contains(detached), body.contains(null), body.contains(undefined),
            thrown(() => body.contains()), thrown(() => body.contains({})),
            thrown(() => contains.call({}, body)), thrown(() => new contains(body)),
            contains.name, contains.length, Object.hasOwn(div, 'contains'),
        ].join('|');
    })()"#
        ),
        Ok("true|true|false|false|false|false|TypeError|TypeError|TypeError|TypeError|contains|1|false".into())
    );
}

#[test]
fn character_data_members_count_utf16_units_and_render_text() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const text = document.createTextNode('a\u{1F600}b');
        document.body.appendChild(text);
        const out = [Object.hasOwn(text, 'data'), text.length, text.substringData(1, 2) === '\u{1F600}'];
        text.appendData('cd'); text.insertData(0, '>'); text.deleteData(1, 1);
        text.replaceData(0, 3, 'X');
        out.push(text.data, document.body.textContent);
        text.data = null;
        out.push(JSON.stringify(text.data), text.length);
        const comment = document.createComment('note');
        comment.data = 7;
        out.push(comment.substringData(0, 4294967296 + 1), comment instanceof CharacterData);
        for (const call of [() => text.substringData(1, 0), () => text.insertData(-1, 'x')]) {
            try { call(); out.push('no throw'); } catch (e) { out.push(e instanceof DOMException && e.name); }
        }
        for (const call of [() => CharacterData.prototype.appendData.call(document.body, 'x'),
                            () => comment.replaceData(0, 1)]) {
            try { call(); out.push('no throw'); } catch (e) { out.push(e.constructor.name); }
        }
        return out.join();
    })()"#
        ),
        Ok("false,4,true,Xbcd,Xbcd,\"\",0,7,true,IndexSizeError,IndexSizeError,TypeError,TypeError"
            .into())
    );
}

#[test]
fn shadydom_descriptor_accessors_live_on_prototypes() {
    // ShadyDOM switches to copying these from the prototypes once
    // Node.prototype.firstChild is a configurable getter; a missing one
    // leaves its __shady_native_ copy undefined.
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const node = ['parentNode', 'firstChild', 'lastChild', 'previousSibling', 'nextSibling',
            'childNodes', 'parentElement', 'textContent'];
        const element = ['firstElementChild', 'lastElementChild', 'children', 'childElementCount',
            'previousElementSibling', 'nextElementSibling', 'innerHTML', 'className'];
        const div = document.getElementById('parsed');
        const bad = [...node.map(n => [Node.prototype, n]), ...element.map(n => [Element.prototype, n])]
            .filter(([proto, name]) => {
                const d = Object.getOwnPropertyDescriptor(proto, name);
                return !d || !d.configurable || typeof d.get !== 'function' || Object.hasOwn(div, name);
            }).map(([, name]) => name);
        div.innerHTML = '<b>x</b>text<i></i>';
        bad.push(div.childNodes.length, div.children.length, div.firstElementChild.localName,
            div.firstChild.nextSibling.textContent, div.lastElementChild.previousElementSibling.localName,
            document.ownerDocument, div.ownerDocument === document, document.body.firstChild.parentNode === document.body);
        return bad.join();
    })()"#
        ),
        Ok("3,2,b,text,b,,true,true".into())
    );
}

#[test]
fn generated_constants_are_on_interface_and_prototype() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const d = Object.getOwnPropertyDescriptor(Node, 'DOCUMENT_POSITION_CONTAINED_BY');
        return [Node.ELEMENT_NODE, document.body.TEXT_NODE, Node.prototype.NOTATION_NODE,
            d.value, d.writable, d.enumerable, d.configurable].join('|');
    })()"#
        ),
        Ok("1|3|12|16|false|true|false".into())
    );
}

#[test]
fn node_queries_are_generated_prototype_operations() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const div = document.createElement('div');
        div.appendChild(document.createElement('c'));
        const misplaced = ['cloneNode', 'hasChildNodes', 'isSameNode'].filter(n =>
            Object.prototype.hasOwnProperty.call(div, n) || typeof Node.prototype[n] !== 'function');
        const out = [misplaced.length, div.hasChildNodes(), div.isSameNode(div), div.isSameNode(null),
            div.cloneNode(true).firstChild.localName, div.cloneNode().hasChildNodes()];
        try { div.isSameNode('x'); } catch (e) { out.push(e instanceof TypeError); }
        try { Node.prototype.cloneNode.call({}); } catch (e) { out.push(e instanceof TypeError); }
        return out.join();
    })()"#
        ),
        Ok("0,true,true,false,c,false,true,true".into())
    );
}

#[test]
fn node_mutations_are_generated_prototype_operations() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const names = ['appendChild', 'insertBefore', 'removeChild', 'replaceChild'];
        const div = document.createElement('div');
        // Element wrappers alias the three redirect-wrapped methods as own
        // properties until the ShadyDOM redirect goes (v8_post.js).
        const aliased = ['appendChild', 'insertBefore', 'removeChild'];
        const misplaced = names.filter(n => typeof Node.prototype[n] !== 'function'
            || Object.prototype.hasOwnProperty.call(div, n) !== aliased.includes(n));
        const a = div.appendChild(document.createElement('a'));
        const b = div.insertBefore(document.createTextNode('b'), a);
        const c = document.createElement('c');
        const old = div.replaceChild(c, a);
        const out = [misplaced.length, div.childNodes.length, div.firstChild === b, old === a,
            div.removeChild(b) === b, div.hasChildNodes(), div.isSameNode(div), div.isSameNode(null),
            div.cloneNode(true).firstChild.localName, div.cloneNode().hasChildNodes()];
        try { div.appendChild('x'); } catch (e) { out.push(e instanceof TypeError); }
        try { c.appendChild(div); } catch (e) { out.push(e instanceof DOMException && e.name); }
        try { Node.prototype.appendChild.call({}, c); } catch (e) { out.push(e instanceof TypeError); }
        return out.join();
    })()"#
        ),
        Ok("0,2,true,true,true,true,true,false,c,false,true,HierarchyRequestError,true".into())
    );
}

#[test]
fn get_root_node_converts_its_options_dictionary() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const host = document.createElement('div');
        document.body.appendChild(host);
        const root = host.attachShadow({ mode: 'open' });
        const inner = document.createElement('span');
        root.appendChild(inner);
        const loose = document.createElement('p');
        const child = loose.appendChild(document.createElement('b'));
        let getterCalls = 0;
        const options = { get composed() { getterCalls++; return 1; } };
        const out = [
            document.body.getRootNode() === document,
            inner.getRootNode() === root,
            inner.getRootNode({}) === root,
            inner.getRootNode(null) === root,
            inner.getRootNode({ composed: true }) === document,
            inner.getRootNode(options) === document && getterCalls === 1,
            child.getRootNode() === loose,
            Object.prototype.hasOwnProperty.call(host, 'getRootNode'),
            Node.prototype.getRootNode.length,
        ];
        try { inner.getRootNode(5); } catch (e) { out.push(e instanceof TypeError); }
        return out.join();
    })()"#
        ),
        Ok("true,true,true,true,true,true,true,false,0,true".into())
    );
}

#[test]
fn node_value_equality_and_document_position() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const div = document.createElement('div');
        const text = div.appendChild(document.createTextNode('hi'));
        const comment = div.appendChild(document.createComment('c'));
        text.nodeValue = 'yo';
        comment.nodeValue = null;
        div.nodeValue = 'ignored';
        const values = [div.nodeValue, text.nodeValue, text.data, comment.data === '', document.nodeValue,
            Object.prototype.hasOwnProperty.call(text, 'nodeValue')];

        const a = document.createElement('p');
        a.setAttribute('x', '1'); a.setAttribute('y', '2');
        a.appendChild(document.createTextNode('t'));
        const b = document.createElement('p');
        b.setAttribute('y', '2'); b.setAttribute('x', '1');
        b.appendChild(document.createTextNode('t'));
        const c = a.cloneNode(true); c.firstChild.data = 'u';
        const equal = [a.isEqualNode(b), a.isEqualNode(c), a.isEqualNode(null),
            a.isEqualNode(document.createElement('span')), a.isEqualNode(a)];

        document.body.appendChild(div);
        const first = document.body.firstChild;
        const P = Node.DOCUMENT_POSITION_PRECEDING, F = Node.DOCUMENT_POSITION_FOLLOWING,
              C = Node.DOCUMENT_POSITION_CONTAINS, CB = Node.DOCUMENT_POSITION_CONTAINED_BY,
              D = Node.DOCUMENT_POSITION_DISCONNECTED, I = Node.DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC;
        const loose = document.createElement('i');
        const shadowChild = document.createElement('b');
        div.attachShadow({ mode: 'open' }).appendChild(shadowChild);
        const disconnected = text.compareDocumentPosition(loose);
        const position = [
            text.compareDocumentPosition(text) === 0,
            text.compareDocumentPosition(comment) === F,
            comment.compareDocumentPosition(text) === P,
            text.compareDocumentPosition(div) === (C | P),
            div.compareDocumentPosition(text) === (CB | F),
            document.compareDocumentPosition(text) === (CB | F),
            (disconnected & (D | I)) === (D | I) && [P, F].includes(disconnected & (P | F)),
            (loose.compareDocumentPosition(text) & (P | F)) === ((disconnected & (P | F)) ^ (P | F)),
            (shadowChild.compareDocumentPosition(div) & D) === D,
        ];
        try { text.compareDocumentPosition(null); } catch (e) { position.push(e instanceof TypeError); }
        return [values.join('|'), equal.join('|'), position.join('|'), first !== null].join(' / ');
    })()"#
        ),
        Ok("|yo|yo|true||false / true|false|false|false|true / true|true|true|true|true|true|true|true|true|true / true".into())
    );
}

#[test]
fn normalize_merges_adjacent_text_and_drops_empty_text() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const div = document.createElement('div');
        const first = div.appendChild(document.createTextNode('a'));
        div.appendChild(document.createTextNode(''));
        div.appendChild(document.createTextNode('b'));
        const b = div.appendChild(document.createElement('b'));
        b.appendChild(document.createTextNode(''));
        b.appendChild(document.createTextNode('x'));
        b.appendChild(document.createTextNode('y'));
        div.appendChild(document.createComment('c'));
        div.appendChild(document.createTextNode(''));
        const template = div.appendChild(document.createElement('template'));
        template.content.appendChild(document.createTextNode('t1'));
        template.content.appendChild(document.createTextNode('t2'));
        document.body.appendChild(div);
        const result = div.normalize();
        return [result, div.childNodes.length, div.firstChild === first, first.data,
            b.childNodes.length, b.firstChild.data, template.content.childNodes.length,
            div.textContent, Object.prototype.hasOwnProperty.call(div, 'normalize'),
            Element.prototype.hasOwnProperty('normalize')].join('|');
    })()"#
        ),
        Ok("|4|true|ab|1|xy|2|abxy|false|false".into())
    );
}

#[test]
fn element_attributes_are_generated_and_toggle_attribute_works() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const names = ['getAttribute', 'setAttribute', 'removeAttribute', 'hasAttribute',
            'toggleAttribute', 'hasAttributes', 'getAttributeNS', 'setAttributeNS',
            'removeAttributeNS', 'hasAttributeNS'];
        const el = document.createElement('div');
        const misplaced = names.filter(n => typeof Element.prototype[n] !== 'function'
            || Object.prototype.hasOwnProperty.call(el, n));
        const text = document.createTextNode('t');
        const onText = names.filter(n => n in text);

        el.setAttribute('a', 1);
        const basics = [el.getAttribute('a'), el.getAttribute('missing'), el.hasAttribute('a'),
            el.hasAttributes(), el.getAttributeNS(null, 'a'), el.hasAttributeNS('x', 'a')];
        el.removeAttribute('a');
        basics.push(el.hasAttributes());

        const toggles = [el.toggleAttribute('t'), el.getAttribute('t'), el.toggleAttribute('t'),
            el.hasAttribute('t'), el.toggleAttribute('t', false), el.hasAttribute('t'),
            el.toggleAttribute('t', true), el.toggleAttribute('t', true), el.getAttribute('t')];

        el.id = 'x';
        const ids = [el.id, el.getAttribute('id'), Object.getOwnPropertyDescriptor(Element.prototype, 'id').set !== undefined];

        const seen = [];
        class Watched extends HTMLElement {
            static get observedAttributes() { return ['flag']; }
            attributeChangedCallback(name, oldValue, newValue) { seen.push(name + ':' + oldValue + '>' + newValue); }
        }
        customElements.define('x-watched', Watched);
        const watched = document.createElement('x-watched');
        watched.toggleAttribute('flag');
        watched.setAttribute('flag', 'on');
        watched.removeAttribute('flag');

        let arity = '';
        try { el.setAttribute('only-one'); } catch (e) { arity = e instanceof TypeError; }
        return [misplaced.length, onText.join(';'), basics.join('|'), toggles.join('|'),
            ids.join('|'), seen.join(','), arity].join(' / ');
    })()"#
        ),
        Ok("0 /  / 1||true|true|1|true|false / true||false|false|false|false|true|true| / x|x|true / flag:null>,flag:>on,flag:on>null / true".into())
    );
}

#[test]
fn anchors_expose_url_members_resolved_against_the_document() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        __aurora_set_location__('https://www.youtube.com/watch?v=abc');
        const a = document.createElement('a');
        const none = [a.href, a.protocol, a.pathname, a.host, a.origin];
        a.setAttribute('href', '/results?search_query=x#top');
        const parts = [a.href, a.protocol, a.host, a.hostname, a.port, a.pathname, a.search,
            a.hash, a.origin, String(a), a.toString() === a.href];
        a.pathname = 'feed/trending';
        a.search = 'gl=US';
        a.hash = '';
        a.port = '8443';
        const written = [a.getAttribute('href'), a.host];
        a.href = 'mailto:someone@example.com';
        const opaque = [a.protocol, a.pathname, a.host];
        a.href = 'http://[bad';
        const broken = [a.href, a.pathname, a.protocol];
        const area = document.createElement('area');
        area.href = 'https://example.com:81/p';
        return [none.join('|'), parts.join('|'), written.join('|'), opaque.join('|'), broken.join('|'),
            area.port, Object.prototype.hasOwnProperty.call(a, 'href'),
            'pathname' in document.createElement('div')].join(' / ');
    })()"#
        ),
        Ok("|:||| / https://www.youtube.com/results?search_query=x#top|https:|www.youtube.com|www.youtube.com||/results|?search_query=x|#top|https://www.youtube.com|https://www.youtube.com/results?search_query=x#top|true / https://www.youtube.com:8443/feed/trending?gl=US|www.youtube.com:8443 / mailto:|someone@example.com| / http://[bad||: / 81 / false / false".into())
    );
}

#[test]
fn element_identity_is_on_the_prototype() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const div = document.createElement('div');
        const custom = document.createElement('x-thing');
        const host = document.createElement('div');
        host.innerHTML = '<svg><clipPath></clipPath></svg>';
        const svg = host.firstChild, clip = svg.firstChild;
        const own = ['tagName', 'localName', 'namespaceURI', 'prefix']
            .filter(n => Object.prototype.hasOwnProperty.call(div, n));
        const html = 'http://www.w3.org/1999/xhtml', svgNs = 'http://www.w3.org/2000/svg';
        return [own.length,
            div.tagName, div.localName, div.namespaceURI === html, div.prefix,
            custom.tagName, custom.localName,
            svg.tagName, svg.localName, svg.namespaceURI === svgNs, svg.nodeName,
            clip.localName, clip.tagName === clip.nodeName,
            document.body.tagName, 'tagName' in document.createTextNode('t')].join('|');
    })()"#
        ),
        Ok("0|DIV|div|true||X-THING|x-thing|svg|svg|true|svg|clipPath|true|BODY|false".into())
    );
}

#[test]
fn script_elements_reflect_their_attributes() {
    let mut runtime = runtime();
    runtime.eval_to_string("location.href = 'https://example.com/dir/page'").unwrap();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const s = document.createElement('script');
        const own = ['src', 'type', 'async', 'defer', 'crossOrigin']
            .filter(n => Object.prototype.hasOwnProperty.call(s, n));
        const out = [own.join(","), JSON.stringify(s.src), String(s.crossOrigin)];
        s.src = 'a.js';
        s.type = 'module';
        s.defer = true;
        out.push(s.getAttribute('src'), s.src, s.getAttribute('type'), s.hasAttribute('defer'));
        s.defer = false;
        s.setAttribute('crossorigin', 'bogus');
        out.push(s.hasAttribute('defer'), s.crossOrigin);
        s.crossOrigin = 'USE-CREDENTIALS';
        out.push(s.crossOrigin);
        s.crossOrigin = null;
        out.push(s.hasAttribute('crossorigin'));
        return out.join('|');
    })()"#
        ),
        Ok("|\"\"|null|a.js|https://example.com/dir/a.js|module|true|false|anonymous|use-credentials|false".to_string())
    );
}

#[test]
fn selector_queries_are_generated_and_spec_shaped() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const box = document.createElement('div');
        box.className = 'box';
        box.innerHTML = '<p class="a b">1</p><div><p class="b\u000Ba">2</p><span class="a">3</span></div>';
        document.body.appendChild(box);
        const own = ['querySelector', 'querySelectorAll', 'matches', 'closest',
            'getElementsByTagName', 'getElementsByClassName']
            .filter(n => Object.prototype.hasOwnProperty.call(box, n)
                || Object.prototype.hasOwnProperty.call(document, n));
        const span = box.querySelector('span');
        const out = [own.length,
            box.querySelector('p').textContent, box.querySelectorAll('p').length,
            document.querySelector('.box') === box, document.querySelectorAll('.box p').length,
            span.matches('.a'), span.webkitMatchesSelector('div > span'), span.closest('.box') === box,
            span.closest('section'), box.getElementsByTagName('div').length,
            box.getElementsByTagName('*').length, box.getElementsByClassName('a b').length,
            box.getElementsByClassName(' ').length, document.getElementsByTagName('P').length >= 2,
            box.getElementsByClassName('b\u000Ba').length];
        for (const call of [() => box.querySelector('##'), () => span.matches(''),
                            () => span.closest('['), () => document.querySelectorAll('a,,b')]) {
            try { call(); out.push('no throw'); } catch (e) { out.push(e.name); }
        }
        const host = document.createElement('div');
        document.body.appendChild(host);
        const root = host.attachShadow({ mode: 'open' });
        root.innerHTML = '<i class="inside"></i>';
        out.push(root.querySelector('.inside') !== null, document.querySelector('.inside'));
        return out.join('|');
    })()"#
        ),
        Ok("0|1|2|true|2|true|true|true||1|4|1|0|true|1|SyntaxError|SyntaxError|SyntaxError|SyntaxError|true|".into())
    );
}

#[test]
fn document_collections_list_their_html_elements_in_tree_order() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const box = document.createElement('div');
        box.innerHTML = '<img id="i1"><form id="f1"><img id="i2"></form>'
            + '<a id="a1" href="/x"></a><a id="a2"></a><map><area id="r1" href="/y"></map>'
            + '<embed id="e1"><script id="s1"></script>';
        document.body.appendChild(box);
        const ids = c => Array.from(c, el => el.id).filter(Boolean).join(',');
        const before = document.images.length;
        box.appendChild(document.createElement('img'));
        return [ids(document.images), ids(document.forms), ids(document.links),
            ids(document.embeds), ids(document.plugins), ids(document.scripts),
            document.images.length === before + 1,
            Object.getOwnPropertyDescriptor(Document.prototype, 'scripts').get !== undefined,
        ].join('|');
    })()"#
        ),
        Ok("i1,i2|f1|a1,r1|e1|e1|s1|true|true".to_string())
    );
}

#[test]
fn child_node_methods_follow_the_spec() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const names = (p) => Array.from(p.childNodes, n => n.nodeType === 1 ? n.localName : n.data).join(',');
        const p = document.createElement('div');
        p.innerHTML = '<a></a><b></b><c></c>';
        const [a, b, c] = p.children;
        const own = ['before', 'after', 'replaceWith', 'remove']
            .filter(n => Object.prototype.hasOwnProperty.call(a, n) || typeof Element.prototype[n] !== 'function');
        const out = [own.length, Element.prototype.before.length];

        b.before('x', c);
        out.push(names(p));
        b.after(a, 'y');
        out.push(names(p));
        a.replaceWith('z');
        out.push(names(p));
        b.replaceWith();
        out.push(names(p));
        c.remove();
        out.push(names(p));

        const loose = document.createElement('i');
        loose.before('ignored'); loose.after(b); loose.remove(); loose.replaceWith(c);
        out.push(b.parentNode === null, c.parentNode === null);

        const text = p.firstChild;
        text.after(document.createComment('k'));
        out.push(typeof text.before, names(p));
        try { b.appendChild(p); p.firstChild.before(p); } catch (e) { out.push(e.name); }
        return out.join(' / ');
    })()"#
        ),
        Ok("0 / 0 / a,x,c,b / x,c,b,a,y / x,c,b,z,y / x,c,z,y / x,z,y / true / true / function / x,k,z,y / HierarchyRequestError".into())
    );
}

#[test]
fn parent_node_methods_follow_the_spec() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const names = (p) => Array.from(p.childNodes, n => n.nodeType === 1 ? n.localName : n.data).join(',');
        const p = document.createElement('div');
        const own = ['append', 'prepend', 'replaceChildren']
            .filter(n => Object.prototype.hasOwnProperty.call(p, n) || typeof Element.prototype[n] !== 'function');
        const out = [own.length, Element.prototype.append.length];
        p.append('a', document.createElement('b'));
        p.prepend('x', 'y');
        p.append();
        out.push(names(p));

        const elsewhere = document.createElement('section');
        const moved = elsewhere.appendChild(document.createElement('m'));
        p.replaceChildren(moved, 'z');
        out.push(names(p), elsewhere.childNodes.length, moved.parentNode === p);

        try { p.replaceChildren(p); } catch (e) { out.push(e.name); }
        out.push(names(p));
        p.replaceChildren();
        out.push(p.childNodes.length);

        const fragment = document.createDocumentFragment();
        fragment.append('f', document.createElement('g'));
        p.append(fragment);
        out.push(names(p), fragment.childNodes.length, typeof document.append, typeof fragment.prepend);
        return out.join(' / ');
    })()"#
        ),
        Ok("0 / 0 / x,y,a,b / m,z / 0 / true / HierarchyRequestError / m,z / 0 / f,g / 0 / function / function".into())
    );
}

#[test]
fn snake_case_member_names() {
    #[path = "../../codegen/generate.rs"]
    #[allow(dead_code)]
    mod generate;
    use generate::snake_case;
    assert_eq!(snake_case("compareDocumentPosition"), "compare_document_position");
    assert_eq!(snake_case("innerHTML"), "inner_html");
    assert_eq!(snake_case("getElementsByTagNameNS"), "get_elements_by_tag_name_ns");
    assert_eq!(snake_case("HTMLElement"), "html_element");
    assert_eq!(snake_case("type"), "r#type");
}

#[test]
fn document_all_collection_exposes_same_object_and_live_lookups() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.eval_to_string(
            r#"(() => {
        const all = document.all;
        const sameObject = document.all === all;
        const distinctFromPrimitives =
            undefined !== all && null !== all && false !== all && '' !== all;
        const toStringTag = Object.prototype.toString.call(all);
        const isInstance = all instanceof HTMLAllCollection;

        let ctorErr = '';
        try { new HTMLAllCollection(); } catch (e) { ctorErr = e.name; }
        let brandErr = '';
        try {
            Object.getOwnPropertyDescriptor(Document.prototype, 'all').get.call({});
        } catch (e) { brandErr = e.name; }
        let lengthBrandErr = '';
        try {
            Object.getOwnPropertyDescriptor(HTMLAllCollection.prototype, 'length').get.call({});
        } catch (e) { lengthBrandErr = e.name; }

        const before = all.length;
        const box = document.createElement('div');
        box.innerHTML = '<div id="single"></div><img name="hero">'
            + '<div name="ignored"></div><span id="dup"></span><a id="dup"></a>';
        document.body.appendChild(box);
        const after = all.length;

        const spreadMatches = [...all].length === after;
        const indexedFirst = all[0] === document.documentElement;
        const itemByIndex = all.item(after - 1).id;
        const itemOob = all.item(after + 5) === null;
        const namedSingle = all.single.id + ',' + all.namedItem('hero').localName;
        const namedIgnored = all.ignored === undefined && all.namedItem('ignored') === null;
        const namedMultiple = all.dup.length + ',' + all.item('dup').map(e => e.localName).join(':');
        const emptyArg = all.item() === null && all.namedItem('') === null;

        return [
            sameObject, distinctFromPrimitives, toStringTag, isInstance,
            ctorErr, brandErr, lengthBrandErr,
            after - before === 6, spreadMatches, indexedFirst,
            itemByIndex, itemOob, namedSingle, namedIgnored, namedMultiple, emptyArg,
        ].join('|');
    })()"#
        ),
        Ok("true|true|[object HTMLAllCollection]|true|TypeError|TypeError|TypeError|true|true|true|dup|true|single,img|true|2,span:a|true".into())
    );
}

