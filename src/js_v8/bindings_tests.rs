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

