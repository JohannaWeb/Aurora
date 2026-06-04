use super::*;
use std::time::Instant;

pub struct BoaRuntime {
    pub(super) context: Context,
    document: NodePtr,
    pub(super) registry: NodeRegistry,
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

    pub fn set_shared_state(
        &mut self,
        layout_tree: Rc<RefCell<crate::layout::LayoutTree>>,
        stylesheet: Rc<RefCell<crate::css::Stylesheet>>,
        viewport: Rc<RefCell<crate::layout::ViewportSize>>,
    ) {
        self.registry
            .set_shared_state(layout_tree, stylesheet, viewport, self.document.clone());
    }

    pub fn dispatch_event(&mut self, node: &NodePtr, event_type: &str) -> bool {
        // O(1) lookup via reverse map.
        let Some(id) = self.registry.node_id(node) else {
            return false;
        };

        // Build a real Event object.
        let event = self.build_event_object(event_type, node);

        // Fire at the target node.
        let listeners = self.registry.get_listeners(id, event_type);
        let mut handled = !listeners.is_empty();
        for listener in listeners {
            let _ = listener.call(
                &JsValue::undefined(),
                &[event.clone().into()],
                &mut self.context,
            );
        }

        // Bubble up the DOM tree.
        let mut current = node.clone();
        loop {
            let parent = {
                let b = current.borrow();
                match &*b {
                    Node::Element(_) => {
                        // Walk the registry to find the parent.
                        // This is a O(N) fallback — replace with parent pointers in Phase 5+.
                        self.find_parent(&current)
                    }
                    _ => None,
                }
            };
            let Some(parent_node) = parent else { break };
            if let Some(parent_id) = self.registry.node_id(&parent_node) {
                let parent_listeners = self.registry.get_listeners(parent_id, event_type);
                for listener in parent_listeners {
                    let _ = listener.call(
                        &JsValue::undefined(),
                        &[event.clone().into()],
                        &mut self.context,
                    );
                    handled = true;
                }
            }
            current = parent_node;
        }

        let _ = self.context.run_jobs();
        self.drain_microtasks();
        handled
    }

    /// Build a real Event JsObject with target, type, preventDefault, stopPropagation.
    fn build_event_object(&mut self, event_type: &str, target: &NodePtr) -> JsObject {
        let event = JsObject::with_null_proto();
        let target_id = self.registry.node_id(target).unwrap_or(0);

        let _ = event.set(
            js_string!("type"),
            js_string!(event_type),
            false,
            &mut self.context,
        );
        let _ = event.set(
            js_string!("bubbles"),
            JsValue::from(true),
            false,
            &mut self.context,
        );
        let _ = event.set(
            js_string!("cancelable"),
            JsValue::from(true),
            false,
            &mut self.context,
        );
        let _ = event.set(
            js_string!("defaultPrevented"),
            JsValue::from(false),
            false,
            &mut self.context,
        );
        let _ = event.set(
            js_string!("isTrusted"),
            JsValue::from(true),
            false,
            &mut self.context,
        );
        let _ = event.set(
            js_string!("timeStamp"),
            JsValue::from(0.0),
            false,
            &mut self.context,
        );

        // target and currentTarget — the DOM node's JS object if available.
        if self.registry.lookup(target_id).is_some() {
            // We can't easily get the JS object here without a full mirror;
            // set the node id as a proxy for now.
            let _ = event.set(
                js_string!("_targetId"),
                JsValue::from(target_id),
                false,
                &mut self.context,
            );
        }

        // preventDefault — sets defaultPrevented.
        let ev_clone = event.clone();
        let prevent_fn = NativeFunction::from_copy_closure_with_captures(
            |_this, _args, ev: &JsObject, ctx| {
                let _ = ev.set(
                    js_string!("defaultPrevented"),
                    JsValue::from(true),
                    false,
                    ctx,
                );
                Ok(JsValue::undefined())
            },
            ev_clone,
        );
        let prevent_js_fn = NativeFunction::to_js_function(prevent_fn, self.context.realm());
        let _ = event.set(
            js_string!("preventDefault"),
            prevent_js_fn,
            false,
            &mut self.context,
        );

        // stopPropagation — noop for now (bubbling is simple, no stop yet).
        let stop_fn = NativeFunction::from_fn_ptr(|_, _, _| Ok(JsValue::undefined()));
        let stop_js_fn = NativeFunction::to_js_function(stop_fn, self.context.realm());
        let _ = event.set(
            js_string!("stopPropagation"),
            stop_js_fn,
            false,
            &mut self.context,
        );
        let stop_imm_fn = NativeFunction::from_fn_ptr(|_, _, _| Ok(JsValue::undefined()));
        let stop_imm_js_fn = NativeFunction::to_js_function(stop_imm_fn, self.context.realm());
        let _ = event.set(
            js_string!("stopImmediatePropagation"),
            stop_imm_js_fn,
            false,
            &mut self.context,
        );

        event
    }

    /// Walk the registered nodes to find the parent of `node`.
    /// This is O(N) — acceptable until parent pointers land in Phase 5+.
    fn find_parent(&self, node: &NodePtr) -> Option<NodePtr> {
        let nodes = self.registry.nodes.borrow();
        for candidate in nodes.values() {
            let borrow = candidate.borrow();
            let children = match &*borrow {
                Node::Document { children, .. } => children.as_slice(),
                Node::Element(el) => el.children.as_slice(),
                Node::Text(_) => continue,
            };
            if children.iter().any(|child| Rc::ptr_eq(child, node)) {
                return Some(candidate.clone());
            }
        }
        None
    }

    pub fn execute(&mut self, script: &str) -> JsResult<JsValue> {
        let result = self.context.eval(Source::from_bytes(script));
        let _ = self.context.run_jobs();
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
        let _ = self.context.run_jobs();
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
        let _ = self.context.run_jobs();
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
            let _ = self.context.run_jobs();
        }
        ran_microtasks
    }
}
