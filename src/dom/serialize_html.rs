use crate::dom::{Node, NodePtr};

pub(crate) fn serialize_outer_html(node: &NodePtr) -> String {
    let mut buf = String::with_capacity(4096);
    serialize_node(node, &mut buf, false);
    buf
}

fn serialize_node(node: &NodePtr, out: &mut String, is_rawtext: bool) {
    match &*node.borrow() {
        Node::Document { children, .. } => {
            out.push_str("<!DOCTYPE html>\n");
            for child in children {
                serialize_node(child, out, false);
            }
        }
        Node::Element(el) => {
            out.push('<');
            out.push_str(&el.tag_name);
            for (k, v) in &el.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(&html_escape(v));
                out.push('"');
            }
            out.push('>');

            // Rawtext tags like <style> and <script> should not have their child text nodes HTML-escaped.
            let el_is_raw = matches!(
                el.tag_name.to_ascii_lowercase().as_str(),
                "script" | "style"
            );

            // <template> elements store their inert content in template_contents,
            // not children. Serialize it so Polymer component styles survive the
            // round-trip through Blitz's HTML parser.
            if el.tag_name.eq_ignore_ascii_case("template") {
                if let Some(ref content) = el.template_contents {
                    serialize_node(content, out, false);
                }
            }

            for child in &el.children {
                serialize_node(child, out, el_is_raw);
            }
            if !is_void(&el.tag_name) {
                out.push_str("</");
                out.push_str(&el.tag_name);
                out.push('>');
            }
        }
        Node::Text(t) => {
            if is_rawtext {
                out.push_str(&t.content);
            } else {
                out.push_str(&html_escape(&t.content));
            }
        }
    }
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::Node;
    use std::collections::BTreeMap;

    #[test]
    fn test_serialize_template_contents() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let style_text = Node::text("div { color: red; }");
        let style_el = Node::element_with_attributes("style", BTreeMap::new(), vec![style_text]);
        let content_fragment = Node::document(vec![style_el]);
        let tpl = Rc::new(RefCell::new(Node::Element(crate::dom::ElementNode {
            tag_name: "template".into(),
            attributes: BTreeMap::new(),
            children: vec![],
            template_contents: Some(content_fragment),
            shadow_root: None,
            assigned_nodes: vec![],
            parent: Default::default(),
        })));
        let doc = Node::document(vec![tpl]);
        let serialized = serialize_outer_html(&doc);
        assert!(
            serialized.contains("<style>div { color: red; }</style>"),
            "{serialized}"
        );
        assert!(serialized.contains("<template>"), "{serialized}");
        assert!(serialized.contains("</template>"), "{serialized}");
    }

    #[test]
    fn test_serialize_doctype_and_rawtext() {
        let text_in_style = Node::text("body > div { color: red; & }");
        let style_el = Node::element_with_attributes("style", BTreeMap::new(), vec![text_in_style]);
        let normal_text = Node::text("Hello <world> &");
        let div_el = Node::element_with_attributes("div", BTreeMap::new(), vec![normal_text]);
        let doc = Node::document(vec![style_el, div_el]);

        let serialized = serialize_outer_html(&doc);
        assert_eq!(
            serialized,
            "<!DOCTYPE html>\n<style>body > div { color: red; & }</style><div>Hello &lt;world&gt; &amp;</div>"
        );
    }
}
