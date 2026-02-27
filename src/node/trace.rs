//! TraceEntry — records every event dispatched to a node.

use crate::event::EventId;
use crate::time::VirtualTime;

use super::id::NodeId;
use super::payload::NodeEvent;

/// A record of a single event dispatched to a node.
///
/// The trace is appended to by `NodeRuntime` on every dispatch and is
/// useful for test assertions, post-mortem debugging, and replay
/// tooling. The trace is intentionally reset on `NodeRuntime::fork()`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct TraceEntry {
    /// Virtual time at which the event was dispatched.
    pub time: VirtualTime,
    /// The scheduler's unique ID for this event.
    pub event_id: EventId,
    /// The node that received the event.
    pub node: NodeId,
    /// The logical event delivered to the node.
    pub node_event: NodeEvent,
}

impl std::fmt::Display for TraceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[T={} E=#{} N={}] {}",
            self.time.ticks(),
            self.event_id.raw(),
            self.node,
            self.node_event,
        )
    }
}
