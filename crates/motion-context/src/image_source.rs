//! Shared qualification of image sources at placement and worker boundaries.
use fold_media::ingest::{InputInterpretation, SourceProfile};
use fold_project::Asset;

pub(crate) fn dimensions(profile: &SourceProfile) -> Result<[u32; 2], String> {
    let dimensions = match profile {
        SourceProfile::Ppm { dimensions } => *dimensions,
        SourceProfile::Raster(info) if info.bit_depth == 8 && info.icc_fingerprint.is_none() => {
            info.dimensions
        }
        _ => {
            return Err(
                "Motion currently requires 8-bit sRGB still images without embedded ICC profiles"
                    .into(),
            );
        }
    };
    if dimensions.contains(&0) || dimensions.iter().any(|value| *value > 16_384) {
        return Err("Motion image dimensions exceed the renderer limit".into());
    }
    Ok(dimensions)
}

pub(crate) fn interpretation(asset: &Asset) -> Result<(), String> {
    if let Some(value) = asset.extensions.get(fold_media::ingest::INTERPRETATION_KEY) {
        let input: InputInterpretation =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        if input.alpha.is_some()
            || input
                .color_space
                .as_ref()
                .is_some_and(|space| !space.eq_ignore_ascii_case("srgb"))
        {
            return Err("Motion image input color/alpha assignment is not qualified yet".into());
        }
    }
    Ok(())
}
