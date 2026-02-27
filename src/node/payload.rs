//! Message payload and node event enumerations.

use super::id::NodeId;

// ── MessagePayload ────────────────────────────────────────────────────

/// Opaque message payload carried between nodes.
///
/// Nodes should treat `Data` and `Text` as opaque bytes; the distinction
/// exists purely for ergonomic test authoring (`Text`) vs. real protocol
/// usage (`Data`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum MessagePayload {
    /// Raw bytes (real protocol use).
    Data(Vec<u8>),
    /// Human-readable text (convenient for examples and tests).
    Text(String),
    /// Empty payload (heartbeats, acks, pings).
    Empty,
}

impl std::fmt::Display for MessagePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessagePayload::Data(d) => write!(f, "Data({} bytes)", d.len()),
            MessagePayload::Text(s) => {
                if s.len() > 32 {
                    write!(f, "Text(\"{}…\")", &s[..32])
                } else {
                    write!(f, "Text({:?})", s)
                }
            }
            MessagePayload::Empty => write!(f, "Empty"),
        }
    }
}

// ── NodeEvent ─────────────────────────────────────────────────────────

/// The events that a node can receive via `SimNode::on_event`.
///
/// These are *logical* events dispatched by `NodeRuntime`. The
/// underlying scheduler speaks `EventType`; `NodeRuntime` translates
/// the relevant variants into `NodeEvent` and delivers them to the
/// appropriate node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub enum NodeEvent {
    /// A message arrived from another node.
    Message {
        from: NodeId,
        payload: MessagePayload,
    },
    /// A previously scheduled timer has fired.
    TimerFired { timer_id: u64 },
    /// This node has been crashed by the simulation controller.
    Crash,
    /// This node has recovered from a prior crash.
    Recover,
}

impl std::fmt::Display for NodeEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeEvent::Message { from, payload } => {
                write!(f, "Message(from={}, {})", from, payload)
            }
            NodeEvent::TimerFired { timer_id } => write!(f, "TimerFired(#{})", timer_id),
            NodeEvent::Crash => write!(f, "Crash"),
            NodeEvent::Recover => write!(f, "Recover"),
        }
    }
}
