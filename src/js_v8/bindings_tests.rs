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

