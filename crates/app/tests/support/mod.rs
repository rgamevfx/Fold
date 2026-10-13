//! Non-motion image fixtures for shared application services.
#![allow(dead_code)]
pub fn solid(rgb: [f32; 3]) -> fold_compositor::Composite {
    image([rgb[0], rgb[1], rgb[2], 1.])
}
pub fn image(rgba: [f32; 4]) -> fold_compositor::Composite {
    use fold_compositor::{Composite, Node, OperationConfig};
    let source = Node::new(OperationConfig::Solid { rgba }, vec![]);
    let output = Node::new(OperationConfig::Output, vec![source.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 1920,
            height: 1080,
            rate: [24, 1],
            frames: 240,
        },
        output: output.id,
        nodes: vec![source, output],
        extensions: Default::default(),
    }
}

/// Native artwork used to qualify shared services; no retired engine is involved.
pub fn motion() -> fold_motion_canvas::model::Canvas {
    let mut canvas = fold_motion_canvas::model::Canvas::default();
    let id = fold_foundation::ObjectId::new();
    canvas.roots.push(id);
    canvas.objects.insert(
        id,
        fold_motion_canvas::tools::new_object("rectangle").unwrap(),
    );
    canvas
}

pub fn motion_request(
    document: fold_foundation::DocumentId,
    native: serde_json::Value,
    x: f64,
) -> serde_json::Value {
    let id = native["native"]["roots"][0].as_str().unwrap();
    let expected = native["native"]["objects"][id]["transform"]["x"].clone();
    let mut value = expected.clone();
    value["value"] = serde_json::json!(x);
    serde_json::json!({"op":"author","document":document,"expected":native,"edits":[
        {"op":"parameter","object":id,"expected":expected,"value":value}
    ]})
}
