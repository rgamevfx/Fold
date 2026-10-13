//! Immutable Linux Vulkan image exchange. No feature document models live here.
//! Transport is RGBA16F; color/alpha semantics remain the frame owner's contract.
#[cfg(target_os = "linux")]
pub mod socket;
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod vulkan;
#[cfg(target_os = "linux")]
pub use vulkan::{Description, Device, Export, Image};
#[cfg(all(target_os = "linux", feature = "consumer"))]
pub use vulkan::{Imported, describe, identity};
