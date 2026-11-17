//! Native `DOMException` (WebIDL §3.14.1).
//!
//! Hand-written rather than generated: the IDL generator only handles node
//! interfaces so far. Instances keep `name` and `message` in private symbols
//! and read them through prototype getters, as Blink does, and the prototype
//! inherits from `Error.prototype`. Native code throws through
//! [`throw_dom_exception`], which reaches the intrinsic constructor through a
//! context slot, so page code replacing `globalThis.DOMException` cannot
//! change what Aurora throws.

use super::node_create::v8_str;

/// Legacy `code` values, indexed by the spec's error-names table.
const LEGACY_CODES: [(&str, &str, u16); 25] = [
    ("IndexSizeError", "INDEX_SIZE_ERR", 1),
    ("DOMStringSizeError", "DOMSTRING_SIZE_ERR", 2),
    ("HierarchyRequestError", "HIERARCHY_REQUEST_ERR", 3),
    ("WrongDocumentError", "WRONG_DOCUMENT_ERR", 4),
    ("InvalidCharacterError", "INVALID_CHARACTER_ERR", 5),
    ("NoDataAllowedError", "NO_DATA_ALLOWED_ERR", 6),
    ("NoModificationAllowedError", "NO_MODIFICATION_ALLOWED_ERR", 7),
    ("NotFoundError", "NOT_FOUND_ERR", 8),
    ("NotSupportedError", "NOT_SUPPORTED_ERR", 9),
    ("InUseAttributeError", "INUSE_ATTRIBUTE_ERR", 10),
    ("InvalidStateError", "INVALID_STATE_ERR", 11),
    ("SyntaxError", "SYNTAX_ERR", 12),
    ("InvalidModificationError", "INVALID_MODIFICATION_ERR", 13),
    ("NamespaceError", "NAMESPACE_ERR", 14),
    ("InvalidAccessError", "INVALID_ACCESS_ERR", 15),
    ("ValidationError", "VALIDATION_ERR", 16),
    ("TypeMismatchError", "TYPE_MISMATCH_ERR", 17),
    ("SecurityError", "SECURITY_ERR", 18),
    ("NetworkError", "NETWORK_ERR", 19),
    ("AbortError", "ABORT_ERR", 20),
    ("URLMismatchError", "URL_MISMATCH_ERR", 21),
    ("QuotaExceededError", "QUOTA_EXCEEDED_ERR", 22),
    ("TimeoutError", "TIMEOUT_ERR", 23),
    ("InvalidNodeTypeError", "INVALID_NODE_TYPE_ERR", 24),
    ("DataCloneError", "DATA_CLONE_ERR", 25),
];

/// The intrinsic constructor, stored in the context's slot map.
struct Intrinsic(v8::Global<v8::Function>);

fn private_key<'s>(scope: &mut v8::PinScope<'s, '_>, field: &str) -> v8::Local<'s, v8::Private> {
    let name = v8_str(scope, &format!("aurora.DOMException.{field}"));
    v8::Private::for_api(scope, Some(name))
}

/// Define `DOMException` on the current context's global.
pub(super) fn install(scope: &mut v8::PinScope<'_, '_>) {
    let template = v8::FunctionTemplate::new(scope, construct);
    let class_name = v8_str(scope, "DOMException");
    template.set_class_name(class_name);
    template.read_only_prototype();
    let proto = template.prototype_template(scope);
    for field in ["name", "message", "code"] {
        let getter = v8::FunctionTemplate::builder(get_field)
            .data(v8_str(scope, field).into())
            .build(scope);
        proto.set_accessor_property(
            v8_str(scope, field).into(),
            Some(getter),
            None,
            v8::PropertyAttribute::NONE,
        );
    }
    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        class_name.into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );
    let constant = || v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_DELETE;
    for (_, constant_name, code) in LEGACY_CODES {
        let value = v8::Integer::new_from_unsigned(scope, code.into());
        template.set_with_attr(v8_str(scope, constant_name).into(), value.into(), constant());
        proto.set_with_attr(v8_str(scope, constant_name).into(), value.into(), constant());
    }

    let context = scope.get_current_context();
    let global = context.global(scope);
    let function = template.get_function(scope).expect("DOMException constructor");
    // WebIDL: DOMException.prototype's [[Prototype]] is %Error.prototype%.
    let error_key = v8_str(scope, "Error");
    let prototype_key = v8_str(scope, "prototype");
    if let Some(error) = global
        .get(scope, error_key.into())
        .and_then(|v| v.to_object(scope))
        .and_then(|error| error.get(scope, prototype_key.into()))
        && let Some(prototype) = function
            .get(scope, prototype_key.into())
            .and_then(|v| v.to_object(scope))
    {
        prototype.set_prototype(scope, error);
    }
    global.define_own_property(
        scope,
        class_name.into(),
        function.into(),
        v8::PropertyAttribute::DONT_ENUM,
    );
    context.set_slot(std::rc::Rc::new(Intrinsic(v8::Global::new(scope, function))));
}

/// `new DOMException(message = "", name = "Error")`.
fn construct(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    if !args.is_construct_call() {
        let message = v8_str(scope, "Failed to construct 'DOMException': Please use the 'new' operator");
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return;
    }
    let mut fields = [("message", ""), ("name", "Error")].map(|(field, default)| {
        (field, default.to_string())
    });
    for (index, (_, value)) in fields.iter_mut().enumerate() {
        let arg = args.get(index as i32);
        if arg.is_undefined() {
            continue;
        }
        // ToString can run page code and throw (e.g. a Symbol argument).
        let Some(string) = arg.to_string(scope) else {
            return;
        };
        *value = string.to_rust_string_lossy(scope);
    }
    let this = args.this();
    for (field, value) in fields {
        let key = private_key(scope, field);
        let value = v8_str(scope, &value);
        this.set_private(scope, key, value.into());
    }
    let context = scope.get_current_context();
    v8::Exception::capture_stack_trace(context, this);
    rv.set(this.into());
}

/// Prototype getter for `name`, `message` and `code`.
fn get_field(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let field = args.data().to_rust_string_lossy(scope);
    let this = args.this();
    let name_key = private_key(scope, "name");
    if !this.has_private(scope, name_key).unwrap_or(false) {
        let message = v8_str(scope, "Illegal invocation");
        let error = v8::Exception::type_error(scope, message);
        scope.throw_exception(error);
        return;
    }
    if field == "code" {
        let name = this
            .get_private(scope, name_key)
            .map(|v| v.to_rust_string_lossy(scope))
            .unwrap_or_default();
        let code = LEGACY_CODES
            .iter()
            .find(|(n, _, _)| *n == name)
            .map_or(0, |(_, _, code)| *code);
        rv.set(v8::Integer::new_from_unsigned(scope, code.into()).into());
        return;
    }
    let key = private_key(scope, &field);
    if let Some(value) = this.get_private(scope, key) {
        rv.set(value);
    }
}

/// `new DOMException(message, name)` built with the intrinsic constructor.
/// Falls back to a named `Error` in contexts without one installed.
pub(super) fn create<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
    message: &str,
) -> Option<v8::Local<'s, v8::Value>> {
    let context = scope.get_current_context();
    let message = v8_str(scope, message);
    let name_value = v8_str(scope, name);
    if let Some(intrinsic) = context.get_slot::<Intrinsic>() {
        let ctor = v8::Local::new(scope, &intrinsic.0);
        return ctor
            .new_instance(scope, &[message.into(), name_value.into()])
            .map(Into::into);
    }
    let error = v8::Exception::error(scope, message);
    let key = v8_str(scope, "name");
    error.to_object(scope)?.set(scope, key.into(), name_value.into());
    Some(error)
}

/// Throw a DOMException; see [`create`].
pub(super) fn throw_dom_exception(scope: &mut v8::PinScope<'_, '_>, name: &str, message: &str) {
    if let Some(exception) = create(scope, name, message) {
        scope.throw_exception(exception);
    }
}

#[cfg(test)]
mod tests {
    use super::super::V8Runtime;
    use crate::html::Parser;

    fn eval(source: &str) -> String {
        let mut runtime = V8Runtime::new(Parser::new("<html><body></body></html>").parse_document());
        runtime.eval_to_string(source).unwrap()
    }

    #[test]
    fn constructor_follows_webidl() {
        assert_eq!(
            eval(
                r#"(() => {
            const e = new DOMException('boom', 'NotFoundError');
            const plain = new DOMException();
            return [e.name, e.message, e.code, plain.name, plain.message, plain.code,
                e instanceof Error, Object.getPrototypeOf(DOMException.prototype) === Error.prototype,
                Object.prototype.toString.call(e), String(e), typeof e.stack,
                DOMException.NOT_FOUND_ERR, e.DATA_CLONE_ERR,
                Object.keys(e).length, 'name' in DOMException.prototype].join('|');
        })()"#
            ),
            "NotFoundError|boom|8|Error||0|true|true|[object DOMException]|NotFoundError: boom|string|8|25|0|true"
        );
    }

    #[test]
    fn rejects_calls_and_foreign_receivers() {
        assert_eq!(
            eval(
                r#"[() => DOMException('x'),
                    () => Object.getOwnPropertyDescriptor(DOMException.prototype, 'name').get.call({})
                ].map(f => { try { f(); return 'ok'; } catch (e) { return e.constructor.name; } }).join('|')"#
            ),
            "TypeError|TypeError"
        );
    }

    #[test]
    fn native_errors_are_intrinsic_dom_exceptions() {
        assert_eq!(
            eval(
                r#"(() => {
            const Intrinsic = DOMException;
            globalThis.DOMException = function Fake() {};
            const thrown = [];
            try { customElements.define('not-valid-!', class extends HTMLElement {}); }
            catch (e) { thrown.push(e instanceof Intrinsic, e.name); }
            try { document.createElement('x-plain').attachInternals(); }
            catch (e) { thrown.push(e instanceof Intrinsic, e.name); }
            return thrown.join('|');
        })()"#
            ),
            "true|SyntaxError|true|NotSupportedError"
        );
    }
}
