//! Private native worker transport. It is distinct from Fold's artist/agent
//! protocol and carries immutable frame demands, never project mutations.
use crate::model::Canvas;
use fold_foundation::Time;
use serde::{Deserialize, Serialize};

pub const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Decoded SDR raster storage, already premultiplied in encoded sRGB. Resource
/// names identify hrefs; identities identify immutable upstream image content.
pub struct Raster {
    pub identity: String,
    pub dimensions: [u32; 2],
    pub rgba8_premultiplied_srgb: Vec<u8>,
}
// Pixels cross the private worker boundary losslessly compressed. JSON arrays
// of raw bytes expanded eight ordinary 1200px cards beyond the transport cap.
// Compression changes neither premultiplication nor the color interpretation.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RasterWire {
    identity: String,
    dimensions: [u32; 2],
    rgba8_premultiplied_srgb_deflate: String,
}
impl Serialize for Raster {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use base64::Engine;
        use std::io::Write;
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder
            .write_all(&self.rgba8_premultiplied_srgb)
            .map_err(serde::ser::Error::custom)?;
        let pixels = encoder.finish().map_err(serde::ser::Error::custom)?;
        RasterWire {
            identity: self.identity.clone(),
            dimensions: self.dimensions,
            rgba8_premultiplied_srgb_deflate: base64::engine::general_purpose::STANDARD
                .encode(pixels),
        }
        .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Raster {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use base64::Engine;
        use std::io::Read;
        let wire = RasterWire::deserialize(deserializer)?;
        let expected = u64::from(wire.dimensions[0])
            .checked_mul(u64::from(wire.dimensions[1]))
            .and_then(|n| n.checked_mul(4))
            .filter(|n| *n > 0 && *n <= MAX_REQUEST_BYTES as u64)
            .ok_or_else(|| {
                serde::de::Error::custom("Motion raster exceeds its decoded allowance")
            })?;
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(&wire.rgba8_premultiplied_srgb_deflate)
            .map_err(serde::de::Error::custom)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(expected as usize + 1)
            .map_err(serde::de::Error::custom)?;
        flate2::read::ZlibDecoder::new(compressed.as_slice())
            .take(expected + 1)
            .read_to_end(&mut pixels)
            .map_err(serde::de::Error::custom)?;
        if pixels.len() as u64 != expected {
            return Err(serde::de::Error::custom(
                "Motion raster size differs from its dimensions",
            ));
        }
        Ok(Self {
            identity: wire.identity,
            dimensions: wire.dimensions,
            rgba8_premultiplied_srgb: pixels,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageSnapshot {
    pub byte_limit: u64,
    pub images: std::collections::BTreeMap<String, Raster>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Render,
    Animate { object: fold_foundation::ObjectId },
    Capture { object: fold_foundation::ObjectId },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request<C = Canvas, F = Vec<Vec<u8>>> {
    pub id: u64,
    pub operation: Operation,
    pub canvas: C,
    pub time: Time,
    pub dimensions: [u32; 2],
    /// Explicit producer allowance. GPU callers reserve the exact footprint
    /// before submitting; standalone reference callers supply their own cap.
    pub allocation_limit: u64,
    /// Retained Skia resource-cache allowance, separate from the shared target.
    pub cache_limit: u64,
    /// Explicit bytes on initialization/change; None retains the worker's fonts.
    pub fonts: Option<F>,
    /// Complete replacement on change; None retains the worker's snapshot.
    /// An empty snapshot explicitly releases the resources.
    pub images: Option<ImageSnapshot>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureResponse {
    pub id: u64,
    pub bytes: usize,
    pub error: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    LinearSrgb,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Alpha {
    Premultiplied,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub id: u64,
    pub dimensions: [u32; 2],
    pub bytes: usize,
    pub encoding: Encoding,
    pub alpha: Alpha,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raster_transport_is_lossless_compact_and_bounded_by_dimensions() {
        let pixels: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();
        let raster = Raster {
            identity: "test".into(),
            dimensions: [128, 128],
            rgba8_premultiplied_srgb: pixels.clone(),
        };
        let json = serde_json::to_vec(&raster).unwrap();
        assert!(json.len() < pixels.len() / 10);
        let decoded: Raster = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded.rgba8_premultiplied_srgb, pixels);
        let mut wire: serde_json::Value = serde_json::from_slice(&json).unwrap();
        wire["dimensions"] = serde_json::json!([1, 1]);
        assert!(serde_json::from_value::<Raster>(wire).is_err());
    }
}
