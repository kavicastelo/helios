use helios::{
    EchoNode, EventType, MessagePayload, Network, NetworkConfig,
    NodeId, NodeRuntime, PingNode, Simulation, VirtualTime,
};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 3: Network Layer + Failure Injection Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // ── Setup: lossy network with latency + jitter + 20% drops ────
    let net = Network::new(
        NetworkConfig::lossy(5, 3, 0.2), // base=5, jitter=[0,3), 20% drop
        42, // deterministic seed
    );

    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    let n2 = NodeId::new(2);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));
    rt.register(n2, Box::new(EchoNode::new(n2)));

    // ── Send messages through the network ─────────────────────────
    // Use MessageSend (goes through network) instead of MessageDelivery.
    for i in 0..5 {
        sim.schedule(
            VirtualTime::new(i * 10),
            EventType::MessageSend {
                from: n0,
                to: n1,
                payload: MessagePayload::Text(format!("msg-{}", i)),
            },
        );
    }

    // Send a message to n2 as well.
    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageSend {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("hello-n2".into()),
        },
    );

    // ── Dynamic partition: isolate n2 at T=20, heal at T=35 ───────
    sim.schedule(
        VirtualTime::new(20),
        EventType::NetworkPartition { a: n0, b: n2 },
    );
    sim.schedule(
        VirtualTime::new(25),
        EventType::MessageSend {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("during-partition".into()),
        },
    );
    sim.schedule(
        VirtualTime::new(35),
        EventType::NetworkHeal { a: n0, b: n2 },
    );
    sim.schedule(
        VirtualTime::new(40),
        EventType::MessageSend {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("after-heal".into()),
        },
    );

    // ── Run ───────────────────────────────────────────────────────
    let processed = sim.run(&mut rt);

    // ── Print trace ───────────────────────────────────────────────
    println!("  Node Event Trace:");
    println!("  {:<6} {:<6} {:<6} {}", "Time", "EvID", "Node", "Event");
    println!("  {}", "─".repeat(60));
    for entry in &rt.trace {
        println!(
            "  {:<6} {:<6} {:<6} {:?}",
            entry.time.ticks(),
            entry.event_id.raw(),
            entry.node,
            entry.node_event,
        );
    }

    // ── Network decisions ─────────────────────────────────────────
    let net = rt.network().unwrap();
    println!();
    println!("  Network Decisions:");
    println!("  {:<6} {:<6} {:<6} {}", "Time", "From", "To", "Decision");
    println!("  {}", "─".repeat(50));
    for entry in net.log() {
        println!(
            "  {:<6} {:<6} {:<6} {:?}",
            entry.time.ticks(),
            entry.from,
            entry.to,
            entry.decision,
        );
    }

    println!();
    println!("  Total events processed : {}", processed);
    println!("  Network delivered      : {}", net.delivered_count());
    println!("  Network dropped        : {}", net.dropped_count());
    println!("  Final time             : {}", sim.current_time());

    let ping = rt.node::<PingNode>(n0).unwrap();
    println!("  {} received {} echoes", n0, ping.received.len());

    println!();
    println!("  ✓ Deterministic chaos simulation complete.");
}
