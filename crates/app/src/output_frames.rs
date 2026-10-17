//! Branch-local, committed-snapshot file delivery. Does not change project outputs.
use fold_media::Cancel;
use fold_platform::delivery::{Channels, Compression, Precision, Settings};
use fold_project::{CommittedSnapshot, DocumentRef};
pub fn export_frames(
    snapshot: &CommittedSnapshot,
    source: &DocumentRef,
    settings: &Settings,
    cancel: &Cancel,
    #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
) -> Result<(), String> {
    cancel.check()?;
    settings.validate()?;
    let registry = crate::packages::builtins();
    let info = registry.output_ref(snapshot, source)?;
    if settings.last >= info.frames {
        return Err("Write range exceeds the document duration".into());
    }
    if let Some(encoding) = settings.movie() {
        let path = settings.path(settings.first)?;
        if path.exists() {
            return Err(format!("Destination already exists: {}", path.display()));
        }
        if settings.create_folders {
            std::fs::create_dir_all(path.parent().ok_or("Missing output directory")?)
                .map_err(|e| e.to_string())?;
        }
        return super::export::export_movie(
            snapshot,
            source,
            &path,
            settings.first..settings.last + 1,
            encoding,
            &settings.movie_color,
            cancel,
            #[cfg(feature = "gpu")]
            host,
        );
    }
    let frames = (settings.first..=settings.last).step_by(settings.step as usize);
    // Preflight every destination before rendering or publishing any frame.
    for frame in frames.clone() {
        cancel.check()?;
        if settings.path(frame)?.exists() {
            return Err(format!(
                "Destination already exists: {}",
                settings.path(frame)?.display()
            ));
        }
    }
    #[cfg(feature = "gpu")]
    let mut renderer = super::OutputRenderer::with_shared_host(host)?;
    #[cfg(not(feature = "gpu"))]
    let mut renderer = super::OutputRenderer::from_environment()?;
    let processor = crate::color::with_config(snapshot, |config| {
        Ok(match config {
            Some(config) => {
                Some(config.conversion(fold_color::WORKING_SPACE, &settings.color_space)?)
            }
            None if settings.color_space == fold_color::LINEAR_SRGB => None,
            None => return Err("This project requires Linear Rec.709 (sRGB) output".into()),
        })
    })
    .map_err(String::from)?;
    for frame in frames {
        cancel.check()?;
        let rendered = renderer.evaluate_linear(
            snapshot,
            &super::SceneRequest {
                source: source.clone(),
                time: info.time(frame)?,
                dimensions: [info.width, info.height],
                region: None,
                preview: if settings.channels == Channels::Alpha {
                    Some(fold_render::view::View::Channel {
                        name: "rgba.alpha".to_owned().try_into()?,
                        range: Default::default(),
                    })
                } else {
                    None
                },
            },
            cancel,
        )?;
        let _lease = fold_media::budget::reserve_output(
            u64::from(info.width) * u64::from(info.height) * 32,
        )?;
        let mut pixels = rendered.pixels().to_vec();
        // Scalar selection packs the selected channel into RGB; retain its original
        // value as output alpha, without applying a color/display transform.
        if settings.channels == Channels::Alpha {
            for pixel in &mut pixels {
                pixel[3] = pixel[0];
            }
        }
        if settings.channels != Channels::Alpha {
            if let Some(processor) = &processor {
                processor.apply(&mut pixels)?;
            }
        }
        if !settings.premultiplied {
            for pixel in &mut pixels {
                if pixel[3] > 0. {
                    for component in 0..3 {
                        pixel[component] /= pixel[3];
                    }
                } else {
                    pixel[..3].fill(0.);
                }
            }
        }
        let path = settings.path(frame)?;
        if settings.create_folders {
            std::fs::create_dir_all(path.parent().ok_or("Missing output directory")?)
                .map_err(|e| e.to_string())?;
        }
        use fold_media::exr::write as exr;
        exr::write(
            &path,
            [info.width, info.height],
            &pixels,
            match settings.channels {
                Channels::Rgba => &[0, 1, 2, 3],
                Channels::Rgb => &[0, 1, 2],
                Channels::Alpha => &[3],
            },
            match settings.precision {
                Precision::Half => exr::Precision::Half,
                Precision::Float => exr::Precision::Float,
            },
            match settings.compression {
                Compression::None => exr::Compression::None,
                Compression::Rle => exr::Compression::Rle,
                Compression::Zip1 => exr::Compression::Zip1,
                Compression::Zip16 => exr::Compression::Zip16,
                Compression::Piz => exr::Compression::Piz,
            },
            cancel,
        )?;
    }
    Ok(())
}
