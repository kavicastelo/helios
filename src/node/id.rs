//! Node ID — a lightweight, ordered, copyable node identifier.

/// A unique identifier for a simulated node.
///
/// `NodeId` is intentionally a newtype around `u64` rather than a
/// bare integer to prevent accidental confusion with other u64 values
/// (event IDs, timestamps, etc.) at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeId(u64);

impl NodeId {
    /// Create a node ID from a raw integer.
    #[inline]
    pub fn new(id: u64) -> Self {
        NodeId(id)
    }

    /// Return the underlying integer.
    #[inline]
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "N{}", self.0)
    }
}
