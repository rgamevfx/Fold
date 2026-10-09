use super::*;

pub(super) fn operations() -> Vec<Operation> {
    let mut operations = vec![];
    for (mode, title, builtin, dimensions, unit, space) in [
        (
            "screen",
            "Transform · Screen",
            Builtin::TransformScreen,
            2,
            Unit::Canvas,
            Space::Screen,
        ),
        (
            "world_mesh",
            "Transform · Mesh",
            Builtin::TransformMesh,
            3,
            Unit::Meters,
            Space::World,
        ),
        (
            "world_instances",
            "Transform · Instances",
            Builtin::TransformInstances,
            3,
            Unit::Meters,
            Space::World,
        ),
    ] {
        let vector = |title, initial, unit| {
            value(
                title,
                exact(Type::Vector {
                    dimensions,
                    unit,
                    space,
                }),
                Some(Literal::Vector {
                    value: vec![initial; dimensions as usize],
                    unit,
                    space,
                }),
            )
        };
        let mut parameters = vec![
            ("geometry", value("Geometry", exact(Type::Geometry), None)),
            ("translation", vector("Translation", 0., unit)),
            ("pivot", vector("Pivot", 0., unit)),
        ];
        if builtin == Builtin::TransformMesh {
            parameters.push((
                "rotation",
                value(
                    "Rotation",
                    exact(Type::Rotation { space }),
                    Some(Literal::Rotation {
                        value: [0., 0., 0., 1.],
                        space,
                    }),
                ),
            ));
        } else {
            parameters.push((
                "angle",
                value(
                    "Z rotation (rad)",
                    exact(Type::Scalar),
                    Some(Literal::Scalar { value: 0. }),
                ),
            ));
        }
        parameters.push((
            "scale",
            if builtin == Builtin::TransformInstances {
                value(
                    "Uniform scale",
                    exact(Type::Scalar),
                    Some(Literal::Scalar { value: 1. }),
                )
            } else {
                vector("Scale", 1., Unit::Unitless)
            },
        ));
        operations.push(operation(
            ("transform", mode, title, builtin),
            Palette::Artist,
            parameters,
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![if builtin == Builtin::TransformInstances {
                Domain::Instance
            } else {
                Domain::Collection
            }],
        ));
    }
    operations
}
