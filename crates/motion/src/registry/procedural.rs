use super::*;
fn text(title: &str, default: &str) -> Port {
    value(
        title,
        exact(Type::Text),
        Some(Literal::Text {
            value: default.into(),
        }),
    )
}
fn vector(title: &str, v: f64) -> Port {
    value(
        title,
        Pattern::Primitive,
        Some(Literal::Vector {
            value: vec![v; 3],
            unit: Unit::Unitless,
            space: Space::World,
        }),
    )
}
pub(super) fn operations() -> Vec<Operation> {
    let geometry = || value("Geometry", exact(Type::Geometry), None);
    let base = || {
        vec![
            ("geometry", geometry()),
            ("domain", text("Domain", "point")),
            ("attribute", text("Attribute", "Cd")),
        ]
    };
    let result = || ResultType::Exact { ty: Type::Geometry };
    let mut create = base();
    create.push(("value", vector("Value", 1.)));
    let mut random = base();
    random.extend([
        ("minimum", vector("Minimum", 0.)),
        ("maximum", vector("Maximum", 1.)),
        (
            "seed",
            value(
                "Seed",
                exact(Type::Integer),
                Some(Literal::Integer { value: 0 }),
            ),
        ),
    ]);
    let mut transfer = base();
    transfer.extend([
        (
            "source",
            value("Source Geometry", exact(Type::Geometry), None),
        ),
        ("source_domain", text("Source Domain", "point")),
        (
            "radius",
            value(
                "Radius (m)",
                exact(Type::Distance { unit: Unit::Meters }),
                Some(Literal::Distance {
                    value: 1.,
                    unit: Unit::Meters,
                }),
            ),
        ),
        ("sampling", text("Sampling", "nearest")),
    ]);
    vec![
        operation(
            (
                "attribute_create",
                "set",
                "Attribute Create",
                Builtin::AttributeCreate,
            ),
            Palette::Foundation,
            create,
            result(),
            Cost::Element,
            vec![],
        ),
        operation(
            (
                "attribute_randomize",
                "uniform",
                "Attribute Randomize",
                Builtin::AttributeRandomize,
            ),
            Palette::Foundation,
            random,
            result(),
            Cost::Element,
            vec![],
        ),
        operation(
            (
                "attribute_transfer",
                "radius",
                "Attribute Transfer",
                Builtin::AttributeTransfer,
            ),
            Palette::Foundation,
            transfer,
            result(),
            Cost::Geometry,
            vec![],
        ),
        operation(
            ("wrangle", "rhai", "Wrangle", Builtin::Wrangle),
            Palette::Foundation,
            vec![
                (
                    "inputs",
                    Port {
                        title: "Geometry Inputs".into(),
                        form: Form::List,
                        pattern: exact(Type::Geometry),
                        default: Some(Binding::List { values: vec![] }),
                    },
                ),
                ("run_over", text("Run Over", "detail")),
                (
                    "source",
                    text("Rhai Source", "let geo = grid(8, 8, 0.25);\ngeo"),
                ),
                ("parameters", text("Parameters", "{}")),
                ("time", time()),
            ],
            result(),
            Cost::Geometry,
            vec![],
        ),
    ]
}
