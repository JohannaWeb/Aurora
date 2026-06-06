mod capture;
mod document;
mod engine;
mod globals;
mod registry;
mod runtime;
mod serialization;
mod state;
mod utils;

pub use runtime::SmRuntime;
pub(crate) use serialization::serialize_outer_html;
