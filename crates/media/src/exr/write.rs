//! Atomic flat EXR publication. The caller supplies output-transformed pixels.
use super::*;
#[derive(Clone, Copy)]
pub enum Precision {
    Half,
    Float,
}
#[derive(Clone, Copy)]
pub enum Compression {
    None,
    Rle,
    Zip1,
    Zip16,
    Piz,
}
pub fn write(
    path: &Path,
    dimensions: [u32; 2],
    pixels: &[[f32; 4]],
    components: &[usize],
    precision: Precision,
    compression: Compression,
    cancel: &Cancel,
) -> Result<(), fold_foundation::Failure> {
    cancel.check()?;
    let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
    if count == 0
        || count > crate::MAX_PIXELS as u64
        || pixels.len() as u64 != count
        || components.is_empty()
        || components.iter().any(|c| *c > 3)
    {
        return Err("Invalid EXR output pixels/channels".into());
    }
    if pixels.iter().any(|p| {
        components.iter().any(|&c| {
            !p[c].is_finite() || (matches!(precision, Precision::Half) && p[c].abs() > 65504.)
        })
    }) {
        return Err("Output samples exceed the selected EXR precision; choose 32-bit float".into());
    }
    let sample_bytes = if matches!(precision, Precision::Half) {
        2
    } else {
        4
    };
    let _samples = budget::reserve_working(count * components.len() as u64 * sample_bytes)?;
    // A sequential compressor bounds temporary storage independently of image height.
    let _scratch = budget::reserve_working(
        u64::from(dimensions[0]) * 32 * components.len() as u64 * 4 * 16 + 1024 * 1024,
    )?;
    let channels: Vec<_> = components
        .iter()
        .map(|&component| {
            let samples = match precision {
                Precision::Half => {
                    FlatSamples::F16(pixels.iter().map(|p| f16::from_f32(p[component])).collect())
                }
                Precision::Float => FlatSamples::F32(pixels.iter().map(|p| p[component]).collect()),
            };
            AnyChannel::new(["R", "G", "B", "A"][component], samples)
        })
        .collect();
    let encoding = Encoding {
        compression: match compression {
            Compression::None => ::exr::compression::Compression::Uncompressed,
            Compression::Rle => ::exr::compression::Compression::RLE,
            Compression::Zip1 => ::exr::compression::Compression::ZIP1,
            Compression::Zip16 => ::exr::compression::Compression::ZIP16,
            Compression::Piz => ::exr::compression::Compression::PIZ,
        },
        blocks: Blocks::ScanLines,
        line_order: LineOrder::Increasing,
    };
    let image = Image::from_layer(Layer::new(
        (dimensions[0] as usize, dimensions[1] as usize),
        LayerAttributes::default(),
        encoding,
        AnyChannels::sort(channels.into_iter().collect()),
    ));
    let parent = path.parent().ok_or("Missing output directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    image
        .write()
        .non_parallel()
        .to_unbuffered(Cancellable {
            inner: temporary.as_file_mut(),
            cancel: cancel.clone(),
        })
        .map_err(|e| e.to_string())?;
    cancel.check()?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist_noclobber(path)
        .map_err(|e| e.to_string())?;
    Ok(())
}
