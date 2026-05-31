//! Agent Framework Module
//!
//! High-performance agent system with advanced state management and messaging.

use std::any::Any;
use std::fmt::Debug;
use std::collections::HashMap;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use async_trait::async_trait;
use nalgebra::Vector3;
use crate::error::Result;
use crate::metrics::AgentMetrics;
use crate::event::Message as EventMessage;

use crate::space::VecN;
use crate::time::SimTime;

use std::fmt;

/// Unique identifier for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Create a new unique agent ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    /// Create an agent ID from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Create a deterministic agent ID from a numeric seed.
    ///
    /// Useful for reproducible scenarios and tests where stable, comparable IDs
    /// are required. The same `raw` value always maps to the same `AgentId`.
    pub fn from_raw(raw: u128) -> Self {
        Self(Uuid::from_u128(raw))
    }
    
    /// Get the raw bytes of the agent ID
    pub fn as_bytes(&self) -> [u8; 16] {
        *self.0.as_bytes()
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message that can be sent between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// The unique identifier of the agent sending the message
    pub id: AgentId,
    /// The content of the message
    pub content: String,
    /// The timestamp when the message was sent
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Agent behavior trait
pub trait AgentBehavior<S: AgentState>: Send + Sync + std::fmt::Debug {
    /// Execute the behavior for one step
    fn execute(&self, agent_state: &mut S, ctx: &mut AgentContext<S>) -> Result<()>;
}

/// Standard behavior implementations
#[derive(Debug, Clone)]
pub enum StandardBehavior {
    /// Random walk behavior with given speed
    RandomWalk {
        speed: f64,
    },
    /// Follow a target position with given speed
    FollowTarget {
        target: VecN,
        speed: f64,
    },
    /// Stationary behavior (no movement)
    Stationary,
}

impl<S: AgentState> AgentBehavior<S> for StandardBehavior {
    fn execute(&self, _agent_state: &mut S, _ctx: &mut AgentContext<S>) -> Result<()> {
        match self {
            StandardBehavior::RandomWalk { speed: _speed } => {
                // Implement random walk behavior
                Ok(())
            }
            StandardBehavior::FollowTarget { target: _target, speed: _speed } => {
                // Implement target following behavior
                Ok(())
            }
            StandardBehavior::Stationary => {
                // No movement needed
                Ok(())
            }
        }
    }
}

/// Core agent trait that all agents must implement
pub trait Agent: Send + Sync + Debug + 'static {
    /// Associated state type for the agent
    type State: AgentState;
    
    /// Get the agent's unique identifier
    fn id(&self) -> AgentId;
    
    /// Get the agent's current state
    fn state(&self) -> &Self::State;
    
    /// Get mutable access to the agent's state
    fn state_mut(&mut self) -> &mut Self::State;
    
    /// Initialize the agent (called once at creation)
    fn initialize(&mut self, ctx: &AgentContext<Self::State>) -> Result<()>;
    
    /// Execute one simulation step
    fn step(&mut self, ctx: &mut AgentContext<Self::State>) -> Result<()>;
    
    /// Handle an incoming message
    fn handle_message(&mut self, message: AgentMessage, ctx: &AgentContext<Self::State>) -> Result<()>;
    
    /// Finalize the agent (called before removal)
    fn finalize(&mut self, ctx: &AgentContext<Self::State>) -> Result<()>;
    
    /// Get agent as Any for downcasting
    fn as_any(&self) -> &dyn Any;
    
    /// Get mutable agent as Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Container for managing multiple agents
pub struct AgentSet<S: AgentState> {
    agents: HashMap<AgentId, Box<dyn Agent<State = S>>>,
}

impl<S: AgentState> Default for AgentSet<S> {
    fn default() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }
}

impl<S: AgentState> AgentSet<S> {
    /// Create a new agent set
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an agent to the set
    pub fn add_agent(&mut self, agent: Box<dyn Agent<State = S>>) {
        let id = agent.id();
        self.agents.insert(id, agent);
    }
    
    /// Remove an agent from the set
    pub fn remove_agent(&mut self, id: AgentId) -> Option<Box<dyn Agent<State = S>>> {
        self.agents.remove(&id)
    }
    
    /// Get an agent by ID
    pub fn get_agent(&self, id: &AgentId) -> Option<&dyn Agent<State = S>> {
        self.agents.get(id).map(|agent| agent.as_ref())
    }
    
    /// Get a mutable agent by ID
    pub fn get_agent_mut(&mut self, id: &AgentId) -> Option<&mut dyn Agent<State = S>> {
        self.agents.get_mut(id).map(|agent| agent.as_mut())
    }
    
    /// Get all agent IDs
    pub fn agent_ids(&self) -> Vec<AgentId> {
        self.agents.keys().copied().collect()
    }
    
    /// Get all agent IDs (alias for compatibility)
    pub fn get_all_ids(&self) -> Vec<AgentId> {
        self.agent_ids()
    }
    
    /// Get the number of agents
    pub fn len(&self) -> usize {
        self.agents.len()
    }
    
    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
    
    /// Execute a step for all agents
    pub fn step_all(&mut self, current_time: SimTime, time_step: f64) -> Result<()> {
        for (agent_id, agent) in &mut self.agents {
            let mut ctx = AgentContext::new(*agent_id, current_time, time_step);
            agent.step(&mut ctx)?;
        }
        Ok(())
    }
    
    /// Initialize all agents
    pub fn initialize_all(&mut self, current_time: SimTime) -> Result<()> {
        for (agent_id, agent) in &mut self.agents {
            let ctx = AgentContext::new(*agent_id, current_time, 0.0);
            agent.initialize(&ctx)?;
        }
        Ok(())
    }
    
    /// Finalize all agents
    pub fn finalize_all(&mut self, current_time: SimTime) -> Result<()> {
        for (agent_id, agent) in &mut self.agents {
            let ctx = AgentContext::new(*agent_id, current_time, 0.0);
            agent.finalize(&ctx)?;
        }
        Ok(())
    }
}

/// Agent factory for creating agents of different types
pub struct AgentFactory;

impl AgentFactory {
    /// Create a basic random walk agent
    pub fn create_random_walker<S: AgentState + Default>(speed: f64) -> Box<dyn Agent<State = S>> {
        Box::new(BasicAgent::new(S::default(), StandardBehavior::RandomWalk { speed }))
    }
    
    /// Create a target-following agent
    pub fn create_follower<S: AgentState + Default>(target: VecN, speed: f64) -> Box<dyn Agent<State = S>> {
        Box::new(BasicAgent::new(S::default(), StandardBehavior::FollowTarget { target, speed }))
    }
    
    /// Create a stationary agent
    pub fn create_stationary<S: AgentState + Default>() -> Box<dyn Agent<State = S>> {
        Box::new(BasicAgent::new(S::default(), StandardBehavior::Stationary))
    }
}

/// Basic agent implementation
#[derive(Debug)]
pub struct BasicAgent<S: AgentState> {
    /// The unique identifier of the agent
    id: AgentId,
    /// The state of the agent
    state: S,
    /// The behavior of the agent
    behavior: Option<Box<dyn AgentBehavior<S>>>,
}

impl<S: AgentState> BasicAgent<S> {
    /// Create a new basic agent
    pub fn new(state: S, behavior: StandardBehavior) -> Self {
        Self {
            id: AgentId::new(),
            state,
            behavior: Some(Box::new(behavior)),
        }
    }
}

impl<S: AgentState + Debug> Agent for BasicAgent<S> {
    type State = S;
    
    fn id(&self) -> AgentId {
        self.id
    }
    
    fn state(&self) -> &Self::State {
        &self.state
    }
    
    fn state_mut(&mut self) -> &mut Self::State {
        &mut self.state
    }
    
    fn initialize(&mut self, _ctx: &AgentContext<Self::State>) -> Result<()> {
        Ok(())
    }
    
    fn step(&mut self, ctx: &mut AgentContext<Self::State>) -> Result<()> {
        if let Some(behavior) = &self.behavior {
            behavior.execute(&mut self.state, ctx)?;
        }
        Ok(())
    }
    
    fn handle_message(&mut self, _message: AgentMessage, _ctx: &AgentContext<Self::State>) -> Result<()> {
        Ok(())
    }
    
    fn finalize(&mut self, _ctx: &AgentContext<Self::State>) -> Result<()> {
        Ok(())
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Trait for agent state
pub trait AgentState: Send + Sync + Debug + 'static {
    /// Get the position of the agent
    fn position(&self) -> Option<VecN>;
    
    /// Set the position of the agent
    fn set_position(&mut self, position: VecN);
    
    /// Get custom properties as a map
    fn properties(&self) -> HashMap<String, Value>;
    
    /// Set a custom property
    fn set_property(&mut self, key: String, value: Value);
}

/// Default agent state implementation
#[derive(Clone, Debug, Default)]
pub struct DefaultAgentState {
    /// The position of the agent in space
    pub position: Option<VecN>,
    /// Custom properties associated with the agent
    pub properties: HashMap<String, Value>,
}

impl AgentState for DefaultAgentState {
    fn position(&self) -> Option<VecN> {
        self.position.clone()
    }
    
    fn set_position(&mut self, position: VecN) {
        self.position = Some(position);
    }
    
    fn properties(&self) -> HashMap<String, Value> {
        self.properties.clone()
    }
    
    fn set_property(&mut self, key: String, value: Value) {
        self.properties.insert(key, value);
    }
}

/// Context for agent execution
pub struct AgentContext<S: AgentState> {
    /// The unique identifier of the agent
    pub agent_id: AgentId,
    /// The current simulation time
    pub current_time: SimTime,
    /// The time step for the simulation
    pub time_step: f64,
    /// Shared state accessible to the agent
    pub shared_state: Option<S>,
    /// Messages received by the agent
    pub messages: Vec<AgentMessage>,
}

impl<S: AgentState> AgentContext<S> {
    /// Create a new agent context
    pub fn new(agent_id: AgentId, current_time: SimTime, time_step: f64) -> Self {
        Self {
            agent_id,
            current_time,
            time_step,
            shared_state: None,
            messages: Vec::new(),
        }
    }
    
    /// Add a message to the context
    pub fn add_message(&mut self, message: AgentMessage) {
        self.messages.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_agent_id() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        
        assert_ne!(id1, id2);
        
        let bytes = id1.as_bytes();
        let id3 = AgentId::from_bytes(bytes);
        assert_eq!(id1, id3);
    }
    
    #[test]
    fn test_agent_set() {
        let mut agent_set = AgentSet::<DefaultAgentState>::new();
        
        let agent1 = AgentFactory::create_random_walker::<DefaultAgentState>(1.0);
        let agent2 = AgentFactory::create_stationary::<DefaultAgentState>();
        
        let id1 = agent1.id();
        let id2 = agent2.id();
        
        agent_set.add_agent(agent1);
        agent_set.add_agent(agent2);
        
        assert_eq!(agent_set.len(), 2);
        assert!(agent_set.get_agent(&id1).is_some());
        assert!(agent_set.get_agent(&id2).is_some());
        
        let removed = agent_set.remove_agent(id1);
        assert!(removed.is_some());
        assert_eq!(agent_set.len(), 1);
    }
    
    #[test]
    fn test_agent_state() {
        let mut state = DefaultAgentState::default();
        
        assert!(state.position().is_none());
        
        state.set_position(VecN::new(1.0, 2.0, 0.0));
        assert!(state.position().is_some());
        
        state.set_property("health".to_string(), serde_json::json!(100));
        let properties = state.properties();
        assert_eq!(properties.get("health"), Some(&serde_json::json!(100)));
    }
} 