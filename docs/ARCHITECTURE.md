# Helios Architecture

This document describes the internal design and principles of the Helios simulation kernel.

## Core Principles

Helios is built on three immutable pillars of determinism:

1. **No Real Time**: The simulation uses `VirtualTime` (ticks). Time only advances when an event is processed.
2. **No Concurrency**: The kernel is single-threaded. Message passing between nodes is simulated by the central scheduler.
3. **No External Randomness**: All probabilities (network drops, jitter) are driven by a PRNG initialized with a fixed seed.

## System Components

### 1. Scheduler (`scheduler.rs`)
The heartbeat of the simulation. It maintains a priority queue of `Event` objects sorted by `VirtualTime`. It ensures that if two events are scheduled for the same tick, they are processed in a stable, deterministic order based on their unique `EventId`.

### 2. Node Runtime (`node.rs`)
Manages the lifecycle of `SimNode` trait objects.
- **Isolated State**: Nodes cannot share memory; they communicate solely through the `NodeRuntime`.
- **Fault Injection**: The runtime tracks which nodes are "Alive" and can silently drop events destined for "Crashed" nodes.
- **Trace Management**: Every state transition is recorded in a `Trace`, allowing for detailed post-mortem analysis.

### 3. Event Log (`eventlog.rs`)
A cryptographic record of every action in the simulation.
- **Event Sourcing**: Every state change is an event.
- **Hashing**: The simulation maintains a rolling hash of the event sequence. Two simulations are identical if and only if their log hashes match.
- **Checkpoints**: Periodic snapshots of metadata allow the simulation to verify consistency at scale.

### 4. Network Layer (`network.rs`)
Models the imperfect world between nodes.
- **Topology**: Distinguishes between directed and undirected links.
- **Disturbances**: Injects latency, jitter, and packet loss based on the `NetworkConfig`.
- **Partitions**: Simulates network splits (A cannot reach B) and heals them dynamically.

## Determinism Model

Helios achieves determinism by ensuring that $[S_{t+1} = f(S_t, E_t)]$, where:
- $S$ is the total simulation state (Nodes + Scheduler + Network).
- $E$ is the event popped from the queue.
- $f$ is a pure transition function.

Because $f$ contains no external side effects, the entire trajectory of a simulation is a pure function of its initial state and the RNG seed.

## Forking & Replay

The `Simulation::fork()` method performs a deep clone of the entire world state. This is possible because:
- All components implement `Clone`.
- Traits like `SimNode` provide dyn-safe `clone_node()` methods.
- No pointers or OS-level handles (like sockets or file descriptors) are used internally.

This allows Helios to branch the timeline at any point, explore a fault, and then "discard" the branch to continue from the original state.
