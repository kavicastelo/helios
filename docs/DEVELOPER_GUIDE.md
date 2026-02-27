# Helios Developer Guide

Welcome to the internal world of Helios! This guide is for developers who want to modify the simulation kernel, add new event types, or extend the core engine.

## Codebase Map

| File | Responsibility |
|------|----------------|
| `lib.rs` | Crate entry point and public re-exports. |
| `simulation.rs` | The central `Simulation` struct and main execution loop. |
| `node.rs` | `NodeRuntime` and the `SimNode` trait system. |
| `event.rs` | Definition of the `Event` and `EventType` enums. |
| `scheduler.rs` | The priority queue managing future events. |
| `eventlog.rs` | Append-only log with deterministic hashing. |
| `network.rs` | Network disturbance logic (latency, drops, partitions). |
| `explorer.rs` | The state space exploration engine. |
| `api.rs` | External JSON/FFI interface layer. |
| `dsl.rs` | Fluent builders (`SimulationBuilder`, `ScenarioBuilder`). |

## Adding a New Event Type

Adding a new event requires updates in three places:

1. **`event.rs`**: Add the variant to `EventType`.
2. **`simulation.rs`**: Handle the event in `Simulation::dispatch`. Usually, this means calling a method on `NodeRuntime`.
3. **`node.rs`**: (Optional) Add a method to `NodeRuntime` if the event interacts with nodes or the network.

## The `NodeRuntime` Lifecycle

The `NodeRuntime` is the only component that nodes can see. It acts as an isolation layer.

- When a node calls `ctx.send()`, it schedules a `MessageSend` or `MessageDelivery` event.
- When a node calls `ctx.set_timer()`, it schedules a `TimerFired` event.
- The `NodeRuntime` handles `NodeCrash` by setting an `alive` bit and silently dropping any future events for that node until a `NodeRecover` event occurs.

## Maintaining Determinism

To avoid breaking reproducibility:
- **NEVER** use `std::time::*`. Only use `VirtualTime`.
- **NEVER** use `HashMap` or `HashSet` unless you use a deterministic hasher or sort the keys before iteration. Helios provides deterministic ordered maps in the core parts.
- **NEVER** spawn threads.
- **ALWAYS** drive randomness from the `DeterministicRng` provided in `Network`.

## Testing Rules
- Every batch should add at least one end-to-end integration test.
- Use `test_fork_divergent_execution` in `node.rs` as a template for testing isolation.
- Verify that `EventLog` hashes are identical across different runs with the same seed.

## Building for the Web
If you modify `src/wasm.rs`, ensure that you can still build with `cargo build`. WASM code is gated behind:
```rust
#[cfg(feature = "wasm")]
pub mod wasm;
```

Compile with: `wasm-pack build --target web --features wasm`.
