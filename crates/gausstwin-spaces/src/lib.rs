//! GaussTwin Spaces - High-performance spatial implementations
//!
//! This crate provides optimized spatial data structures and algorithms for the GaussTwin project.
//! It includes SIMD-accelerated operations, parallel processing capabilities, and comprehensive
//! visualization tools.

#![warn(missing_docs)]
#![cfg_attr(feature = "simd", feature(portable_simd))]

/// Common spatial utilities and data structures
pub mod common;
/// Error types for spatial operations
pub mod error;
/// Spatial indexing and search algorithms
pub mod spatial_index;

// Re-exports
pub use common::{DistanceMetric, HighPerformanceMemoryPool, SpatialCache};
pub use error::{ErrorSeverity, SpatialError, SpatialResult};
pub use spatial_index::{SpatialIndex, SpatialPoint};

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};
use uuid::Uuid;

/// 3D point representation
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
    /// Z coordinate
    pub z: f64,
}

impl Point {
    /// Create a new point
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Create a point at the origin
    pub fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Calculate the distance to another point
    pub fn distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Calculate the squared distance to another point
    pub fn squared_distance_to(&self, other: &Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    /// Calculate the dot product with another point
    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Calculate the cross product with another point
    pub fn cross(&self, other: &Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    /// Calculate the magnitude of the point vector
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Normalize the point vector
    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag == 0.0 {
            *self
        } else {
            Self {
                x: self.x / mag,
                y: self.y / mag,
                z: self.z / mag,
            }
        }
    }

    /// Rotate the point around the X axis
    pub fn rotate_x(&self, angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            x: self.x,
            y: self.y * cos - self.z * sin,
            z: self.y * sin + self.z * cos,
        }
    }

    /// Rotate the point around the Y axis
    pub fn rotate_y(&self, angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            x: self.x * cos + self.z * sin,
            y: self.y,
            z: -self.x * sin + self.z * cos,
        }
    }

    /// Rotate the point around the Z axis
    pub fn rotate_z(&self, angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            x: self.x * cos - self.y * sin,
            y: self.x * sin + self.y * cos,
            z: self.z,
        }
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl Mul<f64> for Point {
    type Output = Self;

    fn mul(self, scalar: f64) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
            z: self.z * scalar,
        }
    }
}

impl Div<f64> for Point {
    type Output = Self;

    fn div(self, scalar: f64) -> Self {
        Self {
            x: self.x / scalar,
            y: self.y / scalar,
            z: self.z / scalar,
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.2}, {:.2}, {:.2})", self.x, self.y, self.z)
    }
}

/// Unique identifier for agents/entities in the spatial space
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(Uuid);

impl AgentId {
    /// Create a new random AgentId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Raw underlying UUID
    pub fn raw(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<[f64; 3]> for Point {
    fn from(arr: [f64; 3]) -> Self {
        Point::new(arr[0], arr[1], arr[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn test_point_operations() {
        let p1 = Point::new(1.0, 2.0, 3.0);
        let p2 = Point::new(4.0, 5.0, 6.0);

        // Test basic operations
        let sum = p1 + p2;
        assert_eq!(sum, Point::new(5.0, 7.0, 9.0));

        let diff = p2 - p1;
        assert_eq!(diff, Point::new(3.0, 3.0, 3.0));

        let scaled = p1 * 2.0;
        assert_eq!(scaled, Point::new(2.0, 4.0, 6.0));

        let divided = p2 / 2.0;
        assert_eq!(divided, Point::new(2.0, 2.5, 3.0));
    }

    #[test]
    fn test_point_methods() {
        let p1 = Point::new(1.0, 0.0, 0.0);
        let p2 = Point::new(0.0, 1.0, 0.0);

        // Test distance calculations
        assert!((p1.distance_to(&p2) - 2.0_f64.sqrt()).abs() < 1e-10);
        assert!((p1.squared_distance_to(&p2) - 2.0).abs() < 1e-10);

        // Test vector operations
        assert_eq!(p1.dot(&p2), 0.0);
        assert_eq!(p1.cross(&p2), Point::new(0.0, 0.0, 1.0));
        assert_eq!(p1.magnitude(), 1.0);
        assert_eq!(p1.normalize(), p1);
    }

    #[test]
    fn test_point_rotations() {
        let p = Point::new(1.0, 0.0, 0.0);

        // Test rotations
        let rotated = p.rotate_z(PI / 2.0);
        assert!((rotated.x - 0.0).abs() < 1e-10);
        assert!((rotated.y - 1.0).abs() < 1e-10);
        assert!((rotated.z - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_display() {
        let p = Point::new(1.234, 5.678, 9.012);
        assert_eq!(format!("{}", p), "(1.23, 5.68, 9.01)");
    }
}

// Conditionally compile visualization module
#[cfg(feature = "plotters")]
/// Visualization and rendering utilities for spatial data
pub mod visualization;

// When plotters feature is disabled, provide no-op stubs so downstream code still compiles
#[cfg(not(feature = "plotters"))]
/// Visualization and rendering utilities for spatial data
pub mod visualization {
    /// Configuration for animation settings
    #[derive(Debug, Clone, Copy)]
    pub struct AnimationConfig;
    /// Configuration for camera settings
    #[derive(Debug, Clone, Copy)]
    pub struct CameraConfig;
    /// Configuration for visualization settings
    #[derive(Debug, Clone, Copy)]
    pub struct VisualizationConfig;
    /// Main visualizer for spatial data
    #[derive(Debug, Clone, Copy)]
    pub struct Visualizer;
    /// Quality settings for rendering
    #[derive(Debug, Clone, Copy)]
    pub enum RenderingQuality {
        /// Low quality rendering
        Low,
        /// Medium quality rendering
        Medium,
        /// High quality rendering
        High,
    }
    /// Easing functions for animations
    #[derive(Debug, Clone, Copy)]
    pub enum EasingFunction {}
}
