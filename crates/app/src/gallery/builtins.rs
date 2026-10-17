//! Built-in definitions use the same package and placement paths as My Library.
use super::storage::Component;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    model::{Canvas, Content},
    motion, tools,
};
use fold_platform::gallery::{Kind, Reference};
use std::sync::OnceLock;
const COUNT: u8 = 17;
fn id(index: u8) -> ObjectId {
    let mut bytes = [0x47; 16];
    bytes[15] = index;
    ObjectId::from_bytes(bytes)
}
pub fn contains(value: ObjectId) -> bool {
    (1..=COUNT).any(|i| id(i) == value)
}
pub fn get(reference: Reference) -> Option<Result<Component, String>> {
    contains(reference.id).then(|| {
        all()?
            .into_iter()
            .find(|c| c.reference == reference)
            .ok_or("Unavailable built-in version".into())
    })
}
pub fn all() -> Result<Vec<Component>, String> {
    static ITEMS: OnceLock<Result<Vec<Component>, String>> = OnceLock::new();
    ITEMS.get_or_init(build).clone()
}
fn build() -> Result<Vec<Component>, String> {
    let mut out = vec![];
    let mut title = tools::new_object("text")?;
    title.name = "Title".into();
    title.transform.x.value = 220.;
    title.transform.y.value = 460.;
    if let Content::Text {
        text, size, layout, ..
    } = &mut title.content
    {
        *text = "Your title".into();
        size.value = 110.;
        layout.width = 1480.;
    }
    title.paint.fill = Some([1., 1., 1., 1.].into());
    let object = ObjectId::new();
    let mut canvas = Canvas {
        roots: vec![object],
        ..Default::default()
    };
    canvas.objects.insert(object, title);
    out.push(component(
        1,
        "Title",
        "Titles",
        Kind::Graphic,
        canvas.document(DocumentId::new())?,
    ));
    let card = ObjectId::new();
    let mut group = tools::new_object("group")?;
    group.name = "Lower third".into();
    group.transform.x.value = 160.;
    group.transform.y.value = 780.;
    let panel = ObjectId::new();
    let text = ObjectId::new();
    if let Content::Group { children, .. } = &mut group.content {
        *children = vec![panel, text];
    }
    let mut bg = tools::rectangle("Card", [0., 0.], [1200., 200.]);
    bg.paint.fill = Some([0.10, 0.20, 0.55, 1.].into());
    let mut caption = tools::new_object("text")?;
    caption.name = "Name".into();
    caption.paint.fill = Some([1., 1., 1., 1.].into());
    caption.transform.x.value = 42.;
    caption.transform.y.value = 45.;
    if let Content::Text { text, size, .. } = &mut caption.content {
        *text = "Name and role".into();
        size.value = 76.;
    }
    let lower = Canvas {
        roots: vec![card],
        objects: [(card, group), (panel, bg), (text, caption)].into(),
        ..Default::default()
    };
    out.push(component(
        2,
        "Lower third",
        "Titles",
        Kind::Graphic,
        lower.document(DocumentId::new())?,
    ));
    for (index, name, scale) in [(3, "Fade up", false), (4, "Scale in", true)] {
        let mut canvas = canvas.clone();
        let owner = canvas.objects.get_mut(&object).unwrap();
        let source = format!(
            "pub fn animate(input: MotionContext<'_>) -> Result<MotionPose,String> {{ let t=(input.seconds()/input.number(\"Duration\")?).clamp(0.,1.); let p=1.-(1.-t).powi(3); Ok(MotionPose {{ position:[0.,{}], scale:[{},{}], opacity:p, ..Default::default() }}) }}",
            if scale {
                "0."
            } else {
                "input.number(\"Distance\")?*(1.-p)"
            },
            if scale { "0.8+0.2*p" } else { "1." },
            if scale { "0.8+0.2*p" } else { "1." }
        );
        let mut curves: [fold_animation::Curve; 6] = Default::default();
        for i in 0..=90 {
            let t = i as f64 / 90.;
            let p = 1. - (1. - t).powi(3);
            for (curve, v) in curves.iter_mut().zip([
                0.,
                if scale { 0. } else { 80. * (1. - p) },
                0.,
                if scale { 0.8 + 0.2 * p } else { 1. },
                if scale { 0.8 + 0.2 * p } else { 1. },
                p,
            ]) {
                curve.insert(Time::new(i, 120).unwrap(), v);
            }
        }
        owner.motion = Some(motion::Motion {
            spec: motion::Spec {
                source,
                controls: [("Duration".into(), 0.75), ("Distance".into(), 80.)].into(),
                sample_rate: 120,
            },
            enabled: true,
            curves,
        });
        out.push(component(
            index,
            name,
            "Motion",
            Kind::Motion,
            canvas.document(DocumentId::new())?,
        ));
    }
    for (index, preset, kind) in [
        (
            5,
            fold_compositor::presets::Preset::Dissolve,
            Kind::Transition,
        ),
        (
            6,
            fold_compositor::presets::Preset::CircularWipe,
            Kind::Transition,
        ),
        (7, fold_compositor::presets::Preset::Glow, Kind::Effect),
    ] {
        out.push(component(
            index,
            preset.label(),
            if kind == Kind::Transition {
                "Transitions"
            } else {
                "Effects"
            },
            kind,
            fold_compositor::presets::create(preset, DocumentId::new())?,
        ));
    }
    for (index, (preset, name)) in fold_motion_canvas::presets::CATALOG.iter().enumerate() {
        let canvas = fold_motion_canvas::presets::canvas(preset)?;
        let mut item = component(
            8 + index as u8,
            name,
            "Animated titles",
            Kind::Graphic,
            canvas.document(DocumentId::new())?,
        );
        if *preset == "logo-assemble" {
            item.category = "Branding".into();
            let images: &[&[u8]] = &[
                include_bytes!("../../../../resources/fframes-components/placeholder-01.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-02.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-03.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-04.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-05.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-06.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-07.png"),
                include_bytes!("../../../../resources/fframes-components/placeholder-08.png"),
            ];
            item.resources = images
                .iter()
                .enumerate()
                .map(|(i, bytes)| super::storage::Resource {
                    asset: fold_project::Asset {
                        id: fold_motion_canvas::presets::image_id(i as u8 + 1),
                        location: format!("placeholder-{:02}.png", i + 1),
                        fingerprint: format!("sha256:{}", fold_media::content_hash(bytes)),
                        extensions: Default::default(),
                    },
                    bytes: bytes.to_vec(),
                })
                .collect();
        }
        item.extensions.insert("attribution".into(), serde_json::json!({"source":"https://github.com/Bilaal0x/fframes-components","revision":"ae04b1be6596b0fc51e18280392ff6ee77629c47","license":"MIT"}));
        out.push(item);
    }
    Ok(out)
}
fn component(
    index: u8,
    name: &str,
    category: &str,
    kind: Kind,
    document: fold_project::Document,
) -> Component {
    Component {
        schema: 1,
        reference: Reference {
            id: id(index),
            version: 1,
        },
        name: name.into(),
        category: category.into(),
        kind,
        document,
        resources: vec![],
        preview: None,
        extensions: Default::default(),
    }
}
