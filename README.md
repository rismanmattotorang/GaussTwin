# 🌟 GaussTwin

> **High-Performance Digital Twin Framework**  
> *Rust-Powered • Multi-Language • WASM-Ready • Cross-Platform UI*

[![Rust](https://img.shields.io/badge/rust-1.74+-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/python-3.8+-blue.svg)](https://www.python.org)
[![TypeScript](https://img.shields.io/badge/typescript-5.0+-blue.svg)](https://www.typescriptlang.org)
[![WASM](https://img.shields.io/badge/wasm-compatible-green.svg)](https://webassembly.org)
[![Tauri](https://img.shields.io/badge/tauri-2.0-blue.svg)](https://tauri.app)

GaussTwin is a high-performance digital twin framework built in Rust with Python and TypeScript bindings. It provides a robust foundation for building complex simulations with efficient space management, agent-based modeling, and AI/ML integration capabilities.

## 🖥️ User Interfaces

GaussTwin provides three comprehensive user interfaces:

| Interface | Description | Status |
|-----------|-------------|--------|
| **Web UI** | Modern React-based dashboard with real-time visualization | ✅ Ready |
| **Desktop App** | Native cross-platform app built with Tauri 2.0 | ✅ Ready |
| **Terminal UI** | Feature-rich TUI built with Ratatui | ✅ Ready |

### Quick Launch

```bash
# Web UI (Development)
cd ui/web && npm run dev

# Desktop App
cd ui/desktop && npm run tauri dev

# Terminal UI
cd ui/tui && cargo run --release
```

## 🚀 Core Features

### Space Management
- **Grid Space** – N-dimensional grid with cell-based partitioning
- **Continuous Space** – Efficient spatial hashing and neighbor search
- **Graph Space** – Directed/undirected graph support with path queries
- **Performance** – O(1) lookups for grid, O(log n) for continuous space

### Agent System
- **Basic Agent** – Generic state and behavior framework
- **Message Passing** – Efficient agent communication
- **Context Management** – Time tracking and shared state
- **Error Handling** – Comprehensive error types and recovery

### Language Bindings
- **Python** – NumPy integration and native data conversion
- **TypeScript** – WASM compilation and browser support
- **Example Code** – Ready-to-use simulation templates
- **Documentation** – Comprehensive API guides

## 📊 Implementation Status

### Completed Features
```rust
✅ Core space management system
✅ Basic agent framework  
✅ Python bindings with NumPy
✅ TypeScript bindings with WASM
✅ Grid, continuous, and graph spaces
✅ Advanced pathfinding (A*, Dijkstra, HPA*, D* Lite)
✅ Spatial indexing (KD-tree, R*-tree, Grid Hash)
✅ Web UI with React + TailwindCSS
✅ Desktop UI with Tauri 2.0
✅ Terminal UI with Ratatui
✅ REST/GraphQL/gRPC API server
✅ Real-time WebSocket streaming
✅ Discrete Event Simulation (DES)
✅ Finite State Machines (FSM)
✅ Co-simulation support (FMI, HLA)
✅ High-performance object pooling
✅ BDI/Cognitive/Reactive agent architectures
```

### In Progress
```rust
🔄 Advanced AI/ML integration (LLM, MARL)
🔄 Distributed computing support
🔄 GPU acceleration (Vulkan, Metal, WebGPU)
🔄 Performance profiling with NUMA awareness
```

### Planned Features
```rust
📋 Quantum algorithm integration
📋 Advanced neural agents
📋 Blockchain integration
📋 Extended visualization tools
```

## 💻 Quick Start

### Python Integration
```python
from gausstwin import Space, Agent, Model

# Create a continuous space
space = Space.continuous(
    bounds=[(0, 100), (0, 100)],
    cell_size=1.0
)

# Define an agent
class MyAgent(Agent):
    def step(self, context):
        # Agent behavior
        position = self.get_position()
        neighbors = space.query_radius(position, 5.0)
        # Process neighbors
        pass

# Create and run simulation
model = Model(space=space)
model.add_agents([MyAgent() for _ in range(100)])
model.run(steps=1000)
```

### TypeScript/WASM Integration
```typescript
import { Space, Agent, Model } from '@gausstwin/wasm';

// Create a grid space
const space = Space.grid({
    dimensions: [100, 100],
    cellSize: 1
});

// Define an agent
class MyAgent extends Agent {
    step(context) {
        // Agent behavior
        const position = this.getPosition();
        const neighbors = space.getNeighbors(position, 1);
        // Process neighbors
    }
}

// Create and run simulation
const model = new Model(space);
model.addAgents(Array(100).fill(null).map(() => new MyAgent()));
model.run(1000);
```

## 🛠 System Requirements

### Core Requirements
- Rust 1.74+
- Cargo and standard Rust toolchain
- Git for version control

### Python Bindings
- Python 3.8+
- NumPy
- Development tools (pip, venv)

### TypeScript Bindings
- Node.js 18+
- npm or yarn
- WASM target support

## ⚙️ Build Instructions

> 📖 **See [`docs/BUILD.md`](docs/BUILD.md) for the authoritative, up-to-date build
> guide** — system prerequisites, optional features, and how to build the bindings/UIs.

The default workspace build is light and hermetic (no network-fetched native blobs):

```sh
# From the project root:
cargo build --workspace
cargo test  --workspace
```

The libtorch-backed ML stack is opt-in behind a feature flag:

```sh
cargo build -p gausstwin-ai --features torch   # requires libtorch
```

> **Prerequisites:** a C toolchain, `cmake`, `pkg-config`, `libssl-dev`,
> `libsasl2-dev`, and `protobuf-compiler` (for gRPC). See `docs/BUILD.md` for the
> exact package list.

## 📚 Documentation

- [API Reference](docs/api/README.md)
- [Python Guide](docs/python/README.md)
- [TypeScript Guide](docs/typescript/README.md)
- [Examples](examples/README.md)
- [Performance Tips](docs/performance.md)

## 🤝 Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 