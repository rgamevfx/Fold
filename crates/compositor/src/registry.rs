//! Versioned image operation contracts.
use fold_graph::types::Type;
#[derive(Clone, Debug)]
pub struct Operator {
    pub(crate) kernel: &'static str,
    pub id: &'static str,
    pub inputs: &'static [Type],
    pub output: Type,
    pub version: u32,
}
use Type::{Image, Mask};
pub const OPERATORS: &[Operator] = &[
    Operator {
        kernel: "Read",
        id: "Read",
        inputs: &[],
        output: Image,
        version: 2,
    },
    Operator {
        kernel: "Solid",
        id: "Solid",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Crop",
        id: "Crop",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Mask",
        id: "Mask",
        inputs: &[],
        output: Mask,
        version: 1,
    },
    Operator {
        kernel: "Output",
        id: "Output",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Shuffle",
        id: "Shuffle",
        inputs: &[Image, Image],
        output: Image,
        version: 2,
    },
    Operator {
        kernel: "CullChannels",
        id: "Cull Channels",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "ColorGrade",
        id: "Grade",
        inputs: &[Image],
        output: Image,
        version: 3,
    },
    Operator {
        kernel: "Composite",
        id: "Merge",
        inputs: &[Image, Image],
        output: Image,
        version: 3,
    },
    Operator {
        kernel: "TransformImage",
        id: "Transform",
        inputs: &[Image],
        output: Image,
        version: 3,
    },
    Operator {
        kernel: "Blur",
        id: "Blur",
        inputs: &[Image],
        output: Image,
        version: 3,
    },
    Operator {
        kernel: "Unary",
        id: "Exposure",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Unary",
        id: "Invert",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Unary",
        id: "Clamp",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Unary",
        id: "Premult",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Unary",
        id: "Unpremult",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Shape",
        id: "Rectangle",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Shape",
        id: "Ellipse",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Input",
        id: "Input",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Instance",
        id: "Effect",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Dissolve",
        id: "Dissolve",
        inputs: &[Image, Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "RelativeTransform",
        id: "Relative Transform",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Shader",
        id: "Shader",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        kernel: "Write",
        id: "Write",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
];

impl Operator {
    /// The catalog and Network creation use native records from these defaults.
    pub fn defaults(&self, info: &fold_media::VideoInfo) -> Result<crate::OperationConfig, String> {
        use crate::OperationConfig as C;
        use fold_render::operations::Unary;
        let rect = [0, 0, info.width, info.height];
        Ok(match (self.id, self.version) {
            ("Write", 1) => C::Write {
                settings: fold_platform::delivery::Settings {
                    last: info.frames - 1,
                    ..Default::default()
                },
            },
            ("Read", 2) => C::Read {
                options: crate::read::ReadOptions {
                    before: fold_media::exr_sequence::Policy::Hold,
                    after: fold_media::exr_sequence::Policy::Hold,
                    ..Default::default()
                },
                source: crate::Source::Unassigned { info: info.clone() },
                start: fold_foundation::Time::ZERO,
                source_start: fold_foundation::Time::ZERO,
                duration: info.time(info.frames)?,
            },
            ("Solid", 1) => C::Solid {
                rgba: [0.2, 0.3, 0.5, 1.],
            },
            ("Crop", 1) => C::Crop { rect },
            ("Mask", 1) => C::Mask { rect },
            ("Output", 1) => C::Output,
            ("Shuffle", 2) => C::Shuffle { mappings: vec![] },
            ("Cull Channels", 1) => C::CullChannels {
                channels: Default::default(),
                keep: false,
            },
            ("Grade", 3) => C::ColorGrade {
                settings: Default::default(),
            },
            ("Merge", 3) => C::Composite {
                mode: Default::default(),
                channels: Default::default(),
            },
            ("Transform", 3) => C::TransformImage {
                settings: crate::Transform {
                    pivot: [info.width as f64 / 2., info.height as f64 / 2.],
                    ..Default::default()
                },
            },
            ("Blur", 3) => C::Blur {
                filter: Default::default(),
                size: [0.; 2],
                edges: Default::default(),
            },
            ("Exposure", 1) => C::Unary {
                operation: Unary::Exposure { stops: 0. },
            },
            ("Invert", 1) => C::Unary {
                operation: Unary::Invert,
            },
            ("Clamp", 1) => C::Unary {
                operation: Unary::Clamp {
                    minimum: 0.,
                    maximum: 1.,
                },
            },
            ("Premult", 1) => C::Unary {
                operation: Unary::Premult,
            },
            ("Unpremult", 1) => C::Unary {
                operation: Unary::Unpremult,
            },
            ("Rectangle", 1) => C::Shape {
                shape: crate::Shape::Rectangle,
                bounds: rect.map(f64::from),
            },
            ("Ellipse", 1) => C::Shape {
                shape: crate::Shape::Ellipse,
                bounds: rect.map(f64::from),
            },
            ("Input", 1) => C::Input {
                id: "source".into(),
            },
            ("Effect", 1) => C::Instance {
                instance: fold_platform::effects::Instance {
                    id: fold_foundation::ObjectId::new(),
                    definition: fold_project::DocumentRef {
                        document: fold_foundation::DocumentId::new(),
                        output: "video".into(),
                        extensions: Default::default(),
                    },
                    version: 1,
                    values: Default::default(),
                    animation: Default::default(),
                    enabled: true,
                    extensions: Default::default(),
                },
                images: vec!["source".into()],
            },
            ("Dissolve", 1) => C::Dissolve {
                curve: Default::default(),
            },
            ("Relative Transform", 1) => C::RelativeTransform {
                settings: crate::RelativeTransform {
                    source_size: [info.width as f64, info.height as f64],
                    ..Default::default()
                },
            },
            ("Shader", 1) => C::Shader {
                settings: Default::default(),
            },
            _ => return Err("Unresolved compositor operation".into()),
        })
    }
}
