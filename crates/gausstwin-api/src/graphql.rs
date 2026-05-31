//! GraphQL server implementation

use crate::{AppState, Error};
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{
    Context, Enum, Error as GqlError, InputObject, Object, Result as GqlResult, Schema,
    SimpleObject, Subscription, ID,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

/// Twin entity for GraphQL
#[derive(Debug, Clone, SimpleObject, Serialize)]
pub struct Twin {
    pub id: ID,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub status: TwinStatus,
}

/// Twin status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum, Serialize, Deserialize)]
pub enum TwinStatus {
    Active,
    Inactive,
    Maintenance,
    Error,
}

/// Input for creating a twin
#[derive(Debug, InputObject)]
pub struct CreateTwinInput {
    pub name: String,
    pub description: Option<String>,
}

/// Input for updating a twin
#[derive(Debug, InputObject)]
pub struct UpdateTwinInput {
    pub id: ID,
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<TwinStatus>,
}

/// Simulation result
#[derive(Debug, Clone, SimpleObject)]
pub struct SimulationResult {
    pub id: ID,
    pub twin_id: ID,
    pub timestamp: String,
    pub status: String,
    pub metrics: Vec<Metric>,
}

/// Metric data point
#[derive(Debug, Clone, SimpleObject)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub unit: Option<String>,
}

/// Event notification
#[derive(Debug, Clone, SimpleObject)]
pub struct Event {
    pub id: ID,
    pub event_type: String,
    pub twin_id: Option<ID>,
    pub data: String,
    pub timestamp: String,
}

/// GraphQL query root
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Health check query
    async fn health(&self, ctx: &Context<'_>) -> GqlResult<String> {
        let state = ctx.data::<Arc<AppState>>()?;
        state
            .metrics
            .increment_counter("graphql.health.calls", 1, None);
        Ok("ok".to_string())
    }

    /// Get server version
    async fn version(&self, _ctx: &Context<'_>) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Get a twin by ID
    async fn twin(&self, ctx: &Context<'_>, id: ID) -> GqlResult<Option<Twin>> {
        let state = ctx.data::<Arc<AppState>>()?;

        // Query from database
        let twin_data = state
            .db
            .get_twin(&id.to_string())
            .await
            .map_err(|e| GqlError::new(format!("Database error: {}", e)))?;

        Ok(twin_data.map(|data| Twin {
            id: ID(data.id),
            name: data.name,
            description: data.description,
            created_at: data.created_at,
            updated_at: data.updated_at,
            status: TwinStatus::Active,
        }))
    }

    /// List all twins with optional filtering
    async fn twins(
        &self,
        ctx: &Context<'_>,
        limit: Option<i32>,
        offset: Option<i32>,
        status: Option<TwinStatus>,
    ) -> GqlResult<Vec<Twin>> {
        let state = ctx.data::<Arc<AppState>>()?;

        let limit = limit.unwrap_or(10).min(100) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;

        // Query from database
        let twins = state
            .db
            .list_twins(limit, offset, status)
            .await
            .map_err(|e| GqlError::new(format!("Database error: {}", e)))?;

        Ok(twins
            .into_iter()
            .map(|data| Twin {
                id: ID(data.id),
                name: data.name,
                description: data.description,
                created_at: data.created_at,
                updated_at: data.updated_at,
                status: TwinStatus::Active,
            })
            .collect())
    }

    /// Get simulation results for a twin
    async fn simulation_results(
        &self,
        ctx: &Context<'_>,
        twin_id: ID,
        limit: Option<i32>,
    ) -> GqlResult<Vec<SimulationResult>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(10).min(100);

        let results = state
            .db
            .get_simulation_results(&twin_id.to_string(), limit as usize)
            .await
            .map_err(|e| GqlError::new(format!("Database error: {}", e)))?;

        Ok(results)
    }

    /// Search twins by name
    async fn search_twins(
        &self,
        ctx: &Context<'_>,
        query: String,
        limit: Option<i32>,
    ) -> GqlResult<Vec<Twin>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(10).min(100);

        let results = state
            .db
            .search_twins(&query, limit as usize)
            .await
            .map_err(|e| GqlError::new(format!("Database error: {}", e)))?;

        Ok(results
            .into_iter()
            .map(|data| Twin {
                id: ID(data.id),
                name: data.name,
                description: data.description,
                created_at: data.created_at,
                updated_at: data.updated_at,
                status: TwinStatus::Active,
            })
            .collect())
    }
}

/// GraphQL mutation root
pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Echo mutation for testing
    async fn echo(&self, _ctx: &Context<'_>, message: String) -> String {
        message
    }

    /// Create a new twin
    async fn create_twin(&self, ctx: &Context<'_>, input: CreateTwinInput) -> GqlResult<Twin> {
        let state = ctx.data::<Arc<AppState>>()?;

        // Validate input
        if input.name.is_empty() {
            return Err(GqlError::new("Name cannot be empty"));
        }

        // Create twin in database
        let twin = state
            .db
            .create_twin(input)
            .await
            .map_err(|e| GqlError::new(format!("Failed to create twin: {}", e)))?;

        // Publish event
        if let Some(tx) = ctx.data_opt::<broadcast::Sender<Event>>() {
            let event = Event {
                id: ID::from(uuid::Uuid::new_v4().to_string()),
                event_type: "twin.created".to_string(),
                twin_id: Some(twin.id.clone()),
                data: serde_json::to_string(&twin).unwrap_or_default(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = tx.send(event);
        }

        state
            .metrics
            .increment_counter("graphql.twins.created", 1, None);
        Ok(twin)
    }

    /// Update an existing twin
    async fn update_twin(&self, ctx: &Context<'_>, input: UpdateTwinInput) -> GqlResult<Twin> {
        let state = ctx.data::<Arc<AppState>>()?;

        // Update twin in database
        let twin = state
            .db
            .update_twin(input)
            .await
            .map_err(|e| GqlError::new(format!("Failed to update twin: {}", e)))?;

        // Publish event
        if let Some(tx) = ctx.data_opt::<broadcast::Sender<Event>>() {
            let event = Event {
                id: ID::from(uuid::Uuid::new_v4().to_string()),
                event_type: "twin.updated".to_string(),
                twin_id: Some(twin.id.clone()),
                data: serde_json::to_string(&twin).unwrap_or_default(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            let _ = tx.send(event);
        }

        state
            .metrics
            .increment_counter("graphql.twins.updated", 1, None);
        Ok(twin)
    }

    /// Delete a twin
    async fn delete_twin(&self, ctx: &Context<'_>, id: ID) -> GqlResult<bool> {
        let state = ctx.data::<Arc<AppState>>()?;

        let deleted = state
            .db
            .delete_twin(&id.to_string())
            .await
            .map_err(|e| GqlError::new(format!("Failed to delete twin: {}", e)))?;

        if deleted {
            // Publish event
            if let Some(tx) = ctx.data_opt::<broadcast::Sender<Event>>() {
                let event = Event {
                    id: ID::from(uuid::Uuid::new_v4().to_string()),
                    event_type: "twin.deleted".to_string(),
                    twin_id: Some(id.clone()),
                    data: serde_json::json!({"id": id.to_string()}).to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let _ = tx.send(event);
            }

            state
                .metrics
                .increment_counter("graphql.twins.deleted", 1, None);
        }

        Ok(deleted)
    }

    /// Start a simulation
    async fn start_simulation(
        &self,
        ctx: &Context<'_>,
        twin_id: ID,
    ) -> GqlResult<SimulationResult> {
        let state = ctx.data::<Arc<AppState>>()?;

        let result = state
            .db
            .start_simulation(&twin_id.to_string())
            .await
            .map_err(|e| GqlError::new(format!("Failed to start simulation: {}", e)))?;

        state
            .metrics
            .increment_counter("graphql.simulations.started", 1, None);
        Ok(result)
    }
}

/// GraphQL subscription root
/// Note: Real-time subscriptions are available via WebSocket at /ws
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to twin events (placeholder - use WebSocket for real-time updates)
    async fn twin_events(
        &self,
        _ctx: &Context<'_>,
        _twin_id: Option<ID>,
    ) -> impl Stream<Item = Event> {
        // Return an empty stream - real subscriptions use WebSocket
        futures_util::stream::empty()
    }

    /// Subscribe to all events (placeholder - use WebSocket for real-time updates)
    async fn events(
        &self,
        _ctx: &Context<'_>,
        _event_type: Option<String>,
    ) -> impl Stream<Item = Event> {
        // Return an empty stream - real subscriptions use WebSocket
        futures_util::stream::empty()
    }
}

/// GraphQL schema type
pub type ApiSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

/// Create GraphQL schema
pub fn create_schema(state: Arc<AppState>, event_tx: broadcast::Sender<Event>) -> ApiSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(state)
        .data(event_tx)
        .finish()
}

/// GraphQL Playground handler
pub async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(
        GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/graphql/ws"),
    ))
}
