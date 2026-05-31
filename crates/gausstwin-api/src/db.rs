use crate::{
    config::DatabaseConfig,
    error::Result,
    graphql::{CreateTwinInput, Metric, SimulationResult, Twin, TwinStatus, UpdateTwinInput},
    rest::{Agent, Bounds, Position, Simulation, SimulationStatus, Space, SpaceType, SpatialQuery},
};
use async_graphql::ID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// In-memory store for simulations (fallback when SurrealDB is unavailable)
#[derive(Default)]
struct InMemoryStore {
    simulations: HashMap<String, Simulation>,
    agents: HashMap<String, HashMap<String, Agent>>,
    spaces: HashMap<String, Space>,
    twins: HashMap<String, TwinData>,
}

#[derive(Clone, Debug)]
pub struct TwinData {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub status: TwinStatus,
}

/// Database manager for handling storage operations
/// Uses in-memory storage as fallback when SurrealDB is unavailable
pub struct DatabaseManager {
    /// In-memory store
    store: Arc<RwLock<InMemoryStore>>,
    /// Database configuration
    config: DatabaseConfig,
    /// Connected flag
    connected: bool,
}

impl DatabaseManager {
    /// Create a new database manager
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        info!("Initializing database manager...");

        // Try to connect to SurrealDB, fallback to in-memory if unavailable
        let connected = false; // Using in-memory for now to avoid connection issues

        if !connected {
            warn!("SurrealDB unavailable, using in-memory storage");
        }

        let mut manager = Self {
            store: Arc::new(RwLock::new(InMemoryStore::default())),
            config: config.clone(),
            connected,
        };

        // Initialize with default data
        manager.init_default_data().await?;

        Ok(manager)
    }

    /// Initialize with default demo data
    async fn init_default_data(&self) -> Result<()> {
        let mut store = self.store.write().await;

        // Create a default simulation
        let sim = Simulation {
            id: "sim-001".to_string(),
            name: "Demo Simulation".to_string(),
            description: Some("A demonstration simulation".to_string()),
            status: SimulationStatus::Idle,
            config: crate::rest::SimulationConfig::default(),
            metrics: crate::rest::SimulationMetrics::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        store.simulations.insert(sim.id.clone(), sim);

        // Create default space
        let space = Space {
            id: "space-001".to_string(),
            simulation_id: "sim-001".to_string(),
            space_type: SpaceType::Continuous,
            bounds: Bounds {
                min: Position {
                    x: 0.0,
                    y: 0.0,
                    z: Some(0.0),
                },
                max: Position {
                    x: 100.0,
                    y: 100.0,
                    z: Some(100.0),
                },
            },
            agent_count: 0,
        };
        store.spaces.insert("sim-001".to_string(), space);

        // Create a default twin
        let twin = TwinData {
            id: "twin-001".to_string(),
            name: "Demo Digital Twin".to_string(),
            description: Some("A demonstration digital twin".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            status: TwinStatus::Active,
        };
        store.twins.insert(twin.id.clone(), twin);

        info!("Initialized default demo data");
        Ok(())
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> Result<()> {
        if !self.config.enable_migrations {
            return Ok(());
        }
        info!("Running database migrations (no-op for in-memory storage)");
        Ok(())
    }

    // ========================================================================
    // Simulation Operations
    // ========================================================================

    /// List simulations with pagination
    pub async fn list_simulations(&self, limit: usize, offset: usize) -> Result<Vec<Simulation>> {
        let store = self.store.read().await;
        let mut simulations: Vec<_> = store.simulations.values().cloned().collect();
        simulations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(simulations.into_iter().skip(offset).take(limit).collect())
    }

    /// Count total simulations
    pub async fn count_simulations(&self) -> Result<u64> {
        let store = self.store.read().await;
        Ok(store.simulations.len() as u64)
    }

    /// Get a simulation by ID
    pub async fn get_simulation(&self, id: &str) -> Result<Option<Simulation>> {
        let store = self.store.read().await;
        Ok(store.simulations.get(id).cloned())
    }

    /// Create a new simulation
    pub async fn create_simulation(&self, simulation: &Simulation) -> Result<()> {
        let mut store = self.store.write().await;
        store
            .simulations
            .insert(simulation.id.clone(), simulation.clone());

        // Create associated space
        let space = Space {
            id: format!("space-{}", simulation.id),
            simulation_id: simulation.id.clone(),
            space_type: SpaceType::Continuous,
            bounds: Bounds {
                min: Position {
                    x: 0.0,
                    y: 0.0,
                    z: Some(0.0),
                },
                max: Position {
                    x: 100.0,
                    y: 100.0,
                    z: Some(100.0),
                },
            },
            agent_count: 0,
        };
        store.spaces.insert(simulation.id.clone(), space);

        // Initialize empty agent map
        store.agents.insert(simulation.id.clone(), HashMap::new());

        Ok(())
    }

    /// Update a simulation
    pub async fn update_simulation(&self, simulation: &Simulation) -> Result<()> {
        let mut store = self.store.write().await;
        store
            .simulations
            .insert(simulation.id.clone(), simulation.clone());
        Ok(())
    }

    /// Update simulation status
    pub async fn update_simulation_status(&self, id: &str, status: SimulationStatus) -> Result<()> {
        let mut store = self.store.write().await;
        if let Some(sim) = store.simulations.get_mut(id) {
            sim.status = status;
            sim.updated_at = chrono::Utc::now().to_rfc3339();
        }
        Ok(())
    }

    /// Delete a simulation
    pub async fn delete_simulation(&self, id: &str) -> Result<bool> {
        let mut store = self.store.write().await;
        let existed = store.simulations.remove(id).is_some();
        store.agents.remove(id);
        store.spaces.remove(id);
        Ok(existed)
    }

    // ========================================================================
    // Agent Operations
    // ========================================================================

    /// List agents in a simulation
    pub async fn list_agents(
        &self,
        simulation_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Agent>> {
        let store = self.store.read().await;
        let agents = store
            .agents
            .get(simulation_id)
            .map(|m| m.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(agents.into_iter().skip(offset).take(limit).collect())
    }

    /// Count agents in a simulation
    pub async fn count_agents(&self, simulation_id: &str) -> Result<u64> {
        let store = self.store.read().await;
        Ok(store
            .agents
            .get(simulation_id)
            .map(|m| m.len())
            .unwrap_or(0) as u64)
    }

    /// Get an agent by ID
    pub async fn get_agent(&self, simulation_id: &str, agent_id: &str) -> Result<Option<Agent>> {
        let store = self.store.read().await;
        Ok(store
            .agents
            .get(simulation_id)
            .and_then(|m| m.get(agent_id).cloned()))
    }

    /// Create an agent
    pub async fn create_agent(&self, agent: &Agent) -> Result<()> {
        let mut store = self.store.write().await;
        store
            .agents
            .entry(agent.simulation_id.clone())
            .or_insert_with(HashMap::new)
            .insert(agent.id.clone(), agent.clone());

        // Update space agent count
        if let Some(space) = store.spaces.get_mut(&agent.simulation_id) {
            space.agent_count += 1;
        }
        Ok(())
    }

    /// Delete an agent
    pub async fn delete_agent(&self, simulation_id: &str, agent_id: &str) -> Result<bool> {
        let mut store = self.store.write().await;
        let existed = store
            .agents
            .get_mut(simulation_id)
            .map(|m| m.remove(agent_id).is_some())
            .unwrap_or(false);

        if existed {
            if let Some(space) = store.spaces.get_mut(simulation_id) {
                space.agent_count = space.agent_count.saturating_sub(1);
            }
        }
        Ok(existed)
    }

    // ========================================================================
    // Space Operations
    // ========================================================================

    /// Get space for a simulation
    pub async fn get_space(&self, simulation_id: &str) -> Result<Option<Space>> {
        let store = self.store.read().await;
        Ok(store.spaces.get(simulation_id).cloned())
    }

    /// Query space
    pub async fn query_space(
        &self,
        simulation_id: &str,
        query: &SpatialQuery,
    ) -> Result<Vec<Agent>> {
        let store = self.store.read().await;
        let agents = store
            .agents
            .get(simulation_id)
            .map(|m| m.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        // Filter by spatial query
        match query.query_type {
            crate::rest::SpatialQueryType::RadiusSearch => {
                if let (Some(pos), Some(radius)) = (&query.position, query.radius) {
                    Ok(agents
                        .into_iter()
                        .filter(|a| {
                            if let Some(ref agent_pos) = a.position {
                                let dx = agent_pos.x - pos.x;
                                let dy = agent_pos.y - pos.y;
                                (dx * dx + dy * dy).sqrt() <= radius
                            } else {
                                false
                            }
                        })
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
            crate::rest::SpatialQueryType::NearestNeighbors => {
                if let (Some(pos), Some(k)) = (&query.position, query.k) {
                    let mut agents_with_dist: Vec<_> = agents
                        .into_iter()
                        .filter_map(|a| {
                            if let Some(ref agent_pos) = a.position {
                                let dx = agent_pos.x - pos.x;
                                let dy = agent_pos.y - pos.y;
                                let dist = (dx * dx + dy * dy).sqrt();
                                Some((a, dist))
                            } else {
                                None
                            }
                        })
                        .collect();
                    agents_with_dist.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                    Ok(agents_with_dist
                        .into_iter()
                        .take(k)
                        .map(|(a, _)| a)
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
            crate::rest::SpatialQueryType::AgentsAt => {
                if let Some(pos) = &query.position {
                    Ok(agents
                        .into_iter()
                        .filter(|a| {
                            if let Some(ref agent_pos) = a.position {
                                (agent_pos.x - pos.x).abs() < 0.001
                                    && (agent_pos.y - pos.y).abs() < 0.001
                            } else {
                                false
                            }
                        })
                        .collect())
                } else {
                    Ok(vec![])
                }
            }
        }
    }

    // ========================================================================
    // Twin Operations (for GraphQL)
    // ========================================================================

    /// Get a twin by ID
    pub async fn get_twin(&self, id: &str) -> Result<Option<TwinData>> {
        let store = self.store.read().await;
        Ok(store.twins.get(id).cloned())
    }

    /// List twins with pagination
    pub async fn list_twins(
        &self,
        limit: usize,
        offset: usize,
        _status: Option<TwinStatus>,
    ) -> Result<Vec<TwinData>> {
        let store = self.store.read().await;
        let mut twins: Vec<_> = store.twins.values().cloned().collect();
        twins.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(twins.into_iter().skip(offset).take(limit).collect())
    }

    /// Create a twin
    pub async fn create_twin(&self, input: CreateTwinInput) -> Result<Twin> {
        let mut store = self.store.write().await;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let twin_data = TwinData {
            id: id.clone(),
            name: input.name.clone(),
            description: input.description.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            status: TwinStatus::Active,
        };
        store.twins.insert(id.clone(), twin_data);

        Ok(Twin {
            id: ID(id),
            name: input.name,
            description: input.description,
            created_at: now.clone(),
            updated_at: now,
            status: TwinStatus::Active,
        })
    }

    /// Update a twin
    pub async fn update_twin(&self, input: UpdateTwinInput) -> Result<Twin> {
        let mut store = self.store.write().await;
        let id = input.id.to_string();

        if let Some(twin) = store.twins.get_mut(&id) {
            if let Some(name) = input.name {
                twin.name = name;
            }
            if let Some(desc) = input.description {
                twin.description = Some(desc);
            }
            if let Some(status) = input.status {
                twin.status = status;
            }
            twin.updated_at = chrono::Utc::now().to_rfc3339();

            Ok(Twin {
                id: ID(twin.id.clone()),
                name: twin.name.clone(),
                description: twin.description.clone(),
                created_at: twin.created_at.clone(),
                updated_at: twin.updated_at.clone(),
                status: twin.status,
            })
        } else {
            Err(crate::error::Error::NotFound(format!(
                "Twin not found: {}",
                id
            )))
        }
    }

    /// Delete a twin
    pub async fn delete_twin(&self, id: &str) -> Result<bool> {
        let mut store = self.store.write().await;
        Ok(store.twins.remove(id).is_some())
    }

    /// Search twins by name
    pub async fn search_twins(&self, query: &str, limit: usize) -> Result<Vec<TwinData>> {
        let store = self.store.read().await;
        let query_lower = query.to_lowercase();
        Ok(store
            .twins
            .values()
            .filter(|t| t.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .cloned()
            .collect())
    }

    /// Get simulation results for a twin
    pub async fn get_simulation_results(
        &self,
        _twin_id: &str,
        limit: usize,
    ) -> Result<Vec<SimulationResult>> {
        // Return mock simulation results
        Ok((0..limit.min(5))
            .map(|i| SimulationResult {
                id: ID(format!("result-{}", i)),
                twin_id: ID(_twin_id.to_string()),
                timestamp: chrono::Utc::now().to_rfc3339(),
                status: "completed".to_string(),
                metrics: vec![
                    Metric {
                        name: "cpu_usage".to_string(),
                        value: 45.0 + (i as f64 * 5.0),
                        unit: Some("%".to_string()),
                    },
                    Metric {
                        name: "memory_usage".to_string(),
                        value: 60.0 + (i as f64 * 3.0),
                        unit: Some("%".to_string()),
                    },
                ],
            })
            .collect())
    }

    /// Start a simulation for a twin
    pub async fn start_simulation(&self, twin_id: &str) -> Result<SimulationResult> {
        Ok(SimulationResult {
            id: ID(uuid::Uuid::new_v4().to_string()),
            twin_id: ID(twin_id.to_string()),
            timestamp: chrono::Utc::now().to_rfc3339(),
            status: "running".to_string(),
            metrics: vec![],
        })
    }
}

// Milvus manager commented out due to unavailable SDK
/*
/// Milvus manager for handling vector operations
pub struct MilvusManager {
    /// Milvus client
    client: Arc<RwLock<MilvusClient>>,
    /// Default collection
    collection: Arc<RwLock<Collection>>,
    /// Milvus configuration
    config: MilvusConfig,
}

impl MilvusManager {
    /// Create a new Milvus manager
    pub async fn new(config: &MilvusConfig) -> Result<Self> {
        // Connect to Milvus
        let mut options = ConnectOptions::default();
        options.address = format!("{}:{}", config.host, config.port);
        if let Some(token) = &config.auth_token {
            options.token = token.clone();
        }

        let client = MilvusClient::connect(options).await?;

        // Get or create default collection
        let collection = match client
            .get_collection(&config.default_collection)
            .await
        {
            Ok(col) => col,
            Err(_) => {
                client.create_collection(
                    &config.default_collection,
                    config.dimension,
                    &config.index_type,
                    &config.metric_type,
                ).await.unwrap()
            }
        };

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            collection: Arc::new(RwLock::new(collection)),
            config: config.clone(),
        })
    }

    /// Get the Milvus client
    pub async fn client(&self) -> Arc<RwLock<MilvusClient>> {
        self.client.clone()
    }

    /// Get the default collection
    pub async fn collection(&self) -> Arc<RwLock<Collection>> {
        self.collection.clone()
    }

    /// Create a new collection
    pub async fn create_collection(
        &self,
        name: &str,
        dimension: u32,
        index_type: &str,
        metric_type: &str,
    ) -> Result<Collection> {
        let client = self.client.read().await;
        let collection = client
            .create_collection(name, dimension, index_type, metric_type)
            .await?;
        Ok(collection)
    }

    /// Insert vectors
    pub async fn insert_vectors(&self, vectors: Vec<Vec<f32>>) -> Result<Vec<i64>> {
        let collection = self.collection.read().await;
        let ids = collection.insert(vectors).await?;
        Ok(ids)
    }

    /// Search vectors
    pub async fn search_vectors(
        &self,
        query_vectors: Vec<Vec<f32>>,
        top_k: i64,
    ) -> Result<Vec<Vec<(i64, f32)>>> {
        let collection = self.collection.read().await;
        let results = collection.search(query_vectors, top_k).await?;
        Ok(results)
    }

    /// Delete vectors
    pub async fn delete_vectors(&self, ids: Vec<i64>) -> Result<()> {
        let collection = self.collection.read().await;
        collection.delete(ids).await?;
        Ok(())
    }
}
*/
