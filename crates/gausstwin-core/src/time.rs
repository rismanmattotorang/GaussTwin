//! Time management for GaussTwin simulations
//!
//! This module provides time representation, stepping, and scheduling utilities.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// Simulation time type (can be integer or floating-point based on configuration)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SimTime(f64);

impl SimTime {
    /// Create a new simulation time
    pub fn new(time: f64) -> Self {
        SimTime(time)
    }

    /// Get the raw time value
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Zero time
    pub fn zero() -> Self {
        SimTime(0.0)
    }

    /// Check if this is zero time
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// Maximum representable time
    pub fn max() -> Self {
        SimTime(f64::MAX)
    }

    /// Check if this is the maximum time
    pub fn is_max(&self) -> bool {
        self.0 == f64::MAX
    }

    /// Add a duration to this time
    pub fn add_duration(&self, duration: Duration) -> Self {
        SimTime(self.0 + duration.0)
    }

    /// Subtract a duration from this time
    pub fn sub_duration(&self, duration: Duration) -> Self {
        SimTime(self.0 - duration.0)
    }

    /// Calculate duration between two times
    pub fn duration_since(&self, earlier: SimTime) -> Duration {
        Duration(self.0 - earlier.0)
    }

    /// Calculate duration until another time
    pub fn duration_until(&self, later: SimTime) -> Duration {
        Duration(later.0 - self.0)
    }
}

impl Default for SimTime {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for SimTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.0)
    }
}

impl From<f64> for SimTime {
    fn from(time: f64) -> Self {
        SimTime(time)
    }
}

impl From<SimTime> for f64 {
    fn from(time: SimTime) -> Self {
        time.0
    }
}

impl Add<Duration> for SimTime {
    type Output = SimTime;

    fn add(self, rhs: Duration) -> Self::Output {
        self.add_duration(rhs)
    }
}

impl Sub<Duration> for SimTime {
    type Output = SimTime;

    fn sub(self, rhs: Duration) -> Self::Output {
        self.sub_duration(rhs)
    }
}

impl Sub<SimTime> for SimTime {
    type Output = Duration;

    fn sub(self, rhs: SimTime) -> Self::Output {
        self.duration_since(rhs)
    }
}

/// Duration between two simulation times
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Duration(f64);

impl Duration {
    /// Create a new duration
    pub fn new(duration: f64) -> Self {
        Duration(duration)
    }

    /// Get the raw duration value
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Zero duration
    pub fn zero() -> Self {
        Duration(0.0)
    }

    /// Check if this is zero duration
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// Create duration from seconds
    pub fn from_secs(secs: f64) -> Self {
        Duration(secs)
    }

    /// Create duration from milliseconds
    pub fn from_millis(millis: f64) -> Self {
        Duration(millis / 1000.0)
    }

    /// Create duration from microseconds
    pub fn from_micros(micros: f64) -> Self {
        Duration(micros / 1_000_000.0)
    }

    /// Get duration in seconds
    pub fn as_secs(&self) -> f64 {
        self.0
    }

    /// Get duration in milliseconds
    pub fn as_millis(&self) -> f64 {
        self.0 * 1000.0
    }

    /// Get duration in microseconds
    pub fn as_micros(&self) -> f64 {
        self.0 * 1_000_000.0
    }

    /// Get absolute value of duration
    pub fn abs(&self) -> Self {
        Duration(self.0.abs())
    }

    /// Check if duration is negative
    pub fn is_negative(&self) -> bool {
        self.0 < 0.0
    }

    /// Check if duration is positive
    pub fn is_positive(&self) -> bool {
        self.0 > 0.0
    }
}

impl Default for Duration {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 1.0 {
            write!(f, "{:.3}ms", self.as_millis())
        } else if self.0 < 60.0 {
            write!(f, "{:.3}s", self.0)
        } else if self.0 < 3600.0 {
            write!(f, "{}m{:.1}s", (self.0 / 60.0) as u32, self.0 % 60.0)
        } else {
            write!(
                f,
                "{}h{}m{:.1}s",
                (self.0 / 3600.0) as u32,
                ((self.0 % 3600.0) / 60.0) as u32,
                self.0 % 60.0
            )
        }
    }
}

impl From<f64> for Duration {
    fn from(duration: f64) -> Self {
        Duration(duration)
    }
}

impl From<Duration> for f64 {
    fn from(duration: Duration) -> Self {
        duration.0
    }
}

impl Add for Duration {
    type Output = Duration;

    fn add(self, rhs: Duration) -> Self::Output {
        Duration(self.0 + rhs.0)
    }
}

impl Sub for Duration {
    type Output = Duration;

    fn sub(self, rhs: Duration) -> Self::Output {
        Duration(self.0 - rhs.0)
    }
}

impl Mul<f64> for Duration {
    type Output = Duration;

    fn mul(self, rhs: f64) -> Self::Output {
        Duration(self.0 * rhs)
    }
}

impl Div<f64> for Duration {
    type Output = Duration;

    fn div(self, rhs: f64) -> Self::Output {
        Duration(self.0 / rhs)
    }
}

impl Div for Duration {
    type Output = f64;

    fn div(self, rhs: Duration) -> Self::Output {
        self.0 / rhs.0
    }
}

/// Time step size for simulation stepping
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeStep(Duration);

impl TimeStep {
    /// Create a new time step
    pub fn new(step: Duration) -> crate::error::Result<Self> {
        if step.is_negative() || step.is_zero() {
            return Err(crate::error::GaussTwinError::InvalidTimeStep(step.value()));
        }
        Ok(TimeStep(step))
    }

    /// Get the step duration
    pub fn duration(&self) -> Duration {
        self.0
    }

    /// Fixed time step (most common)
    pub fn fixed(step: f64) -> crate::error::Result<Self> {
        Self::new(Duration::from_secs(step))
    }

    /// Variable time step (for adaptive stepping)
    pub fn variable(min_step: f64, max_step: f64) -> VariableTimeStep {
        VariableTimeStep {
            min_step: Duration::from_secs(min_step),
            max_step: Duration::from_secs(max_step),
            current_step: Duration::from_secs(min_step),
        }
    }
}

impl fmt::Display for TimeStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TimeStep({})", self.0)
    }
}

/// Variable time step for adaptive stepping
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableTimeStep {
    min_step: Duration,
    max_step: Duration,
    current_step: Duration,
}

impl VariableTimeStep {
    /// Get the current step size
    pub fn current(&self) -> Duration {
        self.current_step
    }

    /// Get the minimum step size
    pub fn min(&self) -> Duration {
        self.min_step
    }

    /// Get the maximum step size
    pub fn max(&self) -> Duration {
        self.max_step
    }

    /// Update the current step size
    pub fn set_current(&mut self, step: Duration) -> crate::error::Result<()> {
        if step < self.min_step || step > self.max_step {
            return Err(crate::error::GaussTwinError::InvalidTimeStep(step.value()));
        }
        self.current_step = step;
        Ok(())
    }

    /// Increase step size (up to maximum)
    pub fn increase(&mut self, factor: f64) {
        let new_step = self.current_step * factor;
        if new_step <= self.max_step {
            self.current_step = new_step;
        } else {
            self.current_step = self.max_step;
        }
    }

    /// Decrease step size (down to minimum)
    pub fn decrease(&mut self, factor: f64) {
        let new_step = self.current_step / factor;
        if new_step >= self.min_step {
            self.current_step = new_step;
        } else {
            self.current_step = self.min_step;
        }
    }
}

/// Time window for collecting statistics or observations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    start: SimTime,
    end: SimTime,
}

impl TimeWindow {
    /// Create a new time window
    pub fn new(start: SimTime, end: SimTime) -> crate::error::Result<Self> {
        if start > end {
            return Err(crate::error::GaussTwinError::InvalidTimeStep(
                (end - start).value(),
            ));
        }
        Ok(TimeWindow { start, end })
    }

    /// Get the start time
    pub fn start(&self) -> SimTime {
        self.start
    }

    /// Get the end time
    pub fn end(&self) -> SimTime {
        self.end
    }

    /// Get the duration of the window
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }

    /// Check if a time is within this window
    pub fn contains(&self, time: SimTime) -> bool {
        time >= self.start && time <= self.end
    }

    /// Check if this window overlaps with another
    pub fn overlaps(&self, other: &TimeWindow) -> bool {
        self.start <= other.end && self.end >= other.start
    }

    /// Get the intersection with another window
    pub fn intersection(&self, other: &TimeWindow) -> Option<TimeWindow> {
        let start = if self.start > other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end < other.end {
            self.end
        } else {
            other.end
        };

        if start <= end {
            Some(TimeWindow { start, end })
        } else {
            None
        }
    }
}

/// Timer for measuring elapsed simulation time
#[derive(Debug, Clone)]
pub struct Timer {
    start_time: SimTime,
    end_time: Option<SimTime>,
}

impl Timer {
    /// Start a new timer
    pub fn start(current_time: SimTime) -> Self {
        Timer {
            start_time: current_time,
            end_time: None,
        }
    }

    /// Stop the timer
    pub fn stop(&mut self, current_time: SimTime) {
        self.end_time = Some(current_time);
    }

    /// Get elapsed time (if timer is stopped) or current elapsed time
    pub fn elapsed(&self, current_time: SimTime) -> Duration {
        let end = self.end_time.unwrap_or(current_time);
        end - self.start_time
    }

    /// Check if timer is running
    pub fn is_running(&self) -> bool {
        self.end_time.is_none()
    }

    /// Reset the timer
    pub fn reset(&mut self, current_time: SimTime) {
        self.start_time = current_time;
        self.end_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sim_time() {
        let t1 = SimTime::new(1.0);
        let t2 = SimTime::new(2.0);

        assert_eq!(t1.value(), 1.0);
        assert!(t1 < t2);

        let duration = t2 - t1;
        assert_eq!(duration.value(), 1.0);
    }

    #[test]
    fn test_duration() {
        let d1 = Duration::from_secs(1.5);
        let d2 = Duration::from_millis(500.0);

        assert_eq!(d1.as_secs(), 1.5);
        assert_eq!(d2.as_secs(), 0.5);

        let sum = d1 + d2;
        assert_eq!(sum.as_secs(), 2.0);
    }

    #[test]
    fn test_time_step() {
        let step = TimeStep::fixed(0.1).unwrap();
        assert_eq!(step.duration().as_secs(), 0.1);

        // Invalid step should fail
        assert!(TimeStep::fixed(0.0).is_err());
        assert!(TimeStep::fixed(-0.1).is_err());
    }

    #[test]
    fn test_time_window() {
        let start = SimTime::new(0.0);
        let end = SimTime::new(10.0);
        let window = TimeWindow::new(start, end).unwrap();

        assert!(window.contains(SimTime::new(5.0)));
        assert!(!window.contains(SimTime::new(15.0)));
        assert_eq!(window.duration().as_secs(), 10.0);
    }

    #[test]
    fn test_timer() {
        let start_time = SimTime::new(0.0);
        let mut timer = Timer::start(start_time);

        assert!(timer.is_running());

        let current_time = SimTime::new(5.0);
        assert_eq!(timer.elapsed(current_time).as_secs(), 5.0);

        timer.stop(current_time);
        assert!(!timer.is_running());
    }
}
