use super::*;

pub(super) fn native_to_jsfn(context: &mut Context, native: NativeFunction) -> JsValue {
    FunctionObjectBuilder::new(context.realm(), native)
        .name(js_string!(""))
        .length(0)
        .constructor(false)
        .build()
        .into()
}

pub(super) fn js_string_of(value: &JsValue) -> String {
    value
        .as_string()
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|| {
            if value.is_undefined() || value.is_null() {
                String::new()
            } else {
                value.display().to_string()
            }
        })
}

pub(super) fn node_from_js(
    value: &JsValue,
    registry: &NodeRegistry,
    context: &mut Context,
) -> Option<NodePtr> {
    let obj = value.as_object()?;
    let id_val = obj.get(js_string!("__node_id"), context).ok()?;
    let id = id_val.as_number()? as u32;
    registry.lookup(id)
}
