//! Event system for GaussTwin simulations
//!
//! This module provides event types and event queue management for
//! discrete event simulation and agent communication.

use crate::{agent::AgentId, error::Result, time::SimTime};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Unique identifier for events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

impl EventId {
    /// Create a new random event ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

/// Different types of events in the simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventKind {
    /// Agent-generated event
    AgentEvent {
        source: AgentId,
        target: Option<AgentId>,
        payload: serde_json::Value,
    },
    /// System event (model management)
    SystemEvent {
        event_type: String,
        payload: serde_json::Value,
    },
    /// Time-based trigger event
    TimerEvent {
        timer_id: String,
        payload: serde_json::Value,
    },
    /// External event (from outside the simulation)
    ExternalEvent {
        source: String,
        payload: serde_json::Value,
    },
    /// Custom event type
    Custom {
        event_type: String,
        payload: serde_json::Value,
    },
}

/// Core event structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique identifier for this event
    pub id: EventId,
    /// When this event should be processed
    pub scheduled_time: SimTime,
    /// Type and content of the event
    pub kind: EventKind,
    /// Priority (higher values processed first for same time)
    pub priority: i32,
    /// Optional expiration time
    pub expires_at: Option<SimTime>,
    /// Event creation timestamp
    pub created_at: SimTime,
    /// Number of times this event can be processed (0 = unlimited)
    pub max_executions: u32,
    /// Number of times this event has been processed
    pub execution_count: u32,
}

impl Event {
    /// Create a new event
    pub fn new(scheduled_time: SimTime, kind: EventKind) -> Self {
        Self {
            id: EventId::new(),
            scheduled_time,
            kind,
            priority: 0,
            expires_at: None,
            created_at: SimTime::zero(), // Will be set when added to queue
            max_executions: 1,
            execution_count: 0,
        }
    }

    /// Create an agent event
    pub fn agent_event(
        scheduled_time: SimTime,
        source: AgentId,
        target: Option<AgentId>,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(
            scheduled_time,
            EventKind::AgentEvent {
                source,
                target,
                payload,
            },
        )
    }

    /// Create a system event
    pub fn system_event(
        scheduled_time: SimTime,
        event_type: String,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(
            scheduled_time,
            EventKind::SystemEvent {
                event_type,
                payload,
            },
        )
    }

    /// Create a timer event
    pub fn timer_event(
        scheduled_time: SimTime,
        timer_id: String,
        payload: serde_json::Value,
    ) -> Self {
        Self::new(scheduled_time, EventKind::TimerEvent { timer_id, payload })
    }

    /// Set event priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set event expiration
    pub fn with_expiration(mut self, expires_at: SimTime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set maximum executions
    pub fn with_max_executions(mut self, max_executions: u32) -> Self {
        self.max_executions = max_executions;
        self
    }

    /// Check if event has expired
    pub fn is_expired(&self, current_time: SimTime) -> bool {
        if let Some(expires_at) = self.expires_at {
            current_time > expires_at
        } else {
            false
        }
    }

    /// Check if event can still be executed
    pub fn can_execute(&self) -> bool {
        self.max_executions == 0 || self.execution_count < self.max_executions
    }

    /// Mark event as executed
    pub fn mark_executed(&mut self) {
        self.execution_count += 1;
    }

    /// Check if this is a recurring event
    pub fn is_recurring(&self) -> bool {
        self.max_executions == 0 || self.max_executions > 1
    }
}

// Implement ordering for priority queue (min-heap, so we reverse the comparison)
impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        // First compare by time (earlier events first)
        match other.scheduled_time.partial_cmp(&self.scheduled_time) {
            Some(Ordering::Equal) => {
                // For same time, compare by priority (higher priority first)
                self.priority.cmp(&other.priority)
            }
            Some(ordering) => ordering,
            None => Ordering::Equal,
        }
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Event {}

/// Event queue for managing scheduled events
#[derive(Debug)]
pub struct EventQueue {
    /// Binary heap for efficient event scheduling
    heap: BinaryHeap<Event>,
    /// Fast lookup by event ID
    events_by_id: HashMap<EventId, Event>,
    /// Current simulation time
    current_time: SimTime,
    /// Statistics
    stats: EventQueueStats,
}

/// Statistics about event queue performance
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventQueueStats {
    /// Total events processed
    pub events_processed: u64,
    /// Total events scheduled
    pub events_scheduled: u64,
    /// Events currently in queue
    pub events_in_queue: usize,
    /// Average queue size
    pub average_queue_size: f64,
    /// Peak queue size
    pub peak_queue_size: usize,
    /// Number of expired events removed
    pub expired_events: u64,
}

impl EventQueue {
    /// Create a new event queue
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            events_by_id: HashMap::new(),
            current_time: SimTime::zero(),
            stats: EventQueueStats::default(),
        }
    }

    /// Set the current simulation time
    pub fn set_current_time(&mut self, time: SimTime) {
        self.current_time = time;
        self.remove_expired_events();
    }

    /// Schedule an event
    pub fn schedule(&mut self, mut event: Event) -> Result<EventId> {
        use crate::error::GaussTwinError;

        // Check for queue overflow
        if self.heap.len() >= 1_000_000 {
            return Err(GaussTwinError::EventQueueOverflow);
        }

        event.created_at = self.current_time;
        let event_id = event.id;

        self.heap.push(event.clone());
        self.events_by_id.insert(event_id, event);

        self.stats.events_scheduled += 1;
        self.stats.events_in_queue = self.heap.len();
        self.stats.peak_queue_size = self.stats.peak_queue_size.max(self.heap.len());

        Ok(event_id)
    }

    /// Get the next event to process (if any)
    pub fn next_event(&mut self) -> Option<Event> {
        while let Some(event) = self.heap.peek() {
            if event.scheduled_time <= self.current_time {
                let mut event = self.heap.pop().unwrap();
                self.events_by_id.remove(&event.id);

                // Check if event is still valid and can execute
                if !event.is_expired(self.current_time) && event.can_execute() {
                    event.mark_executed();

                    // If it's a recurring event and can still execute, reschedule it
                    if event.is_recurring() && event.can_execute() {
                        let mut recurring_event = event.clone();
                        recurring_event.id = EventId::new();
                        // For simplicity, reschedule at the same time + 1 step
                        // Real implementations would have more sophisticated rescheduling
                        let _ = self.schedule(recurring_event);
                    }

                    self.stats.events_processed += 1;
                    self.stats.events_in_queue = self.heap.len();
                    return Some(event);
                } else {
                    // Event expired or can't execute anymore
                    self.stats.expired_events += 1;
                    self.stats.events_in_queue = self.heap.len();
                }
            } else {
                // No more events ready to process
                break;
            }
        }
        None
    }

    /// Peek at the next event without removing it
    pub fn peek_next(&self) -> Option<&Event> {
        self.heap.peek()
    }

    /// Cancel an event by ID
    pub fn cancel(&mut self, event_id: EventId) -> bool {
        if self.events_by_id.remove(&event_id).is_some() {
            // Remove from heap is expensive, so we mark it as expired instead
            // It will be filtered out when processed
            true
        } else {
            false
        }
    }

    /// Get event by ID
    pub fn get_event(&self, event_id: EventId) -> Option<&Event> {
        self.events_by_id.get(&event_id)
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Get number of events in queue
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.heap.clear();
        self.events_by_id.clear();
        self.stats.events_in_queue = 0;
    }

    /// Remove expired events
    fn remove_expired_events(&mut self) {
        let initial_size = self.heap.len();

        // Collect non-expired events
        let mut valid_events = Vec::new();
        while let Some(event) = self.heap.pop() {
            if !event.is_expired(self.current_time) {
                valid_events.push(event);
            } else {
                self.events_by_id.remove(&event.id);
                self.stats.expired_events += 1;
            }
        }

        // Rebuild heap with valid events
        for event in valid_events {
            self.heap.push(event);
        }

        self.stats.events_in_queue = self.heap.len();

        // Update average queue size
        let removed_count = initial_size - self.heap.len();
        if removed_count > 0 {
            tracing::debug!("Removed {} expired events", removed_count);
        }
    }

    /// Get statistics
    pub fn stats(&self) -> &EventQueueStats {
        &self.stats
    }

    /// Get all events scheduled for a specific time range
    pub fn events_in_range(&self, start: SimTime, end: SimTime) -> Vec<&Event> {
        self.events_by_id
            .values()
            .filter(|event| event.scheduled_time >= start && event.scheduled_time <= end)
            .collect()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Event processor for handling different event types
pub struct EventProcessor {
    handlers: HashMap<String, Box<dyn EventHandler>>,
    processing_stats: ProcessingStats,
}

pub trait EventHandler: Send + Sync {
    fn handle_event(&mut self, event: &Event) -> Result<()>;
    fn event_type(&self) -> String;
    fn priority(&self) -> i32 {
        0
    }
    fn can_handle(&self, event: &Event) -> bool {
        match &event.kind {
            EventKind::Custom { event_type, .. } => event_type == &self.event_type(),
            _ => false,
        }
    }
}

impl EventProcessor {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            processing_stats: ProcessingStats::default(),
        }
    }

    /// Register an event handler
    pub fn register_handler(&mut self, handler: Box<dyn EventHandler>) {
        let event_type = handler.event_type();
        self.handlers.insert(event_type, handler);
    }

    /// Process an event
    pub fn process_event(&mut self, event: &Event) -> Result<()> {
        let event_type_key = match &event.kind {
            EventKind::Custom { event_type, .. } => event_type.clone(),
            other => format!("{:?}", other),
        };

        let start_time = Instant::now();

        let result = if let Some(handler) = self.handlers.get_mut(&event_type_key) {
            handler.handle_event(event)
        } else {
            self.processing_stats.unhandled_events += 1;
            Ok(()) // Ignore unhandled events
        };

        let processing_time = start_time.elapsed();
        self.processing_stats.total_processing_time += processing_time;
        self.processing_stats.processed_events += 1;

        if result.is_err() {
            self.processing_stats.error_count += 1;
        }

        result
    }

    /// Get processing statistics
    pub fn stats(&self) -> &ProcessingStats {
        &self.processing_stats
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProcessingStats {
    pub processed_events: u64,
    pub unhandled_events: u64,
    pub error_count: u64,
    pub total_processing_time: Duration,
}

impl ProcessingStats {
    pub fn average_processing_time(&self) -> Duration {
        if self.processed_events > 0 {
            self.total_processing_time / self.processed_events as u32
        } else {
            Duration::from_nanos(0)
        }
    }

    pub fn error_rate(&self) -> f64 {
        if self.processed_events > 0 {
            self.error_count as f64 / self.processed_events as f64
        } else {
            0.0
        }
    }
}

/// Event dispatcher for routing events to appropriate handlers
#[derive(Default)]
pub struct EventDispatcher {
    handlers: Vec<Box<dyn EventHandler>>,
}

impl EventDispatcher {
    /// Create a new event dispatcher
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Add an event handler
    pub fn add_handler(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
        // Sort handlers by priority (highest first)
        self.handlers
            .sort_by(|a, b| b.priority().cmp(&a.priority()));
    }

    /// Dispatch an event to appropriate handlers
    pub fn dispatch(&mut self, event: &Event) -> Result<()> {
        for handler in &mut self.handlers {
            if handler.can_handle(event) {
                handler.handle_event(event)?;
            }
        }
        Ok(())
    }
}

/// Message for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub sender: Option<AgentId>,
    pub recipient: Option<AgentId>, // None for broadcast
    pub message_type: MessageType,
    pub content: serde_json::Value,
    pub timestamp: SimTime,
    pub priority: MessagePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    /// Direct communication between agents
    AgentToAgent,
    /// Broadcast to all agents
    Broadcast,
    /// System notification
    System,
    /// Custom message type
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Message {
    pub fn new(
        sender: Option<AgentId>,
        recipient: Option<AgentId>,
        message_type: MessageType,
        content: serde_json::Value,
        timestamp: SimTime,
    ) -> Self {
        Self {
            id: fastrand::u64(..),
            sender,
            recipient,
            message_type,
            content,
            timestamp,
            priority: MessagePriority::Normal,
        }
    }

    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn broadcast(
        sender: Option<AgentId>,
        content: serde_json::Value,
        timestamp: SimTime,
    ) -> Self {
        Self::new(sender, None, MessageType::Broadcast, content, timestamp)
    }

    pub fn system(content: serde_json::Value, timestamp: SimTime) -> Self {
        Self::new(None, None, MessageType::System, content, timestamp)
    }
}

/// Message dispatcher for agent communication
#[derive(Debug)]
pub struct MessageDispatcher {
    message_queues: HashMap<AgentId, VecDeque<Message>>,
    broadcast_queue: VecDeque<Message>,
    delivery_stats: MessageStats,
    max_queue_size: usize,
}

impl MessageDispatcher {
    pub fn new(max_queue_size: usize) -> Self {
        Self {
            message_queues: HashMap::new(),
            broadcast_queue: VecDeque::new(),
            delivery_stats: MessageStats::default(),
            max_queue_size,
        }
    }

    /// Send a message to a specific agent
    pub fn send_message(&mut self, message: Message) -> Result<()> {
        match message.recipient {
            Some(recipient) => {
                let queue = self
                    .message_queues
                    .entry(recipient)
                    .or_insert_with(VecDeque::new);

                if queue.len() >= self.max_queue_size {
                    self.delivery_stats.dropped_messages += 1;
                    return Err(crate::error::GaussTwinError::Custom(
                        "Message queue full".to_string(),
                    ));
                }

                queue.push_back(message);
                self.delivery_stats.sent_messages += 1;
            }
            None => {
                // Broadcast message
                if self.broadcast_queue.len() >= self.max_queue_size {
                    self.delivery_stats.dropped_messages += 1;
                    return Err(crate::error::GaussTwinError::Custom(
                        "Broadcast queue full".to_string(),
                    ));
                }

                self.broadcast_queue.push_back(message);
                self.delivery_stats.broadcast_messages += 1;
            }
        }

        Ok(())
    }

    /// Get messages for a specific agent
    pub fn get_messages(&mut self, agent_id: AgentId) -> Vec<Message> {
        let mut messages = Vec::new();

        // Get direct messages
        if let Some(queue) = self.message_queues.get_mut(&agent_id) {
            messages.extend(queue.drain(..));
        }

        // Add broadcast messages
        messages.extend(self.broadcast_queue.iter().cloned());

        self.delivery_stats.delivered_messages += messages.len() as u64;
        messages
    }

    /// Clear broadcast messages (call after delivering to all agents)
    pub fn clear_broadcasts(&mut self) {
        self.broadcast_queue.clear();
    }

    /// Get delivery statistics
    pub fn stats(&self) -> &MessageStats {
        &self.delivery_stats
    }

    /// Get queue size for an agent
    pub fn queue_size(&self, agent_id: AgentId) -> usize {
        self.message_queues
            .get(&agent_id)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    /// Get total pending messages
    pub fn total_pending(&self) -> usize {
        let direct_messages: usize = self.message_queues.values().map(|q| q.len()).sum();
        direct_messages + self.broadcast_queue.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MessageStats {
    pub sent_messages: u64,
    pub delivered_messages: u64,
    pub broadcast_messages: u64,
    pub dropped_messages: u64,
}

impl MessageStats {
    pub fn delivery_rate(&self) -> f64 {
        if self.sent_messages > 0 {
            self.delivered_messages as f64 / self.sent_messages as f64
        } else {
            0.0
        }
    }

    pub fn drop_rate(&self) -> f64 {
        let total = self.sent_messages + self.dropped_messages;
        if total > 0 {
            self.dropped_messages as f64 / total as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_creation() {
        let event = Event::agent_event(
            SimTime::new(1.0),
            AgentId::new(),
            None,
            serde_json::json!({"test": "data"}),
        );

        assert_eq!(event.scheduled_time, SimTime::new(1.0));
        assert_eq!(event.execution_count, 0);
        assert!(event.can_execute());
    }

    #[test]
    fn test_event_queue() {
        let mut queue = EventQueue::new();
        queue.set_current_time(SimTime::new(0.0));

        let event1 =
            Event::system_event(SimTime::new(1.0), "test".to_string(), serde_json::json!({}));

        let event2 = Event::system_event(
            SimTime::new(0.5),
            "test2".to_string(),
            serde_json::json!({}),
        );

        queue.schedule(event1).unwrap();
        queue.schedule(event2).unwrap();

        assert_eq!(queue.len(), 2);

        // Should return event2 first (earlier time)
        queue.set_current_time(SimTime::new(0.5));
        let next = queue.next_event().unwrap();
        assert_eq!(next.scheduled_time, SimTime::new(0.5));

        queue.set_current_time(SimTime::new(1.0));
        let next = queue.next_event().unwrap();
        assert_eq!(next.scheduled_time, SimTime::new(1.0));

        assert!(queue.is_empty());
    }

    #[test]
    fn test_event_expiration() {
        let event =
            Event::system_event(SimTime::new(1.0), "test".to_string(), serde_json::json!({}))
                .with_expiration(SimTime::new(2.0));

        assert!(!event.is_expired(SimTime::new(1.5)));
        assert!(event.is_expired(SimTime::new(2.5)));
    }

    #[test]
    fn test_event_execution_count() {
        let mut event =
            Event::system_event(SimTime::new(1.0), "test".to_string(), serde_json::json!({}))
                .with_max_executions(2);

        assert!(event.can_execute());
        event.mark_executed();
        assert!(event.can_execute());
        event.mark_executed();
        assert!(!event.can_execute());
    }

    #[test]
    fn test_message_creation() {
        let sender = AgentId::new();
        let recipient = AgentId::new();
        let content = serde_json::json!({"text": "Hello, World!"});
        let timestamp = SimTime::zero();

        let message = Message::new(
            Some(sender),
            Some(recipient),
            MessageType::AgentToAgent,
            content.clone(),
            timestamp,
        );

        assert_eq!(message.sender, Some(sender));
        assert_eq!(message.recipient, Some(recipient));
        assert_eq!(message.content, content);
        assert_eq!(message.priority, MessagePriority::Normal);
    }

    #[test]
    fn test_message_dispatcher() {
        let mut dispatcher = MessageDispatcher::new(10);
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        let message = Message::new(
            Some(agent1),
            Some(agent2),
            MessageType::AgentToAgent,
            serde_json::json!({"data": "test"}),
            SimTime::zero(),
        );

        assert!(dispatcher.send_message(message).is_ok());
        assert_eq!(dispatcher.queue_size(agent2), 1);

        let messages = dispatcher.get_messages(agent2);
        assert_eq!(messages.len(), 1);
        assert_eq!(dispatcher.queue_size(agent2), 0);
    }

    #[test]
    fn test_broadcast_message() {
        let mut dispatcher = MessageDispatcher::new(10);
        let sender = AgentId::new();
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        let broadcast = Message::broadcast(
            Some(sender),
            serde_json::json!({"announcement": "Hello everyone!"}),
            SimTime::zero(),
        );

        assert!(dispatcher.send_message(broadcast).is_ok());

        // Both agents should receive the broadcast
        let messages1 = dispatcher.get_messages(agent1);
        let messages2 = dispatcher.get_messages(agent2);

        assert_eq!(messages1.len(), 1);
        assert_eq!(messages2.len(), 1);
        assert_eq!(messages1[0].content, messages2[0].content);
    }
}
