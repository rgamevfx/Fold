//! Private native worker transport. It is distinct from Fold's artist/agent
//! protocol and carries immutable frame demands, never project mutations.
use crate::model::Canvas;
use fold_foundation::Time;
use serde::{Deserialize, Serialize};

pub const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// Decoded SDR raster storage, already premultiplied in encoded sRGB. Resource
/// names identify hrefs; identities identify immutable upstream image content.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Raster {
    pub identity: String,
    pub dimensions: [u32; 2],
    pub rgba8_premultiplied_srgb: Vec<u8>,
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
