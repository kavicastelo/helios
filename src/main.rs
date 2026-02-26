use helios::{
    EchoNode, EventType, MessagePayload, NodeId, NodeRuntime,
    PingNode, Simulation, VirtualTime,
};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 2: Node Abstraction + Message Passing Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    let mut sim = Simulation::new();
    let mut rt = NodeRuntime::new();

    let n0 = NodeId::new(0);
    let n1 = NodeId::new(1);
    let n2 = NodeId::new(2);

    // Register nodes.
    rt.register(n0, Box::new(PingNode::new(n0)));
    rt.register(n1, Box::new(EchoNode::new(n1)));
    rt.register(n2, Box::new(EchoNode::new(n2)));

    // Seed initial messages: n0 → n1 and n0 → n2.
    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageDelivery {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("hello from n0".into()),
        },
    );
    sim.schedule(
        VirtualTime::new(0),
        EventType::MessageDelivery {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("greetings from n0".into()),
        },
    );

    // Crash n2 at T=5, recover at T=15, send another message at T=20.
    sim.schedule(VirtualTime::new(5), EventType::NodeCrash { node: n2 });
    sim.schedule(VirtualTime::new(15), EventType::NodeRecover { node: n2 });
    sim.schedule(
        VirtualTime::new(20),
        EventType::MessageDelivery {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("welcome back n2".into()),
        },
    );

    // Run the simulation.
    let processed = sim.run(&mut rt);

    // Print the trace.
    println!("  Event Trace:");
    println!("  {:<6} {:<6} {:<6} {}", "Time", "EvID", "Node", "Event");
    println!("  {}", "─".repeat(50));
    for entry in &rt.trace {
        println!(
            "  {:<6} {:<6} {:<6} {:?}",
            entry.time.ticks(),
            entry.event_id.raw(),
            entry.node,
            entry.node_event,
        );
    }

    println!();
    println!("  Total events dispatched : {}", processed);
    println!("  Node events in trace    : {}", rt.trace.len());
    println!("  Final time              : {}", sim.current_time());

    // Inspect node state.
    let ping = rt.node::<PingNode>(n0).unwrap();
    println!();
    println!("  {} received {} messages:", n0, ping.received.len());
    for (time, from, payload) in &ping.received {
        println!("    [{time}] from {from} — {payload:?}");
    }

    let echo1 = rt.node::<EchoNode>(n1).unwrap();
    let echo2 = rt.node::<EchoNode>(n2).unwrap();
    println!();
    println!("  {} echoed {} messages", n1, echo1.echo_count);
    println!("  {} echoed {} messages", n2, echo2.echo_count);

    println!();
    println!("  ✓ Deterministic distributed simulation complete.");
}
