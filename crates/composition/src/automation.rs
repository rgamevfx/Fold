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
        "operation_schemas":[fold_platform::authoring::reference_schema(serde_json::to_value(schemars::schema_for!(Edit)).expect("Composition edit schema"))],
        "operations":[
            {"op":"format","width":500,"height":888,"rate":[25,1],"frames":150},
            {"op":"layer","layer":"UUID","position":[100,200],"opacity":0.8},
            {"op":"range","layer":"UUID","start":{"numerator":0,"denominator":1},"end":{"numerator":6,"denominator":1}},
            {"op":"keyframe","layer":"UUID","channel":"position_y","time":{"numerator":1,"denominator":1},"value":100}
        ],
        "create":{"op":"create","id":"NEW_UUID","kind":crate::COMPOSITION,"name":"Composition"},
        "outputs":["video"],
        "notes":"Use ordinary batch edit operations. Layer positions are design pixels; time is exact Composition time. Moving both range edges equally moves the source clock; trimming retains it. Keyframe updates preserve key identity/tangents; new keys are Linear. Locks include locked ancestors. Format changes do not retime or resize existing source documents. Graph-owner metadata is synchronized by the application in the same transaction.",
        "layer_creation":"Use batch place/place_asset for existing sources, or app commands for native graph layers. catalog guide=rust-assets describes fold.app.create-rust-asset. fold.app.add-layer-graph arguments are {document: Composition UUID, kind: motion|composite, layer: optional selected layer UUID}."
    })
}
pub fn author(document: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut composition = Composition::from_document(document)?;
    for value in edits {
        let edit: Edit = fold_platform::authoring::decode(value)?;
        let target = match &edit {
            Edit::Format { .. } | Edit::Lock { .. } => None,
            Edit::Layer { layer, .. }
            | Edit::Range { layer, .. }
            | Edit::Keyframe { layer, .. } => Some(*layer),
        };
        if target.is_some_and(|id| composition.locked(id)) {
            return Err("Unlock the layer and its ancestors before editing".into());
        }
        match edit {
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
