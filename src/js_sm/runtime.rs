#![allow(unsafe_op_in_unsafe_fn)]
use std::cell::RefCell;
use std::ptr;
use std::rc::Rc;
use std::time::Instant;

use mozjs::jsapi::{OnNewGlobalHookOption, Value};
use mozjs::jsval::{DoubleValue, UndefinedValue};
use mozjs::realm::AutoRealm;
use mozjs::rooted;
use mozjs::rust::wrappers2::{JS_NewGlobalObject, JS_SetContextPrivate};
use mozjs::rust::{
    CompileOptionsWrapper, RealmOptions, Runtime, SIMPLE_GLOBAL_CLASS, evaluate_script,
};

use crate::css::Stylesheet;
use crate::dom::NodePtr;
use crate::layout::{LayoutTree, ViewportSize};

use super::capture::WindowCapture;
use super::document::install_document;
use super::engine::get_engine_handle;
use super::globals::install_globals;
use super::job_queue::{AuroraJobQueue, install_job_queue, remove_job_queue};
use super::mutation_observer::drain_mutation_observers;
use super::registry::NodeRegistry;
use super::state::SmState;
use super::utils::*;

pub struct SmRuntime {
    rt: Runtime,
    document: NodePtr,
    // Box for stable address used by JS_SetContextPrivate
    state: Box<SmState>,
    // Box for stable address passed as *const c_void to the C++ job queue
    job_queue: Box<AuroraJobQueue>,
    // Opaque JS::JobQueue* installed on the context; freed in Drop
    js_queue: *mut mozjs::jsapi::JS::JobQueue,
    sync_reflow_callback: Option<Box<dyn Fn()>>,
}

impl Drop for SmRuntime {
    fn drop(&mut self) {
        unsafe { remove_job_queue(self.js_queue) };
    }
}

impl SmRuntime {
    pub fn new(document: NodePtr) -> Self {
        let handle = get_engine_handle();
        let mut rt = Runtime::new(handle);

        // Install our custom Promise job queue BEFORE any realm/global work.
        // This replaces the need for UseInternalJobQueues (which crashes post-init).
        let mut job_queue = Box::new(AuroraJobQueue { pending: Vec::new(), total_enqueued: 0 });
        let js_queue = unsafe { install_job_queue(rt.cx(), &mut *job_queue) };

        let state = Box::new(SmState {
            document: document.clone(),
            registry: NodeRegistry::new(),
            window: WindowCapture::new(),
            global: ptr::null_mut(),
            mutation_observers: Vec::new(),
        });

        // We need the state pointer before setting up globals (globals set it)
        // SAFETY: We immediately set it below after creating the global
        let state_ptr = &*state as *const SmState as *mut SmState;

        let global = unsafe {
            let cx = rt.cx();
            let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
            let c_option = RealmOptions::default();

            rooted!(&in(cx) let global = JS_NewGlobalObject(
                cx,
                &SIMPLE_GLOBAL_CLASS,
                ptr::null_mut(),
                h_option,
                &*c_option,
            ));

            // Set state pointer on context so native callbacks can access it
            JS_SetContextPrivate(cx, state_ptr as *mut std::ffi::c_void);

            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();

            // Install all JS globals
            let window_cap = install_globals(realm_ref, global_handle, &document);
            install_document(realm_ref, global_handle, &document);

            // Store the global pointer in state (for callback GC rooting)
            (*state_ptr).global = *global_handle;
            // Replace the WindowCapture with the one returned from install_globals
            (*state_ptr).window = window_cap;
            // Register document for query dispatch
            (*state_ptr).registry.document = Some(document.clone());

            *global
        };

        // Store global raw pointer (we must be careful: it must live as long as rt)
        // The global is kept alive by the realm, which is kept alive by rt.
        unsafe {
            (*state_ptr).global = global;
        }

        SmRuntime {
            rt,
            document,
            state,
            job_queue,
            js_queue,
            sync_reflow_callback: None,
        }
    }

    pub fn execute(&mut self, script: &str) -> Result<(), String> {
        let cx = self.rt.cx();
        let global_raw = self.state.global;
        if global_raw.is_null() {
            return Err("No global object".into());
        }

        let result = unsafe {
            rooted!(&in(cx) let global = global_raw);
            rooted!(&in(cx) let mut rval = UndefinedValue());
            let options = CompileOptionsWrapper::new(cx, "inline", 1);
            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();
            let result = evaluate_script(
                realm_ref,
                global_handle,
                script,
                rval.handle_mut(),
                options,
            );
            // Drain native Promise reactions (jq_enqueue_promise_job → q.pending)
            // and queueMicrotask callbacks in a loop until both queues are empty.
            // Without this loop, Promise chains never resolve during script eval.
            let enqueued_before = self.job_queue.total_enqueued;
            let mut total_microtasks = 0usize;
            for _ in 0..1000 {
                mozjs::rust::wrappers2::RunJobs(realm_ref);
                drain_mutation_observers(realm_ref, &mut self.state);
                let ids: Vec<u32> = self.state.window.microtask_ids.drain(..).collect();
                if ids.is_empty() {
                    break;
                }
                total_microtasks += ids.len();
                for id in ids {
                    call_stored_callback(realm_ref, global_handle, id, &[]);
                    delete_callback(realm_ref, global_handle, id);
                    clear_pending_exception(realm_ref);
                }
                drain_mutation_observers(realm_ref, &mut self.state);
            }
            let total_jobs = self.job_queue.total_enqueued - enqueued_before;
            if total_jobs > 0 || total_microtasks > 0 {
                log::info!("[execute] drained {} promise jobs, {} microtasks", total_jobs, total_microtasks);
            }
            match result {
                Ok(_) => Ok(()),
                Err(()) => {
                    let msg = pending_exception_string(realm_ref);
                    Err(msg)
                }
            }
        };

        result
    }

    pub fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<LayoutTree>>,
        stylesheet: Rc<RefCell<Stylesheet>>,
        viewport: Rc<RefCell<ViewportSize>>,
    ) {
        self.state.registry.set_shared_state(
            layout_tree,
            stylesheet,
            viewport,
            self.document.clone(),
        );
    }

    pub fn clear_dirty_bits(&mut self) {
        self.state.registry.clear_dirty_bits();
    }

    pub fn has_dirty_bits(&self) -> bool {
        self.state.registry.has_dirty_bits()
    }

    pub fn take_needs_reflow(&mut self) -> bool {
        self.state.registry.take_needs_reflow()
    }

    pub fn perform_sync_reflow(&self) {
        self.state.registry.perform_sync_reflow();
    }

    pub fn set_sync_reflow_callback<F>(&mut self, callback: F)
    where
        F: Fn() + 'static,
    {
        self.sync_reflow_callback = Some(Box::new(callback));
    }

    pub fn request_sync_reflow(&self) {
        if let Some(ref cb) = self.sync_reflow_callback {
            cb();
        }
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let ready = self.ready_timers(now);
        if ready.is_empty()
            && self.state.window.microtask_ids.is_empty()
            && self.job_queue.pending.is_empty()
        {
            return false;
        }

        let mut fired = false;
        let global_raw = self.state.global;

        unsafe {
            let cx = self.rt.cx();
            rooted!(&in(cx) let global = global_raw);
            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();

            // Drain microtasks first
            for _ in 0..1000 {
                let ids: Vec<u32> = self.state.window.microtask_ids.drain(..).collect();
                if ids.is_empty() {
                    break;
                }
                fired = true;
                for id in ids {
                    call_stored_callback(realm_ref, global_handle, id, &[]);
                    delete_callback(realm_ref, global_handle, id);
                    clear_pending_exception(realm_ref);
                }
                mozjs::rust::wrappers2::RunJobs(realm_ref);
                drain_mutation_observers(realm_ref, &mut self.state);
            }

            // Fire ready timers
            for entry in &ready {
                let id = entry.id;
                call_stored_callback(realm_ref, global_handle, id, &[]);
                clear_pending_exception(realm_ref);
                // Drain Promise reactions queued by this timer callback.
                mozjs::rust::wrappers2::RunJobs(realm_ref);
                drain_mutation_observers(realm_ref, &mut self.state);
                fired = true;

                if entry.interval.is_none() {
                    delete_callback(realm_ref, global_handle, id);
                }
            }
        }

        // Re-register interval timers with new deadlines
        for entry in ready {
            if let Some(interval) = entry.interval {
                self.state
                    .window
                    .timers
                    .push(crate::js_sm::capture::TimerEntry {
                        id: entry.id,
                        deadline: now + interval,
                        interval: Some(interval),
                    });
            }
        }

        fired && self.state.registry.has_dirty_bits()
    }

    pub fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool {
        let frames: Vec<_> = self.state.window.animation_frames.drain(..).collect();
        if frames.is_empty() {
            return false;
        }

        let timestamp = now
            .duration_since(self.state.window.time_origin)
            .as_secs_f64()
            * 1000.0;
        let ts_val = DoubleValue(timestamp);
        let global_raw = self.state.global;

        unsafe {
            let cx = self.rt.cx();
            rooted!(&in(cx) let global = global_raw);
            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();
            for entry in &frames {
                call_stored_callback(realm_ref, global_handle, entry.id, &[ts_val]);
                delete_callback(realm_ref, global_handle, entry.id);
                clear_pending_exception(realm_ref);
            }
            // Drain Promise reactions queued by rAF callbacks.
            mozjs::rust::wrappers2::RunJobs(realm_ref);
            drain_mutation_observers(realm_ref, &mut self.state);
        }

        self.state.registry.has_dirty_bits()
    }

    pub fn dispatch_event(&mut self, node: &NodePtr, event_type: &str) -> bool {
        let node_id = match self.state.registry.node_id(node) {
            Some(id) => id,
            None => return false,
        };

        let listener_ids = self.state.registry.get_listener_ids(node_id, event_type);
        let doc_listener_ids = self.state.registry.get_listener_ids(0, event_type);

        let all_ids: Vec<u32> = listener_ids.into_iter().chain(doc_listener_ids).collect();
        if all_ids.is_empty() {
            return false;
        }

        let global_raw = self.state.global;
        unsafe {
            let cx = self.rt.cx();
            rooted!(&in(cx) let global = global_raw);
            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();

            let event_obj = make_event_object(realm_ref, event_type);
            let event_val = mozjs::jsval::ObjectValue(event_obj);

            for id in &all_ids {
                call_stored_callback(realm_ref, global_handle, *id, &[event_val]);
                clear_pending_exception(realm_ref);
            }
            mozjs::rust::wrappers2::RunJobs(realm_ref);
            drain_mutation_observers(realm_ref, &mut self.state);
        }

        true
    }

    pub fn fire_dom_content_loaded(&mut self) {
        self.fire_lifecycle_event("DOMContentLoaded");
    }

    pub fn fire_load(&mut self) {
        self.fire_lifecycle_event("load");
    }

    fn fire_lifecycle_event(&mut self, event_type: &str) {
        let ids = self.state.registry.get_listener_ids(0, event_type);
        if ids.is_empty() {
            return;
        }

        let global_raw = self.state.global;
        unsafe {
            let cx = self.rt.cx();
            rooted!(&in(cx) let global = global_raw);
            let mut realm = AutoRealm::new_from_handle(cx, global.handle());
            let (global_handle, realm_ref) = realm.global_and_reborrow();
            let event_obj = make_event_object(realm_ref, event_type);
            let event_val = mozjs::jsval::ObjectValue(event_obj);
            for id in &ids {
                call_stored_callback(realm_ref, global_handle, *id, &[event_val]);
                clear_pending_exception(realm_ref);
            }
            mozjs::rust::wrappers2::RunJobs(realm_ref);
            drain_mutation_observers(realm_ref, &mut self.state);
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.state.window.timers.iter().map(|t| t.deadline).min()
    }

    pub fn has_animation_frame_callbacks(&self) -> bool {
        !self.state.window.animation_frames.is_empty()
    }

    pub fn has_ready_work(&self, now: Instant) -> bool {
        self.has_animation_frame_callbacks()
            || !self.state.window.microtask_ids.is_empty()
            || !self.job_queue.pending.is_empty()
            || self.next_deadline().map(|d| d <= now).unwrap_or(false)
    }

    // ── Private ────────────────────────────────────────────────────────────────

    fn ready_timers(&mut self, now: Instant) -> Vec<crate::js_sm::capture::TimerEntry> {
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        for entry in self.state.window.timers.drain(..) {
            if entry.deadline <= now && ready.len() < 100 {
                ready.push(entry.clone());
                if let Some(_interval) = entry.interval {
                    // Interval timers are re-registered after firing
                }
            } else {
                pending.push(entry);
            }
        }
        self.state.window.timers = pending;
        ready
    }
}

impl crate::js_engine::JsRuntime for SmRuntime {
    fn execute(&mut self, script: &str) -> Result<(), String> {
        SmRuntime::execute(self, script)
    }

    fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<crate::layout::LayoutTree>>,
        stylesheet: Rc<RefCell<crate::css::Stylesheet>>,
        viewport: Rc<RefCell<crate::layout::ViewportSize>>,
    ) {
        SmRuntime::set_shared_state(self, layout_tree, stylesheet, viewport)
    }

    fn clear_dirty_bits(&mut self) {
        SmRuntime::clear_dirty_bits(self)
    }
    fn has_dirty_bits(&self) -> bool {
        SmRuntime::has_dirty_bits(self)
    }
    fn take_needs_reflow(&mut self) -> bool {
        SmRuntime::take_needs_reflow(self)
    }

    fn tick(&mut self, now: std::time::Instant) -> bool {
        SmRuntime::tick(self, now)
    }

    fn drain_animation_frame_callbacks(&mut self, now: std::time::Instant) -> bool {
        SmRuntime::drain_animation_frame_callbacks(self, now)
    }

    fn dispatch_event(&mut self, node: &crate::dom::NodePtr, event_type: &str) -> bool {
        SmRuntime::dispatch_event(self, node, event_type)
    }

    fn fire_dom_content_loaded(&mut self) {
        SmRuntime::fire_dom_content_loaded(self)
    }

    fn fire_load(&mut self) {
        SmRuntime::fire_load(self)
    }

    fn next_deadline(&self) -> Option<std::time::Instant> {
        SmRuntime::next_deadline(self)
    }

    fn has_animation_frame_callbacks(&self) -> bool {
        SmRuntime::has_animation_frame_callbacks(self)
    }

    fn has_ready_work(&self, now: std::time::Instant) -> bool {
        SmRuntime::has_ready_work(self, now)
    }
}

unsafe fn make_event_object(
    cx: &mut mozjs::context::JSContext,
    event_type: &str,
) -> *mut mozjs::jsapi::JSObject {
    let obj = new_plain_object(cx);
    rooted!(&in(cx) let obj_root = obj);
    set_prop_str(cx, obj_root.handle(), c"type", event_type);
    set_prop_bool(cx, obj_root.handle(), c"bubbles", true);
    set_prop_bool(cx, obj_root.handle(), c"cancelable", true);
    set_prop_bool(cx, obj_root.handle(), c"defaultPrevented", false);
    set_prop_bool(cx, obj_root.handle(), c"isTrusted", true);
    define_fn(
        cx,
        obj_root.handle(),
        c"preventDefault",
        Some(noop_native),
        0,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"stopPropagation",
        Some(noop_native),
        0,
    );
    define_fn(
        cx,
        obj_root.handle(),
        c"stopImmediatePropagation",
        Some(noop_native),
        0,
    );
    obj
}

unsafe extern "C" fn noop_native(
    _cx: *mut mozjs::jsapi::JSContext,
    _argc: u32,
    vp: *mut Value,
) -> bool {
    let args = mozjs::jsapi::CallArgs::from_vp(vp, _argc);
    args.rval().set(UndefinedValue());
    true
}
