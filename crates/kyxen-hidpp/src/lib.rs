//! Logitech HID++ 2.0 for wired gaming keyboards (G815, G915).
//!
//! The protocol addresses *features* (`0x8081` per-key lighting, `0x8100`
//! onboard profiles, …) through per-device *indices*. Indices differ between
//! models and firmware versions, so this crate never hardcodes them:
//! [`HidppDevice::feature_index`] resolves them through the ROOT feature.
//!
//! I/O goes through the [`Transport`] trait; [`hidraw::Hidraw`] talks to a
//! `/dev/hidraw*` node, and the `mock` feature provides an in-memory keyboard
//! for tests.

pub mod descriptor;
pub mod device;
pub mod error;
pub mod feature;
pub mod hidraw;
pub mod lighting;
#[cfg(feature = "mock")]
pub mod mock;
pub mod notify;
pub mod onboard;
pub mod report;

pub use device::{HidppDevice, Transport};
pub use error::{Error, ErrorCode, Result};
pub use feature::FeatureId;
pub use report::{Message, Notification};
