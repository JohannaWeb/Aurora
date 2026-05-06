use super::*;

pub(in crate::js_boa) fn build_classlist_object(
    cap: NodeCapture,
    context: &mut Context,
) -> JsObject {
    let add = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            for a in args {
                let cls = js_string_of(a);
                classlist_modify(&cap.node, |set| {
                    set.insert(cls.clone());
                });
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    let remove = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            for a in args {
                let cls = js_string_of(a);
                classlist_modify(&cap.node, |set| {
                    set.remove(&cls);
                });
            }
            Ok(JsValue::undefined())
        },
        cap.clone(),
    );
    let contains = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    return Ok(JsValue::from(v.split_whitespace().any(|c| c == cls)));
                }
            }
            Ok(JsValue::from(false))
        },
        cap.clone(),
    );
    let toggle = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let mut present = false;
            classlist_modify(&cap.node, |set| {
                if set.contains(&cls) {
                    set.remove(&cls);
                } else {
                    set.insert(cls.clone());
                    present = true;
                }
            });
            Ok(JsValue::from(present))
        },
        cap.clone(),
    );
    let replace = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let old_cls = js_string_of(args.get(0).unwrap_or(&JsValue::undefined()));
            let new_cls = js_string_of(args.get(1).unwrap_or(&JsValue::undefined()));
            classlist_modify(&cap.node, |set| {
                if set.remove(&old_cls) {
                    set.insert(new_cls.clone());
                }
            });
            Ok(JsValue::from(true))
        },
        cap.clone(),
    );
    let item = NativeFunction::from_copy_closure_with_captures(
        |_this, args, cap: &NodeCapture, _ctx| {
            let idx = args
                .get(0)
                .and_then(|v| v.as_number())
                .map(|n| n as usize)
                .unwrap_or(0);
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    if let Some(cls) = v.split_whitespace().nth(idx) {
                        return Ok(JsValue::from(JsString::from(cls.to_string())));
                    }
                }
            }
            Ok(JsValue::null())
        },
        cap.clone(),
    );
    let to_string = NativeFunction::from_copy_closure_with_captures(
        |_this, _args, cap: &NodeCapture, _ctx| {
            let b = cap.node.borrow();
            if let Node::Element(el) = &*b {
                if let Some(v) = el.attributes.get("class") {
                    return Ok(JsValue::from(JsString::from(v.clone())));
                }
            }
            Ok(JsValue::from(js_string!("")))
        },
        cap.clone(),
    );

    ObjectInitializer::new(context)
        .function(add, js_string!("add"), 1)
        .function(remove, js_string!("remove"), 1)
        .function(contains, js_string!("contains"), 1)
        .function(toggle, js_string!("toggle"), 1)
        .function(replace, js_string!("replace"), 2)
        .function(item, js_string!("item"), 1)
        .function(to_string, js_string!("toString"), 0)
        .build()
}

pub(in crate::js_boa) fn classlist_modify<F: FnOnce(&mut std::collections::BTreeSet<String>)>(
    node: &NodePtr,
    f: F,
) {
    use std::collections::BTreeSet;
    if let Node::Element(el) = &mut *node.borrow_mut() {
        let mut set: BTreeSet<String> = el
            .attributes
            .get("class")
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        f(&mut set);
        let joined = set.into_iter().collect::<Vec<_>>().join(" ");
        if joined.is_empty() {
            el.attributes.remove("class");
        } else {
            el.attributes.insert("class".to_string(), joined);
        }
    }
}
