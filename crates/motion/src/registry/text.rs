use super::*;
pub(super) fn operations() -> Vec<Operation> {
    let string = |title, default: &str| {
        value(
            title,
            exact(Type::Text),
            Some(Literal::Text {
                value: default.into(),
            }),
        )
    };
    let length = |title, default| {
        value(
            title,
            exact(Type::Distance { unit: Unit::Canvas }),
            Some(Literal::Distance {
                value: default,
                unit: Unit::Canvas,
            }),
        )
    };
    let meters = |title, default| {
        value(
            title,
            exact(Type::Distance { unit: Unit::Meters }),
            Some(Literal::Distance {
                value: default,
                unit: Unit::Meters,
            }),
        )
    };
    vec![
        operation(
            (
                "text_animator",
                "word_position",
                "Text Animator",
                Builtin::TextAnimatorWords,
            ),
            Palette::Artist,
            vec![
                ("text", value("Text", exact(Type::Geometry), None)),
                (
                    "position",
                    value(
                        "Position (canvas)",
                        exact(Type::Vector {
                            dimensions: 2,
                            unit: Unit::Canvas,
                            space: Space::Screen,
                        }),
                        Some(Literal::Vector {
                            value: vec![0., 0.],
                            unit: Unit::Canvas,
                            space: Space::Screen,
                        }),
                    ),
                ),
                (
                    "stagger",
                    value(
                        "Word delay",
                        exact(Type::Duration),
                        Some(Literal::Duration {
                            value: Time::new(7, 100).expect("constant delay"),
                        }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Word],
        ),
        operation(
            ("extrude", "straight_text", "Extrude", Builtin::Extrude),
            Palette::Artist,
            vec![
                ("profile", value("Profile", exact(Type::Geometry), None)),
                ("canvas_unit", meters("Meters per canvas unit", 0.01)),
                ("depth", meters("Depth (m)", 0.15)),
                (
                    "segments",
                    value(
                        "Depth segments",
                        exact(Type::Integer),
                        Some(Literal::Integer { value: 1 }),
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
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Collection, Domain::Point, Domain::Face],
        ),
        operation(
            (
                "outline_text",
                "curves",
                "Outline Text",
                Builtin::OutlineText,
            ),
            Palette::Artist,
            vec![("text", value("Text", exact(Type::Geometry), None))],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Collection, Domain::Curve],
        ),
        operation(
            ("text", "packaged", "Text", Builtin::Text),
            Palette::Artist,
            vec![
                ("text", string("Text", "MAKE IT MOVE")),
                ("font", string("Font", crate::evaluation::text::FONT_NAME)),
                ("size", length("Size (canvas)", 140.)),
                (
                    "wrap",
                    value(
                        "Wrap text",
                        exact(Type::Bool),
                        Some(Literal::Bool { value: false }),
                    ),
                ),
                ("width", length("Wrap width (canvas)", 1920.)),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Collection, Domain::Line],
        ),
    ]
}
