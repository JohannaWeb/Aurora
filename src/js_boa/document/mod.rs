use super::*;

mod events;
mod factories;
mod fields;
mod implementation;
mod properties;
mod queries;

use events::*;
use factories::*;
use fields::*;
use implementation::*;
use properties::*;
use queries::*;

pub(in crate::js_boa) fn install_document(
    context: &mut Context,
    document: &NodePtr,
    registry: &NodeRegistry,
) {
    let doc_cap = DocCapture {
        document: document.clone(),
        registry: registry.clone(),
    };

    let mut init = ObjectInitializer::new(context);
    add_document_properties(&mut init);
    add_document_query_methods(&mut init, &doc_cap);
    add_document_factory_methods(&mut init, &doc_cap);
    add_document_event_methods(&mut init);
    let document_obj = init.build();

    install_document_fields(&document_obj, document, registry, context);
    let implementation = build_document_implementation(document, registry, context);
    let _ = document_obj.set(js_string!("implementation"), implementation, false, context);
    set_object_prototype_from_constructor(&document_obj, "Document", context);
    let _ =
        context.register_global_property(js_string!("document"), document_obj, Attribute::all());
}
