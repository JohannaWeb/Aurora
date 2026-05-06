use super::*;

pub(in crate::js_boa) fn install_document_fields(
    document_obj: &JsObject,
    document: &NodePtr,
    registry: &NodeRegistry,
    context: &mut Context,
) {
    // Set dynamic document fields: documentElement/body/head/forms/links/images.
    let html_root = find_by_tag(document, "html")
        .or_else(|| Some(document.clone()))
        .unwrap();
    let body = find_by_tag(document, "body").unwrap_or_else(|| html_root.clone());
    let head = find_by_tag(document, "head").unwrap_or_else(|| html_root.clone());

    let body_js = create_js_node(body.clone(), registry, document, context);
    let head_js = create_js_node(head.clone(), registry, document, context);
    let root_js = create_js_node(html_root.clone(), registry, document, context);

    let _ = document_obj.set(js_string!("body"), body_js, false, context);
    let _ = document_obj.set(js_string!("head"), head_js, false, context);
    let _ = document_obj.set(js_string!("documentElement"), root_js, false, context);
    let _ = document_obj.set(
        js_string!("scrollingElement"),
        JsValue::null(),
        false,
        context,
    );
    let _ = document_obj.set(js_string!("activeElement"), JsValue::null(), false, context);
    let _ = document_obj.set(
        js_string!("defaultView"),
        context.global_object().clone(),
        false,
        context,
    );

    let mut forms_vec = Vec::new();
    collect_by_tag(document, "form", &mut forms_vec);
    if let Ok(arr) = build_nodelist(forms_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("forms"), arr, false, context);
    }
    let mut links_vec = Vec::new();
    collect_by_tag(document, "a", &mut links_vec);
    if let Ok(arr) = build_nodelist(links_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("links"), arr, false, context);
    }
    let mut images_vec = Vec::new();
    collect_by_tag(document, "img", &mut images_vec);
    if let Ok(arr) = build_nodelist(images_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("images"), arr, false, context);
    }
    let mut scripts_vec = Vec::new();
    collect_by_tag(document, "script", &mut scripts_vec);
    if let Ok(arr) = build_nodelist(scripts_vec, registry, document, context) {
        let _ = document_obj.set(js_string!("scripts"), arr, false, context);
    }

    // title mirror
    if let Some(title_node) = find_by_tag(document, "title") {
        let text = collect_text(&title_node);
        let _ = document_obj.set(js_string!("title"), JsString::from(text), false, context);
    }
}
