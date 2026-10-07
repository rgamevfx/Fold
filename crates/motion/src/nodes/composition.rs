//! A nested document remains an immutable output reference with an exact time map.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Element, Geometry},
};
use fold_foundation::Time;
use fold_platform::image_source::{Outside, Request, Source};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const KIND: &str = "fold.motion.composition";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub source: Option<fold_project::DocumentRef>,
    pub info: fold_media::VideoInfo,
    /// Parent time corresponding to source_start; trimming does not move this origin.
    pub start: Time,
    pub source_start: Time,
}
impl Settings {
    pub fn request(&self, time: Time) -> Result<Option<Request>, String> {
        let Some(source) = &self.source else {
            return Ok(None);
        };
        Ok(Some(Request {
            bake: None,
            source: Source::Output(source.clone()),
            info: self.info.clone(),
            time: time
                .checked_sub(self.start)
                .and_then(|t| t.checked_add(self.source_start))
                .map_err(|e| e.to_string())?,
            outside: Outside::Black,
            color_space: None,
        }))
    }
}
pub fn definition_composition() -> Definition {
    let mut d = definition(
        KIND,
        "Composition",
        "Image",
        vec![],
        vec![("content", Kind::Content)],
        |e, n| {
            let s = n.settings::<Settings>()?;
            let Some(request) = s.request(e.time)? else {
                return Ok(content(Arc::new(vec![])));
            };
            let image = super::image::ImageGeometry {
                texture: super::image::Texture::Source(request),
                driver: None,
                draw: fold_render::textured::Draw::new(super::image::quad([
                    s.info.width as f64,
                    s.info.height as f64,
                ])),
            };
            Ok(content(Arc::new(vec![Element::new(
                n.seed(),
                Geometry::Image(Arc::new(image)),
            )])))
        },
    );
    d.defaults = || {
        serde_json::to_value(Settings {
            source: None,
            info: fold_media::VideoInfo {
                width: 1920,
                height: 1080,
                rate: [24, 1],
                frames: 120,
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
        })
        .unwrap()
    };
    d.validate = |n| {
        let s = n.settings::<Settings>()?;
        s.info.validate()?;
        if s.source.as_ref().is_some_and(|r| r.output != "video") || s.source_start < Time::ZERO {
            return Err(
                "Composition requires a video output and nonnegative source in time".into(),
            );
        }
        Ok(())
    };
    d.atomic_settings = &["source", "info", "start", "source_start"];
    d.description = "Editable nested composition. Source is a document video output. Exact local time is parent time minus start plus source_start; outside the source duration is transparent. Layer trims preserve the source origin; moving the whole layer moves it.";
    d
}

/// Resolve current source dimensions and duration for evaluation without changing authored payloads.
pub(crate) fn refresh_metadata(
    motion: &mut std::borrow::Cow<'_, crate::Motion>,
    describe: &dyn Fn(&fold_project::DocumentRef) -> Result<fold_media::VideoInfo, String>,
) -> Result<(), String> {
    let mut updates = Vec::new();
    for (group, graph) in std::iter::once((None, &motion.graph))
        .chain(motion.groups.iter().map(|(id, g)| (Some(*id), &g.graph)))
    {
        for node in &graph.nodes {
            if node.kind == KIND {
                let settings = node.settings::<Settings>()?;
                if let Some(source) = settings.source {
                    let info = describe(&source)?;
                    if info != settings.info {
                        updates.push((group, node.id, info));
                    }
                }
            }
        }
    }
    for (group, id, info) in updates {
        let motion = motion.to_mut();
        let graph = match group {
            Some(id) => &mut motion.groups.get_mut(&id).unwrap().graph,
            None => &mut motion.graph,
        };
        let node = graph.nodes.iter_mut().find(|n| n.id == id).unwrap();
        node.settings["info"] = serde_json::to_value(info).map_err(|e| e.to_string())?;
    }
    Ok(())
}
