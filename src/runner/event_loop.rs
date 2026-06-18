//! Explicit event-loop phases (action plan Phase 7).
//!
//! Aurora's page-load pump historically drained timers, animation frames, and
//! `MutationObserver` records opportunistically, with the ordering implied by
//! the order of statements in the loop body. That made the scheduling model hard
//! to reason about and impossible to test, and it let YouTube-specific
//! callback-draining hacks creep in as the de-facto event-loop model.
//!
//! [`EventLoopPhase`] names the phases of one event-loop turn in the canonical
//! order of the HTML spec's processing model, and [`run_event_loop_turn`] drives
//! them in that order. The pump implements each phase against the JS runtime; a
//! mock implementation lets the ordering be unit-tested without a real engine.
//!
//! Some phases (style/layout, paint, idle callbacks, resize observers) are not
//! yet driven by the headless pump — those are window-loop concerns or
//! unsupported APIs — but they are part of the model so the full turn ordering
//! is explicit and the gaps are visible rather than implicit.

/// A phase of one event-loop turn, in canonical (HTML processing model) order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventLoopPhase {
    /// Run one task (e.g. a due `setTimeout`/`setInterval` callback).
    RunTask,
    /// Drain the microtask queue (promise reactions). On V8 this is performed
    /// automatically at the end of each task, so the pump treats it as implicit.
    MicrotaskCheckpoint,
    /// Deliver queued `MutationObserver` records to their callbacks.
    MutationObserverDelivery,
    /// Deliver `ResizeObserver` callbacks. Not yet supported.
    ResizeObserverDelivery,
    /// Run `requestAnimationFrame` callbacks for this frame.
    RequestAnimationFrame,
    /// Recompute style and layout. Driven by the window loop, not the pump.
    StyleAndLayout,
    /// Paint the current frame. Driven by the window loop, not the pump.
    Paint,
    /// Run `requestIdleCallback` callbacks. Not yet supported.
    IdleCallbacks,
}

impl EventLoopPhase {
    /// The canonical order phases run within one event-loop turn: run a task,
    /// drain microtasks (where `MutationObserver` records are delivered), deliver
    /// observer callbacks, then the rendering steps (rAF → style/layout → paint),
    /// and finally idle work.
    pub const TURN_ORDER: [EventLoopPhase; 8] = [
        EventLoopPhase::RunTask,
        EventLoopPhase::MicrotaskCheckpoint,
        EventLoopPhase::MutationObserverDelivery,
        EventLoopPhase::ResizeObserverDelivery,
        EventLoopPhase::RequestAnimationFrame,
        EventLoopPhase::StyleAndLayout,
        EventLoopPhase::Paint,
        EventLoopPhase::IdleCallbacks,
    ];
}

/// Run one event-loop turn, invoking `run_phase` for each phase in canonical
/// order. Returns the phases that reported doing work, in the order they ran, so
/// the caller can decide whether the loop should keep spinning.
///
/// Ordering is fixed by [`EventLoopPhase::TURN_ORDER`]; callers cannot reorder
/// phases, which is what makes the scheduler's sequence explicit and testable.
pub fn run_event_loop_turn(
    mut run_phase: impl FnMut(EventLoopPhase) -> bool,
) -> Vec<EventLoopPhase> {
    let mut fired = Vec::new();
    for &phase in &EventLoopPhase::TURN_ORDER {
        if run_phase(phase) {
            fired.push(phase);
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(phase: EventLoopPhase) -> usize {
        EventLoopPhase::TURN_ORDER
            .iter()
            .position(|p| *p == phase)
            .expect("phase is in TURN_ORDER")
    }

    #[test]
    fn turn_order_matches_html_event_loop_invariants() {
        use EventLoopPhase::*;
        // A task runs first; microtasks drain before any rendering work.
        assert!(pos(RunTask) < pos(MicrotaskCheckpoint));
        assert!(pos(MicrotaskCheckpoint) < pos(RequestAnimationFrame));
        // MutationObserver records deliver at the microtask checkpoint, i.e. after
        // the task that mutated the DOM and before rAF/rendering.
        assert!(pos(RunTask) < pos(MutationObserverDelivery));
        assert!(pos(MutationObserverDelivery) < pos(RequestAnimationFrame));
        // Rendering steps: rAF before style/layout before paint.
        assert!(pos(RequestAnimationFrame) < pos(StyleAndLayout));
        assert!(pos(StyleAndLayout) < pos(Paint));
        // Idle callbacks run last.
        assert_eq!(pos(IdleCallbacks), EventLoopPhase::TURN_ORDER.len() - 1);
    }

    #[test]
    fn run_event_loop_turn_invokes_every_phase_in_canonical_order() {
        let mut seen = Vec::new();
        let fired = run_event_loop_turn(|phase| {
            seen.push(phase);
            true
        });
        // Every phase is visited exactly once, in canonical order.
        assert_eq!(seen, EventLoopPhase::TURN_ORDER.to_vec());
        // When every phase reports work, all are returned in the same order.
        assert_eq!(fired, EventLoopPhase::TURN_ORDER.to_vec());
    }

    #[test]
    fn run_event_loop_turn_reports_only_phases_that_did_work_in_order() {
        use EventLoopPhase::*;
        let fired = run_event_loop_turn(|phase| {
            matches!(phase, RunTask | RequestAnimationFrame)
        });
        // A task does not starve rendering: rAF still runs in the same turn, and
        // the reported phases preserve canonical order.
        assert_eq!(fired, vec![RunTask, RequestAnimationFrame]);
    }
}
