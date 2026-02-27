//! `NodeRuntime` — owns all nodes and dispatches events to them.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::{Event, EventType};
use crate::eventlog::hash_combine;
use crate::network::{Network, NetworkDecision};
use crate::simulation::{EventHandler, SimulationContext};

use super::id::NodeId;
use super::payload::NodeEvent;
use super::trace::TraceEntry;
use super::traits::SimNode;

/// Manages a set of simulated nodes and dispatches events to them.
///
/// Implements [`EventHandler`] so it can be passed directly to
/// [`Simulation::run`]. Non-node events (`Noop`, `Log`) are silently
/// ignored.
///
/// When a [`Network`] is attached, `MessageSend` events are processed
/// through the network layer before delivery.
pub struct NodeRuntime {
    pub(crate) nodes: BTreeMap<NodeId, Box<dyn SimNode>>,
    pub(crate) alive: BTreeSet<NodeId>,
    /// Optional simulated network.
    pub(crate) network: Option<Network>,
    /// Append-only trace of every dispatched node event.
    pub trace: Vec<TraceEntry>,
}

impl NodeRuntime {
    /// Create a runtime without a network (direct delivery, zero latency).
    pub fn new() -> Self {
        NodeRuntime {
            nodes: BTreeMap::new(),
            alive: BTreeSet::new(),
            network: None,
            trace: Vec::new(),
        }
    }

    /// Create a runtime with a simulated network.
    pub fn with_network(network: Network) -> Self {
        NodeRuntime {
            nodes: BTreeMap::new(),
            alive: BTreeSet::new(),
            network: Some(network),
            trace: Vec::new(),
        }
    }

    /// Fork this runtime: deep-clone all nodes, the alive set, and the network.
    ///
    /// The forked runtime gets an empty trace — divergent execution
    /// starts fresh. This is the foundation for speculative branching
    /// and state-space exploration.
    pub fn fork(&self) -> Self {
        let cloned_nodes: BTreeMap<NodeId, Box<dyn SimNode>> = self
            .nodes
            .iter()
            .map(|(id, node)| (*id, node.clone_node()))
            .collect();
        NodeRuntime {
            nodes: cloned_nodes,
            alive: self.alive.clone(),
            network: self.network.clone(),
            trace: Vec::new(),
        }
    }

    /// Register a node. It starts in the *alive* state.
    pub fn register(&mut self, id: NodeId, node: Box<dyn SimNode>) {
        self.alive.insert(id);
        self.nodes.insert(id, node);
    }

    /// Check whether a node is currently alive.
    pub fn is_alive(&self, id: NodeId) -> bool {
        self.alive.contains(&id)
    }

    /// Number of registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Downcast a node reference for inspection.
    ///
    /// Returns `None` if the node is not registered or has a wrong type.
    pub fn node<T: SimNode + 'static>(&self, id: NodeId) -> Option<&T> {
        self.nodes.get(&id)?.as_any().downcast_ref::<T>()
    }

    /// Downcast a mutable node reference.
    pub fn node_mut<T: SimNode + 'static>(&mut self, id: NodeId) -> Option<&mut T> {
        self.nodes.get_mut(&id)?.as_any_mut().downcast_mut::<T>()
    }

    /// Access the attached network, if any.
    pub fn network(&self) -> Option<&Network> {
        self.network.as_ref()
    }

    /// Mutable access to the attached network.
    pub fn network_mut(&mut self) -> Option<&mut Network> {
        self.network.as_mut()
    }

    /// Return all registered node IDs in deterministic (sorted) order.
    pub fn all_node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }
}

impl Default for NodeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler for NodeRuntime {
    fn handle(&mut self, ctx: &mut SimulationContext, event: &Event) {
        match &event.payload {
            EventType::MessageDelivery { from, to, payload } => {
                let to_id = *to;
                // Silently drop messages to crashed nodes.
                if !self.alive.contains(&to_id) {
                    return;
                }
                if let Some(node) = self.nodes.get_mut(&to_id) {
                    let node_event = NodeEvent::Message {
                        from: *from,
                        payload: payload.clone(),
                    };
                    self.trace.push(TraceEntry {
                        time: ctx.now(),
                        event_id: event.id,
                        node: to_id,
                        node_event: node_event.clone(),
                    });
                    node.on_event(ctx, node_event);
                }
            }

            EventType::TimerFired { node, timer_id } => {
                let node_id = *node;
                if !self.alive.contains(&node_id) {
                    return;
                }
                if let Some(n) = self.nodes.get_mut(&node_id) {
                    let node_event = NodeEvent::TimerFired {
                        timer_id: *timer_id,
                    };
                    self.trace.push(TraceEntry {
                        time: ctx.now(),
                        event_id: event.id,
                        node: node_id,
                        node_event: node_event.clone(),
                    });
                    n.on_event(ctx, node_event);
                }
            }

            EventType::NodeCrash { node } => {
                let node_id = *node;
                self.alive.remove(&node_id);
                if let Some(n) = self.nodes.get_mut(&node_id) {
                    let node_event = NodeEvent::Crash;
                    self.trace.push(TraceEntry {
                        time: ctx.now(),
                        event_id: event.id,
                        node: node_id,
                        node_event: node_event.clone(),
                    });
                    n.on_event(ctx, node_event);
                }
            }

            EventType::NodeRecover { node } => {
                let node_id = *node;
                self.alive.insert(node_id);
                if let Some(n) = self.nodes.get_mut(&node_id) {
                    let node_event = NodeEvent::Recover;
                    self.trace.push(TraceEntry {
                        time: ctx.now(),
                        event_id: event.id,
                        node: node_id,
                        node_event: node_event.clone(),
                    });
                    n.on_event(ctx, node_event);
                }
            }

            EventType::MessageSend { from, to, payload } => {
                let from = *from;
                let to = *to;
                let payload = payload.clone();

                if let Some(ref mut network) = self.network {
                    let decision = network.process(ctx.now(), from, to);
                    match decision {
                        NetworkDecision::Delivered { latency } => {
                            ctx.schedule_after(
                                latency,
                                EventType::MessageDelivery { from, to, payload },
                            );
                        }
                        NetworkDecision::DroppedByChance
                        | NetworkDecision::DroppedByPartition => {
                            // Message lost — recorded in network.log().
                        }
                    }
                } else {
                    // No network → immediate delivery (zero extra latency).
                    ctx.schedule_after(
                        0,
                        EventType::MessageDelivery { from, to, payload },
                    );
                }
            }

            EventType::NetworkPartition { a, b } => {
                if let Some(ref mut network) = self.network {
                    network.add_partition(*a, *b);
                }
            }

            EventType::NetworkHeal { a, b } => {
                if let Some(ref mut network) = self.network {
                    network.remove_partition(*a, *b);
                }
            }

            // System-level events — not dispatched to nodes.
            EventType::Noop | EventType::Log(_) => {}
        }
    }

    fn compute_state_hash(&self) -> u64 {
        let mut h: u64 = 0;
        for (id, node) in &self.nodes {
            h = hash_combine(h, id.raw());
            h = hash_combine(h, node.state_hash());
        }
        // Include alive status in hash.
        for id in &self.alive {
            h = hash_combine(h, id.raw().wrapping_add(1));
        }
        h
    }
}
