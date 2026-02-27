/// Step-by-step simulation API for external interfaces.
///
/// Provides a controlled API for stepping through a simulation,
/// inspecting state, and exporting data as JSON strings. This is
/// the foundation for the WASM export, CLI tools, and any FFI.
///
/// **No external dependencies** — JSON is generated manually.

use crate::event::EventType;
use crate::node::{MessagePayload, NodeRuntime};
use crate::simulation::Simulation;

// ── StepResult ────────────────────────────────────────────────────────

/// Result of a single simulation step.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serialize", derive(serde::Serialize, serde::Deserialize))]
pub struct StepResult {
    /// The event that was dispatched.
    pub event_id: u64,
    /// Virtual time of the event.
    pub time: u64,
    /// String description of the event.
    pub description: String,
    /// Total events processed so far.
    pub total_events: u64,
}

// ── SimulationApi ─────────────────────────────────────────────────────

/// High-level API wrapping a simulation for external consumption.
///
/// Exposes step-by-step control, state inspection, and JSON export.
pub struct SimulationApi {
    sim: Simulation,
    rt: NodeRuntime,
}

impl SimulationApi {
    /// Create an API from a pre-built simulation and runtime.
    pub fn new(sim: Simulation, rt: NodeRuntime) -> Self {
        SimulationApi { sim, rt }
    }

    /// Execute a single step. Returns `None` if simulation is complete.
    pub fn step(&mut self) -> Option<StepResult> {
        let event = self.sim.step(&mut self.rt)?;
        Some(StepResult {
            event_id: event.id.raw(),
            time: event.scheduled_at.ticks(),
            description: describe_event(&event.payload),
            total_events: self.sim.events_processed(),
        })
    }

    /// Run to completion. Returns number of events processed.
    pub fn run(&mut self) -> u64 {
        self.sim.run(&mut self.rt)
    }

    /// Run up to `n` steps. Returns number actually processed.
    pub fn run_steps(&mut self, n: u64) -> u64 {
        self.sim.run_for(n, &mut self.rt)
    }

    /// Whether the simulation has finished (event queue empty).
    pub fn is_finished(&self) -> bool {
        self.sim.is_finished()
    }

    /// Current virtual time.
    pub fn current_time(&self) -> u64 {
        self.sim.current_time().ticks()
    }

    /// Total events processed.
    pub fn events_processed(&self) -> u64 {
        self.sim.events_processed()
    }

    /// Access the underlying `NodeRuntime` for inspection.
    pub fn runtime(&self) -> &NodeRuntime {
        &self.rt
    }

    /// Access the underlying `Simulation`.
    pub fn simulation(&self) -> &Simulation {
        &self.sim
    }

    /// Fork: create an independent copy of this API.
    pub fn fork(&self) -> Self {
        SimulationApi {
            sim: self.sim.fork(),
            rt: self.rt.fork(),
        }
    }

    // ── JSON Export ───────────────────────────────────────────

    /// Export a snapshot of the current state as a JSON string.
    #[cfg(feature = "serialize")]
    pub fn state_json(&self) -> String {
        #[derive(serde::Serialize)]
        struct NodeState {
            id: u64,
            alive: bool,
        }

        #[derive(serde::Serialize)]
        struct ApiState {
            current_time: u64,
            events_processed: u64,
            nodes: Vec<NodeState>,
            pending_events: usize,
            is_finished: bool,
        }

        let nodes = self.rt.all_node_ids()
            .into_iter()
            .map(|id| NodeState {
                id: id.raw(),
                alive: self.rt.is_alive(id),
            })
            .collect();

        let state = ApiState {
            current_time: self.sim.current_time().ticks(),
            events_processed: self.sim.events_processed(),
            nodes,
            pending_events: self.sim.pending_count(),
            is_finished: self.sim.is_finished(),
        };

        serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".into())
    }

    /// Export a snapshot of the current state as a JSON string.
    #[cfg(not(feature = "serialize"))]
    pub fn state_json(&self) -> String {
        let mut s = String::from("{\n");

        // Time + events.
        s.push_str(&format!(
            "  \"current_time\": {},\n  \"events_processed\": {},\n",
            self.sim.current_time().ticks(),
            self.sim.events_processed()
        ));

        // Nodes.
        s.push_str("  \"nodes\": [\n");
        let node_ids = self.rt.all_node_ids();
        for (i, id) in node_ids.iter().enumerate() {
            let alive = self.rt.is_alive(*id);
            s.push_str(&format!(
                "    {{\"id\": {}, \"alive\": {}}}",
                id.raw(),
                alive
            ));
            if i < node_ids.len() - 1 {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("  ],\n");

        // Pending events.
        s.push_str(&format!(
            "  \"pending_events\": {},\n",
            self.sim.pending_count()
        ));

        // Is finished.
        s.push_str(&format!(
            "  \"is_finished\": {}\n",
            self.sim.is_finished()
        ));

        s.push('}');
        s
    }

    /// Export the event trace as a JSON array string.
    #[cfg(feature = "serialize")]
    pub fn trace_json(&self) -> String {
        serde_json::to_string_pretty(&self.rt.trace).unwrap_or_else(|_| "[]".into())
    }

    /// Export the event trace as a JSON array string.
    #[cfg(not(feature = "serialize"))]
    pub fn trace_json(&self) -> String {
        let trace = &self.rt.trace;
        let mut s = String::from("[\n");
        for (i, entry) in trace.iter().enumerate() {
            s.push_str(&format!(
                "  {{\"time\": {}, \"event_id\": {}, \"node\": {}, \"type\": \"{}\"}}",
                entry.time.ticks(),
                entry.event_id.raw(),
                entry.node.raw(),
                describe_node_event(&entry.node_event),
            ));
            if i < trace.len() - 1 {
                s.push(',');
            }
            s.push('\n');
        }
        s.push(']');
        s
    }

    /// Export the event log (if enabled) as a JSON array string.
    #[cfg(feature = "serialize")]
    pub fn event_log_json(&self) -> String {
        match self.sim.event_log() {
            None => "[]".to_string(),
            Some(log) => serde_json::to_string_pretty(&log).unwrap_or_else(|_| "[]".into()),
        }
    }

    /// Export the event log (if enabled) as a JSON array string.
    #[cfg(not(feature = "serialize"))]
    pub fn event_log_json(&self) -> String {
        match self.sim.event_log() {
            None => "[]".to_string(),
            Some(log) => {
                let events = log.events();
                let mut s = String::from("[\n");
                for (i, event) in events.iter().enumerate() {
                    s.push_str(&format!(
                        "  {{\"id\": {}, \"time\": {}, \"type\": \"{}\"}}",
                        event.id.raw(),
                        event.scheduled_at.ticks(),
                        describe_event(&event.payload),
                    ));
                    if i < events.len() - 1 {
                        s.push(',');
                    }
                    s.push('\n');
                }
                s.push(']');
                s
            }
        }
    }

    /// Export a complete simulation snapshot (state + trace + log).
    #[cfg(feature = "serialize")]
    pub fn snapshot_json(&self) -> String {
        #[derive(serde::Serialize)]
        struct Snapshot {
            state: serde_json::Value,
            trace: serde_json::Value,
            event_log: serde_json::Value,
        }

        let snap = Snapshot {
            state: serde_json::from_str(&self.state_json()).unwrap_or(serde_json::Value::Null),
            trace: serde_json::from_str(&self.trace_json()).unwrap_or(serde_json::Value::Null),
            event_log: serde_json::from_str(&self.event_log_json()).unwrap_or(serde_json::Value::Null),
        };

        serde_json::to_string_pretty(&snap).unwrap_or_else(|_| "{}".into())
    }

    /// Export a complete simulation snapshot (state + trace + log).
    #[cfg(not(feature = "serialize"))]
    pub fn snapshot_json(&self) -> String {
        let mut s = String::from("{\n");
        s.push_str("  \"state\": ");
        s.push_str(&self.state_json());
        s.push_str(",\n  \"trace\": ");
        s.push_str(&self.trace_json());
        s.push_str(",\n  \"event_log\": ");
        s.push_str(&self.event_log_json());
        s.push_str("\n}");
        s
    }
}



// ── Describe helpers ──────────────────────────────────────────────────

fn describe_event(et: &EventType) -> String {
    match et {
        EventType::Noop => "Noop".to_string(),
        EventType::Log(msg) => format!("Log({})", msg),
        EventType::MessageSend { from, to, payload } => {
            format!("Send({}->{}, {})", from, to, describe_payload(payload))
        }
        EventType::MessageDelivery { from, to, payload } => {
            format!("Deliver({}->{}, {})", from, to, describe_payload(payload))
        }
        EventType::TimerFired { node, timer_id } => {
            format!("Timer({}, #{})", node, timer_id)
        }
        EventType::NodeCrash { node } => format!("Crash({})", node),
        EventType::NodeRecover { node } => format!("Recover({})", node),
        EventType::NetworkPartition { a, b } => {
            format!("Partition({}, {})", a, b)
        }
        EventType::NetworkHeal { a, b } => format!("Heal({}, {})", a, b),
    }
}

fn describe_payload(p: &MessagePayload) -> &str {
    match p {
        MessagePayload::Empty => "empty",
        MessagePayload::Text(_) => "text",
        MessagePayload::Data(_) => "data",
    }
}

fn describe_node_event(ne: &crate::node::NodeEvent) -> String {
    use crate::node::NodeEvent;
    match ne {
        NodeEvent::Message { from, .. } => format!("Message(from={})", from),
        NodeEvent::TimerFired { timer_id } => format!("Timer(#{})", timer_id),
        NodeEvent::Crash => "Crash".to_string(),
        NodeEvent::Recover => "Recover".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::SimulationBuilder;

    #[test]
    fn test_api_step_by_step() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);

        // Step 1: message delivery.
        let r1 = api.step().unwrap();
        assert_eq!(r1.time, 0);
        assert!(r1.description.contains("Deliver"));
        assert_eq!(r1.total_events, 1);

        // Step 2: echo back.
        let r2 = api.step().unwrap();
        assert_eq!(r2.time, 1);
        assert!(r2.description.contains("Deliver"));
        assert_eq!(r2.total_events, 2);

        // No more events.
        assert!(api.step().is_none());
        assert!(api.is_finished());
    }

    #[test]
    fn test_api_run() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        let n = api.run();
        assert_eq!(n, 2);
        assert!(api.is_finished());
    }

    #[test]
    fn test_api_run_steps() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "m1", 0)
            .deliver(0, 1, "m2", 5)
            .deliver(0, 1, "m3", 10)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        let n = api.run_steps(2);
        assert_eq!(n, 2);
        assert!(!api.is_finished());
    }

    #[test]
    fn test_api_state_json() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        api.run();

        let json = api.state_json();
        assert!(json.contains("current_time"));
        assert!(json.contains("events_processed"));
        assert!(json.contains("is_finished"));
    }

    #[test]
    fn test_api_trace_json() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        api.run();

        let json = api.trace_json();
        if cfg!(feature = "serialize") {
            assert!(json.contains("Message"));
        } else {
            assert!(json.contains("\"type\": \"Message(from=N0)\""));
            assert!(json.contains("\"node\": 1")); // echo node received msg
        }
    }

    #[test]
    fn test_api_event_log_json() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .with_logging()
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        api.run();

        let json = api.event_log_json();
        if cfg!(feature = "serialize") {
            assert!(json.contains("MessageDelivery"));
        } else {
            assert!(json.contains("\"id\": 0"));
            assert!(json.contains("Deliver"));
        }
    }

    #[test]
    fn test_api_fork() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .deliver(0, 1, "hello", 5)
            .build();

        let mut api = SimulationApi::new(sim, rt);

        // Fork before running.
        let mut fork = api.fork();

        // Run original.
        api.run();
        assert_eq!(api.events_processed(), 2);

        // Fork is independent.
        assert_eq!(fork.events_processed(), 0);
        fork.run();
        assert_eq!(fork.events_processed(), 2);
    }

    #[test]
    fn test_api_snapshot_json() {
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .with_logging()
            .deliver(0, 1, "hello", 0)
            .build();

        let mut api = SimulationApi::new(sim, rt);
        api.run();

        let json = api.snapshot_json();
        assert!(json.contains("\"state\":"));
        assert!(json.contains("\"trace\":"));
        assert!(json.contains("\"event_log\":"));
    }
}
