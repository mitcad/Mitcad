// SPDX-License-Identifier: MIT
//! Reader for `.f3d` (and `.f3z`) files.
//!
//! Layers, bottom up:
//! - [`zip`]: the zip container (stored, deflate, zstd entries; the
//!   `mitcad-zip` crate, shared with the `.FCStd` reader).
//! - [`container`]: the document inside it: segments, body blobs,
//!   previews and display meshes, documents of `.f3z` packages.
//! - [`asm`]: the ASM binary format of the body blobs.
//! - [`brep`]: a neutral B-rep model in millimetres with checks.
//! - [`convert`]: ASM bodies to the neutral model.
//! - [`ogs`]: the display meshes saved with a document, an
//!   independent check of the bodies.
//! - [`protein`]: the appearance and material asset packages (mitcad#228).
//! - [`design`]: the design segment's object streams (timeline, parameters,
//!   sketches, features, components) decoded into the dump IR of
//!   `core/import/SCHEMA.md`.
//!
//! The format knowledge comes from inspecting files only (black-box reverse
//! engineering); see `ASM_FORMAT.md` and `OGS_FORMAT.md`.
//!
//! Since mitcad#776 the layers are crates of their own (`mitcad-asm`,
//! `mitcad-dump`, `mitcad-f3d-names`, `mitcad-f3d-file`,
//! `mitcad-f3d-design`), re-exported here under their old paths; this
//! crate keeps [`read_document_bodies`], the test references, the
//! `mitcad-f3d-inspect` tool and the corpus tests.

// The reader is split into crates (mitcad#776); this one keeps their
// paths: the ASM part (`mitcad-asm`), the names of B-rep entities
// (`mitcad-f3d-names`), the document of the file (`mitcad-f3d-file`) and
// the design segment (`mitcad-f3d-design`). The dump IR (`mitcad-dump`)
// is `design::ir`.
#[doc(hidden)]
pub use mitcad_asm::testdata;
pub use mitcad_asm::{BlobBodies, asm, brep, convert, flat, history_states, ids, read_blob};
pub use mitcad_f3d_design as design;
pub use mitcad_f3d_file::{F3dFile, container, ogs, protein};
pub use mitcad_f3d_names as names;
pub use mitcad_zip as zip;

#[doc(hidden)]
pub mod testreferences;
// A synthetic design with a mesh body (mitcad#1366).
#[doc(hidden)]
pub mod testmeshes;

/// Reads and converts every body blob of a document (`.f3d`).
pub fn read_document_bodies(
    doc: &F3dFile,
    options: &convert::Options,
) -> Vec<Result<BlobBodies, String>> {
    doc.body_blobs()
        .into_iter()
        .map(|b| {
            let data = doc
                .read_large(&b.entry)
                .map_err(|e| format!("{}: {e}", b.entry))?;
            read_blob(&b.entry, &data, options).map_err(|e| format!("{}: {e}", b.entry))
        })
        .collect()
}
