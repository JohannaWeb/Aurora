use super::*;

pub(in crate::js_boa) fn build_style_object(cap: NodeCapture, context: &mut Context) -> JsObject {
    #[derive(Clone)]
    struct StyleCap {
        style: Rc<RefCell<BTreeMap<String, String>>>,
    }
    unsafe impl Trace for StyleCap {
        empty_trace!();
    }
    impl Finalize for StyleCap {}

    let style = Rc::new(RefCell::new(BTreeMap::<String, String>::new()));
    // Seed style map from the element's style attribute if present.
    {
        let b = cap.node.borrow();
        if let Node::Element(el) = &*b {
            if let Some(css) = el.attributes.get("style") {
                for part in css.split(';') {
                    if let Some((k, v)) = part.split_once(':') {
                        style
                            .borrow_mut()
                            .insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
        }
    }

    let scap = StyleCap {
        style: style.clone(),
    };

    let get_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            match cap.style.borrow().get(&k) {
                Some(v) => Ok(JsValue::from(JsString::from(v.clone()))),
                None => Ok(JsValue::from(js_string!(""))),
            }
        },
        scap.clone(),
    );
    let set_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let v = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            cap.style.borrow_mut().insert(k, v);
            Ok(JsValue::undefined())
        },
        scap.clone(),
    );
    let remove_prop = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let k = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            cap.style.borrow_mut().remove(&k);
            Ok(JsValue::undefined())
        },
        scap.clone(),
    );
    let item_fn = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &StyleCap, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let m = cap.style.borrow();
            match m.keys().nth(idx) {
                Some(k) => Ok(JsValue::from(JsString::from(k.clone()))),
                None => Ok(JsValue::from(js_string!(""))),
            }
        },
        scap.clone(),
    );

    let obj = ObjectInitializer::new(context)
        .function(get_prop, js_string!("getPropertyValue"), 1)
        .function(set_prop, js_string!("setProperty"), 2)
        .function(remove_prop, js_string!("removeProperty"), 1)
        .function(item_fn, js_string!("item"), 1)
        .property(js_string!("cssText"), js_string!(""), Attribute::all())
        .property(js_string!("length"), 0, Attribute::all())
        .build();

    obj
}
