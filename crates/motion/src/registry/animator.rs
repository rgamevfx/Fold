use super::*;
pub(super) fn operations() -> Vec<Operation> {
    let mut operations = vec![
        operation(
            (
                "animator",
                "rotation_z_add",
                "Z Rotation Animator",
                Builtin::InstanceRotationZ,
            ),
            Palette::Artist,
            vec![
                ("geometry", value("Instances", exact(Type::Geometry), None)),
                (
                    "angle",
                    value(
                        "Angle (rad)",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 0. }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Instance],
        ),
        operation(
            ("random", "uniform", "Random", Builtin::InstanceRandom),
            Palette::Artist,
            vec![
                (
                    "minimum",
                    value(
                        "Minimum",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 0. }),
                    ),
                ),
                (
                    "maximum",
                    value(
                        "Maximum",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 1. }),
                    ),
                ),
                (
                    "stream",
                    value(
                        "Stream",
                        exact(Type::Text),
                        Some(Literal::Text {
                            value: "variation".into(),
                        }),
                    ),
                ),
            ],
            ResultType::Exact {
                ty: Type::Field {
                    value: Box::new(Type::Scalar),
                    domain: Domain::Instance,
                    space: Space::World,
                },
            },
            Cost::Element,
            vec![Domain::Instance],
        ),
        operation(
            (
                "animator",
                "color_replace",
                "Color Animator",
                Builtin::InstanceColor,
            ),
            Palette::Artist,
            vec![
                ("geometry", value("Instances", exact(Type::Geometry), None)),
                (
                    "color",
                    value(
                        "Color",
                        exact(Type::Color {
                            encoding: crate::types::Encoding::LinearRec709,
                            alpha: crate::types::Alpha::Straight,
                        }),
                        Some(Literal::Color {
                            value: [1.; 4],
                            encoding: crate::types::Encoding::LinearRec709,
                            alpha: crate::types::Alpha::Straight,
                        }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Instance],
        ),
        operation(
            (
                "animator",
                "scale_multiply",
                "Scale Animator",
                Builtin::InstanceScale,
            ),
            Palette::Artist,
            vec![
                ("geometry", value("Instances", exact(Type::Geometry), None)),
                (
                    "factor",
                    value(
                        "Scale multiplier",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 1. }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Instance],
        ),
        operation(
            ("field", "radial", "Radial Field", Builtin::RadialField),
            Palette::Artist,
            vec![
                (
                    "center",
                    value(
                        "Center (m)",
                        exact(Type::Vector {
                            dimensions: 3,
                            unit: Unit::Meters,
                            space: Space::World,
                        }),
                        Some(Literal::Vector {
                            value: vec![0.; 3],
                            unit: Unit::Meters,
                            space: Space::World,
                        }),
                    ),
                ),
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
            ],
            ResultType::Exact {
                ty: Type::Field {
                    value: Box::new(Type::Scalar),
                    domain: Domain::Instance,
                    space: Space::World,
                },
            },
            Cost::Element,
            vec![Domain::Instance],
        ),
    ];
    operations.push(operation(
        ("field", "linear", "Linear Field", Builtin::LinearField),
        Palette::Artist,
        [
            ("start", "Start (m)", [-0.5, 0., 0.]),
            ("end", "End (m)", [0.5, 0., 0.]),
        ]
        .into_iter()
        .map(|(key, title, coordinates)| {
            (
                key,
                value(
                    title,
                    exact(Type::Vector {
                        dimensions: 3,
                        unit: Unit::Meters,
                        space: Space::World,
                    }),
                    Some(Literal::Vector {
                        value: coordinates.to_vec(),
                        unit: Unit::Meters,
                        space: Space::World,
                    }),
                ),
            )
        })
        .collect(),
        ResultType::Exact {
            ty: Type::Field {
                value: Box::new(Type::Scalar),
                domain: Domain::Instance,
                space: Space::World,
            },
        },
        Cost::Element,
        vec![Domain::Instance],
    ));
    for (mode, title, ty) in [
        ("scalar_linear", "Scalar Ramp", Type::Scalar),
        (
            "color_linear",
            "Color Ramp",
            Type::Color {
                encoding: crate::types::Encoding::LinearRec709,
                alpha: crate::types::Alpha::Straight,
            },
        ),
    ] {
        operations.push(operation(
            ("ramp", mode, title, Builtin::Ramp),
            Palette::Artist,
            vec![
                (
                    "position",
                    value(
                        "Input",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 0. }),
                    ),
                ),
                (
                    "stops",
                    Port {
                        title: "Stops".into(),
                        form: Form::Ramp,
                        pattern: exact(ty),
                        default: None,
                    },
                ),
            ],
            ResultType::Parameter {
                parameter: "stops".into(),
                signal: false,
            },
            Cost::Element,
            vec![Domain::Instance, Domain::Collection],
        ));
    }
    operations
}
