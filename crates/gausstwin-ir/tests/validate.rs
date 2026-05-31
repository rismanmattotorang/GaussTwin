//! Tests for the GaussIR deterministic validators.

use gausstwin_ir::{
    validate, Attribute, AttributeType, Constraint, Entity, EntityInstance, GaussIr, Snapshot,
    Unit, Value,
};
use std::collections::BTreeMap;

fn instance(id: &str, entity: &str, attrs: Vec<(&str, Value)>) -> EntityInstance {
    EntityInstance {
        id: id.into(),
        entity: entity.into(),
        attributes: attrs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
    }
}

/// A well-formed `Car { speed: float, position: vector<2>, mass: quantity<kg> }`
/// with one valid instance and a non-negative constraint on speed.
fn valid_ir() -> GaussIr {
    GaussIr {
        entities: vec![Entity::new(
            "Car",
            vec![
                Attribute::new("speed", AttributeType::Float),
                Attribute::new("position", AttributeType::Vector { dim: 2 }),
                Attribute::new(
                    "mass",
                    AttributeType::Quantity {
                        unit: Unit::kilograms(),
                    },
                ),
            ],
        )],
        snapshot: Snapshot {
            instances: vec![instance(
                "car1",
                "Car",
                vec![
                    ("speed", Value::Float(12.0)),
                    ("position", Value::Vector(vec![1.0, 2.0])),
                    (
                        "mass",
                        Value::Quantity {
                            value: 1500.0,
                            unit: Unit::kilograms(),
                        },
                    ),
                ],
            )],
        },
        constraints: vec![Constraint::NonNegative {
            entity: "Car".into(),
            attribute: "speed".into(),
        }],
        ..Default::default()
    }
}

#[test]
fn valid_document_certifies() {
    let report = validate(&valid_ir());
    assert!(report.is_valid(), "diagnostics: {:?}", report.diagnostics);
    assert_eq!(report.error_count(), 0);
}

#[test]
fn duplicate_entity_is_error() {
    let mut ir = valid_ir();
    ir.entities.push(Entity::new("Car", vec![]));
    let report = validate(&ir);
    assert!(!report.is_valid());
    assert!(report.errors().any(|d| d.code == "schema/duplicate-entity"));
}

#[test]
fn unknown_entity_reference_is_error() {
    let mut ir = valid_ir();
    ir.snapshot
        .instances
        .push(instance("ghost", "Spaceship", vec![]));
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "schema/unknown-entity"));
}

#[test]
fn type_mismatch_is_error() {
    let mut ir = valid_ir();
    // speed is float; put a bool there.
    ir.snapshot.instances[0]
        .attributes
        .insert("speed".into(), Value::Bool(true));
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "type/mismatch"));
}

#[test]
fn vector_dimension_mismatch_is_error() {
    let mut ir = valid_ir();
    ir.snapshot.instances[0]
        .attributes
        .insert("position".into(), Value::Vector(vec![1.0, 2.0, 3.0]));
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "type/vector-dim"));
}

#[test]
fn incompatible_unit_is_error() {
    let mut ir = valid_ir();
    // mass declared in kg; provide seconds (wrong dimension).
    ir.snapshot.instances[0].attributes.insert(
        "mass".into(),
        Value::Quantity {
            value: 5.0,
            unit: Unit::seconds(),
        },
    );
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "unit/incompatible"));
}

#[test]
fn compatible_unit_with_different_scale_is_ok() {
    let mut ir = valid_ir();
    // Declared kg; kilometres would be wrong, but grams (mass) should be fine.
    ir.snapshot.instances[0].attributes.insert(
        "mass".into(),
        Value::Quantity {
            value: 1_500_000.0,
            unit: Unit::new("g", gausstwin_ir::Dimension::MASS, 0.001),
        },
    );
    let report = validate(&ir);
    assert!(report.is_valid(), "diagnostics: {:?}", report.diagnostics);
}

#[test]
fn non_negative_constraint_violation_is_error() {
    let mut ir = valid_ir();
    ir.snapshot.instances[0]
        .attributes
        .insert("speed".into(), Value::Float(-3.0));
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "constraint/non-negative"));
}

#[test]
fn range_constraint_violation_is_error() {
    let mut ir = valid_ir();
    ir.constraints.push(Constraint::Range {
        entity: "Car".into(),
        attribute: "speed".into(),
        min: Some(0.0),
        max: Some(10.0),
    });
    // speed is 12.0 > max 10.0
    let report = validate(&ir);
    assert!(report.errors().any(|d| d.code == "constraint/range"));
}

#[test]
fn missing_attribute_is_warning_not_error() {
    let mut ir = valid_ir();
    ir.snapshot.instances[0].attributes.remove("mass");
    let report = validate(&ir);
    // Still certifies (warnings don't block), but a warning is recorded.
    assert!(report.is_valid(), "diagnostics: {:?}", report.diagnostics);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "type/missing-attribute"));
}

#[test]
fn constraint_on_unknown_attribute_is_error() {
    let mut ir = valid_ir();
    ir.constraints.push(Constraint::NonNegative {
        entity: "Car".into(),
        attribute: "altitude".into(), // not declared
    });
    let report = validate(&ir);
    assert!(report
        .errors()
        .any(|d| d.code == "schema/unknown-attribute"));
}

#[test]
fn validation_is_deterministic() {
    // Build a document with several independent problems; the report must be
    // byte-for-byte identical across runs (same diagnostics, same order).
    let mut ir = valid_ir();
    ir.entities.push(Entity::new("Car", vec![])); // duplicate
    ir.snapshot
        .instances
        .push(instance("ghost", "Spaceship", vec![("x", Value::Int(1))]));
    ir.snapshot.instances[0]
        .attributes
        .insert("speed".into(), Value::Bool(true)); // type mismatch

    let a = validate(&ir);
    let b = validate(&ir);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert!(a.error_count() >= 3);
}

#[test]
fn round_trips_through_json() {
    // The IR is fully serializable, so authored documents can be loaded from JSON.
    let ir = valid_ir();
    let json = serde_json::to_string(&ir).unwrap();
    let restored: GaussIr = serde_json::from_str(&json).unwrap();
    assert!(validate(&restored).is_valid());
}
