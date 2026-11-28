//! V8 engine backend (via the `v8` crate, denoland/rusty_v8 bindings).
//!
//! Executes real JavaScript in a V8 isolate behind the `JsRuntime` trait used by
//! the runner.

mod bindings;
#[cfg(test)]
mod bindings_tests;
mod capture;
mod crypto;
#[cfg(test)]
mod css_tests;
mod dom_exception;
mod dom_impl;
mod custom_elements;
mod determinism;
mod form_associated;
mod html_all_collection;
mod image_load;
mod script_load;
mod mutation_observer;
mod node_create;
mod performance;
mod registry;
mod runtime;
#[cfg(test)]
mod runtime_tests;
mod selectors;
mod style_class;
mod templates;
mod tree;

pub(crate) use runtime::V8Runtime;
