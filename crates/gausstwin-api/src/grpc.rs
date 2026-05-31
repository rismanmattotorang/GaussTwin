//! gRPC server implementation

use crate::{AppState, Error};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt};
use tonic::{Code, Request, Response, Status};

// Proto message types (normally generated from .proto files)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwinRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwinResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTwinRequest {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTwinRequest {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTwinRequest {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTwinResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTwinsRequest {
    pub limit: i32,
    pub offset: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTwinsResponse {
    pub twins: Vec<TwinResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationRequest {
    pub twin_id: String,
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResponse {
    pub id: String,
    pub twin_id: String,
    pub status: String,
    pub results: Vec<MetricValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRequest {
    pub twin_id: String,
    pub metrics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricUpdate {
    pub twin_id: String,
    pub metric: MetricValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: i64,
}

/// Twin service trait (normally auto-generated from .proto)
#[tonic::async_trait]
pub trait TwinService: Send + Sync + 'static {
    /// Get a twin by ID
    async fn get_twin(
        &self,
        request: Request<TwinRequest>,
    ) -> Result<Response<TwinResponse>, Status>;

    /// Create a new twin
    async fn create_twin(
        &self,
        request: Request<CreateTwinRequest>,
    ) -> Result<Response<TwinResponse>, Status>;

    /// Update an existing twin
    async fn update_twin(
        &self,
        request: Request<UpdateTwinRequest>,
    ) -> Result<Response<TwinResponse>, Status>;

    /// Delete a twin
    async fn delete_twin(
        &self,
        request: Request<DeleteTwinRequest>,
    ) -> Result<Response<DeleteTwinResponse>, Status>;

    /// List all twins
    async fn list_twins(
        &self,
        request: Request<ListTwinsRequest>,
    ) -> Result<Response<ListTwinsResponse>, Status>;

    /// Start a simulation
    async fn start_simulation(
        &self,
        request: Request<SimulationRequest>,
    ) -> Result<Response<SimulationResponse>, Status>;

    /// Stream simulation metrics (server streaming)
    type StreamMetricsStream: Stream<Item = Result<MetricUpdate, Status>> + Send + 'static;

    async fn stream_metrics(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamMetricsStream>, Status>;

    /// Bidirectional streaming for real-time updates
    type BidirectionalStreamStream: Stream<Item = Result<MetricUpdate, Status>> + Send + 'static;

    async fn bidirectional_stream(
        &self,
        request: Request<tonic::Streaming<MetricUpdate>>,
    ) -> Result<Response<Self::BidirectionalStreamStream>, Status>;

    /// Health check
    async fn health(
        &self,
        request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status>;
}

/// gRPC server implementation
pub struct GrpcServer {
    state: Arc<AppState>,
    start_time: std::time::Instant,
    event_tx: broadcast::Sender<MetricUpdate>,
}

impl GrpcServer {
    pub fn new(state: Arc<AppState>) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            state,
            start_time: std::time::Instant::now(),
            event_tx,
        }
    }

    /// Convert internal error to gRPC status
    fn error_to_status(error: Error) -> Status {
        match error {
            Error::NotFound(msg) => Status::not_found(msg),
            Error::InvalidInput(msg) => Status::invalid_argument(msg),
            Error::Authentication(msg) => Status::unauthenticated(msg),
            Error::Authorization(msg) | Error::PermissionDenied(msg) => {
                Status::permission_denied(msg)
            }
            _ => Status::internal(error.to_string()),
        }
    }
}

#[tonic::async_trait]
impl TwinService for GrpcServer {
    async fn get_twin(
        &self,
        request: Request<TwinRequest>,
    ) -> Result<Response<TwinResponse>, Status> {
        let req = request.into_inner();

        let twin = self
            .state
            .db
            .get_twin(&req.id)
            .await
            .map_err(Self::error_to_status)?
            .ok_or_else(|| Status::not_found(format!("Twin {} not found", req.id)))?;

        self.state
            .metrics
            .increment_counter("grpc.get_twin.calls", 1, None);

        Ok(Response::new(TwinResponse {
            id: twin.id,
            name: twin.name,
            description: twin.description.unwrap_or_default(),
            status: "active".to_string(),
        }))
    }

    async fn create_twin(
        &self,
        request: Request<CreateTwinRequest>,
    ) -> Result<Response<TwinResponse>, Status> {
        let req = request.into_inner();

        if req.name.is_empty() {
            return Err(Status::invalid_argument("Name cannot be empty"));
        }

        let twin = self
            .state
            .db
            .create_twin(crate::graphql::CreateTwinInput {
                name: req.name,
                description: Some(req.description),
            })
            .await
            .map_err(Self::error_to_status)?;

        self.state
            .metrics
            .increment_counter("grpc.create_twin.calls", 1, None);

        Ok(Response::new(TwinResponse {
            id: twin.id.to_string(),
            name: twin.name,
            description: twin.description.unwrap_or_default(),
            status: "active".to_string(),
        }))
    }

    async fn update_twin(
        &self,
        request: Request<UpdateTwinRequest>,
    ) -> Result<Response<TwinResponse>, Status> {
        let req = request.into_inner();

        let twin = self
            .state
            .db
            .update_twin(crate::graphql::UpdateTwinInput {
                id: async_graphql::ID(req.id),
                name: req.name,
                description: req.description,
                status: None,
            })
            .await
            .map_err(Self::error_to_status)?;

        self.state
            .metrics
            .increment_counter("grpc.update_twin.calls", 1, None);

        Ok(Response::new(TwinResponse {
            id: twin.id.to_string(),
            name: twin.name,
            description: twin.description.unwrap_or_default(),
            status: "active".to_string(),
        }))
    }

    async fn delete_twin(
        &self,
        request: Request<DeleteTwinRequest>,
    ) -> Result<Response<DeleteTwinResponse>, Status> {
        let req = request.into_inner();

        let success = self
            .state
            .db
            .delete_twin(&req.id)
            .await
            .map_err(Self::error_to_status)?;

        self.state
            .metrics
            .increment_counter("grpc.delete_twin.calls", 1, None);

        Ok(Response::new(DeleteTwinResponse { success }))
    }

    async fn list_twins(
        &self,
        request: Request<ListTwinsRequest>,
    ) -> Result<Response<ListTwinsResponse>, Status> {
        let req = request.into_inner();

        let limit = req.limit.max(1).min(100) as usize;
        let offset = req.offset.max(0) as usize;

        let twins = self
            .state
            .db
            .list_twins(limit, offset, None)
            .await
            .map_err(Self::error_to_status)?;

        let twin_responses: Vec<TwinResponse> = twins
            .into_iter()
            .map(|t| TwinResponse {
                id: t.id,
                name: t.name,
                description: t.description.unwrap_or_default(),
                status: "active".to_string(),
            })
            .collect();

        let total = twin_responses.len() as i64;

        self.state
            .metrics
            .increment_counter("grpc.list_twins.calls", 1, None);

        Ok(Response::new(ListTwinsResponse {
            twins: twin_responses,
            total,
        }))
    }

    async fn start_simulation(
        &self,
        request: Request<SimulationRequest>,
    ) -> Result<Response<SimulationResponse>, Status> {
        let req = request.into_inner();

        let result = self
            .state
            .db
            .start_simulation(&req.twin_id)
            .await
            .map_err(Self::error_to_status)?;

        self.state
            .metrics
            .increment_counter("grpc.start_simulation.calls", 1, None);

        Ok(Response::new(SimulationResponse {
            id: result.id.to_string(),
            twin_id: result.twin_id.to_string(),
            status: result.status,
            results: result
                .metrics
                .into_iter()
                .map(|m| MetricValue {
                    name: m.name,
                    value: m.value,
                    unit: m.unit.unwrap_or_default(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })
                .collect(),
        }))
    }

    type StreamMetricsStream = Pin<Box<dyn Stream<Item = Result<MetricUpdate, Status>> + Send>>;

    async fn stream_metrics(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::StreamMetricsStream>, Status> {
        let req = request.into_inner();
        let twin_id = req.twin_id;
        let metrics = req.metrics;

        // Create a stream that emits metric updates
        let rx = self.event_tx.subscribe();

        let stream =
            tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(
                move |result| match result {
                    Ok(update)
                        if update.twin_id == twin_id
                            && (metrics.is_empty() || metrics.contains(&update.metric.name)) =>
                    {
                        Some(Ok(update))
                    }
                    _ => None,
                },
            );

        self.state
            .metrics
            .increment_counter("grpc.stream_metrics.calls", 1, None);

        Ok(Response::new(Box::pin(stream) as Self::StreamMetricsStream))
    }

    type BidirectionalStreamStream =
        Pin<Box<dyn Stream<Item = Result<MetricUpdate, Status>> + Send>>;

    async fn bidirectional_stream(
        &self,
        request: Request<tonic::Streaming<MetricUpdate>>,
    ) -> Result<Response<Self::BidirectionalStreamStream>, Status> {
        let mut in_stream = request.into_inner();
        let event_tx = self.event_tx.clone();
        let rx = event_tx.subscribe();

        // Spawn task to handle incoming stream
        tokio::spawn(async move {
            while let Ok(Some(update)) = in_stream.message().await {
                // Process incoming metric update
                let _ = event_tx.send(update);
            }
        });

        // Create outgoing stream
        let stream =
            tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(|result| match result {
                Ok(update) => Some(Ok(update)),
                Err(_) => None,
            });

        self.state
            .metrics
            .increment_counter("grpc.bidirectional_stream.calls", 1, None);

        Ok(Response::new(
            Box::pin(stream) as Self::BidirectionalStreamStream
        ))
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let uptime = self.start_time.elapsed().as_secs() as i64;

        Ok(Response::new(HealthResponse {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
        }))
    }
}

/// Create and configure gRPC server
pub fn create_grpc_server(state: Arc<AppState>) -> GrpcServer {
    GrpcServer::new(state)
}
