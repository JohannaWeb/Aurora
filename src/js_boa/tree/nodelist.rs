use super::*;

pub(in crate::js_boa) fn build_nodelist(
    nodes: Vec<NodePtr>,
    registry: &NodeRegistry,
    document: &NodePtr,
    context: &mut Context,
) -> JsResult<JsValue> {
    let values: Vec<JsValue> = nodes
        .into_iter()
        .map(|n| create_js_node(n, registry, document, context))
        .collect();
    Ok(JsArray::from_iter(values, context).into())
}
