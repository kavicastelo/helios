//! `SimNode` trait and `SimulationContext` extensions for node communication.

use crate::event::{EventId, EventType};
use crate::simulation::SimulationContext;

use super::id::NodeId;
use super::payload::{MessagePayload, NodeEvent};

// ── SimNode ───────────────────────────────────────────────────────────

/// Trait implemented by every simulated node.
///
/// Nodes react to [`NodeEvent`]s via `on_event` and may schedule
/// follow-up events through the provided [`SimulationContext`].
///
/// # Contract
///
/// Implementations **must**:
/// - Not use global mutable state.
/// - Route all side effects through `ctx`.
/// - Be deterministic for equal inputs.
/// - Implement `clone_node` for dyn-safe cloning (required by forking).
///
/// # Example
///
/// ```rust
/// use helios::node::{SimNode, NodeEvent, NodeId};
/// use helios::simulation::SimulationContext;
///
/// #[derive(Clone)]
/// struct MyNode { id: NodeId, count: u32 }
///
/// impl SimNode for MyNode {
///     fn on_event(&mut self, _ctx: &mut SimulationContext, event: NodeEvent) {
///         if matches!(event, NodeEvent::Message { .. }) {
///             self.count += 1;
///         }
///     }
///     fn as_any(&self) -> &dyn std::any::Any { self }
///     fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
///     fn clone_node(&self) -> Box<dyn SimNode> { Box::new(self.clone()) }
/// }
/// ```
pub trait SimNode {
    /// React to a dispatched event.
    fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent);

    /// Downcast support — required for `NodeRuntime::node::<T>()`.
    fn as_any(&self) -> &dyn std::any::Any;
    /// Mutable downcast support.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Return a deterministic hash of this node's current state.
    ///
    /// Used for checkpoint validation during event-sourced replay.
    /// The default returns `0` (opt-out).
    fn state_hash(&self) -> u64 {
        0
    }

    /// Create a boxed clone of this node (dyn-safe cloning for fork).
    ///
    /// **Implementation note**: This is necessary because `Clone` is not
    /// object-safe. Each concrete node must implement this by calling
    /// `Box::new(self.clone())` (assuming `Clone` is derived).
    fn clone_node(&self) -> Box<dyn SimNode>;
}

// ── SimulationContext node extensions ─────────────────────────────────

/// Extension methods on `SimulationContext` for node-level scheduling.
///
/// These are the primary API nodes use to interact with the simulation.
impl SimulationContext<'_> {
    /// Send a message through the network layer (two-phase delivery).
    ///
    /// Schedules a `MessageSend` at the current time. The `NodeRuntime`
    /// will pass it through `Network::process()` to decide delay/drop.
    /// Use [`schedule_message`] to bypass the network entirely.
    pub fn send(
        &mut self,
        from: NodeId,
        to: NodeId,
        payload: MessagePayload,
    ) -> EventId {
        self.schedule_after(0, EventType::MessageSend { from, to, payload })
    }

    /// Schedule a direct `MessageDelivery` (bypasses the network layer).
    ///
    /// Useful for tests that need predictable timing without network
    /// interference. The message is delivered after `delay` ticks.
    pub fn schedule_message(
        &mut self,
        from: NodeId,
        to: NodeId,
        delay: u64,
        payload: MessagePayload,
    ) -> EventId {
        self.schedule_after(delay, EventType::MessageDelivery { from, to, payload })
    }

    /// Schedule a timer for `node_id` to fire after `delay` ticks.
    ///
    /// Returns the `EventId`, which also serves as the `timer_id`
    /// delivered in `NodeEvent::TimerFired`.
    pub fn schedule_timer(&mut self, node_id: NodeId, delay: u64) -> EventId {
        let timer_id = self.scheduler.next_event_id().raw();
        self.schedule_after(
            delay,
            EventType::TimerFired {
                node: node_id,
                timer_id,
            },
        )
    }

    /// Schedule a `NodeCrash` event for `node_id` after `delay` ticks.
    pub fn schedule_crash(&mut self, node_id: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NodeCrash { node: node_id })
    }

    /// Schedule a `NodeRecover` event for `node_id` after `delay` ticks.
    pub fn schedule_recover(&mut self, node_id: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NodeRecover { node: node_id })
    }

    /// Schedule a `NetworkPartition` between `a` and `b` after `delay` ticks.
    pub fn schedule_partition(&mut self, a: NodeId, b: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NetworkPartition { a, b })
    }

    /// Schedule a `NetworkHeal` between `a` and `b` after `delay` ticks.
    pub fn schedule_heal(&mut self, a: NodeId, b: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NetworkHeal { a, b })
    }
}
