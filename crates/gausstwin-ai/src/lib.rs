use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

// Public modules
pub mod core;
pub mod evolution;
pub mod llm;
pub mod marl;
// The `ml` module is backed by libtorch (`tch`) and is only compiled when the
// `torch` feature is enabled, keeping the default build free of heavy native deps.
#[cfg(feature = "torch")]
pub mod ml;
pub mod rl;
pub mod utils;

// Re-exports for convenient access
pub use core::{Agent, Metrics};
pub use evolution::{EvolutionConfig, Individual};
pub use llm::LLMConfig;
pub use marl::{MARLAgent, MARLConfig, RewardStrategy, SyncMode};
#[cfg(feature = "torch")]
pub use ml::{Model, ModelConfig, ModelFactory, ModelMetrics, ModelState};
pub use rl::{Environment, Experience, Policy, Trajectory, Value};

// Import missing types
// use llm::LLMModel;
use evolution::Population;
use marl::{LLMState, MARLState};

/// Error types for AI operations
#[derive(Error, Debug)]
pub enum AIError {
    #[error("Model initialization error: {0}")]
    ModelInitError(String),

    #[error("Initialization error: {0}")]
    InitializationError(String),

    #[error("Training error: {0}")]
    TrainingError(String),

    #[error("Inference error: {0}")]
    InferenceError(String),

    #[error("Environment error: {0}")]
    EnvironmentError(String),

    #[error("Agent error: {0}")]
    AgentError(String),

    #[error("LLM error: {0}")]
    LLMError(String),

    #[error("MARL error: {0}")]
    MARLError(String),

    #[error("Evolution error: {0}")]
    EvolutionError(String),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Result type for AI operations
pub type Result<T> = std::result::Result<T, AIError>;

/// Core AI system configuration
pub struct AIConfig {
    /// Number of agents in the system
    pub num_agents: usize,

    /// Learning rate for optimization
    pub learning_rate: f32,

    /// Batch size for training
    pub batch_size: usize,

    /// Maximum number of training steps
    pub max_steps: usize,

    /// Device to run computations on (CPU/GPU)
    pub device: String,

    /// Model configuration (requires the `torch` feature)
    #[cfg(feature = "torch")]
    pub model_config: Option<ModelConfig>,

    /// LLM configuration
    pub llm_config: Option<LLMConfig>,

    /// MARL configuration
    pub marl_config: Option<MARLConfig>,

    /// Evolution configuration
    pub evolution_config: Option<EvolutionConfig>,
}

/// Core AI system that manages agents and training
pub struct AISystem {
    /// System configuration
    config: AIConfig,

    /// Shared state between agents
    state: Arc<RwLock<SharedState>>,

    /// Active agents
    agents: Vec<Box<dyn Agent>>,

    /// Model factory for creating new models (requires the `torch` feature)
    #[cfg(feature = "torch")]
    model_factory: Arc<ModelFactory>,

    /// LLM engine for natural language tasks
    llm_engine: Option<Arc<crate::llm::LLMModel>>,

    /// MARL coordinator for multi-agent tasks
    marl_coordinator: Option<Arc<MARLAgent>>,

    /// Evolution engine for optimization tasks
    evolution_engine: Option<Arc<Population>>,
}

/// Shared state between agents
#[derive(Debug)]
pub struct SharedState {
    /// Global step counter
    pub step: usize,

    /// Shared memory buffer
    pub memory: Vec<Experience>,

    /// Performance metrics
    pub metrics: Metrics,

    /// Model state if applicable (requires the `torch` feature)
    #[cfg(feature = "torch")]
    pub model_state: Option<ModelState>,

    /// LLM state if applicable
    pub llm_state: Option<LLMState>,

    /// MARL state if applicable
    pub marl_state: Option<MARLState>,
}

impl AISystem {
    /// Create a new AI system
    pub fn new(config: AIConfig) -> Result<Self> {
        let state = Arc::new(RwLock::new(SharedState {
            step: 0,
            memory: Vec::new(),
            metrics: Metrics::default(),
            #[cfg(feature = "torch")]
            model_state: None,
            llm_state: None,
            marl_state: None,
        }));

        #[cfg(feature = "torch")]
        let model_factory = Arc::new(ModelFactory::default());

        // Initialize optional components based on configuration
        let llm_engine = if let Some(llm_config) = &config.llm_config {
            Some(Arc::new(crate::llm::LLMModel::new(llm_config.clone())?))
        } else {
            None
        };

        let marl_coordinator = if let Some(marl_config) = &config.marl_config {
            Some(Arc::new(MARLAgent::new(0, marl_config.clone())))
        } else {
            None
        };

        let evolution_engine = if let Some(evolution_config) = &config.evolution_config {
            Some(Arc::new(Population::new()))
        } else {
            None
        };

        Ok(Self {
            config,
            state,
            agents: Vec::new(),
            #[cfg(feature = "torch")]
            model_factory,
            llm_engine,
            marl_coordinator,
            evolution_engine,
        })
    }

    /// Add an agent to the system
    pub fn add_agent(&mut self, agent: Box<dyn Agent>) {
        self.agents.push(agent);
    }

    /// Train the multi-agent system
    pub async fn train(&mut self) -> Result<()> {
        for step in 0..self.config.max_steps {
            // Update global step
            self.state.write().await.step = step;

            // Collect experiences from all agents
            let mut experiences = Vec::new();
            for agent in &self.agents {
                // Get current state
                let state = self.get_state().await?;

                // Select and execute action
                let action = agent
                    .act(&crate::core::State {
                        timestamp: 0,
                        data: state.iter().map(|&x| x as f64).collect(),
                    })
                    .await?;

                // Get reward and next state from environment
                let (reward, next_state, done) = self.step_environment(0).await?;

                // Store experience
                let exp = crate::rl::Experience {
                    state: state.iter().map(|&x| x as f64).collect(),
                    action: action.parameters.iter().map(|&x| x as f64).collect(),
                    reward: reward as f64,
                    next_state: next_state.iter().map(|&x| x as f64).collect(),
                    done,
                };
                experiences.push(exp.clone());
            }

            // Update all agents with their experiences
            for (i, agent) in self.agents.iter_mut().enumerate() {
                if i < experiences.len() {
                    agent.update(&experiences[i]).await?;
                }
            }

            // Store experiences in shared memory
            self.state.write().await.memory.extend(experiences);

            // Update metrics
            self.update_metrics().await?;

            // Update optional components
            if let Some(llm) = &self.llm_engine {
                llm.update().await?;
            }

            if let Some(marl) = &self.marl_coordinator {
                let shared_state = self.state.read().await;
                // TODO: Fix MARL synchronize call - need to convert SharedState to marl::SharedState
                // marl.synchronize(&*shared_state).await?;
            }

            if let Some(evolution) = &self.evolution_engine {
                // TODO: Fix evolution engine - need mutable access or different approach
                // evolution.evolve()?;
            }
        }
        Ok(())
    }

    /// Get current state of the environment
    async fn get_state(&self) -> Result<Vec<f32>> {
        // TODO: Implement state observation
        Ok(vec![0.0; 10])
    }

    /// Execute action in environment and get reward
    async fn step_environment(&self, _action: usize) -> Result<(f32, Vec<f32>, bool)> {
        // TODO: Implement environment stepping
        Ok((0.0, vec![0.0; 10], false))
    }

    /// Update system metrics
    async fn update_metrics(&self) -> Result<()> {
        let mut metrics = Metrics::default();

        // Collect metrics from all agents
        for agent in &self.agents {
            let agent_metrics = agent.get_metrics().await?;
            // TODO: Fix metrics aggregation - current Metrics struct doesn't have these fields
            // metrics.rewards.extend(agent_metrics.rewards);
            // metrics.losses.extend(agent_metrics.losses);
            // metrics.accuracies.extend(agent_metrics.accuracies);
        }

        // Update shared metrics
        self.state.write().await.metrics = metrics;
        Ok(())
    }

    /// Save system state
    pub async fn save(&self, _path: &str) -> Result<()> {
        // TODO: Implement model saving
        Ok(())
    }

    /// Load system state
    pub async fn load(&mut self, _path: &str) -> Result<()> {
        // TODO: Implement model loading
        Ok(())
    }
}
