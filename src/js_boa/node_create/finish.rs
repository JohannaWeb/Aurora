use super::*;

pub(in crate::js_boa) fn finish_node_object(
    obj: &JsObject,
    cap: &NodeCapture,
    node_type: i32,
    context: &mut Context,
) {
    // attributes: snapshot as an object with name/value entries. Not fully-live.
    {
        let b = cap.node.borrow();
        if let Node::Element(el) = &*b {
            let mut attrs_init = ObjectInitializer::new(context);
            attrs_init.property(
                js_string!("length"),
                el.attributes.len() as u32,
                Attribute::all(),
            );
            for (k, v) in &el.attributes {
                attrs_init.property(
                    JsString::from(k.clone()),
                    JsString::from(v.clone()),
                    Attribute::all(),
                );
            }
            let attrs = attrs_init.build();
            let _ = obj.set(js_string!("attributes"), attrs, false, context);
        }
    }

    // For text nodes: data / nodeValue / length
    if node_type == 3 {
        let text_val = {
            let b = cap.node.borrow();
            if let Node::Text(t) = &*b {
                t.clone()
            } else {
                String::new()
            }
        };
        let _ = obj.set(
            js_string!("data"),
            JsString::from(text_val.clone()),
            false,
            context,
        );
        let _ = obj.set(
            js_string!("nodeValue"),
            JsString::from(text_val.clone()),
            false,
            context,
        );
        let _ = obj.set(
            js_string!("length"),
            text_val.chars().count() as u32,
            false,
            context,
        );
    }
}
