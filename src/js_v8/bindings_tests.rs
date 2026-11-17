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
