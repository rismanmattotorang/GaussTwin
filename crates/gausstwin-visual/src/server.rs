use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use std::net::SocketAddr;

use crate::{
    analytics::Context,
    dashboard::{Dashboard, DashboardConfig, DashboardId},
    scenarios::{ScenarioConfig, ScenarioId},
    SystemState,
};

type SharedState = Arc<RwLock<SystemState>>;

pub async fn start_server(addr: &str, state: SharedState) -> crate::Result<()> {
    let app = Router::new()
        .route(
            "/api/dashboards",
            get(list_dashboards).post(create_dashboard),
        )
        .route("/api/dashboards/:id", get(get_dashboard))
        .route("/api/analytics/predict", post(predict))
        .route("/api/analytics/recommend", post(recommend))
        .route("/api/scenarios", post(create_scenario))
        .route("/api/scenarios/:id", get(get_scenario_results))
        .route("/ws", get(websocket_handler))
        .with_state(state);

    let addr: SocketAddr = addr
        .parse()
        .map_err(|e| crate::Error::Config(format!("Failed to parse address: {}", e)))?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| crate::Error::Server(format!("Failed to bind to address: {}", e)))?;

    axum::Server::from_tcp(listener.into_std()?)
        .map_err(|e| crate::Error::Server(format!("Failed to create server: {}", e)))?
        .serve(app.into_make_service())
        .await
        .map_err(|e| crate::Error::Server(format!("Server error: {}", e)))?;

    Ok(())
}

// Dashboard endpoints
async fn list_dashboards(State(state): State<SharedState>) -> impl IntoResponse {
    let state = state.read().await;
    let dashboards: Vec<_> = state
        .dashboards
        .iter()
        .map(|d| {
            json!({
                "id": d.id(),
                "title": d.config.title,
                "description": d.config.description,
            })
        })
        .collect();
    Json(json!(dashboards))
}

async fn create_dashboard(
    State(state): State<SharedState>,
    Json(config): Json<DashboardConfig>,
) -> impl IntoResponse {
    let mut state = state.write().await;
    let dashboard = Dashboard::new(config);
    let id = dashboard.id();
    state.dashboards.push(dashboard);
    Json(json!({ "id": id }))
}

async fn get_dashboard(
    State(state): State<SharedState>,
    Path(id): Path<DashboardId>,
) -> Json<Value> {
    let state = state.read().await;
    let response = if let Some(dashboard) = state.dashboards.iter().find(|d| d.id() == id) {
        match dashboard.render() {
            Ok(content) => json!({ "content": content }),
            Err(_) => json!({ "error": "Failed to render dashboard" }),
        }
    } else {
        json!({ "error": "Dashboard not found" })
    };
    Json(response)
}

// Analytics endpoints
async fn predict(
    State(state): State<SharedState>,
    Json(request): Json<PredictionRequest>,
) -> impl IntoResponse {
    let state = state.read().await;
    let response = match state.analytics.predict(request.data, request.horizon).await {
        Ok(predictions) => json!({ "predictions": predictions }),
        Err(_) => json!({ "error": "Prediction failed" }),
    };
    Json(response)
}

async fn recommend(
    State(state): State<SharedState>,
    Json(context): Json<Context>,
) -> impl IntoResponse {
    let state = state.read().await;
    let response = match state.analytics.recommend(context).await {
        Ok(recommendations) => json!({ "recommendations": recommendations }),
        Err(_) => json!({ "error": "Recommendation failed" }),
    };
    Json(response)
}

// Scenario endpoints
async fn create_scenario(
    State(state): State<SharedState>,
    Json(config): Json<ScenarioConfig>,
) -> impl IntoResponse {
    let mut state = state.write().await;
    let response = match state.scenarios.create_scenario(config).await {
        Ok(id) => json!({ "id": id }),
        Err(_) => json!({ "error": "Failed to create scenario" }),
    };
    Json(response)
}

async fn get_scenario_results(
    State(state): State<SharedState>,
    Path(id): Path<ScenarioId>,
) -> Json<Value> {
    let state = state.read().await;
    let response = match state.scenarios.analyze_scenario(id).await {
        Ok(results) => json!(results),
        Err(_) => json!({ "error": "Failed to analyze scenario" }),
    };
    Json(response)
}

// WebSocket handler for real-time updates
async fn websocket_handler(
    State(state): State<SharedState>,
    ws: axum::extract::WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(_socket: axum::extract::ws::WebSocket, _state: SharedState) {
    // TODO: Implement WebSocket handling for real-time updates
}

// Request/Response types
#[derive(Debug, Serialize)]
struct DashboardSummary {
    id: DashboardId,
    title: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct PredictionRequest {
    data: Vec<f64>,
    horizon: usize,
}
