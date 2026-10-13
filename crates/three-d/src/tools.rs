//! Native creation recipes; artist adapters submit these same guarded edits.
use crate::{
    editing::{self, Edit, Placement},
    model::{Content, Geometry, Material, Object, Scene},
};
use fold_foundation::ObjectId;

#[derive(Clone, Copy)]
pub enum Kind {
    Box,
    Sphere,
    Group,
    Camera,
    Light,
}
/// Reparent without rewriting authored local transforms or animation.
pub fn move_to(scene: &Scene, id: ObjectId, parent: Option<ObjectId>) -> Result<Vec<Edit>, String> {
    editing::editable(scene, id)?;
    let expected = editing::placement(scene, id)?;
    if expected.parent == parent {
        return Ok(vec![]);
    }
    let index = match parent {
        None => scene.roots.len(),
        Some(parent) => {
            editing::editable(scene, parent)?;
            match &scene
                .objects
                .get(&parent)
                .ok_or("Missing 3D parent")?
                .content
            {
                Content::Group { children, .. } => children.len(),
                _ => return Err("3D parent is not a group".into()),
            }
        }
    };
    let edits = vec![Edit::Move {
        id,
        expected,
        value: Placement { parent, index },
    }];
    editing::apply(scene, &edits, &Default::default())?;
    Ok(edits)
}
/// Preserve the entire world animation with exact parent-space compensation.
/// Later edits to the new parent apply relative to its captured motion.
pub fn move_to_world(
    scene: &Scene,
    id: ObjectId,
    parent: Option<ObjectId>,
) -> Result<Vec<Edit>, String> {
    let mut edits = move_to(scene, id, parent)?;
    if edits.is_empty() {
        return Ok(edits);
    }
    let old_parent = editing::placement(scene, id)?.parent;
    let mut compensation = parent_motion(scene, parent)?;
    compensation.reverse();
    for factor in &mut compensation {
        factor.inverse = !factor.inverse;
    }
    compensation.extend(parent_motion(scene, old_parent)?);
    let expected = &scene.objects[&id];
    compensation.extend(expected.transform.parent_space.clone());
    let mut value = expected.clone();
    value.transform.parent_space = compensation;
    edits.push(Edit::Replace {
        id,
        expected: Box::new(expected.clone()),
        value: Box::new(value),
    });
    editing::apply(scene, &edits, &Default::default())?;
    Ok(edits)
}
fn fresh_binding(binding: &mut fold_graph::binding::Binding) {
    if let fold_graph::binding::Binding::Track { track } = binding {
        track.id = ObjectId::new();
        for key in &mut track.keys {
            key.id = ObjectId::new();
        }
    }
}
/// Flatten a frozen hierarchy. It contains no live object references or cycles.
fn parent_motion(
    scene: &Scene,
    mut id: Option<ObjectId>,
) -> Result<Vec<crate::model::ParentTransform>, String> {
    let mut chain = vec![];
    while let Some(current) = id {
        if chain.len() > 128 {
            return Err("3D hierarchy depth exceeds 128".into());
        }
        chain.push(
            &scene
                .objects
                .get(&current)
                .ok_or("Missing 3D parent")?
                .transform,
        );
        id = editing::placement(scene, current)?.parent;
    }
    let mut factors = vec![];
    for transform in chain.into_iter().rev() {
        factors.extend(transform.parent_space.clone());
        let factor = crate::model::ParentTransform {
            inverse: false,
            channels: [
                transform.position.binding()?,
                transform.rotation.binding()?,
                transform.scale.binding()?,
            ],
            extensions: Default::default(),
        };
        // Identity groups need no stored compensation. Never infer a constant
        // transform from one sample of a track.
        if !factor
            .channels
            .iter()
            .all(|binding| matches!(binding, fold_graph::binding::Binding::Literal { .. }))
            || crate::evaluation::parent_factor(&factor, fold_foundation::Time::ZERO)?
                != fold_foundation::affine::Affine::IDENTITY
        {
            factors.push(factor);
        }
        if factors.len() > 512 {
            return Err("3D parent compensation exceeds 512 factors".into());
        }
    }
    for factor in &mut factors {
        for binding in &mut factor.channels {
            fresh_binding(binding);
        }
    }
    Ok(factors)
}
/// Copy a hierarchy with independent animation identities and shared resources.
/// The returned inserts use the same public edit contract as agent authoring.
pub fn duplicate(scene: &Scene, id: ObjectId) -> Result<(ObjectId, Vec<Edit>), String> {
    scene.validate()?;
    let placement = editing::placement(scene, id)?;
    if let Some(parent) = placement.parent {
        editing::editable(scene, parent)?;
    }
    fn copy(
        scene: &Scene,
        source: ObjectId,
        objects: &mut std::collections::BTreeMap<ObjectId, Object>,
    ) -> Result<ObjectId, String> {
        let mut object = scene.objects[&source].clone();
        let id = ObjectId::new();
        for parameter in [
            &mut object.transform.position,
            &mut object.transform.rotation,
            &mut object.transform.scale,
        ] {
            parameter.id = ObjectId::new();
            let mut binding = parameter.binding()?;
            fresh_binding(&mut binding);
            parameter.value = serde_json::to_value(binding).map_err(|error| error.to_string())?;
        }
        for factor in &mut object.transform.parent_space {
            for binding in &mut factor.channels {
                fresh_binding(binding);
            }
        }
        if let Content::Group { children, .. } = &mut object.content {
            for child in children {
                *child = copy(scene, *child, objects)?;
            }
        }
        objects.insert(id, object);
        Ok(id)
    }
    let mut objects = std::collections::BTreeMap::new();
    let copied = copy(scene, id, &mut objects)?;
    let edits = vec![Edit::InsertTree {
        root: copied,
        objects,
        placement: Placement {
            index: placement.index + 1,
            ..placement
        },
    }];
    editing::apply(scene, &edits, &Default::default())?;
    Ok((copied, edits))
}

/// Detach one instance's resource without altering other instances or animation.
pub fn make_unique(scene: &Scene, id: ObjectId, geometry: bool) -> Result<Vec<Edit>, String> {
    editing::editable(scene, id)?;
    let expected = scene.objects.get(&id).ok_or("Missing 3D object")?;
    let mut object = expected.clone();
    let Content::Instance {
        geometry: mesh,
        material,
        ..
    } = &mut object.content
    else {
        return Err("Select a 3D instance".into());
    };
    let resource = if geometry { mesh } else { material };
    let shared = scene
        .objects
        .values()
        .filter(|object| {
            matches!(object.content,
        Content::Instance { geometry: mesh, material, .. }
        if (if geometry { mesh } else { material }) == *resource)
        })
        .count();
    if shared < 2 {
        return Ok(vec![]);
    }
    let next = ObjectId::new();
    let edit = if geometry {
        Edit::Geometry {
            id: next,
            expected: None,
            value: Some(scene.geometry[resource].clone()),
        }
    } else {
        Edit::Material {
            id: next,
            expected: None,
            value: Some(scene.materials[resource].clone()),
        }
    };
    *resource = next;
    let edits = vec![
        edit,
        Edit::Replace {
            id,
            expected: Box::new(expected.clone()),
            value: Box::new(object),
        },
    ];
    editing::apply(scene, &edits, &Default::default())?;
    Ok(edits)
}

pub fn group(scene: &Scene, selected: &[ObjectId]) -> Result<(ObjectId, Vec<Edit>), String> {
    if selected.is_empty() {
        return Err("Select objects to group".into());
    }
    let mut members = selected
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|id| {
            editing::editable(scene, id)?;
            Ok((id, editing::placement(scene, id)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let parent = members[0].1.parent;
    if members
        .iter()
        .any(|(_, placement)| placement.parent != parent)
    {
        return Err("Select objects with the same parent to group".into());
    }
    members.sort_by_key(|(_, placement)| placement.index);
    let (group, mut edits) = create(scene, Kind::Group, parent)?;
    if let Edit::Insert { placement, .. } = edits.last_mut().unwrap() {
        placement.index = members[0].1.index;
    }
    for (index, (id, placement)) in members.into_iter().enumerate() {
        edits.push(Edit::Move {
            id,
            expected: Placement {
                parent,
                index: placement.index + 1 - index,
            },
            value: Placement {
                parent: Some(group),
                index,
            },
        });
    }
    editing::apply(scene, &edits, &Default::default())?;
    Ok((group, edits))
}
pub fn delete(scene: &Scene, id: ObjectId) -> Result<Vec<Edit>, String> {
    let object = scene.objects.get(&id).ok_or("Missing 3D object")?;
    let mut edits = vec![Edit::Delete {
        id,
        expected: Box::new(object.clone()),
    }];
    let after = editing::apply(scene, &edits, &Default::default())?;
    let mut geometry = std::collections::BTreeSet::new();
    let mut materials = std::collections::BTreeSet::new();
    for (id, object) in &scene.objects {
        if !after.objects.contains_key(id)
            && let Content::Instance {
                geometry: mesh,
                material,
                ..
            } = object.content
        {
            geometry.insert(mesh);
            materials.insert(material);
        }
    }
    for object in after.objects.values() {
        if let Content::Instance {
            geometry: mesh,
            material,
            ..
        } = object.content
        {
            geometry.remove(&mesh);
            materials.remove(&material);
        }
    }
    edits.extend(geometry.into_iter().map(|id| Edit::Geometry {
        id,
        expected: Some(scene.geometry[&id].clone()),
        value: None,
    }));
    edits.extend(materials.into_iter().map(|id| Edit::Material {
        id,
        expected: Some(scene.materials[&id].clone()),
        value: None,
    }));
    editing::apply(scene, &edits, &Default::default())?;
    Ok(edits)
}
pub fn create(
    scene: &Scene,
    kind: Kind,
    parent: Option<ObjectId>,
) -> Result<(ObjectId, Vec<Edit>), String> {
    let index = match parent {
        None => scene.roots.len(),
        Some(id) => {
            editing::editable(scene, id)?;
            match &scene.objects[&id].content {
                Content::Group { children, .. } => children.len(),
                _ => return Err("Create objects inside a group or at scene root".into()),
            }
        }
    };
    let id = ObjectId::new();
    let mut edits = vec![];
    let mut object = match kind {
        Kind::Camera => Object::camera(),
        Kind::Light => Object::light(),
        Kind::Group => Object::new(
            "Group",
            Content::Group {
                children: vec![],
                extensions: Default::default(),
            },
        ),
        Kind::Box | Kind::Sphere => {
            let geometry = ObjectId::new();
            let material = ObjectId::new();
            let (name, shape) = match kind {
                Kind::Box => ("Box", Geometry::default()),
                _ => (
                    "Sphere",
                    Geometry::Sphere {
                        radius: 0.5,
                        extensions: Default::default(),
                    },
                ),
            };
            edits.push(Edit::Geometry {
                id: geometry,
                expected: None,
                value: Some(shape),
            });
            edits.push(Edit::Material {
                id: material,
                expected: None,
                value: Some(Material::default()),
            });
            Object::new(
                name,
                Content::Instance {
                    geometry,
                    material,
                    extensions: Default::default(),
                },
            )
        }
    };
    let names = scene
        .objects
        .values()
        .map(|object| object.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if names.contains(object.name.as_str()) {
        let base = object.name.clone();
        let mut suffix = 2;
        loop {
            let name = format!("{base} {suffix}");
            if !names.contains(name.as_str()) {
                object.name = name;
                break;
            }
            suffix += 1;
        }
    }
    edits.push(Edit::Insert {
        id,
        object: Box::new(object),
        placement: Placement { parent, index },
    });
    editing::apply(scene, &edits, &Default::default())?;
    Ok((id, edits))
}

/// Translate one object in world space at the exact authored time. Parent motion
/// and nonuniform scale are removed before updating its local position channel.
pub fn translate_world(
    scene: &Scene,
    id: ObjectId,
    time: fold_foundation::Time,
    delta: [f64; 3],
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    use fold_foundation::affine::Affine;
    use fold_graph::types::Literal;
    editing::editable(scene, id)?;
    if delta.iter().any(|v| !v.is_finite()) {
        return Err("Invalid 3D movement".into());
    }
    let parent = editing::placement(scene, id)?
        .parent
        .map(|parent| crate::interaction::world(scene, parent, time))
        .transpose()?
        .unwrap_or(Affine::IDENTITY);
    let parent = parent.compose(crate::evaluation::parent_space(
        &scene.objects[&id].transform,
        time,
    )?);
    let delta = parent.inverse()?.vector(delta);
    let expected = &scene.objects[&id].transform.position;
    let mut literal = crate::evaluation::sample(expected, time)?;
    let Literal::Vector { value, .. } = &mut literal else {
        return Err("Invalid position channel".into());
    };
    for (value, delta) in value.iter_mut().zip(delta) {
        *value += delta;
    }
    parameter_edit(scene, id, time, expected, literal, auto_key)
}

/// Rotate around an object's local axis. Parent transforms remain unchanged.
pub fn rotate_local(
    scene: &Scene,
    id: ObjectId,
    time: fold_foundation::Time,
    axis: usize,
    radians: f64,
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    use fold_graph::types::Literal;
    editing::editable(scene, id)?;
    if axis > 2 || !radians.is_finite() {
        return Err("Invalid local rotation".into());
    }
    let expected = &scene.objects[&id].transform.rotation;
    let mut literal = crate::evaluation::sample(expected, time)?;
    let Literal::Rotation { value, .. } = &mut literal else {
        return Err("Invalid rotation channel".into());
    };
    let q = *value;
    let mut r = [0.; 4];
    r[axis] = (radians * 0.5).sin();
    r[3] = (radians * 0.5).cos();
    *value = [
        q[3] * r[0] + q[0] * r[3] + q[1] * r[2] - q[2] * r[1],
        q[3] * r[1] - q[0] * r[2] + q[1] * r[3] + q[2] * r[0],
        q[3] * r[2] + q[0] * r[1] - q[1] * r[0] + q[2] * r[3],
        q[3] * r[3] - q[0] * r[0] - q[1] * r[1] - q[2] * r[2],
    ];
    parameter_edit(scene, id, time, expected, literal, auto_key)
}

/// Scale local channels without changing orientation, position or parent state.
pub fn scale_local(
    scene: &Scene,
    id: ObjectId,
    time: fold_foundation::Time,
    factors: [f64; 3],
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    use fold_graph::types::Literal;
    editing::editable(scene, id)?;
    if factors.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("Invalid local scale".into());
    }
    let expected = &scene.objects[&id].transform.scale;
    let mut literal = crate::evaluation::sample(expected, time)?;
    let Literal::Vector { value, .. } = &mut literal else {
        return Err("Invalid scale channel".into());
    };
    for (value, factor) in value.iter_mut().zip(factors) {
        *value *= factor;
    }
    parameter_edit(scene, id, time, expected, literal, auto_key)
}

fn parameter_edit(
    scene: &Scene,
    id: ObjectId,
    time: fold_foundation::Time,
    expected: &fold_graph::model::Parameter,
    literal: fold_graph::types::Literal,
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    let mut value = expected.clone();
    value.value = serde_json::to_value(crate::controls::edited_binding(
        expected.binding()?,
        literal,
        time,
        auto_key,
    )?)
    .map_err(|e| e.to_string())?;
    let edits = vec![Edit::Parameter {
        object: id,
        expected: expected.clone(),
        value,
    }];
    editing::apply(scene, &edits, &Default::default())?;
    Ok(edits)
}
