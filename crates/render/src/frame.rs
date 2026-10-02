//! Physical frame metadata is descriptive, never permission to borrow storage.
//! `Frame` owns ready CPU pixels today. GPU/external leases and completion tokens
//! are opaque host-service identities; panels cannot derive a pointer from them.
use fold_foundation::Time;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alpha {
    Premultiplied,
    Opaque,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Unorm8,
    Float16,
    Float32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plane {
    pub offset: u64,
    pub row_stride: u64,
    pub channels: u8,
    pub component: Component,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub timestamp: Time,
    pub duration: Time,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Storage {
    /// Owned by the returned Frame; borrowing cannot outlive that owner.
    CpuOwned,
    /// Future host render-resource service leases. Descriptor IDs confer no
    /// access and do not establish completion or authorize resource recycling.
    GpuLease {
        device: u64,
        lease: u64,
    },
    ExternalLease {
        owner: u64,
        lease: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    Completion { owner: u64, token: u64 },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColorEncoding {
    Scene {
        working: crate::WorkingSpace,
        config_identity: Option<String>,
    },
    Output {
        processor_identity: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub dimensions: [u32; 2],
    pub pixel_aspect: [u32; 2],
    pub planes: Vec<Plane>,
    pub alpha: Alpha,
    pub color: ColorEncoding,
    pub timing: Option<Timing>,
    pub storage: Storage,
    pub readiness: Readiness,
}
