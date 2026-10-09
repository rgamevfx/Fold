//! Shared stable IDs and exact time; no feature or UI models.

mod time;
pub use time::{Rounding, Time, TimeError, TimeRange};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
            schemars::JsonSchema,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Canonical UUID bytes in network byte order.
            pub fn to_bytes(self) -> [u8; 16] {
                *self.0.as_bytes()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

stable_id!(DocumentId);
stable_id!(AssetId);
stable_id!(ObjectId);
impl ObjectId {
    /// Construct a deterministic object identity from canonical network-order bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }
}

stable_id!(BinId);
