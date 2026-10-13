//! One strict request schema for replacement transport adapters.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::proposals::{Handle, Receipt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use fold_platform::proposals::Scope;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Guard {
    pub epoch: ObjectId,
    pub request_id: ObjectId,
    pub base: u64,
    pub scope: Scope,
}
/// Project-wide operations deliberately have no document-edit scope.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectGuard {
    pub epoch: ObjectId,
    pub request_id: ObjectId,
    pub base: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum JobGuard {
    Document(Guard),
    Project(ProjectGuard),
}
impl JobGuard {
    pub fn base(&self) -> (ObjectId, u64) {
        match self {
            Self::Document(g) => (g.epoch, g.base),
            Self::Project(g) => (g.epoch, g.base),
        }
    }
    pub fn scope(&self) -> Option<Scope> {
        match self {
            Self::Document(g) => Some(g.scope),
            Self::Project(_) => None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    Project { edits: Vec<serde_json::Value> },
    Native { request: serde_json::Value },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservationTarget {
    Committed {
        epoch: ObjectId,
        revision: u64,
        document: DocumentId,
    },
    Candidate {
        handle: Handle,
        /// Output document in the immutable candidate project, including a
        /// downstream Compositor or Timeline document.
        document: DocumentId,
    },
}

/// Pixel rectangle in the full output raster; it does not change camera framing.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl FrameRegion {
    pub fn native(self) -> fold_render::region::Region {
        fold_render::region::Region {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}
/// Additional exact poses for one immutable frame review. The primary frame
/// remains first. Review pixels are display-space diagnostics, not export media.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameReview {
    pub times: Vec<Time>,
    pub layout: ReviewLayout,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReviewLayout {
    ContactSheet { columns: u32 },
    Onion,
}
impl FrameReview {
    pub fn dimensions(&self, frame: [u32; 2]) -> Result<[u32; 2], String> {
        if self.times.is_empty() || self.times.len() > 15 {
            return Err("Animation review requires 1..15 additional times".into());
        }
        let count = self.times.len() as u64 + 1;
        if frame.iter().any(|v| *v == 0 || *v > 4096)
            || u64::from(frame[0]) * u64::from(frame[1]) * count > 2_073_600
        {
            return Err("Animation review exceeds 2073600 total source pixels".into());
        }
        let output = match self.layout {
            ReviewLayout::Onion => frame,
            ReviewLayout::ContactSheet { columns } => {
                if columns == 0 || u64::from(columns) > count {
                    return Err("Invalid contact sheet column count".into());
                }
                [
                    frame[0]
                        .checked_mul(columns)
                        .ok_or("Contact sheet width overflow")?,
                    frame[1]
                        .checked_mul((count as u32).div_ceil(columns))
                        .ok_or("Contact sheet height overflow")?,
                ]
            }
        };
        if output.iter().any(|v| *v > 4096)
            || u64::from(output[0]) * u64::from(output[1]) > 2_073_600
        {
            return Err("Animation review output exceeds the image limit".into());
        }
        Ok(output)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Discover {},
    JobStatus {
        handle: super::jobs::Handle,
        #[serde(default)]
        #[schemars(range(max = 25000))]
        wait_ms: u32,
    },
    CancelJob {
        guard: JobGuard,
        handle: super::jobs::Handle,
    },
    SaveProjectCopy {
        guard: ProjectGuard,
        /// Absolute destination path. Existing files are never replaced.
        path: String,
    },
    Observe {
        target: ObservationTarget,
        output: ObjectId,
        times: Vec<Time>,
    },
    ObserveFrame {
        target: ObservationTarget,
        output: String,
        /// Exact output-relative time in the registered output's half-open duration.
        time: Time,
        /// Full output raster size, before any region extraction.
        dimensions: [u32; 2],
        #[serde(default)]
        region: Option<FrameRegion>,
        #[serde(default)]
        review: Option<FrameReview>,
    },
    InspectParameters {
        epoch: ObjectId,
        revision: u64,
        /// None discovers the project. A document filter limits the work.
        document: Option<DocumentId>,
        #[serde(default)]
        offset: usize,
        #[schemars(range(min = 1, max = 256))]
        limit: usize,
    },
    ResolveParameter {
        epoch: ObjectId,
        revision: u64,
        /// Unescaped project path of the owning node/container.
        owner: Vec<String>,
        path: String,
    },
    SampleParameter {
        epoch: ObjectId,
        revision: u64,
        target: fold_platform::parameters::Target,
        /// Authored document time; the provider applies nested clocks.
        time: Time,
    },
    InspectDocument {
        epoch: ObjectId,
        revision: u64,
        scope: Scope,
    },
    InspectCandidate {
        handle: Handle,
    },
    Propose {
        guard: Guard,
        edit: Edit,
    },
    Rebase {
        guard: Guard,
        handle: Handle,
    },
    Apply {
        guard: Guard,
        handle: Handle,
        receipt: Receipt,
    },
    CancelCandidate {
        guard: Guard,
        handle: Handle,
    },
}
impl Command {
    pub fn guard_base(&self) -> Option<(ObjectId, u64)> {
        match self {
            Self::InspectParameters {
                epoch, revision, ..
            }
            | Self::ResolveParameter {
                epoch, revision, ..
            }
            | Self::SampleParameter {
                epoch, revision, ..
            } => Some((*epoch, *revision)),
            Self::CancelJob { guard, .. } => Some(guard.base()),
            Self::SaveProjectCopy { guard, .. } => Some((guard.epoch, guard.base)),
            Self::Propose { guard, .. }
            | Self::Rebase { guard, .. }
            | Self::Apply { guard, .. }
            | Self::CancelCandidate { guard, .. } => Some((guard.epoch, guard.base)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    #[test]
    fn animation_review_rejects_unbounded_or_empty_images() {
        let mut review = FrameReview {
            times: vec![Time::ZERO; 2],
            layout: ReviewLayout::ContactSheet { columns: 2 },
        };
        assert_eq!(review.dimensions([640, 360]).unwrap(), [1280, 720]);
        assert!(review.dimensions([u32::MAX, u32::MAX]).is_err());
        assert!(review.dimensions([1920, 1080]).is_err());
        assert!(review.dimensions([0, 360]).is_err());
        review.layout = ReviewLayout::ContactSheet { columns: 0 };
        assert!(review.dimensions([640, 360]).is_err());
        review.layout = ReviewLayout::Onion;
        assert_eq!(review.dimensions([640, 360]).unwrap(), [640, 360]);
        review.times.clear();
        assert!(review.dimensions([640, 360]).is_err());
        review.times = vec![Time::ZERO; 16];
        assert!(review.dimensions([1, 1]).is_err());
    }
    #[test]
    fn candidate_observation_requires_an_explicit_document() {
        let handle = Handle {
            epoch: ObjectId::new(),
            generation: ObjectId::new(),
        };
        let document = DocumentId::new();
        assert!(
            serde_json::from_value::<ObservationTarget>(serde_json::json!({
                "kind":"candidate","handle":handle
            }))
            .is_err()
        );
        let target = serde_json::from_value::<ObservationTarget>(serde_json::json!({
            "kind":"candidate","handle":handle,"document":document
        }))
        .unwrap();
        assert!(
            matches!(target, ObservationTarget::Candidate { document: id, .. } if id == document)
        );
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub command: Command,
}
pub fn schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(Request)).expect("request schema");
    schema["properties"]["version"] = serde_json::json!({"type":"integer", "const":2});
    schema
}
