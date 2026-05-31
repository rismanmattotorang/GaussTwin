//! GaussTwin Finite State Machine and System Dynamics
//!
//! A comprehensive library for finite state machines and system dynamics modeling with features including:
//! - Hierarchical state machines
//! - Guard conditions and actions
//! - System dynamics modeling
//! - State history tracking
//! - Visualization support
//! - Metrics collection
//!
//! # Features
//! - Finite state machines with guards and actions
//! - System dynamics with flows and stocks
//! - State history and metrics
//! - Visualization (DOT and Mermaid)
//! - Observable state changes
//!
//! # Examples
//! ```no_run
//! use gausstwin_fsm::{FiniteStateMachine, State, Signal, StateMachine};
//!
//! fn example() -> Result<(), FsmError> {
//!     let initial = State {
//!         id: 0,
//!         name: "initial".into(),
//!         entry_actions: vec![],
//!         exit_actions: vec![],
//!         data: Arc::new(()),
//!     };
//!     
//!     let mut fsm = FiniteStateMachine::new(initial, ());
//!     Ok(())
//! }
//! ```

use std::any::Any;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Comprehensive error type for FSM operations
#[derive(Debug, Error)]
pub enum FsmError {
    /// Errors related to state management
    #[error("State error: {kind:?}")]
    State {
        kind: StateErrorKind,
        message: String,
        state_id: Option<StateId>,
    },

    /// Errors related to transitions
    #[error("Transition error: {kind:?}")]
    Transition {
        kind: TransitionErrorKind,
        message: String,
        transition_id: Option<TransitionId>,
    },

    /// Errors related to signal handling
    #[error("Signal error: {kind:?}")]
    Signal {
        kind: SignalErrorKind,
        message: String,
    },

    /// Errors related to system dynamics
    #[error("Dynamics error: {kind:?}")]
    Dynamics {
        kind: DynamicsErrorKind,
        message: String,
    },

    /// Internal errors
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Types of state-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateErrorKind {
    /// State not found
    NotFound,
    /// Invalid state data
    InvalidData,
    /// State already exists
    AlreadyExists,
    /// Invalid state operation
    InvalidOperation,
}

/// Types of transition-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionErrorKind {
    /// Transition not found
    NotFound,
    /// Invalid transition
    Invalid,
    /// Guard condition failed
    GuardFailed,
    /// Action failed
    ActionFailed,
    /// Timeout occurred
    Timeout,
}

/// Types of signal-related errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalErrorKind {
    /// Invalid signal format
    InvalidFormat,
    /// Signal handling failed
    HandlingFailed,
    /// Signal timeout
    Timeout,
}

/// Types of system dynamics errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicsErrorKind {
    /// Variable not found
    VariableNotFound,
    /// Invalid flow
    InvalidFlow,
    /// Numerical error
    NumericalError,
    /// Integration error
    IntegrationError,
}

impl FsmError {
    /// Create a new state error
    pub fn state_error(
        kind: StateErrorKind,
        message: impl Into<String>,
        state_id: Option<StateId>,
    ) -> Self {
        FsmError::State {
            kind,
            message: message.into(),
            state_id,
        }
    }

    /// Create a new transition error
    pub fn transition_error(
        kind: TransitionErrorKind,
        message: impl Into<String>,
        transition_id: Option<TransitionId>,
    ) -> Self {
        FsmError::Transition {
            kind,
            message: message.into(),
            transition_id,
        }
    }

    /// Create a new signal error
    pub fn signal_error(kind: SignalErrorKind, message: impl Into<String>) -> Self {
        FsmError::Signal {
            kind,
            message: message.into(),
        }
    }

    /// Create a new dynamics error
    pub fn dynamics_error(kind: DynamicsErrorKind, message: impl Into<String>) -> Self {
        FsmError::Dynamics {
            kind,
            message: message.into(),
        }
    }
}

// Core Types
pub type StateId = u64;
pub type TransitionId = u64;
pub type GuardFn = Arc<dyn Fn(&Signal) -> Result<bool, FsmError> + Send + Sync>;
pub type ActionFn = Arc<dyn Fn(&mut dyn Any) -> Result<(), FsmError> + Send + Sync>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Signal {
    pub name: String,
    pub payload: Vec<u8>,
    pub timestamp: u64,
    pub source: Option<StateId>,
}

pub struct Transition {
    pub id: TransitionId,
    pub from: StateId,
    pub to: StateId,
    pub guard: GuardFn,
    pub action: ActionFn,
    pub priority: i32,
    pub timeout: Option<Duration>,
}

#[derive(Clone)]
pub struct State {
    pub id: StateId,
    pub name: String,
    pub entry_actions: Vec<ActionFn>,
    pub exit_actions: Vec<ActionFn>,
    pub data: Arc<dyn Any + Send + Sync>,
    pub parent: Option<StateId>,
    pub children: Vec<StateId>,
    pub is_composite: bool,
    pub initial_substate: Option<StateId>,
}

impl State {
    /// Create a simple state
    pub fn simple(id: StateId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            entry_actions: vec![],
            exit_actions: vec![],
            data: Arc::new(()),
            parent: None,
            children: vec![],
            is_composite: false,
            initial_substate: None,
        }
    }

    /// Create a composite state (can contain substates)
    pub fn composite(id: StateId, name: impl Into<String>, initial_substate: StateId) -> Self {
        Self {
            id,
            name: name.into(),
            entry_actions: vec![],
            exit_actions: vec![],
            data: Arc::new(()),
            parent: None,
            children: vec![],
            is_composite: true,
            initial_substate: Some(initial_substate),
        }
    }

    /// Add a child state (for hierarchical FSM)
    pub fn with_child(mut self, child_id: StateId) -> Self {
        self.children.push(child_id);
        self
    }

    /// Set parent state
    pub fn with_parent(mut self, parent_id: StateId) -> Self {
        self.parent = Some(parent_id);
        self
    }

    /// Add entry action
    pub fn with_entry_action(mut self, action: ActionFn) -> Self {
        self.entry_actions.push(action);
        self
    }

    /// Add exit action
    pub fn with_exit_action(mut self, action: ActionFn) -> Self {
        self.exit_actions.push(action);
        self
    }
}

// Core Traits
pub trait StateMachine: Send + Sync {
    type Context: Send + Sync;

    /// Get the current state ID
    fn current(&self) -> StateId;

    /// Attempt to transition based on an input signal
    fn transition(&mut self, input: &Signal, ctx: &mut Self::Context) -> Result<StateId, FsmError>;

    /// Add a new state to the machine
    fn add_state(&mut self, state: State) -> Result<(), FsmError>;

    /// Add a new transition to the machine
    fn add_transition(&mut self, transition: Transition) -> Result<(), FsmError>;

    /// Remove a state from the machine
    fn remove_state(&mut self, id: StateId) -> Result<bool, FsmError>;

    /// Remove a transition from the machine
    fn remove_transition(&mut self, id: TransitionId) -> Result<bool, FsmError>;
}

pub trait SystemDynamics: Send + Sync {
    /// Update the system state
    fn update(&mut self, dt: f64) -> Result<(), FsmError>;

    /// Add a new variable to the system
    fn add_variable(&mut self, name: &str, initial: f64) -> Result<(), FsmError>;

    /// Set a variable's value
    fn set_variable(&mut self, name: &str, value: f64) -> Result<(), FsmError>;

    /// Get a variable's current value
    fn get_variable(&self, name: &str) -> Result<f64, FsmError>;

    /// Add a flow between variables
    fn add_flow(
        &mut self,
        from: &str,
        to: &str,
        rate: Box<dyn Fn(f64) -> f64 + Send + Sync>,
    ) -> Result<(), FsmError>;
}

// Implementation
pub struct FiniteStateMachine<C> {
    states: HashMap<StateId, State>,
    transitions: HashMap<TransitionId, Transition>,
    current_state: StateId,
    context: C,
    history: VecDeque<(StateId, u64)>,
    observers: broadcast::Sender<StateId>,
    metrics: Arc<DashMap<String, f64>>,
    state_stack: Vec<StateId>, // For hierarchical FSM
}

impl<C: Send + Sync + 'static> FiniteStateMachine<C> {
    pub fn new(initial_state: State, context: C) -> Self {
        let mut states = HashMap::new();
        let initial_id = initial_state.id;
        states.insert(initial_id, initial_state);

        let (tx, _rx) = broadcast::channel(16);

        Self {
            states,
            transitions: HashMap::new(),
            current_state: initial_id,
            context,
            history: VecDeque::new(),
            observers: tx,
            metrics: Arc::new(DashMap::new()),
            state_stack: vec![initial_id],
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StateId> {
        self.observers.subscribe()
    }

    pub fn get_metrics(&self) -> Arc<DashMap<String, f64>> {
        self.metrics.clone()
    }

    pub fn get_history(&self) -> Vec<(StateId, u64)> {
        self.history.iter().copied().collect()
    }

    pub fn get_state(&self, id: StateId) -> Option<&State> {
        self.states.get(&id)
    }

    pub fn get_current_state(&self) -> Option<&State> {
        self.states.get(&self.current_state)
    }

    pub fn get_state_stack(&self) -> &[StateId] {
        &self.state_stack
    }

    /// Get all states (for visualization)
    pub fn get_all_states(&self) -> &HashMap<StateId, State> {
        &self.states
    }

    /// Get all transitions (for visualization)
    pub fn get_all_transitions(&self) -> Vec<(TransitionId, StateId, StateId, i32)> {
        self.transitions
            .iter()
            .map(|(id, t)| (*id, t.from, t.to, t.priority))
            .collect()
    }

    /// Check if a state is active in the hierarchy
    pub fn is_state_active(&self, state_id: StateId) -> bool {
        self.state_stack.contains(&state_id)
    }

    /// Enter a composite state
    fn enter_composite_state(&mut self, state_id: StateId, ctx: &mut C) -> Result<(), FsmError> {
        if let Some(state) = self.states.get(&state_id).cloned() {
            if state.is_composite {
                self.state_stack.push(state_id);

                // Enter initial substate if defined
                if let Some(initial_substate) = state.initial_substate {
                    // Execute entry actions
                    for action in &state.entry_actions {
                        (action)(ctx as &mut dyn Any)?;
                    }

                    self.current_state = initial_substate;
                    self.enter_composite_state(initial_substate, ctx)?;
                }
            }
        }
        Ok(())
    }

    /// Exit a composite state and all its substates
    fn exit_composite_state(&mut self, state_id: StateId, ctx: &mut C) -> Result<(), FsmError> {
        // Exit all substates first
        while let Some(&top) = self.state_stack.last() {
            if top == state_id {
                break;
            }

            if let Some(state) = self.states.get(&top).cloned() {
                for action in &state.exit_actions {
                    (action)(ctx as &mut dyn Any)?;
                }
            }
            self.state_stack.pop();
        }

        // Exit the composite state itself
        if let Some(state) = self.states.get(&state_id).cloned() {
            for action in &state.exit_actions {
                (action)(ctx as &mut dyn Any)?;
            }
        }
        self.state_stack.pop();

        Ok(())
    }

    /// Find the Least Common Ancestor (LCA) of two states in the hierarchy
    fn find_lca(&self, state1: StateId, state2: StateId) -> Option<StateId> {
        let mut ancestors1 = HashSet::new();
        let mut current = Some(state1);

        // Collect all ancestors of state1
        while let Some(id) = current {
            ancestors1.insert(id);
            current = self.states.get(&id).and_then(|s| s.parent);
        }

        // Find first common ancestor with state2
        let mut current = Some(state2);
        while let Some(id) = current {
            if ancestors1.contains(&id) {
                return Some(id);
            }
            current = self.states.get(&id).and_then(|s| s.parent);
        }

        None
    }
}

impl<C: Send + Sync + 'static> StateMachine for FiniteStateMachine<C> {
    type Context = C;

    fn current(&self) -> StateId {
        self.current_state
    }

    fn transition(&mut self, input: &Signal, ctx: &mut Self::Context) -> Result<StateId, FsmError> {
        let current = self.current_state;
        let mut valid_transitions: Vec<&Transition> = Vec::new();

        // Check transitions from current state and all parent states
        let mut check_state = Some(current);
        while let Some(state_id) = check_state {
            for t in self.transitions.values() {
                if t.from == state_id {
                    match (t.guard)(input) {
                        Ok(true) => valid_transitions.push(t),
                        Ok(false) => {}
                        Err(err) => {
                            warn!("Guard error on transition {}: {}", t.id, err);
                        }
                    }
                }
            }

            // Check parent state
            check_state = self.states.get(&state_id).and_then(|s| s.parent);
        }

        valid_transitions.sort_by_key(|t| -t.priority);

        if let Some(transition) = valid_transitions.first() {
            let from_state = transition.from;
            let to_state = transition.to;

            // Find least common ancestor for hierarchical transitions
            let lca = self.find_lca(from_state, to_state);

            // Exit from current state up to LCA
            let exit_states: Vec<StateId> = if let Some(lca_state) = lca {
                let mut states_to_exit = vec![];
                let mut check = Some(from_state);
                while let Some(state_id) = check {
                    if state_id == lca_state {
                        break;
                    }
                    states_to_exit.push(state_id);
                    check = self.states.get(&state_id).and_then(|s| s.parent);
                }
                states_to_exit
            } else {
                vec![from_state]
            };

            // Execute exit actions
            for &state_id in &exit_states {
                if let Some(state) = self.states.get(&state_id) {
                    for action in &state.exit_actions {
                        (action)(ctx as &mut dyn Any)?;
                    }
                }
            }

            // Execute transition action
            (transition.action)(ctx as &mut dyn Any)?;

            // Enter new state hierarchy
            let mut entry_states = vec![];
            let mut check = Some(to_state);
            while let Some(state_id) = check {
                entry_states.push(state_id);
                check = self.states.get(&state_id).and_then(|s| s.parent);
                if let Some(lca_state) = lca {
                    if check == Some(lca_state) {
                        break;
                    }
                }
            }
            entry_states.reverse();

            // Execute entry actions
            for &state_id in &entry_states {
                if let Some(state) = self.states.get(&state_id) {
                    for action in &state.entry_actions {
                        (action)(ctx as &mut dyn Any)?;
                    }
                }
            }

            // Update state stack for hierarchical FSM
            self.state_stack.clear();
            self.state_stack.extend(&entry_states);

            self.current_state = to_state;
            self.history.push_back((to_state, input.timestamp));
            if self.history.len() > 1000 {
                self.history.pop_front();
            }

            // Update metrics
            let new_total = self
                .metrics
                .get("transitions_total")
                .map(|v| *v + 1.0)
                .unwrap_or(1.0);
            self.metrics.insert("transitions_total".into(), new_total);

            // Notify observers
            let _ = self.observers.send(to_state);

            info!(
                "Transitioned from state {} to state {}",
                from_state, to_state
            );
            Ok(to_state)
        } else {
            Ok(current)
        }
    }

    fn add_state(&mut self, state: State) -> Result<(), FsmError> {
        self.states.insert(state.id, state);
        Ok(())
    }

    fn add_transition(&mut self, transition: Transition) -> Result<(), FsmError> {
        self.transitions.insert(transition.id, transition);
        Ok(())
    }

    fn remove_state(&mut self, id: StateId) -> Result<bool, FsmError> {
        Ok(self.states.remove(&id).is_some())
    }

    fn remove_transition(&mut self, id: TransitionId) -> Result<bool, FsmError> {
        Ok(self.transitions.remove(&id).is_some())
    }
}

pub struct SystemDynamicsModel {
    variables: HashMap<String, f64>,
    flows: Vec<(String, String, Box<dyn Fn(f64) -> f64 + Send + Sync>)>,
    time: f64,
    history: Arc<RwLock<Vec<(f64, HashMap<String, f64>)>>>,
    metrics: Arc<DashMap<String, f64>>,
}

impl SystemDynamicsModel {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            flows: Vec::new(),
            time: 0.0,
            history: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(DashMap::new()),
        }
    }

    pub fn get_history(&self) -> Arc<RwLock<Vec<(f64, HashMap<String, f64>)>>> {
        self.history.clone()
    }

    pub fn get_metrics(&self) -> Arc<DashMap<String, f64>> {
        self.metrics.clone()
    }
}

impl SystemDynamics for SystemDynamicsModel {
    fn update(&mut self, dt: f64) -> Result<(), FsmError> {
        let mut changes = HashMap::new();

        // Calculate all flows
        for (from, to, rate) in &self.flows {
            let current_value = self.variables.get(from).copied().unwrap_or(0.0);
            let flow_value = (rate)(current_value) * dt;

            *changes.entry(from.clone()).or_insert(0.0) -= flow_value;
            *changes.entry(to.clone()).or_insert(0.0) += flow_value;
        }

        // Apply changes
        for (var, change) in changes {
            if let Some(value) = self.variables.get_mut(&var) {
                *value += change;
            }
        }

        self.time += dt;

        // Record history
        let snapshot = self.variables.clone();
        self.history.write().push((self.time, snapshot));

        // Update metrics
        let new_updates = self
            .metrics
            .get("updates_total")
            .map(|v| *v + 1.0)
            .unwrap_or(1.0);
        self.metrics.insert("updates_total".into(), new_updates);

        Ok(())
    }

    fn add_variable(&mut self, name: &str, initial: f64) -> Result<(), FsmError> {
        self.variables.insert(name.to_string(), initial);
        Ok(())
    }

    fn set_variable(&mut self, name: &str, value: f64) -> Result<(), FsmError> {
        self.variables.insert(name.to_string(), value);
        Ok(())
    }

    fn get_variable(&self, name: &str) -> Result<f64, FsmError> {
        self.variables.get(name).copied().ok_or_else(|| {
            FsmError::dynamics_error(
                DynamicsErrorKind::VariableNotFound,
                format!("Variable '{}' not found", name),
            )
        })
    }

    fn add_flow(
        &mut self,
        from: &str,
        to: &str,
        rate: Box<dyn Fn(f64) -> f64 + Send + Sync>,
    ) -> Result<(), FsmError> {
        self.flows.push((from.to_string(), to.to_string(), rate));
        Ok(())
    }
}

// Helper functions for visualization
pub mod viz {
    use super::*;
    use std::fmt::Write;

    /// Generate DOT (Graphviz) representation of FSM
    pub fn generate_dot<C: Send + Sync + 'static>(fsm: &FiniteStateMachine<C>) -> String {
        let mut dot = String::from("digraph FSM {\n");
        dot.push_str("    rankdir=LR;\n");
        dot.push_str("    node [shape=circle];\n");
        dot.push_str("    edge [fontsize=10];\n\n");

        let states = fsm.get_all_states();
        let transitions = fsm.get_all_transitions();
        let current_state = fsm.current();

        // Add states
        for (id, state) in states.iter() {
            let shape = if state.is_composite {
                "doublecircle"
            } else {
                "circle"
            };
            let style = if *id == current_state {
                ", style=filled, fillcolor=lightblue"
            } else {
                ""
            };

            let label = if state.is_composite {
                format!("{}\\n[composite]", state.name)
            } else {
                state.name.clone()
            };

            writeln!(
                dot,
                "    {} [label=\"{}\", shape={}{}];",
                id, label, shape, style
            )
            .ok();

            // Add substate connections
            if state.is_composite {
                if let Some(initial) = state.initial_substate {
                    writeln!(
                        dot,
                        "    {} -> {} [style=dashed, label=\"initial\"];",
                        id, initial
                    )
                    .ok();
                }

                for &child in &state.children {
                    writeln!(
                        dot,
                        "    {} -> {} [style=dotted, label=\"contains\", constraint=false];",
                        id, child
                    )
                    .ok();
                }
            }

            // Show parent relationship
            if let Some(parent) = state.parent {
                writeln!(dot, "    {} [label=\"{}\n↑{}\"];", id, label, parent).ok();
            }
        }

        dot.push('\n');

        // Add transitions
        for (trans_id, from, to, priority) in transitions {
            let label = if priority != 0 {
                format!("T{} (p:{})", trans_id, priority)
            } else {
                format!("T{}", trans_id)
            };

            writeln!(dot, "    {} -> {} [label=\"{}\"];", from, to, label).ok();
        }

        // Add initial state indicator
        writeln!(dot, "\n    __start [shape=point];").ok();
        if let Some(initial_id) = states.keys().min() {
            writeln!(dot, "    __start -> {};", initial_id).ok();
        }

        dot.push_str("}\n");
        dot
    }

    /// Generate Mermaid representation of FSM
    pub fn generate_mermaid<C: Send + Sync + 'static>(fsm: &FiniteStateMachine<C>) -> String {
        let mut mermaid = String::from("stateDiagram-v2\n");

        let states = fsm.get_all_states();
        let transitions = fsm.get_all_transitions();
        let current_state = fsm.current();

        // Add start indicator
        if let Some(initial_id) = states.keys().min() {
            writeln!(mermaid, "    [*] --> S{}", initial_id).ok();
        }

        // Add state definitions
        for (id, state) in states.iter() {
            let state_name = format!("S{}", id);

            if state.is_composite {
                writeln!(mermaid, "    state \"{}\" as {} {{", state.name, state_name).ok();

                // Add substates
                if let Some(initial) = state.initial_substate {
                    writeln!(mermaid, "        [*] --> S{}", initial).ok();
                }

                for &child in &state.children {
                    if let Some(child_state) = states.get(&child) {
                        writeln!(
                            mermaid,
                            "        state \"{}\" as S{}",
                            child_state.name, child
                        )
                        .ok();
                    }
                }

                writeln!(mermaid, "    }}").ok();
            } else {
                writeln!(mermaid, "    state \"{}\" as {}", state.name, state_name).ok();
            }

            // Mark current state
            if *id == current_state {
                writeln!(mermaid, "    {} : <<current>>", state_name).ok();
            }

            // Add parent info
            if let Some(parent) = state.parent {
                writeln!(
                    mermaid,
                    "    note right of {}: Parent: S{}",
                    state_name, parent
                )
                .ok();
            }
        }

        mermaid.push('\n');

        // Add transitions
        for (trans_id, from, to, priority) in transitions {
            let label = if priority != 0 {
                format!("T{} (p:{})", trans_id, priority)
            } else {
                format!("T{}", trans_id)
            };

            writeln!(mermaid, "    S{} --> S{} : {}", from, to, label).ok();
        }

        mermaid
    }

    /// Generate SVG representation using DOT
    pub fn generate_svg<C: Send + Sync + 'static>(
        fsm: &FiniteStateMachine<C>,
    ) -> Result<String, FsmError> {
        let dot = generate_dot(fsm);

        // In a real implementation, this would call Graphviz to render SVG
        // For now, return the DOT source wrapped
        Ok(format!("<!-- DOT source:\n{}\n-->", dot))
    }

    /// Generate HTML visualization with interactive elements
    pub fn generate_html<C: Send + Sync + 'static>(fsm: &FiniteStateMachine<C>) -> String {
        let mermaid_src = generate_mermaid(fsm);

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>FSM Visualization</title>
    <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
    <script>
        mermaid.initialize({{ startOnLoad: true, theme: 'default' }});
    </script>
    <style>
        body {{
            font-family: Arial, sans-serif;
            margin: 20px;
            background: #f5f5f5;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #333;
            border-bottom: 2px solid #4CAF50;
            padding-bottom: 10px;
        }}
        .mermaid {{
            text-align: center;
            margin: 20px 0;
        }}
        .info {{
            background: #e8f5e9;
            padding: 15px;
            border-radius: 4px;
            margin-top: 20px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Finite State Machine Visualization</h1>
        <div class="mermaid">
{}
        </div>
        <div class="info">
            <h3>Legend</h3>
            <ul>
                <li><strong>Current State:</strong> Marked with &lt;&lt;current&gt;&gt;</li>
                <li><strong>Composite States:</strong> Contain substates (shown as nested boxes)</li>
                <li><strong>Transitions:</strong> Arrows with labels (T# and priority p:#)</li>
            </ul>
        </div>
    </div>
</body>
</html>"#,
            mermaid_src
        )
    }

    /// Export FSM structure to JSON
    pub fn export_json<C: Send + Sync + 'static>(
        fsm: &FiniteStateMachine<C>,
    ) -> Result<String, FsmError> {
        use serde_json::json;

        let states = fsm.get_all_states();
        let transitions = fsm.get_all_transitions();

        let states_json: Vec<_> = states
            .iter()
            .map(|(id, state)| {
                json!({
                    "id": id,
                    "name": &state.name,
                    "is_composite": state.is_composite,
                    "parent": state.parent,
                    "children": &state.children,
                    "initial_substate": state.initial_substate,
                })
            })
            .collect();

        let transitions_json: Vec<_> = transitions
            .iter()
            .map(|(id, from, to, priority)| {
                json!({
                    "id": id,
                    "from": from,
                    "to": to,
                    "priority": priority,
                })
            })
            .collect();

        let result = json!({
            "current_state": fsm.current(),
            "states": states_json,
            "transitions": transitions_json,
            "state_stack": fsm.get_state_stack(),
        });

        serde_json::to_string_pretty(&result)
            .map_err(|e| FsmError::Internal(format!("JSON serialization error: {}", e)))
    }
}

// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_basic_fsm() {
        let initial = State::simple(0, "idle");
        let mut fsm = FiniteStateMachine::new(initial, ());

        // Add states
        let active = State::simple(1, "active");
        let error = State::simple(2, "error");

        fsm.add_state(active).unwrap();
        fsm.add_state(error).unwrap();

        // Add transitions
        let transition = Transition {
            id: 1,
            from: 0,
            to: 1,
            guard: Arc::new(|signal| Ok(signal.name == "start")),
            action: Arc::new(|_ctx| Ok(())),
            priority: 0,
            timeout: None,
        };

        fsm.add_transition(transition).unwrap();

        // Test transition
        let signal = Signal {
            name: "start".to_string(),
            payload: vec![],
            timestamp: 0,
            source: Some(0),
        };

        let new_state = fsm.transition(&signal, &mut ()).unwrap();
        assert_eq!(new_state, 1);
        assert_eq!(fsm.current(), 1);
    }

    #[test]
    fn test_hierarchical_fsm() {
        // Create parent state
        let parent = State::composite(0, "parent", 1);
        let mut fsm = FiniteStateMachine::new(parent, ());

        // Add child states
        let child1 = State::simple(1, "child1").with_parent(0);
        let child2 = State::simple(2, "child2").with_parent(0);

        fsm.add_state(child1).unwrap();
        fsm.add_state(child2).unwrap();

        // Add transition between children
        let transition = Transition {
            id: 1,
            from: 1,
            to: 2,
            guard: Arc::new(|signal| Ok(signal.name == "next")),
            action: Arc::new(|_ctx| Ok(())),
            priority: 0,
            timeout: None,
        };

        fsm.add_transition(transition).unwrap();

        assert_eq!(fsm.current(), 0);
    }

    #[test]
    fn test_system_dynamics() {
        let mut model = SystemDynamicsModel::new();

        // Add variables
        model.add_variable("stock1", 100.0).unwrap();
        model.add_variable("stock2", 50.0).unwrap();

        // Add flow
        model
            .add_flow("stock1", "stock2", Box::new(|value| value * 0.1))
            .unwrap();

        // Update
        model.update(1.0).unwrap();

        // Check values
        let stock1 = model.get_variable("stock1").unwrap();
        let stock2 = model.get_variable("stock2").unwrap();

        assert!(stock1 < 100.0);
        assert!(stock2 > 50.0);
    }

    #[test]
    fn test_state_history() {
        let initial = State::simple(0, "s0");
        let mut fsm = FiniteStateMachine::new(initial, ());

        fsm.add_state(State::simple(1, "s1")).unwrap();
        fsm.add_state(State::simple(2, "s2")).unwrap();

        // Add transitions
        fsm.add_transition(Transition {
            id: 1,
            from: 0,
            to: 1,
            guard: Arc::new(|_| Ok(true)),
            action: Arc::new(|_| Ok(())),
            priority: 0,
            timeout: None,
        })
        .unwrap();

        fsm.add_transition(Transition {
            id: 2,
            from: 1,
            to: 2,
            guard: Arc::new(|_| Ok(true)),
            action: Arc::new(|_| Ok(())),
            priority: 0,
            timeout: None,
        })
        .unwrap();

        // Perform transitions
        let signal1 = Signal {
            name: "test".to_string(),
            payload: vec![],
            timestamp: 1,
            source: None,
        };

        let signal2 = Signal {
            name: "test".to_string(),
            payload: vec![],
            timestamp: 2,
            source: None,
        };

        fsm.transition(&signal1, &mut ()).unwrap();
        fsm.transition(&signal2, &mut ()).unwrap();

        let history = fsm.get_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].0, 1);
        assert_eq!(history[1].0, 2);
    }

    #[test]
    fn test_visualization_dot() {
        let initial = State::simple(0, "idle");
        let mut fsm = FiniteStateMachine::new(initial, ());

        fsm.add_state(State::simple(1, "active")).unwrap();

        fsm.add_transition(Transition {
            id: 1,
            from: 0,
            to: 1,
            guard: Arc::new(|_| Ok(true)),
            action: Arc::new(|_| Ok(())),
            priority: 5,
            timeout: None,
        })
        .unwrap();

        let dot = viz::generate_dot(&fsm);
        assert!(dot.contains("digraph FSM"));
        assert!(dot.contains("idle"));
        assert!(dot.contains("active"));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_visualization_mermaid() {
        let initial = State::simple(0, "idle");
        let mut fsm = FiniteStateMachine::new(initial, ());

        fsm.add_state(State::simple(1, "active")).unwrap();

        fsm.add_transition(Transition {
            id: 1,
            from: 0,
            to: 1,
            guard: Arc::new(|_| Ok(true)),
            action: Arc::new(|_| Ok(())),
            priority: 0,
            timeout: None,
        })
        .unwrap();

        let mermaid = viz::generate_mermaid(&fsm);
        assert!(mermaid.contains("stateDiagram-v2"));
        assert!(mermaid.contains("idle"));
        assert!(mermaid.contains("active"));
    }

    #[test]
    fn test_composite_state_visualization() {
        let parent = State::composite(0, "parent", 1);
        let mut fsm = FiniteStateMachine::new(parent, ());

        fsm.add_state(State::simple(1, "child1").with_parent(0))
            .unwrap();
        fsm.add_state(State::simple(2, "child2").with_parent(0))
            .unwrap();

        let dot = viz::generate_dot(&fsm);
        assert!(dot.contains("composite"));
        assert!(dot.contains("child1"));
        assert!(dot.contains("child2"));
    }

    #[test]
    fn test_json_export() {
        let initial = State::simple(0, "idle");
        let mut fsm = FiniteStateMachine::new(initial, ());

        fsm.add_state(State::simple(1, "active")).unwrap();

        let json = viz::export_json(&fsm).unwrap();
        assert!(json.contains("\"id\": 0"));
        assert!(json.contains("\"name\": \"idle\""));
        assert!(json.contains("\"current_state\""));
    }

    #[test]
    fn test_metrics() {
        let initial = State::simple(0, "s0");
        let mut fsm = FiniteStateMachine::new(initial, ());

        fsm.add_state(State::simple(1, "s1")).unwrap();

        fsm.add_transition(Transition {
            id: 1,
            from: 0,
            to: 1,
            guard: Arc::new(|_| Ok(true)),
            action: Arc::new(|_| Ok(())),
            priority: 0,
            timeout: None,
        })
        .unwrap();

        let signal = Signal {
            name: "test".to_string(),
            payload: vec![],
            timestamp: 0,
            source: None,
        };

        fsm.transition(&signal, &mut ()).unwrap();

        let metrics = fsm.get_metrics();
        assert_eq!(*metrics.get("transitions_total").unwrap(), 1.0);
    }
}
