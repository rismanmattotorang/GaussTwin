use crate::{
    common::{DistanceMetric, HighPerformanceMemoryPool, SpatialCache},
    error::SpatialError,
    Point,
};
use dashmap::DashMap;
use kdtree::{distance::squared_euclidean, KdTree};
use parking_lot::RwLock;
use rayon::prelude::*;
use rstar::{RTree, RTreeObject, AABB};
use smallvec::{smallvec, SmallVec};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "simd")]
use std::simd::f64x4;

#[cfg(not(feature = "simd"))]
use crate::common::simd_stub::f64x4;

/// Spatial index implementations
#[derive(Debug)]
pub enum SpatialIndex {
    KdTree(RwLock<KdTree<f64, usize, [f64; 3]>>),
    GridHash {
        cell_size: f64,
        // Store the full point (position + id), not just the id: query_radius needs
        // the real position to compute exact distances.
        cells: DashMap<(i64, i64, i64), SmallVec<[SpatialPoint; 8]>>,
    },
    RTree(RwLock<RTree<SpatialPoint>>),
    Octree(RwLock<Octree>),
}

/// Point type for R*-tree
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialPoint {
    pub position: [f64; 3],
    pub id: usize,
}

impl RTreeObject for SpatialPoint {
    type Envelope = AABB<[f64; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.position)
    }
}

/// Octree node for spatial partitioning
#[derive(Debug)]
pub struct OctreeNode {
    center: Point,
    half_size: f64,
    points: SmallVec<[SpatialPoint; 8]>,
    children: Option<Box<[OctreeNode; 8]>>,
}

impl OctreeNode {
    fn new(center: Point, half_size: f64) -> Self {
        Self {
            center,
            half_size,
            points: SmallVec::new(),
            children: None,
        }
    }

    fn insert(&mut self, point: SpatialPoint, max_points: usize, min_size: f64) {
        // Internal node: descend into the child octant containing the point.
        if self.children.is_some() {
            let p: Point = point.position.into();
            let child_idx = self.get_child_index(&p);
            if let Some(children) = &mut self.children {
                children[child_idx].insert(point, max_points, min_size);
            }
            return;
        }

        // Leaf node: store the point here.
        self.points.push(point);

        // Split once we exceed capacity, provided the resulting children would not
        // be smaller than the minimum cell size. `split` drains `self.points` into
        // the new children, so the point just inserted is redistributed too.
        if self.points.len() > max_points && self.half_size * 0.5 >= min_size {
            self.split();
        }
    }

    fn split(&mut self) {
        let mut children = Box::new([
            OctreeNode::new(self.get_child_center(0), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(1), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(2), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(3), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(4), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(5), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(6), self.half_size * 0.5),
            OctreeNode::new(self.get_child_center(7), self.half_size * 0.5),
        ]);

        for point in self.points.drain(..) {
            let p: Point = point.position.into();
            let idx = {
                let mut index = 0;
                if p.x >= self.center.x {
                    index |= 1;
                }
                if p.y >= self.center.y {
                    index |= 2;
                }
                if p.z >= self.center.z {
                    index |= 4;
                }
                index
            };
            children[idx].points.push(point);
        }

        self.children = Some(children);
    }

    fn get_child_center(&self, index: usize) -> Point {
        let offset = self.half_size * 0.5;
        let x = self.center.x + if index & 1 != 0 { offset } else { -offset };
        let y = self.center.y + if index & 2 != 0 { offset } else { -offset };
        let z = self.center.z + if index & 4 != 0 { offset } else { -offset };
        Point::new(x, y, z)
    }

    fn get_child_index(&self, point: &Point) -> usize {
        let mut index = 0;
        if point.x >= self.center.x {
            index |= 1;
        }
        if point.y >= self.center.y {
            index |= 2;
        }
        if point.z >= self.center.z {
            index |= 4;
        }
        index
    }

    fn query_radius(&self, center: &Point, radius: f64, result: &mut Vec<SpatialPoint>) {
        let dist_sq = squared_distance_to_box(center, &self.get_bounds());
        if dist_sq > radius * radius {
            return;
        }

        for point in &self.points {
            let dx = center.x - point.position[0];
            let dy = center.y - point.position[1];
            let dz = center.z - point.position[2];
            let dist_sq = dx * dx + dy * dy + dz * dz;
            if dist_sq <= radius * radius {
                result.push(*point);
            }
        }

        if let Some(children) = &self.children {
            for child in children.iter() {
                child.query_radius(center, radius, result);
            }
        }
    }

    fn get_bounds(&self) -> AABB<[f64; 3]> {
        let min = [
            self.center.x - self.half_size,
            self.center.y - self.half_size,
            self.center.z - self.half_size,
        ];
        let max = [
            self.center.x + self.half_size,
            self.center.y + self.half_size,
            self.center.z + self.half_size,
        ];
        AABB::from_corners(min, max)
    }
}

/// Octree implementation
#[derive(Debug)]
pub struct Octree {
    root: OctreeNode,
    max_points: usize,
    min_size: f64,
}

impl Octree {
    pub fn new(center: Point, size: f64, max_points: usize, min_size: f64) -> Self {
        Self {
            root: OctreeNode::new(center, size * 0.5),
            max_points,
            min_size,
        }
    }

    pub fn insert(&mut self, point: SpatialPoint) {
        self.root.insert(point, self.max_points, self.min_size);
    }

    pub fn query_radius(&self, center: &Point, radius: f64) -> Vec<SpatialPoint> {
        let mut result = Vec::new();
        self.root.query_radius(center, radius, &mut result);
        result
    }
}

impl SpatialIndex {
    /// Create a new KD-tree index
    pub fn new_kdtree() -> Self {
        Self::KdTree(RwLock::new(KdTree::new(3)))
    }

    /// Create a new grid hash index
    pub fn new_grid_hash(cell_size: f64) -> Self {
        Self::GridHash {
            cell_size,
            cells: DashMap::new(),
        }
    }

    /// Create a new R*-tree index
    pub fn new_rtree() -> Self {
        Self::RTree(RwLock::new(RTree::new()))
    }

    /// Create a new octree index
    pub fn new_octree(center: Point, size: f64) -> Self {
        Self::Octree(RwLock::new(Octree::new(center, size, 8, 0.1)))
    }

    /// Insert a point into the index
    pub fn insert(&self, point: Point, id: usize) -> Result<(), SpatialError> {
        match self {
            Self::KdTree(tree) => {
                let mut tree = tree.write();
                tree.add([point.x, point.y, point.z], id)
                    .map_err(|_| SpatialError::InsertionFailed)?;
            }
            Self::GridHash { cell_size, cells } => {
                let cell_x = (point.x / cell_size).floor() as i64;
                let cell_y = (point.y / cell_size).floor() as i64;
                let cell_z = (point.z / cell_size).floor() as i64;
                cells
                    .entry((cell_x, cell_y, cell_z))
                    .or_default()
                    .push(SpatialPoint {
                        position: [point.x, point.y, point.z],
                        id,
                    });
            }
            Self::RTree(tree) => {
                let mut tree = tree.write();
                tree.insert(SpatialPoint {
                    position: [point.x, point.y, point.z],
                    id,
                });
            }
            Self::Octree(tree) => {
                let mut tree = tree.write();
                tree.insert(SpatialPoint {
                    position: [point.x, point.y, point.z],
                    id,
                });
            }
        }
        Ok(())
    }

    /// Query points within radius using SIMD acceleration where possible
    pub fn query_radius(&self, center: Point, radius: f64) -> Vec<usize> {
        match self {
            Self::KdTree(tree) => {
                let tree = tree.read();
                let hits = tree.within(
                    &[center.x, center.y, center.z],
                    radius * radius,
                    &squared_euclidean,
                );
                hits.into_iter()
                    .flat_map(|inner| inner.into_iter().map(|(_, id)| *id))
                    .collect::<Vec<_>>()
            }
            Self::GridHash { cell_size, cells } => {
                // Scan every cell that could overlap the query sphere. Use one extra
                // ring of cells (`+ 1`) because `center` is generally not aligned to
                // a cell boundary, so the reachable cell-index span can exceed
                // ceil(radius / cell_size) by one.
                let cell_radius = (radius / cell_size).ceil() as i64 + 1;
                let center_x = (center.x / cell_size).floor() as i64;
                let center_y = (center.y / cell_size).floor() as i64;
                let center_z = (center.z / cell_size).floor() as i64;

                let radius_sq = radius * radius;
                let mut result = Vec::new();

                for dx in -cell_radius..=cell_radius {
                    for dy in -cell_radius..=cell_radius {
                        for dz in -cell_radius..=cell_radius {
                            if let Some(cell) =
                                cells.get(&(center_x + dx, center_y + dy, center_z + dz))
                            {
                                // Test each point's *actual* position against the
                                // radius (the previous code used the cell corner as a
                                // proxy, which misclassified points near the boundary).
                                for sp in cell.value() {
                                    let ddx = sp.position[0] - center.x;
                                    let ddy = sp.position[1] - center.y;
                                    let ddz = sp.position[2] - center.z;
                                    if ddx * ddx + ddy * ddy + ddz * ddz <= radius_sq {
                                        result.push(sp.id);
                                    }
                                }
                            }
                        }
                    }
                }
                result
            }
            Self::RTree(tree) => {
                let tree = tree.read();
                let query_box = AABB::from_corners(
                    [center.x - radius, center.y - radius, center.z - radius],
                    [center.x + radius, center.y + radius, center.z + radius],
                );
                tree.locate_in_envelope(&query_box)
                    .map(|point| point.id)
                    .collect()
            }
            Self::Octree(tree) => {
                let tree = tree.read();
                tree.query_radius(&center, radius)
                    .into_iter()
                    .map(|point| point.id)
                    .collect()
            }
        }
    }

    /// Bulk insert points using parallel processing
    pub fn bulk_insert(&self, points: Vec<(Point, usize)>) -> Result<(), SpatialError> {
        match self {
            Self::KdTree(tree_lock) => {
                let mut tree = tree_lock.write();
                for (point, id) in points {
                    tree.add([point.x, point.y, point.z], id)
                        .map_err(|_| SpatialError::InsertionFailed)?;
                }
            }
            Self::GridHash { cell_size, cells } => {
                points.into_par_iter().for_each(|(point, id)| {
                    let cell_x = (point.x / cell_size).floor() as i64;
                    let cell_y = (point.y / cell_size).floor() as i64;
                    let cell_z = (point.z / cell_size).floor() as i64;
                    cells
                        .entry((cell_x, cell_y, cell_z))
                        .or_default()
                        .push(SpatialPoint {
                            position: [point.x, point.y, point.z],
                            id,
                        });
                });
            }
            Self::RTree(tree_lock) => {
                let mut tree = tree_lock.write();
                for (point, id) in points {
                    tree.insert(SpatialPoint {
                        position: [point.x, point.y, point.z],
                        id,
                    });
                }
            }
            Self::Octree(tree_lock) => {
                let mut tree = tree_lock.write();
                for (point, id) in points {
                    tree.insert(SpatialPoint {
                        position: [point.x, point.y, point.z],
                        id,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Helper function to calculate squared distance between a point and an AABB
fn squared_distance_to_box(point: &Point, bounds: &AABB<[f64; 3]>) -> f64 {
    let lower = bounds.lower();
    let upper = bounds.upper();

    let dx = if point.x < lower[0] {
        lower[0] - point.x
    } else if point.x > upper[0] {
        point.x - upper[0]
    } else {
        0.0
    };

    let dy = if point.y < lower[1] {
        lower[1] - point.y
    } else if point.y > upper[1] {
        point.y - upper[1]
    } else {
        0.0
    };

    let dz = if point.z < lower[2] {
        lower[2] - point.z
    } else if point.z > upper[2] {
        point.z - upper[2]
    } else {
        0.0
    };

    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, SeedableRng};

    #[test]
    fn test_kdtree() {
        let index = SpatialIndex::new_kdtree();
        test_index_implementation(&index);
    }

    #[test]
    fn test_grid_hash() {
        let index = SpatialIndex::new_grid_hash(1.0);
        test_index_implementation(&index);
    }

    #[test]
    fn test_rtree() {
        let index = SpatialIndex::new_rtree();
        test_index_implementation(&index);
    }

    #[test]
    fn test_octree() {
        let index = SpatialIndex::new_octree(Point::new(0.0, 0.0, 0.0), 100.0);
        test_index_implementation(&index);
    }

    fn test_index_implementation(index: &SpatialIndex) {
        // Seeded RNG so the index tests are deterministic/reproducible (Phase 2):
        // a fixed seed gives the same point set every run, so a failure is a real
        // bug rather than boundary-condition flakiness from random data.
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0FFEE);
        let mut points = Vec::new();

        // Generate random points
        for i in 0..1000 {
            let point = Point::new(
                rng.gen_range(-50.0..50.0),
                rng.gen_range(-50.0..50.0),
                rng.gen_range(-50.0..50.0),
            );
            points.push((point, i));
        }

        // Test bulk insert
        index.bulk_insert(points.clone()).unwrap();

        // Test radius query
        let center = Point::new(0.0, 0.0, 0.0);
        let radius = 10.0;
        let results = index.query_radius(center, radius);

        // Verify results
        for (point, id) in &points {
            let dx = point.x - center.x;
            let dy = point.y - center.y;
            let dz = point.z - center.z;
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();

            if dist <= radius {
                assert!(results.contains(id));
            }
        }
    }

    #[test]
    fn test_parallel_bulk_insert() {
        let index = SpatialIndex::new_kdtree();
        let mut points = Vec::new();
        let mut rng = rand::thread_rng();

        // Generate many points
        for i in 0..10000 {
            let point = Point::new(
                rng.gen_range(-100.0..100.0),
                rng.gen_range(-100.0..100.0),
                rng.gen_range(-100.0..100.0),
            );
            points.push((point, i));
        }

        // Measure parallel insertion time
        let start = Instant::now();
        index.bulk_insert(points.clone()).unwrap();
        let parallel_time = start.elapsed();

        // Verify all points were inserted
        let center = Point::new(0.0, 0.0, 0.0);
        let radius = 200.0; // Large enough to cover all points
        let results = index.query_radius(center, radius);
        assert_eq!(results.len(), points.len());
    }
}
