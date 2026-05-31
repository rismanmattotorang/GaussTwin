use crate::agent::AgentId;
use crate::error::Result;
use crate::space::{Bounds, Position, Space, SpaceExtent, VecN};
use petgraph::graph::{Graph, NodeIndex};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
/// Graph-based space implementation
pub struct GraphSpace {
    graph: Graph<Vec<AgentId>, f64>,
    positions: HashMap<NodeIndex, VecN>,
    agent_nodes: HashMap<AgentId, NodeIndex>,
}

impl GraphSpace {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            positions: HashMap::new(),
            agent_nodes: HashMap::new(),
        }
    }

    fn find_node_with_agent(&self, agent_id: &AgentId) -> Option<NodeIndex> {
        self.graph.node_indices().find(|&idx| {
            let agents = &self.graph[idx];
            agents.contains(agent_id)
        })
    }

    fn get_neighbors(&self, node: NodeIndex, distance: usize) -> HashSet<NodeIndex> {
        let mut visited = HashSet::new();
        let mut frontier = HashSet::new();
        frontier.insert(node);
        visited.insert(node);

        for _ in 0..distance {
            let mut new_frontier = HashSet::new();
            for &node in &frontier {
                for neighbor in self.graph.neighbors(node) {
                    if !visited.contains(&neighbor) {
                        new_frontier.insert(neighbor);
                    }
                }
            }
            visited.extend(&frontier);
            if new_frontier.is_empty() {
                break;
            }
            frontier = new_frontier;
        }

        visited
    }
}

impl Space for GraphSpace {
    fn add_agent(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        let coords = position.coords().clone();
        let node = self.graph.add_node(vec![agent_id.clone()]);
        self.positions.insert(node, coords);
        self.agent_nodes.insert(agent_id, node);
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(node) = self.agent_nodes.remove(&agent_id) {
            if let Some(agents) = self.graph.node_weight_mut(node) {
                agents.retain(|id| *id != agent_id);
                if agents.is_empty() {
                    self.graph.remove_node(node);
                    self.positions.remove(&node);
                }
            }
        }
        Ok(())
    }

    fn get_position(&self, agent_id: &AgentId) -> Option<Position> {
        self.agent_nodes
            .get(agent_id)
            .and_then(|node| self.positions.get(node))
            .map(|pos| Position::Continuous(pos.clone()))
    }

    fn set_position(&mut self, agent_id: AgentId, position: Position) -> Result<()> {
        if let Some(node) = self.agent_nodes.get(&agent_id) {
            self.positions.insert(*node, position.coords().clone());
        }
        Ok(())
    }

    fn get_agents_at(&self, pos: &Position) -> Vec<AgentId> {
        let coords = pos.coords();
        self.positions
            .iter()
            .filter(|(_, p)| *p == coords)
            .flat_map(|(node, _)| self.graph.node_weight(*node).into_iter().flatten().cloned())
            .collect()
    }

    fn move_agent(&mut self, agent_id: AgentId, new_pos: Position) -> Result<()> {
        self.set_position(agent_id, new_pos)
    }

    fn get_positions(&self) -> Vec<Position> {
        self.positions
            .values()
            .map(|pos| Position::Continuous(pos.clone()))
            .collect()
    }

    fn get_empty_positions(&self) -> Vec<Position> {
        Vec::new() // Graph space has infinite positions
    }

    fn get_neighbors(&self, pos: &Position, radius: f64) -> Vec<Position> {
        let center = pos.coords();
        let radius_sq = radius * radius;

        self.positions
            .values()
            .filter(|p| {
                let diff = *p - center;
                diff.dot(&diff) <= radius_sq
            })
            .map(|pos| Position::Continuous(pos.clone()))
            .collect()
    }

    fn query_radius(&self, center: &Position, radius: f64) -> Vec<AgentId> {
        let center_coords = center.coords();
        let radius_sq = radius * radius;

        self.positions
            .iter()
            .filter(|(_, pos)| {
                let diff = *pos - center_coords;
                diff.dot(&diff) <= radius_sq
            })
            .flat_map(|(node, _)| self.graph.node_weight(*node).into_iter().flatten().cloned())
            .collect()
    }

    fn nearest_neighbors(&self, position: &Position, k: usize) -> Vec<AgentId> {
        let pos_coords = position.coords();
        let mut distances: Vec<_> = self
            .positions
            .iter()
            .flat_map(|(node, pos)| {
                let diff = pos - pos_coords;
                let dist = diff.dot(&diff).sqrt();
                self.graph
                    .node_weight(*node)
                    .into_iter()
                    .flatten()
                    .map(move |agent_id| (agent_id.clone(), dist))
            })
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.into_iter().take(k).map(|(id, _)| id).collect()
    }

    fn bounds(&self) -> Bounds {
        if self.positions.is_empty() {
            return Bounds {
                min: VecN::new(0.0, 0.0, 0.0),
                max: VecN::new(0.0, 0.0, 0.0),
            };
        }

        let mut min = VecN::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = VecN::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);

        for pos in self.positions.values() {
            min.x = min.x.min(pos.x);
            min.y = min.y.min(pos.y);
            min.z = min.z.min(pos.z);
            max.x = max.x.max(pos.x);
            max.y = max.y.max(pos.y);
            max.z = max.z.max(pos.z);
        }

        Bounds { min, max }
    }

    fn positions(&self) -> Vec<(AgentId, Position)> {
        self.agent_nodes
            .iter()
            .filter_map(|(agent_id, node)| {
                self.positions
                    .get(node)
                    .map(|pos| (agent_id.clone(), Position::Continuous(pos.clone())))
            })
            .collect()
    }

    fn extent(&self) -> SpaceExtent {
        SpaceExtent::Continuous(self.bounds())
    }
}
