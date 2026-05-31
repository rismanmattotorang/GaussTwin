//! WebSocket server implementation

use crate::{AppState, Error};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WsMessage {
    /// Subscribe to events
    Subscribe { topics: Vec<String> },
    /// Unsubscribe from events
    Unsubscribe { topics: Vec<String> },
    /// Ping message
    Ping { timestamp: i64 },
    /// Pong response
    Pong { timestamp: i64 },
    /// Event notification
    Event {
        topic: String,
        data: serde_json::Value,
        timestamp: String,
    },
    /// Metric update
    Metric {
        twin_id: String,
        name: String,
        value: f64,
        unit: Option<String>,
        timestamp: String,
    },
    /// Command to execute
    Command {
        id: String,
        twin_id: String,
        action: String,
        params: serde_json::Value,
    },
    /// Command response
    CommandResponse {
        id: String,
        success: bool,
        result: Option<serde_json::Value>,
        error: Option<String>,
    },
    /// Error message
    Error { message: String, code: Option<i32> },
    /// Acknowledgment
    Ack { message_id: String },
}

/// Client connection info
#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub id: String,
    pub subscriptions: Vec<String>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// WebSocket connection manager
pub struct ConnectionManager {
    clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
    event_tx: broadcast::Sender<WsMessage>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Register a new client
    pub async fn register_client(&self, client_id: String) {
        let info = ClientInfo {
            id: client_id.clone(),
            subscriptions: Vec::new(),
            connected_at: chrono::Utc::now(),
        };

        self.clients.write().await.insert(client_id, info);
    }

    /// Unregister a client
    pub async fn unregister_client(&self, client_id: &str) {
        self.clients.write().await.remove(client_id);
    }

    /// Subscribe client to topics
    pub async fn subscribe(&self, client_id: &str, topics: Vec<String>) {
        if let Some(client) = self.clients.write().await.get_mut(client_id) {
            for topic in topics {
                if !client.subscriptions.contains(&topic) {
                    client.subscriptions.push(topic);
                }
            }
        }
    }

    /// Unsubscribe client from topics
    pub async fn unsubscribe(&self, client_id: &str, topics: Vec<String>) {
        if let Some(client) = self.clients.write().await.get_mut(client_id) {
            client.subscriptions.retain(|t| !topics.contains(t));
        }
    }

    /// Get client info
    pub async fn get_client(&self, client_id: &str) -> Option<ClientInfo> {
        self.clients.read().await.get(client_id).cloned()
    }

    /// Get all connected clients
    pub async fn get_all_clients(&self) -> Vec<ClientInfo> {
        self.clients.read().await.values().cloned().collect()
    }

    /// Broadcast message to all clients subscribed to a topic
    pub fn broadcast(&self, topic: &str, message: WsMessage) {
        let _ = self.event_tx.send(message);
    }

    /// Get event receiver
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<WsMessage> {
        self.event_tx.subscribe()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket server implementation
pub struct WebSocketServer {
    manager: Arc<ConnectionManager>,
}

impl WebSocketServer {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(ConnectionManager::new()),
        }
    }

    pub fn with_manager(manager: Arc<ConnectionManager>) -> Self {
        Self { manager }
    }

    /// Handle WebSocket connection upgrade
    pub async fn handle_connection(
        ws: WebSocketUpgrade,
        axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    ) -> impl IntoResponse {
        let manager = Arc::new(ConnectionManager::new());
        ws.on_upgrade(move |socket| Self::handle_socket_with_manager(socket, state, manager))
    }

    /// Handle WebSocket connection upgrade with custom manager
    pub async fn handle_connection_with_manager(
        ws: WebSocketUpgrade,
        axum::extract::State(state): axum::extract::State<Arc<AppState>>,
        manager: Arc<ConnectionManager>,
    ) -> impl IntoResponse {
        ws.on_upgrade(move |socket| Self::handle_socket_with_manager(socket, state, manager))
    }

    async fn handle_socket_with_manager(
        socket: WebSocket,
        state: Arc<AppState>,
        manager: Arc<ConnectionManager>,
    ) {
        let client_id = Uuid::new_v4().to_string();
        info!("WebSocket client connected: {}", client_id);

        // Register client
        manager.register_client(client_id.clone()).await;
        state
            .metrics
            .increment_counter("websocket.connections.total", 1, None);
        state
            .metrics
            .set_gauge("websocket.connections.active", 1.0, None);

        // Split socket into sender and receiver
        let (sender, mut receiver) = socket.split();

        // Wrap sender in Arc<Mutex> for shared access
        let sender = Arc::new(tokio::sync::Mutex::new(sender));

        // Subscribe to broadcast events
        let mut event_rx = manager.subscribe_to_events();
        let client_id_clone = client_id.clone();
        let manager_clone = manager.clone();
        let sender_clone = sender.clone();

        // Spawn task to handle outgoing messages
        let send_task = tokio::spawn(async move {
            while let Ok(msg) = event_rx.recv().await {
                // Check if client is subscribed to this message's topic
                let client_info = manager_clone.get_client(&client_id_clone).await;

                let should_send = match &msg {
                    WsMessage::Event { topic, .. } => client_info
                        .as_ref()
                        .map(|c| c.subscriptions.contains(topic))
                        .unwrap_or(false),
                    WsMessage::Metric { twin_id, .. } => client_info
                        .as_ref()
                        .map(|c| {
                            c.subscriptions
                                .iter()
                                .any(|s| s.starts_with(&format!("twin.{}", twin_id)))
                        })
                        .unwrap_or(false),
                    _ => true, // Send system messages to all clients
                };

                if should_send {
                    if let Ok(json) = serde_json::to_string(&msg) {
                        let mut sender = sender_clone.lock().await;
                        if sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        // Handle incoming messages
        let manager_clone = manager.clone();
        let state_clone = state.clone();
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    let mut sender_guard = sender.lock().await;
                    if let Err(e) = Self::handle_text_message_locked(
                        &text,
                        &client_id,
                        &mut *sender_guard,
                        &state_clone,
                        &manager_clone,
                    )
                    .await
                    {
                        error!("Error handling message: {}", e);
                        let error_msg = WsMessage::Error {
                            message: e.to_string(),
                            code: Some(500),
                        };
                        if let Ok(json) = serde_json::to_string(&error_msg) {
                            let _ = sender_guard.send(Message::Text(json)).await;
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    warn!(
                        "Received binary message from {}: {} bytes",
                        client_id,
                        data.len()
                    );
                }
                Ok(Message::Ping(data)) => {
                    let mut sender_guard = sender.lock().await;
                    if sender_guard.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    // Pong received
                }
                Ok(Message::Close(_)) => {
                    info!("Client {} closed connection", client_id);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for client {}: {}", client_id, e);
                    break;
                }
            }
        }

        // Cleanup
        send_task.abort();
        manager.unregister_client(&client_id).await;
        state
            .metrics
            .set_gauge("websocket.connections.active", 0.0, None);
        info!("WebSocket client disconnected: {}", client_id);
    }

    async fn handle_text_message_locked(
        text: &str,
        client_id: &str,
        sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
        state: &Arc<AppState>,
        manager: &Arc<ConnectionManager>,
    ) -> Result<(), Error> {
        let message: WsMessage = serde_json::from_str(text)
            .map_err(|e| Error::InvalidInput(format!("Invalid JSON: {}", e)))?;

        match message {
            WsMessage::Subscribe { topics } => {
                manager.subscribe(client_id, topics.clone()).await;
                info!("Client {} subscribed to topics: {:?}", client_id, topics);

                let ack = WsMessage::Ack {
                    message_id: Uuid::new_v4().to_string(),
                };
                let json = serde_json::to_string(&ack)?;
                sender
                    .send(Message::Text(json))
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;

                state.metrics.increment_counter(
                    "websocket.subscriptions.total",
                    topics.len() as u64,
                    None,
                );
            }

            WsMessage::Unsubscribe { topics } => {
                manager.unsubscribe(client_id, topics.clone()).await;
                info!(
                    "Client {} unsubscribed from topics: {:?}",
                    client_id, topics
                );

                let ack = WsMessage::Ack {
                    message_id: Uuid::new_v4().to_string(),
                };
                let json = serde_json::to_string(&ack)?;
                sender
                    .send(Message::Text(json))
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
            }

            WsMessage::Ping { timestamp } => {
                let pong = WsMessage::Pong { timestamp };
                let json = serde_json::to_string(&pong)?;
                sender
                    .send(Message::Text(json))
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
            }

            WsMessage::Command {
                id,
                twin_id,
                action,
                params,
            } => {
                info!(
                    "Client {} sent command {} for twin {}",
                    client_id, action, twin_id
                );

                // Process command (implementation would call appropriate service)
                let result = serde_json::json!({
                    "status": "executed",
                    "action": action,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let response = WsMessage::CommandResponse {
                    id,
                    success: true,
                    result: Some(result),
                    error: None,
                };

                let json = serde_json::to_string(&response)?;
                sender
                    .send(Message::Text(json))
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;

                state
                    .metrics
                    .increment_counter("websocket.commands.executed", 1, None);
            }

            _ => {
                warn!("Client {} sent unexpected message type", client_id);
            }
        }

        Ok(())
    }
}

impl Default for WebSocketServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager() {
        let manager = ConnectionManager::new();
        let client_id = "test_client".to_string();

        // Register client
        manager.register_client(client_id.clone()).await;
        let client = manager.get_client(&client_id).await;
        assert!(client.is_some());

        // Subscribe to topics
        manager
            .subscribe(&client_id, vec!["topic1".to_string(), "topic2".to_string()])
            .await;
        let client = manager.get_client(&client_id).await.unwrap();
        assert_eq!(client.subscriptions.len(), 2);

        // Unsubscribe from topic
        manager
            .unsubscribe(&client_id, vec!["topic1".to_string()])
            .await;
        let client = manager.get_client(&client_id).await.unwrap();
        assert_eq!(client.subscriptions.len(), 1);

        // Unregister client
        manager.unregister_client(&client_id).await;
        let client = manager.get_client(&client_id).await;
        assert!(client.is_none());
    }

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::Event {
            topic: "test".to_string(),
            data: serde_json::json!({"key": "value"}),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: WsMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            WsMessage::Event { topic, .. } => assert_eq!(topic, "test"),
            _ => panic!("Wrong message type"),
        }
    }
}
