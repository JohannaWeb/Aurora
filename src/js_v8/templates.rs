//! Object templates shared by every native object of a kind.
//!
//! Element wrappers, their `style` and `classList` objects and `attributes`
//! maps used to be built from a fresh `ObjectTemplate` with fresh
//! `FunctionTemplate`s whose data was that object's native pointer. V8 never
//! frees a `FunctionTemplate`, every such object got its own hidden class,
//! and `classList`/`attributes` rebuilt theirs on each access, so the heap
//! grew with every node and every access: ~240 MB of V8 heap on a Wikipedia
//! article, most of it templates, accessor records and object shapes.
//!
//! Now each kind's template is built once per runtime and cached in the
//! registry. Its callbacks carry no data; they find the native pointer in the
//! receiver's internal fields, tagged with what it points to so a method
//! called on the wrong kind of object throws instead of misreading memory.

use super::registry::NodeRegistry;

/// Which shared template an object is instantiated from.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(super) enum TemplateKind {
    Element,
    /// A registered engine shadow root: also has a native `host` accessor.
    ShadowRoot,
    /// A `<slot>`: also has `assignedNodes`/`assignedElements`.
    Slot,
    Style,
    ComputedStyle,
    ClassList,
    NamedNodeMap,
    ElementInternals,
}

/// What the pointer in an object's internal fields points to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum NativeTag {
    /// `node_create::NodeData`.
    Node = 1,
    /// `style::StyleData`.
    Style = 2,
    /// `computed_style::ComputedStyleData`.
    ComputedStyle = 3,
}

const POINTER_FIELD: usize = 0;
const TAG_FIELD: usize = 1;
const FIELD_COUNT: usize = 2;

/// The cached template for `kind`, built by `build` on first use.
pub(super) fn cached<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    registry: &NodeRegistry,
    kind: TemplateKind,
    build: impl FnOnce(&mut v8::PinScope<'s, '_>, v8::Local<'s, v8::ObjectTemplate>),
) -> v8::Local<'s, v8::ObjectTemplate> {
    if let Some(global) = registry.templates.borrow().get(&kind) {
        return v8::Local::new(scope, global);
    }
    let template = v8::ObjectTemplate::new(scope);
    template.set_internal_field_count(FIELD_COUNT);
    build(scope, template);
    registry
        .templates
        .borrow_mut()
        .insert(kind, v8::Global::new(scope, template));
    template
}

/// Instantiate `template` and point it at `native`, which must outlive it.
pub(super) fn instantiate<'s, T>(
    scope: &mut v8::PinScope<'s, '_>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    tag: NativeTag,
    native: *const T,
) -> v8::Local<'s, v8::Object> {
    let obj = template
        .new_instance(scope)
        .expect("object template instantiation failed");
    let external = v8::External::new(scope, native as *mut std::ffi::c_void);
    obj.set_internal_field(POINTER_FIELD, external.into());
    let tag = v8::Integer::new(scope, tag as i32);
    obj.set_internal_field(TAG_FIELD, tag.into());
    obj
}

/// The native data behind `obj`, if it was instantiated with `tag`.
pub(super) fn native<'a, T>(
    scope: &v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
    tag: NativeTag,
) -> Option<&'a T> {
    if obj.internal_field_count() < FIELD_COUNT {
        return None;
    }
    let stored: v8::Local<v8::Value> = obj.get_internal_field(scope, TAG_FIELD)?.try_into().ok()?;
    if !stored.is_int32() || stored.int32_value(scope)? != tag as i32 {
        return None;
    }
    let pointer: v8::Local<v8::Value> = obj.get_internal_field(scope, POINTER_FIELD)?.try_into().ok()?;
    let external = v8::Local::<v8::External>::try_from(pointer).ok()?;
    // SAFETY: only `instantiate` sets these fields, with a `T` for this tag
    // that lives as long as the registry.
    Some(unsafe { &*(external.value() as *const T) })
}

/// `native`, throwing a TypeError like a browser's "Illegal invocation" when
/// the receiver is not that kind of object.
pub(super) fn receiver<'a, T>(
    scope: &mut v8::PinScope<'_, '_>,
    obj: v8::Local<v8::Object>,
    tag: NativeTag,
) -> Option<&'a T> {
    let found = native(scope, obj, tag);
    if found.is_none() {
        let message = v8::String::new(scope, "Illegal invocation").expect("string allocation");
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
    }
    found
}
