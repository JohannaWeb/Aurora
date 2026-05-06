//! Boa JavaScript runtime with an expanded DOM/BOM bridge.

use crate::dom::{ElementNode, Node, NodePtr};

use boa_engine::object::builtins::JsArray;
use boa_engine::object::{FunctionObjectBuilder, ObjectInitializer};
use boa_engine::property::Attribute;
use boa_engine::{
    js_string, Context, JsError, JsObject, JsResult, JsString, JsValue, NativeFunction, Source,
};
use boa_gc::{empty_trace, Finalize, Trace};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

mod accessor_install;
mod accessors;
mod capture;
mod constructors;
mod convert;
mod document;
mod globals;
mod network;
mod node_create;
mod observers;
mod reflection;
mod registry;
mod runtime;
mod selectors;
mod serialization;
mod storage;
mod style_class;
mod tree;
mod utils;

use accessor_install::*;
use accessors::*;
use capture::*;
use constructors::*;
use convert::*;
use document::*;
use globals::*;
use network::*;
use node_create::*;
use observers::*;
use reflection::*;
use registry::*;
use selectors::*;
use serialization::*;
use storage::*;
use style_class::*;
use tree::*;
use utils::*;

pub use runtime::BoaRuntime;
