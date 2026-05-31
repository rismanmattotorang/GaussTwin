//! REST API implementation
//!
//! RESTful endpoints for the GaussTwin API server.

use crate::{AppState, Error, Result};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

// ============================================================================
// Types
// ============================================================================

/// Pagination parameters
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl PaginationParams {
    pub fn offset(&self) -> usize {
        let page = self.page.unwrap_or(1).max(1) as usize;
        let per_page = self.per_page() as usize;
        (page - 1) * per_page
    }

    pub fn per_page(&self) -> u32 {
        self.per_page.unwrap_or(10).min(100).max(1)
    }
}

/// Paginated response wrapper
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
}

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

// ============================================================================
// Simulation Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simulation {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: SimulationStatus,
    pub config: SimulationConfig,
    pub metrics: SimulationMetrics,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SimulationStatus {
    Idle,
    Running,
    Paused,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub max_steps: Option<u64>,
    pub time_step: f64,
    pub scheduler: String,
    pub seed: Option<u64>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            max_steps: None,
            time_step: 1.0,
            scheduler: "sequential".to_string(),
            seed: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimulationMetrics {
    pub current_step: u64,
    pub elapsed_time: f64,
    pub agent_count: u64,
    pub events_processed: u64,
    pub steps_per_second: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateSimulationRequest {
    pub name: String,
    pub description: Option<String>,
    pub config: Option<SimulationConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSimulationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<SimulationConfig>,
}

// ============================================================================
// Agent Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub simulation_id: String,
    pub agent_type: String,
    pub state: serde_json::Value,
    pub position: Option<Position>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub agent_type: String,
    pub state: Option<serde_json::Value>,
    pub position: Option<Position>,
}

#[derive(Debug, Deserialize)]
pub struct AgentQueryParams {
    pub agent_type: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

// ============================================================================
// Auth Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub created_at: String,
}

// ============================================================================
// Space Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub simulation_id: String,
    pub space_type: SpaceType,
    pub bounds: Bounds,
    pub agent_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceType {
    Grid,
    Continuous,
    Graph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Position,
    pub max: Position,
}

// ============================================================================
// Router
// ============================================================================

/// Create the REST API router
pub fn create_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Health and info
        .route("/health", get(health_check))
        .route("/info", get(server_info))
        // Auth
        .route("/auth/login", post(login_handler))
        .route("/auth/register", post(register_handler))
        .route("/auth/refresh", post(refresh_handler))
        // Simulations
        .route(
            "/simulations",
            get(list_simulations).post(create_simulation),
        )
        .route(
            "/simulations/:id",
            get(get_simulation)
                .put(update_simulation)
                .delete(delete_simulation),
        )
        .route("/simulations/:id/start", post(start_simulation))
        .route("/simulations/:id/pause", post(pause_simulation))
        .route("/simulations/:id/stop", post(stop_simulation))
        .route("/simulations/:id/step", post(step_simulation))
        .route("/simulations/:id/metrics", get(get_simulation_metrics))
        // Agents
        .route(
            "/simulations/:id/agents",
            get(list_agents).post(create_agent),
        )
        .route(
            "/simulations/:id/agents/:agent_id",
            get(get_agent).delete(delete_agent),
        )
        // Spaces
        .route("/simulations/:id/space", get(get_space))
        .route("/simulations/:id/space/query", post(query_space))
        // System
        .route("/metrics", get(get_metrics))
        .with_state(state)
}

// ============================================================================
// Auth Handlers
// ============================================================================

/// Mock login handler
async fn login_handler(
    State(_state): State<Arc<AppState>>,
    Json(request): Json<LoginRequest>,
) -> impl IntoResponse {
    info!("Login attempt for: {}", request.email);

    let user = UserResponse {
        id: "user-001".to_string(),
        email: request.email,
        name: "Test User".to_string(),
        role: "admin".to_string(),
        permissions: vec!["all".to_string()],
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    Json(AuthResponse {
        user,
        access_token: "mock-access-token".to_string(),
        refresh_token: Some("mock-refresh-token".to_string()),
    })
}

/// Mock register handler
async fn register_handler(
    State(_state): State<Arc<AppState>>,
    Json(request): Json<RegisterRequest>,
) -> impl IntoResponse {
    info!("Registration attempt for: {}", request.email);

    let user = UserResponse {
        id: Uuid::new_v4().to_string(),
        email: request.email,
        name: request.name,
        role: "user".to_string(),
        permissions: vec!["read".to_string(), "write".to_string()],
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    Json(AuthResponse {
        user,
        access_token: "mock-access-token".to_string(),
        refresh_token: Some("mock-refresh-token".to_string()),
    })
}

/// Mock refresh handler
async fn refresh_handler(
    State(_state): State<Arc<AppState>>,
    Json(_request): Json<RefreshRequest>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "accessToken": "new-mock-access-token",
        "refreshToken": "new-mock-refresh-token",
    }))
}

// ============================================================================
// Health & Info Handlers
// ============================================================================

/// Health check endpoint
async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.metrics.increment_counter("api.health.calls", 1, None);

    Json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Server info endpoint
async fn server_info(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "GaussTwin API Server",
        "version": env!("CARGO_PKG_VERSION"),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "endpoints": {
            "rest": true,
            "graphql": true,
            "websocket": true,
            "grpc": true,
        },
        "features": [
            "simulation",
            "agents",
            "spaces",
            "metrics",
            "real-time",
        ],
    }))
}

// ============================================================================
// Simulation Handlers
// ============================================================================

/// List all simulations
async fn list_simulations(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> impl IntoResponse {
    info!(
        "Listing simulations, page: {:?}, per_page: {:?}",
        params.page, params.per_page
    );
    state
        .metrics
        .increment_counter("api.simulations.list", 1, None);

    // Get simulations from database
    let result = state
        .db
        .list_simulations(params.per_page() as usize, params.offset())
        .await;

    match result {
        Ok(simulations) => {
            let total = state.db.count_simulations().await.unwrap_or(0);
            let total_pages = (total as f64 / params.per_page() as f64).ceil() as u32;

            Json(ApiResponse::success(PaginatedResponse {
                data: simulations,
                pagination: PaginationInfo {
                    page: params.page.unwrap_or(1),
                    per_page: params.per_page(),
                    total,
                    total_pages,
                },
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to list simulations: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to list simulations: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Get a simulation by ID
async fn get_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Getting simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.get", 1, None);

    match state.db.get_simulation(&id).await {
        Ok(Some(simulation)) => Json(ApiResponse::success(simulation)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Simulation not found: {}",
                id
            ))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to get simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Create a new simulation
async fn create_simulation(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateSimulationRequest>,
) -> impl IntoResponse {
    info!("Creating simulation: {}", request.name);
    state
        .metrics
        .increment_counter("api.simulations.create", 1, None);

    let simulation = Simulation {
        id: Uuid::new_v4().to_string(),
        name: request.name,
        description: request.description,
        status: SimulationStatus::Idle,
        config: request.config.unwrap_or_default(),
        metrics: SimulationMetrics::default(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    match state.db.create_simulation(&simulation).await {
        Ok(_) => (StatusCode::CREATED, Json(ApiResponse::success(simulation))).into_response(),
        Err(e) => {
            error!("Failed to create simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to create simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Update a simulation
async fn update_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSimulationRequest>,
) -> impl IntoResponse {
    info!("Updating simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.update", 1, None);

    // Get existing simulation
    let existing = match state.db.get_simulation(&id).await {
        Ok(Some(sim)) => sim,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiResponse::<()>::error(format!(
                    "Simulation not found: {}",
                    id
                ))),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to get simulation: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let updated = Simulation {
        name: request.name.unwrap_or(existing.name),
        description: request.description.or(existing.description),
        config: request.config.unwrap_or(existing.config),
        updated_at: chrono::Utc::now().to_rfc3339(),
        ..existing
    };

    match state.db.update_simulation(&updated).await {
        Ok(_) => Json(ApiResponse::success(updated)).into_response(),
        Err(e) => {
            error!("Failed to update simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to update simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Delete a simulation
async fn delete_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Deleting simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.delete", 1, None);

    match state.db.delete_simulation(&id).await {
        Ok(true) => (StatusCode::NO_CONTENT, Json(ApiResponse::success(()))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Simulation not found: {}",
                id
            ))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to delete simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to delete simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Start a simulation
async fn start_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Starting simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.start", 1, None);

    match state
        .db
        .update_simulation_status(&id, SimulationStatus::Running)
        .await
    {
        Ok(_) => Json(ApiResponse::success(serde_json::json!({
            "id": id,
            "status": "running",
            "message": "Simulation started successfully"
        })))
        .into_response(),
        Err(e) => {
            error!("Failed to start simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to start simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Pause a simulation
async fn pause_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Pausing simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.pause", 1, None);

    match state
        .db
        .update_simulation_status(&id, SimulationStatus::Paused)
        .await
    {
        Ok(_) => Json(ApiResponse::success(serde_json::json!({
            "id": id,
            "status": "paused",
            "message": "Simulation paused successfully"
        })))
        .into_response(),
        Err(e) => {
            error!("Failed to pause simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to pause simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Stop a simulation
async fn stop_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Stopping simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.stop", 1, None);

    match state
        .db
        .update_simulation_status(&id, SimulationStatus::Stopped)
        .await
    {
        Ok(_) => Json(ApiResponse::success(serde_json::json!({
            "id": id,
            "status": "stopped",
            "message": "Simulation stopped successfully"
        })))
        .into_response(),
        Err(e) => {
            error!("Failed to stop simulation: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to stop simulation: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Execute a single simulation step
async fn step_simulation(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Stepping simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.step", 1, None);

    // Get current metrics and increment step
    match state.db.get_simulation(&id).await {
        Ok(Some(mut simulation)) => {
            simulation.metrics.current_step += 1;
            simulation.updated_at = chrono::Utc::now().to_rfc3339();

            if let Err(e) = state.db.update_simulation(&simulation).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::error(format!(
                        "Failed to update simulation: {}",
                        e
                    ))),
                )
                    .into_response();
            }

            Json(ApiResponse::success(serde_json::json!({
                "id": id,
                "step": simulation.metrics.current_step,
                "message": "Step executed successfully"
            })))
            .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Simulation not found: {}",
                id
            ))),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!(
                "Failed to get simulation: {}",
                e
            ))),
        )
            .into_response(),
    }
}

/// Get simulation metrics
async fn get_simulation_metrics(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    info!("Getting metrics for simulation: {}", id);
    state
        .metrics
        .increment_counter("api.simulations.metrics", 1, None);

    match state.db.get_simulation(&id).await {
        Ok(Some(simulation)) => Json(ApiResponse::success(simulation.metrics)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Simulation not found: {}",
                id
            ))),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::error(format!(
                "Failed to get simulation: {}",
                e
            ))),
        )
            .into_response(),
    }
}

// ============================================================================
// Agent Handlers
// ============================================================================

/// List agents in a simulation
async fn list_agents(
    State(state): State<Arc<AppState>>,
    Path(simulation_id): Path<String>,
    Query(params): Query<AgentQueryParams>,
) -> impl IntoResponse {
    info!("Listing agents for simulation: {}", simulation_id);
    state.metrics.increment_counter("api.agents.list", 1, None);

    match state
        .db
        .list_agents(
            &simulation_id,
            params.pagination.per_page() as usize,
            params.pagination.offset(),
        )
        .await
    {
        Ok(agents) => {
            let total = state.db.count_agents(&simulation_id).await.unwrap_or(0);
            let total_pages = (total as f64 / params.pagination.per_page() as f64).ceil() as u32;

            Json(ApiResponse::success(PaginatedResponse {
                data: agents,
                pagination: PaginationInfo {
                    page: params.pagination.page.unwrap_or(1),
                    per_page: params.pagination.per_page(),
                    total,
                    total_pages,
                },
            }))
            .into_response()
        }
        Err(e) => {
            error!("Failed to list agents: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to list agents: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Get an agent by ID
async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path((simulation_id, agent_id)): Path<(String, String)>,
) -> impl IntoResponse {
    info!(
        "Getting agent {} from simulation {}",
        agent_id, simulation_id
    );
    state.metrics.increment_counter("api.agents.get", 1, None);

    match state.db.get_agent(&simulation_id, &agent_id).await {
        Ok(Some(agent)) => Json(ApiResponse::success(agent)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Agent not found: {}",
                agent_id
            ))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get agent: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to get agent: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Create an agent in a simulation
async fn create_agent(
    State(state): State<Arc<AppState>>,
    Path(simulation_id): Path<String>,
    Json(request): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    info!("Creating agent in simulation: {}", simulation_id);
    state
        .metrics
        .increment_counter("api.agents.create", 1, None);

    let agent = Agent {
        id: Uuid::new_v4().to_string(),
        simulation_id,
        agent_type: request.agent_type,
        state: request.state.unwrap_or(serde_json::json!({})),
        position: request.position,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    match state.db.create_agent(&agent).await {
        Ok(_) => (StatusCode::CREATED, Json(ApiResponse::success(agent))).into_response(),
        Err(e) => {
            error!("Failed to create agent: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to create agent: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Delete an agent
async fn delete_agent(
    State(state): State<Arc<AppState>>,
    Path((simulation_id, agent_id)): Path<(String, String)>,
) -> impl IntoResponse {
    info!(
        "Deleting agent {} from simulation {}",
        agent_id, simulation_id
    );
    state
        .metrics
        .increment_counter("api.agents.delete", 1, None);

    match state.db.delete_agent(&simulation_id, &agent_id).await {
        Ok(true) => (StatusCode::NO_CONTENT, Json(ApiResponse::success(()))).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Agent not found: {}",
                agent_id
            ))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to delete agent: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to delete agent: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Space Handlers
// ============================================================================

/// Get space information for a simulation
async fn get_space(
    State(state): State<Arc<AppState>>,
    Path(simulation_id): Path<String>,
) -> impl IntoResponse {
    info!("Getting space for simulation: {}", simulation_id);
    state.metrics.increment_counter("api.spaces.get", 1, None);

    match state.db.get_space(&simulation_id).await {
        Ok(Some(space)) => Json(ApiResponse::success(space)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<()>::error(format!(
                "Space not found for simulation: {}",
                simulation_id
            ))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to get space: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to get space: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

/// Query for spatial information
#[derive(Debug, Deserialize)]
pub struct SpatialQuery {
    pub query_type: SpatialQueryType,
    pub position: Option<Position>,
    pub radius: Option<f64>,
    pub k: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialQueryType {
    RadiusSearch,
    NearestNeighbors,
    AgentsAt,
}

async fn query_space(
    State(state): State<Arc<AppState>>,
    Path(simulation_id): Path<String>,
    Json(query): Json<SpatialQuery>,
) -> impl IntoResponse {
    info!("Querying space for simulation: {}", simulation_id);
    state.metrics.increment_counter("api.spaces.query", 1, None);

    match state.db.query_space(&simulation_id, &query).await {
        Ok(results) => Json(ApiResponse::success(results)).into_response(),
        Err(e) => {
            error!("Failed to query space: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<()>::error(format!(
                    "Failed to query space: {}",
                    e
                ))),
            )
                .into_response()
        }
    }
}

// ============================================================================
// System Handlers
// ============================================================================

/// Get system metrics
async fn get_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Getting system metrics");
    state.metrics.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params() {
        let params = PaginationParams {
            page: Some(2),
            per_page: Some(20),
        };
        assert_eq!(params.offset(), 20);
        assert_eq!(params.per_page(), 20);
    }

    #[test]
    fn test_pagination_defaults() {
        let params = PaginationParams {
            page: None,
            per_page: None,
        };
        assert_eq!(params.offset(), 0);
        assert_eq!(params.per_page(), 10);
    }

    #[test]
    fn test_api_response() {
        let response: ApiResponse<String> = ApiResponse::success("test".to_string());
        assert!(response.success);
        assert_eq!(response.data.unwrap(), "test");

        let error = ApiResponse::<()>::error("error message");
        assert!(!error.success);
        assert_eq!(error.error.unwrap(), "error message");
    }
}
