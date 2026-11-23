//! Runtime support for the bindings generated from `idl/*.idl` (see
//! `codegen/generate.rs`): interface objects, WebIDL argument conversion,
//! receiver brand checks and exception mapping. Member behaviour lives in
//! `dom_impl.rs`, which implements the generated `<Interface>Members` traits.

#[allow(unused_imports)] // Used by generated [CEReactions] members.
use super::custom_elements::CeReactionsGuard;
use super::dom_impl::Dom;
use super::node_create::{create_js_node, interface_for, node_from_js, v8_str};
use super::registry::NodeRegistry;
use crate::dom::NodePtr;
use std::rc::Rc;

include!(concat!(env!("OUT_DIR"), "/web_interfaces.rs"));

/// Why a binding call failed, mapped to the exception WebIDL requires.
#[derive(Debug)]
pub(super) enum Error {
    Type(String),
    Dom { name: &'static str, message: String },
    /// A JS exception is already pending (e.g. a throwing `toString`).
    Pending,
}

pub(super) type Fallible<T> = Result<T, Error>;

/// A `(Node or DOMString)` argument.
#[derive(Debug)]
pub(super) enum NodeOrString {
    Node(NodePtr),
    String(String),
}

pub(super) fn install(scope: &mut v8::PinScope<'_, '_>, registry: &Rc<NodeRegistry>) {
    // Generated callbacks find the registry here rather than in per-function
    // data, so it lives exactly as long as the context.
    scope.get_current_context().set_slot(registry.clone());
    install_interfaces(scope, registry);
    let focus_from_click = v8::FunctionTemplate::new(scope, focus_from_click);
    let focus_from_click = focus_from_click.get_function(scope).expect("focus hook");
    let global = scope.get_current_context().global(scope);
    let key = v8_str(scope, "__aurora_focus_from_click__");
    global.define_own_property(scope, key.into(), focus_from_click.into(), v8::PropertyAttribute::DONT_ENUM);
    let edit = v8::FunctionTemplate::new(scope, edit_text_input);
    let edit = edit.get_function(scope).expect("edit hook");
    let key = v8_str(scope, "__aurora_edit_text_input__");
    global.define_own_property(scope, key.into(), edit.into(), v8::PropertyAttribute::DONT_ENUM);
}

/// `__aurora_edit_text_input__(element, operation, data)`: a keyboard edit
/// of a focused text control (v8_post.js `__aurora_dispatch_key__`).
/// `operation` "check" only reports whether the element is a text control.
fn edit_text_input(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(registry) = scope.get_current_context().get_slot::<NodeRegistry>() else { return };
    let Some(target) = node_from_js(scope, args.get(0), &registry) else {
        rv.set(v8::Boolean::new(scope, false).into());
        return;
    };
    let operation = args.get(1).to_rust_string_lossy(scope);
    let data = args.get(2).to_rust_string_lossy(scope);
    let result = if operation == "check" {
        super::dom_impl::is_text_control(&target)
    } else {
        super::dom_impl::edit_text_input(&registry, &target, &operation, &data)
    };
    rv.set(v8::Boolean::new(scope, result).into());
}

/// `__aurora_focus_from_click__(element)`: the focus part of a click's
/// mousedown default action (v8_post.js `__aurora_dispatch_click__`).
fn focus_from_click(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    let Some(registry) = scope.get_current_context().get_slot::<NodeRegistry>() else { return };
    if let Some(target) = node_from_js(scope, args.get(0), &registry) {
        super::dom_impl::focus_from_click(scope, &registry, &target);
    }
}

fn define_interface<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &NodeRegistry,
    name: &'static str,
    parent: Option<v8::Local<'s, v8::FunctionTemplate>>,
    members: fn(&mut v8::PinScope<'_, '_>, v8::Local<v8::FunctionTemplate>),
) -> v8::Local<'s, v8::FunctionTemplate> {
    let template = v8::FunctionTemplate::builder(constructor)
        .data(v8_str(scope, name).into())
        .build(scope);
    template.set_class_name(v8_str(scope, name));
    template.read_only_prototype();
    if let Some(parent) = parent {
        template.inherit(parent);
    }
    template.prototype_template(scope).set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        v8_str(scope, name).into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );
    // Templates freeze once instantiated, so members go on first.
    members(scope, template);
    let function = template.get_function(scope).expect("interface constructor");
    if let Some(parent) = parent {
        let parent = parent.get_function(scope).expect("parent constructor");
        function.set_prototype(scope, parent.into());
    }
    let prototype = function
        .get(scope, v8_str(scope, "prototype").into())
        .unwrap();
    let prototype = v8::Local::<v8::Object>::try_from(prototype).unwrap();
    registry
        .interface_prototypes
        .borrow_mut()
        .insert(name, v8::Global::new(scope, prototype));
    scope
        .get_current_context()
        .global(scope)
        .define_own_property(
            scope,
            v8_str(scope, name).into(),
            function.into(),
            v8::PropertyAttribute::DONT_ENUM,
        );
    template
}

pub(super) fn set_prototype(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &NodeRegistry,
    object: v8::Local<v8::Object>,
    interface: &str,
) {
    let prototypes = registry.interface_prototypes.borrow();
    let prototype = v8::Local::new(
        scope,
        prototypes.get(interface).expect("generated interface"),
    );
    object.set_prototype(scope, prototype.into());
}

// Constructor behavior migrates with the members in P2.2–P2.5. Keep the
// custom-element construction stack behind the generated HTMLElement identity.
fn constructor(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if args.data().to_rust_string_lossy(scope) != "HTMLElement" {
        return;
    }
    let global = scope.get_current_context().global(scope);
    let hook = global.get(
        scope,
        v8_str(scope, "__aurora_construct_html_element__").into(),
    );
    if let Some(hook) = hook.and_then(|v| v8::Local::<v8::Function>::try_from(v).ok()) {
        let is_constructor = v8::Boolean::new(scope, args.is_construct_call());
        if let Some(value) = hook.call(
            scope,
            args.this().into(),
            &[args.new_target(), is_constructor.into()],
        ) {
            rv.set(value);
        }
    }
}

// ─── Helpers used by generated code ──────────────────────────────────────────

/// A non-constructor function for an operation or accessor.
fn accessor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    name: &str,
    length: i32,
) -> v8::Local<'s, v8::FunctionTemplate> {
    let function = v8::FunctionTemplate::builder(callback)
        .length(length)
        .constructor_behavior(v8::ConstructorBehavior::Throw)
        .build(scope);
    function.set_class_name(v8_str(scope, name));
    function.remove_prototype();
    function
}

/// Constants live on both the interface object and its prototype.
fn define_constant(
    scope: &mut v8::PinScope<'_, '_>,
    template: v8::Local<v8::FunctionTemplate>,
    proto: v8::Local<v8::ObjectTemplate>,
    name: &str,
    value: v8::Local<v8::Data>,
) {
    let attributes = || v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE;
    template.set_with_attr(v8_str(scope, name).into(), value, attributes());
    proto.set_with_attr(v8_str(scope, name).into(), value, attributes());
}

/// Whether the generated interface `name` is `target` or inherits from it.
pub(super) fn implements(mut name: &str, target: &str) -> bool {
    loop {
        if name == target {
            return true;
        }
        match parent_interface(name) {
            Some(parent) => name = parent,
            None => return false,
        }
    }
}

/// The node behind `value`, if it is a wrapper implementing `interface`.
fn unwrap_node(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &NodeRegistry,
    value: v8::Local<v8::Value>,
    interface: &str,
) -> Option<NodePtr> {
    node_from_js(scope, value, registry).filter(|node| implements(interface_for(node), interface))
}

fn unwrap_this(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &NodeRegistry,
    this: v8::Local<v8::Object>,
    interface: &str,
) -> Fallible<NodePtr> {
    unwrap_node(scope, registry, this.into(), interface)
        .ok_or_else(|| Error::Type("Illegal invocation".into()))
}

fn check_arity(args: &v8::FunctionCallbackArguments, required: i32, context: &str) -> Fallible<()> {
    if args.length() >= required {
        return Ok(());
    }
    let plural = if required == 1 { "" } else { "s" };
    Err(Error::Type(format!(
        "{context}: {required} argument{plural} required, but only {} present.",
        args.length()
    )))
}

/// WebIDL §3.2 conversions from JS values.
#[allow(dead_code)] // Each conversion is used once an IDL member needs it.
mod conv {
    use super::{Error, Fallible, NodeOrString, NodePtr, NodeRegistry, unwrap_node};

    pub(super) fn string(
        scope: &mut v8::PinScope<'_, '_>,
        v: v8::Local<v8::Value>,
    ) -> Fallible<String> {
        Ok(v.to_string(scope).ok_or(Error::Pending)?.to_rust_string_lossy(scope))
    }

    /// ConvertToInt without [EnforceRange]/[Clamp]: truncate, then wrap
    /// modulo 2^bits into the type's range.
    pub(super) fn integer(
        scope: &mut v8::PinScope<'_, '_>,
        v: v8::Local<v8::Value>,
        bits: u32,
        signed: bool,
    ) -> Fallible<f64> {
        Ok(wrap_integer(v.number_value(scope).ok_or(Error::Pending)?, bits, signed))
    }

    pub(super) fn wrap_integer(x: f64, bits: u32, signed: bool) -> f64 {
        if !x.is_finite() || x == 0.0 {
            return 0.0;
        }
        let modulus = 2f64.powi(bits as i32);
        let x = x.trunc().rem_euclid(modulus);
        if signed && x >= modulus / 2.0 { x - modulus } else { x }
    }

    pub(super) fn float(
        scope: &mut v8::PinScope<'_, '_>,
        v: v8::Local<v8::Value>,
        double: bool,
        unrestricted: bool,
    ) -> Fallible<f64> {
        let x = v.number_value(scope).ok_or(Error::Pending)?;
        let x = if double { x } else { x as f32 as f64 };
        if !unrestricted && !x.is_finite() {
            return Err(Error::Type("The provided value is non-finite.".into()));
        }
        Ok(x)
    }

    pub(super) fn interface(
        scope: &mut v8::PinScope<'_, '_>,
        registry: &NodeRegistry,
        v: v8::Local<v8::Value>,
        interface: &str,
    ) -> Fallible<NodePtr> {
        unwrap_node(scope, registry, v, interface)
            .ok_or_else(|| Error::Type(format!("parameter is not of type '{interface}'.")))
    }

    /// A Node stays a node; anything else converts to a string.
    pub(super) fn node_or_string(
        scope: &mut v8::PinScope<'_, '_>,
        registry: &NodeRegistry,
        v: v8::Local<v8::Value>,
    ) -> Fallible<NodeOrString> {
        match unwrap_node(scope, registry, v, "Node") {
            Some(node) => Ok(NodeOrString::Node(node)),
            None => string(scope, v).map(NodeOrString::String),
        }
    }
}

/// Conversion of a binding's Rust result back to JS.
trait ToJs {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        registry: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value>;
}

impl ToJs for () {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        _: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        v8::undefined(scope).into()
    }
}

impl ToJs for bool {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        _: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        v8::Boolean::new(scope, self).into()
    }
}

impl ToJs for String {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        _: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        v8_str(scope, &self).into()
    }
}

macro_rules! number_to_js {
    ($($t:ty),*) => {$(
        impl ToJs for $t {
            fn to_js<'s>(
                self,
                scope: &mut v8::PinScope<'s, '_>,
                _: &Rc<NodeRegistry>,
            ) -> v8::Local<'s, v8::Value> {
                v8::Number::new(scope, self as f64).into()
            }
        }
    )*};
}
number_to_js!(i8, u8, i16, u16, i32, u32, i64, u64, f32, f64);

impl ToJs for NodePtr {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        registry: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        let document = registry.document.borrow().clone().unwrap_or_else(|| self.clone());
        create_js_node(scope, self, registry, &document).into()
    }
}

/// Interim `NodeList`/`HTMLCollection`: a fresh array, as before P2.3.
impl ToJs for Vec<NodePtr> {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        registry: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        let array = v8::Array::new(scope, self.len() as i32);
        for (index, node) in self.into_iter().enumerate() {
            let wrapper = node.to_js(scope, registry);
            array.set_index(scope, index as u32, wrapper);
        }
        array.into()
    }
}

impl<T: ToJs> ToJs for Option<T> {
    fn to_js<'s>(
        self,
        scope: &mut v8::PinScope<'s, '_>,
        registry: &Rc<NodeRegistry>,
    ) -> v8::Local<'s, v8::Value> {
        match self {
            Some(value) => value.to_js(scope, registry),
            None => v8::null(scope).into(),
        }
    }
}

fn finish<T: ToJs>(
    scope: &mut v8::PinScope<'_, '_>,
    registry: &Rc<NodeRegistry>,
    result: Fallible<T>,
    rv: &mut v8::ReturnValue,
) {
    match result {
        Ok(value) => rv.set(value.to_js(scope, registry)),
        Err(Error::Type(message)) => {
            let message = v8_str(scope, &message);
            let error = v8::Exception::type_error(scope, message);
            scope.throw_exception(error);
        }
        Err(Error::Dom { name, message }) => {
            super::dom_exception::throw_dom_exception(scope, name, &message);
        }
        Err(Error::Pending) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::conv::wrap_integer;

    #[test]
    fn integer_conversion_wraps_like_webidl() {
        let cases = [
            (1.9, 32, true, 1.0),
            (-1.9, 32, true, -1.0),
            (f64::NAN, 32, true, 0.0),
            (f64::INFINITY, 16, false, 0.0),
            (-1.0, 16, false, 65535.0),
            (65536.0 + 5.0, 16, false, 5.0),
            (2147483648.0, 32, true, -2147483648.0),
            (4294967295.0, 32, true, -1.0),
            (-129.0, 8, true, 127.0),
            (300.0, 8, false, 44.0),
        ];
        for (input, bits, signed, expected) in cases {
            assert_eq!(wrap_integer(input, bits, signed), expected, "{input} as {bits}/{signed}");
        }
    }
}
