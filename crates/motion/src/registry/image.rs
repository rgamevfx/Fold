use super::*;
pub(super) fn operations() -> Vec<Operation> {
    let text = |title, default: &str| {
        value(
            title,
            exact(Type::Text),
            Some(Literal::Text {
                value: default.into(),
            }),
        )
    };
    let mut entries = vec![];
    for (mode, builtin, source) in [
        (
            "asset",
            Builtin::ImageTexture,
            vec![("asset", value("Image", exact(Type::AssetRef), None))],
        ),
        (
            "output",
            Builtin::ImageTexture,
            vec![
                ("document", text("Document", "")),
                ("output", text("Output", "")),
            ],
        ),
    ] {
        let mut parameters = source;
        parameters.extend([
            ("time", {
                let mut p = time();
                p.title = "Source Time".into();
                p
            }),
            ("outside", text("Outside Range", "hold")),
            ("color_space", text("Input Color Space", "")),
        ]);
        entries.push(operation(
            ("image_texture", mode, "Image Texture", builtin),
            Palette::Artist,
            parameters,
            ResultType::Exact { ty: Type::Image },
            Cost::Uniform,
            vec![],
        ));
    }
    for (mode, builtin, ty) in [
        (
            "color",
            Builtin::SampleImageColor,
            Type::Color {
                encoding: crate::types::Encoding::LinearRec709,
                alpha: crate::types::Alpha::Straight,
            },
        ),
        ("scalar", Builtin::SampleImageScalar, Type::Scalar),
    ] {
        let mut parameters = vec![
            ("image", value("Image", exact(Type::Image), None)),
            (
                "uv",
                value(
                    "UV",
                    exact(Type::Vector {
                        dimensions: 2,
                        unit: Unit::Unitless,
                        space: Space::World,
                    }),
                    Some(Literal::Vector {
                        value: vec![0.5; 2],
                        unit: Unit::Unitless,
                        space: Space::World,
                    }),
                ),
            ),
            ("filter", text("Filter", "bilinear")),
            ("wrap", text("Wrap", "clamp")),
        ];
        if mode == "scalar" {
            parameters.push(("channel", text("Channel", "luminance")));
        } else {
            parameters.push((
                "clamp_negative",
                value(
                    "Clamp Negative RGB",
                    exact(Type::Bool),
                    Some(Literal::Bool { value: true }),
                ),
            ));
        }
        entries.push(operation(
            (
                "sample_image",
                mode,
                if mode == "color" {
                    "Sample Image Color"
                } else {
                    "Sample Image Channel"
                },
                builtin,
            ),
            Palette::Foundation,
            parameters,
            ResultType::Exact { ty },
            Cost::Element,
            vec![Domain::Point, Domain::Instance],
        ));
    }
    entries
}
