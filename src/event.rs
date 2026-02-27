/// Event system for the deterministic simulation kernel.
///
/// Every effect in Helios is modeled as an `Event`. Events are immutable
/// records that are placed on the scheduler's priority queue and dispatched
/// in deterministic order.

use crate::node::{MessagePayload, NodeId};
use crate::time::VirtualTime;
use std::cmp::Ordering;

// ── Event ID ──────────────────────────────────────────────────────────

/// A globally unique, strictly‐increasing event identifier.
///
/// The monotonic nature of `EventId` breaks ties in the scheduler:
/// two events scheduled at the same `VirtualTime` are ordered by their
/// `EventId`, which corresponds to creation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct EventId(u64);

impl EventId {
    /// Wrap a raw u64 into an `EventId`.
    #[inline]
    pub fn new(raw: u64) -> Self {
        EventId(raw)
    }

    /// Return the raw value.
    #[inline]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E#{}", self.0)
    }
}

// ── Event ID Generator ───────────────────────────────────────────────

/// Deterministic, strictly-increasing event-ID generator.
///
/// Each `Simulation` owns exactly one of these. Because the simulation
/// is single-threaded and there is no shared mutable state, the counter
/// is trivially deterministic.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct EventIdGen {
    next: u64,
}

impl EventIdGen {
    /// Create a generator starting at 0.
    pub fn new() -> Self {
        EventIdGen { next: 0 }
    }

    /// Create a generator starting at a specific value (useful for replay/fork).
    pub fn starting_at(start: u64) -> Self {
        EventIdGen { next: start }
    }

    /// Mint the next event ID.
    pub fn next_id(&mut self) -> EventId {
        let id = EventId(self.next);
        self.next += 1;
        id
    }

    /// Peek at the next ID without consuming it.
    pub fn peek(&self) -> EventId {
        EventId(self.next)
    }
}

impl Default for EventIdGen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Event Type ────────────────────────────────────────────────────────

/// The payload of an event.
///
/// System-level events (`Noop`, `Log`), node-directed events from Batch 2
/// (`MessageDelivery`, `TimerFired`, `NodeCrash`, `NodeRecover`), and
/// network-layer events from Batch 3 (`MessageSend`, `NetworkPartition`,
/// `NetworkHeal`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum EventType {
    /// A no-op event used for testing and scheduling heartbeats.
    Noop,

    /// A generic log / trace marker.
    Log(String),

    /// Intent to send a message — processed by the network layer.
    /// The network decides whether to deliver (scheduling `MessageDelivery`)
    /// or drop the message.
    MessageSend {
        from: NodeId,
        to: NodeId,
        payload: MessagePayload,
    },

    /// A message that has passed through the network and is ready
    /// for delivery to the target node.
    MessageDelivery {
        from: NodeId,
        to: NodeId,
        payload: MessagePayload,
    },

    /// A previously scheduled timer has fired on a node.
    TimerFired {
        node: NodeId,
        timer_id: u64,
    },

    /// A node crash injection.
    NodeCrash {
        node: NodeId,
    },

    /// A node recovery injection.
    NodeRecover {
        node: NodeId,
    },

    /// Dynamically inject a network partition between two nodes.
    NetworkPartition {
        a: NodeId,
        b: NodeId,
    },

    /// Heal a network partition between two nodes.
    NetworkHeal {
        a: NodeId,
        b: NodeId,
    },
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Noop => write!(f, "Noop"),
            EventType::Log(msg) => write!(f, "Log({})", msg),
            EventType::MessageSend { from, to, .. } => {
                write!(f, "Send({} → {})", from, to)
            }
            EventType::MessageDelivery { from, to, payload } => {
                write!(f, "Deliver({} → {}, {})", from, to, payload)
            }
            EventType::TimerFired { node, timer_id } => {
                write!(f, "Timer({}, #{})", node, timer_id)
            }
            EventType::NodeCrash { node } => write!(f, "Crash({})", node),
            EventType::NodeRecover { node } => write!(f, "Recover({})", node),
            EventType::NetworkPartition { a, b } => {
                write!(f, "Partition({} ↔ {})", a, b)
            }
            EventType::NetworkHeal { a, b } => {
                write!(f, "Heal({} ↔ {})", a, b)
            }
        }
    }
}

// ── Event ─────────────────────────────────────────────────────────────

/// A single simulation event.
///
/// Events are the atomic unit of execution in Helios. The scheduler
/// orders them by `(scheduled_at, id)` to guarantee deterministic
/// processing order.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct Event {
    /// Unique identifier (monotonically increasing).
    pub id: EventId,

    /// The virtual time at which this event should be dispatched.
    pub scheduled_at: VirtualTime,

    /// The event payload.
    pub payload: EventType,
}

impl Event {
    /// Convenience constructor.
    pub fn new(id: EventId, scheduled_at: VirtualTime, payload: EventType) -> Self {
        Event {
            id,
            scheduled_at,
            payload,
        }
    }
}

/// Ordering: smallest `(scheduled_at, id)` first.
///
/// Rust's `BinaryHeap` is a *max*-heap, so we **reverse** the natural
/// ordering here to turn it into a min-heap.
impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse so that BinaryHeap pops the *smallest* key first.
        other
            .scheduled_at
            .cmp(&self.scheduled_at)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id_monotonic() {
        let mut gen = EventIdGen::new();
        let a = gen.next_id();
        let b = gen.next_id();
        let c = gen.next_id();
        assert_eq!(a.raw(), 0);
        assert_eq!(b.raw(), 1);
        assert_eq!(c.raw(), 2);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn test_event_ordering_by_time() {
        let e1 = Event::new(
            EventId::new(0),
            VirtualTime::new(10),
            EventType::Noop,
        );
        let e2 = Event::new(
            EventId::new(1),
            VirtualTime::new(20),
            EventType::Noop,
        );
        // e1 should come first (smaller time) → in reversed ordering e1 > e2.
        assert!(e1 > e2);
    }

    #[test]
    fn test_event_ordering_tiebreak_by_id() {
        let e1 = Event::new(
            EventId::new(0),
            VirtualTime::new(10),
            EventType::Noop,
        );
        let e2 = Event::new(
            EventId::new(1),
            VirtualTime::new(10),
            EventType::Log("hello".into()),
        );
        // Same time → smaller ID wins → e1 > e2 in reversed ordering.
        assert!(e1 > e2);
    }

    #[test]
    fn test_event_display() {
        let e = Event::new(
            EventId::new(42),
            VirtualTime::new(100),
            EventType::Log("test".into()),
        );
        assert_eq!(format!("{}", e.id), "E#42");
        assert_eq!(format!("{}", e.payload), "Log(test)");
    }
}
