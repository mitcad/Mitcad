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
//! - [`design`]: the design segment's object streams (timeline, parameters,
//!   sketches, features, components) decoded into the dump IR of
//!   `core/import/SCHEMA.md`.
//!
//! The format knowledge comes from inspecting files only (black-box reverse
//! engineering); see `ASM_FORMAT.md` and `OGS_FORMAT.md`.

pub mod asm;
pub mod brep;
pub mod container;
pub mod convert;
pub mod design;
pub mod flat;
pub mod names;
pub mod ogs;
#[doc(hidden)]
pub mod testdata;
pub use mitcad_zip as zip;

pub use container::F3dFile;

/// All bodies of one body blob.
pub struct BlobBodies {
    pub entry: String,
    pub header: asm::Header,
    pub coverage: asm::Coverage,
    /// Tokenising stopped early (unknown tag); the message.
    pub truncated: Option<String>,
    pub bodies: Vec<convert::ConvertedBody>,
}

/// Parses one body blob and converts its top-level bodies.
pub fn read_blob(
    entry: &str,
    data: &[u8],
    options: &convert::Options,
) -> Result<BlobBodies, asm::AsmError> {
    let file = asm::AsmFile::parse(data)?;
    let coverage = asm::Coverage::of(&file);
    let bodies = convert::convert_file(&file, options);
    Ok(BlobBodies {
        entry: entry.to_string(),
        header: file.header.clone(),
        coverage,
        truncated: file.truncated.as_ref().map(|e| e.to_string()),
        bodies,
    })
}

/// Reads and converts every body blob of a document (`.f3d`).
pub fn read_document_bodies(
    doc: &F3dFile,
    options: &convert::Options,
) -> Vec<Result<BlobBodies, String>> {
    doc.body_blobs()
        .into_iter()
        .map(|b| {
            let data = doc
                .read(&b.entry)
                .map_err(|e| format!("{}: {e}", b.entry))?;
            read_blob(&b.entry, &data, options).map_err(|e| format!("{}: {e}", b.entry))
        })
        .collect()
}
