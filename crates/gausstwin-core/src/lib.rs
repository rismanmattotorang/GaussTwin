//! GaussTwin Core Library
//!
//! This library provides the core functionality for the GaussTwin agent-based modeling framework.
//! It includes implementations for spaces, agents, and model management.
//!
//! # Features
//! - High-performance agent-based simulation
//! - Multiple space types (grid, continuous, graph)
//! - GPU acceleration support
//! - Distributed computing capabilities
//! - Comprehensive profiling and monitoring
//!
//! # Example
//! ```no_run
//! use gausstwin_core::{Model, ModelConfig, Agent, AgentId};
//!
//! // Create a model configuration
//! let config = ModelConfig::new("My Simulation".to_string());
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("features `std` and `no_std` are mutually exclusive");

// Re-exports
pub use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

// Core modules
pub mod agent;
pub mod error;
pub mod event;
pub mod metrics;
pub mod model;
pub mod pool;
pub mod scheduler;
pub mod space;
pub mod time;

// Convenience re-exports
pub use agent::{Agent, AgentId, AgentState, BasicAgent};
pub use error::{GaussTwinError, Result};
pub use event::{Event, EventKind, EventQueue};
pub use metrics::{Measurable, MetricsCollector, MetricsConfig};
pub use model::{Model, ModelConfig, ModelMetrics};
pub use scheduler::{Scheduler, SchedulerKind};
pub use space::{Bounds, Position, Space, SpaceExtent, VecN};
pub use time::{Duration, SimTime, TimeStep};

/// Type alias for entity identifiers
pub type EntityId = u64;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build information  
pub const BUILD_INFO: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_NAME"),
    " ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info() {
        assert!(!VERSION.is_empty());
        assert!(!BUILD_INFO.is_empty());
    }
}

/// Advanced high-performance computing module
pub mod hpc;

/// GPU acceleration and CUDA integration
pub mod gpu;

/// Machine learning and AI integration
pub mod ai;

/// Advanced spatial algorithms and data structures
pub mod spatial;

/// Real-time streaming and data ingestion
pub mod streaming;

/// Distributed computing and federation
pub mod distributed;

/// Quantum-inspired algorithms
pub mod quantum;

/// Blockchain integration for audit trails
pub mod blockchain;

/// Advanced visualization and rendering
pub mod viz;

/// Performance profiling and optimization
pub mod profiler;
