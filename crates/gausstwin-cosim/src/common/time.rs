//! Time management and synchronization
//!
//! Provides robust time management capabilities for co-simulation:
//! - Logical time management
//! - Time synchronization
//! - Event scheduling
//! - Causality tracking

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    ops::{Add, Sub},
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use super::{CosimError, Result, TimeStatus};

/// Simulation time representation
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SimulationTime {
    /// Integer part (for discrete steps)
    pub steps: u64,

    /// Fractional part (for continuous time)
    pub fraction: f64,
}

impl SimulationTime {
    /// Create new simulation time
    pub fn new(steps: u64, fraction: f64) -> Self {
        Self { steps, fraction }
    }

    /// Get zero time
    pub fn zero() -> Self {
        Self::new(0, 0.0)
    }

    /// Convert from real time
    pub fn from_duration(duration: std::time::Duration) -> Self {
        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        SimulationTime {
            steps: secs,
            fraction: nanos as f64 / 1_000_000_000.0,
        }
    }

    /// Convert to real time
    pub fn to_duration(&self) -> std::time::Duration {
        let secs = self.steps;
        let nanos = (self.fraction * 1_000_000_000.0) as u32;
        std::time::Duration::new(secs, nanos)
    }

    /// Check if time is zero
    pub fn is_zero(&self) -> bool {
        self.steps == 0 && self.fraction == 0.0
    }

    /// Get the next discrete step
    pub fn next_step(&self) -> Self {
        Self::new(self.steps + 1, 0.0)
    }

    pub fn duration_since(&self, other: &Self) -> std::time::Duration {
        let self_total = self.steps as f64 + self.fraction;
        let other_total = other.steps as f64 + other.fraction;
        let diff = if self_total > other_total {
            self_total - other_total
        } else {
            0.0
        };
        std::time::Duration::from_secs_f64(diff)
    }
}

impl Add for SimulationTime {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        let total = (self.steps as f64 + self.fraction) + (other.steps as f64 + other.fraction);
        let steps = total.floor() as u64;
        let fraction = total.fract();
        Self::new(steps, fraction)
    }
}

impl Sub for SimulationTime {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        let total = (self.steps as f64 + self.fraction) - (other.steps as f64 + other.fraction);
        let steps = total.floor() as u64;
        let fraction = total.fract();
        Self::new(steps, fraction)
    }
}

impl Eq for SimulationTime {}
impl Ord for SimulationTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.steps.cmp(&other.steps) {
            std::cmp::Ordering::Equal => self
                .fraction
                .partial_cmp(&other.fraction)
                .unwrap_or(std::cmp::Ordering::Equal),
            ord => ord,
        }
    }
}

/// Time manager for co-simulation
#[derive(Debug)]
pub struct TimeManager {
    /// Current simulation time
    current_time: SimulationTime,

    /// Lookahead time
    lookahead: Duration,

    /// Time horizon
    horizon: Option<SimulationTime>,

    /// Pending time requests
    pending_requests: HashMap<String, SimulationTime>,

    /// Event queue
    event_queue: BinaryHeap<TimedEvent>,

    /// Time status
    status: TimeStatus,
}

impl TimeManager {
    /// Create new time manager
    pub fn new() -> Self {
        Self {
            current_time: SimulationTime::zero(),
            lookahead: Duration::from_secs(0),
            horizon: None,
            pending_requests: HashMap::new(),
            event_queue: BinaryHeap::new(),
            status: TimeStatus::Granted,
        }
    }

    /// Set lookahead time
    pub fn set_lookahead(&mut self, lookahead: Duration) {
        self.lookahead = lookahead;
    }

    /// Set time horizon
    pub fn set_horizon(&mut self, horizon: SimulationTime) {
        self.horizon = Some(horizon);
    }

    /// Get current time
    pub fn current_time(&self) -> SimulationTime {
        self.current_time
    }

    /// Request time advance
    pub fn request_time(&mut self, federate: String, requested_time: SimulationTime) -> Result<()> {
        // Validate request
        if let Some(horizon) = self.horizon {
            if requested_time > horizon {
                return Err(CosimError::TimeSync(format!(
                    "Requested time {} exceeds horizon {}",
                    requested_time.steps, horizon.steps
                )));
            }
        }

        // Add to pending requests
        self.pending_requests.insert(federate, requested_time);
        self.status = TimeStatus::Pending;

        Ok(())
    }

    /// Grant time advance
    pub fn grant_time(&mut self, federate: &str) -> Result<()> {
        if let Some(requested_time) = self.pending_requests.get(federate) {
            self.current_time = *requested_time;
            self.pending_requests.remove(federate);
            self.status = TimeStatus::Granted;
            Ok(())
        } else {
            Err(CosimError::TimeSync(format!(
                "No pending time request for federate {}",
                federate
            )))
        }
    }

    /// Schedule event
    pub fn schedule_event(&mut self, event: TimedEvent) {
        self.event_queue.push(event);
    }

    /// Get next event
    pub fn next_event(&mut self) -> Option<TimedEvent> {
        self.event_queue.pop()
    }

    /// Check if time advance is safe
    pub fn is_advance_safe(&self, target_time: SimulationTime) -> bool {
        if let Some(next_event) = self.event_queue.peek() {
            next_event.time > target_time
        } else {
            true
        }
    }
}

/// Timed event for scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedEvent {
    /// Event time
    pub time: SimulationTime,

    /// Event priority
    pub priority: i32,

    /// Event data
    pub data: Vec<u8>,
}

impl PartialEq for TimedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.priority == other.priority
    }
}

impl Eq for TimedEvent {}

impl PartialOrd for TimedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TimedEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority events come first
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.time.partial_cmp(&other.time).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_time() {
        let t1 = SimulationTime::new(1, 0.5);
        let t2 = SimulationTime::new(2, 0.7);

        assert!(t1 < t2);

        let sum = t1 + t2;
        assert_eq!(sum.steps, 4);
        // Fractions accumulate in f64 (0.5 + 0.7 carries into steps), so compare
        // with a tolerance rather than for exact equality.
        assert!((sum.fraction - 0.2).abs() < 1e-9);

        let diff = t2 - t1;
        assert_eq!(diff.steps, 1);
        assert!((diff.fraction - 0.2).abs() < 1e-9);
    }

    #[test]
    fn test_time_manager() {
        let mut tm = TimeManager::new();

        // Test time advance
        tm.request_time("fed1".to_string(), SimulationTime::new(1, 0.0))
            .unwrap();
        assert_eq!(tm.status, TimeStatus::Pending);

        tm.grant_time("fed1").unwrap();
        assert_eq!(tm.status, TimeStatus::Granted);
        assert_eq!(tm.current_time(), SimulationTime::new(1, 0.0));

        // Test event scheduling
        let event = TimedEvent {
            time: SimulationTime::new(2, 0.0),
            priority: 1,
            data: vec![1, 2, 3],
        };
        tm.schedule_event(event.clone());

        let next = tm.next_event().unwrap();
        assert_eq!(next.time, event.time);
    }
}
