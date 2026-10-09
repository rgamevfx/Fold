use super::*;
use crate::types::{Alpha, Encoding};

pub(super) fn operations() -> Vec<Operation> {
    let scalar = |title, default| {
        value(
            title,
            exact(Type::Scalar),
            Some(Literal::Scalar { value: default }),
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
    let vector = |title, default: [f64; 3], unit| {
        value(
            title,
            exact(Type::Vector {
                dimensions: 3,
                unit,
                space: Space::World,
            }),
            Some(Literal::Vector {
                value: default.to_vec(),
                unit,
                space: Space::World,
            }),
        )
    };
    let camera = |mode, title, builtin, projection| {
        operation(
            ("camera", mode, title, builtin),
            Palette::Artist,
            vec![
                (
                    "position",
                    vector("Position (m)", [0., 1., 5.], Unit::Meters),
                ),
                ("target", vector("Look at (m)", [0.; 3], Unit::Meters)),
                ("up", vector("Up direction", [0., 1., 0.], Unit::Unitless)),
                projection,
                ("near", meters("Near clip (m)", 0.01)),
                ("far", meters("Far clip (m)", 100.)),
                ("ev100", scalar("Exposure (EV100)", 8.)),
            ],
            ResultType::Exact { ty: Type::Camera },
            Cost::Uniform,
            vec![Domain::Collection],
        )
    };
    let plane = |mode, title, builtin, physical| {
        let mut inputs = vec![
            ("geometry", value("Geometry", exact(Type::Geometry), None)),
            ("camera", value("Camera", exact(Type::Camera), None)),
            ("distance", meters("Camera distance (m)", 2.)),
            (
                "position",
                value(
                    "Canvas position",
                    exact(Type::Vector {
                        dimensions: 2,
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    }),
                    Some(Literal::Vector {
                        value: vec![0.; 2],
                        unit: Unit::Canvas,
                        space: Space::Screen,
                    }),
                ),
            ),
            ("opacity", scalar("Opacity", 1.)),
        ];
        if physical {
            inputs.push(("meters_per_unit", scalar("Meters per canvas unit", 0.001)));
        }
        operation(
            ("place", mode, title, builtin),
            Palette::Artist,
            inputs,
            ResultType::Exact {
                ty: Type::Placement,
            },
            Cost::Uniform,
            vec![Domain::Collection],
        )
    };
    vec![
        plane(
            "camera_plane_viewport",
            "Camera Plane · Viewport",
            Builtin::CameraPlaneViewport,
            false,
        ),
        plane(
            "camera_plane_physical",
            "Camera Plane · Physical Size",
            Builtin::CameraPlanePhysical,
            true,
        ),
        operation(
            ("mesh", "box", "Box", Builtin::BoxMesh),
            Palette::Artist,
            vec![("size", vector("Size (m)", [0.06; 3], Unit::Meters))],
            ResultType::Exact { ty: Type::Geometry },
            Cost::Geometry,
            vec![Domain::Collection, Domain::Point, Domain::Face],
        ),
        operation(
            ("place", "world", "World Placement", Builtin::World),
            Palette::Artist,
            vec![
                ("geometry", value("Geometry", exact(Type::Geometry), None)),
                ("position", vector("Position (m)", [0.; 3], Unit::Meters)),
                (
                    "rotation",
                    value(
                        "Rotation",
                        exact(Type::Rotation {
                            space: Space::World,
                        }),
                        Some(Literal::Rotation {
                            value: [0., 0., 0., 1.],
                            space: Space::World,
                        }),
                    ),
                ),
                ("scale", vector("Scale", [1.; 3], Unit::Unitless)),
                ("opacity", scalar("Opacity", 1.)),
            ],
            ResultType::Exact {
                ty: Type::Placement,
            },
            Cost::Uniform,
            vec![Domain::Collection],
        ),
        camera(
            "perspective",
            "Perspective Camera",
            Builtin::PerspectiveCamera,
            (
                "vertical_fov",
                scalar("Vertical FOV (rad)", std::f64::consts::FRAC_PI_4),
            ),
        ),
        camera(
            "orthographic",
            "Orthographic Camera",
            Builtin::OrthographicCamera,
            ("vertical_size", meters("Vertical size (m)", 4.)),
        ),
        operation(
            (
                "light",
                "directional",
                "Directional Light",
                Builtin::DirectionalLight,
            ),
            Palette::Artist,
            vec![
                (
                    "direction",
                    vector("Light direction", [0., -1., -1.], Unit::Unitless),
                ),
                (
                    "color",
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
                    ),
                ),
                ("illuminance", scalar("Illuminance (lux)", 1000.)),
            ],
            ResultType::Exact { ty: Type::Light },
            Cost::Uniform,
            vec![Domain::Collection],
        ),
        operation(
            ("scene", "spatial", "Spatial Scene", Builtin::SpatialScene),
            Palette::Artist,
            vec![
                ("camera", value("Active camera", exact(Type::Camera), None)),
                (
                    "children",
                    Port {
                        title: "Ordered contents".into(),
                        form: Form::List,
                        pattern: Pattern::ScenePart,
                        default: Some(Binding::List { values: vec![] }),
                    },
                ),
            ],
            ResultType::Exact { ty: Type::Scene },
            Cost::Collection,
            vec![Domain::Collection],
        ),
    ]
}
