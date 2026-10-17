//! Curated component documents. Controls are ordinary script inputs: the same
//! native identities, animation curves, parameter references and guarded edits.
use crate::model::{Canvas, Content, Control, ControlValue, Scalar};
use fold_foundation::{AssetId, ObjectId, Time};
use std::collections::BTreeMap;
pub const CATALOG: &[(&str, &str)] = &[
    ("cinematic-text-swap", "Cinematic Text Swap"),
    ("soft-blur-in", "Soft Blur In"),
    ("tracking-in", "Tracking In"),
    ("word-gather", "Word Gather"),
    ("word-flip", "Word Flip"),
    ("lines-to-text", "Lines to Text"),
    ("echo-stack", "Echo Stack"),
    ("glitch-cycle", "Glitch Cycle"),
    ("text-highlight", "Text Highlight"),
    ("logo-assemble", "Logo Assemble"),
];
/// Stable resource identities let packages own and remap the image bytes normally.
pub fn image_id(index: u8) -> AssetId {
    serde_json::from_value(serde_json::json!(format!(
        "58585858-5858-5858-5858-5858585858{index:02x}"
    )))
    .expect("fixed asset identity")
}
pub fn canvas(preset: &str) -> Result<Canvas, String> {
    let name = CATALOG
        .iter()
        .find(|p| p.0 == preset)
        .ok_or("Unknown component preset")?
        .1;
    let mut controls = BTreeMap::new();
    let mut add = |name: &str,
                   value: ControlValue,
                   group: &str,
                   range: Option<[f64; 2]>,
                   choices: &[&str]| {
        let order = controls.len();
        controls.insert(ObjectId::new(), Control { name: name.into(), value, protected: false,
            extensions: [("fold.control".into(), serde_json::json!({"group":group,"order":order,"range":range,"choices":choices}))].into() });
    };
    match preset {
        "cinematic-text-swap" => {
            add(
                "From",
                ControlValue::Text("Manual work".into()),
                "Content",
                None,
                &[],
            );
            add(
                "To",
                ControlValue::Text("Made with Fold".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(64.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Transition",
                ControlValue::Text("Fly through".into()),
                "Motion",
                None,
                &[
                    "Fly through",
                    "Fade through",
                    "Crossfade",
                    "Shared axis Y",
                    "Shared axis Z",
                    "Cut",
                ],
            );
            add(
                "Unit",
                ControlValue::Text("Words".into()),
                "Motion",
                None,
                &["Block", "Words"],
            );
            add(
                "Enter frames",
                ControlValue::Number(Scalar::new(18.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Exit frames",
                ControlValue::Number(Scalar::new(18.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Stagger",
                ControlValue::Number(Scalar::new(2.0)),
                "Motion",
                Some([0.0, 30.0]),
                &[],
            );
        }
        "soft-blur-in" => {
            add(
                "Text",
                ControlValue::Text("Softly, then sharp.".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(96.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Blur",
                ControlValue::Number(Scalar::new(12.0)),
                "Motion",
                Some([0.0, 40.0]),
                &[],
            );
            add(
                "Stagger",
                ControlValue::Number(Scalar::new(1.0)),
                "Motion",
                Some([0.0, 20.0]),
                &[],
            );
            add(
                "Reveal frames",
                ControlValue::Number(Scalar::new(27.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
        }
        "tracking-in" => {
            add(
                "Text",
                ControlValue::Text("Give ideas room.".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(96.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Start tracking",
                ControlValue::Number(Scalar::new(1.5)),
                "Motion",
                Some([-0.1, 4.0]),
                &[],
            );
            add(
                "End tracking",
                ControlValue::Number(Scalar::new(-0.03)),
                "Motion",
                Some([-0.1, 2.0]),
                &[],
            );
            add(
                "Damping",
                ControlValue::Number(Scalar::new(18.0)),
                "Motion",
                Some([1.0, 100.0]),
                &[],
            );
            add(
                "Stiffness",
                ControlValue::Number(Scalar::new(100.0)),
                "Motion",
                Some([1.0, 500.0]),
                &[],
            );
        }
        "word-gather" => {
            add(
                "Text",
                ControlValue::Text("Every scene is yours to own.".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Line width",
                ControlValue::Number(Scalar::new(330.0)),
                "Appearance",
                Some([100.0, 1200.0]),
                &[],
            );
            add(
                "Accent",
                ControlValue::Color([0.15, 0.43, 0.94, 1.0]),
                "Appearance",
                None,
                &[],
            );
        }
        "word-flip" => {
            add(
                "Prefix",
                ControlValue::Text("Made for ".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Words",
                ControlValue::Text("artists\ndesigners\nstorytellers".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Suffix",
                ControlValue::Text(".".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(96.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Accent",
                ControlValue::Color([0.15, 0.43, 0.94, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Cycle frames",
                ControlValue::Number(Scalar::new(60.0)),
                "Motion",
                Some([12.0, 240.0]),
                &[],
            );
            add(
                "Overlap",
                ControlValue::Number(Scalar::new(6.0)),
                "Motion",
                Some([0.0, 24.0]),
                &[],
            );
        }
        "lines-to-text" => {
            add(
                "Lines",
                ControlValue::Text("LINES\nTO\nTEXT".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Bars per line",
                ControlValue::Number(Scalar::new(3.0)),
                "Appearance",
                Some([1.0, 12.0]),
                &[],
            );
            add(
                "Drift frames",
                ControlValue::Number(Scalar::new(12.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Converge frames",
                ControlValue::Number(Scalar::new(14.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Resolve frames",
                ControlValue::Number(Scalar::new(8.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Hold frames",
                ControlValue::Number(Scalar::new(48.0)),
                "Motion",
                Some([1.0, 300.0]),
                &[],
            );
            add(
                "Exit frames",
                ControlValue::Number(Scalar::new(18.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
        }
        "echo-stack" => {
            add(
                "Text",
                ControlValue::Text("Bold.".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(170.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Echo color",
                ControlValue::Color([0.15, 0.43, 0.94, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Echoes",
                ControlValue::Number(Scalar::new(3.0)),
                "Appearance",
                Some([1.0, 12.0]),
                &[],
            );
            add(
                "Echo opacity",
                ControlValue::Number(Scalar::new(0.35)),
                "Appearance",
                Some([0.0, 1.0]),
                &[],
            );
            add(
                "Outline",
                ControlValue::Number(Scalar::new(1.5)),
                "Appearance",
                Some([0.0, 8.0]),
                &[],
            );
            add(
                "Direction",
                ControlValue::Text("Up".into()),
                "Motion",
                None,
                &["Up", "Down"],
            );
            add(
                "Cycle frames",
                ControlValue::Number(Scalar::new(60.0)),
                "Motion",
                Some([12.0, 240.0]),
                &[],
            );
        }
        "glitch-cycle" => {
            add(
                "Words",
                ControlValue::Text("MAKE\nMOVE\nCREATE".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(120.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Item frames",
                ControlValue::Number(Scalar::new(45.0)),
                "Motion",
                Some([6.0, 240.0]),
                &[],
            );
            add(
                "Glitch amount",
                ControlValue::Number(Scalar::new(1.0)),
                "Motion",
                Some([0.0, 3.0]),
                &[],
            );
            add(
                "Delay frames",
                ControlValue::Number(Scalar::new(0.0)),
                "Motion",
                Some([0.0, 120.0]),
                &[],
            );
        }
        "text-highlight" => {
            add(
                "Before",
                ControlValue::Text("Made for ".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Highlight",
                ControlValue::Text("your ideas".into()),
                "Content",
                None,
                &[],
            );
            add(
                "After",
                ControlValue::Text(".".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Replacement",
                ControlValue::Text("your story".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Size",
                ControlValue::Number(Scalar::new(56.0)),
                "Appearance",
                Some([8.0, 500.0]),
                &[],
            );
            add(
                "Color",
                ControlValue::Color([0.94, 0.95, 0.98, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Accent",
                ControlValue::Color([0.15, 0.43, 0.94, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Style",
                ControlValue::Text("Marker".into()),
                "Motion",
                None,
                &[
                    "Logo wipe",
                    "Marker",
                    "Color",
                    "Underline",
                    "Strikethrough",
                    "Shimmer",
                ],
            );
            add(
                "Draw frames",
                ControlValue::Number(Scalar::new(14.0)),
                "Motion",
                Some([1.0, 120.0]),
                &[],
            );
            add(
                "Thickness",
                ControlValue::Number(Scalar::new(4.0)),
                "Appearance",
                Some([1.0, 30.0]),
                &[],
            );
        }
        "logo-assemble" => {
            add(
                "Brand",
                ControlValue::Text("Fold".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Middle text",
                ControlValue::Text("Make something move.".into()),
                "Content",
                None,
                &[],
            );
            add(
                "Cards",
                ControlValue::Number(Scalar::new(10.0)),
                "Appearance",
                Some([3.0, 20.0]),
                &[],
            );
            add(
                "Background",
                ControlValue::Color([0.02, 0.02, 0.02, 1.0]),
                "Appearance",
                None,
                &[],
            );
            add(
                "Logo",
                ControlValue::Image(image_id(1)),
                "Content",
                None,
                &[],
            );
            for index in 1..=8 {
                add(
                    &format!("Image {index}"),
                    ControlValue::Image(image_id(index)),
                    "Content",
                    None,
                    &[],
                );
            }
        }
        _ => unreachable!(),
    }
    add(
        "Speed",
        ControlValue::Number(Scalar::new(1.)),
        "Motion",
        Some([0.05, 5.0]),
        &[],
    );
    add(
        "Start",
        ControlValue::Number(Scalar::new(0.)),
        "Motion",
        Some([0.0, 3600.0]),
        &[],
    );
    let mut object = crate::tools::new_object("script")?;
    object.name = name.into();
    object.content = Content::Script {
        source: format!(
            "pub fn render(input: Context<'_>) -> Result<fframes::Svgr<'static>, String> {{ fold_motion_fframes::presets::render({preset:?}, input) }}"
        ),
        controls,
        extensions: Default::default(),
    };
    let id = ObjectId::new();
    let canvas = Canvas {
        roots: vec![id],
        objects: [(id, object)].into(),
        duration: Time::new(
            match preset {
                "soft-blur-in" | "tracking-in" | "word-gather" => 3,
                "word-flip" => 8,
                "glitch-cycle" => 6,
                _ => 4,
            },
            1,
        )
        .unwrap(),
        ..Default::default()
    };
    canvas.validate()?;
    Ok(canvas)
}
