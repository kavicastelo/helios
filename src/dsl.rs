/// Fluent builder DSL for simulation setup and scenario exploration.
///
/// Provides ergonomic APIs that hide the boilerplate of creating
/// simulations, registering nodes, scheduling events, and configuring
/// the explorer — all while preserving full determinism.

use crate::event::EventType;
use crate::explorer::{Choice, ExplorationResult, Explorer, NamedProperty};
use crate::network::{Network, NetworkConfig};
use crate::node::{
    EchoNode, MessagePayload, NodeId, NodeRuntime, PingNode, SimNode,
};
use crate::simulation::Simulation;
use crate::time::VirtualTime;

// ── SimulationBuilder ─────────────────────────────────────────────────

/// Fluent builder for constructing a `Simulation` + `NodeRuntime` pair.
///
/// # Example
/// ```rust
/// use helios::dsl::SimulationBuilder;
///
/// let (sim, rt) = SimulationBuilder::new()
///     .ping(0)
///     .echo(1)
///     .echo(2)
///     .lossy_network(3, 2, 0.2, 42)
///     .with_checkpoints(5)
///     .deliver(0, 1, "hello", 0)
///     .crash(1, 10)
///     .build();
/// ```
pub struct SimulationBuilder {
    network: Option<Network>,
    nodes: Vec<(NodeId, Box<dyn SimNode>)>,
    events: Vec<(VirtualTime, EventType)>,
    logging: LoggingConfig,
}

#[derive(Clone, Copy)]
enum LoggingConfig {
    Off,
    On,
    WithCheckpoints(u64),
}

impl SimulationBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        SimulationBuilder {
            network: None,
            nodes: Vec::new(),
            events: Vec::new(),
            logging: LoggingConfig::Off,
        }
    }

    // ── Nodes ─────────────────────────────────────────────────

    /// Register a custom node.
    pub fn node(mut self, id: u64, node: Box<dyn SimNode>) -> Self {
        self.nodes.push((NodeId::new(id), node));
        self
    }

    /// Register a custom node using a factory function.
    pub fn node_with<F, N>(mut self, id: u64, factory: F) -> Self
    where
        F: FnOnce(NodeId) -> N,
        N: SimNode + 'static,
    {
        let nid = NodeId::new(id);
        self.nodes.push((nid, Box::new(factory(nid))));
        self
    }

    /// Register a `PingNode` (records all received messages).
    pub fn ping(self, id: u64) -> Self {
        let nid = NodeId::new(id);
        self.node(id, Box::new(PingNode::new(nid)))
    }

    /// Register an `EchoNode` (echoes every message back).
    pub fn echo(self, id: u64) -> Self {
        let nid = NodeId::new(id);
        self.node(id, Box::new(EchoNode::new(nid)))
    }

    // ── Network ───────────────────────────────────────────────

    /// Attach a reliable network (no drops, base latency = 1).
    pub fn reliable_network(mut self, seed: u64) -> Self {
        self.network = Some(Network::new(NetworkConfig::reliable(), seed));
        self
    }

    /// Attach a lossy network.
    pub fn lossy_network(
        mut self,
        base_latency: u64,
        jitter: u64,
        drop_rate: f64,
        seed: u64,
    ) -> Self {
        self.network = Some(Network::new(
            NetworkConfig::lossy(base_latency, jitter, drop_rate),
            seed,
        ));
        self
    }

    /// Attach a custom network config.
    pub fn network(mut self, config: NetworkConfig, seed: u64) -> Self {
        self.network = Some(Network::new(config, seed));
        self
    }

    // ── Logging ───────────────────────────────────────────────

    /// Enable event logging.
    pub fn with_logging(mut self) -> Self {
        self.logging = LoggingConfig::On;
        self
    }

    /// Enable event logging with checkpoint interval.
    pub fn with_checkpoints(mut self, interval: u64) -> Self {
        self.logging = LoggingConfig::WithCheckpoints(interval);
        self
    }

    // ── Events ────────────────────────────────────────────────

    /// Schedule a `MessageSend` (two-phase, through network).
    pub fn send(mut self, from: u64, to: u64, msg: &str, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::MessageSend {
                from: NodeId::new(from),
                to: NodeId::new(to),
                payload: MessagePayload::Text(msg.into()),
            },
        ));
        self
    }

    /// Schedule a direct `MessageDelivery` (bypasses network).
    pub fn deliver(mut self, from: u64, to: u64, msg: &str, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::MessageDelivery {
                from: NodeId::new(from),
                to: NodeId::new(to),
                payload: MessagePayload::Text(msg.into()),
            },
        ));
        self
    }

    /// Schedule a `MessageDelivery` with binary data.
    pub fn deliver_data(
        mut self,
        from: u64,
        to: u64,
        data: Vec<u8>,
        at: u64,
    ) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::MessageDelivery {
                from: NodeId::new(from),
                to: NodeId::new(to),
                payload: MessagePayload::Data(data),
            },
        ));
        self
    }

    /// Schedule a `NodeCrash` event.
    pub fn crash(mut self, node: u64, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::NodeCrash {
                node: NodeId::new(node),
            },
        ));
        self
    }

    /// Schedule a `NodeRecover` event.
    pub fn recover(mut self, node: u64, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::NodeRecover {
                node: NodeId::new(node),
            },
        ));
        self
    }

    /// Schedule a `NetworkPartition` event.
    pub fn partition(mut self, a: u64, b: u64, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::NetworkPartition {
                a: NodeId::new(a),
                b: NodeId::new(b),
            },
        ));
        self
    }

    /// Schedule a `NetworkHeal` event.
    pub fn heal(mut self, a: u64, b: u64, at: u64) -> Self {
        self.events.push((
            VirtualTime::new(at),
            EventType::NetworkHeal {
                a: NodeId::new(a),
                b: NodeId::new(b),
            },
        ));
        self
    }

    /// Schedule a raw event.
    pub fn event(mut self, at: u64, event: EventType) -> Self {
        self.events.push((VirtualTime::new(at), event));
        self
    }

    // ── Build ─────────────────────────────────────────────────

    /// Build and return `(Simulation, NodeRuntime)`.
    pub fn build(self) -> (Simulation, NodeRuntime) {
        let mut sim = Simulation::new();
        let mut rt = match self.network {
            Some(net) => NodeRuntime::with_network(net),
            None => NodeRuntime::new(),
        };

        match self.logging {
            LoggingConfig::Off => {}
            LoggingConfig::On => sim.enable_logging(),
            LoggingConfig::WithCheckpoints(n) => {
                sim.enable_logging_with_checkpoints(n)
            }
        }

        for (id, node) in self.nodes {
            rt.register(id, node);
        }

        for (time, event) in self.events {
            sim.schedule(time, event);
        }

        (sim, rt)
    }

    /// Build, run to completion, return `(Simulation, NodeRuntime, u64)`.
    /// The `u64` is the number of events processed.
    pub fn run(self) -> (Simulation, NodeRuntime, u64) {
        let (mut sim, mut rt) = self.build();
        let n = sim.run(&mut rt);
        (sim, rt, n)
    }
}

impl Default for SimulationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── ScenarioBuilder ───────────────────────────────────────────────────

/// Fluent builder for state-space exploration scenarios.
///
/// Wraps `Explorer` with a concise API for defining choices and
/// properties, then runs exploration.
///
/// # Example
/// ```rust
/// use helios::dsl::{SimulationBuilder, ScenarioBuilder};
///
/// let (sim, rt) = SimulationBuilder::new()
///     .ping(0).echo(1)
///     .deliver(0, 1, "hello", 10)
///     .build();
///
/// let result = ScenarioBuilder::new(sim, rt)
///     .maybe_crash(1, 5)
///     .maybe_partition(0, 1, 3)
///     .assert("n0 gets echo", |_sim, rt| {
///         let ping = rt.node::<helios::PingNode>(helios::NodeId::new(0)).unwrap();
///         if ping.received.is_empty() { Err("no echo".into()) }
///         else { Ok(()) }
///     })
///     .explore();
/// ```
pub struct ScenarioBuilder {
    sim: Simulation,
    rt: NodeRuntime,
    choices: Vec<Choice>,
    properties: Vec<Box<dyn crate::explorer::Property>>,
    max_branches: usize,
}

impl ScenarioBuilder {
    /// Create a new scenario from a base simulation state.
    pub fn new(sim: Simulation, rt: NodeRuntime) -> Self {
        ScenarioBuilder {
            sim,
            rt,
            choices: Vec::new(),
            properties: Vec::new(),
            max_branches: 10_000,
        }
    }

    // ── Choices ───────────────────────────────────────────────

    /// Add a binary choice: crash a node or skip.
    pub fn maybe_crash(mut self, node: u64, at: u64) -> Self {
        self.choices.push(Choice::binary(
            &format!("crash N{} at T={}", node, at),
            VirtualTime::new(at),
            EventType::NodeCrash {
                node: NodeId::new(node),
            },
        ));
        self
    }

    /// Add a binary choice: partition two nodes or skip.
    pub fn maybe_partition(mut self, a: u64, b: u64, at: u64) -> Self {
        self.choices.push(Choice::binary(
            &format!("partition N{}↔N{} at T={}", a, b, at),
            VirtualTime::new(at),
            EventType::NetworkPartition {
                a: NodeId::new(a),
                b: NodeId::new(b),
            },
        ));
        self
    }

    /// Add a binary choice: heal a partition or skip.
    pub fn maybe_heal(mut self, a: u64, b: u64, at: u64) -> Self {
        self.choices.push(Choice::binary(
            &format!("heal N{}↔N{} at T={}", a, b, at),
            VirtualTime::new(at),
            EventType::NetworkHeal {
                a: NodeId::new(a),
                b: NodeId::new(b),
            },
        ));
        self
    }

    /// Add a binary choice: send a message or skip.
    pub fn maybe_send(mut self, from: u64, to: u64, msg: &str, at: u64) -> Self {
        self.choices.push(Choice::binary(
            &format!("send \"{}\" N{}→N{} at T={}", msg, from, to, at),
            VirtualTime::new(at),
            EventType::MessageSend {
                from: NodeId::new(from),
                to: NodeId::new(to),
                payload: MessagePayload::Text(msg.into()),
            },
        ));
        self
    }

    /// Add a binary choice: deliver a message or skip.
    pub fn maybe_deliver(
        mut self,
        from: u64,
        to: u64,
        msg: &str,
        at: u64,
    ) -> Self {
        self.choices.push(Choice::binary(
            &format!("deliver \"{}\" N{}→N{} at T={}", msg, from, to, at),
            VirtualTime::new(at),
            EventType::MessageDelivery {
                from: NodeId::new(from),
                to: NodeId::new(to),
                payload: MessagePayload::Text(msg.into()),
            },
        ));
        self
    }

    /// Add a custom choice.
    pub fn choice(mut self, choice: Choice) -> Self {
        self.choices.push(choice);
        self
    }

    // ── Properties ────────────────────────────────────────────

    /// Add a closure-based safety property.
    pub fn assert<F>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(&Simulation, &NodeRuntime) -> Result<(), String> + 'static,
    {
        self.properties
            .push(Box::new(NamedProperty::new(name, f)));
        self
    }

    /// Set max branches to explore.
    pub fn max_branches(mut self, n: usize) -> Self {
        self.max_branches = n;
        self
    }

    // ── Run ───────────────────────────────────────────────────

    /// Compute total branches.
    pub fn total_branches(&self) -> usize {
        if self.choices.is_empty() {
            1
        } else {
            self.choices.iter().map(|c| c.options.len()).product()
        }
    }

    /// Run the exploration and return results.
    pub fn explore(self) -> ExplorationResult {
        let mut explorer = Explorer::new(self.sim, self.rt);
        for choice in self.choices {
            explorer.add_choice(choice);
        }
        for prop in self.properties {
            explorer.add_property(prop);
        }
        explorer.set_max_branches(self.max_branches);
        explorer.explore()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let (mut sim, mut rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        sim.run(&mut rt);

        // PingNode 0 should have received the echo.
        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    #[test]
    fn test_builder_with_network() {
        let (mut sim, mut rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .reliable_network(42)
            .send(0, 1, "hello", 0)
            .build();

        sim.run(&mut rt);

        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    #[test]
    fn test_builder_with_logging() {
        let (sim, _, n) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .with_checkpoints(5)
            .deliver(0, 1, "hello", 0)
            .run();

        assert!(n > 0);
        let log = sim.event_log().unwrap();
        assert!(!log.is_empty());
    }

    #[test]
    fn test_builder_crash_and_recover() {
        let (_, rt, _) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 10)
            .crash(1, 5)
            .recover(1, 15)
            .deliver(0, 1, "after-recover", 20)
            .run();

        // First msg dropped (crash at T=5 before T=10).
        // Second msg after recover at T=15 should go through.
        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    #[test]
    fn test_builder_partition_and_heal() {
        let (_, rt, _) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "before", 0)
            .partition(0, 1, 5)
            .heal(0, 1, 15)
            .run();

        // MessageDelivery bypasses network, so partition doesn't affect it.
        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    #[test]
    fn test_builder_run_shorthand() {
        let (_, rt, events) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .run();

        assert_eq!(events, 2); // deliver + echo
        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    #[test]
    fn test_builder_multi_node() {
        let (_, rt, _) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .echo(2)
            .echo(3)
            .deliver(0, 1, "m1", 0)
            .deliver(0, 2, "m2", 0)
            .deliver(0, 3, "m3", 0)
            .run();

        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 3);
    }

    #[test]
    fn test_builder_node_with_factory() {
        let (_, rt, _) = SimulationBuilder::new()
            .node_with(0, |id| PingNode::new(id))
            .node_with(1, |id| EchoNode::new(id))
            .deliver(0, 1, "hello", 0)
            .run();

        let ping = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        assert_eq!(ping.received.len(), 1);
    }

    // ── ScenarioBuilder tests ─────────────────────────────────

    #[test]
    fn test_scenario_no_violations() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        let result = ScenarioBuilder::new(sim, rt)
            .assert("has echo", |_sim, rt| {
                let p = rt.node::<PingNode>(NodeId::new(0)).unwrap();
                if p.received.len() >= 1 {
                    Ok(())
                } else {
                    Err("no echo".into())
                }
            })
            .explore();

        assert!(result.is_safe());
        assert_eq!(result.branches_explored, 1);
    }

    #[test]
    fn test_scenario_crash_violation() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 10)
            .build();

        let result = ScenarioBuilder::new(sim, rt)
            .maybe_crash(1, 5)
            .assert("has echo", |_sim, rt| {
                let p = rt.node::<PingNode>(NodeId::new(0)).unwrap();
                if p.received.is_empty() {
                    Err("no echo".into())
                } else {
                    Ok(())
                }
            })
            .explore();

        assert_eq!(result.branches_explored, 2);
        assert_eq!(result.violation_count(), 1);
    }

    #[test]
    fn test_scenario_combined_faults() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .echo(2)
            .deliver(0, 1, "m1", 10)
            .deliver(0, 2, "m2", 10)
            .build();

        let result = ScenarioBuilder::new(sim, rt)
            .maybe_crash(1, 5)
            .maybe_crash(2, 5)
            .assert("at least one echo", |_sim, rt| {
                let p = rt.node::<PingNode>(NodeId::new(0)).unwrap();
                if p.received.is_empty() {
                    Err("no echoes".into())
                } else {
                    Ok(())
                }
            })
            .explore();

        // 4 branches: (skip,skip), (crash1,skip), (skip,crash2), (crash1,crash2)
        assert_eq!(result.branches_explored, 4);
        // Only the (crash1, crash2) branch should fail.
        assert_eq!(result.violation_count(), 1);
    }

    #[test]
    fn test_scenario_max_branches() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 10)
            .build();

        let result = ScenarioBuilder::new(sim, rt)
            .maybe_crash(1, 5)
            .maybe_crash(1, 8)
            .maybe_crash(1, 12)
            .max_branches(4)
            .assert("trivial", |_, _| Ok(()))
            .explore();

        // 2^3 = 8, but limited to 4.
        assert_eq!(result.branches_explored, 4);
    }

    #[test]
    fn test_full_dsl_pipeline() {
        // Build → Explore → Verify — all in one fluent chain.
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .echo(2)
            .reliable_network(42)
            .with_logging()
            .send(0, 1, "msg-1", 10)
            .send(0, 2, "msg-2", 10)
            .build();

        let result = ScenarioBuilder::new(sim, rt)
            .maybe_crash(1, 5)
            .maybe_crash(2, 5)
            .assert("liveness", |_sim, rt| {
                let p = rt.node::<PingNode>(NodeId::new(0)).unwrap();
                if p.received.is_empty() {
                    Err("dead".into())
                } else {
                    Ok(())
                }
            })
            .explore();

        assert_eq!(result.branches_explored, 4);
        // Crash both → no echoes → violation
        assert_eq!(result.violation_count(), 1);
    }
}
