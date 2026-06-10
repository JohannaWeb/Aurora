use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Per-timer callback reference: the callback is stored as a global property
/// named `__cb{id}__` on the JS global object to prevent GC.
#[derive(Clone)]
pub(super) struct TimerEntry {
    pub(super) id: u32,
    pub(super) deadline: Instant,
    pub(super) interval: Option<Duration>,
    pub(super) is_idle: bool,
    pub(super) idle_timeout: Option<Duration>,
}

#[derive(Clone)]
pub(super) struct AnimationFrameEntry {
    pub(super) id: u32,
}

/// All mutable state that native JS callbacks need to access.
/// One instance lives inside SmRuntime (Boxed for stable address).
pub(super) struct WindowCapture {
    pub(super) local_storage: BTreeMap<String, String>,
    pub(super) session_storage: BTreeMap<String, String>,
    pub(super) next_timer: u32,
    pub(super) timers: Vec<TimerEntry>,
    pub(super) animation_frames: Vec<AnimationFrameEntry>,
    pub(super) microtask_ids: Vec<u32>,
    pub(super) time_origin: Instant,
}

impl WindowCapture {
    pub(super) fn new() -> Self {
        WindowCapture {
            local_storage: BTreeMap::new(),
            session_storage: BTreeMap::new(),
            next_timer: 1,
            timers: Vec::new(),
            animation_frames: Vec::new(),
            microtask_ids: Vec::new(),
            time_origin: Instant::now(),
        }
    }

    pub(super) fn next_id(&mut self) -> u32 {
        let id = self.next_timer;
        self.next_timer += 1;
        id
    }
}
