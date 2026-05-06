use super::*;

mod browser_api;
mod core;
mod location;
mod navigator;
mod platform;
mod timers;

use browser_api::*;
use core::*;
use location::*;
use navigator::*;
use platform::*;
use timers::*;

pub(in crate::js_boa) fn install_globals(
    context: &mut Context,
    document: &NodePtr,
    _registry: &NodeRegistry,
) {
    let global_obj = context.global_object().clone();
    install_window_core(context, &global_obj);
    let win_cap = install_timers(context, &global_obj);
    install_browser_apis(context, &global_obj, &win_cap);
    install_location(context);
    install_navigator(context);
    install_platform_objects(context, &global_obj);
    let _ = document;
}
