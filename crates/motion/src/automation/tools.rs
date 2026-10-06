//! Public creative operations use the same constructions as the artist's shelf.
use crate::{
    Motion,
    authoring::scene::{self, assets},
    graph::Input,
};
use fold_foundation::{ObjectId, Time};
use serde::Deserialize;
use serde_json::Value;
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Badge {
        id: ObjectId,
        text: ObjectId,
        background: ObjectId,
        name: String,
    },
    SceneGroup {
        id: ObjectId,
        name: String,
    },
    Duplicator {
        id: ObjectId,
        source: ObjectId,
        name: String,
    },
    Driver {
        id: ObjectId,
        target: ObjectId,
        input: String,
        kind: Driver,
    },
    Ramp {
        node: ObjectId,
        settings: Value,
    },
    Parent {
        object: ObjectId,
        parent: Option<ObjectId>,
        time: Time,
    },
    Reference {
        node: ObjectId,
        input: String,
        object: ObjectId,
        local: bool,
    },
    Style {
        id: ObjectId,
        object: ObjectId,
        kind: String,
    },
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Driver {
    Oscillator,
    ColorRamp,
}
pub(super) fn accepts(op: &str) -> bool {
    matches!(
        op,
        "badge"
            | "scene_group"
            | "duplicator"
            | "driver"
            | "ramp"
            | "parent"
            | "reference"
            | "style"
    )
}
pub(super) fn schema() -> Value {
    fold_platform::authoring::reference_schema(
        serde_json::to_value(schemars::schema_for!(Edit)).unwrap(),
    )
}
fn available(m: &Motion, ids: &[ObjectId]) -> Result<(), String> {
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    if unique.len() != ids.len()
        || ids
            .iter()
            .any(|id| m.graph.nodes.iter().any(|n| n.id == *id) || m.groups.contains_key(id))
    {
        return Err("tool identities must be new and distinct".into());
    }
    Ok(())
}
/// Remap only graph references and scene membership, never literal artwork strings.
fn rename(m: &mut Motion, old: ObjectId, id: ObjectId, name: Option<String>) {
    for n in &mut m.graph.nodes {
        if n.id == old {
            n.id = id;
        }
        for input in n.inputs.values_mut() {
            if let Input::Link(l) = input
                && l.node == old
            {
                l.node = id;
            }
        }
        if matches!(
            n.kind.as_str(),
            "fold.motion.scene_children" | "fold.motion.object_reference"
        ) && n.settings["object"] == serde_json::json!(old)
        {
            n.settings["object"] = serde_json::json!(id);
        }
    }
    if m.graph.output.node == old {
        m.graph.output.node = id;
    }
    if let Some(s) = &mut m.scene {
        if s.output == Some(old) {
            s.output = Some(id);
        }
        for o in &mut s.objects {
            if o.id == old {
                o.id = id;
                if let Some(name) = &name {
                    o.name = name.clone();
                }
            }
            if o.source.node == old {
                o.source.node = id;
            }
            if o.parent == Some(old) {
                o.parent = Some(id);
            }
            for modifier in &mut o.modifiers {
                if *modifier == old {
                    *modifier = id;
                }
            }
        }
    }
}
pub(super) fn apply(m: &mut Motion, v: &Value) -> Result<(), String> {
    match fold_platform::authoring::decode(v)? {
        Edit::Badge {
            id,
            text,
            background,
            name,
        } => {
            available(m, &[id, text, background])?;
            let old = assets::badge(m)?;
            let children: Vec<_> = m
                .scene
                .as_ref()
                .unwrap()
                .objects
                .iter()
                .filter(|o| o.parent == Some(old))
                .map(|o| o.id)
                .collect();
            for child in children {
                let dest = if m.graph.node(child)?.kind == "fold.motion.text" {
                    text
                } else {
                    background
                };
                rename(m, child, dest, None);
            }
            rename(m, old, id, Some(name));
        }
        Edit::SceneGroup { id, name } => {
            available(m, &[id])?;
            let old = scene::create_group(m)?;
            rename(m, old, id, Some(name));
        }
        Edit::Duplicator { id, source, name } => {
            available(m, &[id])?;
            let old = assets::duplicate(m, source)?;
            rename(m, old, id, Some(name));
        }
        Edit::Driver {
            id,
            target,
            input,
            kind,
        } => {
            available(m, &[id])?;
            if matches!(
                m.graph.node(target)?.inputs.get(&input),
                Some(Input::Link(_))
            ) {
                return Err(
                    "input already driven; edit its driver or explicitly disconnect first".into(),
                );
            }
            let old = match kind {
                Driver::Oscillator => assets::attach_oscillator(m, target, &input)?,
                Driver::ColorRamp => assets::attach_color_ramp(m, target, &input)?,
            };
            rename(m, old, id, None);
        }
        Edit::Ramp { node: id, settings } => assets::set_color_ramp(
            m,
            id,
            serde_json::from_value(settings).map_err(|e| e.to_string())?,
        )?,
        Edit::Parent {
            object,
            parent,
            time,
        } => scene::place_object(m, object, parent, None, time)?,
        Edit::Reference {
            node: id,
            input,
            object,
            local,
        } => assets::set_reference(m, id, &input, object, local)?,
        Edit::Style { id, object, kind } => {
            available(m, &[id])?;
            let old = assets::apply_style(m, object, &kind)?;
            rename(m, old, id, None);
        }
    }
    Ok(())
}
