//! `PingNode` — records all received messages for test assertions.

use crate::eventlog::{hash_bytes, hash_combine};
use crate::simulation::SimulationContext;
use crate::time::VirtualTime;

use crate::node::id::NodeId;
use crate::node::payload::{MessagePayload, NodeEvent};
use crate::node::traits::SimNode;

/// A node that records every message it receives.
///
/// `PingNode` has no active behavior — it simply accumulates received
/// messages. This makes it ideal as a "sink" node in tests that
/// verify message delivery counts, ordering, and payloads.
#[derive(Debug, Clone)]
pub struct PingNode {
    pub id: NodeId,
    /// All messages received, in delivery order: `(time, from, payload)`.
    pub received: Vec<(VirtualTime, NodeId, MessagePayload)>,
}

impl PingNode {
    /// Create a new PingNode with the given ID.
    pub fn new(id: NodeId) -> Self {
        PingNode {
            id,
            received: Vec::new(),
        }
    }
}

impl SimNode for PingNode {
    fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent) {
        if let NodeEvent::Message { from, payload } = event {
            self.received.push((ctx.now(), from, payload));
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn state_hash(&self) -> u64 {
        let mut h = self.id.raw();
        h = hash_combine(h, self.received.len() as u64);
        for (time, from, payload) in &self.received {
            h = hash_combine(h, time.ticks());
            h = hash_combine(h, from.raw());
            h = hash_combine(
                h,
                match payload {
                    MessagePayload::Empty => 0,
                    MessagePayload::Text(s) => hash_bytes(s.as_bytes()),
                    MessagePayload::Data(d) => hash_bytes(d),
                },
            );
        }
        h
    }

    fn clone_node(&self) -> Box<dyn SimNode> {
        Box::new(self.clone())
    }
}
