/// Node abstraction and message passing for distributed simulation.
///
/// Introduces logical "nodes" that communicate exclusively through
/// events. Nodes never share memory or mutate global state — all
/// interaction is mediated by the deterministic scheduler.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::{Event, EventId, EventType};
use crate::simulation::{EventHandler, SimulationContext};
use crate::time::VirtualTime;

// ── NodeId ────────────────────────────────────────────────────────────

/// A unique identifier for a simulated node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u64);

impl NodeId {
    #[inline]
    pub fn new(id: u64) -> Self {
        NodeId(id)
    }

    #[inline]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "N{}", self.0)
    }
}

// ── MessagePayload ───────────────────────────────────────────────────

/// Opaque message payload carried between nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagePayload {
    /// Raw bytes.
    Data(Vec<u8>),
    /// Human-readable text (convenient for examples and tests).
    Text(String),
    /// Empty payload (e.g. heartbeats, acks).
    Empty,
}

// ── NodeEvent ─────────────────────────────────────────────────────────

/// The events that a node can receive.
///
/// These are *logical* events dispatched by `NodeRuntime`. The
/// underlying scheduler speaks `EventType`; `NodeRuntime` translates
/// the relevant variants into `NodeEvent` and delivers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEvent {
    /// A message from another node.
    Message {
        from: NodeId,
        payload: MessagePayload,
    },
    /// A previously scheduled timer has fired.
    TimerFired { timer_id: u64 },
    /// This node has been crashed by the simulation.
    Crash,
    /// This node has recovered from a crash.
    Recover,
}

// ── SimNode trait ─────────────────────────────────────────────────────

/// Trait implemented by every simulated node.
///
/// Nodes react to events via `on_event` and may schedule follow-up
/// events through the provided `SimulationContext`.
///
/// # Contract
/// - Implementations must **not** use global mutable state.
/// - All side effects must go through `ctx`.
/// - The method must be deterministic given the same inputs.
pub trait SimNode {
    /// React to a dispatched event.
    fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent);

    /// Downcast support for test inspection.
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

// ── SimulationContext node extensions ─────────────────────────────────

impl SimulationContext<'_> {
    /// Schedule a message from `from` to `to` with the given `delay`.
    pub fn schedule_message(
        &mut self,
        from: NodeId,
        to: NodeId,
        delay: u64,
        payload: MessagePayload,
    ) -> EventId {
        self.schedule_after(
            delay,
            EventType::MessageDelivery { from, to, payload },
        )
    }

    /// Schedule a timer for `node_id` to fire after `delay` ticks.
    ///
    /// Returns the `EventId`, which also serves as the `timer_id`
    /// delivered to the node in `NodeEvent::TimerFired`.
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

    /// Schedule a crash event for `node_id` after `delay` ticks.
    pub fn schedule_crash(&mut self, node_id: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NodeCrash { node: node_id })
    }

    /// Schedule a recovery event for `node_id` after `delay` ticks.
    pub fn schedule_recover(&mut self, node_id: NodeId, delay: u64) -> EventId {
        self.schedule_after(delay, EventType::NodeRecover { node: node_id })
    }
}

// ── Trace Entry ───────────────────────────────────────────────────────

/// A record of an event dispatched to a node — useful for test assertions.
#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub time: VirtualTime,
    pub event_id: EventId,
    pub node: NodeId,
    pub node_event: NodeEvent,
}

// ── NodeRuntime ───────────────────────────────────────────────────────

/// Manages a set of simulated nodes and dispatches events to them.
///
/// Implements `EventHandler` so it can be passed directly to
/// `Simulation::run`. Non-node events (`Noop`, `Log`) are silently
/// ignored.
pub struct NodeRuntime {
    nodes: BTreeMap<NodeId, Box<dyn SimNode>>,
    alive: BTreeSet<NodeId>,
    /// Append-only trace of every dispatched node event.
    pub trace: Vec<TraceEntry>,
}

impl NodeRuntime {
    pub fn new() -> Self {
        NodeRuntime {
            nodes: BTreeMap::new(),
            alive: BTreeSet::new(),
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

    /// Downcast a node reference for test inspection.
    pub fn node<T: SimNode + 'static>(&self, id: NodeId) -> Option<&T> {
        self.nodes.get(&id)?.as_any().downcast_ref::<T>()
    }

    /// Downcast a mutable node reference.
    pub fn node_mut<T: SimNode + 'static>(&mut self, id: NodeId) -> Option<&mut T> {
        self.nodes.get_mut(&id)?.as_any_mut().downcast_mut::<T>()
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
                        time: ctx.now,
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
                        time: ctx.now,
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
                        time: ctx.now,
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
                        time: ctx.now,
                        event_id: event.id,
                        node: node_id,
                        node_event: node_event.clone(),
                    });
                    n.on_event(ctx, node_event);
                }
            }

            // System-level events — not dispatched to nodes.
            EventType::Noop | EventType::Log(_) => {}
        }
    }
}

// ── Example: EchoNode ─────────────────────────────────────────────────

/// A simple node that echoes back every message to its sender.
///
/// Useful for testing message round-trips.
pub struct EchoNode {
    pub id: NodeId,
    pub echo_count: u64,
}

impl EchoNode {
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
}

// ── Example: PingNode ─────────────────────────────────────────────────

/// A node that records all received messages (for test assertions).
pub struct PingNode {
    pub id: NodeId,
    pub received: Vec<(VirtualTime, NodeId, MessagePayload)>,
}

impl PingNode {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Simulation;

    #[test]
    fn test_echo_round_trip() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        // n0 sends a message to n1 at T=0.
        sim.schedule(
            VirtualTime::new(0),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("hello".into()),
            },
        );

        sim.run(&mut rt);

        // n1 echoes back at T=1, n0 receives it.
        let ping = rt.node::<PingNode>(n0).unwrap();
        assert_eq!(ping.received.len(), 1);
        assert_eq!(ping.received[0].0, VirtualTime::new(1));
        assert_eq!(ping.received[0].1, n1);
        assert_eq!(
            ping.received[0].2,
            MessagePayload::Text("hello".into())
        );

        let echo = rt.node::<EchoNode>(n1).unwrap();
        assert_eq!(echo.echo_count, 1);
    }

    #[test]
    fn test_three_node_simulation() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);
        let n2 = NodeId::new(2);

        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));
        rt.register(n2, Box::new(EchoNode::new(n2)));

        // n0 sends to n1 and n2 at T=0.
        sim.schedule(
            VirtualTime::new(0),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("ping-1".into()),
            },
        );
        sim.schedule(
            VirtualTime::new(0),
            EventType::MessageDelivery {
                from: n0,
                to: n2,
                payload: MessagePayload::Text("ping-2".into()),
            },
        );

        sim.run(&mut rt);

        // n0 should have received two echoes at T=1.
        let ping = rt.node::<PingNode>(n0).unwrap();
        assert_eq!(ping.received.len(), 2);

        // Both echoes arrive at T=1.
        assert_eq!(ping.received[0].0, VirtualTime::new(1));
        assert_eq!(ping.received[1].0, VirtualTime::new(1));

        // Echoes come from n1 and n2 (in event-ID order).
        assert_eq!(ping.received[0].1, n1);
        assert_eq!(ping.received[1].1, n2);

        // Payloads preserved.
        assert_eq!(
            ping.received[0].2,
            MessagePayload::Text("ping-1".into())
        );
        assert_eq!(
            ping.received[1].2,
            MessagePayload::Text("ping-2".into())
        );

        // Trace should have 4 entries: 2 deliveries to echo nodes + 2 echoes to ping node.
        assert_eq!(rt.trace.len(), 4);
    }

    #[test]
    fn test_crash_drops_messages() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        // Crash n1 at T=5, send message to n1 at T=10.
        sim.schedule(VirtualTime::new(5), EventType::NodeCrash { node: n1 });
        sim.schedule(
            VirtualTime::new(10),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("lost".into()),
            },
        );

        sim.run(&mut rt);

        // n1 never echoed → n0 received nothing.
        let ping = rt.node::<PingNode>(n0).unwrap();
        assert_eq!(ping.received.len(), 0);

        // n1 is crashed.
        assert!(!rt.is_alive(n1));
    }

    #[test]
    fn test_crash_and_recover() {
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));

        // Crash n1 at T=5, recover at T=15, send message at T=20.
        sim.schedule(VirtualTime::new(5), EventType::NodeCrash { node: n1 });
        sim.schedule(VirtualTime::new(15), EventType::NodeRecover { node: n1 });
        sim.schedule(
            VirtualTime::new(20),
            EventType::MessageDelivery {
                from: n0,
                to: n1,
                payload: MessagePayload::Text("after-recovery".into()),
            },
        );

        sim.run(&mut rt);

        // n1 recovered → echo delivered → n0 received it.
        let ping = rt.node::<PingNode>(n0).unwrap();
        assert_eq!(ping.received.len(), 1);
        assert_eq!(ping.received[0].0, VirtualTime::new(21)); // echo at T=20+1
        assert!(rt.is_alive(n1));
    }

    #[test]
    fn test_timer_fires() {
        /// A node that schedules a self-timer on first message,
        /// and records when the timer fires.
        struct TimerNode {
            id: NodeId,
            timer_fired_at: Option<VirtualTime>,
        }

        impl SimNode for TimerNode {
            fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent) {
                match event {
                    NodeEvent::Message { .. } => {
                        ctx.schedule_timer(self.id, 10);
                    }
                    NodeEvent::TimerFired { .. } => {
                        self.timer_fired_at = Some(ctx.now());
                    }
                    _ => {}
                }
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }
        }

        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::new();

        let n0 = NodeId::new(0);
        rt.register(
            n0,
            Box::new(TimerNode {
                id: n0,
                timer_fired_at: None,
            }),
        );

        // Trigger with a message at T=5 → timer should fire at T=15.
        sim.schedule(
            VirtualTime::new(5),
            EventType::MessageDelivery {
                from: NodeId::new(99),
                to: n0,
                payload: MessagePayload::Empty,
            },
        );

        sim.run(&mut rt);

        let node = rt.node::<TimerNode>(n0).unwrap();
        assert_eq!(node.timer_fired_at, Some(VirtualTime::new(15)));
    }

    #[test]
    fn test_deterministic_three_node_replay() {
        fn run_trace() -> Vec<(u64, u64, NodeId)> {
            let mut sim = Simulation::new();
            let mut rt = NodeRuntime::new();

            let n0 = NodeId::new(0);
            let n1 = NodeId::new(1);
            let n2 = NodeId::new(2);

            rt.register(n0, Box::new(PingNode::new(n0)));
            rt.register(n1, Box::new(EchoNode::new(n1)));
            rt.register(n2, Box::new(EchoNode::new(n2)));

            sim.schedule(
                VirtualTime::new(0),
                EventType::MessageDelivery {
                    from: n0,
                    to: n1,
                    payload: MessagePayload::Text("a".into()),
                },
            );
            sim.schedule(
                VirtualTime::new(0),
                EventType::MessageDelivery {
                    from: n0,
                    to: n2,
                    payload: MessagePayload::Text("b".into()),
                },
            );
            sim.schedule(
                VirtualTime::new(3),
                EventType::MessageDelivery {
                    from: n0,
                    to: n2,
                    payload: MessagePayload::Text("c".into()),
                },
            );

            sim.run(&mut rt);

            rt.trace
                .iter()
                .map(|t| (t.time.ticks(), t.event_id.raw(), t.node))
                .collect()
        }

        let run1 = run_trace();
        let run2 = run_trace();
        assert_eq!(run1, run2, "3-node simulation is not deterministic!");
    }
}
