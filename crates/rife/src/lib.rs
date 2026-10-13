//! Private native RIFE adapter. Linux Vulkan tensors stay in device memory.
//! Unsafe code and external allocation ownership are confined to this crate.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Runtime;
