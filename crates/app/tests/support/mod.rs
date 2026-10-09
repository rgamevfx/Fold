//! Non-motion image fixtures for shared application services.
#![allow(dead_code)]
pub fn solid(rgb: [f32; 3]) -> fold_compositor::Composite {
    image([rgb[0], rgb[1], rgb[2], 1.])
}
pub fn image(rgba: [f32; 4]) -> fold_compositor::Composite {
    use fold_compositor::{Composite, Node, Parameters};
    let source = Node::new(Parameters::Solid { rgba }, vec![]);
    let output = Node::new(Parameters::Output, vec![source.id]);
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
