//! Small, valid starter values for the qualified operation catalog.
use crate::{
    binding::{Binding, Combination, Interpolation, Key, Track},
    model::{Definition, Node, Reference},
    registry::{self, Builtin},
    types::{Literal, Type},
};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeMap;

pub(super) fn add(asset: &mut Definition, template: &str) -> Result<Vec<ObjectId>, String> {
    let (key, version, mode): (String, u32, String) =
        serde_json::from_str(template).map_err(|e| e.to_string())?;
    let operation = registry::resolve(&key, version, &mode)?;
    if operation.role != registry::Role::Operation || !operation.cpu_reference {
        return Err("Operation is not qualified for native authoring".into());
    }
    let scalar = |value| Binding::Literal {
        value: Literal::Scalar { value },
    };
    let mut bindings = BTreeMap::new();
    let mut ids = vec![];
    match operation.builtin {
        Builtin::Ramp => {
            let pair = if operation.mode == "scalar_linear" {
                [
                    Literal::Scalar { value: 0.2 },
                    Literal::Scalar { value: 1. },
                ]
            } else {
                [0., 1.].map(|v| Literal::Color {
                    value: [v, v, v, 1.],
                    encoding: crate::types::Encoding::LinearRec709,
                    alpha: crate::types::Alpha::Straight,
                })
            };
            let [left, right] = pair;
            bindings.insert(
                "stops".into(),
                Binding::Ramp {
                    ramp: crate::ramp::Ramp::two(left, right)?,
                },
            );
        }
        Builtin::TransformInstances
        | Builtin::InstanceScale
        | Builtin::InstanceColor
        | Builtin::InstanceRotationZ => {
            let template = serde_json::to_string(&("fold.motion.cloner", 1, "points"))
                .map_err(|e| e.to_string())?;
            ids = add(asset, &template)?;
            bindings.insert(
                "geometry".into(),
                Binding::Link {
                    reference: Reference::Node {
                        node: *ids.last().ok_or("Missing Animator input")?,
                    }
                    .into(),
                },
            );
        }
        Builtin::CameraPlaneViewport | Builtin::CameraPlanePhysical => {
            for (key, mode, parameter) in [
                ("fold.motion.shape", "rectangle", "geometry"),
                ("fold.motion.camera", "perspective", "camera"),
            ] {
                let template = serde_json::to_string(&(key, 1, mode)).map_err(|e| e.to_string())?;
                let dependencies = add(asset, &template)?;
                bindings.insert(
                    parameter.into(),
                    Binding::Link {
                        reference: Reference::Node {
                            node: *dependencies
                                .last()
                                .ok_or("Missing Camera Plane dependency")?,
                        }
                        .into(),
                    },
                );
                ids.extend(dependencies);
            }
        }
        Builtin::Cloner => {
            for (key, mode, parameter) in [
                ("fold.motion.mesh", "box", "prototype"),
                ("fold.motion.distribute", "grid", "points"),
            ] {
                let template = serde_json::to_string(&(key, 1, mode)).map_err(|e| e.to_string())?;
                let dependencies = add(asset, &template)?;
                bindings.insert(
                    parameter.into(),
                    Binding::Link {
                        reference: Reference::Node {
                            node: *dependencies.last().ok_or("Missing Cloner dependency")?,
                        }
                        .into(),
                    },
                );
                ids.extend(dependencies);
            }
        }
        Builtin::Expression => {
            bindings.insert("value".into(), scalar(0.));
        }
        Builtin::TweenLinear | Builtin::TweenCubicOut => {
            bindings.insert("from".into(), scalar(0.));
            bindings.insert("to".into(), scalar(1.));
        }
        Builtin::Keyframes => {
            bindings.insert(
                "track".into(),
                Binding::Track {
                    track: Track {
                        id: ObjectId::new(),
                        ty: Type::Scalar,
                        keys: vec![Key {
                            id: ObjectId::new(),
                            time: Time::ZERO,
                            value: Literal::Scalar { value: 0. },
                            interpolation: Interpolation::Linear,
                            handles: Default::default(),
                            extensions: Default::default(),
                        }],
                        combination: Combination::Replace,
                        base: None,
                        extensions: Default::default(),
                    },
                },
            );
        }
        Builtin::Extrude | Builtin::LitMaterial | Builtin::UnlitMaterial | Builtin::World => {
            let (key, mode, parameter) = if operation.builtin == Builtin::Extrude {
                ("fold.motion.outline_text", "curves", "profile")
            } else if operation.builtin == Builtin::World {
                ("fold.motion.material", "lit", "geometry")
            } else {
                ("fold.motion.extrude", "straight_text", "geometry")
            };
            let template = serde_json::to_string(&(key, 1, mode)).map_err(|e| e.to_string())?;
            ids = add(asset, &template)?;
            bindings.insert(
                parameter.into(),
                Binding::Link {
                    reference: Reference::Node {
                        node: *ids.last().ok_or("Missing geometry dependency")?,
                    }
                    .into(),
                },
            );
        }
        Builtin::TransformScreen
        | Builtin::TransformMesh
        | Builtin::Tessellate
        | Builtin::ExtrudeProfile
        | Builtin::Screen
        | Builtin::Fill
        | Builtin::Stroke
        | Builtin::StrokeButtRound
        | Builtin::StrokeRoundMiter
        | Builtin::StrokeRoundRound
        | Builtin::SpatialScene
        | Builtin::OutlineText
        | Builtin::TextAnimatorWords => {
            let (key, mode, title, parameter) = if matches!(
                operation.builtin,
                Builtin::OutlineText | Builtin::TextAnimatorWords
            ) {
                ("fold.motion.text", "packaged", "Text", "text")
            } else if operation.builtin == Builtin::TransformMesh {
                ("fold.motion.mesh", "box", "Box", "geometry")
            } else if operation.builtin != Builtin::SpatialScene {
                (
                    "fold.motion.shape",
                    "rectangle",
                    "Rectangle",
                    if matches!(
                        operation.builtin,
                        Builtin::Tessellate | Builtin::ExtrudeProfile
                    ) {
                        "profile"
                    } else {
                        "geometry"
                    },
                )
            } else {
                (
                    "fold.motion.camera",
                    "perspective",
                    "Perspective Camera",
                    "camera",
                )
            };
            let mut dependency =
                registry::resolve(key, 1, mode)?.node(title, Default::default())?;
            unique(asset, &mut dependency);
            bindings.insert(
                parameter.into(),
                Binding::Link {
                    reference: Reference::Node {
                        node: dependency.id,
                    }
                    .into(),
                },
            );
            ids.push(dependency.id);
            asset.nodes.push(dependency);
            if operation.builtin == Builtin::TextAnimatorWords {
                let position = |y| Binding::Literal {
                    value: Literal::Vector {
                        value: vec![0., y],
                        unit: crate::types::Unit::Canvas,
                        space: crate::types::Space::Screen,
                    },
                };
                let mut tween = registry::resolve("fold.motion.tween", 1, "cubic_out")?.node(
                    "Entrance",
                    [
                        ("from".into(), position(80.)),
                        ("to".into(), position(0.)),
                        (
                            "duration".into(),
                            Binding::Literal {
                                value: Literal::Duration {
                                    value: Time::new(3, 5).expect("constant duration"),
                                },
                            },
                        ),
                    ]
                    .into(),
                )?;
                unique(asset, &mut tween);
                bindings.insert(
                    "position".into(),
                    Binding::Link {
                        reference: Reference::Node { node: tween.id }.into(),
                    },
                );
                ids.push(tween.id);
                asset.nodes.push(tween);
            }
        }
        Builtin::RadialField
        | Builtin::LinearField
        | Builtin::InstanceRandom
        | Builtin::Grid
        | Builtin::BoxMesh
        | Builtin::SphereMesh
        | Builtin::Rectangle
        | Builtin::SceneTime
        | Builtin::AssetTime
        | Builtin::Scene
        | Builtin::PerspectiveCamera
        | Builtin::OrthographicCamera
        | Builtin::DirectionalLight => {}
        Builtin::Text => {}
        _ => return Err("No native starter for this operation".into()),
    }
    let mut node = operation.node(&operation.title, bindings)?;
    unique(asset, &mut node);
    ids.push(node.id);
    asset.nodes.push(node);
    Ok(ids)
}
fn unique(asset: &Definition, node: &mut Node) {
    let names: std::collections::BTreeSet<_> = asset
        .inputs
        .iter()
        .map(|i| i.name.as_str())
        .chain(asset.nodes.iter().map(|n| n.name.as_str()))
        .chain(asset.instances.iter().map(|i| i.name.as_str()))
        .chain(asset.outputs.iter().map(|o| o.name.as_str()))
        .collect();
    let stem = node.name.clone();
    let mut suffix = 2;
    while names.contains(node.name.as_str()) {
        node.name = format!("{stem} {suffix}");
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m5_geometry_starters_are_valid_native_constructions() {
        for (key, mode) in [
            ("stroke", "solid_centered_butt_round"),
            ("stroke", "solid_centered_round_miter"),
            ("stroke", "solid_centered_round_round"),
            ("transform", "screen"),
            ("transform", "world_mesh"),
            ("transform", "world_instances"),
            ("tessellate", "planar"),
            ("extrude", "closed_profile"),
        ] {
            let document = crate::package::create(fold_foundation::DocumentId::new()).unwrap();
            let mut construction = crate::model::Construction::from_document(&document).unwrap();
            let asset = &mut construction.definitions[0];
            let template = serde_json::to_string(&(format!("fold.motion.{key}"), 1, mode)).unwrap();
            let ids = add(asset, &template).unwrap();
            asset.outputs[0].ty = Type::Geometry;
            asset.outputs[0].value = Reference::Node {
                node: *ids.last().unwrap(),
            }
            .into();
            let output = asset.outputs[0].id;
            let prepared = crate::evaluation::Prepared::new(construction).unwrap();
            assert!(matches!(
                prepared
                    .evaluate(output, Time::ZERO, Default::default(), &|| Ok(()))
                    .unwrap(),
                crate::evaluation::Value::Geometry(_)
            ));
        }
    }
}
