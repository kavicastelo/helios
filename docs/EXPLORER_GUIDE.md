# State Space Exploration in Helios

Helios doesn't just run simulations; it proves they are safe against a defined set of faults.

## The Problem: Combinatorial Explosion

Distributed systems have massive state spaces. If you have 3 nodes that can each independently crash at $T=5, 10, 15$, the number of possible execution paths is $2^9 = 512$. Testing these manually is impossible.

## The Solution: Cartesian Product Exploration

The `Explorer` engine systematically traverses the Cartesian product of all defined `Choice` points.

### How it Works
1. **Define the Base State**: Set up your simulation with the data you want to process.
2. **Add Choices**: Define the points of uncertainty (crashes, message drops, partitions).
3. **Execute in Parallel**: Internally, Helios uses `fork()` to create an independent memory space for every single branch.
4. **Report Violations**: If any branch fails a `Property` check, Helios reports exactly which sequence of choices led to that failure.

## Choice Types

### Binary Choice
Explores "Yes" or "No" for an event.
```rust
Choice::binary("crash", time, EventType::NodeCrash { node })
```

### Multi-Way Choice
Explores $N$ different possibilities. Useful for message reordering or choosing which node fails.
```rust
Choice::multi("target", vec![
    vec![(t, event_a)],
    vec![(t, event_b)],
    vec![(t, event_a), (t, event_b)],
])
```

### Alternatives
Choose exactly one of $N$ alternatives.
```rust
Choice::alternatives("leader", vec![
    (t, node_0_elected),
    (t, node_1_elected),
])
```

## Writing Safe Properties

Properties are evaluated after a simulation branch completes. They should check the state of the nodes for consistency or progress.

```rust
explorer.check("no message loss", |sim, rt| {
    // Return Ok(()) or Err(String)
});
```

## Best Practices
- **Max Branches**: Exploration can grow exponentially. Use `explorer.set_max_branches(limit)` to cap tests during development.
- **Timing tie-breaks**: Use specific timestamps to reduce state space. Crashing a node significantly after a message is sent is often redundant compared to crashing it right before.
- **Isolate Faults**: Group related faults into a single `Choice` if the system treats them the same way.
- **Zero Dependencies**: The explorer runs in pure Rust, making it ideal for CI/CD pipelines to find regressions in distributed protocols.
