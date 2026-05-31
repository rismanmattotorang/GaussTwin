use crate::{
    error::Result,
    graphql::{create_schema, Event, MutationRoot, QueryRoot, SubscriptionRoot},
    rest,
    websocket::WebSocketServer,
    AppState,
};
use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{routing::get, Extension, Router};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

/// Server instance
pub struct Server {
    /// Application state
    state: Arc<AppState>,
    /// Shutdown signal
    shutdown: broadcast::Sender<()>,
}

impl Server {
    /// Create a new server instance
    pub fn new(state: AppState) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            state: Arc::new(state),
            shutdown: shutdown_tx,
        }
    }

    /// Start the server
    pub async fn start(&self) -> Result<()> {
        // Create event broadcast channel for GraphQL subscriptions
        let (event_tx, _) = broadcast::channel::<Event>(1000);

        // Create GraphQL schema
        let schema = create_schema(self.state.clone(), event_tx.clone());

        // Create REST API router
        let api_router = rest::create_router(self.state.clone());

        // Create HTTP router
        let router = Router::new()
            // REST API routes (mounted at /api/v1)
            .nest("/api/v1", api_router)
            // GraphQL routes
            .route("/graphql", get(graphql_playground).post(graphql_handler))
            // WebSocket route
            .route("/ws", get(WebSocketServer::handle_connection))
            // Root health check
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .layer(Extension(schema))
            .with_state(self.state.clone());

        // Get server address
        let http_addr = self.state.config.http.addr;

        info!("🚀 GaussTwin API Server starting...");
        info!("   HTTP Server:   http://{}", http_addr);
        info!("   REST API:      http://{}/api/v1", http_addr);
        info!("   GraphQL:       http://{}/graphql", http_addr);
        info!("   WebSocket:     ws://{}/ws", http_addr);
        info!("   Health Check:  http://{}/health", http_addr);

        // Start HTTP server (axum 0.6 style)
        let server = axum::Server::bind(&http_addr)
            .serve(router.into_make_service())
            .with_graceful_shutdown(shutdown_signal(self.shutdown.subscribe()));

        if let Err(e) = server.await {
            tracing::error!("Server error: {}", e);
        }

        info!("Server shutdown complete");
        Ok(())
    }

    /// Stop the server
    pub async fn stop(&self) {
        let _ = self.shutdown.send(());
    }

    /// Check if the server is ready
    pub fn is_ready(&self) -> bool {
        true
    }
}

/// Root handler - API overview
async fn root_handler() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({
        "name": "GaussTwin API Server",
        "version": env!("CARGO_PKG_VERSION"),
        "documentation": "/graphql",
        "health": "/health",
        "endpoints": {
            "rest": "/api/v1",
            "graphql": "/graphql",
            "websocket": "/ws"
        }
    }))
}

/// GraphQL playground handler
async fn graphql_playground() -> impl axum::response::IntoResponse {
    axum::response::Html(playground_source(
        GraphQLPlaygroundConfig::new("/graphql")
            .subscription_endpoint("/ws")
            .title("GaussTwin GraphQL Playground"),
    ))
}

/// GraphQL handler
async fn graphql_handler(
    schema: Extension<crate::graphql::ApiSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

/// Health check handler
async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    state.metrics.increment_counter("api.health.calls", 1, None);

    axum::Json(serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "services": {
            "database": "ok",
            "cache": "ok",
            "metrics": "ok"
        }
    }))
}

/// Shutdown signal handler
async fn shutdown_signal(mut shutdown: broadcast::Receiver<()>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let shutdown_recv = async {
        let _ = shutdown.recv().await;
    };

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, shutting down...");
        }
        _ = shutdown_recv => {
            info!("Received shutdown signal");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use axum::http::StatusCode;
    use hyper::Client;

    #[tokio::test]
    async fn test_server_startup() {
        let config = ServerConfig::default();
        let state = AppState::new(config).await.unwrap();
        let server = Server::new(state);

        // Start server in background
        tokio::spawn(async move {
            server.start().await.unwrap();
        });

        // Wait for server to start
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Test health endpoint
        let client = Client::new();
        let resp = client
            .get("http://localhost:8080/health".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
