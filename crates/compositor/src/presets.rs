//! Library entries are ordinary retained graphs with promoted interfaces.
use crate::{
    Composite, Node, OperationConfig as P,
    reusable::{Binding, Definition, KEY},
    shader::Shader,
};
use fold_foundation::DocumentId;
use fold_platform::effects::{Control, Interface};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Preset {
    CustomEffect,
    Glow,
    Dissolve,
    CircularWipe,
    FilmLook,
    Grain,
}
impl Preset {
    pub fn label(self) -> &'static str {
        match self {
            Self::CustomEffect => "Custom Effect",
            Self::Glow => "Glow",
            Self::Dissolve => "Dissolve",
            Self::CircularWipe => "Circular Wipe",
            Self::FilmLook => "Film Look",
            Self::Grain => "Grain",
        }
    }
}
fn control(id: &str, label: &str, default: Vec<f64>, range: Option<[f64; 2]>) -> Control {
    Control {
        id: id.into(),
        label: label.into(),
        default,
        range,
        extensions: Default::default(),
    }
}
pub fn create(preset: Preset, id: DocumentId) -> Result<fold_project::Document, String> {
    let images: Vec<String> = if matches!(preset, Preset::Dissolve | Preset::CircularWipe) {
        vec!["outgoing".into(), "incoming".into()]
    } else {
        vec!["source".into()]
    };
    let mut nodes: Vec<_> = images
        .iter()
        .map(|id| Node::new(P::Input { id: id.clone() }, vec![]))
        .collect();
    let mut controls = vec![];
    let mut bindings = vec![];
    match preset {
        Preset::CustomEffect => {}
        Preset::Dissolve => nodes.push(Node::new(
            P::Dissolve {
                curve: Default::default(),
            },
            nodes.iter().map(|n| n.id).collect(),
        )),
        Preset::Glow => {
            let threshold = control("threshold", "Threshold", vec![0.75], Some([0., 16.]));
            let strength = control("strength", "Strength", vec![1.], Some([0., 10.]));
            let extract=Node::new(P::Shader {settings:Shader {
                images:images.clone(),controls:vec![threshold.clone(),strength.clone()],
                source:"fn effect(uv:vec2<f32>)->vec4<f32>{let p=source(uv);let bright=max(max(p.r,p.g),p.b);let weight=clamp((bright-threshold()*p.a)/max(bright,0.000001),0.,1.);return vec4<f32>(p.rgb*weight*strength(),p.a*weight);}".into(),
            }},vec![nodes[0].id]);
            for c in [threshold, strength] {
                bindings.push(Binding {
                    extensions: Default::default(),
                    control: c.id.clone(),
                    node: extract.id,
                    property: c.id.clone(),
                });
                controls.push(c);
            }
            let blur = Node::new(
                P::Blur {
                    filter: Default::default(),
                    size: [16.; 2],
                    edges: fold_render::operations::Edges::Transparent,
                },
                vec![extract.id],
            );
            let radius = control("radius", "Radius", vec![16.; 2], Some([0., 256.]));
            bindings.push(Binding {
                extensions: Default::default(),
                control: radius.id.clone(),
                node: blur.id,
                property: "size".into(),
            });
            controls.push(radius);
            let merge = Node::new(
                P::Composite {
                    mode: fold_render::operations::MergeMode::Plus,
                    channels: Default::default(),
                },
                vec![blur.id, nodes[0].id],
            );
            nodes.extend([extract, blur, merge]);
        }
        Preset::FilmLook | Preset::Grain => {
            let config = if matches!(preset, Preset::FilmLook) {
                P::FilmEmulation {
                    settings: Default::default(),
                }
            } else {
                P::Grain {
                    settings: Default::default(),
                }
            };
            let parameters = config.animated_properties();
            let node = Node::new(config, vec![nodes[0].id]);
            for p in parameters {
                controls.push(control(&p.id, &p.label, p.values, p.range));
                bindings.push(Binding {
                    extensions: Default::default(),
                    control: p.id.clone(),
                    node: node.id,
                    property: p.id,
                });
            }
            nodes.push(node);
        }
        preset => {
            let (source, parameters) = match preset {
                Preset::CircularWipe => (
                    "fn effect(uv:vec2<f32>)->vec4<f32>{let p=fold_params.clock.y;if(p<=0.){return outgoing(uv);}if(p>=1.){return incoming(uv);}let size=fold_params.size.xy;let c=center()*size;let extent=max(abs(c),abs(size-c));let radius=length(extent)*p;let distance=length(uv*size-c);let mask=1.-smoothstep(radius-0.5,radius+0.5,distance);return mix(outgoing(uv),incoming(uv),mask);}",
                    vec![control("center", "Center", vec![0.5; 2], Some([0., 1.]))],
                ),
                _ => unreachable!(),
            };
            let shader = Node::new(
                P::Shader {
                    settings: Shader {
                        source: source.into(),
                        images: images.clone(),
                        controls: parameters.clone(),
                    },
                },
                nodes.iter().map(|n| n.id).collect(),
            );
            for c in parameters {
                bindings.push(Binding {
                    extensions: Default::default(),
                    control: c.id.clone(),
                    node: shader.id,
                    property: c.id.clone(),
                });
                controls.push(c);
            }
            nodes.push(shader);
        }
    }
    let output = Node::new(P::Output, vec![nodes.last().unwrap().id]);
    let output_id = output.id;
    nodes.push(output);
    let mut composite = Composite {
        info: fold_media::VideoInfo {
            width: 1920,
            height: 1080,
            rate: [24, 1],
            frames: 24,
        },
        nodes,
        output: output_id,
        extensions: Default::default(),
    };
    composite.extensions.insert(
        KEY.into(),
        serde_json::to_value(Definition {
            extensions: Default::default(),
            interface: Interface {
                version: 1,
                images,
                controls,
                extensions: Default::default(),
            },
            bindings,
        })
        .map_err(|e| e.to_string())?,
    );
    composite.auto_layout()?;
    composite.document(id)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    id: DocumentId,
    preset: Preset,
}
pub(crate) fn command(
    snapshot: &fold_project::Snapshot,
    args: &[u8],
    cancel: &fold_media::Cancel,
) -> Result<fold_project::EditBatch, String> {
    cancel.check()?;
    let args: Create = serde_json::from_slice(args).map_err(|e| e.to_string())?;
    if snapshot.state().documents.contains_key(&args.id) {
        return Err("Effect document already exists".into());
    }
    Ok(fold_project::EditBatch {
        base: snapshot.revision(),
        mutations: vec![
            fold_project::Mutation::PutDocument(create(args.preset, args.id)?),
            fold_project::Mutation::PutItem(fold_project::Item {
                id: fold_project::ItemId::Document(args.id),
                name: args.preset.label().into(),
                parent: Default::default(),
                order: snapshot.state().organization.items.len() as u64,
                extensions: Default::default(),
            }),
        ],
    })
}
