//! Targeted common-layer authoring. The application synchronizes graph owners in
//! the same transaction; this package only interprets common composition state.
use crate::Composition;
use fold_foundation::{ObjectId, Time};
use fold_project::Document;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Edit {
    Publish {
        definition: crate::reusable::Definition,
    },
    Effects {
        layer: ObjectId,
        effects: Vec<fold_platform::effects::Instance>,
    },
    Adjustment {
        layer: ObjectId,
        enabled: bool,
    },
    Format {
        width: u32,
        height: u32,
        rate: [u32; 2],
        frames: u32,
    },
    Layer {
        layer: ObjectId,
        name: Option<String>,
        visible: Option<bool>,
        opacity: Option<f64>,
        position: Option<[f64; 2]>,
    },
    Lock {
        layer: ObjectId,
        locked: bool,
    },
    Range {
        layer: ObjectId,
        start: Time,
        end: Time,
    },
    Keyframe {
        layer: ObjectId,
        channel: Channel,
        time: Time,
        value: f64,
    },
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum Channel {
    PositionX,
    PositionY,
    Opacity,
}
impl Channel {
    fn path(&self) -> &'static str {
        match self {
            Self::PositionX => "transform.4",
            Self::PositionY => "transform.5",
            Self::Opacity => "scene.opacity.0",
        }
    }
}
pub fn catalog() -> Value {
    json!({
        "operation_schemas": [schemars::schema_for!(Edit)],
        "outputs": ["video"],
        "notes": "Layer positions are design pixels; time is exact Composition time. Identities are literal UUIDs. Moving both range edges equally moves the source clock; trimming retains it. Keyframe updates preserve key identity/tangents; new keys are Linear. Locks include locked ancestors. Format changes do not retime or resize existing source documents."
    })
}
pub fn author(document: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut composition = Composition::from_document(document)?;
    for value in edits {
        let edit: Edit = fold_platform::authoring::decode(value)?;
        let target = match &edit {
            Edit::Publish { .. } | Edit::Format { .. } | Edit::Lock { .. } => None,
            Edit::Effects { layer, .. }
            | Edit::Adjustment { layer, .. }
            | Edit::Layer { layer, .. }
            | Edit::Range { layer, .. }
            | Edit::Keyframe { layer, .. } => Some(*layer),
        };
        if target.is_some_and(|id| composition.locked(id)) {
            return Err("Unlock the layer and its ancestors before editing".into());
        }
        match edit {
            Edit::Publish { definition } => {
                composition.extensions.insert(
                    crate::reusable::KEY.into(),
                    serde_json::to_value(definition).map_err(|e| e.to_string())?,
                );
            }
            Edit::Effects { layer, effects } => composition.layer_mut(layer)?.effects = effects,
            Edit::Adjustment { layer, enabled } => {
                composition.layer_mut(layer)?.adjustment = enabled
            }
            Edit::Format {
                width,
                height,
                rate,
                frames,
            } => {
                composition.info = fold_media::VideoInfo {
                    width,
                    height,
                    rate,
                    frames,
                };
            }
            Edit::Layer {
                layer,
                name,
                visible,
                opacity,
                position,
            } => {
                let layer = composition.layer_mut(layer)?;
                if let Some(name) = name {
                    layer.name = name;
                }
                if let Some(visible) = visible {
                    layer.visible = visible;
                }
                if let Some(opacity) = opacity {
                    layer.opacity = opacity;
                }
                if let Some([x, y]) = position {
                    layer.offset[4] = x;
                    layer.offset[5] = y;
                }
            }
            Edit::Lock { layer, locked } => composition.layer_mut(layer)?.locked = locked,
            Edit::Range { layer, start, end } => {
                composition.layer_mut(layer)?.set_range(start, end)?
            }
            Edit::Keyframe {
                layer,
                channel,
                time,
                value,
            } => {
                if !value.is_finite()
                    || matches!(channel, Channel::Opacity) && !(0. ..=1.).contains(&value)
                {
                    return Err("Invalid keyframe value".into());
                }
                composition
                    .layer_mut(layer)?
                    .animation
                    .entry(channel.path().into())
                    .or_default()
                    .insert(time, value);
            }
        }
    }
    let mut changed = composition.document(document.id)?;
    changed.extensions = document.extensions.clone();
    Ok(changed)
}
