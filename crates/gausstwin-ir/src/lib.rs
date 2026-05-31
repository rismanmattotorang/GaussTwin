//! # GaussIR — typed intermediate representation + deterministic validators
//!
//! GaussIR is the artifact at the centre of the GaussTwin compilation pipeline
//! described in the paper:
//!
//! ```text
//! authoring (spreadsheet / LLM) --> GaussIR --> [deterministic validators] --> codegen --> run
//!                                                        ^ certification gate
//! ```
//!
//! This crate provides the **typed schema** ([`schema`]) and the **deterministic
//! validators** ([`validate`]) — the "certified compilation" acceptance gate. The
//! validators are pure and deterministic (same document ⇒ same diagnostics, same
//! order), so they can gate code generation today and an LLM *proposer* later,
//! without introducing any nondeterminism. No LLM is required for this layer.
//!
//! ```
//! use gausstwin_ir::{validate, GaussIr, Entity, Attribute, AttributeType,
//!     EntityInstance, Snapshot, Constraint, Value};
//! use std::collections::BTreeMap;
//!
//! let mut attrs = BTreeMap::new();
//! attrs.insert("speed".to_string(), Value::Float(12.0));
//!
//! let ir = GaussIr {
//!     entities: vec![Entity::new(
//!         "Car",
//!         vec![Attribute::new("speed", AttributeType::Float)],
//!     )],
//!     snapshot: Snapshot {
//!         instances: vec![EntityInstance {
//!             id: "car1".into(),
//!             entity: "Car".into(),
//!             attributes: attrs,
//!         }],
//!     },
//!     constraints: vec![Constraint::NonNegative {
//!         entity: "Car".into(),
//!         attribute: "speed".into(),
//!     }],
//!     ..Default::default()
//! };
//!
//! let report = validate(&ir);
//! assert!(report.is_valid());
//! ```

pub mod schema;
pub mod units;
pub mod validate;

pub use schema::{
    Attribute, AttributeType, Constraint, Entity, EntityInstance, GaussIr, Id, Scenario, Snapshot,
    Value,
};
pub use units::{Dimension, Unit};
pub use validate::{validate, Diagnostic, Severity, ValidationReport};
