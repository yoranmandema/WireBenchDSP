//! Library side of `pedal-app`: the egui development UI.
//!
//! The engine lives in `pedal-engine` and is re-exported here so the existing
//! UI code keeps working; `ui` is the only thing this crate owns.

pub mod ui;

pub use pedal_engine::{audio, channels, store};
