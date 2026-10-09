//! One strict request schema for replacement transport adapters.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::patch::Patch;
use fold_project::proposals::{Handle, Receipt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub document: DocumentId,
    pub definition: Option<ObjectId>,
    #[serde(default)]
    pub object: Option<ObjectId>,
}
impl Scope {
    pub fn native(self) -> fold_motion::editing::Scope {
        self.definition.map_or(
            fold_motion::editing::Scope::Document,
            fold_motion::editing::Scope::Definition,
        )
    }
}
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
    Project {
        edits: Vec<super::dispatch::native::Edit>,
    },
    Source {
        expected: String,
        text: String,
        mapping: BTreeMap<ObjectId, ObjectId>,
    },
    Patches {
        patches: Vec<Patch>,
    },
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
