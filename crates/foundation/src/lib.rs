//! Shared stable IDs and exact time; no feature or UI models.

mod time;
pub use time::{Rounding, Time, TimeError, TimeRange};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! stable_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
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
stable_id!(BinId);
