//! Node abstraction and message passing for distributed simulation.
//!
//! Introduces logical "nodes" that communicate exclusively through
//! events. Nodes never share memory or mutate global state — all
//! interaction is mediated by the deterministic scheduler.
//!
//! # Module structure
//!
//! | Sub-module | Contents |
//! |---|---|
//! | [`id`] | [`NodeId`] newtype |
//! | [`payload`] | [`MessagePayload`], [`NodeEvent`] |
//! | [`traits`] | [`SimNode`] trait + [`SimulationContext`] extensions |
//! | [`trace`] | [`TraceEntry`] struct |
//! | [`runtime`] | [`NodeRuntime`] struct |
//! | [`builtin`] | [`EchoNode`], [`PingNode`] |

pub mod builtin;
pub mod id;
pub mod payload;
pub mod runtime;
pub mod trace;
pub mod traits;

// Flat re-exports so external callers can use `helios::node::NodeId` etc.
pub use builtin::{EchoNode, PingNode};
pub use id::NodeId;
pub use payload::{MessagePayload, NodeEvent};
pub use runtime::NodeRuntime;
pub use trace::TraceEntry;
pub use traits::SimNode;

#[cfg(test)]
mod tests;
