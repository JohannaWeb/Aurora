use super::*;

mod family;
mod identity;
mod objects;
mod text_html;

use family::*;
use identity::*;
use objects::*;
use text_html::*;

pub(in crate::js_boa) fn install_accessors(
    obj: &JsObject,
    cap: &NodeCapture,
    context: &mut Context,
) {
    install_text_html_accessors(obj, cap, context);
    install_identity_accessors(obj, cap, context);
    install_family_accessors(obj, cap, context);
    install_object_accessors(obj, cap, context);
}
