//! Scheduling algorithms for GaussTwin agent execution
//!
//! This module provides different strategies for determining the order
//! in which agents are executed during simulation steps.

use crate::{
    agent::{AgentId, AgentState},
    error::Result,
    time::SimTime,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};

/// Different types of schedulers available
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SchedulerKind {
    /// Execute agents in random order (shuffled each step)
    Random,
    /// Execute agents in fixed sequential order
    Sequential,
    /// Execute agents in parallel (when possible)
    Parallel,
    /// Execute agents based on priority queue
    Priority,
    /// Execute agents based on event scheduling
    EventBased,
    /// Custom scheduler with user-defined logic
    Custom(String),
}

/// Core trait for scheduling agents
pub trait Scheduler<S: AgentState>: Send + Sync {
    /// Initialize the scheduler with available agents
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()>;

    /// Add a new agent to the scheduler
    fn add_agent(&mut self, agent_id: AgentId) -> Result<()>;

    /// Remove an agent from the scheduler
    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()>;

    /// Get the next batch of agents to execute
    fn next_batch(&mut self, current_time: SimTime) -> Result<Vec<AgentId>>;

    /// Check if there are more agents to execute in this step
    fn has_next(&self) -> bool;

    /// Reset the scheduler for the next simulation step
    fn reset_step(&mut self) -> Result<()>;

    /// Get scheduler statistics
    fn stats(&self) -> SchedulerStats;

    /// Get scheduler type
    fn kind(&self) -> SchedulerKind;
}

/// Statistics about scheduler performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerStats {
    /// Total number of scheduling decisions made
    pub decisions_made: u64,
    /// Total time spent scheduling (in simulation units)
    pub total_scheduling_time: f64,
    /// Average batch size
    pub average_batch_size: f64,
    /// Number of agents currently managed
    pub active_agents: usize,
}

/// Random scheduler that shuffles agents each step
#[derive(Debug)]
pub struct RandomScheduler {
    agents: Vec<AgentId>,
    current_index: usize,
    rng: StdRng,
    stats: SchedulerStats,
}

impl RandomScheduler {
    /// Create a new random scheduler with given seed
    pub fn new(seed: u64) -> Self {
        Self {
            agents: Vec::new(),
            current_index: 0,
            rng: StdRng::seed_from_u64(seed),
            stats: SchedulerStats::default(),
        }
    }

    /// Create a new random scheduler with random seed
    pub fn new_random() -> Self {
        Self::new(rand::random())
    }

    /// Shuffle the agent list
    fn shuffle(&mut self) {
        use rand::seq::SliceRandom;
        self.agents.shuffle(&mut self.rng);
    }
}

impl<S: AgentState> Scheduler<S> for RandomScheduler {
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()> {
        self.agents = agents.to_vec();
        self.shuffle();
        self.current_index = 0;
        self.stats.active_agents = self.agents.len();
        Ok(())
    }

    fn add_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if !self.agents.contains(&agent_id) {
            self.agents.push(agent_id);
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(pos) = self.agents.iter().position(|&id| id == agent_id) {
            self.agents.remove(pos);
            if pos < self.current_index {
                self.current_index = self.current_index.saturating_sub(1);
            }
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn next_batch(&mut self, _current_time: SimTime) -> Result<Vec<AgentId>> {
        if self.current_index < self.agents.len() {
            let agent = self.agents[self.current_index];
            self.current_index += 1;
            self.stats.decisions_made += 1;
            Ok(vec![agent])
        } else {
            Ok(vec![])
        }
    }

    fn has_next(&self) -> bool {
        self.current_index < self.agents.len()
    }

    fn reset_step(&mut self) -> Result<()> {
        self.shuffle();
        self.current_index = 0;
        Ok(())
    }

    fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.average_batch_size = if stats.decisions_made > 0 {
            1.0 // Random scheduler always returns single agents
        } else {
            0.0
        };
        stats
    }

    fn kind(&self) -> SchedulerKind {
        SchedulerKind::Random
    }
}

/// Sequential scheduler that executes agents in fixed order
#[derive(Debug)]
pub struct SequentialScheduler {
    agents: Vec<AgentId>,
    current_index: usize,
    stats: SchedulerStats,
}

impl SequentialScheduler {
    /// Create a new sequential scheduler
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            current_index: 0,
            stats: SchedulerStats::default(),
        }
    }
}

impl Default for SequentialScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: AgentState> Scheduler<S> for SequentialScheduler {
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()> {
        self.agents = agents.to_vec();
        self.current_index = 0;
        self.stats.active_agents = self.agents.len();
        Ok(())
    }

    fn add_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if !self.agents.contains(&agent_id) {
            self.agents.push(agent_id);
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(pos) = self.agents.iter().position(|&id| id == agent_id) {
            self.agents.remove(pos);
            if pos < self.current_index {
                self.current_index = self.current_index.saturating_sub(1);
            }
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn next_batch(&mut self, _current_time: SimTime) -> Result<Vec<AgentId>> {
        if self.current_index < self.agents.len() {
            let agent = self.agents[self.current_index];
            self.current_index += 1;
            self.stats.decisions_made += 1;
            Ok(vec![agent])
        } else {
            Ok(vec![])
        }
    }

    fn has_next(&self) -> bool {
        self.current_index < self.agents.len()
    }

    fn reset_step(&mut self) -> Result<()> {
        self.current_index = 0;
        Ok(())
    }

    fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.average_batch_size = if stats.decisions_made > 0 {
            1.0 // Sequential scheduler always returns single agents
        } else {
            0.0
        };
        stats
    }

    fn kind(&self) -> SchedulerKind {
        SchedulerKind::Sequential
    }
}

/// Parallel scheduler that can execute multiple agents simultaneously
#[derive(Debug)]
pub struct ParallelScheduler {
    agents: Vec<AgentId>,
    batch_size: usize,
    current_index: usize,
    stats: SchedulerStats,
}

impl ParallelScheduler {
    /// Create a new parallel scheduler with specified batch size
    pub fn new(batch_size: usize) -> Self {
        Self {
            agents: Vec::new(),
            batch_size: batch_size.max(1),
            current_index: 0,
            stats: SchedulerStats::default(),
        }
    }

    /// Create a parallel scheduler with automatic batch sizing
    pub fn auto_batch() -> Self {
        let batch_size = num_cpus::get().max(1);
        Self::new(batch_size)
    }
}

impl<S: AgentState> Scheduler<S> for ParallelScheduler {
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()> {
        self.agents = agents.to_vec();
        self.current_index = 0;
        self.stats.active_agents = self.agents.len();
        Ok(())
    }

    fn add_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if !self.agents.contains(&agent_id) {
            self.agents.push(agent_id);
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if let Some(pos) = self.agents.iter().position(|&id| id == agent_id) {
            self.agents.remove(pos);
            if pos < self.current_index {
                self.current_index = self.current_index.saturating_sub(1);
            }
            self.stats.active_agents = self.agents.len();
        }
        Ok(())
    }

    fn next_batch(&mut self, _current_time: SimTime) -> Result<Vec<AgentId>> {
        let end_index = (self.current_index + self.batch_size).min(self.agents.len());
        if self.current_index < end_index {
            let batch = self.agents[self.current_index..end_index].to_vec();
            self.current_index = end_index;
            self.stats.decisions_made += 1;
            Ok(batch)
        } else {
            Ok(vec![])
        }
    }

    fn has_next(&self) -> bool {
        self.current_index < self.agents.len()
    }

    fn reset_step(&mut self) -> Result<()> {
        self.current_index = 0;
        Ok(())
    }

    fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.average_batch_size = if stats.decisions_made > 0 {
            self.batch_size as f64
        } else {
            0.0
        };
        stats
    }

    fn kind(&self) -> SchedulerKind {
        SchedulerKind::Parallel
    }
}

/// Priority-based scheduler with agent priorities
#[derive(Debug)]
pub struct PriorityScheduler {
    priority_queue: BinaryHeap<PriorityItem>,
    agent_priorities: HashMap<AgentId, i32>,
    stats: SchedulerStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PriorityItem {
    agent_id: AgentId,
    priority: i32,
}

impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PriorityScheduler {
    /// Create a new priority scheduler
    pub fn new() -> Self {
        Self {
            priority_queue: BinaryHeap::new(),
            agent_priorities: HashMap::new(),
            stats: SchedulerStats::default(),
        }
    }

    /// Set priority for an agent (higher values = higher priority)
    pub fn set_priority(&mut self, agent_id: AgentId, priority: i32) {
        self.agent_priorities.insert(agent_id, priority);
    }

    /// Get priority for an agent
    pub fn get_priority(&self, agent_id: AgentId) -> i32 {
        self.agent_priorities.get(&agent_id).copied().unwrap_or(0)
    }

    /// Rebuild the priority queue
    fn rebuild_queue(&mut self) {
        self.priority_queue.clear();
        for (&agent_id, &priority) in &self.agent_priorities {
            self.priority_queue
                .push(PriorityItem { agent_id, priority });
        }
    }
}

impl Default for PriorityScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: AgentState> Scheduler<S> for PriorityScheduler {
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()> {
        self.agent_priorities.clear();
        for &agent_id in agents {
            self.agent_priorities.insert(agent_id, 0);
        }
        self.rebuild_queue();
        self.stats.active_agents = agents.len();
        Ok(())
    }

    fn add_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if !self.agent_priorities.contains_key(&agent_id) {
            self.agent_priorities.insert(agent_id, 0);
            self.priority_queue.push(PriorityItem {
                agent_id,
                priority: 0,
            });
            self.stats.active_agents = self.agent_priorities.len();
        }
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        if self.agent_priorities.remove(&agent_id).is_some() {
            self.rebuild_queue();
            self.stats.active_agents = self.agent_priorities.len();
        }
        Ok(())
    }

    fn next_batch(&mut self, _current_time: SimTime) -> Result<Vec<AgentId>> {
        loop {
            if let Some(item) = self.priority_queue.pop() {
                // Check if agent still exists (might have been removed)
                if self.agent_priorities.contains_key(&item.agent_id) {
                    self.stats.decisions_made += 1;
                    return Ok(vec![item.agent_id]);
                }
                // Agent was removed, continue to next one
            } else {
                return Ok(vec![]);
            }
        }
    }

    fn has_next(&self) -> bool {
        !self.priority_queue.is_empty()
    }

    fn reset_step(&mut self) -> Result<()> {
        self.rebuild_queue();
        Ok(())
    }

    fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.average_batch_size = if stats.decisions_made > 0 {
            1.0 // Priority scheduler returns single agents
        } else {
            0.0
        };
        stats
    }

    fn kind(&self) -> SchedulerKind {
        SchedulerKind::Priority
    }
}

/// Event-based scheduler for discrete event simulation
#[derive(Debug)]
pub struct EventScheduler {
    event_queue: VecDeque<ScheduledEvent>,
    stats: SchedulerStats,
}

#[derive(Debug, Clone)]
struct ScheduledEvent {
    agent_id: AgentId,
    scheduled_time: SimTime,
}

impl EventScheduler {
    /// Create a new event scheduler
    pub fn new() -> Self {
        Self {
            event_queue: VecDeque::new(),
            stats: SchedulerStats::default(),
        }
    }

    /// Schedule an agent to execute at a specific time
    pub fn schedule_at(&mut self, agent_id: AgentId, time: SimTime) {
        let event = ScheduledEvent {
            agent_id,
            scheduled_time: time,
        };

        // Insert in sorted order (simple implementation)
        let mut inserted = false;
        for (i, existing_event) in self.event_queue.iter().enumerate() {
            if time <= existing_event.scheduled_time {
                self.event_queue.insert(i, event.clone());
                inserted = true;
                break;
            }
        }

        if !inserted {
            self.event_queue.push_back(event);
        }
    }
}

impl Default for EventScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: AgentState> Scheduler<S> for EventScheduler {
    fn initialize(&mut self, agents: &[AgentId]) -> Result<()> {
        self.event_queue.clear();
        // Schedule all agents for immediate execution
        for &agent_id in agents {
            self.schedule_at(agent_id, SimTime::zero());
        }
        self.stats.active_agents = agents.len();
        Ok(())
    }

    fn add_agent(&mut self, agent_id: AgentId) -> Result<()> {
        self.schedule_at(agent_id, SimTime::zero());
        self.stats.active_agents += 1;
        Ok(())
    }

    fn remove_agent(&mut self, agent_id: AgentId) -> Result<()> {
        self.event_queue.retain(|event| event.agent_id != agent_id);
        self.stats.active_agents = self.stats.active_agents.saturating_sub(1);
        Ok(())
    }

    fn next_batch(&mut self, current_time: SimTime) -> Result<Vec<AgentId>> {
        if let Some(event) = self.event_queue.front() {
            if event.scheduled_time <= current_time {
                let event = self.event_queue.pop_front().unwrap();
                self.stats.decisions_made += 1;
                Ok(vec![event.agent_id])
            } else {
                Ok(vec![])
            }
        } else {
            Ok(vec![])
        }
    }

    fn has_next(&self) -> bool {
        !self.event_queue.is_empty()
    }

    fn reset_step(&mut self) -> Result<()> {
        // Event scheduler doesn't need step reset
        Ok(())
    }

    fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.clone();
        stats.average_batch_size = if stats.decisions_made > 0 {
            1.0 // Event scheduler returns single agents
        } else {
            0.0
        };
        stats
    }

    fn kind(&self) -> SchedulerKind {
        SchedulerKind::EventBased
    }
}

// Provide a fallback for num_cpus when the feature is not available
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
    struct TestState;

    impl crate::agent::AgentState for TestState {
        fn position(&self) -> Option<crate::space::VecN> {
            None
        }

        fn set_position(&mut self, _position: crate::space::VecN) {
            // Do nothing for test
        }

        fn properties(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }

        fn set_property(&mut self, _key: String, _value: serde_json::Value) {
            // Do nothing for test
        }
    }

    #[test]
    fn test_random_scheduler() {
        let mut scheduler = RandomScheduler::new(42);
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];

        <RandomScheduler as Scheduler<TestState>>::initialize(&mut scheduler, &agents).unwrap();
        assert_eq!(
            <RandomScheduler as Scheduler<TestState>>::stats(&scheduler).active_agents,
            3
        );

        let mut executed_agents = Vec::new();
        while <RandomScheduler as Scheduler<TestState>>::has_next(&scheduler) {
            let batch = <RandomScheduler as Scheduler<TestState>>::next_batch(
                &mut scheduler,
                SimTime::zero(),
            )
            .unwrap();
            executed_agents.extend(batch);
        }

        assert_eq!(executed_agents.len(), 3);
    }

    #[test]
    fn test_sequential_scheduler() {
        let mut scheduler = SequentialScheduler::new();
        let agents = vec![AgentId::new(), AgentId::new()];

        <SequentialScheduler as Scheduler<TestState>>::initialize(&mut scheduler, &agents).unwrap();

        let batch1 = <SequentialScheduler as Scheduler<TestState>>::next_batch(
            &mut scheduler,
            SimTime::zero(),
        )
        .unwrap();
        assert_eq!(batch1.len(), 1);
        assert_eq!(batch1[0], agents[0]);

        let batch2 = <SequentialScheduler as Scheduler<TestState>>::next_batch(
            &mut scheduler,
            SimTime::zero(),
        )
        .unwrap();
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0], agents[1]);

        assert!(!<SequentialScheduler as Scheduler<TestState>>::has_next(
            &scheduler
        ));
    }

    #[test]
    fn test_parallel_scheduler() {
        let mut scheduler = ParallelScheduler::new(2);
        let agents = vec![AgentId::new(), AgentId::new(), AgentId::new()];

        <ParallelScheduler as Scheduler<TestState>>::initialize(&mut scheduler, &agents).unwrap();

        let batch1 = <ParallelScheduler as Scheduler<TestState>>::next_batch(
            &mut scheduler,
            SimTime::zero(),
        )
        .unwrap();
        assert_eq!(batch1.len(), 2);

        let batch2 = <ParallelScheduler as Scheduler<TestState>>::next_batch(
            &mut scheduler,
            SimTime::zero(),
        )
        .unwrap();
        assert_eq!(batch2.len(), 1);

        assert!(!<ParallelScheduler as Scheduler<TestState>>::has_next(
            &scheduler
        ));
    }
}
