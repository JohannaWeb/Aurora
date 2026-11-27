use std::rc::Rc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use super::dom_exception::throw_dom_exception;
use super::node_create::v8_str;
use super::registry::NodeRegistry;

/// Backing state for `window.performance` (`now()`, `timeOrigin`, User Timing).
pub(super) struct PerformanceState {
    start_instant: Instant,
    time_origin_ms: f64,
    entries: Vec<StoredEntry>,
}

struct StoredEntry {
    name: String,
    entry_type: &'static str,
    start_time: f64,
    duration: f64,
    detail: Option<v8::Global<v8::Value>>,
}

impl PerformanceState {
    pub(super) fn new() -> Self {
        let time_origin_ms = crate::fetch::replay::start_time_ms().unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        });
        Self {
            start_instant: Instant::now(),
            time_origin_ms,
            entries: Vec::new(),
        }
    }

    fn now_ms(&self) -> f64 {
        if crate::fetch::replay::start_time_ms().is_some() {
            crate::clock::script_elapsed_ms()
        } else {
            self.start_instant.elapsed().as_secs_f64() * 1000.0
        }
    }

    fn latest_mark_time(&self, name: &str) -> Option<f64> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.entry_type == "mark" && e.name == name)
            .map(|e| e.start_time)
    }

    fn sorted_entries(&self, filter_type: Option<&str>, filter_name: Option<&str>) -> Vec<usize> {
        let mut indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                filter_type.is_none_or(|t| e.entry_type == t)
                    && filter_name.is_none_or(|n| e.name == n)
            })
            .map(|(i, _)| i)
            .collect();
        indices.sort_by(|&a, &b| {
            self.entries[a]
                .start_time
                .total_cmp(&self.entries[b].start_time)
                .then_with(|| a.cmp(&b))
        });
        indices
    }
}

/// Installs `Performance`, `PerformanceEntry`, `PerformanceMark`,
/// `PerformanceMeasure`, and `globalThis.performance`.
pub(super) fn install<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    global: v8::Local<'s, v8::Object>,
    registry_data: v8::Local<'s, v8::External>,
) {
    let entry_ctor = build_illegal_ctor(scope, "PerformanceEntry");
    let mark_ctor = v8::FunctionTemplate::builder(construct_mark)
        .data(registry_data.into())
        .build(scope);
    mark_ctor.set_class_name(v8_str(scope, "PerformanceMark"));
    mark_ctor.inherit(entry_ctor);
    let measure_ctor = build_illegal_ctor(scope, "PerformanceMeasure");
    measure_ctor.inherit(entry_ctor);

    let perf_entry_fn = entry_ctor
        .get_function(scope)
        .expect("PerformanceEntry ctor");
    let perf_mark_fn = mark_ctor
        .get_function(scope)
        .expect("PerformanceMark ctor");
    let perf_measure_fn = measure_ctor
        .get_function(scope)
        .expect("PerformanceMeasure ctor");

    global.set(
        scope,
        v8_str(scope, "PerformanceEntry").into(),
        perf_entry_fn.into(),
    );
    global.set(
        scope,
        v8_str(scope, "PerformanceMark").into(),
        perf_mark_fn.into(),
    );
    global.set(
        scope,
        v8_str(scope, "PerformanceMeasure").into(),
        perf_measure_fn.into(),
    );

    let perf_ctor = build_illegal_ctor(scope, "Performance");
    let proto = perf_ctor.prototype_template(scope);
    install_method(scope, proto, "now", perf_now, registry_data);
    install_method(scope, proto, "mark", perf_mark, registry_data);
    install_method(scope, proto, "measure", perf_measure, registry_data);
    install_method(scope, proto, "clearMarks", perf_clear_marks, registry_data);
    install_method(
        scope,
        proto,
        "clearMeasures",
        perf_clear_measures,
        registry_data,
    );
    install_method(scope, proto, "getEntries", perf_get_entries, registry_data);
    install_method(
        scope,
        proto,
        "getEntriesByType",
        perf_get_entries_by_type,
        registry_data,
    );
    install_method(
        scope,
        proto,
        "getEntriesByName",
        perf_get_entries_by_name,
        registry_data,
    );
    install_method(scope, proto, "toJSON", perf_to_json, registry_data);
    proto.set_accessor_with_configuration(
        v8_str(scope, "timeOrigin").into(),
        v8::AccessorConfiguration::new(perf_time_origin).data(registry_data.into()),
    );

    let perf_ctor_fn = perf_ctor.get_function(scope).expect("Performance ctor");
    global.set(
        scope,
        v8_str(scope, "Performance").into(),
        perf_ctor_fn.into(),
    );

    let inst = perf_ctor
        .instance_template(scope)
        .new_instance(scope)
        .expect("performance instance");
    let perf_proto = perf_ctor_fn
        .get(scope, v8_str(scope, "prototype").into())
        .expect("Performance.prototype");
    let _ = inst.set_prototype(scope, perf_proto);

    let registry = registry_from(registry_data.into());
    let timing = build_timing_object(scope, registry.performance.borrow().time_origin_ms);
    let navigation = build_navigation_object(scope);
    inst.set(scope, v8_str(scope, "timing").into(), timing.into());
    inst.set(scope, v8_str(scope, "navigation").into(), navigation.into());
    global.set(scope, v8_str(scope, "performance").into(), inst.into());
}

fn build_illegal_ctor<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
) -> v8::Local<'s, v8::FunctionTemplate> {
    let t = v8::FunctionTemplate::new(scope, illegal_constructor);
    t.set_class_name(v8_str(scope, name));
    t
}

fn illegal_constructor(
    scope: &mut v8::PinScope<'_, '_>,
    _args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    let msg = v8_str(scope, "Illegal constructor");
    let err = v8::Exception::type_error(scope, msg);
    scope.throw_exception(err);
}

fn install_method<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    proto: v8::Local<'s, v8::ObjectTemplate>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    data: v8::Local<'s, v8::External>,
) {
    let fn_t = v8::FunctionTemplate::builder(callback)
        .data(data.into())
        .build(scope);
    proto.set(v8_str(scope, name).into(), fn_t.into());
}

fn registry_from<'a>(data: v8::Local<'a, v8::Value>) -> &'a Rc<NodeRegistry> {
    let external = v8::Local::<v8::External>::try_from(data)
        .expect("performance callback data is always Registry external");
    unsafe { &*(external.value() as *const Rc<NodeRegistry>) }
}

fn build_timing_object<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    origin_ms: f64,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    let base = origin_ms.floor();
    let val = v8::Number::new(scope, base);
    for key in [
        "navigationStart",
        "fetchStart",
        "domainLookupStart",
        "domainLookupEnd",
        "connectStart",
        "connectEnd",
        "requestStart",
        "responseStart",
        "responseEnd",
        "domLoading",
        "domInteractive",
        "domContentLoadedEventStart",
        "domContentLoadedEventEnd",
        "domComplete",
        "loadEventStart",
        "loadEventEnd",
    ] {
        obj.set(scope, v8_str(scope, key).into(), val.into());
    }
    obj
}

fn build_navigation_object<'s>(scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    let zero = v8::Integer::new(scope, 0);
    obj.set(scope, v8_str(scope, "type").into(), zero.into());
    obj.set(scope, v8_str(scope, "redirectCount").into(), zero.into());
    obj
}

fn entry_to_v8<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    entry: &StoredEntry,
) -> v8::Local<'s, v8::Object> {
    let obj = v8::Object::new(scope);
    let ctor_name = match entry.entry_type {
        "mark" => "PerformanceMark",
        "measure" => "PerformanceMeasure",
        _ => "PerformanceEntry",
    };
    let global = scope.get_current_context().global(scope);
    if let Some(ctor) = global
        .get(scope, v8_str(scope, ctor_name).into())
        .and_then(|v| v.to_object(scope))
        && let Some(proto) = ctor.get(scope, v8_str(scope, "prototype").into())
    {
        let _ = obj.set_prototype(scope, proto);
    }
    obj.set(
        scope,
        v8_str(scope, "name").into(),
        v8_str(scope, &entry.name).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "entryType").into(),
        v8_str(scope, entry.entry_type).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "startTime").into(),
        v8::Number::new(scope, entry.start_time).into(),
    );
    obj.set(
        scope,
        v8_str(scope, "duration").into(),
        v8::Number::new(scope, entry.duration).into(),
    );
    if entry.entry_type == "mark" || entry.entry_type == "measure" {
        let detail = match &entry.detail {
            Some(global_val) => v8::Local::new(scope, global_val),
            None => v8::null(scope).into(),
        };
        obj.set(scope, v8_str(scope, "detail").into(), detail);
    }
    obj
}

fn perf_now(
    _scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    retval.set_double(registry.performance.borrow().now_ms());
}

fn perf_time_origin(
    scope: &mut v8::PinScope<'_, '_>,
    _name: v8::Local<v8::Name>,
    args: v8::PropertyCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    let val = v8::Number::new(scope, registry.performance.borrow().time_origin_ms);
    retval.set(val.into());
}

fn construct_mark(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if !args.is_construct_call() {
        let msg = v8_str(
            scope,
            "Failed to construct 'PerformanceMark': Please use the 'new' operator.",
        );
        let err = v8::Exception::type_error(scope, msg);
        scope.throw_exception(err);
        return;
    }
    let registry = registry_from(args.data());
    if let Some(entry) = parse_mark_entry(scope, &args, &registry.performance.borrow()) {
        retval.set(entry_to_v8(scope, &entry).into());
    }
}

fn parse_mark_entry(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
    perf: &PerformanceState,
) -> Option<StoredEntry> {
    if args.length() < 1 {
        let msg = v8_str(
            scope,
            "Failed to execute 'mark' on 'Performance': 1 argument required, but only 0 present.",
        );
        let err = v8::Exception::type_error(scope, msg);
        scope.throw_exception(err);
        return None;
    }
    let name = args.get(0).to_rust_string_lossy(scope);
    let mut start_time = perf.now_ms();
    let mut detail = None;
    let opts = args.get(1);
    if !opts.is_undefined()
        && !opts.is_null()
        && let Some(obj) = opts.to_object(scope)
    {
        if let Some(st) = obj.get(scope, v8_str(scope, "startTime").into())
            && !st.is_undefined()
        {
            let Some(n) = st.number_value(scope) else {
                let msg = v8_str(scope, "Invalid startTime on PerformanceMark");
                let err = v8::Exception::type_error(scope, msg);
                scope.throw_exception(err);
                return None;
            };
            if n < 0.0 || !n.is_finite() {
                let msg = v8_str(scope, "mark startTime cannot be negative");
                let err = v8::Exception::type_error(scope, msg);
                scope.throw_exception(err);
                return None;
            }
            start_time = n;
        }
        if let Some(d) = obj.get(scope, v8_str(scope, "detail").into())
            && !d.is_undefined()
        {
            detail = Some(v8::Global::new(scope, d));
        }
    }
    Some(StoredEntry {
        name,
        entry_type: "mark",
        start_time,
        duration: 0.0,
        detail,
    })
}

fn perf_mark(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    let entry = {
        let perf = registry.performance.borrow();
        let Some(entry) = parse_mark_entry(scope, &args, &perf) else {
            return;
        };
        entry
    };
    let obj = entry_to_v8(scope, &entry);
    registry.performance.borrow_mut().entries.push(entry);
    retval.set(obj.into());
}

fn resolve_timestamp(
    scope: &mut v8::PinScope<'_, '_>,
    perf: &PerformanceState,
    val: v8::Local<v8::Value>,
) -> Option<f64> {
    if val.is_number() {
        let n = val.number_value(scope).unwrap_or(f64::NAN);
        if n < 0.0 || !n.is_finite() {
            let msg = v8_str(scope, "Timestamp cannot be negative");
            let err = v8::Exception::type_error(scope, msg);
            scope.throw_exception(err);
            return None;
        }
        return Some(n);
    }
    let mark_name = val.to_rust_string_lossy(scope);
    if let Some(t) = perf.latest_mark_time(&mark_name) {
        return Some(t);
    }
    throw_dom_exception(
        scope,
        "SyntaxError",
        &format!("The mark '{mark_name}' does not exist."),
    );
    None
}

fn perf_measure(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    if args.length() < 1 {
        let msg = v8_str(
            scope,
            "Failed to execute 'measure' on 'Performance': 1 argument required, but only 0 present.",
        );
        let err = v8::Exception::type_error(scope, msg);
        scope.throw_exception(err);
        return;
    }
    let registry = registry_from(args.data());
    let name = args.get(0).to_rust_string_lossy(scope);
    let arg1 = args.get(1);
    let arg2 = args.get(2);

    let entry = {
        let perf = registry.performance.borrow();
        let now = perf.now_ms();
        let mut detail = None;
        let (start_time, duration) = if arg1.is_object() && !arg1.is_null() {
            let obj = arg1.to_object(scope).unwrap();
            if let Some(d) = obj.get(scope, v8_str(scope, "detail").into())
                && !d.is_undefined()
            {
                detail = Some(v8::Global::new(scope, d));
            }
            let start_val = obj
                .get(scope, v8_str(scope, "start").into())
                .filter(|v| !v.is_undefined());
            let end_val = obj
                .get(scope, v8_str(scope, "end").into())
                .filter(|v| !v.is_undefined());
            let dur_val = obj
                .get(scope, v8_str(scope, "duration").into())
                .filter(|v| !v.is_undefined());
            let duration_opt = dur_val.map(|v| v.number_value(scope).unwrap_or(0.0));
            match (start_val, end_val, duration_opt) {
                (Some(s), Some(e), _) => {
                    let Some(st) = resolve_timestamp(scope, &perf, s) else {
                        return;
                    };
                    let Some(et) = resolve_timestamp(scope, &perf, e) else {
                        return;
                    };
                    (st, et - st)
                }
                (Some(s), None, Some(d)) => {
                    let Some(st) = resolve_timestamp(scope, &perf, s) else {
                        return;
                    };
                    (st, d)
                }
                (None, Some(e), Some(d)) => {
                    let Some(et) = resolve_timestamp(scope, &perf, e) else {
                        return;
                    };
                    (et - d, d)
                }
                (Some(s), None, None) => {
                    let Some(st) = resolve_timestamp(scope, &perf, s) else {
                        return;
                    };
                    (st, now - st)
                }
                (None, Some(e), None) => {
                    let Some(et) = resolve_timestamp(scope, &perf, e) else {
                        return;
                    };
                    (0.0, et)
                }
                (None, None, _) => {
                    let msg = v8_str(
                        scope,
                        "measure options require at least start or end",
                    );
                    let err = v8::Exception::type_error(scope, msg);
                    scope.throw_exception(err);
                    return;
                }
            }
        } else {
            let start_time = if arg1.is_undefined() || arg1.is_null() {
                0.0
            } else {
                let Some(st) = resolve_timestamp(scope, &perf, arg1) else {
                    return;
                };
                st
            };
            let end_time = if arg2.is_undefined() || arg2.is_null() {
                now
            } else {
                let Some(et) = resolve_timestamp(scope, &perf, arg2) else {
                    return;
                };
                et
            };
            (start_time, end_time - start_time)
        };
        StoredEntry {
            name,
            entry_type: "measure",
            start_time,
            duration,
            detail,
        }
    };

    let obj = entry_to_v8(scope, &entry);
    registry.performance.borrow_mut().entries.push(entry);
    retval.set(obj.into());
}

fn clear_by_type(
    scope: &mut v8::PinScope<'_, '_>,
    args: &v8::FunctionCallbackArguments,
    entry_type: &str,
) {
    let registry = registry_from(args.data());
    let name_arg = args.get(0);
    let filter_name = if name_arg.is_undefined() {
        None
    } else {
        Some(name_arg.to_rust_string_lossy(scope))
    };
    registry.performance.borrow_mut().entries.retain(|e| {
        e.entry_type != entry_type || filter_name.as_deref().is_some_and(|n| e.name != n)
    });
}

fn perf_clear_marks(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    clear_by_type(scope, &args, "mark");
}

fn perf_clear_measures(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    _retval: v8::ReturnValue,
) {
    clear_by_type(scope, &args, "measure");
}

fn return_entries<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    perf: &PerformanceState,
    filter_type: Option<&str>,
    filter_name: Option<&str>,
    mut retval: v8::ReturnValue,
) {
    let indices = perf.sorted_entries(filter_type, filter_name);
    let items: Vec<v8::Local<v8::Value>> = indices
        .into_iter()
        .map(|i| entry_to_v8(scope, &perf.entries[i]).into())
        .collect();
    retval.set(v8::Array::new_with_elements(scope, &items).into());
}

fn perf_get_entries(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    return_entries(scope, &registry.performance.borrow(), None, None, retval);
}

fn perf_get_entries_by_type(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    let ty = args.get(0).to_rust_string_lossy(scope);
    return_entries(
        scope,
        &registry.performance.borrow(),
        Some(&ty),
        None,
        retval,
    );
}

fn perf_get_entries_by_name(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    let name = args.get(0).to_rust_string_lossy(scope);
    let type_arg = args.get(1);
    let ty = if type_arg.is_undefined() {
        None
    } else {
        Some(type_arg.to_rust_string_lossy(scope))
    };
    return_entries(
        scope,
        &registry.performance.borrow(),
        ty.as_deref(),
        Some(&name),
        retval,
    );
}

fn perf_to_json(
    scope: &mut v8::PinScope<'_, '_>,
    args: v8::FunctionCallbackArguments,
    mut retval: v8::ReturnValue,
) {
    let registry = registry_from(args.data());
    let origin_ms = registry.performance.borrow().time_origin_ms;
    let origin_val = v8::Number::new(scope, origin_ms);
    let timing = build_timing_object(scope, origin_ms);
    let navigation = build_navigation_object(scope);
    let obj = v8::Object::new(scope);
    obj.set(scope, v8_str(scope, "timeOrigin").into(), origin_val.into());
    obj.set(scope, v8_str(scope, "timing").into(), timing.into());
    obj.set(scope, v8_str(scope, "navigation").into(), navigation.into());
    retval.set(obj.into());
}
