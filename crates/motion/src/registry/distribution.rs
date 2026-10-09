use super::*;

pub(super) fn operations() -> Vec<Operation> {
    let count = |title, default| {
        value(
            title,
            exact(Type::Integer),
            Some(Literal::Integer { value: default }),
        )
    };
    let mut entries = vec![operation(
        ("distribute", "grid", "Grid", Builtin::Grid),
        Palette::Foundation,
        vec![
            ("columns", count("Columns", 20)),
            ("rows", count("Rows", 12)),
            (
                "spacing",
                value(
                    "Spacing (m)",
                    exact(Type::Vector {
                        dimensions: 2,
                        unit: Unit::Meters,
                        space: Space::World,
                    }),
                    Some(Literal::Vector {
                        value: vec![0.12; 2],
                        unit: Unit::Meters,
                        space: Space::World,
                    }),
                ),
            ),
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
        ],
        ResultType::Exact { ty: Type::Geometry },
        Cost::Geometry,
        vec![Domain::Point],
    )];
    entries.push(operation(
        ("cloner", "points", "Copy to Points", Builtin::Cloner),
        Palette::Artist,
        vec![
            ("prototype", value("Prototype", exact(Type::Geometry), None)),
            (
                "points",
                value("Target Points", exact(Type::Geometry), None),
            ),
            (
                "attributes",
                value(
                    "Attributes to Copy",
                    exact(Type::Text),
                    Some(Literal::Text { value: "Cd".into() }),
                ),
            ),
        ],
        ResultType::Exact { ty: Type::Geometry },
        Cost::Geometry,
        vec![Domain::Instance],
    ));
    for entry in &mut entries {
        entry.version = 2;
    }
    entries
}
