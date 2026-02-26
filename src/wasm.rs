#![cfg(feature = "wasm")]

use wasm_bindgen::prelude::*;
use crate::api::SimulationApi;
use crate::dsl::SimulationBuilder;

/// WASM binding for the Simulation API.
///
/// Exposes the pure Rust `SimulationApi` to JavaScript.
#[wasm_bindgen]
pub struct Simulator {
    api: SimulationApi,
}

#[wasm_bindgen]
impl Simulator {
    /// Create a new interactive simulation from a predefined scenario.
    /// In a real app, this could parse JSON to build a dynamic scenario.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        
        // Build a default interesting scenario.
        let (sim, rt) = SimulationBuilder::new()
            .ping(0)
            .echo(1)
            .echo(2)
            .reliable_network(42)
            .with_logging()
            .send(0, 1, "hello-n1", 10)
            .send(0, 2, "hello-n2", 15)
            .crash(1, 30)
            .recover(1, 50)
            .send(0, 1, "are you there?", 40)
            .build();
            
        Simulator {
            api: SimulationApi::new(sim, rt)
        }
    }

    /// Execute a single step of the simulation.
    /// Returns a JSON string describing what happened, or null if finished.
    pub fn step(&mut self) -> Option<String> {
        let result = self.api.step()?;
        // Convert the simplified result into a JSON string manually (or via serde_json later).
        // Since we have serde_json via the wasm feature, we can use it, but to keep it simple and zero-dep in api, we'll format:
        Some(format!(
            "{{\"event_id\":{},\"time\":{},\"description\":\"{}\",\"total_events\":{}}}",
            result.event_id,
            result.time,
            result.description.replace('"', "\\\""), // simple escape
            result.total_events
        ))
    }

    /// Run up to `n` steps.
    pub fn run_steps(&mut self, n: u32) -> u32 {
        self.api.run_steps(n as u64) as u32
    }

    /// Run until completion.
    pub fn run_all(&mut self) -> u32 {
        self.api.run() as u32
    }

    /// Export the state of all nodes as a JSON string.
    pub fn state_json(&self) -> String {
        self.api.state_json()
    }

    /// Export the full event trace history as a JSON string.
    pub fn trace_json(&self) -> String {
        self.api.trace_json()
    }

    /// Returns `true` if the simulation is finished.
    pub fn is_finished(&self) -> bool {
        self.api.is_finished()
    }

    /// Get current virtual time.
    pub fn current_time(&self) -> u32 {
        self.api.current_time() as u32
    }
}
