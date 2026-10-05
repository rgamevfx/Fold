//! Scene commands reuse ordinary graph nodes; the scene owns ordering and visibility.
use super::*;
#[path = "assets.rs"]
pub mod assets;
#[path = "modifiers.rs"]
mod modifiers;
use crate::{
    graph::Graph,
    nodes::interface::GroupDefinition,
    scene::{Object, Scene},
};
use fold_foundation::Time;
use modifiers::reconnect_modifiers;
pub use modifiers::{
    add_modifier, expose_group_input, move_modifier, new_modifier_definition,
    new_path_modifier_definition, remove_modifier,
};
use std::collections::BTreeMap;

fn duration(m: &Motion) -> Result<Time, String> {
    Time::new(
        i64::from(m.info.frames) * i64::from(m.info.rate[1]),
        m.info.rate[0],
    )
    .map_err(|e| e.to_string())
}
/// Legacy networks stay intact as a procedural object when scene authoring begins.
pub fn ensure_scene(m: &mut Motion) -> Result<(), String> {
    if m.scene.is_some() {
        return Ok(());
    }
    let mut scene = Scene::default();
    let output = m.graph.node(m.graph.output.node)?;
    if output.inputs.contains_key("content") {
        let mut original = output.clone();
        original.id = ObjectId::new();
        let link = Link {
            node: original.id,
            socket: m.graph.output.socket.clone(),
        };
        m.graph.nodes.push(original.clone());
        scene.objects.push(Object {
            id: original.id,
            parent: None,
            name: "Original construction".into(),
            source: link.clone(),
            output: link,
            transform: None,
            appearance: None,
            modifiers: vec![],
            animation: Default::default(),
            offset: crate::geometry::IDENTITY,
            opacity: 1.,
            isolated: false,
            masks: vec![],
            constraints: vec![],
            visible: true,
            locked: false,
            start: Time::ZERO,
            end: duration(m)?,
            extensions: BTreeMap::new(),
        });
    }
    m.scene = Some(scene);
    Ok(())
}
pub fn create_object(m: &mut Motion, kind: &str) -> Result<ObjectId, String> {
    ensure_scene(m)?;
    let (source, appearance, transform) = super::content_nodes(m, kind)?;
    if kind == "fold.motion.path" {
        node(m, transform)?.set("translation", Datum::Vector([0.; 2]));
        node(m, source)?.settings = serde_json::json!({
            "segments":[{"Move":[50.,180.]},{"Cubic":[[220.,40.],[420.,320.],[590.,180.]]}],
            "attributes":{"color":[{"position":0.,"value":{"Color":[0.05,0.7,1.,1.]}},{"position":1.,"value":{"Color":[1.,0.12,0.4,1.]}}]}
        });
    }
    let name = crate::graph::registry::find(kind)?.name.to_owned();
    let end = duration(m)?;
    m.scene.as_mut().unwrap().objects.insert(
        0,
        Object {
            id: source,
            parent: None,
            name,
            source: Link {
                node: source,
                socket: "content".into(),
            },
            output: Link {
                node: transform,
                socket: "content".into(),
            },
            transform: Some(transform),
            appearance: Some(appearance),
            modifiers: vec![],
            animation: Default::default(),
            offset: crate::geometry::IDENTITY,
            opacity: 1.,
            isolated: false,
            masks: vec![],
            constraints: vec![],
            visible: true,
            locked: false,
            start: Time::ZERO,
            end,
            extensions: BTreeMap::new(),
        },
    );
    Ok(source)
}

pub fn create_procedural(m: &mut Motion) -> Result<ObjectId, String> {
    ensure_scene(m)?;
    // Empty group gives a valid transparent starting result; reference nodes can feed it.
    let mut empty = Node::new("fold.motion.empty")?;
    empty.position = [0., 0.];
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":"Content"});
    output.connect("value", empty.id, "content");
    output.position = [320., 0.];
    let link = Link {
        node: output.id,
        socket: "value".into(),
    };
    let group = ObjectId::new();
    m.groups.insert(
        group,
        GroupDefinition {
            name: "Procedural Object".into(),
            version: 1,
            inputs: vec![],
            outputs: BTreeMap::from([("content".into(), link.clone())]),
            graph: Graph {
                nodes: vec![empty, output],
                output: link,
            },
        },
    );
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.settings = serde_json::json!({"group":group});
    let id = instance.id;
    m.graph.nodes.push(instance);
    let mut transform = Node::new("fold.motion.transform")?;
    transform.connect("content", id, "content");
    let transform_id = transform.id;
    m.graph.nodes.push(transform);
    let end = duration(m)?;
    m.scene.as_mut().unwrap().objects.insert(
        0,
        Object {
            id,
            parent: None,
            name: "Procedural Object".into(),
            source: Link {
                node: id,
                socket: "content".into(),
            },
            output: Link {
                node: transform_id,
                socket: "content".into(),
            },
            transform: Some(transform_id),
            appearance: None,
            modifiers: vec![],
            animation: Default::default(),
            offset: crate::geometry::IDENTITY,
            opacity: 1.,
            isolated: false,
            masks: vec![],
            constraints: vec![],
            visible: true,
            locked: false,
            start: Time::ZERO,
            end,
            extensions: BTreeMap::new(),
        },
    );
    Ok(id)
}

/// Groups are ordinary transformable scene objects; children retain local transforms.
pub fn create_group(m: &mut Motion) -> Result<ObjectId, String> {
    let id = create_procedural(m)?;
    let node = node(m, id)?;
    let old = node
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group;
    node.kind = "fold.motion.scene_children".into();
    node.settings = serde_json::json!({"object":id});
    if let Some(old) = old {
        m.groups.remove(&old);
    }
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap()
        .name = "Group".into();
    Ok(id)
}

/// Reorder sibling objects or modifiers without inferring transform parenting.
pub fn reorder(
    m: &mut Motion,
    source: ObjectId,
    target: ObjectId,
    after: bool,
) -> Result<(), String> {
    let scene = m.scene.as_mut().ok_or("no scene")?;
    if let Some(from) = scene.objects.iter().position(|o| o.id == source) {
        let to = scene
            .objects
            .iter()
            .position(|o| o.id == target)
            .ok_or("drop an object beside another object")?;
        if scene.objects[from].locked {
            return Err("unlock this object before reordering".into());
        }
        if scene.objects[from].parent != scene.objects[to].parent {
            return Err("use Move into group to change parenting".into());
        }
        let object = scene.objects.remove(from);
        let to = scene
            .objects
            .iter()
            .position(|o| o.id == target)
            .ok_or("missing target")?;
        scene.objects.insert(to + usize::from(after), object);
        return Ok(());
    }
    let object = scene
        .objects
        .iter_mut()
        .find(|o| o.modifiers.contains(&source))
        .ok_or("this row cannot be reordered")?;
    if object.locked {
        return Err("unlock this object before reordering".into());
    }
    if !object.modifiers.contains(&target) {
        return Err("drop a modifier beside another modifier on the same object".into());
    }
    object.modifiers.retain(|&id| id != source);
    let to = object
        .modifiers
        .iter()
        .position(|&id| id == target)
        .unwrap();
    object.modifiers.insert(to + usize::from(after), source);
    let id = object.id;
    reconnect_modifiers(m, id)
}

/// Place a scene object in a group or at root, optionally beside a sibling.
/// Validate on a proposal so rejected drops leave the document untouched.
pub fn place_object(
    m: &mut Motion,
    source: ObjectId,
    parent: Option<ObjectId>,
    beside: Option<(ObjectId, bool)>,
    time: Time,
) -> Result<(), String> {
    let scene = m.scene.as_ref().ok_or("no scene")?;
    let object = scene
        .objects
        .iter()
        .find(|o| o.id == source)
        .ok_or("missing object")?;
    if object.locked {
        return Err("unlock this object before moving it".into());
    }
    let changes_parent = object.parent != parent;
    if let Some(parent) = parent {
        let group = scene
            .objects
            .iter()
            .find(|o| o.id == parent)
            .ok_or("missing group")?;
        if group.locked {
            return Err("unlock the destination group first".into());
        }
        if m.graph.node(group.source.node)?.kind != "fold.motion.scene_children" {
            return Err("drop onto a group to add a child".into());
        }
    }
    let mut proposal = m.clone();
    if changes_parent {
        reparent(&mut proposal, source, parent, time, true)?;
    }
    if let Some((target, after)) = beside {
        reorder(&mut proposal, source, target, after)?;
    } else {
        let objects = &mut proposal.scene.as_mut().unwrap().objects;
        let index = objects
            .iter()
            .position(|o| o.id == source)
            .ok_or("missing object")?;
        let object = objects.remove(index);
        objects.push(object);
    }
    proposal.validate()?;
    *m = proposal;
    Ok(())
}

/// Row-edge drops may change an object's parent; modifiers stay on their owner.
pub fn drop_beside(
    m: &mut Motion,
    source: ObjectId,
    target: ObjectId,
    after: bool,
    time: Time,
) -> Result<(), String> {
    if source == target {
        return Ok(());
    }
    let scene = m.scene.as_ref().ok_or("no scene")?;
    if let Some(object) = scene.objects.iter().find(|o| o.id == source) {
        let parent = scene
            .objects
            .iter()
            .find(|o| o.id == target)
            .ok_or("drop an object beside another object")?
            .parent;
        if object.parent == parent {
            reorder(m, source, target, after)
        } else {
            place_object(m, source, parent, Some((target, after)), time)
        }
    } else {
        reorder(m, source, target, after)
    }
}

/// Preserve current placement through an explicit affine offset, without baking animation.
pub fn reparent(
    m: &mut Motion,
    id: ObjectId,
    parent: Option<ObjectId>,
    time: Time,
    preserve: bool,
) -> Result<(), String> {
    let object = m
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == id))
        .ok_or("missing object")?;
    if object.locked {
        return Err("unlock this object before reparenting".into());
    }
    let offset = if preserve {
        let mut e = crate::evaluation::Evaluator::new(m, time)?;
        let old_parent = if let Some(p) = object.parent {
            e.world_matrix(p)?
        } else {
            crate::geometry::IDENTITY
        };
        let parent_world = if let Some(p) = parent {
            e.world_matrix(p)?
        } else {
            crate::geometry::IDENTITY
        };
        crate::geometry::multiply(
            crate::geometry::multiply(crate::geometry::inverse(parent_world)?, old_parent),
            object.offset,
        )
    } else {
        object.offset
    };
    let mut proposal = m.clone();
    let object = proposal
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap();
    object.parent = parent;
    object.offset = offset;
    proposal.validate()?;
    *m = proposal;
    Ok(())
}

pub fn add_constraint(
    m: &mut Motion,
    id: ObjectId,
    target: ObjectId,
    kind: crate::scene::ConstraintKind,
    time: Time,
    preserve: bool,
) -> Result<(), String> {
    let mut c = crate::scene::Constraint::new(target, kind);
    if preserve && kind == crate::scene::ConstraintKind::Follow {
        let mut e = crate::evaluation::Evaluator::new(m, time)?;
        c.offset = crate::geometry::multiply(
            crate::geometry::inverse(e.world_matrix(target)?)?,
            e.world_matrix(id)?,
        );
    }
    let mut proposal = m.clone();
    let o = proposal
        .scene
        .as_mut()
        .and_then(|s| s.objects.iter_mut().find(|o| o.id == id))
        .ok_or("missing object")?;
    if o.locked {
        return Err("unlock this object before editing constraints".into());
    }
    o.constraints.push(c);
    proposal.validate()?;
    *m = proposal;
    Ok(())
}
