use super::*;
use crate::js_boa::style_class::style_object::StyleCap;

#[derive(Clone)]
struct StylePropertyCap {
    style_cap: StyleCap,
    property: &'static str,
}
unsafe impl Trace for StylePropertyCap {
    empty_trace!();
}
impl Finalize for StylePropertyCap {}

pub(super) fn install_css_text_accessor(obj: &JsObject, cap: &StyleCap, context: &mut Context) {
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &StyleCap, _ctx| {
            Ok(JsValue::from(JsString::from(serialize_style(
                &cap.style.borrow(),
            ))))
        },
        cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let css = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            *cap.style.borrow_mut() = parse_style_text(&css);
            sync_style_attribute(&cap.node, &cap.style.borrow());
            cap.registry.mark_style_dirty(&cap.node);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    install_accessor(obj, context, "cssText", Some(getter), Some(setter));
}

pub(super) fn install_style_property_accessors(
    obj: &JsObject,
    cap: &StyleCap,
    context: &mut Context,
) {
    for (js_name, css_name) in [
        ("backgroundColor", "background-color"),
        ("borderColor", "border-color"),
        ("borderRadius", "border-radius"),
        ("borderWidth", "border-width"),
        ("color", "color"),
        ("display", "display"),
        ("fontSize", "font-size"),
        ("fontWeight", "font-weight"),
        ("height", "height"),
        ("margin", "margin"),
        ("marginBottom", "margin-bottom"),
        ("marginLeft", "margin-left"),
        ("marginRight", "margin-right"),
        ("marginTop", "margin-top"),
        ("maxHeight", "max-height"),
        ("maxWidth", "max-width"),
        ("minHeight", "min-height"),
        ("minWidth", "min-width"),
        ("opacity", "opacity"),
        ("padding", "padding"),
        ("paddingBottom", "padding-bottom"),
        ("paddingLeft", "padding-left"),
        ("paddingRight", "padding-right"),
        ("paddingTop", "padding-top"),
        ("visibility", "visibility"),
        ("whiteSpace", "white-space"),
        ("width", "width"),
    ] {
        install_style_property_accessor(obj, cap, context, js_name, css_name);
    }
}

pub(super) fn parse_style_text(css: &str) -> BTreeMap<String, String> {
    let mut style = BTreeMap::new();
    for part in css.split(';') {
        if let Some((key, value)) = part.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                style.insert(key.to_string(), value.to_string());
            }
        }
    }
    style
}

pub(super) fn sync_style_attribute(node: &NodePtr, style: &BTreeMap<String, String>) {
    if let Node::Element(el) = &mut *node.borrow_mut() {
        let css = serialize_style(style);
        if css.is_empty() {
            el.attributes.remove("style");
        } else {
            el.attributes.insert("style".to_string(), css);
        }
    }
}

fn install_style_property_accessor(
    obj: &JsObject,
    cap: &StyleCap,
    context: &mut Context,
    js_name: &str,
    css_name: &'static str,
) {
    let property_cap = StylePropertyCap {
        style_cap: cap.clone(),
        property: css_name,
    };
    let getter = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &StylePropertyCap, _ctx| {
            let value = cap
                .style_cap
                .style
                .borrow()
                .get(cap.property)
                .cloned()
                .unwrap_or_default();
            Ok(JsValue::from(JsString::from(value)))
        },
        property_cap.clone(),
    );
    let setter = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StylePropertyCap, _ctx| {
            let value = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            set_style_property(&cap.style_cap, cap.property, value);
            Ok(JsValue::undefined())
        },
        property_cap,
    );
    install_accessor(obj, context, js_name, Some(getter), Some(setter));
}

fn set_style_property(cap: &StyleCap, property: &str, value: String) {
    if value.is_empty() {
        cap.style.borrow_mut().remove(property);
    } else {
        cap.style.borrow_mut().insert(property.to_string(), value);
    }
    sync_style_attribute(&cap.node, &cap.style.borrow());
    cap.registry.mark_style_dirty(&cap.node);
}

fn serialize_style(style: &BTreeMap<String, String>) -> String {
    style
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join("; ")
}
