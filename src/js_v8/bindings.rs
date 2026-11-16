use super::{node_create::v8_str, registry::NodeRegistry};

include!(concat!(env!("OUT_DIR"), "/web_interfaces.rs"));

fn define_interface<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &NodeRegistry,
    name: &'static str,
    parent: Option<v8::Local<'s, v8::FunctionTemplate>>,
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
