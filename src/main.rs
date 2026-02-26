use helios::{
    EchoNode, EventType, MessagePayload, Network, NetworkConfig,
    NodeId, NodeRuntime, PingNode, Simulation, VirtualTime,
};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 5: Forkable Execution Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // ── Setup a simulation with 3 nodes ───────────────────────
    let net = Network::new(NetworkConfig::reliable(), 42);
    let mut sim = Simulation::new();
    sim.enable_logging();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    let n2 = NodeId::new(2);
    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));
    rt.register(n2, Box::new(EchoNode::new(n2)));

    // Send initial messages.
    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("hello-n1".into()),
        },
    );
    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("hello-n2".into()),
        },
    );
    sim.run(&mut rt);

    let pre_fork_msgs = rt.node::<PingNode>(n0).unwrap().received.len();
    println!("  Pre-fork: N0 received {} echoes", pre_fork_msgs);
    println!("  Forking into two branches...");
    println!();

    // ── Branch A: normal operation ────────────────────────────
    let mut sim_a = sim.fork();
    let mut rt_a = rt.fork();
    sim_a.schedule(
        VirtualTime::new(10),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("branch-a-msg".into()),
        },
    );
    sim_a.run(&mut rt_a);

    let a_msgs = rt_a.node::<PingNode>(n0).unwrap().received.len();
    let a_events = sim_a.event_log().unwrap().len();
    let a_hash = sim_a.event_log().unwrap().log_hash();

    // ── Branch B: n1 crashes → message lost ───────────────────
    let mut sim_b = sim.fork();
    let mut rt_b = rt.fork();
    sim_b.schedule(
        VirtualTime::new(5),
        EventType::NodeCrash { node: n1 },
    );
    sim_b.schedule(
        VirtualTime::new(10),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("branch-b-msg".into()),
        },
    );
    sim_b.run(&mut rt_b);

    let b_msgs = rt_b.node::<PingNode>(n0).unwrap().received.len();
    let b_events = sim_b.event_log().unwrap().len();
    let b_hash = sim_b.event_log().unwrap().log_hash();

    // ── Print results ─────────────────────────────────────────
    println!("  Branch A (normal):");
    println!("    N0 echoes: {}", a_msgs);
    println!("    Events:    {}", a_events);
    println!("    Log hash:  {:016x}", a_hash);
    println!();
    println!("  Branch B (n1 crashed):");
    println!("    N0 echoes: {}", b_msgs);
    println!("    Events:    {}", b_events);
    println!("    Log hash:  {:016x}", b_hash);
    println!();

    if a_hash != b_hash {
        println!("  ✓ Branches diverged — different log hashes (expected).");
    } else {
        println!("  ✗ Branches should have diverged!");
    }
    if a_msgs > b_msgs {
        println!("  ✓ Branch A received more echoes than B (crash effect).");
    }
    println!();
    println!("  ✓ Forkable execution demo complete.");
}
