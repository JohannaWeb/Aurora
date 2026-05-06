use super::*;

pub(super) fn install_dom_constructors(context: &mut Context) {
    let constructors = r#"
        (function(global) {
            function install(name, parentName) {
                if (typeof global[name] !== "function") {
                    global[name] = function() {};
                }
                if (!global[name].prototype) {
                    global[name].prototype = {};
                }
                if (parentName && global[parentName] && global[parentName].prototype) {
                    Object.setPrototypeOf(global[name].prototype, global[parentName].prototype);
                }
                global[name].prototype.constructor = global[name];
            }

            install("EventTarget");
            install("Node", "EventTarget");
            install("Document", "Node");
            install("DocumentFragment", "Node");
            install("CharacterData", "Node");
            install("Text", "CharacterData");
            install("Comment", "CharacterData");
            install("Element", "Node");
            install("HTMLElement", "Element");
            install("HTMLAnchorElement", "HTMLElement");
            install("HTMLBodyElement", "HTMLElement");
            install("HTMLDivElement", "HTMLElement");
            install("HTMLFormElement", "HTMLElement");
            install("HTMLHeadElement", "HTMLElement");
            install("HTMLHtmlElement", "HTMLElement");
            install("HTMLImageElement", "HTMLElement");
            install("HTMLInputElement", "HTMLElement");
            install("HTMLLinkElement", "HTMLElement");
            install("HTMLMetaElement", "HTMLElement");
            install("HTMLOptionElement", "HTMLElement");
            install("HTMLScriptElement", "HTMLElement");
            install("HTMLSelectElement", "HTMLElement");
            install("HTMLStyleElement", "HTMLElement");
            install("HTMLTableElement", "HTMLElement");
            install("HTMLTextAreaElement", "HTMLElement");
        })(globalThis);
    "#;
    let _ = context.eval(Source::from_bytes(constructors.as_bytes()));
}

pub(super) fn set_object_prototype_from_constructor(
    obj: &JsObject,
    constructor_name: &str,
    context: &mut Context,
) {
    let global = context.global_object().clone();
    let Ok(constructor) = global.get(JsString::from(constructor_name), context) else {
        return;
    };
    let Some(constructor) = constructor.as_object() else {
        return;
    };
    let Ok(prototype) = constructor.get(js_string!("prototype"), context) else {
        return;
    };
    let Some(prototype) = prototype.as_object() else {
        return;
    };
    let _ = obj.set_prototype(Some(prototype.clone()));
}

pub(super) fn constructor_for_node(node: &Node) -> &'static str {
    match node {
        Node::Document { .. } => "Document",
        Node::Text(_) => "Text",
        Node::Element(el) => match el.tag_name.as_str() {
            "#document-fragment" => "DocumentFragment",
            "a" => "HTMLAnchorElement",
            "body" => "HTMLBodyElement",
            "div" => "HTMLDivElement",
            "form" => "HTMLFormElement",
            "head" => "HTMLHeadElement",
            "html" => "HTMLHtmlElement",
            "img" => "HTMLImageElement",
            "input" => "HTMLInputElement",
            "link" => "HTMLLinkElement",
            "meta" => "HTMLMetaElement",
            "option" => "HTMLOptionElement",
            "script" => "HTMLScriptElement",
            "select" => "HTMLSelectElement",
            "style" => "HTMLStyleElement",
            "table" => "HTMLTableElement",
            "textarea" => "HTMLTextAreaElement",
            _ => "HTMLElement",
        },
    }
}

// Mini Pipe to keep the chain readable above.
pub(super) trait Pipe: Sized {
    fn pipe<F, T>(self, f: F) -> T
    where
        F: FnOnce(Self) -> T,
    {
        f(self)
    }
}
impl<T> Pipe for T {}
