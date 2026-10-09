use super::*;

pub(super) fn operations() -> Vec<Operation> {
    let mut ports = vec![
        ("profile", value("Profile", exact(Type::Geometry), None)),
        (
            "canvas_unit",
            value(
                "Meters per canvas unit",
                exact(Type::Distance { unit: Unit::Meters }),
                Some(Literal::Distance {
                    value: 0.01,
                    unit: Unit::Meters,
                }),
            ),
        ),
        (
            "detail",
            value(
                "Curve detail (pixels per canvas unit)",
                exact(Type::Scalar),
                Some(Literal::Scalar { value: 1. }),
            ),
        ),
        (
            "even_odd",
            value(
                "Even-odd fill",
                exact(Type::Bool),
                Some(Literal::Bool { value: false }),
            ),
        ),
    ];
    let tessellate = operation(
        ("tessellate", "planar", "Tessellate", Builtin::Tessellate),
        Palette::Foundation,
        ports.clone(),
        ResultType::Exact { ty: Type::Geometry },
        Cost::Geometry,
        vec![Domain::Point, Domain::Face, Domain::FaceCorner],
    );
    ports.extend([
        (
            "depth",
            value(
                "Depth (m)",
                exact(Type::Distance { unit: Unit::Meters }),
                Some(Literal::Distance {
                    value: 0.15,
                    unit: Unit::Meters,
                }),
            ),
        ),
        (
            "segments",
            value(
                "Depth segments",
                exact(Type::Integer),
                Some(Literal::Integer { value: 1 }),
            ),
        ),
    ]);
    vec![
        tessellate,
        operation(
            (
                "extrude",
                "closed_profile",
                "Extrude Profile",
                Builtin::ExtrudeProfile,
            ),
            Palette::Artist,
            ports,
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Point, Domain::Face, Domain::FaceCorner],
        ),
    ]
}
