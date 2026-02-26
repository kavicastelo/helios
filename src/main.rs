use helios::dsl::{ScenarioBuilder, SimulationBuilder};
use helios::{NodeId, PingNode};

fn main() {
    println!("═══════════════════════════════════════════════════════");
    println!("  Helios — Deterministic Distributed Runtime");
    println!("  Batch 7: DSL Layer Demo");
    println!("═══════════════════════════════════════════════════════");
    println!();

    // ── Before DSL (verbose) vs After DSL (concise) ───────────
    println!("  ┌─ Step 1: Build a 3-node simulation (1 line chain) ─┐");
    println!();

    let (sim, rt) = SimulationBuilder::new()
        .ping(0)
        .echo(1)
        .echo(2)
        .reliable_network(42)
        .with_checkpoints(5)
        .send(0, 1, "hello-n1", 10)
        .send(0, 2, "hello-n2", 10)
        .build();

    println!("    Nodes: 3 (1 ping, 2 echo)");
    println!("    Network: reliable, seed=42");
    println!("    Messages: 2 sends at T=10");
    println!();

    // ── Step 2: Explore fault scenarios ────────────────────────
    println!("  ┌─ Step 2: Explore fault space (fluent DSL) ─────────┐");
    println!();

    let result = ScenarioBuilder::new(sim, rt)
        .maybe_crash(1, 5)
        .maybe_crash(2, 5)
        .maybe_partition(0, 1, 3)
        .assert("liveness: ≥1 echo", |_sim, rt| {
            let p = rt.node::<PingNode>(NodeId::new(0)).unwrap();
            if p.received.is_empty() {
                Err(format!("N0 got 0 echoes"))
            } else {
                Ok(())
            }
        })
        .explore();

    println!("    Branches explored: {}", result.branches_explored);
    println!("    Violations:        {}", result.violation_count());
    println!();

    if result.is_safe() {
        println!("    ✓ All branches safe.");
    } else {
        for v in &result.violations {
            let choices: Vec<String> = v
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
            println!("    ✗ [{}] {}", v.property, v.message);
            println!("      {}", choices.join(", "));
        }
    }

    println!();
    println!("  ✓ DSL demo complete.");
}
