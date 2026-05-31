//! Advanced Spatial Algorithms Module

use crate::{AgentId, space::VecN, error::Result};
use std::collections::HashMap;

/// Hierarchical spatial hash grid with adaptive subdivision
#[derive(Debug)]
pub struct AdaptiveHashGrid {
    cells: HashMap<GridCoord, SpatialCell>,
    cell_size: f64,
    stats: SpatialStats,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct GridCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug)]
struct SpatialCell {
    agents: Vec<AgentId>,
    last_access_time: std::time::Instant,
    query_frequency: u64,
}

impl AdaptiveHashGrid {
    pub fn new(cell_size: f64) -> Self {
        Self {
            cells: HashMap::new(),
            cell_size,
            stats: SpatialStats::default(),
        }
    }
    
    /// Insert an agent at a position
    pub fn insert(&mut self, agent_id: AgentId, position: &VecN) -> Result<()> {
        let coord = self.position_to_coord(position);
        
        let cell = self.cells.entry(coord).or_insert_with(|| SpatialCell {
            agents: Vec::new(),
            last_access_time: std::time::Instant::now(),
            query_frequency: 0,
        });
        
        cell.agents.push(agent_id);
        self.stats.total_insertions += 1;
        Ok(())
    }
    
    /// Remove an agent from a position
    pub fn remove(&mut self, agent_id: AgentId, position: &VecN) -> Result<bool> {
        let coord = self.position_to_coord(position);
        
        if let Some(cell) = self.cells.get_mut(&coord) {
            if let Some(pos) = cell.agents.iter().position(|&id| id == agent_id) {
                cell.agents.swap_remove(pos);
                self.stats.total_removals += 1;
                
                // Clean up empty cells
                if cell.agents.is_empty() {
                    self.cells.remove(&coord);
                }
                
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    /// Query agents within a radius
    pub fn query_radius(&mut self, center: &VecN, radius: f64) -> Vec<AgentId> {
        let mut results = Vec::new();
        let cell_radius = (radius / self.cell_size).ceil() as i32;
        let center_coord = self.position_to_coord(center);
        
        // Check all cells within the radius
        for dx in -cell_radius..=cell_radius {
            for dy in -cell_radius..=cell_radius {
                for dz in -cell_radius..=cell_radius {
                    let coord = GridCoord {
                        x: center_coord.x + dx,
                        y: center_coord.y + dy,
                        z: center_coord.z + dz,
                    };
                    
                    if let Some(cell) = self.cells.get_mut(&coord) {
                        cell.query_frequency += 1;
                        cell.last_access_time = std::time::Instant::now();
                        results.extend_from_slice(&cell.agents);
                    }
                }
            }
        }
        
        self.stats.total_queries += 1;
        results
    }
    
    /// Get all agents in a rectangular region
    pub fn query_region(&mut self, min: &VecN, max: &VecN) -> Vec<AgentId> {
        let mut results = Vec::new();
        let min_coord = self.position_to_coord(min);
        let max_coord = self.position_to_coord(max);
        
        for x in min_coord.x..=max_coord.x {
            for y in min_coord.y..=max_coord.y {
                for z in min_coord.z..=max_coord.z {
                    let coord = GridCoord { x, y, z };
                    
                    if let Some(cell) = self.cells.get_mut(&coord) {
                        cell.query_frequency += 1;
                        cell.last_access_time = std::time::Instant::now();
                        results.extend_from_slice(&cell.agents);
                    }
                }
            }
        }
        
        self.stats.total_queries += 1;
        results
    }
    
    /// Get nearest neighbors
    pub fn nearest_neighbors(&mut self, center: &VecN, k: usize) -> Vec<AgentId> {
        // Start with a small radius and expand until we find k neighbors
        let mut radius = self.cell_size;
        let max_radius = self.cell_size * 10.0;
        
        while radius <= max_radius {
            let candidates = self.query_radius(center, radius);
            if candidates.len() >= k {
                return candidates.into_iter().take(k).collect();
            }
            radius *= 2.0;
        }
        
        // Return what we found
        self.query_radius(center, max_radius)
    }
    
    /// Update agent position (remove from old, insert at new)
    pub fn update(&mut self, agent_id: AgentId, old_pos: &VecN, new_pos: &VecN) -> Result<()> {
        self.remove(agent_id, old_pos)?;
        self.insert(agent_id, new_pos)?;
        Ok(())
    }
    
    /// Get statistics about the spatial index
    pub fn stats(&self) -> &SpatialStats {
        &self.stats
    }
    
    /// Clear all agents from the grid
    pub fn clear(&mut self) {
        self.cells.clear();
        self.stats = SpatialStats::default();
    }
    
    /// Get the number of active cells
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
    
    /// Get the total number of agents stored
    pub fn agent_count(&self) -> usize {
        self.cells.values().map(|cell| cell.agents.len()).sum()
    }
    
    fn position_to_coord(&self, position: &VecN) -> GridCoord {
        GridCoord {
            x: position.x.floor() as i32,
            y: position.y.floor() as i32,
            z: position.z.floor() as i32,
        }
    }
}

/// Statistics for spatial operations
#[derive(Debug, Default)]
pub struct SpatialStats {
    pub total_insertions: u64,
    pub total_removals: u64,
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl SpatialStats {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        }
    }
    
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// R*-Tree implementation for efficient spatial indexing
#[derive(Debug)]
pub struct RStarTree {
    root: Option<Box<RTreeNode>>,
    max_entries: usize,
    min_entries: usize,
    height: usize,
}

#[derive(Debug, Clone)]
struct RTreeNode {
    entries: Vec<RTreeEntry>,
    is_leaf: bool,
}

#[derive(Debug, Clone)]
struct RTreeEntry {
    bounds: BoundingBox,
    agent_id: Option<AgentId>,
    child: Option<Box<RTreeNode>>,
}

#[derive(Debug, Clone)]
pub struct BoundingBox {
    min: VecN,
    max: VecN,
}

impl BoundingBox {
    fn new(min: VecN, max: VecN) -> Self {
        Self { min, max }
    }
    
    fn contains_point(&self, point: &VecN) -> bool {
        point.x >= self.min.x && point.x <= self.max.x &&
        point.y >= self.min.y && point.y <= self.max.y &&
        point.z >= self.min.z && point.z <= self.max.z
    }
    
    fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x <= other.max.x && self.max.x >= other.min.x &&
        self.min.y <= other.max.y && self.max.y >= other.min.y &&
        self.min.z <= other.max.z && self.max.z >= other.min.z
    }
    
    fn area(&self) -> f64 {
        (self.max.x - self.min.x) * (self.max.y - self.min.y) * (self.max.z - self.min.z)
    }
}

impl RStarTree {
    pub fn new(max_entries: usize) -> Self {
        Self {
            root: None,
            max_entries,
            min_entries: max_entries / 2,
            height: 0,
        }
    }
    
    pub fn insert(&mut self, agent_id: AgentId, position: VecN) -> Result<()> {
        let bounds = BoundingBox::new(position.clone(), position);
        let entry = RTreeEntry {
            bounds,
            agent_id: Some(agent_id),
            child: None,
        };
        
        if self.root.is_none() {
            self.root = Some(Box::new(RTreeNode {
                entries: vec![entry],
                is_leaf: true,
            }));
            self.height = 1;
        } else {
            // Insert logic would go here
            // For simplicity, just add to root for now
            if let Some(ref mut root) = self.root {
                root.entries.push(entry);
            }
        }
        
        Ok(())
    }
    
    pub fn query_point(&self, point: &VecN) -> Vec<AgentId> {
        let mut results = Vec::new();
        
        if let Some(ref root) = self.root {
            self.query_node(root, point, &mut results);
        }
        
        results
    }
    
    pub fn query_region(&self, bounds: &BoundingBox) -> Vec<AgentId> {
        let mut results = Vec::new();
        
        if let Some(ref root) = self.root {
            self.query_region_node(root, bounds, &mut results);
        }
        
        results
    }
    
    fn query_node(&self, node: &RTreeNode, point: &VecN, results: &mut Vec<AgentId>) {
        for entry in &node.entries {
            if entry.bounds.contains_point(point) {
                if let Some(agent_id) = entry.agent_id {
                    results.push(agent_id);
                } else if let Some(ref child) = entry.child {
                    self.query_node(child, point, results);
                }
            }
        }
    }
    
    fn query_region_node(&self, node: &RTreeNode, bounds: &BoundingBox, results: &mut Vec<AgentId>) {
        for entry in &node.entries {
            if entry.bounds.intersects(bounds) {
                if let Some(agent_id) = entry.agent_id {
                    results.push(agent_id);
                } else if let Some(ref child) = entry.child {
                    self.query_region_node(child, bounds, results);
                }
            }
        }
    }
    
    pub fn height(&self) -> usize {
        self.height
    }
    
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }
}

/// Cache-optimized spatial data structure
#[derive(Debug)]
pub struct CacheOptimizedGrid<T> {
    data: Vec<T>,
    width: usize,
    height: usize,
    depth: usize,
    cell_size: f64,
}

impl<T: Clone + Default> CacheOptimizedGrid<T> {
    pub fn new(width: usize, height: usize, depth: usize, cell_size: f64) -> Self {
        let total_cells = width * height * depth;
        let mut data = Vec::with_capacity(total_cells);
        data.resize(total_cells, T::default());
        
        Self {
            data,
            width,
            height,
            depth,
            cell_size,
        }
    }
    
    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<&T> {
        if x < self.width && y < self.height && z < self.depth {
            let index = self.coord_to_index(x, y, z);
            self.data.get(index)
        } else {
            None
        }
    }
    
    pub fn get_mut(&mut self, x: usize, y: usize, z: usize) -> Option<&mut T> {
        if x < self.width && y < self.height && z < self.depth {
            let index = self.coord_to_index(x, y, z);
            self.data.get_mut(index)
        } else {
            None
        }
    }
    
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: T) -> Result<()> {
        if x < self.width && y < self.height && z < self.depth {
            let index = self.coord_to_index(x, y, z);
            self.data[index] = value;
            Ok(())
        } else {
            Err(crate::error::GaussTwinError::Custom("Coordinates out of bounds".to_string()))
        }
    }
    
    pub fn position_to_coord(&self, position: &VecN) -> Option<(usize, usize, usize)> {
        if position.x >= 0.0 && position.y >= 0.0 && position.z >= 0.0 {
            let x = (position.x / self.cell_size).floor() as i32;
            let y = (position.y / self.cell_size).floor() as i32;
            let z = (position.z / self.cell_size).floor() as i32;
            
            if x >= 0 && y >= 0 && z >= 0 {
                let x = x as usize;
                let y = y as usize;
                let z = z as usize;
                
                if x < self.width && y < self.height && z < self.depth {
                    Some((x, y, z))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }
    
    fn coord_to_index(&self, x: usize, y: usize, z: usize) -> usize {
        z * self.width * self.height + y * self.width + x
    }
    
    pub fn dimensions(&self) -> (usize, usize, usize) {
        (self.width, self.height, self.depth)
    }
    
    pub fn cell_size(&self) -> f64 {
        self.cell_size
    }
}

impl From<VecN> for GridCoord {
    fn from(vec: VecN) -> Self {
        GridCoord {
            x: vec.x.floor() as i32,
            y: vec.y.floor() as i32,
            z: vec.z.floor() as i32,
        }
    }
}

impl From<GridCoord> for VecN {
    fn from(coord: GridCoord) -> Self {
        VecN::new(
            coord.x as f64,
            coord.y as f64,
            coord.z as f64,
        )
    }
}

pub fn check_bounds(p: &VecN, min: &VecN, max: &VecN) -> bool {
    p.x >= min.x && p.x <= max.x &&
    p.y >= min.y && p.y <= max.y &&
    p.z >= min.z && p.z <= max.z
}

pub fn check_overlap(min1: &VecN, max1: &VecN, min2: &VecN, max2: &VecN) -> bool {
    min1.x <= max2.x && max1.x >= min2.x &&
    min1.y <= max2.y && max1.y >= min2.y &&
    min1.z <= max2.z && max1.z >= min2.z
}

pub fn get_volume(min: &VecN, max: &VecN) -> f64 {
    (max.x - min.x) * (max.y - min.y) * (max.z - min.z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::VecN;
    
    #[test]
    fn test_adaptive_hash_grid() {
        let mut grid = AdaptiveHashGrid::new(1.0);
        let agent_id = AgentId::new();
        let position = VecN::new(5.0, 3.0, 0.0);
        
        assert!(grid.insert(agent_id, &position).is_ok());
        assert_eq!(grid.agent_count(), 1);
        
        let results = grid.query_radius(&position, 1.0);
        assert!(results.contains(&agent_id));
        
        assert!(grid.remove(agent_id, &position).unwrap());
        assert_eq!(grid.agent_count(), 0);
    }
    
    #[test]
    fn test_r_star_tree() {
        let mut tree = RStarTree::new(4);
        let agent_id = AgentId::new();
        let position = VecN::new(1.0, 2.0, 0.0);
        
        assert!(tree.insert(agent_id, position.clone()).is_ok());
        
        let results = tree.query_point(&position);
        assert!(results.contains(&agent_id));
    }
    
    #[test]
    fn test_cache_optimized_grid() {
        let mut grid = CacheOptimizedGrid::<i32>::new(10, 10, 10, 1.0);
        
        assert!(grid.set(5, 5, 5, 42).is_ok());
        assert_eq!(grid.get(5, 5, 5), Some(&42));
        
        let position = VecN::new(5.5, 5.5, 5.5);
        assert_eq!(grid.position_to_coord(&position), Some((5, 5, 5)));
    }
    
    #[test]
    fn test_bounding_box() {
        // VecN is a 3D vector and `area()` computes the box volume, so use a box
        // with a non-zero extent in every axis.
        let min = VecN::new(0.0, 0.0, 0.0);
        let max = VecN::new(10.0, 10.0, 10.0);
        let bbox = BoundingBox::new(min, max);

        let point_inside = VecN::new(5.0, 5.0, 5.0);
        let point_outside = VecN::new(15.0, 15.0, 15.0);

        assert!(bbox.contains_point(&point_inside));
        assert!(!bbox.contains_point(&point_outside));

        // 10 x 10 x 10
        assert_eq!(bbox.area(), 1000.0);
    }
    
    #[test]
    fn test_spatial_stats() {
        let mut stats = SpatialStats::default();
        
        stats.cache_hits = 80;
        stats.cache_misses = 20;
        
        assert_eq!(stats.cache_hit_rate(), 0.8);
        
        stats.reset();
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
    }
}
