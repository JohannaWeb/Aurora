use mozjs::rust::{JSEngine, JSEngineHandle};
use std::sync::Mutex;

// One JSEngine per process, ever: mozjs 0.16 marks the engine state ShutDown
// when the JSEngine drops, and JS_Init cannot run again after JS_ShutDown.
// So the engine is created once, kept alive for the whole process, and shut
// down via atexit. The shutdown must actually happen: leaking the engine
// leaves the JS Helper threads running into libc's exit handlers, where
// SpiderMonkey's static destructors tear down mutexes those threads still
// use (pthread_mutex_destroy EBUSY, then SIGSEGV).
//
// JSEngine is !Send as a caution marker, but JS_Init/JS_ShutDown are
// process-global; the engine is only moved to stash it, never used cross-
// thread except for the final drop.
struct EngineHolder(Option<JSEngine>);
unsafe impl Send for EngineHolder {}

static SM_ENGINE: Mutex<EngineHolder> = Mutex::new(EngineHolder(None));

unsafe extern "C" {
    fn atexit(cb: extern "C" fn()) -> i32;
}

extern "C" fn shutdown_engine() {
    if let Ok(mut holder) = SM_ENGINE.lock() {
        if let Some(engine) = holder.0.take() {
            if engine.can_shutdown() {
                drop(engine); // JS_ShutDown: joins the helper threads
            } else {
                // A runtime is still alive (e.g. process::exit mid-run);
                // dropping would assert. Leak and accept the unclean exit.
                std::mem::forget(engine);
            }
        }
    }
}

// Handles are derived from the stashed engine on demand rather than cloned
// from a static template: a handle living in a static would never drop, so
// can_shutdown() would never become true and shutdown_engine would have to
// leak.
pub(super) fn get_engine_handle() -> JSEngineHandle {
    let mut holder = SM_ENGINE.lock().unwrap();
    if holder.0.is_none() {
        holder.0 = Some(JSEngine::init().expect("SpiderMonkey init failed"));
        // Registered during main, so it runs before the static
        // destructors that were registered at program load.
        unsafe { atexit(shutdown_engine) };
    }
    holder.0.as_ref().unwrap().handle()
}
