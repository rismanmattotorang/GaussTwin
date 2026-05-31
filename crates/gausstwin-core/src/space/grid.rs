use crate::agent::AgentId;
use crate::error::Result;
use crate::space::{Bounds, Position, Space, SpaceExtent, VecN};
use std::collections::HashMap;

/// Grid-based space implementation
#[derive(Debug)]
pub struct GridSpace {
    cells: HashMap<(i32, i32, i32), Vec<AgentId>>,
    cell_size: f64,
}

impl GridSpace {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cells: HashMap::new(),
            cell_size,
        }
    }

    fn get_cell_coords(&self, pos: &VecN) -> (i32, i32, i32) {
        (
            (pos.x / self.cell_size).floor() as i32,
            (pos.y / self.cell_size).floor() as i32,
            (pos.z / self.cell_size).floor() as i32,
        )
    }

    fn get_cell_center(&self, coords: (i32, i32, i32)) -> VecN {
        VecN::new(
            (coords.0 as f64 + 0.5) * self.cell_size,
            (coords.1 as f64 + 0.5) * self.cell_size,
            (coords.2 as f64 + 0.5) * self.cell_size,
        )
    }
}

impl Space for GridSpace {
    fn add_agent(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        let coords = self.get_cell_coords(position.coords());
        self.cells
            .entry(coords)
            .or_insert_with(Vec::new)
            .push(agent_id);
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        for agents in self.cells.values_mut() {
            if let Some(pos) = agents.iter().position(|id| *id == agent_id) {
                agents.remove(pos);
                break;
            }
        }
        Ok(())
    }

    fn get_position(&self, agent_id: &AgentId) -> Option<Position> {
        for (coords, agents) in &self.cells {
            if agents.contains(agent_id) {
                return Some(Position::Grid(self.get_cell_center(*coords)));
            }
        }
        None
    }

    fn set_position(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        self.remove_agent(agent_id)?;
        self.add_agent(agent_id, position)
    }

    fn get_agents_at(&self, pos: &Position) -> Vec<AgentId> {
        let coords = self.get_cell_coords(pos.coords());
        self.cells
            .get(&coords)
            .map(|agents| agents.clone())
            .unwrap_or_default()
    }

    fn move_agent(&mut self, agent_id: AgentId, new_pos: Position) -> Result<()> {
        self.set_position(agent_id, new_pos)
    }

    fn get_positions(&self) -> Vec<Position> {
        self.cells
            .iter()
            .flat_map(|(coords, agents)| {
                let pos = self.get_cell_center(*coords);
                agents.iter().map(move |_| Position::Grid(pos.clone()))
            })
            .collect()
    }

    fn get_empty_positions(&self) -> Vec<Position> {
        let mut empty = Vec::new();
        let bounds = self.bounds();

        for x in (bounds.min.x.floor() as i32)..=(bounds.max.x.ceil() as i32) {
            for y in (bounds.min.y.floor() as i32)..=(bounds.max.y.ceil() as i32) {
                for z in (bounds.min.z.floor() as i32)..=(bounds.max.z.ceil() as i32) {
                    let coords = (x, y, z);
                    if !self.cells.contains_key(&coords) {
                        empty.push(Position::Grid(self.get_cell_center(coords)));
                    }
                }
            }
        }
        empty
    }

    fn get_neighbors(&self, pos: &Position, radius: f64) -> Vec<Position> {
        let center_coords = self.get_cell_coords(pos.coords());
        let cell_radius = (radius / self.cell_size).ceil() as i32;
        let mut neighbors = Vec::new();

        for dx in -cell_radius..=cell_radius {
            for dy in -cell_radius..=cell_radius {
                for dz in -cell_radius..=cell_radius {
                    let neighbor_coords = (
                        center_coords.0 + dx,
                        center_coords.1 + dy,
                        center_coords.2 + dz,
                    );
                    let neighbor_pos = self.get_cell_center(neighbor_coords);
                    let dist = (neighbor_pos - pos.coords()).magnitude();
                    if dist <= radius {
                        neighbors.push(Position::Grid(neighbor_pos));
                    }
                }
            }
        }
        neighbors
    }

    fn query_radius(&self, center: &Position, radius: f64) -> Vec<AgentId> {
        let center_coords = self.get_cell_coords(center.coords());
        let cell_radius = (radius / self.cell_size).ceil() as i32;
        let mut result = Vec::new();

        for dx in -cell_radius..=cell_radius {
            for dy in -cell_radius..=cell_radius {
                for dz in -cell_radius..=cell_radius {
                    let pos = (
                        center_coords.0 + dx,
                        center_coords.1 + dy,
                        center_coords.2 + dz,
                    );
                    if let Some(agents) = self.cells.get(&pos) {
                        let pos_center = self.get_cell_center(pos);
                        let dist = (pos_center - center.coords()).magnitude();
                        if dist <= radius {
                            result.extend(agents.iter().cloned());
                        }
                    }
                }
            }
        }
        result
    }

    fn nearest_neighbors(&self, position: &Position, k: usize) -> Vec<AgentId> {
        let pos_coords = position.coords();
        let mut distances: Vec<_> = self
            .cells
            .iter()
            .flat_map(|(coords, agents)| {
                let cell_center = self.get_cell_center(*coords);
                let dist = (cell_center - pos_coords).magnitude();
                agents.iter().map(move |agent_id| (agent_id.clone(), dist))
            })
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.into_iter().take(k).map(|(id, _)| id).collect()
    }

    fn bounds(&self) -> Bounds {
        if self.cells.is_empty() {
            return Bounds {
                min: VecN::new(0.0, 0.0, 0.0),
                max: VecN::new(0.0, 0.0, 0.0),
            };
        }

        let mut min = VecN::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = VecN::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

        for coords in self.cells.keys() {
            let pos = self.get_cell_center(*coords);
            min.x = min.x.min(pos.x - self.cell_size / 2.0);
            min.y = min.y.min(pos.y - self.cell_size / 2.0);
            min.z = min.z.min(pos.z - self.cell_size / 2.0);
            max.x = max.x.max(pos.x + self.cell_size / 2.0);
            max.y = max.y.max(pos.y + self.cell_size / 2.0);
            max.z = max.z.max(pos.z + self.cell_size / 2.0);
        }

        Bounds { min, max }
    }

    fn positions(&self) -> Vec<(AgentId, Position)> {
        self.cells
            .iter()
            .flat_map(|(coords, agents)| {
                let pos = self.get_cell_center(*coords);
                agents
                    .iter()
                    .map(move |agent_id| (agent_id.clone(), Position::Grid(pos.clone())))
            })
            .collect()
    }

    fn extent(&self) -> SpaceExtent {
        let bounds = self.bounds();
        SpaceExtent::Grid(bounds)
    }
}
