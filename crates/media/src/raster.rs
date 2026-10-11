//! Common still formats. Animation and multipage files use their first image.
//! Samples retain precision and straight alpha; the default interpretation is sRGB.
use crate::{Cancel, RgbImage};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub dimensions: [u32; 2],
    pub format: String,
    pub bit_depth: u8,
    pub alpha: bool,
    pub premultiplied: bool,
    pub icc_fingerprint: Option<String>,
}

pub(crate) fn decode(
    bytes: &[u8],
    cancel: &Cancel,
) -> Result<(RgbImage, Info), fold_foundation::Failure> {
    cancel.check()?;
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let name = match format {
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Png => "png",
        ImageFormat::WebP => "webp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        _ => return Err("unsupported still image format".into()),
    };
    let premultiplied = if format == ImageFormat::Tiff {
        let mut tags =
            tiff::decoder::Decoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        tags.find_tag_unsigned_vec::<u16>(tiff::tags::Tag::ExtraSamples)
            .map_err(|e| e.to_string())?
            .is_some_and(|values| values.first() == Some(&1))
    } else {
        false
    };
    let mut reader = ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(crate::MAX_PIXELS as u32);
    limits.max_image_height = Some(crate::MAX_PIXELS as u32);
    limits.max_alloc = Some(crate::budget::working_usage().budget);
    reader.limits(limits);
    // The JPEG adapter owns a copy of its complete encoded input. Other
    // readers stream or allocate decoder scratch under their image limits.
    let _jpeg_input = if format == ImageFormat::Jpeg {
        Some(crate::budget::reserve_working(bytes.len() as u64)?)
    } else {
        None
    };
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    let count = u64::from(w) * u64::from(h);
    if count == 0 || count > crate::MAX_PIXELS as u64 {
        return Err("image exceeds pixel budget".into());
    }
    // Cover the decoder output, orientation copy and float conversion on this worker.
    let _working = crate::budget::reserve_working(count * 32)?;
    let color = decoder.color_type();
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let icc_fingerprint = decoder
        .icc_profile()
        .map_err(|e| e.to_string())?
        .map(|p| crate::content_hash(&p));
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    cancel.check()?;
    decoded.apply_orientation(orientation);
    let info = Info {
        dimensions: [decoded.width(), decoded.height()],
        format: name.into(),
        bit_depth: (color.bits_per_pixel() / u16::from(color.channel_count())) as u8,
        alpha: color.has_alpha(),
        premultiplied,
        icc_fingerprint,
    };
    if info.bit_depth == 8 && !info.alpha {
        return Ok((
            RgbImage::from_rgb(info.dimensions, decoded.into_rgb8().into_raw())?,
            info,
        ));
    }
    let rgba = decoded.into_rgba32f();
    let count = count as usize;
    let storage = crate::budget::DECODED.reserve(count as u64 * 16)?;
    let mut planes = vec![0.; count * 4];
    for (i, pixel) in rgba.pixels().enumerate() {
        if i % 65536 == 0 {
            cancel.check()?;
        }
        if pixel.0.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite image sample".into());
        }
        let straight = |value| {
            if premultiplied {
                if pixel[3] > 0. { value / pixel[3] } else { 0. }
            } else {
                value
            }
        };
        planes[i] = straight(pixel[1]);
        planes[count + i] = straight(pixel[2]);
        planes[count * 2 + i] = straight(pixel[0]);
        planes[count * 3 + i] = pixel[3];
    }
    Ok((
        RgbImage {
            dimensions: info.dimensions,
            rgb: Default::default(),
            identity: Default::default(),
            signal: Some(std::sync::Arc::new(planes)),
            srgb_signal: true,
            _storage: storage,
        },
        info,
    ))
}
