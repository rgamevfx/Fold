//! Immutable image-source resolution shared by creative packages. Decode remains
//! in render/media; document outputs compile through the existing capability seam.
use fold_foundation::{AssetId, Time};
use fold_project::DocumentRef;
use fold_render::{ImageOp, RenderGraph};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    Asset(AssetId),
    Output(DocumentRef),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Outside {
    #[default]
    Hold,
    Loop,
    Black,
    Error,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bake: Option<crate::image_bake::Reference>,
    pub source: Source,
    pub time: Time,
    #[serde(default)]
    pub outside: Outside,
    /// Optional input color assignment; None inherits the asset assignment.
    #[serde(default)]
    pub color_space: Option<String>,
    /// Document/sequence rate and range, supplied from output capability on placement.
    pub info: fold_media::VideoInfo,
}
impl Request {
    pub fn mapped_time(&self) -> Result<Option<Time>, String> {
        self.info.validate()?;
        let frame = self
            .time
            .to_ticks(
                self.info.rate[0],
                self.info.rate[1],
                fold_foundation::Rounding::Floor,
            )
            .map_err(|e| e.to_string())?;
        let total = i64::from(self.info.frames);
        if frame >= 0 && frame < total {
            return Ok(Some(self.time));
        }
        match self.outside {
            Outside::Hold => Ok(Some(self.info.time(frame.clamp(0, total - 1) as u32)?)),
            Outside::Loop => {
                let cycles = frame.div_euclid(total);
                let duration = self.info.time(self.info.frames)?;
                let offset = duration
                    .checked_scale(cycles, 1)
                    .map_err(|e| e.to_string())?;
                Ok(Some(
                    self.time.checked_sub(offset).map_err(|e| e.to_string())?,
                ))
            }
            Outside::Black => Ok(None),
            Outside::Error => Err("image source time outside duration".into()),
        }
    }
}
pub fn compile(
    request: &Request,
    context: &crate::VideoCompile<'_>,
) -> Result<RenderGraph, String> {
    context.cancel.check()?;
    let [width, height] = context.dimensions;
    let single = |op| RenderGraph {
        width,
        height,
        nodes: vec![op],
        output: 0,
    };
    if let Some(bake) = &request.bake {
        return Ok(single(ImageOp::Baked(crate::image_bake::resolve(
            bake,
            context.snapshot,
            request,
            context.cancel,
        )?)));
    }
    let Some(time) = request.mapped_time()? else {
        return Ok(single(ImageOp::Solid { rgba: [0.; 4] }));
    };
    let Source::Asset(id) = &request.source else {
        let Source::Output(reference) = &request.source else {
            unreachable!()
        };
        return (context.resolve_image)(reference, time)?.select(None);
    };
    let asset = context
        .snapshot
        .state()
        .assets
        .get(id)
        .ok_or("missing image asset")?;
    let metadata = crate::browser::asset_metadata(context.snapshot, *id)?;
    let expected = asset_info(context.snapshot, *id, &request.info)?;
    if expected != request.info {
        return Err(
            "image source metadata changed; replace the source to update its contract".into(),
        );
    }
    let space = request
        .color_space
        .clone()
        .or(crate::color::input(&asset.extensions)?);
    let op = match metadata.profile {
        fold_media::ingest::SourceProfile::Video(info) => ImageOp::video(
            fold_media::VideoSource {
                path: asset.location.clone().into(),
                fingerprint: asset.fingerprint.clone(),
                info,
            },
            time,
            space,
        ),
        fold_media::ingest::SourceProfile::Ppm { .. } => {
            if space
                .as_deref()
                .is_some_and(|s| s != fold_color::settings::SRGB_INPUT)
            {
                return Err("PPM supports the sRGB input assignment".into());
            }
            ImageOp::Media(fold_media::still::ppm(
                std::path::Path::new(&asset.location),
                &asset.fingerprint,
                context.cancel,
            )?)
        }
        fold_media::ingest::SourceProfile::Exr(info) => {
            let mut source = fold_media::exr::Source {
                path: asset.location.clone().into(),
                fingerprint: asset.fingerprint.clone(),
                info,
            };
            if source.info.sequence.is_some() {
                let first = source.info.sequence.as_ref().unwrap().first();
                let frame = time
                    .to_ticks(
                        request.info.rate[0],
                        request.info.rate[1],
                        fold_foundation::Rounding::Floor,
                    )
                    .map_err(|e| e.to_string())?;
                source = fold_media::exr_sequence::resolve(
                    &source,
                    frame + i64::from(first),
                    &Default::default(),
                    false,
                )?
                .ok_or("image sequence frame unavailable")?;
            }
            let channels = ["red", "green", "blue", "alpha"].map(|c| {
                source
                    .info
                    .channels
                    .iter()
                    .find(|v| v.name == format!("rgba.{c}"))
                    .map(|v| v.name.clone())
            });
            if channels[..3].iter().any(Option::is_none) {
                return Err(
                    "image requires rgba RGB channels; select another layer in a composition"
                        .into(),
                );
            }
            let aces = crate::color::project(context.snapshot)?.is_some();
            ImageOp::Exr {
                source,
                channels,
                space: if aces {
                    Some(space.unwrap_or_else(|| "ACEScg".into()))
                } else {
                    None
                },
            }
        }
        _ => return Err("Motion image input requires image/video media".into()),
    };
    Ok(single(op))
}

/// Lightweight placement metadata, with image sequences starting at local zero.
pub fn asset_info(
    snapshot: &fold_project::Snapshot,
    id: AssetId,
    document: &fold_media::VideoInfo,
) -> Result<fold_media::VideoInfo, String> {
    Ok(
        match crate::browser::asset_metadata(snapshot, id)?.profile {
            fold_media::ingest::SourceProfile::Video(info) => info,
            fold_media::ingest::SourceProfile::Ppm { dimensions } => fold_media::VideoInfo {
                width: dimensions[0],
                height: dimensions[1],
                ..document.clone()
            },
            fold_media::ingest::SourceProfile::Exr(image) => fold_media::VideoInfo {
                width: image.dimensions[0],
                height: image.dimensions[1],
                frames: image
                    .sequence
                    .as_ref()
                    .map_or(document.frames, |s| s.last() - s.first() + 1),
                ..document.clone()
            },
            _ => return Err("Motion requires image/video media".into()),
        },
    )
}
