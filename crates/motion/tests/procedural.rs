use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{
        Geometry, GeometryData, Limits, Prepared, Value,
        attributes::{Domain, vector},
    },
    model::{Construction, Node, Reference},
    registry,
    types::{Literal, Type, Unit},
};
use std::sync::Arc;
fn val(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn text(s: &str) -> Binding {
    val(Literal::Text { value: s.into() })
}
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
fn node(key: &str, mode: &str, args: Vec<(&str, Binding)>) -> Node {
    registry::resolve(
        &format!("fold.motion.{key}"),
        if matches!(key, "distribute" | "cloner") {
            2
        } else {
            1
        },
        mode,
    )
    .unwrap()
    .node(key, args.into_iter().map(|(k, v)| (k.into(), v)).collect())
    .unwrap()
}
fn construction(nodes: Vec<Node>) -> Construction {
    let mut c =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    let asset = &mut c.definitions[0];
    asset.outputs[0].ty = Type::Geometry;
    asset.outputs[0].value = Reference::Node {
        node: nodes.last().unwrap().id,
    }
    .into();
    asset.nodes = nodes;
    c
}
fn evaluate(c: &Construction, time: Time) -> Arc<Geometry> {
    let Value::Geometry(g) = Prepared::new(c.clone())
        .unwrap()
        .evaluate(
            c.definitions[0].outputs[0].id,
            time,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
    else {
        panic!("geometry")
    };
    g
}
fn script(source: &str) -> Construction {
    construction(vec![node(
        "wrangle",
        "rhai",
        vec![("source", text(source))],
    )])
}
#[test]
fn grid_attributes_copy_material_share_geometry_and_seeded_colors() {
    let grid = node(
        "distribute",
        "grid",
        vec![
            ("columns", val(Literal::Integer { value: 3 })),
            ("rows", val(Literal::Integer { value: 2 })),
        ],
    );
    let g = evaluate(&construction(vec![grid.clone()]), Time::ZERO);
    assert_eq!(g.count(Domain::Point).unwrap(), 6);
    assert_eq!(g.count(Domain::Primitive).unwrap(), 4);
    assert_eq!(g.count(Domain::Edge).unwrap(), 9);
    let random = node(
        "attribute_randomize",
        "uniform",
        vec![("geometry", link(grid.id))],
    );
    let colored = evaluate(
        &construction(vec![grid.clone(), random.clone()]),
        Time::ZERO,
    );
    assert!(colored.attributes.get(Domain::Point, "Cd").is_some());
    let sphere = node("mesh", "sphere", vec![]);
    let copies = node(
        "cloner",
        "points",
        vec![("prototype", link(sphere.id)), ("points", link(random.id))],
    );
    let c = construction(vec![grid, random, sphere, copies]);
    let g = evaluate(&c, Time::ZERO);
    let GeometryData::Instances(instances) = &g.data else {
        panic!()
    };
    assert_eq!(instances.instances.len(), 6);
    assert_eq!(
        g.attributes.get(Domain::Instance, "Cd").unwrap().values,
        instances
            .distribution
            .attributes
            .get(Domain::Point, "Cd")
            .unwrap()
            .values
    );
    assert_eq!(g, evaluate(&c, Time::new(8, 3).unwrap()));
}
#[test]
fn create_randomize_and_transfer_support_domains_and_typed_values() {
    let grid = node("distribute", "grid", vec![]);
    for domain in ["point", "primitive", "edge", "corner", "detail"] {
        let create = node(
            "attribute_create",
            "set",
            vec![
                ("geometry", link(grid.id)),
                ("domain", text(domain)),
                ("attribute", text("weight")),
                ("value", val(Literal::Scalar { value: 0.75 })),
            ],
        );
        let result = evaluate(&construction(vec![grid.clone(), create]), Time::ZERO);
        let d = Domain::parse(domain).unwrap();
        assert_eq!(
            result.attributes.get(d, "weight").unwrap().values.len(),
            result.count(d).unwrap()
        );
    }
    let color = node(
        "attribute_create",
        "set",
        vec![
            ("geometry", link(grid.id)),
            ("value", val(vector([0.2, 0.4, 0.8], Unit::Unitless))),
        ],
    );
    let transfer = node(
        "attribute_transfer",
        "radius",
        vec![("geometry", link(grid.id)), ("source", link(color.id))],
    );
    let result = evaluate(&construction(vec![grid, color, transfer]), Time::ZERO);
    assert_eq!(
        result.read_attribute(Domain::Point, "Cd", 0).unwrap(),
        vector([0.2, 0.4, 0.8], Unit::Unitless)
    );
}
#[test]
fn input_free_wrangle_animates_geometry_and_roundtrips_source() {
    let c = script(
        "// Keep this artist comment.\nlet g = geometry();\nfor i in 0..4 { add_point(g, vec3(i.to_float(), sin(time), 0.0)); }\nadd_triangle(g, 0, 1, 2);\ng",
    );
    let reopened = fold_motion::source::parse(&fold_motion::source::canonical(&c).unwrap())
        .unwrap()
        .construction;
    assert_eq!(c, reopened);
    let initial = evaluate(&c, Time::ZERO);
    assert_eq!(initial.count(Domain::Point).unwrap(), 4);
    assert_eq!(initial.count(Domain::Primitive).unwrap(), 1);
    let later = evaluate(&c, Time::new(1, 1).unwrap());
    assert!((later.position(Domain::Point, 0).unwrap()[1] - 1f64.sin()).abs() < 1e-6);
    assert_eq!(initial, evaluate(&c, Time::ZERO));
    assert_eq!(
        initial.element_id(Domain::Point, 0).unwrap(),
        later.element_id(Domain::Point, 0).unwrap()
    );
}
#[test]
fn whole_sphere_animation_in_one_wrangle() {
    let c = script(
        r#"let g = grid(3, 2, 0.3);
for i in 0..count(g, "point") {
 let p = point(g, i); p.z = sin(time + i.to_float()) * 0.1; set_point(g, i, p);
 set_attribute(g, "point", "Cd", i, vec3(random(i, 12), 0.5, 0.8));
 set_attribute(g, "point", "orient", i, orient(vec3(0.0, 1.0, 0.0), time));
}
copy_to_points(sphere(0.1), g, "Cd")"#,
    );
    let g = evaluate(&c, Time::new(1, 2).unwrap());
    let GeometryData::Instances(copies) = &g.data else {
        panic!()
    };
    assert_eq!(copies.instances.len(), 6);
    assert_ne!(copies.instances[0].orient, [0., 0., 0., 1.]);
    assert!(g.attributes.get(Domain::Instance, "Cd").is_some());
}
#[test]
fn element_wrangle_reads_original_input_and_can_add_geometry() {
    let grid = node(
        "distribute",
        "grid",
        vec![
            ("columns", val(Literal::Integer { value: 2 })),
            ("rows", val(Literal::Integer { value: 1 })),
        ],
    );
    let wrangle = node(
        "wrangle",
        "rhai",
        vec![
            (
                "inputs",
                Binding::List {
                    values: vec![link(grid.id)],
                },
            ),
            ("run_over", text("point")),
            (
                "source",
                text("add_point(geo, point(input, index) + vec3(0.0, 1.0, 0.0)); geo"),
            ),
        ],
    );
    let g = evaluate(&construction(vec![grid, wrangle]), Time::ZERO);
    assert_eq!(g.count(Domain::Point).unwrap(), 4);
    assert_eq!(g.position(Domain::Point, 2).unwrap()[1], 1.);
}
#[test]
fn wrangle_limits_cancellation_and_errors_do_not_return_partial_geometry() {
    let sources = [
        "let x = ;",
        "let g = geometry(); add_point(g, vec3(1.0, 0.0, 0.0)); 42",
        "import \"/etc/passwd\" as io; geometry()",
        "let g=grid(2,2,0.2); set_attribute(g,\"point\",\"Cd\",0,42); g",
        "loop {}",
    ];
    for source in sources {
        let c = script(source);
        let result = Prepared::new(c.clone()).unwrap().evaluate(
            c.definitions[0].outputs[0].id,
            Time::ZERO,
            Limits {
                steps: 1000,
                elements: 1000,
            },
            &|| Ok(()),
        );
        assert!(result.is_err(), "{source}");
    }
    let c = script("loop {}");
    let start = std::time::Instant::now();
    let result = Prepared::new(c.clone()).unwrap().evaluate_detailed(
        c.definitions[0].outputs[0].id,
        Time::ZERO,
        Limits::default(),
        &|| {
            if start.elapsed().as_millis() > 20 {
                Err("cancel test".into())
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(
        result.unwrap_err().kind,
        fold_motion::evaluation::FailureKind::Cancelled
    );
    eprintln!(
        "Wrangle cancellation after 20 ms request: {:?} total",
        start.elapsed()
    );
    assert!(start.elapsed().as_secs() < 2);
}
#[test]
fn arbitrary_attributes_reject_conflicting_types_and_positions_are_authoritative() {
    let mut g = evaluate(&script("grid(2,2,0.2)"), Time::ZERO)
        .as_ref()
        .clone();
    let values = vec![vector([1., 2., 3.], Unit::Meters); 4];
    g.set_attribute(Domain::Point, "P", values).unwrap();
    assert_eq!(g.position(Domain::Point, 3).unwrap(), [1., 2., 3.]);
    g.set_attribute(
        Domain::Point,
        "custom",
        vec![Literal::Integer { value: 3 }; 4],
    )
    .unwrap();
    assert!(
        g.set_attribute(
            Domain::Point,
            "custom",
            vec![Literal::Scalar { value: 3. }; 4]
        )
        .is_err()
    );
}

#[test]
fn topology_edits_preserve_surviving_domain_attributes() {
    let c = script(
        r#"let g=grid(3,2,1.0);
for i in 0..count(g,"primitive"){set_attribute(g,"primitive","weight",i,i.to_float()+1.0);}
for i in 0..count(g,"edge"){set_attribute(g,"edge","tag",i,element_id(g,"edge",i));}
remove_point(g,0);
g"#,
    );
    let g = evaluate(&c, Time::ZERO);
    assert_eq!(g.count(Domain::Point).unwrap(), 5);
    for domain in [Domain::Primitive, Domain::Edge] {
        for a in g.attributes.0.iter().filter(|a| a.domain == domain) {
            assert_eq!(a.values.len(), g.count(domain).unwrap());
        }
    }
    for (i, value) in g
        .attributes
        .get(Domain::Edge, "tag")
        .unwrap()
        .values
        .iter()
        .enumerate()
    {
        assert_eq!(
            *value,
            Literal::Integer {
                value: g.element_id(Domain::Edge, i).unwrap() as i64
            }
        );
    }
}
#[test]
fn source_cache_does_not_capture_geometry_or_clock_from_another_wrangle() {
    let a = script("grid(2,2,0.2)");
    let b = script("grid(2,2,0.2)");
    let ga = evaluate(&a, Time::ZERO);
    let gb = evaluate(&b, Time::ZERO);
    assert_ne!(ga.id, gb.id);
    assert_ne!(
        ga.element_id(Domain::Point, 0).unwrap(),
        gb.element_id(Domain::Point, 0).unwrap()
    );
    assert_eq!(ga, evaluate(&a, Time::new(2, 3).unwrap()));
}
#[test]
fn radius_transfer_retains_unmatched_and_rejects_missing_or_categorical_blending() {
    let base = node("distribute", "grid", vec![]);
    let target = node(
        "attribute_create",
        "set",
        vec![
            ("geometry", link(base.id)),
            ("attribute", text("tag")),
            ("value", val(Literal::Integer { value: 7 })),
        ],
    );
    let far = node(
        "wrangle",
        "rhai",
        vec![(
            "source",
            text(
                "let g=geometry(); add_point(g,vec3(100.0,100.0,100.0)); set_attribute(g,\"point\",\"tag\",0,9); g",
            ),
        )],
    );
    let transfer = node(
        "attribute_transfer",
        "radius",
        vec![
            ("geometry", link(target.id)),
            ("source", link(far.id)),
            ("attribute", text("tag")),
        ],
    );
    let g = evaluate(&construction(vec![base, target, far, transfer]), Time::ZERO);
    assert!(
        g.attributes
            .get(Domain::Point, "tag")
            .unwrap()
            .values
            .iter()
            .all(|v| *v == Literal::Integer { value: 7 })
    );
}

#[test]
fn wrangle_parameter_candidate_undo_and_project_reopen_preserve_exact_source() {
    use fold_motion::editing;
    use fold_project::proposals::Store;
    use fold_project::{EditBatch, Mutation, Project};
    let id = DocumentId::new();
    let mut base = script(
        "// Artist comment survives.\nlet g = geometry(); add_point(g, vec3(params.height, time, 0.0)); g",
    );
    base.definitions[0].nodes[0]
        .parameters
        .get_mut("parameters")
        .unwrap()
        .value = serde_json::to_value(text(r#"{"height":0.25}"#)).unwrap();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(base.document(id).unwrap())],
        })
        .unwrap();
    let mut changed = base.clone();
    changed.definitions[0].nodes[0]
        .parameters
        .get_mut("parameters")
        .unwrap()
        .value = serde_json::to_value(text(r#"{"height":0.75}"#)).unwrap();
    let now = std::time::Instant::now();
    let mut store = Store::default();
    let candidate = editing::prepare(
        &project.snapshot(),
        id,
        editing::Scope::Document,
        changed.clone(),
    )
    .unwrap();
    let handle = store.insert(candidate, now).unwrap();
    let inspected = store.inspect(handle, now).unwrap();
    store
        .apply(handle, inspected.receipt, inspected.base, &mut project, now)
        .unwrap();
    let read =
        |p: &Project| Construction::from_document(&p.snapshot().state().documents[&id]).unwrap();
    let time = Time::new(1001, 24000).unwrap();
    assert_eq!(
        evaluate(&read(&project), time)
            .position(Domain::Point, 0)
            .unwrap()[0],
        0.75
    );
    project.undo().unwrap();
    assert_eq!(read(&project), base);
    assert_eq!(
        evaluate(&read(&project), time)
            .position(Domain::Point, 0)
            .unwrap()[0],
        0.25
    );
    project.redo().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wrangle.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 8).unwrap();
    assert_eq!(read(&reopened), changed);
    for time in [Time::ZERO, time, Time::new(2, 1).unwrap(), time, Time::ZERO] {
        assert_eq!(evaluate(&read(&reopened), time), evaluate(&changed, time));
    }
}

#[test]
fn copying_owned_attribute_payloads_consumes_the_geometry_budget() {
    let source = r#"let p = grid(1,1,1.0);
set_attribute(p,"point","payload",0,"PAYLOAD");
let shape = box_mesh(vec3(0.1,0.1,0.1));
let result = geometry();
for i in 0..100 { result = copy_to_points(shape,p,"payload"); }
result"#
        .replace("PAYLOAD", &"x".repeat(2048));
    let c = script(&source);
    let result = Prepared::new(c.clone()).unwrap().evaluate(
        c.definitions[0].outputs[0].id,
        Time::ZERO,
        Limits {
            steps: 1_000_000,
            elements: 1500,
        },
        &|| Ok(()),
    );
    assert!(result.unwrap_err().contains("budget"));
}
