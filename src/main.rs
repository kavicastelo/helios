use helios::{
    Choice, EchoNode, EventType, Explorer, MessagePayload, Network, NetworkConfig,
    NodeId, NodeRuntime, PingNode, Simulation, VirtualTime,
};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 6: State Space Exploration Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // ── Setup base simulation ─────────────────────────────────
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

    // Base traffic.
    sim.schedule(
        VirtualTime::new(10),
        EventType::MessageSend {
            from: n0,
            to: n1,
            payload: MessagePayload::Text("ping-n1".into()),
        },
    );
    sim.schedule(
        VirtualTime::new(10),
        EventType::MessageSend {
            from: n0,
            to: n2,
            payload: MessagePayload::Text("ping-n2".into()),
        },
    );

    // ── Explore: crash n1, crash n2, and partition — 8 branches ─
    let mut explorer = Explorer::new(sim, rt);

    explorer.add_choice(Choice::binary(
        "crash n1 at T=5",
        VirtualTime::new(5),
        EventType::NodeCrash { node: n1 },
    ));
    explorer.add_choice(Choice::binary(
        "crash n2 at T=5",
        VirtualTime::new(5),
        EventType::NodeCrash { node: n2 },
    ));
    explorer.add_choice(Choice::binary(
        "partition n0↔n1 at T=3",
        VirtualTime::new(3),
        EventType::NetworkPartition { a: n0, b: n1 },
    ));

    // Safety property: n0 must receive at least 1 echo.
    explorer.check("at least 1 echo", move |_sim, rt| {
        let ping = rt.node::<PingNode>(n0).unwrap();
        if ping.received.is_empty() {
            Err(format!(
                "N0 received 0 echoes (expected ≥1)"
            ))
        } else {
            Ok(())
        }
    });

    println!("  Exploring {} branches...", explorer.total_branches());
    let result = explorer.explore();

    println!("  Branches explored: {}", result.branches_explored);
    println!("  Violations found:  {}", result.violation_count());
    println!();

    if result.is_safe() {
        println!("  ✓ All branches satisfy all properties.");
    } else {
        for v in &result.violations {
            println!("  ✗ Violation: \"{}\"", v.property);
            println!("    Message: {}", v.message);
            print!("    Choices: ");
            let choice_strs: Vec<String> = v
                .choices
                .iter()
                .map(|(label, opt)| {
                    format!(
                        "{}={}",
                        label,
                        if *opt == 0 { "skip" } else { "apply" }
                    )
                })
                .collect();
            println!("{}", choice_strs.join(", "));
            println!();
        }
    }
    println!("  ✓ State space exploration complete.");
}
