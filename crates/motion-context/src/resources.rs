//! Worker-only resolution of immutable project assets for the native renderer.
use fold_foundation::Failure;
use fold_media::{Cancel, budget};
use fold_motion_canvas::protocol::{ImageSnapshot, Raster};
use fold_project::Asset;
use std::{path::Path, sync::Arc};

/// Bind immutable project records without performing file I/O.
pub(crate) fn bind(
    canvas: &fold_motion_canvas::model::Canvas,
    project: &fold_project::Snapshot,
) -> Result<Vec<Arc<Asset>>, Failure> {
    canvas
        .assets()
        .into_iter()
        .map(|id| {
            project
                .state()
                .assets
                .get(&id)
                .cloned()
                .ok_or_else(|| Failure::from("Missing Motion image asset"))
        })
        .collect()
}

pub(crate) fn validate(
    canvas: &fold_motion_canvas::model::Canvas,
    assets: &[Arc<Asset>],
) -> Result<(), Failure> {
    let expected = canvas.assets();
    let mut actual = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();
    actual.sort();
    if expected != actual {
        return Err("Motion image assets were not bound to the immutable demand".into());
    }
    Ok(())
}

/// Metadata and source stamps are checked on each demand; pixel buffers are
/// rebuilt only when this key changes. Publication belongs to the worker ack.
#[derive(PartialEq, Eq)]
pub(crate) struct Key(Vec<(Arc<Asset>, fold_media::exr::cache::Stamp)>);
pub(crate) fn key(assets: &[Arc<Asset>], cancel: &Cancel) -> Result<Key, Failure> {
    assets
        .iter()
        .map(|asset| {
            Ok((
                asset.clone(),
                fold_media::exr::cache::Stamp::read(Path::new(&asset.location), cancel)?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Key)
}

pub(crate) struct Resolved {
    pub snapshot: ImageSnapshot,
    pub bytes: u64,
    pub _storage: Vec<Arc<budget::Lease>>,
}

pub(crate) fn load(assets: &[Arc<Asset>], cancel: &Cancel) -> Result<Resolved, Failure> {
    let mut result = Resolved {
        snapshot: ImageSnapshot {
            byte_limit: budget::working_usage().budget,
            images: Default::default(),
        },
        bytes: 0,
        _storage: Vec::new(),
    };
    for asset in assets {
        cancel.check()?;
        let alpha = crate::image_source::interpretation(asset)?;
        let (image, info) =
            fold_media::still::load(Path::new(&asset.location), &asset.fingerprint, cancel)?;
        if !image.is_srgb() {
            return Err("Motion image input is not sRGB".into());
        }
        let profile = info.map_or(
            fold_media::ingest::SourceProfile::Ppm {
                dimensions: image.dimensions(),
            },
            fold_media::ingest::SourceProfile::Raster,
        );
        let dimensions = crate::image_source::dimensions(&profile)?;
        let bytes = u64::from(dimensions[0]) * u64::from(dimensions[1]) * 4;
        let storage = budget::reserve_working(bytes)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(
                bytes
                    .try_into()
                    .map_err(|_| "Motion image allocation overflow")?,
            )
            .map_err(|_| "Motion image allocation failed")?;
        for (index, [r, g, b, source_alpha]) in image.straight_pixels().enumerate() {
            let a = match alpha {
                crate::image_source::Alpha::Straight => source_alpha,
                crate::image_source::Alpha::Opaque => 1.,
            };
            let rgba = [r * a, g * a, b * a, a];
            if index % 65536 == 0 {
                cancel.check()?;
            }
            if rgba
                .iter()
                .any(|value| !value.is_finite() || !(0. ..=1.).contains(value))
            {
                return Err("Motion image samples exceed the qualified SDR range".into());
            }
            pixels.extend(rgba.map(|value| (value * 255.).round() as u8));
        }
        result.snapshot.images.insert(
            asset.id.to_string(),
            Raster {
                identity: format!("{}:{}:{alpha:?}", asset.id, asset.fingerprint),
                dimensions,
                rgba8_premultiplied_srgb: pixels,
            },
        );
        result.bytes = result
            .bytes
            .checked_add(bytes)
            .ok_or("Motion resource allocation overflow")?;
        result._storage.push(storage);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_image_alpha_preserves_transparency_or_ignores_it_without_losing_rgb() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("alpha.png");
        // RGBA8: [255,64,128,128], [64,128,255,0]. The transparent
        // pixel's RGB must survive an explicit opaque interpretation.
        let bytes = include_bytes!("../../../resources/motion-context/alpha-input.png");
        std::fs::write(&path, bytes).unwrap();
        let mut asset = Asset {
            id: fold_foundation::AssetId::new(),
            location: path.to_str().unwrap().into(),
            fingerprint: format!("sha256:{}", fold_media::content_hash(bytes)),
            extensions: Default::default(),
        };
        let cancel = Cancel::default();
        let name = asset.id.to_string();
        let default = load(&[Arc::new(asset.clone())], &cancel).unwrap();
        assert_eq!(
            default.snapshot.images[&name].rgba8_premultiplied_srgb,
            [128, 32, 64, 128, 0, 0, 0, 0]
        );
        asset.extensions.insert(
            fold_media::ingest::INTERPRETATION_KEY.into(),
            serde_json::json!({"color_space":"sRGB","alpha":"straight","provenance":"artist"}),
        );
        let straight = load(&[Arc::new(asset.clone())], &cancel).unwrap();
        assert_eq!(
            straight.snapshot.images[&name].rgba8_premultiplied_srgb,
            default.snapshot.images[&name].rgba8_premultiplied_srgb
        );
        assert_eq!(
            straight.snapshot.images[&name].identity,
            default.snapshot.images[&name].identity
        );
        asset.extensions.insert(
            fold_media::ingest::INTERPRETATION_KEY.into(),
            serde_json::json!({"color_space":"sRGB","alpha":"opaque","provenance":"artist"}),
        );
        let opaque = load(&[Arc::new(asset.clone())], &cancel).unwrap();
        assert_eq!(
            opaque.snapshot.images[&name].rgba8_premultiplied_srgb,
            [255, 64, 128, 255, 64, 128, 255, 255]
        );
        assert_ne!(
            opaque.snapshot.images[&name].identity,
            straight.snapshot.images[&name].identity
        );
        asset.extensions.insert(
            fold_media::ingest::INTERPRETATION_KEY.into(),
            serde_json::json!({"color_space":"sRGB","alpha":"premultiplied","provenance":"artist"}),
        );
        assert!(load(&[Arc::new(asset)], &cancel).is_err());
    }

    #[test]
    fn project_image_decode_preserves_samples_and_rejects_changed_source_and_assignments() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.ppm");
        let bytes = b"P6\n2 1\n255\n\xff\x00\x00\x00\x80\xff";
        std::fs::write(&path, bytes).unwrap();
        let mut asset = Asset {
            id: fold_foundation::AssetId::new(),
            location: path.to_str().unwrap().into(),
            fingerprint: format!("sha256:{}", fold_media::content_hash(bytes)),
            extensions: Default::default(),
        };
        let cancel = Cancel::default();
        let object = fold_foundation::ObjectId::new();
        let canvas = fold_motion_canvas::model::Canvas {
            roots: vec![object],
            objects: [(
                object,
                fold_motion_canvas::tools::image(asset.id, [2, 1]).unwrap(),
            )]
            .into(),
            ..Default::default()
        };
        validate(&canvas, &[Arc::new(asset.clone())]).unwrap();
        let mut other = asset.clone();
        other.id = fold_foundation::AssetId::new();
        assert!(validate(&canvas, &[Arc::new(other)]).is_err());
        assert!(validate(&canvas, &[]).is_err());
        assert!(validate(&canvas, &[Arc::new(asset.clone()), Arc::new(asset.clone())]).is_err());
        assert!(bind(&canvas, &fold_project::Project::new(1).snapshot()).is_err());
        let resolved = load(&[Arc::new(asset.clone())], &cancel).unwrap();
        assert_eq!(resolved.bytes, 8);
        assert_eq!(
            resolved.snapshot.images[&asset.id.to_string()].rgba8_premultiplied_srgb,
            vec![255, 0, 0, 255, 0, 128, 255, 255]
        );
        asset.extensions.insert(
            fold_media::ingest::INTERPRETATION_KEY.into(),
            serde_json::json!({
            "color_space":"linear_rec709","alpha":null,"provenance":"artist"}),
        );
        assert!(load(&[Arc::new(asset.clone())], &cancel).is_err());
        asset.extensions.clear();
        std::fs::write(&path, b"P6\n1 1\n255\n\x00\x00\x00").unwrap();
        assert!(load(&[Arc::new(asset)], &cancel).is_err());
    }
}
