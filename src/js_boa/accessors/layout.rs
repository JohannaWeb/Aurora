use super::*;

pub(in crate::js_boa) fn install_layout_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    // offsetWidth
    let ow_get = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, _args: &[JsValue], cap: &NodeCapture, _ctx: &mut Context| {
            cap.registry.perform_sync_reflow();
            let layout_tree_opt = cap.registry.layout_tree.borrow();
            if let Some(layout_tree_rc) = layout_tree_opt.as_ref() {
                let layout_tree = layout_tree_rc.borrow();
                if let Some(layout_box) = layout_tree.find_box_for_node(&cap.node) {
                    return Ok(JsValue::from(layout_box.rect().width));
                }
            }
            Ok(JsValue::from(0.0))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "offsetWidth", Some(ow_get), None);

    // offsetHeight
    let oh_get = NativeFunction::from_copy_closure_with_captures(
        |_this: &JsValue, _args: &[JsValue], cap: &NodeCapture, _ctx: &mut Context| {
            cap.registry.perform_sync_reflow();
            let layout_tree_opt = cap.registry.layout_tree.borrow();
            if let Some(layout_tree_rc) = layout_tree_opt.as_ref() {
                let layout_tree = layout_tree_rc.borrow();
                if let Some(layout_box) = layout_tree.find_box_for_node(&cap.node) {
                    return Ok(JsValue::from(layout_box.rect().height));
                }
            }
            Ok(JsValue::from(0.0))
        },
        cap.clone(),
    );
    install_accessor(obj, context, "offsetHeight", Some(oh_get), None);

    // getBoundingClientRect
    init_get_bounding_client_rect(obj, cap, context);
}

fn init_get_bounding_client_rect(obj: &JsObject, cap: &NodeCapture, context: &mut Context) {
    let func = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_this: &JsValue, _args: &[JsValue], cap: &NodeCapture, ctx: &mut Context| {
                cap.registry.perform_sync_reflow();
                let mut x = 0.0;
                let mut y = 0.0;
                let mut width = 0.0;
                let mut height = 0.0;

                let layout_tree_opt = cap.registry.layout_tree.borrow();
                if let Some(layout_tree_rc) = layout_tree_opt.as_ref() {
                    let layout_tree = layout_tree_rc.borrow();
                    if let Some(layout_box) = layout_tree.find_box_for_node(&cap.node) {
                        let rect = layout_box.rect();
                        x = rect.x;
                        y = rect.y;
                        width = rect.width;
                        height = rect.height;
                    }
                }

                let rect_obj = ObjectInitializer::new(ctx)
                    .property(js_string!("x"), x, Attribute::all())
                    .property(js_string!("y"), y, Attribute::all())
                    .property(js_string!("top"), y, Attribute::all())
                    .property(js_string!("left"), x, Attribute::all())
                    .property(js_string!("right"), x + width, Attribute::all())
                    .property(js_string!("bottom"), y + height, Attribute::all())
                    .property(js_string!("width"), width, Attribute::all())
                    .property(js_string!("height"), height, Attribute::all())
                    .build();
                Ok(rect_obj.into())
            },
            cap.clone(),
        ),
    )
    .name("getBoundingClientRect")
    .length(0)
    .build();

    obj.set(
        js_string!("getBoundingClientRect"),
        JsValue::from(func),
        false,
        context,
    )
    .expect("failed to set getBoundingClientRect");
}
