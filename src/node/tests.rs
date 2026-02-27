//! Integration tests for `NodeRuntime` and built-in nodes.
//!
//! All tests from the original `node.rs` live here, organised by batch.

use crate::event::EventType;
use crate::node::{EchoNode, MessagePayload, NodeId, NodeRuntime, PingNode};
use crate::simulation::Simulation;
use crate::time::VirtualTime;

// ── Basic dispatch tests ──────────────────────────────────────────────

#[test]
fn test_echo_round_trip() {
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::new();

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("hello".into()),
        },
    );

    sim.run(&mut rt);

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 1);
    assert_eq!(ping.received[0].0, VirtualTime::new(1));
    assert_eq!(ping.received[0].1, n1);
    assert_eq!(ping.received[0].2, MessagePayload::Text("hello".into()));

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

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 2);
    assert_eq!(ping.received[0].0, VirtualTime::new(1));
    assert_eq!(ping.received[1].0, VirtualTime::new(1));
    assert_eq!(ping.received[0].1, n1);
    assert_eq!(ping.received[1].1, n2);
    assert_eq!(ping.received[0].2, MessagePayload::Text("ping-1".into()));
    assert_eq!(ping.received[1].2, MessagePayload::Text("ping-2".into()));
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

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 0);
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

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 1);
    assert_eq!(ping.received[0].0, VirtualTime::new(21));
    assert!(rt.is_alive(n1));
}

#[test]
fn test_timer_fires() {
    use crate::node::traits::SimNode;
    use crate::node::payload::NodeEvent;
    use crate::simulation::SimulationContext;

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
        fn clone_node(&self) -> Box<dyn SimNode> {
            Box::new(TimerNode {
                id: self.id,
                timer_fired_at: self.timer_fired_at,
            })
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

        for (to, payload, time) in [
            (n1, "a", 0u64),
            (n2, "b", 0u64),
            (n2, "c", 3u64),
        ] {
            sim.schedule(
                VirtualTime::new(time),
                EventType::MessageDelivery {
                    from: n0,
                    to,
                    payload: MessagePayload::Text(payload.into()),
                },
            );
        }

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

// ── Batch 3: Network integration ──────────────────────────────────────

#[test]
fn test_send_through_network() {
    use crate::network::{Network, NetworkConfig};

    let net = Network::new(NetworkConfig::reliable(), 42);
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("net-hello".into()),
        },
    );

    sim.run(&mut rt);

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 1);
    assert_eq!(ping.received[0].0, VirtualTime::new(2));
    assert_eq!(ping.received[0].2, MessagePayload::Text("net-hello".into()));
}

#[test]
fn test_network_partition_blocks_messages() {
    use crate::network::{Network, NetworkConfig};

    let mut net = Network::new(NetworkConfig::reliable(), 42);
    net.add_partition(NodeId::new(0), NodeId::new(1));

    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("blocked".into()),
        },
    );

    sim.run(&mut rt);

    let echo = rt.node::<EchoNode>(n1).unwrap();
    assert_eq!(echo.echo_count, 0);
    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 0);
    assert_eq!(rt.network().unwrap().dropped_count(), 1);
}

#[test]
fn test_dynamic_partition_and_heal() {
    use crate::network::{Network, NetworkConfig};

    let net = Network::new(NetworkConfig::reliable(), 42);
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(VirtualTime::new(0),  EventType::MessageSend { from: n0, to: n1, payload: MessagePayload::Text("before".into()) });
    sim.schedule(VirtualTime::new(5),  EventType::NetworkPartition { a: n0, b: n1 });
    sim.schedule(VirtualTime::new(10), EventType::MessageSend { from: n0, to: n1, payload: MessagePayload::Text("during".into()) });
    sim.schedule(VirtualTime::new(15), EventType::NetworkHeal { a: n0, b: n1 });
    sim.schedule(VirtualTime::new(20), EventType::MessageSend { from: n0, to: n1, payload: MessagePayload::Text("after".into()) });

    sim.run(&mut rt);

    let ping = rt.node::<PingNode>(n0).unwrap();
    assert_eq!(ping.received.len(), 2);
    let net = rt.network().unwrap();
    assert_eq!(net.delivered_count(), 2);
    assert_eq!(net.dropped_count(), 1);
}

#[test]
fn test_network_reproducibility() {
    use crate::network::{Network, NetworkConfig};

    fn run_with_chaos() -> Vec<(u64, u64, NodeId)> {
        let net = Network::new(NetworkConfig::lossy(3, 5, 0.3), 12345);
        let mut sim = Simulation::new();
        let mut rt = NodeRuntime::with_network(net);

        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);
        let n2 = NodeId::new(2);

        rt.register(n0, Box::new(PingNode::new(n0)));
        rt.register(n1, Box::new(EchoNode::new(n1)));
        rt.register(n2, Box::new(EchoNode::new(n2)));

        for i in 0..20 {
            sim.schedule(
                VirtualTime::new(i * 5),
                EventType::MessageSend {
                    from: n0,
                    to: if i % 2 == 0 { n1 } else { n2 },
                    payload: MessagePayload::Text(format!("m{}", i)),
                },
            );
        }

        sim.schedule(VirtualTime::new(30), EventType::NetworkPartition { a: n0, b: n1 });
        sim.schedule(VirtualTime::new(60), EventType::NetworkHeal { a: n0, b: n1 });

        sim.run(&mut rt);

        rt.trace
            .iter()
            .map(|t| (t.time.ticks(), t.event_id.raw(), t.node))
            .collect()
    }

    let run1 = run_with_chaos();
    let run2 = run_with_chaos();
    assert_eq!(run1, run2, "Network chaos simulation is not deterministic!");
    assert!(!run1.is_empty(), "Should have processed some events");
}

// ── Batch 5: Fork tests ───────────────────────────────────────────────

#[test]
fn test_fork_basic() {
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::new();

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("before-fork".into()),
        },
    );
    sim.run(&mut rt);

    assert_eq!(rt.node::<PingNode>(n0).unwrap().received.len(), 1);

    let mut sim_fork = sim.fork();
    let mut rt_fork = rt.fork();

    assert_eq!(rt_fork.node::<PingNode>(n0).unwrap().received.len(), 1);
    assert!(rt_fork.trace.is_empty());

    sim_fork.schedule(
        VirtualTime::new(5),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("fork-only".into()),
        },
    );
    sim_fork.run(&mut rt_fork);

    assert_eq!(rt_fork.node::<PingNode>(n0).unwrap().received.len(), 2);
    assert_eq!(rt.node::<PingNode>(n0).unwrap().received.len(), 1);
}

#[test]
fn test_fork_divergent_execution() {
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::new();

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(5),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("shared".into()),
        },
    );

    let mut sim_a = sim.fork();
    let mut rt_a = rt.fork();
    let mut sim_b = sim.fork();
    let mut rt_b = rt.fork();

    sim_a.schedule(VirtualTime::new(0), EventType::NodeCrash { node: n1 });

    sim_a.run(&mut rt_a);
    sim_b.run(&mut rt_b);

    assert_eq!(rt_a.node::<PingNode>(n0).unwrap().received.len(), 0);
    assert_eq!(rt_b.node::<PingNode>(n0).unwrap().received.len(), 1);
}

#[test]
fn test_fork_with_network() {
    use crate::network::{Network, NetworkConfig};

    let net = Network::new(NetworkConfig::reliable(), 42);
    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("pre-fork".into()),
        },
    );
    sim.run(&mut rt);

    let mut sim_fork = sim.fork();
    let mut rt_fork = rt.fork();

    rt_fork.network_mut().unwrap().add_partition(n0, n1);

    sim.schedule(VirtualTime::new(10), EventType::MessageSend { from: n0, to: n1, payload: MessagePayload::Text("after".into()) });
    sim_fork.schedule(VirtualTime::new(10), EventType::MessageSend { from: n0, to: n1, payload: MessagePayload::Text("after".into()) });

    sim.run(&mut rt);
    sim_fork.run(&mut rt_fork);

    assert_eq!(rt.node::<PingNode>(n0).unwrap().received.len(), 2);
    assert_eq!(rt_fork.node::<PingNode>(n0).unwrap().received.len(), 1);
}

#[test]
fn test_fork_log_hash_divergence() {
    let mut sim = Simulation::new();
    sim.enable_logging();
    let mut rt = NodeRuntime::new();

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));

    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("seed".into()),
        },
    );
    sim.run(&mut rt);

    let _hash_pre_fork = sim.event_log().unwrap().log_hash();

    let mut sim_a = sim.fork();
    let mut rt_a = rt.fork();
    sim_a.schedule(VirtualTime::new(10), EventType::MessageDelivery {
        from: n0, to: n1, payload: MessagePayload::Text("branch-a".into()),
    });
    sim_a.run(&mut rt_a);

    let mut sim_b = sim.fork();
    let mut rt_b = rt.fork();
    sim_b.schedule(VirtualTime::new(10), EventType::MessageDelivery {
        from: n0, to: n1, payload: MessagePayload::Text("branch-b".into()),
    });
    sim_b.run(&mut rt_b);

    let hash_a = sim_a.event_log().unwrap().log_hash();
    let hash_b = sim_b.event_log().unwrap().log_hash();

    assert_ne!(hash_a, hash_b, "Divergent branches should have different log hashes");
    assert_eq!(sim_a.event_log().unwrap().len(), 4);
    assert_eq!(sim_b.event_log().unwrap().len(), 4);
}
