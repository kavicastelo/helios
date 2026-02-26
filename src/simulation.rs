/// Simulation execution loop.
///
/// Drives the scheduler: pops events, advances virtual time, dispatches
/// to a user-supplied handler. The loop is purely synchronous and
/// single-threaded — determinism is trivial.

use crate::event::{Event, EventId, EventType};
use crate::scheduler::Scheduler;
use crate::time::VirtualTime;

// ── Handler trait ─────────────────────────────────────────────────────

/// User-defined event handler.
///
/// Implement this trait to react to dispatched events. The handler
/// receives a mutable reference to `SimulationContext` so it can
/// schedule follow-up events.
pub trait EventHandler {
    /// Called for every dispatched event.
    fn handle(&mut self, ctx: &mut SimulationContext, event: &Event);
}

/// A handler backed by a closure — useful for tests and one-off scripts.
impl<F> EventHandler for F
where
    F: FnMut(&mut SimulationContext, &Event),
{
    fn handle(&mut self, ctx: &mut SimulationContext, event: &Event) {
        (self)(ctx, event);
    }
}

// ── Simulation Context ───────────────────────────────────────────────

/// Mutable context passed to the handler on every event dispatch.
///
/// Provides the handler with:
/// - the current virtual time
/// - the ability to schedule follow-up events
///
/// The context borrows the scheduler mutably, so a handler cannot
/// interfere with dispatch ordering outside of the schedule API.
pub struct SimulationContext<'a> {
    pub(crate) scheduler: &'a mut Scheduler,
    pub(crate) now: VirtualTime,
}

impl<'a> SimulationContext<'a> {
    /// Current virtual time.
    #[inline]
    pub fn now(&self) -> VirtualTime {
        self.now
    }

    /// Schedule an event at an absolute virtual time.
    ///
    /// # Panics
    /// Panics if `at` is before the current time (non-causal scheduling).
    pub fn schedule_at(&mut self, at: VirtualTime, payload: EventType) -> EventId {
        assert!(
            at >= self.now,
            "Cannot schedule event in the past: now={}, at={}",
            self.now,
            at
        );
        self.scheduler.schedule(at, payload)
    }

    /// Schedule an event `delay` ticks into the future relative to now.
    ///
    /// # Panics
    /// Panics on arithmetic overflow (astronomically unlikely).
    pub fn schedule_after(&mut self, delay: u64, payload: EventType) -> EventId {
        let at = self
            .now
            .plus(delay)
            .expect("VirtualTime overflow when scheduling");
        self.scheduler.schedule(at, payload)
    }

    /// Number of pending events in the scheduler.
    pub fn pending_count(&self) -> usize {
        self.scheduler.len()
    }
}

// ── Simulation ────────────────────────────────────────────────────────

/// Top-level simulation driver.
///
/// Owns the scheduler and tracks the current virtual time.
/// Call `run` to execute until the queue is drained, or
/// `step` to advance by exactly one event.
#[derive(Debug, Clone)]
pub struct Simulation {
    scheduler: Scheduler,
    current_time: VirtualTime,
    events_processed: u64,
}

impl Simulation {
    /// Create a new simulation starting at time zero.
    pub fn new() -> Self {
        Simulation {
            scheduler: Scheduler::new(),
            current_time: VirtualTime::ZERO,
            events_processed: 0,
        }
    }

    /// Access the scheduler directly (e.g., for initial event seeding).
    pub fn scheduler_mut(&mut self) -> &mut Scheduler {
        &mut self.scheduler
    }

    /// Current virtual time.
    pub fn current_time(&self) -> VirtualTime {
        self.current_time
    }

    /// Total events processed so far.
    pub fn events_processed(&self) -> u64 {
        self.events_processed
    }

    /// Schedule an event before the simulation starts running.
    pub fn schedule(&mut self, at: VirtualTime, payload: EventType) -> EventId {
        self.scheduler.schedule(at, payload)
    }

    /// Execute a single step: pop one event, advance time, dispatch.
    ///
    /// Returns `Some(event)` if an event was processed, `None` if the
    /// queue is empty.
    pub fn step(&mut self, handler: &mut dyn EventHandler) -> Option<Event> {
        let event = self.scheduler.pop_next()?;

        // Virtual time must never go backward.
        assert!(
            event.scheduled_at >= self.current_time,
            "Time went backward! current={}, event={}",
            self.current_time,
            event.scheduled_at
        );
        self.current_time = event.scheduled_at;
        self.events_processed += 1;

        let mut ctx = SimulationContext {
            scheduler: &mut self.scheduler,
            now: self.current_time,
        };
        handler.handle(&mut ctx, &event);

        Some(event)
    }

    /// Run until the event queue is empty.
    ///
    /// Returns the total number of events processed during this run.
    pub fn run(&mut self, handler: &mut dyn EventHandler) -> u64 {
        let start = self.events_processed;
        while self.step(handler).is_some() {}
        self.events_processed - start
    }

    /// Run until the event queue is empty **or** `max_steps` events
    /// have been dispatched, whichever comes first.
    ///
    /// Returns the number of events processed in this call.
    pub fn run_for(&mut self, max_steps: u64, handler: &mut dyn EventHandler) -> u64 {
        let start = self.events_processed;
        let mut steps = 0u64;
        while steps < max_steps {
            if self.step(handler).is_none() {
                break;
            }
            steps += 1;
        }
        self.events_processed - start
    }

    /// Returns `true` if there are no more events to process.
    pub fn is_finished(&self) -> bool {
        self.scheduler.is_empty()
    }
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventType;
    use crate::time::VirtualTime;

    #[test]
    fn test_basic_execution_loop() {
        let mut sim = Simulation::new();

        sim.schedule(VirtualTime::new(10), EventType::Log("a".into()));
        sim.schedule(VirtualTime::new(20), EventType::Log("b".into()));
        sim.schedule(VirtualTime::new(30), EventType::Log("c".into()));

        let mut log: Vec<String> = Vec::new();

        let processed = sim.run(&mut |_ctx: &mut SimulationContext, event: &Event| {
            if let EventType::Log(msg) = &event.payload {
                log.push(msg.clone());
            }
        });

        assert_eq!(processed, 3);
        assert_eq!(log, vec!["a", "b", "c"]);
        assert_eq!(sim.current_time(), VirtualTime::new(30));
    }

    #[test]
    fn test_handler_schedules_followup() {
        let mut sim = Simulation::new();

        // Seed a single event.
        sim.schedule(VirtualTime::new(0), EventType::Log("start".into()));

        let mut log: Vec<(u64, String)> = Vec::new();

        sim.run(&mut |ctx: &mut SimulationContext, event: &Event| {
            if let EventType::Log(msg) = &event.payload {
                log.push((ctx.now().ticks(), msg.clone()));

                // Schedule a follow-up 10 ticks later, up to tick 30.
                if ctx.now().ticks() < 30 {
                    ctx.schedule_after(10, EventType::Log("ping".into()));
                }
            }
        });

        assert_eq!(
            log,
            vec![
                (0, "start".into()),
                (10, "ping".into()),
                (20, "ping".into()),
                (30, "ping".into()),
            ]
        );
        assert_eq!(sim.current_time(), VirtualTime::new(30));
    }

    #[test]
    fn test_step_by_step() {
        let mut sim = Simulation::new();

        sim.schedule(VirtualTime::new(5), EventType::Noop);
        sim.schedule(VirtualTime::new(15), EventType::Noop);

        let mut noop_handler = |_ctx: &mut SimulationContext, _event: &Event| {};

        let first = sim.step(&mut noop_handler).unwrap();
        assert_eq!(first.scheduled_at, VirtualTime::new(5));
        assert_eq!(sim.current_time(), VirtualTime::new(5));

        let second = sim.step(&mut noop_handler).unwrap();
        assert_eq!(second.scheduled_at, VirtualTime::new(15));
        assert_eq!(sim.current_time(), VirtualTime::new(15));

        assert!(sim.step(&mut noop_handler).is_none());
    }

    #[test]
    fn test_run_for_limits_steps() {
        let mut sim = Simulation::new();

        for i in 0..100 {
            sim.schedule(VirtualTime::new(i), EventType::Noop);
        }

        let mut noop_handler = |_ctx: &mut SimulationContext, _event: &Event| {};

        let processed = sim.run_for(10, &mut noop_handler);
        assert_eq!(processed, 10);
        assert_eq!(sim.events_processed(), 10);
        assert!(!sim.is_finished());
    }

    #[test]
    fn test_time_monotonicity() {
        let mut sim = Simulation::new();

        // Schedule events in reverse order — scheduler must still
        // dispatch in time-ascending order.
        sim.schedule(VirtualTime::new(100), EventType::Noop);
        sim.schedule(VirtualTime::new(50), EventType::Noop);
        sim.schedule(VirtualTime::new(75), EventType::Noop);
        sim.schedule(VirtualTime::new(10), EventType::Noop);

        let mut times: Vec<u64> = Vec::new();

        sim.run(&mut |ctx: &mut SimulationContext, _event: &Event| {
            times.push(ctx.now().ticks());
        });

        // Must be monotonically non-decreasing.
        for window in times.windows(2) {
            assert!(window[0] <= window[1], "Time went backward: {:?}", times);
        }
        assert_eq!(times, vec![10, 50, 75, 100]);
    }

    #[test]
    fn test_deterministic_replay() {
        /// Run a deterministic simulation and return the event trace.
        fn run_trace() -> Vec<(u64, u64, String)> {
            let mut sim = Simulation::new();

            sim.schedule(VirtualTime::new(5), EventType::Log("alpha".into()));
            sim.schedule(VirtualTime::new(5), EventType::Log("beta".into()));
            sim.schedule(VirtualTime::new(3), EventType::Log("gamma".into()));
            sim.schedule(VirtualTime::new(10), EventType::Log("delta".into()));

            let mut trace = Vec::new();

            sim.run(
                &mut |ctx: &mut SimulationContext, event: &Event| {
                    if let EventType::Log(msg) = &event.payload {
                        trace.push((event.id.raw(), ctx.now().ticks(), msg.clone()));
                    }
                },
            );

            trace
        }

        // Two independent runs must produce the exact same trace.
        let run1 = run_trace();
        let run2 = run_trace();
        assert_eq!(run1, run2, "Simulation is not deterministic!");
    }

    #[test]
    fn test_empty_simulation() {
        let mut sim = Simulation::new();
        let mut noop_handler = |_ctx: &mut SimulationContext, _event: &Event| {};
        let processed = sim.run(&mut noop_handler);
        assert_eq!(processed, 0);
        assert!(sim.is_finished());
    }

    #[test]
    fn test_cascade_scheduling() {
        // Handler that creates a "cascade" of events: each event
        // schedules two more, up to a limit.
        let mut sim = Simulation::new();
        sim.schedule(VirtualTime::new(0), EventType::Log("root".into()));

        let mut count = 0u64;
        let max_events = 15u64;

        sim.run(&mut |ctx: &mut SimulationContext, _event: &Event| {
            count += 1;
            if count < max_events {
                ctx.schedule_after(1, EventType::Noop);
            }
        });

        assert_eq!(count, max_events);
    }
}
