//! Deterministic validators for [`GaussIr`].
//!
//! These are the acceptance gate of the GaussTwin compilation pipeline: a GaussIR
//! document is *certified* only if [`validate`] reports no errors. Validation is
//! fully deterministic — the same document always yields the same diagnostics in the
//! same order — so it can gate code generation (and, later, an LLM proposer) without
//! any nondeterminism.
//!
//! Four passes run in order and their findings are merged and sorted:
//! 1. **schema** — names unique, references resolve;
//! 2. **type** — snapshot/parameter values match declared attribute types;
//! 3. **unit** — quantity values are dimensionally compatible with declarations;
//! 4. **constraint** — snapshot values satisfy declared constraints.

use crate::schema::{AttributeType, Constraint, GaussIr, Value};
use std::collections::BTreeSet;

/// Severity of a [`Diagnostic`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// A single validation finding.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `"type/mismatch"`.
    pub code: String,
    /// Where the problem is, e.g. `"instance 'car1' attribute 'speed'"`.
    pub path: String,
    pub message: String,
}

impl Diagnostic {
    fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }

    fn warning(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.to_string(),
            path: path.into(),
            message: message.into(),
        }
    }
}

/// The result of validating a [`GaussIr`]: a deterministically-ordered list of
/// diagnostics.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// `true` iff there are no `Error`-severity diagnostics. This is the
    /// certification predicate.
    pub fn is_valid(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Number of `Error`-severity diagnostics.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Iterator over only the error diagnostics.
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }
}

/// Validate a GaussIR document, returning a deterministic [`ValidationReport`].
pub fn validate(ir: &GaussIr) -> ValidationReport {
    let mut diags = Vec::new();

    validate_schema(ir, &mut diags);
    validate_types(ir, &mut diags);
    validate_constraints(ir, &mut diags);

    // Deterministic ordering: by path, then severity, then code, then message. This
    // guarantees identical input ⇒ identical output regardless of discovery order.
    diags.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.severity.cmp(&b.severity))
            .then(a.code.cmp(&b.code))
            .then(a.message.cmp(&b.message))
    });

    ValidationReport { diagnostics: diags }
}

/// Pass 1 — structural integrity: unique names, resolvable references.
fn validate_schema(ir: &GaussIr, diags: &mut Vec<Diagnostic>) {
    // Entity names unique.
    let mut seen_entities = BTreeSet::new();
    for entity in &ir.entities {
        if !seen_entities.insert(entity.name.clone()) {
            diags.push(Diagnostic::error(
                "schema/duplicate-entity",
                format!("entity '{}'", entity.name),
                "duplicate entity name",
            ));
        }
        // Attribute names unique within an entity.
        let mut seen_attrs = BTreeSet::new();
        for attr in &entity.attributes {
            if !seen_attrs.insert(attr.name.clone()) {
                diags.push(Diagnostic::error(
                    "schema/duplicate-attribute",
                    format!("entity '{}' attribute '{}'", entity.name, attr.name),
                    "duplicate attribute name",
                ));
            }
        }
    }

    // Instance ids unique; instance entity references resolve.
    let mut seen_instances = BTreeSet::new();
    for inst in &ir.snapshot.instances {
        if !seen_instances.insert(inst.id.clone()) {
            diags.push(Diagnostic::error(
                "schema/duplicate-instance",
                format!("instance '{}'", inst.id),
                "duplicate instance id",
            ));
        }
        if ir.entity(&inst.entity).is_none() {
            diags.push(Diagnostic::error(
                "schema/unknown-entity",
                format!("instance '{}'", inst.id),
                format!("references undefined entity '{}'", inst.entity),
            ));
        }
    }

    // Constraint references resolve to a defined entity + attribute.
    for c in &ir.constraints {
        let path = format!("constraint on '{}.{}'", c.entity(), c.attribute());
        match ir.entity(c.entity()) {
            None => diags.push(Diagnostic::error(
                "schema/unknown-entity",
                path,
                format!("constraint references undefined entity '{}'", c.entity()),
            )),
            Some(entity) => {
                if entity.attribute_type(c.attribute()).is_none() {
                    diags.push(Diagnostic::error(
                        "schema/unknown-attribute",
                        path,
                        format!(
                            "entity '{}' has no attribute '{}'",
                            c.entity(),
                            c.attribute()
                        ),
                    ));
                }
            }
        }
    }
}

/// Pass 2/3 — type and unit checking of snapshot values against declarations.
fn validate_types(ir: &GaussIr, diags: &mut Vec<Diagnostic>) {
    for inst in &ir.snapshot.instances {
        let Some(entity) = ir.entity(&inst.entity) else {
            // Unknown entity already reported in pass 1.
            continue;
        };

        // Declared attributes that are missing from the instance → warning.
        for attr in &entity.attributes {
            if !inst.attributes.contains_key(&attr.name) {
                diags.push(Diagnostic::warning(
                    "type/missing-attribute",
                    format!("instance '{}' attribute '{}'", inst.id, attr.name),
                    format!("declared attribute '{}' is not set", attr.name),
                ));
            }
        }

        for (name, value) in &inst.attributes {
            let path = format!("instance '{}' attribute '{}'", inst.id, name);
            let Some(declared) = entity.attribute_type(name) else {
                diags.push(Diagnostic::error(
                    "type/unknown-attribute",
                    path,
                    format!("entity '{}' has no attribute '{}'", inst.entity, name),
                ));
                continue;
            };
            check_value_against_type(&path, declared, value, diags);
        }
    }
}

/// Check a single value against a declared attribute type (types + units).
fn check_value_against_type(
    path: &str,
    declared: &AttributeType,
    value: &Value,
    diags: &mut Vec<Diagnostic>,
) {
    let mismatch = |diags: &mut Vec<Diagnostic>| {
        diags.push(Diagnostic::error(
            "type/mismatch",
            path,
            format!("expected {}, found {}", declared.name(), value.type_name()),
        ));
    };

    match (declared, value) {
        (AttributeType::Int, Value::Int(_)) => {}
        (AttributeType::Float, Value::Float(_)) => {}
        // An integer literal is acceptable where a float is expected.
        (AttributeType::Float, Value::Int(_)) => {}
        (AttributeType::Bool, Value::Bool(_)) => {}
        (AttributeType::Text, Value::Text(_)) => {}
        (AttributeType::Vector { dim }, Value::Vector(v)) => {
            if v.len() != *dim {
                diags.push(Diagnostic::error(
                    "type/vector-dim",
                    path,
                    format!("expected vector of length {}, found {}", dim, v.len()),
                ));
            }
        }
        (
            AttributeType::Quantity {
                unit: declared_unit,
            },
            Value::Quantity { unit, .. },
        ) => {
            // Pass 3 — unit dimensional compatibility.
            if !declared_unit.compatible_with(unit) {
                diags.push(Diagnostic::error(
                    "unit/incompatible",
                    path,
                    format!(
                        "unit '{}' is not dimensionally compatible with declared unit '{}'",
                        unit.name, declared_unit.name
                    ),
                ));
            }
        }
        _ => mismatch(diags),
    }
}

/// Pass 4 — every snapshot value must satisfy the declared constraints.
fn validate_constraints(ir: &GaussIr, diags: &mut Vec<Diagnostic>) {
    for c in &ir.constraints {
        for inst in ir
            .snapshot
            .instances
            .iter()
            .filter(|i| i.entity == c.entity())
        {
            let Some(value) = inst.attributes.get(c.attribute()) else {
                // Missing attribute already reported as a warning in pass 2.
                continue;
            };
            let path = format!("instance '{}' attribute '{}'", inst.id, c.attribute());

            match c {
                Constraint::NonNegative { .. } => {
                    if let Some(n) = value.as_number() {
                        if n < 0.0 {
                            diags.push(Diagnostic::error(
                                "constraint/non-negative",
                                path,
                                format!("value {n} violates non-negative constraint"),
                            ));
                        }
                    }
                }
                Constraint::Range { min, max, .. } => {
                    if let Some(n) = value.as_number() {
                        if let Some(lo) = min {
                            if n < *lo {
                                diags.push(Diagnostic::error(
                                    "constraint/range",
                                    path.clone(),
                                    format!("value {n} is below minimum {lo}"),
                                ));
                            }
                        }
                        if let Some(hi) = max {
                            if n > *hi {
                                diags.push(Diagnostic::error(
                                    "constraint/range",
                                    path,
                                    format!("value {n} is above maximum {hi}"),
                                ));
                            }
                        }
                    }
                }
                Constraint::VectorDim { dim, .. } => {
                    if let Value::Vector(v) = value {
                        if v.len() != *dim {
                            diags.push(Diagnostic::error(
                                "constraint/vector-dim",
                                path,
                                format!("expected vector of length {}, found {}", dim, v.len()),
                            ));
                        }
                    }
                }
            }
        }
    }
}
