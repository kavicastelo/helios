/// Simulated network layer with deterministic failure injection.
///
/// All messages can optionally pass through the `Network` before being
/// delivered. The network applies latency, jitter, drops, and partitions
/// — all driven by a seeded deterministic RNG so that every run with the
/// same seed produces identical results.

use std::collections::BTreeSet;

use crate::node::NodeId;
use crate::time::VirtualTime;

// ── Deterministic RNG ─────────────────────────────────────────────────

/// SplitMix64 — a fast, high-quality deterministic PRNG.
///
/// Zero external dependencies. Produces identical sequences for a given
/// seed across all platforms.
#[derive(Debug, Clone)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    /// Create a new RNG from a seed.
    pub fn new(seed: u64) -> Self {
        DeterministicRng { state: seed }
    }

    /// Generate the next u64.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Generate a uniform f64 in [0.0, 1.0).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Generate a uniform u64 in [min, max). Returns `min` if min >= max.
    pub fn next_range(&mut self, min: u64, max: u64) -> u64 {
        if min >= max {
            return min;
        }
        min + (self.next_u64() % (max - min))
    }

    /// Current internal state (useful for snapshotting / forking).
    pub fn state(&self) -> u64 {
        self.state
    }
}

// ── Network Config ────────────────────────────────────────────────────

/// Configuration for simulated network behavior.
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Fixed base latency (ticks) applied to every delivered message.
    pub base_latency: u64,
    /// Maximum random jitter added on top of `base_latency`.
    /// Actual jitter is in `[0, jitter_range)`.
    pub jitter_range: u64,
    /// Probability of randomly dropping a message [0.0, 1.0].
    pub drop_probability: f64,
}

impl NetworkConfig {
    /// A perfectly reliable network: 1 tick latency, no jitter, no drops.
    pub fn reliable() -> Self {
        NetworkConfig {
            base_latency: 1,
            jitter_range: 0,
            drop_probability: 0.0,
        }
    }

    /// A lossy, high-latency network for stress testing.
    pub fn lossy(base_latency: u64, jitter_range: u64, drop_probability: f64) -> Self {
        NetworkConfig {
            base_latency,
            jitter_range,
            drop_probability,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self::reliable()
    }
}

// ── Network Decision ──────────────────────────────────────────────────

/// The outcome of a network processing decision.
#[derive(Debug, Clone, PartialEq)]
pub enum NetworkDecision {
    /// Message will be delivered after `latency` ticks.
    Delivered { latency: u64 },
    /// Message randomly dropped based on `drop_probability`.
    DroppedByChance,
    /// Message dropped because of an active network partition.
    DroppedByPartition,
}

/// A log entry recording a network decision.
#[derive(Debug, Clone)]
pub struct NetworkLogEntry {
    pub time: VirtualTime,
    pub from: NodeId,
    pub to: NodeId,
    pub decision: NetworkDecision,
}

// ── Network ───────────────────────────────────────────────────────────

/// Simulated network with deterministic latency, drops, and partitions.
///
/// Owned by `NodeRuntime`. Every `MessageSend` event is passed through
/// `Network::process()`, which decides whether to deliver or drop.
#[derive(Debug, Clone)]
pub struct Network {
    config: NetworkConfig,
    rng: DeterministicRng,
    /// Directional partition set: `(a, b)` means a cannot reach b.
    partitions: BTreeSet<(NodeId, NodeId)>,
    /// Append-only log of all decisions.
    log: Vec<NetworkLogEntry>,
}

impl Network {
    /// Create a new network with the given config and RNG seed.
    pub fn new(config: NetworkConfig, seed: u64) -> Self {
        Network {
            config,
            rng: DeterministicRng::new(seed),
            partitions: BTreeSet::new(),
            log: Vec::new(),
        }
    }

    /// Create a reliable network (no drops, minimal latency).
    pub fn reliable(seed: u64) -> Self {
        Self::new(NetworkConfig::reliable(), seed)
    }

    // ── Partition management ──────────────────────────────────────

    /// Add a bidirectional partition between two nodes.
    pub fn add_partition(&mut self, a: NodeId, b: NodeId) {
        self.partitions.insert((a, b));
        self.partitions.insert((b, a));
    }

    /// Add a one-way partition (a cannot reach b, but b can reach a).
    pub fn add_one_way_partition(&mut self, from: NodeId, to: NodeId) {
        self.partitions.insert((from, to));
    }

    /// Remove all partitions between two nodes (both directions).
    pub fn remove_partition(&mut self, a: NodeId, b: NodeId) {
        self.partitions.remove(&(a, b));
        self.partitions.remove(&(b, a));
    }

    /// Check if a partition exists from `from` to `to`.
    pub fn is_partitioned(&self, from: NodeId, to: NodeId) -> bool {
        self.partitions.contains(&(from, to))
    }

    /// Remove all partitions.
    pub fn heal_all(&mut self) {
        self.partitions.clear();
    }

    // ── Message processing ────────────────────────────────────────

    /// Process a message send attempt. Returns the decision.
    ///
    /// This is the core function: it consumes RNG state deterministically,
    /// records the decision in the log, and returns the outcome.
    pub fn process(
        &mut self,
        time: VirtualTime,
        from: NodeId,
        to: NodeId,
    ) -> NetworkDecision {
        // 1. Check partition.
        if self.partitions.contains(&(from, to)) {
            let decision = NetworkDecision::DroppedByPartition;
            self.log.push(NetworkLogEntry {
                time,
                from,
                to,
                decision: decision.clone(),
            });
            return decision;
        }

        // 2. Check random drop.
        if self.config.drop_probability > 0.0 && self.rng.next_f64() < self.config.drop_probability
        {
            let decision = NetworkDecision::DroppedByChance;
            self.log.push(NetworkLogEntry {
                time,
                from,
                to,
                decision: decision.clone(),
            });
            return decision;
        }

        // 3. Compute latency.
        let jitter = if self.config.jitter_range > 0 {
            self.rng.next_range(0, self.config.jitter_range)
        } else {
            0
        };
        let latency = self.config.base_latency + jitter;

        let decision = NetworkDecision::Delivered { latency };
        self.log.push(NetworkLogEntry {
            time,
            from,
            to,
            decision: decision.clone(),
        });
        decision
    }

    // ── Accessors ─────────────────────────────────────────────────

    /// Access the decision log.
    pub fn log(&self) -> &[NetworkLogEntry] {
        &self.log
    }

    /// Number of decisions made.
    pub fn decision_count(&self) -> usize {
        self.log.len()
    }

    /// Count of messages delivered.
    pub fn delivered_count(&self) -> usize {
        self.log
            .iter()
            .filter(|e| matches!(e.decision, NetworkDecision::Delivered { .. }))
            .count()
    }

    /// Count of messages dropped (by any reason).
    pub fn dropped_count(&self) -> usize {
        self.log
            .iter()
            .filter(|e| !matches!(e.decision, NetworkDecision::Delivered { .. }))
            .count()
    }

    /// Access the config.
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    /// Mutable access to the config (for dynamic reconfiguration).
    pub fn config_mut(&mut self) -> &mut NetworkConfig {
        &mut self.config
    }

    /// Access the RNG (for forking / snapshotting).
    pub fn rng(&self) -> &DeterministicRng {
        &self.rng
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_determinism() {
        let mut rng1 = DeterministicRng::new(42);
        let mut rng2 = DeterministicRng::new(42);

        let seq1: Vec<u64> = (0..100).map(|_| rng1.next_u64()).collect();
        let seq2: Vec<u64> = (0..100).map(|_| rng2.next_u64()).collect();

        assert_eq!(seq1, seq2, "RNG is not deterministic!");
    }

    #[test]
    fn test_rng_different_seeds_differ() {
        let mut rng1 = DeterministicRng::new(1);
        let mut rng2 = DeterministicRng::new(2);

        let v1 = rng1.next_u64();
        let v2 = rng2.next_u64();
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_rng_f64_range() {
        let mut rng = DeterministicRng::new(123);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "f64 out of range: {}", v);
        }
    }

    #[test]
    fn test_rng_range() {
        let mut rng = DeterministicRng::new(99);
        for _ in 0..1000 {
            let v = rng.next_range(10, 20);
            assert!((10..20).contains(&v), "range out of bounds: {}", v);
        }
    }

    #[test]
    fn test_reliable_network() {
        let mut net = Network::new(NetworkConfig::reliable(), 42);
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        for _ in 0..100 {
            let d = net.process(VirtualTime::new(0), n0, n1);
            assert!(
                matches!(d, NetworkDecision::Delivered { latency: 1 }),
                "Reliable network dropped or wrong latency: {:?}",
                d
            );
        }
        assert_eq!(net.delivered_count(), 100);
        assert_eq!(net.dropped_count(), 0);
    }

    #[test]
    fn test_partition_drops_messages() {
        let mut net = Network::new(NetworkConfig::reliable(), 42);
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        net.add_partition(n0, n1);

        let d = net.process(VirtualTime::new(0), n0, n1);
        assert_eq!(d, NetworkDecision::DroppedByPartition);

        // Reverse direction also blocked (bidirectional).
        let d = net.process(VirtualTime::new(0), n1, n0);
        assert_eq!(d, NetworkDecision::DroppedByPartition);

        assert_eq!(net.dropped_count(), 2);
    }

    #[test]
    fn test_one_way_partition() {
        let mut net = Network::new(NetworkConfig::reliable(), 42);
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        net.add_one_way_partition(n0, n1);

        // n0 → n1 blocked.
        let d = net.process(VirtualTime::new(0), n0, n1);
        assert_eq!(d, NetworkDecision::DroppedByPartition);

        // n1 → n0 still works.
        let d = net.process(VirtualTime::new(0), n1, n0);
        assert!(matches!(d, NetworkDecision::Delivered { .. }));
    }

    #[test]
    fn test_partition_heal() {
        let mut net = Network::new(NetworkConfig::reliable(), 42);
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        net.add_partition(n0, n1);
        assert!(net.is_partitioned(n0, n1));

        net.remove_partition(n0, n1);
        assert!(!net.is_partitioned(n0, n1));

        let d = net.process(VirtualTime::new(0), n0, n1);
        assert!(matches!(d, NetworkDecision::Delivered { .. }));
    }

    #[test]
    fn test_drop_probability() {
        let mut net = Network::new(
            NetworkConfig::lossy(1, 0, 0.5), // 50% drop rate
            42,
        );
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        for _ in 0..1000 {
            net.process(VirtualTime::new(0), n0, n1);
        }

        // With 50% drop, we expect roughly 500 drops.
        // Allow generous margin for randomness.
        let drops = net.dropped_count();
        assert!(
            (350..650).contains(&drops),
            "Drop count {} is outside expected range for p=0.5",
            drops
        );
    }

    #[test]
    fn test_deterministic_drop_decisions() {
        fn run_decisions(seed: u64) -> Vec<NetworkDecision> {
            let mut net = Network::new(NetworkConfig::lossy(1, 0, 0.3), seed);
            let n0 = NodeId::new(0);
            let n1 = NodeId::new(1);

            (0..50)
                .map(|_| net.process(VirtualTime::new(0), n0, n1))
                .collect()
        }

        let run1 = run_decisions(42);
        let run2 = run_decisions(42);
        assert_eq!(run1, run2, "Network decisions are not deterministic!");
    }

    #[test]
    fn test_jitter_variation() {
        let mut net = Network::new(
            NetworkConfig::lossy(10, 5, 0.0), // base=10, jitter=[0,5)
            42,
        );
        let n0 = NodeId::new(0);
        let n1 = NodeId::new(1);

        let mut latencies = Vec::new();
        for _ in 0..100 {
            if let NetworkDecision::Delivered { latency } =
                net.process(VirtualTime::new(0), n0, n1)
            {
                assert!(
                    (10..15).contains(&latency),
                    "Latency {} outside [10, 15)",
                    latency
                );
                latencies.push(latency);
            }
        }

        // With jitter, we should see at least 2 distinct latencies.
        latencies.sort();
        latencies.dedup();
        assert!(
            latencies.len() >= 2,
            "Jitter produced no variation: {:?}",
            latencies
        );
    }

    #[test]
    fn test_deterministic_latency_with_jitter() {
        fn run_latencies(seed: u64) -> Vec<u64> {
            let mut net = Network::new(NetworkConfig::lossy(5, 10, 0.0), seed);
            let n0 = NodeId::new(0);
            let n1 = NodeId::new(1);
            (0..50)
                .filter_map(|_| {
                    if let NetworkDecision::Delivered { latency } =
                        net.process(VirtualTime::new(0), n0, n1)
                    {
                        Some(latency)
                    } else {
                        None
                    }
                })
                .collect()
        }

        let run1 = run_latencies(99);
        let run2 = run_latencies(99);
        assert_eq!(run1, run2, "Latencies not deterministic across runs!");
    }
}
