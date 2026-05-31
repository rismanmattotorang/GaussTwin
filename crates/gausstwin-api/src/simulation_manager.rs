//! Simulation Manager
//!
//! Manages running simulations - simplified version for API integration

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{Error, Result};

/// Status of a managed simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SimulationState {
    Idle,
    Running,
    Paused,
    Completed,
    Failed,
}

/// A simulation entry
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimulationEntry {
    pub id: String,
    pub name: String,
    pub state: SimulationState,
    pub current_step: u64,
    pub config: Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Manages all active simulations
/// This is a simplified manager that tracks simulation state
/// Real simulation execution is delegated to gausstwin-core
pub struct SimulationManager {
    simulations: Arc<RwLock<HashMap<String, SimulationEntry>>>,
}

impl SimulationManager {
    /// Create a new simulation manager
    pub fn new() -> Self {
        Self {
            simulations: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new simulation
    pub async fn register(&self, id: String, name: String, config: Value) -> Result<()> {
        let mut sims = self.simulations.write().await;

        if sims.contains_key(&id) {
            return Err(Error::Validation("Simulation already exists".into()));
        }

        sims.insert(
            id.clone(),
            SimulationEntry {
                id: id.clone(),
                name,
                state: SimulationState::Idle,
                current_step: 0,
                config,
                created_at: chrono::Utc::now(),
            },
        );

        info!("Registered simulation: {}", id);
        Ok(())
    }

    /// Update simulation state
    pub async fn update_state(&self, id: &str, state: SimulationState) -> Result<()> {
        let mut sims = self.simulations.write().await;

        let sim = sims
            .get_mut(id)
            .ok_or_else(|| Error::NotFound("Simulation not found".into()))?;

        sim.state = state;
        info!("Updated simulation {} state to {:?}", id, state);
        Ok(())
    }

    /// Update simulation step
    pub async fn update_step(&self, id: &str, step: u64) -> Result<()> {
        let mut sims = self.simulations.write().await;

        let sim = sims
            .get_mut(id)
            .ok_or_else(|| Error::NotFound("Simulation not found".into()))?;

        sim.current_step = step;
        Ok(())
    }

    /// Get simulation state
    pub async fn get_state(&self, id: &str) -> Result<SimulationState> {
        let sims = self.simulations.read().await;

        let sim = sims
            .get(id)
            .ok_or_else(|| Error::NotFound("Simulation not found".into()))?;

        Ok(sim.state)
    }

    /// Get simulation entry
    pub async fn get(&self, id: &str) -> Result<SimulationEntry> {
        let sims = self.simulations.read().await;

        sims.get(id)
            .cloned()
            .ok_or_else(|| Error::NotFound("Simulation not found".into()))
    }

    /// Delete a simulation
    pub async fn remove(&self, id: &str) -> Result<()> {
        let mut sims = self.simulations.write().await;

        if sims.remove(id).is_some() {
            info!("Removed simulation: {}", id);
            Ok(())
        } else {
            Err(Error::NotFound("Simulation not found".into()))
        }
    }

    /// List all simulations
    pub async fn list(&self) -> Vec<SimulationEntry> {
        let sims = self.simulations.read().await;
        sims.values().cloned().collect()
    }

    /// Count active simulations
    pub async fn count(&self) -> usize {
        let sims = self.simulations.read().await;
        sims.len()
    }

    /// Get running simulations count
    pub async fn count_running(&self) -> usize {
        let sims = self.simulations.read().await;
        sims.values()
            .filter(|s| s.state == SimulationState::Running)
            .count()
    }
}

impl Default for SimulationManager {
    fn default() -> Self {
        Self::new()
    }
}
