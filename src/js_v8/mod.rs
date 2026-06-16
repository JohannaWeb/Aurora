//! V8 engine backend (via the `v8` crate, denoland/rusty_v8 bindings).
//!
//! Proof-of-concept backend: it executes real JavaScript in a V8 isolate
//! behind the same `JsRuntime` trait as SpiderMonkey and Boa, so the runner
//! can hot-swap engines through `js_engine::create_runtime`. No DOM bridge is
//! wired up yet — scripts run in a bare global scope.

mod capture;
mod mutation_observer;
mod node_create;
mod registry;
mod runtime;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod css_tests;
mod selectors;
mod style_class;
mod tree;

pub(crate) use runtime::V8Runtime;
