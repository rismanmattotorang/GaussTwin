use super::{Bounds, Position, Space, SpaceExtent, VecN};
use crate::agent::AgentId;
use crate::error::GaussTwinError;
use crate::error::Result;
use nalgebra::Vector3;
use rand;
use rand::Rng;
use std::collections::HashMap;

/// Continuous space implementation
#[derive(Debug)]
pub struct ContinuousSpace {
    /// Space bounds (min, max) for each dimension
    bounds: Bounds,
    /// Map from agent ID to position
    positions: HashMap<AgentId, Vector3<f64>>,
}

impl ContinuousSpace {
    /// Create new continuous space
    pub fn new(bounds: Bounds) -> Self {
        Self {
            bounds,
            positions: HashMap::new(),
        }
    }

    /// Check if position is valid
    fn is_valid_position(&self, pos: &Vector3<f64>) -> bool {
        pos.x >= self.bounds.min.x
            && pos.x <= self.bounds.max.x
            && pos.y >= self.bounds.min.y
            && pos.y <= self.bounds.max.y
            && pos.z >= self.bounds.min.z
            && pos.z <= self.bounds.max.z
    }

    /// Normalize position to be within bounds
    fn normalize_position(&self, pos: &Vector3<f64>) -> Vector3<f64> {
        let mut normalized = *pos;

        // Normalize x
        if normalized.x < self.bounds.min.x {
            normalized.x = self.bounds.max.x - (self.bounds.min.x - normalized.x);
        } else if normalized.x > self.bounds.max.x {
            normalized.x = self.bounds.min.x + (normalized.x - self.bounds.max.x);
        }

        // Normalize y
        if normalized.y < self.bounds.min.y {
            normalized.y = self.bounds.max.y - (self.bounds.min.y - normalized.y);
        } else if normalized.y > self.bounds.max.y {
            normalized.y = self.bounds.min.y + (normalized.y - self.bounds.max.y);
        }

        // Normalize z
        if normalized.z < self.bounds.min.z {
            normalized.z = self.bounds.max.z - (self.bounds.min.z - normalized.z);
        } else if normalized.z > self.bounds.max.z {
            normalized.z = self.bounds.min.z + (normalized.z - self.bounds.max.z);
        }

        normalized
    }

    /// Calculate distance between positions
    fn distance(&self, pos1: &Vector3<f64>, pos2: &Vector3<f64>) -> f64 {
        let diff = pos1 - pos2;
        diff.norm()
    }
}

impl Space for ContinuousSpace {
    fn add_agent(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        match position {
            Position::Continuous(coords) => {
                if self.positions.contains_key(&agent_id) {
                    return Err(GaussTwinError::InvalidPosition(format!(
                        "Agent {} already exists in space",
                        agent_id
                    )));
                }
                if !self.is_valid_position(&coords) {
                    return Err(GaussTwinError::InvalidPosition(
                        "Position out of bounds".to_string(),
                    ));
                }
                self.positions.insert(agent_id, coords);
                Ok(())
            }
            _ => Err(GaussTwinError::InvalidPosition(
                "Expected continuous position".to_string(),
            )),
        }
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if self.positions.remove(&agent_id).is_none() {
            return Err(GaussTwinError::InvalidPosition(format!(
                "Agent {} does not exist in space",
                agent_id
            )));
        }
        Ok(())
    }

    fn get_position(&self, agent_id: &AgentId) -> Option<Position> {
        self.positions
            .get(agent_id)
            .map(|v| Position::Continuous(v.clone()))
    }

    fn set_position(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        match position {
            Position::Continuous(coords) => {
                if !self.positions.contains_key(&agent_id) {
                    return Err(GaussTwinError::InvalidPosition(format!(
                        "Agent {} does not exist in space",
                        agent_id
                    )));
                }
                if !self.is_valid_position(&coords) {
                    return Err(GaussTwinError::InvalidPosition(
                        "Position out of bounds".to_string(),
                    ));
                }
                self.positions.insert(agent_id, coords);
                Ok(())
            }
            _ => Err(GaussTwinError::InvalidPosition(
                "Invalid position type for continuous space".to_string(),
            )),
        }
    }

    fn move_agent(&mut self, agent_id: AgentId, new_pos: Position) -> Result<()> {
        self.set_position(agent_id, new_pos)
    }

    fn get_agents_at(&self, pos: &Position) -> Vec<AgentId> {
        let coords = pos.coords();
        self.positions
            .iter()
            .filter(|(_, p)| *p == coords)
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn get_positions(&self) -> Vec<Position> {
        self.positions
            .values()
            .map(|v| Position::Continuous(v.clone()))
            .collect()
    }

    fn get_empty_positions(&self) -> Vec<Position> {
        // In continuous space, there are infinite positions
        // Return a sample of positions not too close to existing agents
        let mut empty = Vec::new();
        let bounds = self.bounds();
        let min_distance = 1.0; // Minimum distance between agents

        for _ in 0..10 {
            // Sample 10 positions
            let x =
                rand::random::<f64>() * (self.bounds.max.x - self.bounds.min.x) + self.bounds.min.x;
            let y =
                rand::random::<f64>() * (self.bounds.max.y - self.bounds.min.y) + self.bounds.min.y;
            let z =
                rand::random::<f64>() * (self.bounds.max.z - self.bounds.min.z) + self.bounds.min.z;
            let pos = Vector3::new(x, y, z);

            if !self.positions.values().any(|p| {
                let dx = p.x - x;
                let dy = p.y - y;
                let dz = p.z - z;
                (dx * dx + dy * dy + dz * dz).sqrt() < min_distance
            }) {
                empty.push(Position::Continuous(pos));
            }
        }
        empty
    }

    fn get_neighbors(&self, pos: &Position, radius: f64) -> Vec<Position> {
        let center = pos.coords();
        self.positions
            .values()
            .filter(|p| {
                let dx = p.x - center.x;
                let dy = p.y - center.y;
                let dz = p.z - center.z;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                dist > 0.0 && dist <= radius
            })
            .map(|v| Position::Continuous(v.clone()))
            .collect()
    }

    fn query_radius(&self, center: &Position, radius: f64) -> Vec<AgentId> {
        match center {
            Position::Continuous(center_coords) => self
                .positions
                .iter()
                .filter(|(_, pos)| {
                    let diff = *pos - center_coords;
                    diff.norm() <= radius
                })
                .map(|(id, _)| id.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    fn nearest_neighbors(&self, position: &Position, k: usize) -> Vec<AgentId> {
        match position {
            Position::Continuous(pos_coords) => {
                let mut distances: Vec<_> = self
                    .positions
                    .iter()
                    .map(|(id, pos)| {
                        let diff = pos - pos_coords;
                        (id.clone(), diff.norm())
                    })
                    .collect();
                distances
                    .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                distances.into_iter().take(k).map(|(id, _)| id).collect()
            }
            _ => Vec::new(),
        }
    }

    fn bounds(&self) -> Bounds {
        self.bounds.clone()
    }

    fn positions(&self) -> Vec<(AgentId, Position)> {
        self.positions
            .iter()
            .map(|(id, pos)| (id.clone(), Position::Continuous(pos.clone())))
            .collect()
    }

    fn extent(&self) -> SpaceExtent {
        SpaceExtent::Continuous(self.bounds.clone())
    }
}

/// A simple continuous space implementation using a HashMap
#[derive(Debug)]
pub struct HashMapSpace {
    positions: HashMap<AgentId, VecN>,
    bounds: Bounds,
}

impl HashMapSpace {
    pub fn new(bounds: Bounds) -> Self {
        Self {
            positions: HashMap::new(),
            bounds,
        }
    }
}

impl Space for HashMapSpace {
    fn add_agent(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        let coords = position.coords();
        self.positions.insert(agent_id, coords.clone());
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        self.positions.remove(&agent_id);
        Ok(())
    }

    fn get_position(&self, agent_id: &AgentId) -> Option<Position> {
        self.positions
            .get(agent_id)
            .map(|v| Position::Continuous(v.clone()))
    }

    fn set_position(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        let coords = position.coords();
        self.positions.insert(agent_id, coords.clone());
        Ok(())
    }

    fn get_agents_at(&self, pos: &Position) -> Vec<AgentId> {
        let coords = pos.coords();
        self.positions
            .iter()
            .filter(|(_, p)| *p == coords)
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn get_positions(&self) -> Vec<Position> {
        self.positions
            .values()
            .map(|v| Position::Continuous(v.clone()))
            .collect()
    }

    fn get_empty_positions(&self) -> Vec<Position> {
        let mut rng = rand::thread_rng();
        let mut positions = Vec::new();
        for _ in 0..10 {
            let x = rng.gen_range(self.bounds.min.x..=self.bounds.max.x);
            let y = rng.gen_range(self.bounds.min.y..=self.bounds.max.y);
            let z = rng.gen_range(self.bounds.min.z..=self.bounds.max.z);
            positions.push(Position::Continuous(VecN::new(x, y, z)));
        }
        positions
    }

    fn get_neighbors(&self, pos: &Position, radius: f64) -> Vec<Position> {
        let center = pos.coords();
        self.positions
            .values()
            .filter(|p| {
                let dx = p.x - center.x;
                let dy = p.y - center.y;
                let dz = p.z - center.z;
                (dx * dx + dy * dy + dz * dz).sqrt() <= radius
            })
            .map(|v| Position::Continuous(v.clone()))
            .collect()
    }

    fn query_radius(&self, center: &Position, radius: f64) -> Vec<AgentId> {
        let center_coords = center.coords();
        self.positions
            .iter()
            .filter(|(_, pos)| {
                let dx = pos.x - center_coords.x;
                let dy = pos.y - center_coords.y;
                let dz = pos.z - center_coords.z;
                (dx * dx + dy * dy + dz * dz).sqrt() <= radius
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn nearest_neighbors(&self, position: &Position, k: usize) -> Vec<AgentId> {
        let pos_coords = position.coords();
        let mut distances: Vec<_> = self
            .positions
            .iter()
            .map(|(id, pos)| {
                let dx = pos.x - pos_coords.x;
                let dy = pos.y - pos_coords.y;
                let dz = pos.z - pos_coords.z;
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                (id.clone(), dist)
            })
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        distances.into_iter().take(k).map(|(id, _)| id).collect()
    }

    fn bounds(&self) -> Bounds {
        self.bounds.clone()
    }

    fn positions(&self) -> Vec<(AgentId, Position)> {
        self.positions
            .iter()
            .map(|(id, pos)| (id.clone(), Position::Continuous(pos.clone())))
            .collect()
    }

    fn extent(&self) -> SpaceExtent {
        SpaceExtent::Continuous(self.bounds.clone())
    }
}
