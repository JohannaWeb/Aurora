use super::*;

pub(in crate::js_boa) fn build_document_implementation(
    document: &NodePtr,
    registry: &NodeRegistry,
    context: &mut Context,
) -> JsObject {
    let doc_cap = DocCapture {
        document: document.clone(),
        registry: registry.clone(),
    };

    ObjectInitializer::new(context)
        .function(
            NativeFunction::from_copy_closure_with_captures(
                |_this, args, cap: &DocCapture, ctx| {
                    let title = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
                    let title_node = Node::element("title", vec![Node::text(title)]);
                    let head = Node::element("head", vec![title_node]);
                    let body = Node::element("body", vec![]);
                    let html = Node::element("html", vec![head, body]);
                    let doc = Node::document(vec![html]);
                    let doc_obj = create_js_node(doc, &cap.registry, &cap.document, ctx);
                    Ok(doc_obj)
                },
                doc_cap,
            ),
            js_string!("createHTMLDocument"),
            1,
        )
        .function(return_bool(true), js_string!("hasFeature"), 2)
        .build()
}
