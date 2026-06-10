use mozjs::rust::{JSEngine, JSEngineHandle};
use std::cell::OnceCell;

thread_local! {
    static SM_ENGINE: OnceCell<JSEngine> = OnceCell::new();
}

pub(super) fn get_engine_handle() -> JSEngineHandle {
    SM_ENGINE.with(|cell| {
        cell.get_or_init(|| JSEngine::init().expect("SpiderMonkey init failed"))
            .handle()
    })
}
