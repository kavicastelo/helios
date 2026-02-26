/// Event sourcing and replay engine.
///
/// Records every dispatched event into an append-only log, supports
/// checkpoint snapshots with hash-based validation, and provides
/// export/import for deterministic replay verification.

use std::io::{self, BufRead, Write};

use crate::event::{Event, EventId, EventType};
use crate::node::{MessagePayload, NodeId};
use crate::time::VirtualTime;

// ── Hash utility ──────────────────────────────────────────────────────

/// Combine two u64 hashes deterministically.
pub fn hash_combine(a: u64, b: u64) -> u64 {
    let mut h = a;
    h = h.wrapping_mul(0x517cc1b727220a95);
    h = h.wrapping_add(b);
    h ^= h >> 32;
    h
}

/// Hash a byte slice deterministically (FNV-1a variant).
pub fn hash_bytes(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ── Checkpoint ────────────────────────────────────────────────────────

/// A snapshot of the simulation state at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// Number of events processed when this checkpoint was taken.
    pub event_index: u64,
    /// Virtual time at the checkpoint.
    pub time: VirtualTime,
    /// Combined hash of all node states.
    pub state_hash: u64,
}

// ── Event Log ─────────────────────────────────────────────────────────

/// Append-only log of dispatched events with optional checkpointing.
#[derive(Debug, Clone)]
pub struct EventLog {
    events: Vec<Event>,
    checkpoints: Vec<Checkpoint>,
    checkpoint_interval: Option<u64>,
}

impl EventLog {
    /// Create an empty event log.
    pub fn new() -> Self {
        EventLog {
            events: Vec::new(),
            checkpoints: Vec::new(),
            checkpoint_interval: None,
        }
    }

    /// Create an event log with automatic checkpointing every `n` events.
    pub fn with_checkpoint_interval(n: u64) -> Self {
        EventLog {
            events: Vec::new(),
            checkpoints: Vec::new(),
            checkpoint_interval: Some(n),
        }
    }

    /// Record a dispatched event.
    pub fn record(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Add a checkpoint.
    pub fn add_checkpoint(&mut self, event_index: u64, time: VirtualTime, state_hash: u64) {
        self.checkpoints.push(Checkpoint {
            event_index,
            time,
            state_hash,
        });
    }

    /// Check if a checkpoint should be taken at this event count.
    pub fn should_checkpoint(&self, events_processed: u64) -> bool {
        match self.checkpoint_interval {
            Some(n) if n > 0 => events_processed % n == 0,
            _ => false,
        }
    }

    /// Access the recorded events.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Access the checkpoints.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Number of recorded events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Compute a deterministic hash of the entire event log.
    pub fn log_hash(&self) -> u64 {
        let mut h: u64 = 0;
        for event in &self.events {
            h = hash_combine(h, event.id.raw());
            h = hash_combine(h, event.scheduled_at.ticks());
            h = hash_combine(h, event_type_hash(&event.payload));
        }
        h
    }

    // ── Export / Import ───────────────────────────────────────────

    /// Export the event log to a writer in a deterministic text format.
    pub fn export<W: Write>(&self, w: &mut W) -> io::Result<()> {
        writeln!(w, "# HELIOS EVENT LOG v1")?;
        writeln!(w, "# events: {}", self.events.len())?;
        writeln!(w, "# checkpoints: {}", self.checkpoints.len())?;

        for event in &self.events {
            write!(w, "E {} {} ", event.id.raw(), event.scheduled_at.ticks())?;
            serialize_event_type(w, &event.payload)?;
            writeln!(w)?;
        }

        for cp in &self.checkpoints {
            writeln!(
                w,
                "C {} {} {:016x}",
                cp.event_index,
                cp.time.ticks(),
                cp.state_hash
            )?;
        }

        Ok(())
    }

    /// Export to a file path.
    pub fn export_to_file(&self, path: &str) -> io::Result<()> {
        let mut f = std::fs::File::create(path)?;
        self.export(&mut f)
    }

    /// Import an event log from a reader.
    pub fn import<R: BufRead>(r: R) -> io::Result<Self> {
        let mut events = Vec::new();
        let mut checkpoints = Vec::new();

        for line in r.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('E') {
                let event = deserialize_event(line).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, e)
                })?;
                events.push(event);
            } else if line.starts_with('C') {
                let cp = deserialize_checkpoint(line).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, e)
                })?;
                checkpoints.push(cp);
            }
        }

        Ok(EventLog {
            events,
            checkpoints,
            checkpoint_interval: None,
        })
    }

    /// Import from a file path.
    pub fn import_from_file(path: &str) -> io::Result<Self> {
        let f = std::fs::File::open(path)?;
        let r = io::BufReader::new(f);
        Self::import(r)
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

// ── Verification ──────────────────────────────────────────────────────

/// Compare two event logs for identical event ordering and payloads.
pub fn logs_match(a: &EventLog, b: &EventLog) -> bool {
    if a.events.len() != b.events.len() {
        return false;
    }
    a.events.iter().zip(b.events.iter()).all(|(ea, eb)| {
        ea.id == eb.id && ea.scheduled_at == eb.scheduled_at && ea.payload == eb.payload
    })
}

/// Compare checkpoints between two logs.
pub fn checkpoints_match(a: &EventLog, b: &EventLog) -> bool {
    a.checkpoints == b.checkpoints
}

// ── Serialization helpers ─────────────────────────────────────────────

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| format!("hex decode error: {}", e))
        })
        .collect()
}

fn serialize_payload<W: Write>(w: &mut W, payload: &MessagePayload) -> io::Result<()> {
    match payload {
        MessagePayload::Empty => write!(w, "Empty"),
        MessagePayload::Text(t) => write!(w, "Text:{}", hex_encode(t.as_bytes())),
        MessagePayload::Data(d) => write!(w, "Data:{}", hex_encode(d)),
    }
}

fn deserialize_payload(s: &str) -> Result<MessagePayload, String> {
    if s == "Empty" {
        return Ok(MessagePayload::Empty);
    }
    if let Some(hex) = s.strip_prefix("Text:") {
        let bytes = hex_decode(hex)?;
        let text =
            String::from_utf8(bytes).map_err(|e| format!("utf8 error: {}", e))?;
        return Ok(MessagePayload::Text(text));
    }
    if let Some(hex) = s.strip_prefix("Data:") {
        let bytes = hex_decode(hex)?;
        return Ok(MessagePayload::Data(bytes));
    }
    Err(format!("unknown payload: {}", s))
}

fn serialize_event_type<W: Write>(w: &mut W, et: &EventType) -> io::Result<()> {
    match et {
        EventType::Noop => write!(w, "Noop"),
        EventType::Log(msg) => write!(w, "Log {}", hex_encode(msg.as_bytes())),
        EventType::MessageSend { from, to, payload } => {
            write!(w, "MsgSend {} {} ", from.raw(), to.raw())?;
            serialize_payload(w, payload)
        }
        EventType::MessageDelivery { from, to, payload } => {
            write!(w, "MsgDel {} {} ", from.raw(), to.raw())?;
            serialize_payload(w, payload)
        }
        EventType::TimerFired { node, timer_id } => {
            write!(w, "Timer {} {}", node.raw(), timer_id)
        }
        EventType::NodeCrash { node } => write!(w, "Crash {}", node.raw()),
        EventType::NodeRecover { node } => write!(w, "Recover {}", node.raw()),
        EventType::NetworkPartition { a, b } => {
            write!(w, "Partition {} {}", a.raw(), b.raw())
        }
        EventType::NetworkHeal { a, b } => {
            write!(w, "Heal {} {}", a.raw(), b.raw())
        }
    }
}

fn deserialize_event(line: &str) -> Result<Event, String> {
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    if parts.len() < 3 || parts[0] != "E" {
        return Err(format!("invalid event line: {}", line));
    }

    let id: u64 = parts[1].parse().map_err(|e| format!("id: {}", e))?;
    let time: u64 = parts[2].parse().map_err(|e| format!("time: {}", e))?;

    let type_str = if parts.len() > 3 { parts[3] } else { "" };
    let payload = deserialize_event_type(type_str)?;

    Ok(Event::new(EventId::new(id), VirtualTime::new(time), payload))
}

fn deserialize_event_type(s: &str) -> Result<EventType, String> {
    let parts: Vec<&str> = s.splitn(4, ' ').collect();
    if parts.is_empty() {
        return Err("empty event type".into());
    }

    match parts[0] {
        "Noop" => Ok(EventType::Noop),
        "Log" => {
            let hex = parts.get(1).ok_or("missing log text")?;
            let bytes = hex_decode(hex)?;
            let text =
                String::from_utf8(bytes).map_err(|e| format!("utf8: {}", e))?;
            Ok(EventType::Log(text))
        }
        "MsgSend" => {
            let from = parse_node_id(parts.get(1), "from")?;
            let to = parse_node_id(parts.get(2), "to")?;
            let payload = deserialize_payload(parts.get(3).ok_or("missing payload")?)?;
            Ok(EventType::MessageSend { from, to, payload })
        }
        "MsgDel" => {
            let from = parse_node_id(parts.get(1), "from")?;
            let to = parse_node_id(parts.get(2), "to")?;
            let payload = deserialize_payload(parts.get(3).ok_or("missing payload")?)?;
            Ok(EventType::MessageDelivery { from, to, payload })
        }
        "Timer" => {
            let node = parse_node_id(parts.get(1), "node")?;
            let timer_id: u64 = parts
                .get(2)
                .ok_or("missing timer_id")?
                .parse()
                .map_err(|e| format!("timer_id: {}", e))?;
            Ok(EventType::TimerFired { node, timer_id })
        }
        "Crash" => {
            let node = parse_node_id(parts.get(1), "node")?;
            Ok(EventType::NodeCrash { node })
        }
        "Recover" => {
            let node = parse_node_id(parts.get(1), "node")?;
            Ok(EventType::NodeRecover { node })
        }
        "Partition" => {
            let a = parse_node_id(parts.get(1), "a")?;
            let b = parse_node_id(parts.get(2), "b")?;
            Ok(EventType::NetworkPartition { a, b })
        }
        "Heal" => {
            let a = parse_node_id(parts.get(1), "a")?;
            let b = parse_node_id(parts.get(2), "b")?;
            Ok(EventType::NetworkHeal { a, b })
        }
        other => Err(format!("unknown event type: {}", other)),
    }
}

fn deserialize_checkpoint(line: &str) -> Result<Checkpoint, String> {
    let parts: Vec<&str> = line.split(' ').collect();
    if parts.len() < 4 || parts[0] != "C" {
        return Err(format!("invalid checkpoint: {}", line));
    }
    let event_index: u64 = parts[1].parse().map_err(|e| format!("index: {}", e))?;
    let time: u64 = parts[2].parse().map_err(|e| format!("time: {}", e))?;
    let state_hash =
        u64::from_str_radix(parts[3], 16).map_err(|e| format!("hash: {}", e))?;
    Ok(Checkpoint {
        event_index,
        time: VirtualTime::new(time),
        state_hash,
    })
}

fn parse_node_id(s: Option<&&str>, label: &str) -> Result<NodeId, String> {
    let raw: u64 = s
        .ok_or(format!("missing {}", label))?
        .parse()
        .map_err(|e| format!("{}: {}", label, e))?;
    Ok(NodeId::new(raw))
}

fn event_type_hash(et: &EventType) -> u64 {
    match et {
        EventType::Noop => 1,
        EventType::Log(s) => hash_combine(2, hash_bytes(s.as_bytes())),
        EventType::MessageSend { from, to, payload } => {
            let mut h = hash_combine(3, from.raw());
            h = hash_combine(h, to.raw());
            h = hash_combine(h, payload_hash(payload));
            h
        }
        EventType::MessageDelivery { from, to, payload } => {
            let mut h = hash_combine(4, from.raw());
            h = hash_combine(h, to.raw());
            h = hash_combine(h, payload_hash(payload));
            h
        }
        EventType::TimerFired { node, timer_id } => {
            hash_combine(5, hash_combine(node.raw(), *timer_id))
        }
        EventType::NodeCrash { node } => hash_combine(6, node.raw()),
        EventType::NodeRecover { node } => hash_combine(7, node.raw()),
        EventType::NetworkPartition { a, b } => {
            hash_combine(8, hash_combine(a.raw(), b.raw()))
        }
        EventType::NetworkHeal { a, b } => {
            hash_combine(9, hash_combine(a.raw(), b.raw()))
        }
    }
}

fn payload_hash(p: &MessagePayload) -> u64 {
    match p {
        MessagePayload::Empty => 0,
        MessagePayload::Text(s) => hash_combine(1, hash_bytes(s.as_bytes())),
        MessagePayload::Data(d) => hash_combine(2, hash_bytes(d)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_determinism() {
        let h1 = hash_combine(42, 99);
        let h2 = hash_combine(42, 99);
        assert_eq!(h1, h2);

        // Different inputs → different hashes.
        let h3 = hash_combine(42, 100);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_hex_roundtrip() {
        let data = b"hello world";
        let hex = hex_encode(data);
        let decoded = hex_decode(&hex).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_payload_roundtrip() {
        let payloads = vec![
            MessagePayload::Empty,
            MessagePayload::Text("hello world".into()),
            MessagePayload::Data(vec![1, 2, 3, 255]),
        ];
        for p in &payloads {
            let mut buf = Vec::new();
            serialize_payload(&mut buf, p).unwrap();
            let s = String::from_utf8(buf).unwrap();
            let p2 = deserialize_payload(&s).unwrap();
            assert_eq!(*p, p2, "Payload roundtrip failed for {:?}", p);
        }
    }

    #[test]
    fn test_event_roundtrip() {
        let events = vec![
            Event::new(EventId::new(0), VirtualTime::new(0), EventType::Noop),
            Event::new(
                EventId::new(1),
                VirtualTime::new(5),
                EventType::Log("test log".into()),
            ),
            Event::new(
                EventId::new(2),
                VirtualTime::new(10),
                EventType::MessageSend {
                    from: NodeId::new(0),
                    to: NodeId::new(1),
                    payload: MessagePayload::Text("hello".into()),
                },
            ),
            Event::new(
                EventId::new(3),
                VirtualTime::new(15),
                EventType::MessageDelivery {
                    from: NodeId::new(1),
                    to: NodeId::new(0),
                    payload: MessagePayload::Empty,
                },
            ),
            Event::new(
                EventId::new(4),
                VirtualTime::new(20),
                EventType::TimerFired {
                    node: NodeId::new(0),
                    timer_id: 42,
                },
            ),
            Event::new(
                EventId::new(5),
                VirtualTime::new(25),
                EventType::NodeCrash {
                    node: NodeId::new(1),
                },
            ),
            Event::new(
                EventId::new(6),
                VirtualTime::new(30),
                EventType::NodeRecover {
                    node: NodeId::new(1),
                },
            ),
            Event::new(
                EventId::new(7),
                VirtualTime::new(35),
                EventType::NetworkPartition {
                    a: NodeId::new(0),
                    b: NodeId::new(2),
                },
            ),
            Event::new(
                EventId::new(8),
                VirtualTime::new(40),
                EventType::NetworkHeal {
                    a: NodeId::new(0),
                    b: NodeId::new(2),
                },
            ),
        ];

        let mut log = EventLog::new();
        for e in &events {
            log.record(e.clone());
        }

        // Export to buffer.
        let mut buf = Vec::new();
        log.export(&mut buf).unwrap();

        // Import from buffer.
        let imported = EventLog::import(io::BufReader::new(buf.as_slice())).unwrap();

        assert!(logs_match(&log, &imported), "Export/import roundtrip failed");
    }

    #[test]
    fn test_log_hash_determinism() {
        let mut log1 = EventLog::new();
        let mut log2 = EventLog::new();

        for i in 0..10 {
            let e = Event::new(
                EventId::new(i),
                VirtualTime::new(i * 5),
                EventType::Log(format!("event-{}", i)),
            );
            log1.record(e.clone());
            log2.record(e);
        }

        assert_eq!(log1.log_hash(), log2.log_hash());
    }

    #[test]
    fn test_checkpoint_serialization() {
        let mut log = EventLog::new();
        log.record(Event::new(
            EventId::new(0),
            VirtualTime::new(0),
            EventType::Noop,
        ));
        log.add_checkpoint(1, VirtualTime::new(0), 0xDEADBEEF);

        let mut buf = Vec::new();
        log.export(&mut buf).unwrap();

        let imported = EventLog::import(io::BufReader::new(buf.as_slice())).unwrap();
        assert!(checkpoints_match(&log, &imported));
    }

    #[test]
    fn test_should_checkpoint() {
        let log = EventLog::with_checkpoint_interval(5);
        assert!(!log.should_checkpoint(1));
        assert!(!log.should_checkpoint(4));
        assert!(log.should_checkpoint(5));
        assert!(log.should_checkpoint(10));
        assert!(!log.should_checkpoint(7));
    }

    // ── Integration: live vs replay ───────────────────────────

    #[test]
    fn test_live_vs_replay_log_hash() {
        use crate::node::{EchoNode, NodeRuntime, PingNode};
        use crate::simulation::Simulation;

        fn run_simulation() -> u64 {
            let mut sim = Simulation::new();
            sim.enable_logging();

            let mut rt = NodeRuntime::new();
            let n0 = NodeId::new(0);
            let n1 = NodeId::new(1);
            rt.register(n0, Box::new(PingNode::new(n0)));
            rt.register(n1, Box::new(EchoNode::new(n1)));

            sim.schedule(
                VirtualTime::new(0),
                EventType::MessageDelivery {
                    from: n0,
                    to: n1,
                    payload: MessagePayload::Text("hello".into()),
                },
            );
            sim.schedule(
                VirtualTime::new(5),
                EventType::MessageDelivery {
                    from: n0,
                    to: n1,
                    payload: MessagePayload::Text("world".into()),
                },
            );

            sim.run(&mut rt);
            sim.event_log().unwrap().log_hash()
        }

        let hash1 = run_simulation();
        let hash2 = run_simulation();
        assert_eq!(hash1, hash2, "Live vs replay log hashes differ!");
    }

    #[test]
    fn test_checkpoint_state_validation() {
        use crate::node::{EchoNode, NodeRuntime, PingNode};
        use crate::simulation::Simulation;

        fn run_with_checkpoints() -> Vec<Checkpoint> {
            let mut sim = Simulation::new();
            sim.enable_logging_with_checkpoints(3);

            let mut rt = NodeRuntime::new();
            let n0 = NodeId::new(0);
            let n1 = NodeId::new(1);
            rt.register(n0, Box::new(PingNode::new(n0)));
            rt.register(n1, Box::new(EchoNode::new(n1)));

            for i in 0..10 {
                sim.schedule(
                    VirtualTime::new(i * 5),
                    EventType::MessageDelivery {
                        from: n0,
                        to: n1,
                        payload: MessagePayload::Text(format!("msg-{}", i)),
                    },
                );
            }

            sim.run(&mut rt);
            sim.event_log().unwrap().checkpoints().to_vec()
        }

        let cp1 = run_with_checkpoints();
        let cp2 = run_with_checkpoints();

        assert!(!cp1.is_empty(), "Should have produced checkpoints");
        assert_eq!(cp1, cp2, "Checkpoint state hashes differ between runs!");
    }
}
