/// Deterministic event scheduler.
///
/// Uses a `BinaryHeap` with reversed `Ord` on `Event` to act as a
/// min-heap keyed by `(scheduled_at, event_id)`. Because event IDs are
/// strictly increasing and the heap is deterministic, two runs with the
/// same seed will always produce the same dispatch order.

use std::collections::BinaryHeap;

use crate::event::{Event, EventId, EventIdGen, EventType};
use crate::time::VirtualTime;

/// The core deterministic scheduler.
///
/// Owns the event queue and the ID generator. All scheduling goes through
/// this struct to ensure monotonic IDs and deterministic ordering.
#[derive(Debug, Clone)]
pub struct Scheduler {
    /// Min-heap (via reversed Ord on Event).
    queue: BinaryHeap<Event>,

    /// Monotonic event-ID generator.
    id_gen: EventIdGen,
}

impl Scheduler {
    /// Create a new, empty scheduler.
    pub fn new() -> Self {
        Scheduler {
            queue: BinaryHeap::new(),
            id_gen: EventIdGen::new(),
        }
    }

    /// Create a scheduler whose ID generator starts at `start_id`.
    /// Useful when forking a simulation (Batch 5).
    pub fn with_id_start(start_id: u64) -> Self {
        Scheduler {
            queue: BinaryHeap::new(),
            id_gen: EventIdGen::starting_at(start_id),
        }
    }

    /// Schedule a new event at the given virtual time.
    ///
    /// Returns the `EventId` assigned to this event.
    pub fn schedule(&mut self, at: VirtualTime, payload: EventType) -> EventId {
        let id = self.id_gen.next_id();
        self.queue.push(Event::new(id, at, payload));
        id
    }

    /// Pop the next event (earliest time, lowest ID).
    ///
    /// Returns `None` when the queue is empty.
    pub fn pop_next(&mut self) -> Option<Event> {
        self.queue.pop()
    }

    /// Peek at the next event without removing it.
    pub fn peek_next(&self) -> Option<&Event> {
        self.queue.peek()
    }

    /// Returns `true` if the event queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns the number of pending events.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns the next event ID that will be assigned.
    pub fn next_event_id(&self) -> EventId {
        self.id_gen.peek()
    }

    /// Drain all events in deterministic order into a `Vec`.
    /// Useful for testing and snapshotting.
    pub fn drain_ordered(&mut self) -> Vec<Event> {
        let mut events = Vec::with_capacity(self.queue.len());
        while let Some(e) = self.queue.pop() {
            events.push(e);
        }
        events
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_at_same_time() {
        let mut sched = Scheduler::new();

        sched.schedule(VirtualTime::new(10), EventType::Log("first".into()));
        sched.schedule(VirtualTime::new(10), EventType::Log("second".into()));
        sched.schedule(VirtualTime::new(10), EventType::Log("third".into()));

        let e1 = sched.pop_next().unwrap();
        let e2 = sched.pop_next().unwrap();
        let e3 = sched.pop_next().unwrap();

        // Same time → ordered by ascending event ID (creation order).
        assert!(e1.id < e2.id);
        assert!(e2.id < e3.id);
        assert_eq!(e1.payload, EventType::Log("first".into()));
        assert_eq!(e2.payload, EventType::Log("second".into()));
        assert_eq!(e3.payload, EventType::Log("third".into()));
    }

    #[test]
    fn test_time_ordering() {
        let mut sched = Scheduler::new();

        sched.schedule(VirtualTime::new(30), EventType::Log("late".into()));
        sched.schedule(VirtualTime::new(10), EventType::Log("early".into()));
        sched.schedule(VirtualTime::new(20), EventType::Log("mid".into()));

        let e1 = sched.pop_next().unwrap();
        let e2 = sched.pop_next().unwrap();
        let e3 = sched.pop_next().unwrap();

        assert_eq!(e1.scheduled_at, VirtualTime::new(10));
        assert_eq!(e2.scheduled_at, VirtualTime::new(20));
        assert_eq!(e3.scheduled_at, VirtualTime::new(30));
    }

    #[test]
    fn test_mixed_ordering() {
        let mut sched = Scheduler::new();

        // Interleave times to stress the heap.
        sched.schedule(VirtualTime::new(50), EventType::Noop);
        sched.schedule(VirtualTime::new(10), EventType::Noop);
        sched.schedule(VirtualTime::new(10), EventType::Noop);
        sched.schedule(VirtualTime::new(30), EventType::Noop);
        sched.schedule(VirtualTime::new(10), EventType::Noop);

        let events = sched.drain_ordered();
        // Must be sorted by (time, id).
        for window in events.windows(2) {
            let (a, b) = (&window[0], &window[1]);
            assert!(
                (a.scheduled_at, a.id) <= (b.scheduled_at, b.id),
                "Events out of order: {:?} vs {:?}",
                a,
                b
            );
        }
    }

    #[test]
    fn test_empty_scheduler() {
        let mut sched = Scheduler::new();
        assert!(sched.is_empty());
        assert_eq!(sched.len(), 0);
        assert!(sched.pop_next().is_none());
    }

    #[test]
    fn test_determinism_across_runs() {
        // Two independent schedulers with the same insertion order must
        // produce the same output order.
        fn build_schedule() -> Vec<Event> {
            let mut sched = Scheduler::new();
            sched.schedule(VirtualTime::new(5), EventType::Log("a".into()));
            sched.schedule(VirtualTime::new(3), EventType::Log("b".into()));
            sched.schedule(VirtualTime::new(5), EventType::Log("c".into()));
            sched.schedule(VirtualTime::new(1), EventType::Log("d".into()));
            sched.schedule(VirtualTime::new(3), EventType::Log("e".into()));
            sched.drain_ordered()
        }

        let run1 = build_schedule();
        let run2 = build_schedule();

        assert_eq!(run1.len(), run2.len());
        for (a, b) in run1.iter().zip(run2.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.scheduled_at, b.scheduled_at);
            assert_eq!(a.payload, b.payload);
        }
    }
}
