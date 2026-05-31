//! GaussTwin Agent Framework
//!
//! A high-performance, enterprise-grade agent-based modeling framework with features including:
//! - SIMD-accelerated agent operations
//! - Advanced spatial indexing
//! - LLM-powered agent reasoning
//! - Multi-agent reinforcement learning
//! - Distributed agent simulation
//! - Real-time visualization
//!
//! # Features
//! - Multiple agent architectures (Reactive, BDI, Cognitive)
//! - Advanced spatial awareness and pathfinding
//! - Agent communication and coordination
//! - Dynamic agent creation/destruction
//! - Agent memory and learning
//! - Comprehensive metrics and monitoring

mod architectures;
mod cognitive;
mod reactive;

pub use architectures::*;
pub use cognitive::*;
pub use reactive::*;

use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque},
    fmt::Debug,
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use dashmap::DashMap;
use ndarray::{Array1, Array2};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Agent goal for planning and decision making
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Goal {
    pub id: Uuid,
    pub description: String,
    pub priority: Priority,
    pub preconditions: serde_json::Value,
    pub success_conditions: serde_json::Value,
    pub failure_conditions: serde_json::Value,
}

/// Plan execution status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PlanStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed(String),
}

/// Priority levels for messages and actions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// Comprehensive error type for agent operations
#[derive(Debug, Error)]
pub enum AgentError {
    /// Errors related to agent state
    #[error("State error: {kind:?}")]
    State {
        kind: StateErrorKind,
        message: String,
        agent_id: Option<Uuid>,
    },

    /// Errors related to agent behavior
    #[error("Behavior error: {kind:?}")]
    Behavior {
        kind: BehaviorErrorKind,
        message: String,
        agent_id: Option<Uuid>,
    },

    /// Errors related to agent communication
    #[error("Communication error: {kind:?}")]
    Communication {
        kind: CommunicationErrorKind,
        message: String,
        agent_id: Option<Uuid>,
    },

    /// Errors related to agent resources
    #[error("Resource error: {kind:?}")]
    Resource {
        kind: ResourceErrorKind,
        message: String,
        agent_id: Option<Uuid>,
    },

    /// Internal errors
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Types of state-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateErrorKind {
    /// Invalid state transition
    InvalidTransition,
    /// State not found
    NotFound,
    /// Invalid state data
    InvalidData,
    /// State corruption
    Corruption,
}

/// Types of behavior-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorErrorKind {
    /// Invalid action
    InvalidAction,
    /// Action failed
    ActionFailed,
    /// Decision failed
    DecisionFailed,
    /// Learning failed
    LearningFailed,
}

/// Types of communication-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunicationErrorKind {
    /// Message delivery failed
    DeliveryFailed,
    /// Invalid message format
    InvalidFormat,
    /// Channel error
    ChannelError,
    /// Protocol error
    ProtocolError,
    /// Send failed
    SendFailed,
}

/// Types of resource-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceErrorKind {
    /// Resource not available
    NotAvailable,
    /// Resource limit exceeded
    LimitExceeded,
    /// Resource allocation failed
    AllocationFailed,
    /// Resource conflict
    Conflict,
}

/// Core agent trait defining behavior and capabilities
#[async_trait]
pub trait Agent: Send + Sync {
    /// Agent's internal state type
    type State: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    /// Agent's observation type
    type Observation: Clone + Send + Sync;

    /// Agent's action type
    type Action: Clone + Send + Sync;

    /// Get agent's unique identifier
    fn id(&self) -> Uuid;

    /// Get agent's current state
    fn state(&self) -> &Self::State;

    /// Update agent's state
    fn set_state(&mut self, state: Self::State) -> Result<(), AgentError>;

    /// Observe environment and other agents
    async fn observe(&self, ctx: &AgentContext) -> Result<Self::Observation, AgentError>;

    /// Decide next action based on observation
    async fn decide(&mut self, obs: &Self::Observation) -> Result<Self::Action, AgentError>;

    /// Execute decided action
    async fn act(
        &mut self,
        action: &Self::Action,
        ctx: &mut AgentContext,
    ) -> Result<(), AgentError>;

    /// Handle incoming messages
    async fn handle_message(&mut self, msg: Message) -> Result<(), AgentError>;

    /// Get agent's memory
    fn memory(&self) -> Option<&AgentMemory>;

    /// Update agent's memory
    fn update_memory(&mut self, memory: AgentMemory) -> Result<(), AgentError>;
}

/// Context provided to agents during execution
pub struct AgentContext {
    /// Current simulation time
    pub time: f64,

    /// Spatial environment
    pub space: Arc<dyn Space>,

    /// Communication channels
    pub channels: Arc<DashMap<String, broadcast::Sender<Message>>>,

    /// Shared resources
    pub resources: Arc<DashMap<String, Box<dyn Any + Send + Sync>>>,

    /// Metrics collector
    pub metrics: Arc<MetricsCollector>,
}

/// Agent memory for storing experiences and learned knowledge
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentMemory {
    /// Short-term memory (recent experiences)
    pub short_term: VecDeque<Experience>,

    /// Long-term memory (learned patterns/knowledge)
    pub long_term: HashMap<String, Vec<f32>>,

    /// Semantic memory (LLM embeddings)
    pub semantic: Option<Vec<f32>>,

    /// Memory capacity limits
    pub capacity: MemoryCapacity,
}

/// Memory capacity configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryCapacity {
    /// Maximum short-term memory size
    pub short_term_size: usize,

    /// Maximum long-term memory size
    pub long_term_size: usize,

    /// Maximum semantic memory size
    pub semantic_size: Option<usize>,
}

/// Agent experience for learning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Experience {
    /// State before action
    pub state: Vec<f32>,

    /// Action taken
    pub action: Vec<f32>,

    /// Reward received
    pub reward: f32,

    /// Next state after action
    pub next_state: Vec<f32>,

    /// Whether episode ended
    pub done: bool,

    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Message for agent communication
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: Uuid,

    /// Sender agent ID
    pub sender: Uuid,

    /// Receiver agent ID
    pub receiver: Option<Uuid>,

    /// Message content
    pub content: MessageContent,

    /// Message metadata
    pub metadata: Option<serde_json::Value>,

    /// Message timestamp
    pub timestamp: f64,
}

/// Message content types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MessageContent {
    /// Raw text message
    Text(String),

    /// Binary data
    Binary(Vec<u8>),

    /// JSON data
    Json(serde_json::Value),

    /// Vector data
    Vector(Vec<f32>),
}

/// Spatial environment abstraction
#[async_trait]
pub trait Space: Send + Sync {
    async fn add_agent(
        &mut self,
        agent: Box<DynAgent>,
        position: Position,
    ) -> Result<(), AgentError>;
    async fn remove_agent(&mut self, agent_id: Uuid) -> Result<(), AgentError>;
    async fn move_agent(
        &mut self,
        agent_id: Uuid,
        new_position: Position,
    ) -> Result<(), AgentError>;
    async fn get_agent_position(&self, agent_id: Uuid) -> Result<Position, AgentError>;
    async fn get_neighbors(&self, agent_id: Uuid, radius: f64) -> Result<Vec<Uuid>, AgentError>;
    async fn get_agents_in_radius(
        &self,
        position: Position,
        radius: f64,
    ) -> Result<Vec<Uuid>, AgentError>;
}

/// Position in 2D/3D space
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Position {
    /// 2D position
    Vec2(f64, f64),

    /// 3D position
    Vec3(f64, f64, f64),
}

/// Metrics collector for agent monitoring
pub struct MetricsCollector {
    /// Agent-specific metrics
    agent_metrics: DashMap<Uuid, AgentMetrics>,

    /// Global metrics
    global_metrics: Arc<RwLock<GlobalMetrics>>,
}

/// Agent-specific metrics
#[derive(Clone, Debug, Default)]
pub struct AgentMetrics {
    /// Number of actions taken
    pub actions: u64,

    /// Number of messages sent
    pub messages_sent: u64,

    /// Number of messages received
    pub messages_received: u64,

    /// Average decision time
    pub avg_decision_time: f64,

    /// Memory usage
    pub memory_usage: usize,
}

/// Global simulation metrics
#[derive(Clone, Debug, Default)]
pub struct GlobalMetrics {
    /// Total number of agents
    pub total_agents: u64,

    /// Total number of actions
    pub total_actions: u64,

    /// Total number of messages
    pub total_messages: u64,

    /// Average actions per second
    pub actions_per_second: f64,

    /// Average messages per second
    pub messages_per_second: f64,
}

// Trait-object safe alias for heterogeneous agents used across the crate
pub type DynAgent = dyn Agent<State = serde_json::Value, Observation = serde_json::Value, Action = serde_json::Value>
    + Send
    + Sync;

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::Internal(format!("Serialization error: {}", err))
    }
}

impl From<tokio::sync::broadcast::error::SendError<Message>> for AgentError {
    fn from(err: tokio::sync::broadcast::error::SendError<Message>) -> Self {
        AgentError::Communication {
            kind: CommunicationErrorKind::SendFailed,
            message: format!("Failed to send message: {}", err),
            agent_id: None,
        }
    }
}

/// Communication intent for agent communication
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommunicationIntent {
    pub target: String,
    pub content: String,
    pub priority: Priority,
}

// ... implementation of traits and structs ...
