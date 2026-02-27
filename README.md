# Helios — Deterministic Distributed Runtime

Helios is a high-performance, single-threaded simulation kernel designed for building, testing, and verifying distributed systems with **100% determinism**.

It eliminates non-determinism at the root: no async runtimes, no threads, and no wall-clock time. Every execution is driven by a seeded RNG and a virtual clock, ensuring that any scenario—no matter how complex or fault-ridden—is perfectly reproducible.

## Key Features

- **Virtual Time Scheduler**: Discrete-event simulation with microsecond precision.
- **Fault Injection**: Programmatic node crashes, network partitions, and message loss.
- **Divergent Forking**: Snapshots and forking of the entire simulation state for speculative execution.
- **State Space Explorer**: Cartesian product exploration of fault scenarios to find safety violations.
- **Fluent DSL**: Ergonomic API for defining nodes, networks, and complex failure scenarios.
- **WASM Support**: Compile your simulation to WebAssembly for browser-based visualization and debugging.

## Quick Start

### 1. Define a Simulation
Using the Helios DSL, setting up a 3-node cluster with a lossy network is trivial:

```rust
use helios::dsl::SimulationBuilder;

let (mut sim, mut rt) = SimulationBuilder::new()
    .ping(0)   // Node 0
    .echo(1)   // Node 1
    .echo(2)   // Node 2
    .lossy_network(10, 2, 0.1, 42) // 10ms lat, 2ms jitter, 10% drop
    .send(0, 1, "Hello", 100)      // Send msg at T=100
    .build();

sim.run(&mut rt);
```

### 2. Verify Safety Properties
Systematically check if your system survives network partitions:

```rust
use helios::dsl::ScenarioBuilder;

let result = ScenarioBuilder::new(sim, rt)
    .maybe_crash(1, 50)           // Explore branch with/without crash
    .maybe_partition(0, 2, 20)    // Explore branch with/without partition
    .assert("liveness", |_, rt| {
        // Your check logic here
        Ok(())
    })
    .explore();

println!("Violations: {}", result.violation_count());
```

## Documentation

- [Architecture Overview](docs/ARCHITECTURE.md)
- [DSL & Usage Guide](docs/DSL_GUIDE.md)
- [State Space Exploration](docs/EXPLORER_GUIDE.md)
- [WASM & Visualizer](docs/WASM_GUIDE.md)

## Building & Testing

```bash
# Run unit tests
cargo test

# Run the demo
cargo run

# Build WASM target (requires wasm-pack)
wasm-pack build --target web --features wasm
```

## License
MIT
