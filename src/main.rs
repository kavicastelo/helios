use helios::{
    EchoNode, EventType, MessagePayload, Network, NetworkConfig,
    NodeId, NodeRuntime, PingNode, Simulation, VirtualTime,
};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 4: Event Sourcing + Replay Verification Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // ── Run 1: original simulation with event logging ─────────
    let log_hash_1 = run_simulation("Run 1");

    // ── Run 2: identical replay ───────────────────────────────
    let log_hash_2 = run_simulation("Run 2");

    // ── Verify ────────────────────────────────────────────────
    println!("  Verification:");
    println!("    Run 1 log hash: {:016x}", log_hash_1);
    println!("    Run 2 log hash: {:016x}", log_hash_2);
    if log_hash_1 == log_hash_2 {
        println!("    ✓ Logs are IDENTICAL — deterministic replay confirmed.");
    } else {
        println!("    ✗ MISMATCH — determinism violation detected!");
    }
    println!();
    println!("  ✓ Event sourcing demo complete.");
}

fn run_simulation(label: &str) -> u64 {
    let net = Network::new(
        NetworkConfig::lossy(3, 2, 0.2),
        42,
    );

    let mut sim = Simulation::new();
    sim.enable_logging_with_checkpoints(5);

    let mut rt = NodeRuntime::with_network(net);

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    let n2 = NodeId::new(2);

    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));
    rt.register(n2, Box::new(EchoNode::new(n2)));

    // Send 10 messages through the lossy network.
    for i in 0..10 {
        sim.schedule(
            VirtualTime::new(i * 5),
            EventType::MessageSend {
                from: n0,
                to: if i % 2 == 0 { n1 } else { n2 },
                payload: MessagePayload::Text(format!("msg-{}", i)),
            },
        );
    }

    // Dynamic partition mid-simulation.
    sim.schedule(
        VirtualTime::new(20),
        EventType::NetworkPartition { a: n0, b: n1 },
    );
    sim.schedule(
        VirtualTime::new(35),
        EventType::NetworkHeal { a: n0, b: n1 },
    );

    let processed = sim.run(&mut rt);
    let log = sim.event_log().unwrap();

    println!("  {}: {} events, {} logged, {} checkpoints",
        label, processed, log.len(), log.checkpoints().len());

    for cp in log.checkpoints() {
        println!("    Checkpoint: event #{}, T={}, hash={:016x}",
            cp.event_index, cp.time.ticks(), cp.state_hash);
    }

    log.log_hash()
}
