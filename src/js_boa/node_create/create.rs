use super::*;

pub(in crate::js_boa) fn create_js_node(
    node: NodePtr,
    registry: &NodeRegistry,
    document: &NodePtr,
    context: &mut Context,
) -> JsValue {
    let id = registry.register(node.clone());

    let cap = NodeCapture {
        node: node.clone(),
        registry: registry.clone(),
        document: document.clone(),
    };

    // Compute static tag/node info now.
    let (tag_name, node_type, node_name): (String, i32, String) = {
        let b = node.borrow();
        match &*b {
            Node::Element(el) => (el.tag_name.clone(), 1, el.tag_name.to_uppercase()),
            Node::Text(_) => (String::new(), 3, "#text".to_string()),
            Node::Document { .. } => (String::new(), 9, "#document".to_string()),
        }
    };

    let mut init = ObjectInitializer::new(context);
    init.property(
        js_string!("__node_id"),
        id,
        Attribute::READONLY | Attribute::NON_ENUMERABLE,
    );
    init.property(
        js_string!("tagName"),
        JsString::from(tag_name.to_uppercase()),
        Attribute::all(),
    );
    init.property(
        js_string!("localName"),
        JsString::from(tag_name.to_lowercase()),
        Attribute::all(),
    );
    init.property(
        js_string!("nodeName"),
        JsString::from(node_name.clone()),
        Attribute::all(),
    );
    init.property(js_string!("nodeType"), node_type, Attribute::all());
    init.property(
        js_string!("namespaceURI"),
        JsValue::null(),
        Attribute::all(),
    );
    init.property(js_string!("prefix"), JsValue::null(), Attribute::all());
    init.property(
        js_string!("baseURI"),
        js_string!("http://localhost/"),
        Attribute::all(),
    );
    init.property(
        js_string!("ownerDocument"),
        JsValue::null(),
        Attribute::all(),
    );
    init.property(js_string!("isConnected"), true, Attribute::all());
    init.property(js_string!("scrollTop"), 0, Attribute::all());
    init.property(js_string!("scrollLeft"), 0, Attribute::all());
    init.property(js_string!("scrollWidth"), 0, Attribute::all());
    init.property(js_string!("scrollHeight"), 0, Attribute::all());
    init.property(js_string!("clientWidth"), 0, Attribute::all());
    init.property(js_string!("clientHeight"), 0, Attribute::all());
    init.property(js_string!("clientTop"), 0, Attribute::all());
    init.property(js_string!("clientLeft"), 0, Attribute::all());
    init.property(js_string!("offsetTop"), 0, Attribute::all());
    init.property(js_string!("offsetLeft"), 0, Attribute::all());
    init.property(js_string!("offsetWidth"), 0, Attribute::all());
    init.property(js_string!("offsetHeight"), 0, Attribute::all());

    install_mutation_methods(&mut init, &cap);
    install_attribute_methods(&mut init, &cap);
    install_query_methods(&mut init, &cap);
    install_command_methods(&mut init, &cap);

    let obj = init.build();
    let constructor_name = {
        let b = cap.node.borrow();
        constructor_for_node(&b)
    };
    set_object_prototype_from_constructor(&obj, constructor_name, context);
    install_accessors(&obj, &cap, context);
    finish_node_object(&obj, &cap, node_type, context);
    install_element_reflection_properties(&obj, &cap, context);

    obj.into()
}
