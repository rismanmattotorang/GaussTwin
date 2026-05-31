//! Physical units with dimensional analysis.
//!
//! A [`Unit`] carries an SI [`Dimension`] (a vector of base-dimension exponents) plus
//! a scale factor to SI base units. Two units are *compatible* iff their dimensions
//! match — this is what unit validation checks, so `meters` and `kilometers` are
//! compatible but `meters` and `seconds` are not.

use serde::{Deserialize, Serialize};

/// Exponents of the seven SI base dimensions, in order:
/// length, mass, time, electric current, temperature, amount of substance,
/// luminous intensity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dimension(pub [i8; 7]);

impl Dimension {
    /// Dimensionless (all exponents zero).
    pub const DIMENSIONLESS: Dimension = Dimension([0, 0, 0, 0, 0, 0, 0]);
    /// Length (L).
    pub const LENGTH: Dimension = Dimension([1, 0, 0, 0, 0, 0, 0]);
    /// Mass (M).
    pub const MASS: Dimension = Dimension([0, 1, 0, 0, 0, 0, 0]);
    /// Time (T).
    pub const TIME: Dimension = Dimension([0, 0, 1, 0, 0, 0, 0]);
    /// Temperature (Θ).
    pub const TEMPERATURE: Dimension = Dimension([0, 0, 0, 0, 1, 0, 0]);

    /// Product of two dimensions (exponent-wise addition), e.g. for `a * b`.
    pub fn mul(self, other: Dimension) -> Dimension {
        let mut out = [0i8; 7];
        for i in 0..7 {
            out[i] = self.0[i] + other.0[i];
        }
        Dimension(out)
    }

    /// Quotient of two dimensions (exponent-wise subtraction), e.g. for `a / b`.
    pub fn div(self, other: Dimension) -> Dimension {
        let mut out = [0i8; 7];
        for i in 0..7 {
            out[i] = self.0[i] - other.0[i];
        }
        Dimension(out)
    }

    /// Whether this is the dimensionless dimension.
    pub fn is_dimensionless(&self) -> bool {
        *self == Dimension::DIMENSIONLESS
    }
}

/// A physical unit: a display name, its [`Dimension`], and a `scale` factor that
/// converts a value in this unit to the SI base unit of the same dimension
/// (e.g. `kilometers` has dimension [`Dimension::LENGTH`] and scale `1000.0`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub name: String,
    pub dimension: Dimension,
    pub scale: f64,
}

impl Unit {
    /// Construct a unit.
    pub fn new(name: impl Into<String>, dimension: Dimension, scale: f64) -> Self {
        Self {
            name: name.into(),
            dimension,
            scale,
        }
    }

    /// A dimensionless unit (e.g. a ratio or count).
    pub fn dimensionless() -> Self {
        Unit::new("1", Dimension::DIMENSIONLESS, 1.0)
    }

    /// Whether two units measure the same physical dimension (and can therefore be
    /// compared, added, or assigned to one another after scaling).
    pub fn compatible_with(&self, other: &Unit) -> bool {
        self.dimension == other.dimension
    }

    // --- A few common SI units for convenience ---

    /// Metre (length).
    pub fn meters() -> Self {
        Unit::new("m", Dimension::LENGTH, 1.0)
    }
    /// Kilometre (length, scale 1000).
    pub fn kilometers() -> Self {
        Unit::new("km", Dimension::LENGTH, 1000.0)
    }
    /// Second (time).
    pub fn seconds() -> Self {
        Unit::new("s", Dimension::TIME, 1.0)
    }
    /// Kilogram (mass).
    pub fn kilograms() -> Self {
        Unit::new("kg", Dimension::MASS, 1.0)
    }
    /// Metres per second (velocity = length / time).
    pub fn meters_per_second() -> Self {
        Unit::new("m/s", Dimension::LENGTH.div(Dimension::TIME), 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_is_by_dimension_not_scale() {
        assert!(Unit::meters().compatible_with(&Unit::kilometers()));
        assert!(!Unit::meters().compatible_with(&Unit::seconds()));
    }

    #[test]
    fn velocity_dimension_is_length_over_time() {
        assert_eq!(
            Unit::meters_per_second().dimension,
            Dimension::LENGTH.div(Dimension::TIME)
        );
        assert!(!Unit::meters_per_second().compatible_with(&Unit::meters()));
    }
}
