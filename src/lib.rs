//! # Helios — Deterministic Distributed Runtime
//!
//! A simulation kernel for building, testing, and exploring distributed
//! systems with full determinism. No async, no threads, no wall-clock
//! time — just pure state machines driven by a virtual clock.

pub mod dsl;
pub mod event;
pub mod eventlog;
pub mod explorer;
pub mod network;
pub mod node;
pub mod scheduler;
pub mod simulation;
pub mod time;

// Re-exports for convenience.
pub use event::{Event, EventId, EventIdGen, EventType};
pub use eventlog::{Checkpoint, EventLog};
pub use explorer::{Choice, ExplorationResult, Explorer, NamedProperty, Property, Violation};
pub use network::{DeterministicRng, Network, NetworkConfig, NetworkDecision};
pub use node::{
    EchoNode, MessagePayload, NodeEvent, NodeId, NodeRuntime, PingNode, SimNode, TraceEntry,
};
pub use scheduler::Scheduler;
pub use simulation::{EventHandler, Simulation, SimulationContext};
pub use time::VirtualTime;
