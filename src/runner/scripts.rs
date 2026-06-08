pub(crate) fn extract_scripts(node: &crate::dom::NodePtr) -> Vec<(String, bool)> {
    let mut scripts = Vec::new();
    walk(node, &mut scripts);
    scripts
}

fn walk(node: &crate::dom::NodePtr, scripts: &mut Vec<(String, bool)>) {
    let node_borrow = node.borrow();

    match &*node_borrow {
        crate::dom::Node::Element(el) if el.tag_name == "script" => {
            collect_script(el, scripts);
        }
        crate::dom::Node::Element(el) => {
            for child in &el.children {
                walk(child, scripts);
            }
        }
        crate::dom::Node::Document { children, .. } => {
            for child in children {
                walk(child, scripts);
            }
        }
        crate::dom::Node::Text(_) => {}
    }
}

fn collect_script(el: &crate::dom::ElementNode, scripts: &mut Vec<(String, bool)>) {
    // `<script type="application/json">`, `type="module"`, `type="importmap"`,
    // etc. are not classic scripts. Feeding their contents to the classic-script
    // evaluator either throws a syntax error (JSON object literals, `import`/
    // `export`) or, worse, runs untrusted data as code. Skip anything that
    // isn't a classic JavaScript script.
    if !is_classic_script_type(el.attributes.get("type").map(String::as_str)) {
        return;
    }

    if let Some(src) = el.attributes.get("src") {
        scripts.push((src.clone(), true));
        return;
    }

    let mut content = String::new();
    for child in &el.children {
        let child_borrow = child.borrow();
        if let crate::dom::Node::Text(t) = &*child_borrow {
            content.push_str(t);
        }
    }

    if !content.is_empty() {
        scripts.push((content, false));
    }
}

/// Mirrors the HTML spec's "JavaScript MIME type essence match": a missing or
/// empty `type` attribute means classic JS, a small set of legacy MIME types
/// also count, and everything else (including `module`, `importmap`, and data
/// blobs like `application/json`/`application/ld+json`) is something else.
///
/// `module` is excluded deliberately — Aurora's runtime has no ES module
/// loader, and running module source through the classic-script evaluator just
/// fails on `import`/`export` syntax.
fn is_classic_script_type(type_attr: Option<&str>) -> bool {
    let Some(raw) = type_attr else { return true };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return true;
    }
    matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "text/javascript"
            | "application/javascript"
            | "text/ecmascript"
            | "application/ecmascript"
            | "text/x-ecmascript"
            | "application/x-ecmascript"
            | "text/x-javascript"
            | "application/x-javascript"
            | "text/javascript1.0"
            | "text/javascript1.1"
            | "text/javascript1.2"
            | "text/javascript1.3"
            | "text/javascript1.4"
            | "text/javascript1.5"
            | "text/jscript"
            | "text/livescript"
    )
}
