//! Built-in node implementations — EchoNode and PingNode.
//!
//! These are simple reference nodes used for testing and demonstration.
//! Real protocol nodes (Raft, Bully, Gossip) live in `src/protocols/`.

pub mod echo;
pub mod ping;

pub use echo::EchoNode;
pub use ping::PingNode;
