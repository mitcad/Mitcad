// SPDX-License-Identifier: MIT
//! The file layer of the `.f3d` reader: the document inside the zip
//! container and what it keeps beside the design and the bodies.
//!
//! - [`container`]: the document inside the zip container: segments, body
//!   blobs, previews and display meshes, documents of `.f3z` packages.
//! - [`ogs`]: the display meshes saved with a document, an independent
//!   check of the bodies (`core/f3d/OGS_FORMAT.md`).
//! - [`protein`]: the appearance and material asset packages (mitcad#228).
//! - [`paramesh`]: the mesh containers of mesh bodies (mitcad#1366), their
//!   streams inflated by [`lzma`].
//!
//! Until mitcad#776 this was part of `mitcad-f3d`, which re-exports it
//! under the same paths.

pub mod container;
pub mod ogs;
pub mod protein;
pub use mitcad_zip as zip;
// The mesh containers of mesh bodies (mitcad#1366).
pub mod lzma;
pub mod paramesh;

pub use container::F3dFile;
