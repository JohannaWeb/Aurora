use v8;
use crate::dom::{Node, NodePtr};
use crate::js_v8::node_create::{NodeData, create_js_node};
use crate::css::parse_style_text;
use std::collections::BTreeMap;
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) struct StyleData {
    pub(crate) node: NodePtr,
    pub(crate) registry: Rc<crate::js_v8::registry::NodeRegistry>,
    pub(crate) style: Rc<RefCell<BTreeMap<String, String>>>,
}

pub(crate) fn build_style_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    node_data: &NodeData,
) -> v8::Local<'s, v8::Object> {
    let style = Rc::new(RefCell::new(BTreeMap::<String, String>::new()));
    if let Node::Element(el) = &*node_data.node.borrow() {
        if let Some(css) = el.attributes.get("style") {
            *style.borrow_mut() = parse_style_text(css);
        }
    }

    let style_data = Box::into_raw(Box::new(StyleData {
        node: node_data.node.clone(),
        registry: node_data.registry.clone(),
        style: style.clone(),
    })) as *mut _;
    let style_external = v8::External::new(scope, style_data);

    let template = v8::ObjectTemplate::new(scope);

    install_method(scope, template, "getPropertyValue", get_property_value, style_external);
    install_method(scope, template, "setProperty", set_property, style_external);
    install_method(scope, template, "removeProperty", remove_property, style_external);
    install_method(scope, template, "item", item, style_external);

    install_accessor(scope, template, "cssText", get_css_text, Some(set_css_text), style_external);

    // Style properties
    for (js_name, css_name) in STYLE_PROPERTIES {
        install_style_property_accessor(scope, template, js_name, css_name, style_external);
    }

    template.new_instance(scope).unwrap()
}

fn install_method<'s>(
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'s, v8::External>,
) {
    let t = v8::FunctionTemplate::builder(callback)
        .data(data.into())
        .build(scope);
    let name_str = v8::String::new(scope, name).unwrap();
    template.set(name_str.into(), t.into());
}

fn install_accessor<'s>(
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<v8::ObjectTemplate>,
    name: &str,
    getter: impl v8::MapFnTo<v8::AccessorNameGetterCallback>,
    setter: Option<impl v8::MapFnTo<v8::AccessorNameSetterCallback>>,
    data: v8::Local<'s, v8::External>,
) {
    let name_str = v8::String::new(scope, name).unwrap();
    if let Some(s) = setter {
        template.set_accessor_with_data_setter(
            name_str.into(),
            getter,
            s,
            data.into(),
        );
    } else {
        template.set_accessor_with_data(
            name_str.into(),
            getter,
            data.into(),
        );
    }
}

fn get_property_value(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

    let style = style_data.style.borrow();
    let val = style.get(&k).cloned().unwrap_or_default();
    let s = v8::String::new(scope, &val).unwrap();
    retval.set(s.into());
}

fn set_property(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let v = args.get(1).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

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
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut _retval: v8::ReturnValue,
) {
    let k = args.get(0).to_rust_string_lossy(scope);
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

    {
        let mut style = style_data.style.borrow_mut();
        style.remove(&k);
        sync_style_attribute(&style_data.node, &style);
    }
    style_data.registry.mark_style_dirty(&style_data.node);
}

fn item(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let idx = args.get(0).uint32_value(scope).unwrap_or(0) as usize;
    let data = args.data();
    let external = v8::Local::<v8::External>::try_from(data).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

    let style = style_data.style.borrow();
    if let Some(k) = style.keys().nth(idx) {
        let s = v8::String::new(scope, k).unwrap();
        retval.set(s.into());
    } else {
        let empty = v8::String::new(scope, "").unwrap();
        retval.set(empty.into());
    }
}

fn get_css_text(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

    let style = style_data.style.borrow();
    let css = serialize_style(&style);
    let s = v8::String::new(scope, &css).unwrap();
    retval.set(s.into());
}

fn set_css_text(
    scope: &mut v8::HandleScope,
    _name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

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
    scope: &mut v8::HandleScope<'s>,
    template: v8::Local<v8::ObjectTemplate>,
    js_name: &str,
    css_name: &'static str,
    data: v8::Local<'s, v8::External>,
) {
    // V8 accessors don't easily support capturing extra data like the property name
    // without another level of indirection or using a single callback that checks the name.
    // However, template.set_accessor_with_data passes the 'name' to the getter.
    
    // BUT 'name' is the name of the property being accessed (e.g., "backgroundColor").
    // We can use that if we have a mapping.
    
    // For simplicity, let's use a single getter/setter that looks up the property name.
    // We need to pass the mapping to the callback.
    
    template.set_accessor_with_data_setter(
        v8::String::new(scope, js_name).unwrap().into(),
        style_property_getter,
        style_property_setter,
        data.into(),
    );
}

fn style_property_getter(
    scope: &mut v8::HandleScope,
    name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

    let js_name = name.to_rust_string_lossy(scope);
    if let Some(css_name) = js_to_css_name(&js_name) {
        let style = style_data.style.borrow();
        let val = style.get(css_name).cloned().unwrap_or_default();
        let s = v8::String::new(scope, &val).unwrap();
        retval.set(s.into());
    }
}

fn style_property_setter(
    scope: &mut v8::HandleScope,
    name: v8::Local<v8::Name>,
    value: v8::Local<v8::Value>,
    args: v8::PropertyCallbackArguments,
) {
    let external = v8::Local::<v8::External>::try_from(args.data()).unwrap();
    let style_data = unsafe { &*(external.value() as *const StyleData) };

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
