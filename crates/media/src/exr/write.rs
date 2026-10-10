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
    if pixels.len() != dimensions[0] as usize * dimensions[1] as usize
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
        .to_unbuffered(temporary.as_file_mut())
        .map_err(|e| e.to_string())?;
    cancel.check()?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary
        .persist_noclobber(path)
        .map_err(|e| e.to_string())?;
    Ok(())
}
