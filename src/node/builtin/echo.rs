//! `EchoNode` — echoes every received message back to its sender.

use crate::eventlog::hash_combine;
use crate::simulation::SimulationContext;

use crate::node::id::NodeId;
use crate::node::payload::NodeEvent;
use crate::node::traits::SimNode;

/// A simple node that echoes every message back to its sender.
///
/// Useful for testing message round-trips and verifying network behavior.
/// The `echo_count` field tracks the total number of messages echoed.
#[derive(Debug, Clone)]
pub struct EchoNode {
    pub id: NodeId,
    pub echo_count: u64,
}

impl EchoNode {
    /// Create a new EchoNode with the given ID.
    pub fn new(id: NodeId) -> Self {
        EchoNode { id, echo_count: 0 }
    }
}

impl SimNode for EchoNode {
    fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent) {
        if let NodeEvent::Message { from, payload } = event {
            self.echo_count += 1;
            ctx.schedule_message(self.id, from, 1, payload);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn state_hash(&self) -> u64 {
        hash_combine(self.id.raw(), self.echo_count)
    }

    fn clone_node(&self) -> Box<dyn SimNode> {
        Box::new(self.clone())
    }
}
