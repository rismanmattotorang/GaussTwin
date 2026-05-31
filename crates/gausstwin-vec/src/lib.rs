//! GaussTwin Vector Store
//!
//! High-performance vector database with enterprise features including:
//! - SIMD-accelerated vector operations
//! - Advanced caching and load balancing
//! - High availability and clustering
//! - Comprehensive metrics and monitoring
//! - Vector aggregations and analytics
//!
//! # Features
//! - Multiple index types (IVF, HNSW, etc.)
//! - Multiple distance metrics (L2, IP, Cosine)
//! - SIMD acceleration for vector operations
//! - Advanced caching strategies
//! - Cluster management and HA
//! - Vector analytics and aggregations
//!
//! # Examples
//! ```no_run
//! use gausstwin_vec::{VectorStore, MilvusStore, IndexParams, IndexType, MetricType};
//!
//! async fn example() -> Result<(), VectorError> {
//!     let store = MilvusStore::new(
//!         "localhost",
//!         19530,
//!         "collection",
//!         128,
//!         Default::default(),
//!         Default::default(),
//!     ).await?;
//!     
//!     // Create HNSW index
//!     let index = IndexParams {
//!         index_type: IndexType::HNSW,
//!         metric_type: MetricType::L2,
//!         params: serde_json::json!({
//!             "M": 16,
//!             "efConstruction": 200
//!         }),
//!     };
//!     
//!     store.create_index(index).await?;
//!     Ok(())
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{RwLock, Semaphore};
use tracing::error;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[derive(Debug, Error)]
pub enum VectorError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndexType {
    IVFFlat,
    IVFSQ8,
    IVFPQ,
    HNSW,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    L2,
    IP,
    Cosine,
    Hamming,
    Jaccard,
}

impl std::fmt::Display for IndexType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexType::IVFFlat => write!(f, "IVF_FLAT"),
            IndexType::IVFSQ8 => write!(f, "IVF_SQ8"),
            IndexType::IVFPQ => write!(f, "IVF_PQ"),
            IndexType::HNSW => write!(f, "HNSW"),
            IndexType::Flat => write!(f, "FLAT"),
        }
    }
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricType::L2 => write!(f, "L2"),
            MetricType::IP => write!(f, "IP"),
            MetricType::Cosine => write!(f, "COSINE"),
            MetricType::Hamming => write!(f, "HAMMING"),
            MetricType::Jaccard => write!(f, "JACCARD"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vector {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub vector: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
}

pub struct VectorStore {
    vectors: Arc<RwLock<Vec<Vector>>>,
    dimension: usize,
    metric_type: MetricType,
    max_connections: Arc<Semaphore>,
}

impl VectorStore {
    pub fn new(dimension: usize, metric_type: MetricType) -> Self {
        Self {
            vectors: Arc::new(RwLock::new(Vec::new())),
            dimension,
            metric_type,
            max_connections: Arc::new(Semaphore::new(100)), // Limit concurrent operations
        }
    }

    pub async fn add_vectors(&self, vectors: Vec<Vector>) -> Result<(), VectorError> {
        // Validate vectors
        for vector in &vectors {
            if vector.vector.len() != self.dimension {
                return Err(VectorError::InvalidInput(format!(
                    "Vector dimension mismatch. Expected {}, got {}",
                    self.dimension,
                    vector.vector.len()
                )));
            }
        }

        let _permit = self.max_connections.acquire().await.map_err(|e| {
            VectorError::DatabaseError(format!("Failed to acquire connection: {}", e))
        })?;

        let mut store = self.vectors.write().await;
        store.extend(vectors);
        Ok(())
    }

    pub async fn search(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, VectorError> {
        if query.len() != self.dimension {
            return Err(VectorError::InvalidInput(format!(
                "Query dimension mismatch. Expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let _permit = self.max_connections.acquire().await.map_err(|e| {
            VectorError::DatabaseError(format!("Failed to acquire connection: {}", e))
        })?;

        let store = self.vectors.read().await;
        let mut results: Vec<(usize, f32)> = store
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let score = match self.metric_type {
                    MetricType::L2 => -l2_distance(&v.vector, query),
                    MetricType::IP => dot_product(&v.vector, query),
                    MetricType::Cosine => {
                        let dot = dot_product(&v.vector, query);
                        let norm1 = dot_product(&v.vector, &v.vector).sqrt();
                        let norm2 = dot_product(query, query).sqrt();
                        dot / (norm1 * norm2)
                    }
                    _ => unimplemented!("Metric type not implemented"),
                };
                (i, score)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        Ok(results
            .into_iter()
            .map(|(i, score)| {
                let vector = &store[i];
                SearchResult {
                    id: vector.id.clone(),
                    score,
                    vector: vector.vector.clone(),
                    metadata: vector.metadata.clone(),
                }
            })
            .collect())
    }

    pub async fn delete_vectors(&self, ids: &[String]) -> Result<(), VectorError> {
        let _permit = self.max_connections.acquire().await.map_err(|e| {
            VectorError::DatabaseError(format!("Failed to acquire connection: {}", e))
        })?;

        let mut store = self.vectors.write().await;
        store.retain(|v| !ids.contains(&v.id));
        Ok(())
    }

    pub async fn get_vector(&self, id: &str) -> Result<Vector, VectorError> {
        let _permit = self.max_connections.acquire().await.map_err(|e| {
            VectorError::DatabaseError(format!("Failed to acquire connection: {}", e))
        })?;

        let store = self.vectors.read().await;
        store
            .iter()
            .find(|v| v.id == id)
            .cloned()
            .ok_or_else(|| VectorError::NotFound(format!("Vector with id {} not found", id)))
    }
}

// SIMD implementations
//
// SAFETY (applies to both AVX2 helpers below):
// - They use AVX2 intrinsics, so callers MUST only invoke them after confirming
//   AVX2 is available (`is_x86_feature_detected!("avx2")`); the `#[target_feature]`
//   attribute encodes that requirement. The public `l2_distance`/`dot_product`
//   wrappers perform that check.
// - Each 8-wide `_mm256_loadu_ps(&x[i])` reads `x[i..i+8]`. The loop bound
//   `n_simd = n - (n % 8)` guarantees `i + 8 <= n`, so every load is in bounds for a
//   slice of length `n`.
// - They assume `a.len() == b.len()`; the public wrappers assert this before calling.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_l2_distance_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = _mm256_setzero_ps();
    let n = a.len();
    let n_simd = n - (n % 8); // Process in chunks of 8 floats

    for i in (0..n_simd).step_by(8) {
        let va = _mm256_loadu_ps(&a[i]);
        let vb = _mm256_loadu_ps(&b[i]);
        let diff = _mm256_sub_ps(va, vb);
        sum = _mm256_add_ps(sum, _mm256_mul_ps(diff, diff));
    }

    let mut result = [0.0f32; 8];
    _mm256_storeu_ps(result.as_mut_ptr(), sum);

    let mut total = result.iter().sum::<f32>();

    // Handle remaining elements
    for i in n_simd..n {
        let diff = a[i] - b[i];
        total += diff * diff;
    }

    total.sqrt()
}

fn compute_l2_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = x - y;
            diff * diff
        })
        .sum::<f32>()
        .sqrt()
}

// SAFETY: see the note above `compute_l2_distance_simd`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn compute_dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = _mm256_setzero_ps();
    let n = a.len();
    let n_simd = n - (n % 8); // Process in chunks of 8 floats

    for i in (0..n_simd).step_by(8) {
        let va = _mm256_loadu_ps(&a[i]);
        let vb = _mm256_loadu_ps(&b[i]);
        sum = _mm256_add_ps(sum, _mm256_mul_ps(va, vb));
    }

    let mut result = [0.0f32; 8];
    _mm256_storeu_ps(result.as_mut_ptr(), sum);

    let mut total = result.iter().sum::<f32>();

    // Handle remaining elements
    for i in n_simd..n {
        total += a[i] * b[i];
    }

    total
}

fn compute_dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum()
}

// Public interface that chooses the appropriate implementation
pub fn l2_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have the same length");

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { compute_l2_distance_simd(a, b) }
        } else {
            compute_l2_distance(a, b)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        compute_l2_distance(a, b)
    }
}

pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have the same length");

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { compute_dot_product_simd(a, b) }
        } else {
            compute_dot_product(a, b)
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        compute_dot_product(a, b)
    }
}

// K-means clustering implementation
pub fn kmeans_clustering(
    vectors: &[Vec<f32>],
    k: u32,
    max_iterations: u32,
) -> Result<Vec<Vec<f32>>, VectorError> {
    use rand::seq::IteratorRandom;
    let mut rng = rand::thread_rng();

    if vectors.is_empty() {
        return Err(VectorError::InvalidInput("Empty vector list".to_string()));
    }
    if k as usize > vectors.len() {
        return Err(VectorError::InvalidInput(
            "k is larger than number of vectors".to_string(),
        ));
    }

    let mut centroids = vectors
        .iter()
        .cloned()
        .choose_multiple(&mut rng, k as usize);

    for _ in 0..max_iterations {
        // Assign points to clusters
        let mut clusters: Vec<Vec<&Vec<f32>>> = vec![Vec::new(); k as usize];
        for vector in vectors.iter() {
            let mut min_dist = f32::INFINITY;
            let mut cluster_idx = 0;

            for (i, centroid) in centroids.iter().enumerate() {
                let dist = l2_distance(vector, centroid);
                if dist < min_dist {
                    min_dist = dist;
                    cluster_idx = i;
                }
            }

            clusters[cluster_idx].push(vector);
        }

        // Update centroids
        let mut new_centroids = Vec::with_capacity(k as usize);
        for cluster in clusters {
            if cluster.is_empty() {
                // If a cluster is empty, keep the old centroid
                new_centroids.push(centroids[new_centroids.len()].clone());
                continue;
            }

            let dim = cluster[0].len();
            let mut new_centroid = vec![0.0; dim];
            let cluster_size = cluster.len() as f32;

            for point in cluster {
                for (i, &value) in point.iter().enumerate() {
                    new_centroid[i] += value / cluster_size;
                }
            }

            new_centroids.push(new_centroid);
        }

        // Check convergence
        let mut converged = true;
        for (old, new) in centroids.iter().zip(new_centroids.iter()) {
            if l2_distance(old, new) > 1e-6 {
                converged = false;
                break;
            }
        }

        centroids = new_centroids;

        if converged {
            break;
        }
    }

    Ok(centroids)
}

/// HNSW (Hierarchical Navigable Small World) index configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Maximum number of connections per node (default: 16)
    pub m: usize,
    /// Size of the dynamic candidate list during construction (default: 200)
    pub ef_construction: usize,
    /// Size of the dynamic candidate list during search (default: 50)
    pub ef_search: usize,
    /// Maximum number of layers
    pub max_layers: usize,
}

impl Default for HnswConfig {
    fn default() -> Self {
        Self {
            m: 16,
            ef_construction: 200,
            ef_search: 50,
            max_layers: 6,
        }
    }
}

/// HNSW Index for approximate nearest neighbor search
pub struct HnswIndex {
    config: HnswConfig,
    dimension: usize,
    metric_type: MetricType,
    // Layers of the graph: layer -> node_id -> neighbor_ids
    layers: Vec<std::collections::HashMap<usize, Vec<usize>>>,
    // Vector data storage
    vectors: Vec<Vector>,
    // Entry point for search
    entry_point: Option<usize>,
    // Maximum layer for each node
    node_layers: Vec<usize>,
}

impl HnswIndex {
    /// Create a new HNSW index
    pub fn new(dimension: usize, metric_type: MetricType, config: HnswConfig) -> Self {
        Self {
            config,
            dimension,
            metric_type,
            layers: vec![std::collections::HashMap::new(); 1],
            vectors: Vec::new(),
            entry_point: None,
            node_layers: Vec::new(),
        }
    }

    /// Calculate random level for new node using exponential distribution
    fn random_level(&self) -> usize {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let ml = 1.0 / (self.config.m as f64).ln();
        let random: f64 = rng.gen();
        (-random.ln() * ml)
            .floor()
            .min(self.config.max_layers as f64 - 1.0) as usize
    }

    /// Calculate distance between two vectors based on metric type
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.metric_type {
            MetricType::L2 => l2_distance(a, b),
            MetricType::IP => -dot_product(a, b),
            MetricType::Cosine => {
                let dot = dot_product(a, b);
                let norm_a = dot_product(a, a).sqrt();
                let norm_b = dot_product(b, b).sqrt();
                1.0 - (dot / (norm_a * norm_b))
            }
            _ => l2_distance(a, b),
        }
    }

    /// Search for nearest neighbors in a specific layer
    fn search_layer(
        &self,
        query: &[f32],
        entry_point: usize,
        ef: usize,
        layer: usize,
    ) -> Vec<(usize, f32)> {
        use std::cmp::Ordering;
        use std::collections::{BinaryHeap, HashSet};

        #[derive(Clone, PartialEq)]
        struct DistNode(f32, usize);

        impl Eq for DistNode {}

        impl PartialOrd for DistNode {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                // Min-heap for candidates (closest first)
                other.0.partial_cmp(&self.0)
            }
        }

        impl Ord for DistNode {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let entry_dist = self.distance(query, &self.vectors[entry_point].vector);
        candidates.push(DistNode(entry_dist, entry_point));
        results.push(DistNode(-entry_dist, entry_point)); // Max-heap for results
        visited.insert(entry_point);

        while let Some(DistNode(c_dist, c_node)) = candidates.pop() {
            let worst_dist = if let Some(DistNode(d, _)) = results.peek() {
                -d
            } else {
                f32::INFINITY
            };

            if c_dist > worst_dist {
                break;
            }

            if let Some(neighbors) = self.layers.get(layer).and_then(|l| l.get(&c_node)) {
                for &neighbor in neighbors {
                    if visited.insert(neighbor) {
                        let dist = self.distance(query, &self.vectors[neighbor].vector);

                        if results.len() < ef || dist < worst_dist {
                            candidates.push(DistNode(dist, neighbor));
                            results.push(DistNode(-dist, neighbor));

                            if results.len() > ef {
                                results.pop();
                            }
                        }
                    }
                }
            }
        }

        results
            .into_iter()
            .map(|DistNode(neg_dist, node)| (node, -neg_dist))
            .collect()
    }

    /// Select neighbors for a node (greedy heuristic)
    fn select_neighbors(&self, node: usize, candidates: Vec<(usize, f32)>, m: usize) -> Vec<usize> {
        let mut sorted = candidates;
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(m);
        sorted.into_iter().map(|(idx, _)| idx).collect()
    }

    /// Insert a vector into the index
    pub fn insert(&mut self, vector: Vector) -> Result<usize, VectorError> {
        if vector.vector.len() != self.dimension {
            return Err(VectorError::InvalidInput(format!(
                "Vector dimension mismatch. Expected {}, got {}",
                self.dimension,
                vector.vector.len()
            )));
        }

        let node_id = self.vectors.len();
        let level = self.random_level();

        // Extend layers if needed
        while self.layers.len() <= level {
            self.layers.push(std::collections::HashMap::new());
        }

        self.vectors.push(vector);
        self.node_layers.push(level);

        // Initialize empty neighbor lists for this node
        for l in 0..=level {
            self.layers[l].insert(node_id, Vec::new());
        }

        if let Some(entry) = self.entry_point {
            let mut current_entry = entry;
            let query = &self.vectors[node_id].vector;

            // Search from top layer to node's level + 1
            let entry_level = self.node_layers[entry];
            for l in (level + 1..=entry_level).rev() {
                let neighbors = self.search_layer(query, current_entry, 1, l);
                if let Some((nearest, _)) = neighbors.first() {
                    current_entry = *nearest;
                }
            }

            // Insert into each layer from level down to 0
            for l in (0..=level.min(entry_level)).rev() {
                let neighbors =
                    self.search_layer(query, current_entry, self.config.ef_construction, l);
                let selected = self.select_neighbors(node_id, neighbors.clone(), self.config.m);

                // Add bidirectional edges
                if let Some(node_neighbors) = self.layers[l].get_mut(&node_id) {
                    *node_neighbors = selected.clone();
                }

                for &neighbor in &selected {
                    if let Some(neighbor_list) = self.layers[l].get_mut(&neighbor) {
                        neighbor_list.push(node_id);
                        // Prune if too many neighbors
                        if neighbor_list.len() > self.config.m * 2 {
                            // Clone data needed for distance calculation to avoid borrow conflict
                            let neighbor_vec = self.vectors[neighbor].vector.clone();
                            let neighbor_ids: Vec<usize> = neighbor_list.iter().copied().collect();
                            let vectors_for_dist: Vec<(usize, Vec<f32>)> = neighbor_ids
                                .iter()
                                .map(|&n| (n, self.vectors[n].vector.clone()))
                                .collect();

                            let mut with_dist: Vec<(usize, f32)> = vectors_for_dist
                                .iter()
                                .map(|(n, vec)| (*n, self.distance(&neighbor_vec, vec)))
                                .collect();
                            with_dist.sort_by(|a, b| {
                                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                            });

                            if let Some(neighbor_list) = self.layers[l].get_mut(&neighbor) {
                                *neighbor_list = with_dist
                                    .into_iter()
                                    .take(self.config.m)
                                    .map(|(n, _)| n)
                                    .collect();
                            }
                        }
                    }
                }

                if !neighbors.is_empty() {
                    current_entry = neighbors[0].0;
                }
            }

            // Update entry point if new node has higher level
            if level > entry_level {
                self.entry_point = Some(node_id);
            }
        } else {
            self.entry_point = Some(node_id);
        }

        Ok(node_id)
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, VectorError> {
        if query.len() != self.dimension {
            return Err(VectorError::InvalidInput(format!(
                "Query dimension mismatch. Expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        if self.vectors.is_empty() {
            return Ok(Vec::new());
        }

        let entry = self.entry_point.unwrap();
        let entry_level = self.node_layers[entry];
        let mut current_entry = entry;

        // Search from top layer down to layer 1
        for l in (1..=entry_level).rev() {
            let neighbors = self.search_layer(query, current_entry, 1, l);
            if let Some((nearest, _)) = neighbors.first() {
                current_entry = *nearest;
            }
        }

        // Search layer 0 with full ef_search
        let results = self.search_layer(query, current_entry, self.config.ef_search.max(k), 0);

        Ok(results
            .into_iter()
            .take(k)
            .map(|(idx, dist)| {
                let vector = &self.vectors[idx];
                SearchResult {
                    id: vector.id.clone(),
                    score: 1.0 / (1.0 + dist), // Convert distance to similarity score
                    vector: vector.vector.clone(),
                    metadata: vector.metadata.clone(),
                }
            })
            .collect())
    }

    /// Get number of vectors in the index
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

/// Product Quantization for vector compression
pub struct ProductQuantizer {
    /// Number of subspaces
    num_subspaces: usize,
    /// Number of centroids per subspace
    num_centroids: usize,
    /// Dimension per subspace
    dim_per_subspace: usize,
    /// Centroids for each subspace
    codebooks: Vec<Vec<Vec<f32>>>,
}

impl ProductQuantizer {
    /// Create a new product quantizer
    pub fn new(dimension: usize, num_subspaces: usize, num_centroids: usize) -> Self {
        assert!(dimension % num_subspaces == 0);
        Self {
            num_subspaces,
            num_centroids,
            dim_per_subspace: dimension / num_subspaces,
            codebooks: Vec::new(),
        }
    }

    /// Train the quantizer on a set of vectors
    pub fn train(&mut self, vectors: &[Vec<f32>]) -> Result<(), VectorError> {
        self.codebooks.clear();

        for s in 0..self.num_subspaces {
            // Extract subvectors for this subspace
            let start = s * self.dim_per_subspace;
            let end = start + self.dim_per_subspace;

            let subvectors: Vec<Vec<f32>> =
                vectors.iter().map(|v| v[start..end].to_vec()).collect();

            // Run k-means on subvectors
            let centroids = kmeans_clustering(&subvectors, self.num_centroids as u32, 100)?;
            self.codebooks.push(centroids);
        }

        Ok(())
    }

    /// Encode a vector to codes
    pub fn encode(&self, vector: &[f32]) -> Vec<u8> {
        let mut codes = Vec::with_capacity(self.num_subspaces);

        for s in 0..self.num_subspaces {
            let start = s * self.dim_per_subspace;
            let end = start + self.dim_per_subspace;
            let subvector = &vector[start..end];

            // Find nearest centroid
            let mut min_dist = f32::INFINITY;
            let mut best_code = 0u8;

            for (i, centroid) in self.codebooks[s].iter().enumerate() {
                let dist = l2_distance(subvector, centroid);
                if dist < min_dist {
                    min_dist = dist;
                    best_code = i as u8;
                }
            }

            codes.push(best_code);
        }

        codes
    }

    /// Decode codes back to approximate vector
    pub fn decode(&self, codes: &[u8]) -> Vec<f32> {
        let mut vector = Vec::with_capacity(self.num_subspaces * self.dim_per_subspace);

        for (s, &code) in codes.iter().enumerate() {
            vector.extend_from_slice(&self.codebooks[s][code as usize]);
        }

        vector
    }

    /// Compute asymmetric distance between query and encoded vector
    pub fn asymmetric_distance(&self, query: &[f32], codes: &[u8]) -> f32 {
        let mut total_dist = 0.0;

        for (s, &code) in codes.iter().enumerate() {
            let start = s * self.dim_per_subspace;
            let end = start + self.dim_per_subspace;
            let subquery = &query[start..end];
            let centroid = &self.codebooks[s][code as usize];

            for (q, c) in subquery.iter().zip(centroid.iter()) {
                let diff = q - c;
                total_dist += diff * diff;
            }
        }

        total_dist.sqrt()
    }
}

/// Normalize a vector to unit length
pub fn normalize(vector: &[f32]) -> Vec<f32> {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 {
        return vector.to_vec();
    }
    vector.iter().map(|x| x / norm).collect()
}

/// Compute cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());

    let dot = dot_product(a, b);
    let norm_a = dot_product(a, a).sqrt();
    let norm_b = dot_product(b, b).sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Batch normalize a set of vectors
pub fn batch_normalize(vectors: &[Vec<f32>]) -> Vec<Vec<f32>> {
    vectors.iter().map(|v| normalize(v)).collect()
}

/// IVF (Inverted File) Index for efficient similarity search
pub struct IvfIndex {
    /// Number of clusters (inverted lists)
    num_clusters: usize,
    /// Dimension of vectors
    dimension: usize,
    /// Metric type for distance calculation
    metric_type: MetricType,
    /// Cluster centroids
    centroids: Vec<Vec<f32>>,
    /// Inverted lists: cluster_id -> list of (vector_id, vector)
    inverted_lists: Vec<Vec<(usize, Vec<f32>)>>,
    /// Vector ID to original Vector mapping
    vectors: Vec<Vector>,
    /// Number of clusters to probe during search
    nprobe: usize,
    /// Product quantizer for compression (optional)
    quantizer: Option<ProductQuantizer>,
}

impl IvfIndex {
    /// Create a new IVF index
    pub fn new(
        dimension: usize,
        metric_type: MetricType,
        num_clusters: usize,
        nprobe: usize,
    ) -> Self {
        Self {
            num_clusters,
            dimension,
            metric_type,
            centroids: Vec::new(),
            inverted_lists: vec![Vec::new(); num_clusters],
            vectors: Vec::new(),
            nprobe,
            quantizer: None,
        }
    }

    /// Create an IVF index with product quantization
    pub fn new_with_pq(
        dimension: usize,
        metric_type: MetricType,
        num_clusters: usize,
        nprobe: usize,
        num_subspaces: usize,
        num_centroids_pq: usize,
    ) -> Self {
        let pq = ProductQuantizer::new(dimension, num_subspaces, num_centroids_pq);
        Self {
            num_clusters,
            dimension,
            metric_type,
            centroids: Vec::new(),
            inverted_lists: vec![Vec::new(); num_clusters],
            vectors: Vec::new(),
            nprobe,
            quantizer: Some(pq),
        }
    }

    /// Train the index on a set of vectors (build centroids)
    pub fn train(&mut self, training_vectors: &[Vec<f32>]) -> Result<(), VectorError> {
        if training_vectors.is_empty() {
            return Err(VectorError::InvalidInput(
                "Training vectors cannot be empty".to_string(),
            ));
        }

        // Use k-means to find cluster centroids
        self.centroids = kmeans_clustering(training_vectors, self.num_clusters as u32, 100)?;

        // Train product quantizer if enabled
        if let Some(ref mut pq) = self.quantizer {
            pq.train(training_vectors)?;
        }

        Ok(())
    }

    /// Calculate distance between two vectors
    fn distance(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.metric_type {
            MetricType::L2 => l2_distance(a, b),
            MetricType::IP => -dot_product(a, b),
            MetricType::Cosine => {
                let dot = dot_product(a, b);
                let norm_a = dot_product(a, a).sqrt();
                let norm_b = dot_product(b, b).sqrt();
                1.0 - (dot / (norm_a * norm_b))
            }
            _ => l2_distance(a, b),
        }
    }

    /// Find the nearest centroid for a vector
    fn find_nearest_centroid(&self, vector: &[f32]) -> usize {
        let mut min_dist = f32::INFINITY;
        let mut nearest = 0;

        for (i, centroid) in self.centroids.iter().enumerate() {
            let dist = self.distance(vector, centroid);
            if dist < min_dist {
                min_dist = dist;
                nearest = i;
            }
        }

        nearest
    }

    /// Find the k nearest centroids for a query vector
    fn find_nearest_centroids(&self, vector: &[f32], k: usize) -> Vec<usize> {
        let mut distances: Vec<(usize, f32)> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(i, centroid)| (i, self.distance(vector, centroid)))
            .collect();

        distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        distances.truncate(k);
        distances.into_iter().map(|(i, _)| i).collect()
    }

    /// Add a vector to the index
    pub fn insert(&mut self, vector: Vector) -> Result<(), VectorError> {
        if vector.vector.len() != self.dimension {
            return Err(VectorError::InvalidInput(format!(
                "Vector dimension mismatch. Expected {}, got {}",
                self.dimension,
                vector.vector.len()
            )));
        }

        if self.centroids.is_empty() {
            return Err(VectorError::InvalidInput(
                "Index not trained. Call train() before inserting vectors".to_string(),
            ));
        }

        // Find nearest centroid
        let cluster_id = self.find_nearest_centroid(&vector.vector);

        // Add to inverted list
        let vector_id = self.vectors.len();
        self.inverted_lists[cluster_id].push((vector_id, vector.vector.clone()));
        self.vectors.push(vector);

        Ok(())
    }

    /// Add multiple vectors to the index
    pub fn insert_batch(&mut self, vectors: Vec<Vector>) -> Result<(), VectorError> {
        for vector in vectors {
            self.insert(vector)?;
        }
        Ok(())
    }

    /// Search for nearest neighbors
    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<SearchResult>, VectorError> {
        if query.len() != self.dimension {
            return Err(VectorError::InvalidInput(format!(
                "Query dimension mismatch. Expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        if self.centroids.is_empty() {
            return Err(VectorError::InvalidInput(
                "Index not trained. Call train() before searching".to_string(),
            ));
        }

        // Find nprobe nearest centroids
        let probe_clusters = self.find_nearest_centroids(query, self.nprobe.min(self.num_clusters));

        // Search in each cluster
        let mut candidates = Vec::new();

        for cluster_id in probe_clusters {
            for &(vector_id, ref vec) in &self.inverted_lists[cluster_id] {
                let dist = self.distance(query, vec);
                candidates.push((vector_id, dist));
            }
        }

        // Sort by distance
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(top_k);

        // Convert to SearchResult
        Ok(candidates
            .into_iter()
            .map(|(vector_id, score)| {
                let vector = &self.vectors[vector_id];
                SearchResult {
                    id: vector.id.clone(),
                    score: if matches!(self.metric_type, MetricType::L2) {
                        -score
                    } else {
                        score
                    },
                    vector: vector.vector.clone(),
                    metadata: vector.metadata.clone(),
                }
            })
            .collect())
    }

    /// Get index statistics
    pub fn stats(&self) -> IvfStats {
        let mut cluster_sizes: Vec<usize> =
            self.inverted_lists.iter().map(|list| list.len()).collect();

        cluster_sizes.sort_unstable();

        let total_vectors = self.vectors.len();
        let avg_cluster_size = if self.num_clusters > 0 {
            total_vectors as f64 / self.num_clusters as f64
        } else {
            0.0
        };

        let median_cluster_size = if !cluster_sizes.is_empty() {
            cluster_sizes[cluster_sizes.len() / 2]
        } else {
            0
        };

        IvfStats {
            num_clusters: self.num_clusters,
            total_vectors,
            avg_cluster_size,
            median_cluster_size,
            min_cluster_size: cluster_sizes.first().copied().unwrap_or(0),
            max_cluster_size: cluster_sizes.last().copied().unwrap_or(0),
            nprobe: self.nprobe,
            has_quantizer: self.quantizer.is_some(),
        }
    }

    /// Remove a vector from the index
    pub fn delete(&mut self, id: &str) -> Result<bool, VectorError> {
        // Find vector
        if let Some(pos) = self.vectors.iter().position(|v| v.id == id) {
            // Remove from vectors
            let vector = self.vectors.remove(pos);

            // Find and remove from inverted list
            let cluster_id = self.find_nearest_centroid(&vector.vector);
            self.inverted_lists[cluster_id].retain(|(vid, _)| *vid != pos);

            // Update vector IDs in inverted lists
            for list in &mut self.inverted_lists {
                for (vid, _) in list.iter_mut() {
                    if *vid > pos {
                        *vid -= 1;
                    }
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Statistics for IVF index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IvfStats {
    pub num_clusters: usize,
    pub total_vectors: usize,
    pub avg_cluster_size: f64,
    pub median_cluster_size: usize,
    pub min_cluster_size: usize,
    pub max_cluster_size: usize,
    pub nprobe: usize,
    pub has_quantizer: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_index() {
        let config = HnswConfig::default();
        let mut index = HnswIndex::new(4, MetricType::L2, config);

        // Insert vectors
        for i in 0..100 {
            let vector = Vector {
                id: format!("vec_{}", i),
                vector: vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32],
                metadata: None,
            };
            index.insert(vector).unwrap();
        }

        // Search
        let query = vec![50.0, 100.0, 150.0, 200.0];
        let results = index.search(&query, 5).unwrap();

        assert_eq!(results.len(), 5);
        // First result should be vec_50 (exact match)
        assert_eq!(results[0].id, "vec_50");
    }

    #[test]
    fn test_product_quantizer() {
        let mut pq = ProductQuantizer::new(8, 2, 16);

        // Generate training data
        let vectors: Vec<Vec<f32>> = (0..100)
            .map(|i| (0..8).map(|j| (i * j) as f32).collect())
            .collect();

        pq.train(&vectors).unwrap();

        // Test encode/decode
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let codes = pq.encode(&original);
        let decoded = pq.decode(&codes);

        assert_eq!(codes.len(), 2);
        assert_eq!(decoded.len(), 8);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
        assert!(cosine_similarity(&a, &c).abs() < 1e-6);
    }

    #[test]
    fn test_ivf_index() {
        // Create IVF index
        let mut index = IvfIndex::new(4, MetricType::L2, 4, 2);

        // Generate training data
        let training_data: Vec<Vec<f32>> = (0..100)
            .map(|i| vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32])
            .collect();

        // Train index
        index.train(&training_data).unwrap();

        // Insert vectors
        for i in 0..100 {
            let vector = Vector {
                id: format!("vec_{}", i),
                vector: vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32],
                metadata: None,
            };
            index.insert(vector).unwrap();
        }

        // Search
        let query = vec![50.0, 100.0, 150.0, 200.0];
        let results = index.search(&query, 5).unwrap();

        assert_eq!(results.len(), 5);
        // First result should be vec_50 (exact match)
        assert_eq!(results[0].id, "vec_50");

        // Check stats
        let stats = index.stats();
        assert_eq!(stats.num_clusters, 4);
        assert_eq!(stats.total_vectors, 100);
        assert_eq!(stats.nprobe, 2);
    }

    #[test]
    fn test_ivf_with_pq() {
        // Create IVF index with product quantization
        let mut index = IvfIndex::new_with_pq(8, MetricType::L2, 4, 2, 2, 16);

        // Generate training data
        let training_data: Vec<Vec<f32>> = (0..100)
            .map(|i| (0..8).map(|j| (i * j) as f32).collect())
            .collect();

        // Train index
        index.train(&training_data).unwrap();

        // Insert vectors
        for i in 0..50 {
            let vector = Vector {
                id: format!("vec_{}", i),
                vector: (0..8).map(|j| (i * j) as f32).collect(),
                metadata: None,
            };
            index.insert(vector).unwrap();
        }

        // Search
        let query: Vec<f32> = (0..8).map(|j| (25 * j) as f32).collect();
        let results = index.search(&query, 5).unwrap();

        assert_eq!(results.len(), 5);

        // Check that quantizer is present
        let stats = index.stats();
        assert!(stats.has_quantizer);
    }

    #[test]
    fn test_ivf_delete() {
        let mut index = IvfIndex::new(4, MetricType::L2, 2, 1);

        // Generate training data
        let training_data: Vec<Vec<f32>> = (0..20)
            .map(|i| vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32])
            .collect();

        // Train and insert
        index.train(&training_data).unwrap();

        for i in 0..20 {
            let vector = Vector {
                id: format!("vec_{}", i),
                vector: vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32],
                metadata: None,
            };
            index.insert(vector).unwrap();
        }

        // Delete a vector
        let deleted = index.delete("vec_10").unwrap();
        assert!(deleted);

        // Try to delete again
        let deleted_again = index.delete("vec_10").unwrap();
        assert!(!deleted_again);

        // Check stats
        let stats = index.stats();
        assert_eq!(stats.total_vectors, 19);
    }

    #[test]
    fn test_ivf_batch_insert() {
        let mut index = IvfIndex::new(4, MetricType::Cosine, 2, 1);

        // Training data
        let training_data: Vec<Vec<f32>> = (0..20)
            .map(|i| vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32])
            .collect();

        index.train(&training_data).unwrap();

        // Batch insert
        let vectors: Vec<Vector> = (0..10)
            .map(|i| Vector {
                id: format!("vec_{}", i),
                vector: vec![i as f32, (i * 2) as f32, (i * 3) as f32, (i * 4) as f32],
                metadata: Some(serde_json::json!({"index": i})),
            })
            .collect();

        index.insert_batch(vectors).unwrap();

        let stats = index.stats();
        assert_eq!(stats.total_vectors, 10);
    }
}
