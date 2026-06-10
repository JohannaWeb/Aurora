#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::c_void;
use std::ptr::NonNull;

use mozjs::context::JSContext;
use mozjs::jsapi::JS::{Handle, MutableHandle};
use mozjs::jsapi::JSContext as RawJSContext;
use mozjs::jsapi::JSObject;
use mozjs::rooted;

use super::utils::{
    call_stored_callback, clear_pending_exception, delete_callback, get_state_ptr, store_callback,
};

/// Rust-side job queue for SpiderMonkey's Promise reaction system.
/// Lives in SmRuntime as Box<AuroraJobQueue> for a stable heap address.
pub(super) struct AuroraJobQueue {
    pub(super) pending: Vec<u32>,
    pub(super) total_enqueued: usize,
}

// ── Trap callbacks ────────────────────────────────────────────────────────────

unsafe extern "C" fn jq_get_host_defined_data(
    _queue: *const c_void,
    _cx: *mut RawJSContext,
    data: MutableHandle<*mut JSObject>,
) -> bool {
    *data.ptr = std::ptr::null_mut();
    true
}

unsafe extern "C" fn jq_enqueue_promise_job(
    queue: *const c_void,
    cx: *mut RawJSContext,
    _promise: Handle<*mut JSObject>,
    job: Handle<*mut JSObject>,
    _alloc_site: Handle<*mut JSObject>,
    _host_data: Handle<*mut JSObject>,
) -> bool {
    let q = &mut *(queue as *mut AuroraJobQueue);
    let mut cx = JSContext::from_ptr(NonNull::new_unchecked(cx));

    let state = get_state_ptr(&cx);
    if state.is_null() {
        log::warn!(target: "aurora::js", "[job-queue] enqueuePromiseJob: state is null, dropping job");
        return true;
    }
    let state = &mut *state;

    let id = state.window.next_id();
    let job_val = mozjs::jsval::ObjectValue(*job.ptr);

    rooted!(&in(cx) let global = state.global);
    rooted!(&in(cx) let job_rooted = job_val);
    store_callback(&mut cx, global.handle(), id, job_rooted.handle());

    q.pending.push(id);
    q.total_enqueued += 1;
    log::debug!(target: "aurora::js", "[job-queue] enqueue #{} pending={}", id, q.pending.len());
    true
}

unsafe extern "C" fn jq_run_jobs(queue: *const c_void, cx: *mut RawJSContext) {
    let q = &mut *(queue as *mut AuroraJobQueue);
    let mut cx = JSContext::from_ptr(NonNull::new_unchecked(cx));

    let state = get_state_ptr(&cx);
    if state.is_null() {
        log::warn!(target: "aurora::js", "[job-queue] run_jobs called but state is null");
        return;
    }
    let global_raw = (*state).global;
    rooted!(&in(cx) let global = global_raw);

    let mut total_run = 0usize;
    for _ in 0..10_000 {
        let ids: Vec<u32> = q.pending.drain(..).collect();
        if ids.is_empty() {
            break;
        }
        total_run += ids.len();
        for id in ids {
            call_stored_callback(&mut cx, global.handle(), id, &[]);
            delete_callback(&mut cx, global.handle(), id);
            clear_pending_exception(&mut cx);
        }
    }
    log::debug!(target: "aurora::js", "[job-queue] run_jobs drained {} jobs", total_run);
}

unsafe extern "C" fn jq_empty(queue: *const c_void) -> bool {
    (*(queue as *const AuroraJobQueue)).pending.is_empty()
}

// Atomics.waitAsync interrupt-queue stubs — YouTube never calls these.
unsafe extern "C" fn jq_push_interrupt(_queues: *mut c_void) -> *const c_void {
    1usize as *const c_void // non-null sentinel
}

unsafe extern "C" fn jq_pop_interrupt(_queues: *mut c_void) -> *const c_void {
    1usize as *const c_void
}

unsafe extern "C" fn jq_drop_interrupts(_queues: *mut c_void) {}

// ── Install ───────────────────────────────────────────────────────────────────

/// Install our custom job queue on the context.
/// Must be called BEFORE any realm or script work.
/// Returns the opaque JS::JobQueue pointer — free with `remove_job_queue`.
pub(super) unsafe fn install_job_queue(
    cx: &mut JSContext,
    queue: &mut AuroraJobQueue,
) -> *mut mozjs::jsapi::JS::JobQueue {
    use mozjs::glue::{CreateJobQueue, JobQueueTraps};
    use mozjs::rust::wrappers2::SetJobQueue;

    let traps = JobQueueTraps {
        getHostDefinedData: Some(jq_get_host_defined_data),
        enqueuePromiseJob: Some(jq_enqueue_promise_job),
        runJobs: Some(jq_run_jobs),
        empty: Some(jq_empty),
        pushNewInterruptQueue: Some(jq_push_interrupt),
        popInterruptQueue: Some(jq_pop_interrupt),
        dropInterruptQueues: Some(jq_drop_interrupts),
    };

    let queue_ptr = queue as *mut AuroraJobQueue as *const c_void;
    let js_queue = CreateJobQueue(&traps as *const _, queue_ptr, std::ptr::null_mut());
    SetJobQueue(cx, js_queue);
    js_queue
}

pub(super) unsafe fn remove_job_queue(queue: *mut mozjs::jsapi::JS::JobQueue) {
    if !queue.is_null() {
        mozjs::glue::DeleteJobQueue(queue);
    }
}
