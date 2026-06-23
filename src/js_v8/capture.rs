use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub struct TimerEntry {
    pub id: u32,
    pub callback: v8::Global<v8::Function>,
    pub deadline: Instant,
    pub interval: Option<Duration>,
}

pub struct AnimationFrameEntry {
    pub id: u32,
    pub callback: v8::Global<v8::Function>,
}

pub struct WindowCapture {
    pub timers: Vec<TimerEntry>,
    pub animation_frames: Vec<AnimationFrameEntry>,
    pub next_timer_id: u32,
    pub next_raf_id: u32,
    pub time_origin: Instant,
    pub storage: Rc<RefCell<BTreeMap<String, String>>>,
    pub session: Rc<RefCell<BTreeMap<String, String>>>,
}

impl WindowCapture {
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            animation_frames: Vec::new(),
            next_timer_id: 1,
            next_raf_id: 1,
            time_origin: Instant::now(),
            storage: Rc::new(RefCell::new(BTreeMap::new())),
            session: Rc::new(RefCell::new(BTreeMap::new())),
        }
    }
}
