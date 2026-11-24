use crate::css::parse_style_text;
use crate::dom::{Node, NodePtr};
use crate::js_v8::node_create::{NodeData, v8_str};
use crate::js_v8::templates::{self, NativeTag, TemplateKind};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub(crate) struct StyleData {
    pub(crate) node: NodePtr,
    pub(crate) registry: Rc<crate::js_v8::registry::NodeRegistry>,
    pub(crate) style: Rc<RefCell<BTreeMap<String, String>>>,
}

pub(crate) fn build_style_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Object> {
    let style = Rc::new(RefCell::new(BTreeMap::<String, String>::new()));
    if let Node::Element(el) = &*node_data.node.borrow() {
        if let Some(css) = el.attributes.get("style") {
            *style.borrow_mut() = parse_style_text(css);
        }
    }

    // Never freed, like the wrapper's `NodeData`.
    let style_data: *const StyleData = Box::into_raw(Box::new(StyleData {
        node: node_data.node.clone(),
        registry: node_data.registry.clone(),
        style: style.clone(),
    }));

    let template = templates::cached(scope, &node_data.registry, TemplateKind::Style, |scope, template| {
        install_method(scope, template, "getPropertyValue", get_property_value);
        install_method(scope, template, "setProperty", set_property);
        install_method(scope, template, "removeProperty", remove_property);
        install_method(scope, template, "item", item);
        install_accessor(scope, template, "cssText", get_css_text, set_css_text);
        for (js_name, css_name) in STYLE_PROPERTIES {
            install_style_property_accessor(scope, template, js_name, css_name);
        }
    });
    templates::instantiate(scope, template, NativeTag::Style, style_data)
}

fn this_style<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
) -> Option<&'a StyleData> {
    templates::receiver(scope, args.this(), NativeTag::Style)
}

fn holder_style<'a>(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::PropertyCallbackArguments,
) -> Option<&'a StyleData> {
    templates::receiver(scope, args.holder(), NativeTag::Style)
}

fn install_method<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
) {
    let t = v8::FunctionTemplate::builder(callback).build(scope);
    let name_str = v8_str(scope, name);
    template.set(name_str.into(), t.into());
}

fn install_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    setter: impl v8::MapFnTo<v8::AccessorNameSetterCallback>,
) {
    let name_str = v8_str(scope, name);
    template.set_accessor_with_configuration(
        name_str.into(),
        v8::AccessorConfiguration::new(getter).setter(setter),
    );
}

fn get_property_value(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let Some(style_data) = this_style(scope, &args) else {
        return;
    };

    let style = style_data.style.borrow();
    let val = style.get(&k).cloned().unwrap_or_default();
    let s = v8_str(scope, &val);
    retval.set(s.into());
}

fn set_property(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let v = args.get(1).to_rust_string_lossy(scope);
    let Some(style_data) = this_style(scope, &args) else {
        return;
    };

    {
        let mut style = style_data.style.borrow_mut();
        if v.is_empty() {
            style.remove(&k);
        } else {
            style.insert(k, v);
        }
        sync_style_attribute(&style_data.node, &style);
    }
    style_data.registry.mark_style_dirty(&style_data.node);
}

fn remove_property(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let Some(style_data) = this_style(scope, &args) else {
        return;
    };

    {
        let mut style = style_data.style.borrow_mut();
        style.remove(&k);
        sync_style_attribute(&style_data.node, &style);
    }
    style_data.registry.mark_style_dirty(&style_data.node);
}

fn item(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let Some(style_data) = this_style(scope, &args) else {
        return;
    };

    let style = style_data.style.borrow();
    if let Some(k) = style.keys().nth(idx) {
        let s = v8_str(scope, k);
        retval.set(s.into());
    } else {
        let empty = v8_str(scope, "");
        retval.set(empty.into());
    }
}

fn get_css_text(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(style_data) = holder_style(scope, &args) else {
        return;
    };

    let style = style_data.style.borrow();
    let css = serialize_style(&style);
    let s = v8_str(scope, &css);
    retval.set(s.into());
}

fn set_css_text(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    let Some(style_data) = holder_style(scope, &args) else {
        return;
    };

    let css = value.to_rust_string_lossy(scope);
    {
        let mut style = style_data.style.borrow_mut();
        *style = parse_style_text(&css);
        sync_style_attribute(&style_data.node, &style);
    }
    style_data.registry.mark_style_dirty(&style_data.node);
}

fn serialize_style(style: &BTreeMap<String, String>) -> String {
    style
        .iter()
        .map(|(key, value)| format!("{key}: {value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn sync_style_attribute(node: &NodePtr, style: &BTreeMap<String, String>) {
    if let Node::Element(el) = &mut *node.borrow_mut() {
        let css = serialize_style(style);
        if css.is_empty() {
            el.attributes.remove("style");
        } else {
            el.attributes.insert("style".to_string(), css);
        }
    }
}

const STYLE_PROPERTIES: &[(&str, &str)] = &[
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
];

fn install_style_property_accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<v8::ObjectTemplate>,
    js_name: &str,
    _css_name: &'static str,
) {
    // V8 accessors don't easily support capturing extra data like the property name
    // without another level of indirection or using a single callback that checks the name.
    // However, template.set_accessor_with_data passes the 'name' to the getter.

    // BUT 'name' is the name of the property being accessed (e.g., "backgroundColor").
    // We can use that if we have a mapping.

    // For simplicity, let's use a single getter/setter that looks up the property name.
    // We need to pass the mapping to the callback.

    template.set_accessor_with_configuration(
        v8_str(scope, js_name).into(),
        v8::AccessorConfiguration::new(style_property_getter).setter(style_property_setter),
    );
}

fn style_property_getter(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let Some(style_data) = holder_style(scope, &args) else {
        return;
    };

    let js_name = name.to_rust_string_lossy(scope);
    if let Some(css_name) = js_to_css_name(&js_name) {
        let style = style_data.style.borrow();
        let val = style.get(css_name).cloned().unwrap_or_default();
        let s = v8_str(scope, &val);
        retval.set(s.into());
    }
}

fn style_property_setter(
    scope: &mut v8::PinScope<'_, '_>,
    name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
    _retval: v8::ReturnValue<()>,
) {
    let Some(style_data) = holder_style(scope, &args) else {
        return;
    };

    let js_name = name.to_rust_string_lossy(scope);
    if let Some(css_name) = js_to_css_name(&js_name) {
        let val = value.to_rust_string_lossy(scope);
        {
            let mut style = style_data.style.borrow_mut();
            if val.is_empty() {
                style.remove(css_name);
            } else {
                style.insert(css_name.to_string(), val);
            }
            sync_style_attribute(&style_data.node, &style);
        }
        style_data.registry.mark_style_dirty(&style_data.node);
    }
}

fn js_to_css_name(js_name: &str) -> Option<&'static str> {
    for (j, c) in STYLE_PROPERTIES {
        if *j == js_name {
            return Some(c);
        }
    }
    None
}
