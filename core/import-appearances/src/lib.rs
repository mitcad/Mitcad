// SPDX-License-Identifier: MIT
//! The appearances family of the `.f3d` dump replay (mitcad#228): the
//! file's appearances and physical materials on the replay's bodies, faces
//! and occurrences, after the replay (`appearances`).
//!
//! A crate above the replay's core (`mitcad-import-replay`, mitcad#791):
//! its method is the extension trait [`Appearances`] on the core's
//! `Importer`, and the core reaches it only through its `Families` trait,
//! which `mitcad-import` fills, so that an edit here does not rebuild the
//! core. The report's types ([`AppearanceReport`] and the entries it
//! holds) are the core's, since the design's report holds them; they are
//! re-exported here. `mitcad-import` re-exports this crate as
//! `appearances`, the module's old path.

// Appearances and physical materials of bodies and faces (mitcad#228).
mod appearances;

pub use appearances::{
    AppearanceEntry, AppearanceReport, Appearances, Look, MaterialEntry, TextureEntry, apply,
    density, display_name, look, material_entry, mitcad_material,
};

// The mesh bodies' physical materials and appearances (mitcad#1376).
mod meshes;

pub use meshes::mesh_appearances;
