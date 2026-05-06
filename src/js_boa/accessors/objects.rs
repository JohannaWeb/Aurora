use super::*;

pub(in crate::js_boa) fn install_object_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    // style: per-node backing map exposed as a plain object with methods.
    let style_obj = build_style_object(cap.clone(), context);
    let _ = obj.set(js_string!("style"), style_obj, false, context);

    // classList
    let cl_obj = build_classlist_object(cap.clone(), context);
    let _ = obj.set(js_string!("classList"), cl_obj, false, context);

    // dataset — flat object mirroring data-* attributes.
    let dataset = {
        let b = cap.node.borrow();
        let mut init = ObjectInitializer::new(context);
        if let Node::Element(el) = &*b {
            for (k, v) in &el.attributes {
                if let Some(rest) = k.strip_prefix("data-") {
                    let camel = kebab_to_camel(rest);
                    init.property(
                        JsString::from(camel),
                        JsString::from(v.clone()),
                        Attribute::all(),
                    );
                }
            }
        }
        init.build()
    };
    let _ = obj.set(js_string!("dataset"), dataset, false, context);
}
