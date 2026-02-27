# Helios WASM & Visualization

Helios provides a first-class WebAssembly export, allowing you to run, visualize, and debug your distributed systems directly in the browser.

## The WASM Architecture

The WASM layer is built on top of the `SimulationApi` (`src/api.rs`). This API provides a controlled interface for:
1. **Stepping**: Moving the simulation forward by exactly one event.
2. **State Export**: Converting internal Rust structs (like `NodeRuntime`) into JSON for the browser.
3. **Trace Inspection**: Viewing the history of events.

## Building the WASM Target

Helios uses `wasm-pack` for compilation.

### Prerequisites
- [rustup](https://rustup.rs/) (wasm32-unknown-unknown target)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

### Compilation Command
```bash
wasm-pack build --target web --features wasm
```

This creates a `pkg/` directory containing the `.wasm` binary and a JavaScript wrapper.

## Running the Web Visualizer

The project includes a reference dashboard in `web/index.html`.

### 1. Serve the Project
Since raw WASM modules cannot be loaded via `file://`, you must use a local server:

```bash
# Using npx
npx serve .

# Or using Python
python -m http.server
```

### 2. Access the Dashboard
Navigate to `http://localhost:3000/web/` (or the port provided by your server).

## Custom Visualizers

You can build your own UI on top of the Helios WASM package.

### Basic JS Usage
```javascript
import init, { Simulator } from './pkg/helios.js';

async function run() {
    await init();
    const sim = new Simulator(); // Uses the default scenario
    
    // Step forward
    const event = JSON.parse(sim.step());
    console.log(`Time: ${event.time}, Action: ${event.description}`);
    
    // Check node status
    const state = JSON.parse(sim.state_json());
    console.log(state.nodes);
}
```

## UI Optimization
- **Batch Processing**: For large simulations, use `sim.run_steps(100)` instead of stepping one by one to avoid UI thread lag.
- **Trace Window**: The `trace_json()` can grow large. Display only the last 50-100 events in your UI for better performance.
- **Node pulsing**: The visualizer should "pulse" nodes when they receive a `MessageDelivery` event to show network activity.
