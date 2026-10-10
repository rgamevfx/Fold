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
                },
                vec![blur.id, nodes[0].id],
            );
            nodes.extend([extract, blur, merge]);
        }
        preset => {
            let (source, parameters) = match preset {
                Preset::CircularWipe => (
                    "fn effect(uv:vec2<f32>)->vec4<f32>{let p=fold_params.clock.y;if(p<=0.){return outgoing(uv);}if(p>=1.){return incoming(uv);}let size=fold_params.size.xy;let c=center()*size;let extent=max(abs(c),abs(size-c));let radius=length(extent)*p;let distance=length(uv*size-c);let mask=1.-smoothstep(radius-0.5,radius+0.5,distance);return mix(outgoing(uv),incoming(uv),mask);}",
                    vec![control("center", "Center", vec![0.5; 2], Some([0., 1.]))],
                ),
                Preset::Grain => (
                    "fn effect(uv:vec2<f32>)->vec4<f32>{let p=source(uv);let xy=vec2<u32>(floor(uv*fold_params.size.xy));var h=xy.x*1664525u+xy.y*1013904223u+u32(seed())+u32(max(0.,floor(fold_params.clock.x*rate())))*747796405u;h=(h^(h>>16u))*2246822519u;h=(h^(h>>13u))*3266489917u;let noise=f32(h^(h>>16u))/4294967295.-0.5;return vec4<f32>(p.rgb+vec3<f32>(noise*amount()*p.a),p.a);}",
                    vec![
                        control("amount", "Amount", vec![0.04], Some([0., 1.])),
                        control("seed", "Seed", vec![1.], Some([0., 1000000.])),
                        control("rate", "Grain rate", vec![24.], Some([1., 120.])),
                    ],
                ),
                Preset::FilmLook => (
                    "fn effect(uv:vec2<f32>)->vec4<f32>{let p=source(uv);if(p.a<=0.){return p;}let rgb=p.rgb/p.a;let l=dot(rgb,vec3<f32>(0.2722287,0.6740818,0.0536895));let chroma=mix(vec3<f32>(l),rgb,saturation());let look=chroma*exp2(exposure())+vec3<f32>(lift());return vec4<f32>(mix(rgb,look,strength())*p.a,p.a);}",
                    vec![
                        control("exposure", "Exposure", vec![-0.15], Some([-8., 8.])),
                        control("saturation", "Saturation", vec![0.85], Some([0., 2.])),
                        control("lift", "Shadow lift", vec![0.01], Some([-0.1, 0.1])),
                        control("strength", "Strength", vec![1.], Some([0., 1.])),
                    ],
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
