use fold_foundation::{DocumentId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{GeometryData, Limits, Prepared, Value},
    model::{Construction, Reference},
    registry, source,
    types::{Literal, Type, Unit},
};
fn fixture() -> Construction {
    let mut value =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let sphere = registry::resolve("fold.motion.mesh", 1, "sphere")
        .unwrap()
        .node("Sphere", Default::default())
        .unwrap();
    value.definitions[0].outputs[0].value = Reference::Node { node: sphere.id }.into();
    value.definitions[0].outputs[0].ty = Type::Geometry;
    value.definitions[0].nodes = vec![sphere];
    value
}
#[test]
fn sphere_source_sampling_radius_identity_and_budget() {
    let mut value = fixture();
    assert_eq!(
        source::parse(&source::canonical(&value).unwrap())
            .unwrap()
            .construction,
        value
    );
    let evaluate = |value: &Construction| {
        let Value::Geometry(geometry) = Prepared::new(value.clone())
            .unwrap()
            .evaluate(
                value.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Ok(()),
            )
            .unwrap()
        else {
            panic!()
        };
        geometry
    };
    let before = evaluate(&value);
    let GeometryData::Mesh(a) = &before.data else {
        panic!()
    };
    assert_eq!(a.vertex_ids.len(), 561);
    assert_eq!(a.face_ids.len(), 960);
    value.definitions[0].nodes[0]
        .parameters
        .get_mut("radius")
        .unwrap()
        .value = serde_json::to_value(Binding::Literal {
        value: Literal::Distance {
            value: 0.1,
            unit: Unit::Meters,
        },
    })
    .unwrap();
    let after = evaluate(&value);
    let GeometryData::Mesh(b) = &after.data else {
        panic!()
    };
    assert_eq!(before.id, after.id);
    assert_eq!(a.vertex_ids, b.vertex_ids);
    assert_eq!(a.face_ids, b.face_ids);
    assert_eq!(a.prepared.mesh.indices, b.prepared.mesh.indices);
    assert_ne!(a.prepared.mesh.positions, b.prepared.mesh.positions);
    assert!(
        Prepared::new(value.clone())
            .unwrap()
            .evaluate(
                value.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits {
                    elements: 100,
                    ..Default::default()
                },
                &|| Ok(())
            )
            .is_err()
    );
    assert!(
        Prepared::new(value.clone())
            .unwrap()
            .evaluate(
                value.definitions[0].outputs[0].id,
                Time::ZERO,
                Limits::default(),
                &|| Err("cancelled".into())
            )
            .is_err()
    );
}
