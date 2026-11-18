//! V8 engine backend (via the `v8` crate, denoland/rusty_v8 bindings).
//!
//! Executes real JavaScript in a V8 isolate behind the `JsRuntime` trait used by
//! the runner.

mod bindings;
#[cfg(test)]
mod bindings_tests;
mod capture;
#[cfg(test)]
mod css_tests;
mod dom_exception;
mod custom_elements;
mod determinism;
mod form_associated;
mod mutation_observer;
mod node_create;
mod performance;
mod registry;
mod runtime;
#[cfg(test)]
mod runtime_tests;
mod selectors;
mod style_class;
mod tree;

pub(crate) use runtime::V8Runtime;
