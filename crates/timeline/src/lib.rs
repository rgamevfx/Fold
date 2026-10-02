//! Timeline-owned source timing and embedded image-sequence documents.
mod authoring;
mod editing;
pub mod package;
mod placement;
mod sequence;
#[cfg(feature = "ui")]
pub mod ui;
pub use authoring::{ImportArgs, active, has_sequence, import, import_media};
pub use editing::{ClipEdit, SequenceEdit, edit_clip, edit_sequence};
mod sequence_audio;
mod sequence_video;
mod video;
use fold_foundation::{DocumentId, Rounding};
use fold_media::RgbImage;
use fold_platform::VideoProvider;
use fold_project::{Document, Revision};
use fold_render::{ImageOp, RenderGraph};
pub use sequence::{Clip, SEQUENCE, SEQUENCE_SCHEMA, Sequence, SourceMedia, Track, TrackKind};
pub use sequence_video::SequenceProvider;
pub use video::{VIDEO_LAYERS, VideoLayers, VideoLayersProvider};

pub const PACKAGE: &str = "fold.timeline";
pub const IMAGE_SEQUENCE: &str = "fold.timeline.image-sequence";
const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
const MAX_FRAMES: usize = 4096;

/// Stage an import; the caller commits this document via the project coordinator.
/// Frames are embedded, so later changes to source files cannot alter a snapshot.
/// Schema 1: LE u32 rate numerator, denominator, count, then length-prefixed PPMs.
pub fn import_sequence(
    id: DocumentId,
    rate: [u32; 2],
    frames: &[Vec<u8>],
) -> Result<Document, String> {
    if rate.contains(&0) || frames.is_empty() || frames.len() > MAX_FRAMES {
        return Err("sequence requires a positive rate and 1..=4096 frames".into());
    }
    let mut size = 12usize;
    let mut dimensions = None;
    for bytes in frames {
        size = size
            .checked_add(4)
            .and_then(|n| n.checked_add(bytes.len()))
            .ok_or("sequence size overflow")?;
        if size > MAX_PAYLOAD {
            return Err("sequence exceeds 16 MiB payload budget".into());
        }
        let frame = RgbImage::decode_ppm(bytes)?;
        if dimensions.is_some_and(|d| d != frame.dimensions()) {
            return Err("sequence dimensions must be constant".into());
        }
        dimensions = Some(frame.dimensions());
    }
    let mut payload = Vec::with_capacity(size);
    for value in [rate[0], rate[1], frames.len() as u32] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    for frame in frames {
        payload.extend_from_slice(&(frame.len() as u32).to_le_bytes());
        payload.extend_from_slice(frame);
    }
    Ok(Document {
        id,
        type_id: IMAGE_SEQUENCE.into(),
        package_id: PACKAGE.into(),
        schema_version: 1,
        revision: Revision::default(),
        dependencies: vec![],
        assets: vec![],
        payload,
        extensions: Default::default(),
    })
}

pub struct ImageSequenceProvider;
impl fold_platform::packages::DocumentProvider for ImageSequenceProvider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        IMAGE_SEQUENCE
    }
    fn schema(&self) -> u32 {
        1
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        self.video_info(document).map(|_| ())
    }
}
impl VideoProvider for ImageSequenceProvider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        IMAGE_SEQUENCE
    }
    fn video_info(&self, document: &Document) -> Result<fold_media::VideoInfo, String> {
        if document.schema_version != 1
            || document.payload.len() > MAX_PAYLOAD
            || document.payload.len() < 16
            || !document.assets.is_empty()
            || !document.dependencies.is_empty()
        {
            return Err("Invalid image sequence".into());
        }
        let word =
            |offset| u32::from_le_bytes(document.payload[offset..offset + 4].try_into().unwrap());
        let rate = [word(0), word(4)];
        let frames = word(8);
        if frames == 0 || frames as usize > MAX_FRAMES {
            return Err("Invalid image sequence count".into());
        }
        let mut offset = 12usize;
        let mut dimensions = None;
        for _ in 0..frames {
            let length = document
                .payload
                .get(offset..offset + 4)
                .ok_or("Truncated image sequence")?;
            offset += 4;
            let end = offset
                .checked_add(u32::from_le_bytes(length.try_into().unwrap()) as usize)
                .ok_or("Image sequence overflow")?;
            let size = RgbImage::ppm_dimensions(
                document
                    .payload
                    .get(offset..end)
                    .ok_or("Truncated image sequence")?,
            )?;
            if dimensions.is_some_and(|previous| previous != size) {
                return Err("Image sequence dimensions differ".into());
            }
            dimensions = Some(size);
            offset = end;
        }
        if offset != document.payload.len() {
            return Err("Trailing image sequence payload".into());
        }
        let [width, height] = dimensions.unwrap();
        let info = fold_media::VideoInfo {
            width,
            height,
            rate,
            frames,
        };
        info.validate()?;
        Ok(info)
    }
    fn compile(&self, request: fold_platform::VideoCompile<'_>) -> Result<RenderGraph, String> {
        let fold_platform::VideoCompile {
            document,
            reference,
            time,
            dimensions: [width, height],
            cancel,
            ..
        } = request;
        let output = reference.output.as_str();
        cancel.check()?;
        if document.package_id != PACKAGE
            || document.type_id != IMAGE_SEQUENCE
            || document.schema_version != 1
            || output != "video"
            || !document.dependencies.is_empty()
            || !document.assets.is_empty()
        {
            return Err("unsupported image-sequence document or output".into());
        }
        if document.payload.len() > MAX_PAYLOAD {
            return Err("sequence exceeds 16 MiB payload budget".into());
        }
        let mut data = document.payload.as_slice();
        fn word(data: &mut &[u8]) -> Result<u32, String> {
            let bytes = data.get(..4).ok_or("truncated sequence header")?;
            let value = u32::from_le_bytes(bytes.try_into().unwrap());
            *data = &data[4..];
            Ok(value)
        }
        let numerator = word(&mut data)?;
        let denominator = word(&mut data)?;
        let count = word(&mut data)? as usize;
        if count == 0 || count > MAX_FRAMES {
            return Err("invalid sequence frame count".into());
        }
        let index = time
            .to_ticks(numerator, denominator, Rounding::Floor)
            .map_err(|e| e.to_string())?;
        if index < 0 || index as u64 >= count as u64 {
            return Err("time outside half-open sequence range".into());
        }
        let mut selected = None;
        for i in 0..count {
            cancel.check()?;
            let length = word(&mut data)? as usize;
            let bytes = data.get(..length).ok_or("truncated sequence frame")?;
            data = &data[length..];
            // Validate the whole document, including non-selected frames.
            let size = RgbImage::ppm_dimensions(bytes).map_err(|e| format!("frame {i}: {e}"))?;
            if size != [width, height] {
                return Err("sequence dimensions must match output".into());
            }
            if i == index as usize {
                selected = Some(RgbImage::decode_ppm(bytes)?);
            }
        }
        if !data.is_empty() {
            return Err("trailing sequence payload".into());
        }
        Ok(RenderGraph {
            width,
            height,
            nodes: vec![ImageOp::Media(selected.unwrap())],
            output: 0,
        })
    }
}
