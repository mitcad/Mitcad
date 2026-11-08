// SPDX-License-Identifier: MIT
//! Preview images of a design without a GPU or a display (mitcad#1486):
//! the MCP tool `model_render`'s arguments ([`args`]), its camera
//! ([`camera`]), a software rasterizer over the bodies' display meshes
//! ([`raster`]) and a PNG writer ([`png`]).
//!
//! Everything here is plain Rust on `f64` and integers, single threaded
//! and in a fixed order, so the same scene, camera and size give the same
//! bytes on one platform. The model builds the scene from its bodies
//! (`mitcad_model::preview`); this crate knows no document.

pub mod args;
pub mod camera;
pub mod png;
pub mod raster;
// What an analysis finds, drawn over the bodies (mitcad#1488).
pub mod overlay;

pub use args::{Background, Fit, RenderArgs, Selection};
pub use camera::{Aabb, Camera, Frame, Projection, SavedView, View, resolve_camera};
pub use overlay::{Overlay, OverlayKind, OverlayMesh};

// The inspection options and several views in one call (mitcad#1487).
pub mod inspect;
pub mod views;
pub use inspect::{EntityKind, Highlight, Inspect, Section};
#[cfg(test)]
mod inspect_tests;

#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod tests;
