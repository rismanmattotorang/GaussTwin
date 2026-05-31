//! The GaussIR typed schema.
//!
//! GaussIR is a typed intermediate representation for digital-twin models: a set of
//! typed [`Entity`] definitions, an initial [`Snapshot`] of entity instances, a
//! [`Scenario`] (time bounds, seed, parameters), and a set of [`Constraint`]s. It is
//! the artifact that deterministic validators accept or reject before any code
//! generation — the paper's "certified compilation" gate.

use crate::units::Unit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An identifier (entity name, instance id, …).
pub type Id = String;

/// The declared type of an entity attribute.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeType {
    Int,
    Float,
    Bool,
    Text,
    /// A fixed-length vector of `f64`.
    Vector {
        dim: usize,
    },
    /// A scalar `f64` carrying a physical unit (dimensional quantity).
    Quantity {
        unit: Unit,
    },
}

impl AttributeType {
    /// Human-readable type name (for diagnostics).
    pub fn name(&self) -> String {
        match self {
            AttributeType::Int => "int".into(),
            AttributeType::Float => "float".into(),
            AttributeType::Bool => "bool".into(),
            AttributeType::Text => "text".into(),
            AttributeType::Vector { dim } => format!("vector<{dim}>"),
            AttributeType::Quantity { unit } => format!("quantity<{}>", unit.name),
        }
    }
}

/// A named, typed attribute of an entity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: AttributeType,
}

impl Attribute {
    pub fn new(name: impl Into<String>, ty: AttributeType) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

/// A typed entity (agent / object) definition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub name: Id,
    pub attributes: Vec<Attribute>,
}

impl Entity {
    pub fn new(name: impl Into<Id>, attributes: Vec<Attribute>) -> Self {
        Self {
            name: name.into(),
            attributes,
        }
    }

    /// Look up an attribute's declared type by name.
    pub fn attribute_type(&self, name: &str) -> Option<&AttributeType> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.ty)
    }
}

/// A concrete attribute value in the initial snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Vector(Vec<f64>),
    Quantity { value: f64, unit: Unit },
}

impl Value {
    /// Human-readable type name (for diagnostics).
    pub fn type_name(&self) -> String {
        match self {
            Value::Int(_) => "int".into(),
            Value::Float(_) => "float".into(),
            Value::Bool(_) => "bool".into(),
            Value::Text(_) => "text".into(),
            Value::Vector(v) => format!("vector<{}>", v.len()),
            Value::Quantity { unit, .. } => format!("quantity<{}>", unit.name),
        }
    }

    /// The numeric magnitude of a scalar numeric value, if any (used by constraint
    /// checks). `Int`, `Float`, and `Quantity` are numeric; others are not.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Quantity { value, .. } => Some(*value),
            _ => None,
        }
    }
}

/// An instance of an [`Entity`] in the initial snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityInstance {
    pub id: Id,
    /// The [`Entity::name`] this instance is of.
    pub entity: Id,
    /// Attribute values, keyed by attribute name (sorted for determinism).
    pub attributes: BTreeMap<String, Value>,
}

/// The initial state: a set of entity instances.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub instances: Vec<EntityInstance>,
}

/// A declarative invariant that the snapshot (and, later, every simulation step)
/// must satisfy. Deterministic to evaluate.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// A numeric attribute must lie within `[min, max]` (either bound optional).
    Range {
        entity: Id,
        attribute: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// A numeric attribute must be `>= 0`.
    NonNegative { entity: Id, attribute: String },
    /// A vector attribute must have exactly `dim` components.
    VectorDim {
        entity: Id,
        attribute: String,
        dim: usize,
    },
}

impl Constraint {
    /// The entity name this constraint targets.
    pub fn entity(&self) -> &str {
        match self {
            Constraint::Range { entity, .. }
            | Constraint::NonNegative { entity, .. }
            | Constraint::VectorDim { entity, .. } => entity,
        }
    }

    /// The attribute name this constraint targets.
    pub fn attribute(&self) -> &str {
        match self {
            Constraint::Range { attribute, .. }
            | Constraint::NonNegative { attribute, .. }
            | Constraint::VectorDim { attribute, .. } => attribute,
        }
    }
}

/// Run parameters for a model.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// Seed for reproducible runs (see the core scheduler's seed-stability guarantee).
    pub seed: Option<u64>,
    pub start_time: f64,
    pub end_time: f64,
    pub time_step: f64,
    pub parameters: BTreeMap<String, Value>,
}

/// The root GaussIR document.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GaussIr {
    pub entities: Vec<Entity>,
    pub snapshot: Snapshot,
    pub scenario: Scenario,
    pub constraints: Vec<Constraint>,
}

impl GaussIr {
    /// Look up an entity definition by name.
    pub fn entity(&self, name: &str) -> Option<&Entity> {
        self.entities.iter().find(|e| e.name == name)
    }
}
