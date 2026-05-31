pub mod continuous;
pub mod graph;
pub mod grid;
pub mod metrics;

use crate::agent::AgentId;
use crate::error::Result;
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Type alias for 3D vector
pub type VecN = Vector3<f64>;

/// Position in space
#[derive(Debug, Clone)]
pub enum Position {
    /// Grid position
    Grid(Vector3<f64>),
    /// Continuous position
    Continuous(Vector3<f64>),
}

impl Position {
    pub fn new(coords: VecN) -> Self {
        Self::Continuous(coords)
    }

    pub fn coords(&self) -> &VecN {
        match self {
            Position::Grid(v) => v,
            Position::Continuous(v) => v,
        }
    }
}

/// Bounds in space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub min: VecN,
    pub max: VecN,
}

impl Bounds {
    pub fn new(min: VecN, max: VecN) -> Self {
        Self { min, max }
    }

    pub fn contains(&self, point: &VecN) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }
}

/// Space trait for different spatial implementations
pub trait Space: Send + Sync + Debug {
    /// Add an agent to the space
    fn add_agent(&mut self, agent_id: AgentId, position: Position) -> Result<()>;

    /// Remove an agent from the space
    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()>;

    /// Get the position of an agent
    fn get_position(&self, agent_id: &AgentId) -> Option<Position>;

    /// Set the position of an agent
    fn set_position(&mut self, agent_id: AgentId, position: Position) -> Result<()>;

    /// Move an agent to a new position
    fn move_agent(&mut self, agent_id: AgentId, new_pos: Position) -> Result<()> {
        self.set_position(agent_id, new_pos)
    }

    /// Get all agents at a specific position
    fn get_agents_at(&self, pos: &Position) -> Vec<AgentId>;

    /// Get all positions in the space
    fn get_positions(&self) -> Vec<Position>;

    /// Get empty positions in the space
    fn get_empty_positions(&self) -> Vec<Position>;

    /// Get neighbors within a radius
    fn get_neighbors(&self, pos: &Position, radius: f64) -> Vec<Position>;

    /// Query agents within a radius
    fn query_radius(&self, center: &Position, radius: f64) -> Vec<AgentId>;

    /// Get k nearest neighbors
    fn nearest_neighbors(&self, position: &Position, k: usize) -> Vec<AgentId>;

    /// Get the bounds of the space
    fn bounds(&self) -> Bounds;

    /// Get all agent positions
    fn positions(&self) -> Vec<(AgentId, Position)>;

    /// Get the extent of the space
    fn extent(&self) -> SpaceExtent;
}

/// Extent of space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpaceExtent {
    /// Grid space extent
    Grid(Bounds),
    /// Continuous space extent
    Continuous(Bounds),
}
