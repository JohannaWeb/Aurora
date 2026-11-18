//! `window.performance`: High Resolution Time (`now`, `timeOrigin`), User
//! Timing (`mark`, `measure`) and the entry accessors of Performance
//! Timeline, plus the legacy `performance.timing`.
//!
//! Under replay the clock is the virtual event-loop clock, as for `Date`, so
//! a replayed page reads the same times on every run. Aurora records no
//! resource, navigation or paint entries yet; those types are always empty.
//! <https://w3c.github.io/hr-time/>, <https://w3c.github.io/user-timing/>

use super::dom_exception::throw_dom_exception;
use super::node_create::v8_str;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Entry {
    name: String,
    entry_type: &'static str,
    start_time: f64,
    duration: f64,
}

struct State {
    started: Instant,
    entries: RefCell<Vec<Entry>>,
}

impl State {
    fn now(&self) -> f64 {
        if crate::fetch::replay::start_time_ms().is_some() {
            crate::clock::script_elapsed_ms()
        } else {
            self.started.elapsed().as_secs_f64() * 1000.0
        }
    }
}

fn state(scope: &mut v8::PinScope<'_, '_>) -> Rc<State> {
    scope.get_current_context().get_slot::<State>().expect("performance installed")
}

/// Legacy PerformanceTiming fields. Aurora does not time its navigation
/// phases, so every timestamp reports the time origin.
const TIMING_FIELDS: &[&str] = &[
    "navigationStart", "unloadEventStart", "unloadEventEnd", "redirectStart", "redirectEnd",
    "fetchStart", "domainLookupStart", "domainLookupEnd", "connectStart", "connectEnd",
    "secureConnectionStart", "requestStart", "responseStart", "responseEnd", "domLoading",
    "domInteractive", "domContentLoadedEventStart", "domContentLoadedEventEnd", "domComplete",
    "loadEventStart", "loadEventEnd",
];

pub(super) fn install(scope: &mut v8::PinScope<'_, '_>) {
    let time_origin = crate::fetch::replay::start_time_ms().unwrap_or_else(|| {
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
    });
    let context = scope.get_current_context();
    context.set_slot(Rc::new(State {
        started: Instant::now(),
        entries: RefCell::new(Vec::new()),
    }));

    let template = v8::FunctionTemplate::new(scope, illegal_constructor);
    template.set_class_name(v8_str(scope, "Performance"));
    let proto = template.prototype_template(scope);
    let methods: [(&str, v8::FunctionCallback); 9] = [
        ("now", v8::MapFnTo::map_fn_to(now)),
        ("mark", v8::MapFnTo::map_fn_to(mark)),
        ("measure", v8::MapFnTo::map_fn_to(measure)),
        ("getEntries", v8::MapFnTo::map_fn_to(get_entries)),
        ("getEntriesByType", v8::MapFnTo::map_fn_to(get_entries_by_type)),
        ("getEntriesByName", v8::MapFnTo::map_fn_to(get_entries_by_name)),
        ("clearMarks", v8::MapFnTo::map_fn_to(clear_marks)),
        ("clearMeasures", v8::MapFnTo::map_fn_to(clear_measures)),
        ("clearResourceTimings", v8::MapFnTo::map_fn_to(clear_resource_timings)),
    ];
    for (name, callback) in methods {
        let function = v8::FunctionTemplate::builder_raw(callback)
            .constructor_behavior(v8::ConstructorBehavior::Throw)
            .build(scope);
        proto.set(v8_str(scope, name).into(), function.into());
    }
    proto.set_with_attr(
        v8::Symbol::get_to_string_tag(scope).into(),
        v8_str(scope, "Performance").into(),
        v8::PropertyAttribute::READ_ONLY | v8::PropertyAttribute::DONT_ENUM,
    );
    let global = context.global(scope);
    let constructor = template.get_function(scope).expect("Performance constructor");
    let prototype = constructor
        .get(scope, v8_str(scope, "prototype").into())
        .and_then(|p| p.to_object(scope))
        .expect("Performance.prototype");
    // Performance : EventTarget, so listeners work like on any target.
    if let Some(event_target) = global
        .get(scope, v8_str(scope, "EventTarget").into())
        .and_then(|v| v.to_object(scope))
        .and_then(|c| c.get(scope, v8_str(scope, "prototype").into()))
    {
        prototype.set_prototype(scope, event_target);
    }
    let performance = v8::Object::new(scope);
    performance.set_prototype(scope, prototype.into());
    let origin = v8::Number::new(scope, time_origin);
    performance.set(scope, v8_str(scope, "timeOrigin").into(), origin.into());
    let timing = v8::Object::new(scope);
    let navigation_start = v8::Number::new(scope, time_origin.floor());
    for field in TIMING_FIELDS {
        timing.set(scope, v8_str(scope, field).into(), navigation_start.into());
    }
    performance.set(scope, v8_str(scope, "timing").into(), timing.into());
    let interface = v8::PropertyAttribute::DONT_ENUM;
    global.define_own_property(scope, v8_str(scope, "Performance").into(), constructor.into(), interface);
    let instance = v8::PropertyAttribute::NONE;
    global.define_own_property(scope, v8_str(scope, "performance").into(), performance.into(), instance);
}

fn illegal_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    _: v8::FunctionCallbackArguments,
    _: v8::ReturnValue,
) {
    let message = v8_str(scope, "Illegal constructor");
    let error = v8::Exception::type_error(scope, message);
    scope.throw_exception(error);
}

fn now(scope: &mut v8::PinScope<'_, '_>, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set_double(state(scope).now());
}

fn entry_object<'s>(scope: &mut v8::PinScope<'s, '_>, entry: &Entry) -> v8::Local<'s, v8::Value> {
    let object = v8::Object::new(scope);
    let name = v8_str(scope, &entry.name);
    object.set(scope, v8_str(scope, "name").into(), name.into());
    let entry_type = v8_str(scope, entry.entry_type);
    object.set(scope, v8_str(scope, "entryType").into(), entry_type.into());
    let start_time = v8::Number::new(scope, entry.start_time);
    object.set(scope, v8_str(scope, "startTime").into(), start_time.into());
    let duration = v8::Number::new(scope, entry.duration);
    object.set(scope, v8_str(scope, "duration").into(), duration.into());
    object.into()
}

fn entries_array<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    keep: impl Fn(&Entry) -> bool,
) -> v8::Local<'s, v8::Value> {
    let state = state(scope);
    let entries = state.entries.borrow();
    let mut matching: Vec<&Entry> = entries.iter().filter(|e| keep(e)).collect();
    matching.sort_by(|a, b| a.start_time.total_cmp(&b.start_time));
    let array = v8::Array::new(scope, matching.len() as i32);
    for (index, entry) in matching.into_iter().enumerate() {
        let value = entry_object(scope, entry);
        array.set_index(scope, index as u32, value);
    }
    array.into()
}

/// <https://w3c.github.io/user-timing/#dom-performance-mark>; `startTime` in
/// the options dictionary is honoured, `detail` is not stored.
fn mark(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(name) = args.get(0).to_string(scope).map(|s| s.to_rust_string_lossy(scope)) else {
        return;
    };
    let state = state(scope);
    let mut start_time = state.now();
    if let Some(options) = args.get(1).to_object(scope).filter(|_| args.get(1).is_object())
        && let Some(value) = options.get(scope, v8_str(scope, "startTime").into())
        && !value.is_undefined()
    {
        start_time = value.number_value(scope).unwrap_or(start_time);
    }
    let entry = Entry { name, entry_type: "mark", start_time, duration: 0.0 };
    rv.set(entry_object(scope, &entry));
    state.entries.borrow_mut().push(entry);
}

/// A mark name or timestamp, per "convert a mark to a timestamp".
fn timestamp(scope: &mut v8::PinScope<'_, '_>, state: &State, value: v8::Local<v8::Value>) -> Option<f64> {
    if value.is_number() {
        return value.number_value(scope);
    }
    let name = value.to_string(scope)?.to_rust_string_lossy(scope);
    let found = state
        .entries
        .borrow()
        .iter()
        .rev()
        .find(|e| e.entry_type == "mark" && e.name == name)
        .map(|e| e.start_time);
    if found.is_none() {
        let message = format!("The mark '{name}' does not exist.");
        throw_dom_exception(scope, "SyntaxError", &message);
    }
    found
}

/// <https://w3c.github.io/user-timing/#dom-performance-measure>: start and
/// end come from mark names, timestamps or a `{start, end, duration}`
/// dictionary, defaulting to the time origin and now.
fn measure(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let Some(name) = args.get(0).to_string(scope).map(|s| s.to_rust_string_lossy(scope)) else {
        return;
    };
    let state = state(scope);
    let (mut start, mut end, mut duration) = (None, None, None);
    let first = args.get(1);
    if first.is_object() && !first.is_null() {
        let options = first.to_object(scope).expect("object");
        for (key, slot) in [("start", &mut start), ("end", &mut end), ("duration", &mut duration)] {
            let value = options.get(scope, v8_str(scope, key).into()).filter(|v| !v.is_undefined());
            if let Some(value) = value {
                let Some(time) = (if key == "duration" { value.number_value(scope) } else { timestamp(scope, &state, value) })
                else {
                    return;
                };
                *slot = Some(time);
            }
        }
    } else if !first.is_undefined() {
        let Some(time) = timestamp(scope, &state, first) else { return };
        start = Some(time);
    }
    let last = args.get(2);
    if !last.is_undefined() {
        let Some(time) = timestamp(scope, &state, last) else { return };
        end = Some(time);
    }
    let end_time = match (end, start, duration) {
        (Some(end), _, _) => end,
        (None, Some(start), Some(duration)) => start + duration,
        _ => state.now(),
    };
    let start_time = match (start, duration) {
        (Some(start), _) => start,
        (None, Some(duration)) => end_time - duration,
        _ => 0.0,
    };
    let entry = Entry { name, entry_type: "measure", start_time, duration: end_time - start_time };
    rv.set(entry_object(scope, &entry));
    state.entries.borrow_mut().push(entry);
}

fn get_entries(scope: &mut v8::PinScope<'_, '_>, _: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    rv.set(entries_array(scope, |_| true));
}

fn get_entries_by_type(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(kind) = args.get(0).to_string(scope).map(|s| s.to_rust_string_lossy(scope)) else {
        return;
    };
    rv.set(entries_array(scope, |e| e.entry_type == kind));
}

fn get_entries_by_name(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut rv: v8::ReturnValue,
) {
    let Some(name) = args.get(0).to_string(scope).map(|s| s.to_rust_string_lossy(scope)) else {
        return;
    };
    let kind = if args.get(1).is_undefined() {
        None
    } else {
        let Some(kind) = args.get(1).to_string(scope) else { return };
        Some(kind.to_rust_string_lossy(scope))
    };
    rv.set(entries_array(scope, |e| e.name == name && kind.as_deref().is_none_or(|k| e.entry_type == k)));
}

fn clear(scope: &mut v8::PinScope<'_, '_>, args: &v8::FunctionCallbackArguments, entry_type: &str) {
    let name = if args.get(0).is_undefined() {
        None
    } else {
        let Some(name) = args.get(0).to_string(scope) else { return };
        Some(name.to_rust_string_lossy(scope))
    };
    let state = state(scope);
    state
        .entries
        .borrow_mut()
        .retain(|e| e.entry_type != entry_type || name.as_ref().is_some_and(|n| &e.name != n));
}

fn clear_marks(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    clear(scope, &args, "mark");
}

fn clear_measures(scope: &mut v8::PinScope<'_, '_>, args: v8::FunctionCallbackArguments, _: v8::ReturnValue) {
    clear(scope, &args, "measure");
}

/// No resource entries are recorded, so there is nothing to clear.
fn clear_resource_timings(_: &mut v8::PinScope<'_, '_>, _: v8::FunctionCallbackArguments, _: v8::ReturnValue) {}
