use super::*;
use std::time::Instant;

pub struct BoaRuntime {
    context: Context,
    #[allow(dead_code)]
    document: NodePtr,
    #[allow(dead_code)]
    registry: NodeRegistry,
    window: WindowCapture,
}

impl BoaRuntime {
    pub fn new(document: NodePtr) -> Self {
        let mut context = Context::default();
        let registry = NodeRegistry::new();

        let window = install_globals(&mut context, &document, &registry);
        install_dom_constructors(&mut context);
        install_document(&mut context, &document, &registry);
        install_observers(&mut context);
        install_xhr_and_fetch(&mut context);

        Self {
            context,
            document,
            registry,
            window,
        }
    }

    pub fn execute(&mut self, script: &str) -> JsResult<JsValue> {
        let result = self.context.eval(Source::from_bytes(script));
        self.context.run_jobs();
        self.drain_microtasks();
        result
    }

    pub fn clear_dirty_bits(&self) {
        self.registry.clear_dirty_bits();
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        let mut fired = false;
        for entry in self.ready_timers(now) {
            let _ = entry
                .callback
                .call(&JsValue::undefined(), &[], &mut self.context);
            fired = true;
        }
        self.context.run_jobs();
        let ran_microtasks = self.drain_microtasks();
        (fired || ran_microtasks) && self.registry.take_needs_reflow()
    }

    pub fn drain_animation_frame_callbacks(&mut self, now: Instant) -> bool {
        let callbacks = self
            .window
            .animation_frames
            .borrow_mut()
            .drain(..)
            .collect::<Vec<_>>();
        if callbacks.is_empty() {
            return false;
        }

        let timestamp = now.duration_since(self.window.time_origin).as_secs_f64() * 1000.0;
        for entry in callbacks {
            let _ = entry.callback.call(
                &JsValue::undefined(),
                &[JsValue::from(timestamp)],
                &mut self.context,
            );
        }
        self.context.run_jobs();
        self.drain_microtasks();
        self.registry.take_needs_reflow()
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.window
            .timers
            .borrow()
            .iter()
            .map(|entry| entry.deadline)
            .min()
    }

    pub fn has_animation_frame_callbacks(&self) -> bool {
        !self.window.animation_frames.borrow().is_empty()
    }

    pub fn has_ready_work(&self, now: Instant) -> bool {
        self.has_animation_frame_callbacks()
            || !self.window.microtasks.borrow().is_empty()
            || self
                .next_deadline()
                .map(|deadline| deadline <= now)
                .unwrap_or(false)
    }

    fn ready_timers(&mut self, now: Instant) -> Vec<TimerEntry> {
        let mut ready = Vec::new();
        let mut pending = Vec::new();
        for mut entry in self.window.timers.borrow_mut().drain(..) {
            if entry.deadline <= now && ready.len() < 100 {
                ready.push(entry.clone());
                if let Some(interval) = entry.interval {
                    entry.deadline = now + interval;
                    pending.push(entry);
                }
            } else {
                pending.push(entry);
            }
        }
        *self.window.timers.borrow_mut() = pending;
        ready
    }

    fn drain_microtasks(&mut self) -> bool {
        let mut ran_microtasks = false;
        for _ in 0..1000 {
            let callbacks = self
                .window
                .microtasks
                .borrow_mut()
                .drain(..)
                .collect::<Vec<_>>();
            if callbacks.is_empty() {
                return ran_microtasks;
            }
            ran_microtasks = true;
            for callback in callbacks {
                let _ = callback.call(&JsValue::undefined(), &[], &mut self.context);
            }
            self.context.run_jobs();
        }
        ran_microtasks
    }
}
