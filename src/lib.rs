//! # Helios — Deterministic Distributed Runtime
//!
//! A simulation kernel for building, testing, and exploring distributed
//! systems with full determinism. No async, no threads, no wall-clock
//! time — just pure state machines driven by a virtual clock.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────┐
//! │     NodeRuntime       │ ← dispatches events to SimNodes
//! │  ┌────────────────┐  │
//! │  │   Simulation    │  │ ← execution loop
//! │  │  ┌────────────┐ │  │
//! │  │  │ Scheduler  │ │  │ ← deterministic min-heap
//! │  │  └────────────┘ │  │
//! │  │  ┌────────────┐ │  │
//! │  │  │  Events    │ │  │ ← immutable event records
//! │  │  └────────────┘ │  │
//! │  │  ┌────────────┐ │  │
//! │  │  │  VTime     │ │  │ ← logical clock
//! │  │  └────────────┘ │  │
//! │  └────────────────┘  │
//! └──────────────────────┘
//! ```

pub mod event;
pub mod node;
pub mod scheduler;
pub mod simulation;
pub mod time;

// Re-exports for convenience.
pub use event::{Event, EventId, EventIdGen, EventType};
pub use node::{EchoNode, MessagePayload, NodeEvent, NodeId, NodeRuntime, PingNode, SimNode};
pub use scheduler::Scheduler;
pub use simulation::{EventHandler, Simulation, SimulationContext};
pub use time::VirtualTime;
