//! Structured error types for Helios.
//!
//! All fallible public APIs return `Result<T, HeliosError>`. This lets
//! callers distinguish operational errors (e.g. node not found) from
//! programming errors (e.g. scheduling in the past) without relying on
//! panics or stringly-typed errors.

use crate::node::NodeId;

/// The top-level error type for the Helios simulation kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum HeliosError {
    // ── Node errors ───────────────────────────────────────

    /// A node ID was referenced but is not registered in the runtime.
    NodeNotFound(NodeId),

    /// Attempted to register a node with an ID that is already in use.
    NodeAlreadyRegistered(NodeId),

    /// A node downcast to a concrete type failed (type mismatch).
    NodeTypeMismatch {
        node: NodeId,
        expected: &'static str,
    },

    /// An operation was attempted on a crashed node.
    NodeCrashed(NodeId),

    // ── Scheduling errors ─────────────────────────────────

    /// Attempted to schedule an event in the past.
    NonCausalEvent {
        requested: u64,
        current: u64,
    },

    /// Attempted to step an already-finished simulation.
    SchedulerEmpty,

    // ── Log / replay errors ───────────────────────────────

    /// Event logging has not been enabled on this simulation.
    LogNotEnabled,

    /// The event log failed its integrity check (hash mismatch).
    LogCorrupted {
        expected_hash: u64,
        actual_hash: u64,
    },

    // ── Network errors ────────────────────────────────────

    /// An operation requiring a network was called but no network is attached.
    NetworkNotAttached,

    // ── Scenario / config errors ──────────────────────────

    /// A scenario configuration could not be parsed.
    InvalidScenario(String),

    // ── WASM errors ───────────────────────────────────────

    /// A serialization error occurred in the WASM/API boundary.
    SerializationError(String),
}

impl std::fmt::Display for HeliosError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HeliosError::NodeNotFound(id) => write!(f, "node {} not found", id),
            HeliosError::NodeAlreadyRegistered(id) => {
                write!(f, "node {} is already registered", id)
            }
            HeliosError::NodeTypeMismatch { node, expected } => {
                write!(f, "node {} is not a {}", node, expected)
            }
            HeliosError::NodeCrashed(id) => write!(f, "node {} is crashed", id),
            HeliosError::NonCausalEvent { requested, current } => write!(
                f,
                "cannot schedule event at T={} when current time is T={}",
                requested, current
            ),
            HeliosError::SchedulerEmpty => write!(f, "simulation has no pending events"),
            HeliosError::LogNotEnabled => write!(f, "event logging is not enabled"),
            HeliosError::LogCorrupted { expected_hash, actual_hash } => write!(
                f,
                "event log corrupted: expected hash {:#018x}, got {:#018x}",
                expected_hash, actual_hash
            ),
            HeliosError::NetworkNotAttached => {
                write!(f, "no network is attached to this runtime")
            }
            HeliosError::InvalidScenario(msg) => write!(f, "invalid scenario: {}", msg),
            HeliosError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for HeliosError {}

/// Convenience alias for `Result<T, HeliosError>`.
pub type HeliosResult<T> = Result<T, HeliosError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeId;

    #[test]
    fn test_error_display_node_not_found() {
        let e = HeliosError::NodeNotFound(NodeId::new(5));
        assert_eq!(e.to_string(), "node N5 not found");
    }

    #[test]
    fn test_error_display_non_causal() {
        let e = HeliosError::NonCausalEvent { requested: 3, current: 10 };
        assert!(e.to_string().contains("T=3"));
        assert!(e.to_string().contains("T=10"));
    }

    #[test]
    fn test_error_display_log_corrupted() {
        let e = HeliosError::LogCorrupted { expected_hash: 0xABCD, actual_hash: 0x1234 };
        let s = e.to_string();
        assert!(s.contains("corrupted"));
    }

    #[test]
    fn test_error_is_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(HeliosError::SchedulerEmpty);
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn test_helios_result_ok() {
        let r: HeliosResult<u32> = Ok(42);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn test_helios_result_err() {
        let r: HeliosResult<u32> = Err(HeliosError::NetworkNotAttached);
        assert!(r.is_err());
    }
}
