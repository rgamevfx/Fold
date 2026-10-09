use super::*;
use crate::types::{Alpha, Encoding};

pub(super) fn operations() -> Vec<Operation> {
    let geometry = || value("Geometry", exact(Type::Geometry), None);
    let color = || {
        value(
            "Color",
            exact(Type::Color {
                encoding: Encoding::LinearRec709,
                alpha: Alpha::Straight,
            }),
            Some(Literal::Color {
                value: [1.; 4],
                encoding: Encoding::LinearRec709,
                alpha: Alpha::Straight,
            }),
        )
    };
    let mut operations = vec![
        operation(
            ("fill", "solid", "Fill", Builtin::Fill),
            Palette::Artist,
            vec![
                ("geometry", geometry()),
                ("color", color()),
                (
                    "even_odd",
                    value(
                        "Even-odd fill",
                        exact(Type::Bool),
                        Some(Literal::Bool { value: false }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Uniform,
            vec![Domain::Collection],
        ),
        operation(
            (
                "stroke",
                "solid_centered_butt_miter",
                "Stroke",
                Builtin::Stroke,
            ),
            Palette::Artist,
            vec![
                ("geometry", geometry()),
                ("color", color()),
                (
                    "width",
                    value(
                        "Width (canvas)",
                        exact(Type::Distance { unit: Unit::Canvas }),
                        Some(Literal::Distance {
                            value: 2.,
                            unit: Unit::Canvas,
                        }),
                    ),
                ),
            ],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Uniform,
            vec![Domain::Collection],
        ),
    ];
    let stroke = operations[1].clone();
    for (mode, title, builtin) in [
        (
            "solid_centered_butt_round",
            "Stroke · Butt / Round",
            Builtin::StrokeButtRound,
        ),
        (
            "solid_centered_round_miter",
            "Stroke · Round / Miter",
            Builtin::StrokeRoundMiter,
        ),
        (
            "solid_centered_round_round",
            "Stroke · Round / Round",
            Builtin::StrokeRoundRound,
        ),
    ] {
        let mut choice = stroke.clone();
        choice.mode = mode.into();
        choice.title = title.into();
        choice.builtin = builtin;
        operations.push(choice);
    }
    for (mode, title, builtin) in [
        ("lit", "Material", Builtin::LitMaterial),
        ("unlit", "Unlit Material", Builtin::UnlitMaterial),
    ] {
        let mut parameters = vec![("geometry", geometry()), ("color", color())];
        if builtin == Builtin::LitMaterial {
            parameters.extend([
                (
                    "roughness",
                    value(
                        "Roughness",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 0.5 }),
                    ),
                ),
                (
                    "reflectance",
                    value(
                        "Reflectance",
                        exact(Type::Scalar),
                        Some(Literal::Scalar { value: 0.5 }),
                    ),
                ),
            ]);
        }
        operations.push(operation(
            ("material", mode, title, builtin),
            Palette::Artist,
            parameters,
            ResultType::Exact { ty: Type::Geometry },
            Cost::Uniform,
            vec![Domain::Collection],
        ));
    }
    operations
}
