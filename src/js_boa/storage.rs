use super::*;

pub(super) fn install_storage(
    context: &mut Context,
    global: &JsObject,
    name: &str,
    backing: Rc<RefCell<BTreeMap<String, String>>>,
) {
    #[derive(Clone)]
    struct StorageCap(Rc<RefCell<BTreeMap<String, String>>>);
    unsafe impl Trace for StorageCap {
        empty_trace!();
    }
    impl Finalize for StorageCap {}

    let cap = StorageCap(backing);

    let get_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            match cap.0.borrow().get(&key) {
                Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );

    let set_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let val = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            cap.0.borrow_mut().insert(key, val);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let remove_item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let key = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            cap.0.borrow_mut().remove(&key);
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let clear = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &StorageCap, _ctx| {
            cap.0.borrow_mut().clear();
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );

    let key_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StorageCap, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let map = cap.0.borrow();
            match map.keys().nth(idx) {
                Some(k) => Ok(JsValue::from(JsString::from(k.clone()))),
                None => Ok(JsValue::null()),
            }
        },
        cap.clone(),
    );

    let storage = ObjectInitializer::new(context)
        .function(get_item, js_string!("getItem"), 1)
        .function(set_item, js_string!("setItem"), 2)
        .function(remove_item, js_string!("removeItem"), 1)
        .function(clear, js_string!("clear"), 0)
        .function(key_fn, js_string!("key"), 1)
        .property(js_string!("length"), 0, Attribute::all())
        .build();

    let _ = global.set(JsString::from(name), storage, false, context);
}
