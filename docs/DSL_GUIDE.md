# Helios DSL Guide

Helios treats "Developer Experience" as a first-class citizen. This guide explains how to use the fluent DSL to build complex simulations in a few lines of code.

## The Simulation Builder

The `SimulationBuilder` is the entry point for creating the world.

### Basic Setup
```rust
use helios::dsl::SimulationBuilder;

let (sim, rt) = SimulationBuilder::new()
    .ping(0)         // Registers a PingNode with ID 0
    .echo(1)         // Registers an EchoNode with ID 1
    .build();
```

### Configuring the Network
By default, the network is reliable. You can easily add realism:

```rust
builder.lossy_network(
    20,   // Base latency (ticks)
    5,    // Max jitter (ticks)
    0.05, // Drop rate (5%)
    1234  // Seed
)
```

### Scheduling Events
Events are scheduled at specific virtual timestamps:

```rust
builder
    .send(0, 1, "Ping", 10)     // N0 sends "Ping" to N1 at T=10 (through network)
    .deliver(1, 0, "Pong", 20)  // Direct delivery bypassing network at T=20
    .crash(1, 50)               // Node 1 crashes at T=50
    .recover(1, 100)            // Node 1 recovers at T=100
    .partition(0, 1, 60)        // N0 cannot reach N1 at T=60
```

## The Scenario Builder

The `ScenarioBuilder` is for "What If" analysis. It automates the process of forking simulations to test multiple possibilities.

### Defining Choices
Choices represent non-deterministic points in your simulation's history (like "will the node crash or not?").

```rust
use helios::dsl::ScenarioBuilder;

let result = ScenarioBuilder::new(sim, rt)
    .maybe_crash(1, 10)         // Branch A: N1 crashes. Branch B: N1 survives.
    .maybe_partition(0, 1, 5)   // Combines with previous choices.
```

### Asserting Invariants
After all branches execute, the builder checks your assertions:

```rust
    .assert("all messages received", |sim, rt| {
        let node = rt.node::<PingNode>(NodeId::new(0)).unwrap();
        if node.received.len() == 2 {
            Ok(())
        } else {
            Err("Missed message".into())
        }
    })
    .explore();
```

## Creating Custom Nodes

Implement the `SimNode` trait to create your own protocols:

```rust
impl SimNode for MyNode {
    fn on_event(&mut self, ctx: &mut SimulationContext, event: NodeEvent) {
        match event {
            NodeEvent::Message { from, payload } => {
                // Logic here
            }
            NodeEvent::TimerFired { timer_id } => {
                // Logic here
            }
            _ => {}
        }
    }
    
    fn clone_node(&self) -> Box<dyn SimNode> {
        Box::new(self.clone())
    }
}
```
