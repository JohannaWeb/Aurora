use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::rc::Rc;

use html5ever::interface::QualName;
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ParseOpts, parse_document};
use markup5ever::ExpandedName;

use crate::dom::{DocumentMode, Node, NodePtr};

pub struct Parser<'a> {
    source: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub fn parse_document(&mut self) -> NodePtr {
        let sink = AuroraTreeSink::new();
        let mut bytes = Cursor::new(self.source.as_bytes());
        parse_document(sink, ParseOpts::default())
            .from_utf8()
            .read_from(&mut bytes)
            .expect("html5ever should parse from an in-memory buffer")
    }
}

struct AuroraTreeSink {
    document: HtmlHandle,
    parents: RefCell<BTreeMap<usize, NodePtr>>,
    template_contents: RefCell<BTreeMap<usize, NodePtr>>,
    mode: Rc<RefCell<DocumentMode>>,
}

#[derive(Clone)]
struct HtmlHandle {
    node: NodePtr,
    name: Option<QualName>,
}

impl AuroraTreeSink {
    fn new() -> Self {
        let mode = Rc::new(RefCell::new(DocumentMode::NoQuirks));
        Self {
            document: HtmlHandle {
                node: Node::document_with_mode(Vec::new(), DocumentMode::NoQuirks),
                name: Some(QualName::new(
                    None,
                    html5ever::namespace_url!(""),
                    html5ever::local_name!(""),
                )),
            },
            parents: RefCell::new(BTreeMap::new()),
            template_contents: RefCell::new(BTreeMap::new()),
            mode,
        }
    }

    fn append_child(&self, parent: &NodePtr, child: NodePtr) {
        let key = node_key(&child);
        match &mut *parent.borrow_mut() {
            Node::Document { children, .. } => append_or_merge_text(children, child.clone()),
            Node::Element(element) => append_or_merge_text(&mut element.children, child.clone()),
            Node::Text(_) => return,
        }
        self.parents.borrow_mut().insert(key, parent.clone());
    }

    fn remove_child_from_parent(&self, target: &NodePtr) {
        let Some(parent) = self.parents.borrow_mut().remove(&node_key(target)) else {
            return;
        };
        let mut parent_borrow = parent.borrow_mut();
        let children = match &mut *parent_borrow {
            Node::Document { children, .. } => children,
            Node::Element(element) => &mut element.children,
            Node::Text(_) => return,
        };
        children.retain(|child| !Rc::ptr_eq(child, target));
    }

    fn append_before(&self, sibling: &NodePtr, child: NodePtr) {
        let Some(parent) = self.parents.borrow().get(&node_key(sibling)).cloned() else {
            return;
        };
        let mut parent_borrow = parent.borrow_mut();
        let children = match &mut *parent_borrow {
            Node::Document { children, .. } => children,
            Node::Element(element) => &mut element.children,
            Node::Text(_) => return,
        };
        let index = children
            .iter()
            .position(|node| Rc::ptr_eq(node, sibling))
            .unwrap_or(children.len());
        children.insert(index, child.clone());
        self.parents
            .borrow_mut()
            .insert(node_key(&child), parent.clone());
    }

    fn append_node_or_text(&self, parent: &NodePtr, child: NodeOrText<HtmlHandle>) {
        match child {
            NodeOrText::AppendNode(node) => self.append_child(parent, node.node),
            NodeOrText::AppendText(text) => self.append_child(parent, Node::text(text.to_string())),
        }
    }
}

impl TreeSink for AuroraTreeSink {
    type Handle = HtmlHandle;
    type Output = NodePtr;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self::Output {
        if let Node::Document { mode, .. } = &mut *self.document.node.borrow_mut() {
            *mode = *self.mode.borrow();
        }
        prune_whitespace_text(&self.document.node, false);
        self.document.node
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        self.document.clone()
    }

    fn elem_name<'b>(&'b self, target: &'b Self::Handle) -> ExpandedName<'b> {
        target
            .name
            .as_ref()
            .expect("html5ever handle must carry an expanded name")
            .expanded()
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let attributes = attrs
            .into_iter()
            .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
            .collect::<BTreeMap<_, _>>();
        let node = Node::element_with_attributes(name.local.to_string(), attributes, Vec::new());

        if name.local.as_ref() == "template" {
            let content = Node::document_fragment(Vec::new());
            self.template_contents
                .borrow_mut()
                .insert(node_key(&node), content.clone());
            if let Node::Element(element) = &mut *node.borrow_mut() {
                element.template_contents = Some(content);
            }
        }

        HtmlHandle {
            node,
            name: Some(name),
        }
    }

    fn create_comment(&self, _text: StrTendril) -> Self::Handle {
        HtmlHandle {
            node: Node::text(""),
            name: None,
        }
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Self::Handle {
        HtmlHandle {
            node: Node::text(""),
            name: None,
        }
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.append_node_or_text(&parent.node, child);
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => self.append_before(&sibling.node, node.node),
            NodeOrText::AppendText(text) => {
                self.append_before(&sibling.node, Node::text(text.to_string()))
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        if self.parents.borrow().contains_key(&node_key(&element.node)) {
            match child {
                NodeOrText::AppendNode(node) => self.append_before(&element.node, node.node),
                NodeOrText::AppendText(text) => {
                    self.append_before(&element.node, Node::text(text.to_string()));
                }
            }
        } else {
            self.append_node_or_text(&prev_element.node, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        let node = self
            .template_contents
            .borrow()
            .get(&node_key(&target.node))
            .cloned()
            .unwrap_or_else(|| target.node.clone());
        HtmlHandle { node, name: None }
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        Rc::ptr_eq(&x.node, &y.node)
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        *self.mode.borrow_mut() = match mode {
            QuirksMode::NoQuirks => DocumentMode::NoQuirks,
            QuirksMode::Quirks => DocumentMode::Quirks,
            QuirksMode::LimitedQuirks => DocumentMode::LimitedQuirks,
        };
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        let Node::Element(element) = &mut *target.node.borrow_mut() else {
            return;
        };
        for attr in attrs {
            element
                .attributes
                .entry(attr.name.local.to_string())
                .or_insert_with(|| attr.value.to_string());
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.remove_child_from_parent(&target.node);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        let children = match &mut *node.node.borrow_mut() {
            Node::Document { children, .. } => std::mem::take(children),
            Node::Element(element) => std::mem::take(&mut element.children),
            Node::Text(_) => Vec::new(),
        };
        for child in children {
            self.append_child(&new_parent.node, child);
        }
    }

    fn mark_script_already_started(&self, _node: &Self::Handle) {}
}

fn node_key(node: &NodePtr) -> usize {
    Rc::as_ptr(node) as usize
}

fn append_or_merge_text(children: &mut Vec<NodePtr>, child: NodePtr) {
    let text = match &*child.borrow() {
        Node::Text(text) => Some(text.clone()),
        _ => None,
    };

    if let (Some(text), Some(last)) = (text, children.last()) {
        if let Node::Text(last_text) = &mut *last.borrow_mut() {
            last_text.push_str(&text);
            return;
        }
    }

    children.push(child);
}

fn prune_whitespace_text(node: &NodePtr, preserve_text_whitespace: bool) {
    let preserve_children = match &*node.borrow() {
        Node::Element(element) => matches!(
            element.tag_name.as_str(),
            "script" | "style" | "textarea" | "title"
        ),
        _ => preserve_text_whitespace,
    };

    let mut node_borrow = node.borrow_mut();
    let children = match &mut *node_borrow {
        Node::Document { children, .. } => children,
        Node::Element(element) => &mut element.children,
        Node::Text(_) => return,
    };

    children.retain(|child| {
        preserve_children || !matches!(&*child.borrow(), Node::Text(text) if text.trim().is_empty())
    });

    for child in children {
        prune_whitespace_text(child, preserve_children);
    }
}
