// SPDX-License-Identifier: MIT
//! The design segment: the object database with the timeline,
//! parameters, sketches, features, components and occurrences.
//!
//! Layers:
//! - [`stream`]: `MetaStream.dat` and the object framing of
//!   `BulkStream.dat` (headers, root parts, references, strings).
//! - [`decode`] and [`sketch`]: decoders for the parts that the timeline
//!   format study marks certain or probable.
//! - [`build`]: the decoded data shaped as the dump IR ([`ir`]), the JSON
//!   of `core/import/SCHEMA.md`.
//! - [`recipe`] and [`inputs`]: feature inputs named as the bodies name
//!   their faces and edges, found in the ASM history of the body blobs
//!   ([`decode_document`] does it).
//!
//! The decoder reproduces the reference decoder (a separate Python
//! implementation of the format study, not part of this repository)
//! exactly, so that the two can be compared on the corpus
//! (`tests/design_corpus.rs`). Fields that cannot be decoded are absent,
//! never guessed.
//!
//! ```no_run
//! use mitcad_f3d::design;
//! for d in design::decode_path(std::path::Path::new("part.f3d")).unwrap() {
//!     let dump = d.dump.expect("design segment");
//!     for item in dump.timeline_items() {
//!         println!("{:?} {:?}", item.name(), item.object_type());
//!     }
//! }
//! ```

pub mod build;
pub mod classes;
pub mod decode;
pub mod inputs;
pub mod ir;
// Raw item records for the import's learning dump (mitcad#96).
pub mod learn;
mod nonfinite;
pub mod recipe;
mod record;
pub mod sketch;
pub mod stream;
mod unicode;

use std::path::Path;

pub use ir::Dump;
pub use stream::{MetaError, MetaStream, Segment};

use crate::container::F3dFile;
use crate::zip::ZipError;

/// Folder names of the design segment.
pub const DESIGN_SEGMENTS: [&str; 2] = ["Design1", "FusionDesignSegmentType1"];

/// The design segment's streams.
#[derive(Clone, Debug)]
pub struct DesignStreams {
    /// `Design1` or `FusionDesignSegmentType1`.
    pub segment_dir: String,
    pub meta: Vec<u8>,
    pub bulk: Vec<u8>,
}

/// The design segment of a document (`FusionAssetName.../Design1/` or
/// `.../FusionDesignSegmentType1/`), the first in archive order.
pub fn find_design_streams(doc: &F3dFile) -> Result<Option<DesignStreams>, ZipError> {
    for e in doc.archive().entries() {
        let parts: Vec<&str> = e.name.split('/').collect();
        if parts.len() == 3
            && parts[0].starts_with("FusionAssetName")
            && DESIGN_SEGMENTS.contains(&parts[1])
            && parts[2] == "MetaStream.dat"
        {
            let meta = doc.archive().read(e)?;
            let bulk = doc.read(&format!("{}/{}/BulkStream.dat", parts[0], parts[1]))?;
            return Ok(Some(DesignStreams {
                segment_dir: parts[1].to_string(),
                meta,
                bulk,
            }));
        }
    }
    Ok(None)
}

/// A decoded design segment.
pub struct Design {
    pub segment: Segment,
    pub decoded: decode::Decoded,
}

impl Design {
    /// Parses the streams and decodes them.
    pub fn parse(meta: &[u8], bulk: Vec<u8>) -> Result<Design, MetaError> {
        let segment = Segment::parse(meta, bulk)?;
        let decoded = decode::decode(&segment);
        Ok(Design { segment, decoded })
    }

    /// The dump IR. `file` and `segment_dir` are written to `source`.
    pub fn dump(&self, file: &str, segment_dir: &str) -> Dump {
        build::build(&self.segment, &self.decoded, file, segment_dir)
    }
}

/// One design of a file (an `.f3z` package holds several).
pub struct FileDesign {
    /// `name.f3d`, or `package.f3z!<first 8 characters of the entry>`.
    pub label: String,
    /// The design segment folder, if the document has one.
    pub segment_dir: Option<String>,
    /// The dump; `None` if there is no design segment.
    pub dump: Option<Dump>,
}

/// The file name without a cache GUID
/// (`name.<36 characters>.f3d` -> `name.f3d`).
pub fn short_name(path: &Path) -> String {
    let b = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parts: Vec<&str> = b.split('.').collect();
    if parts.len() >= 3 && parts[parts.len() - 2].chars().count() == 36 {
        format!(
            "{}.{}",
            parts[..parts.len() - 2].join("."),
            parts[parts.len() - 1]
        )
    } else {
        b
    }
}

/// The documents of a file with their labels: the file itself, or the
/// `.f3d` entries of an `.f3z` package.
pub fn documents(path: &Path) -> Result<Vec<(String, F3dFile)>, String> {
    let f = F3dFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = short_name(path);
    let is_package = path.to_string_lossy().to_lowercase().ends_with(".f3z");
    if !is_package {
        return Ok(vec![(name, f)]);
    }
    let mut out = Vec::new();
    for e in f.archive().entries() {
        if !e.name.to_lowercase().ends_with(".f3d") {
            continue;
        }
        let data = f
            .archive()
            .read(e)
            .map_err(|err| format!("{}!{}: {err}", path.display(), e.name))?;
        let doc = F3dFile::from_bytes(data)
            .map_err(|err| format!("{}!{}: {err}", path.display(), e.name))?;
        let prefix: String = e.name.chars().take(8).collect();
        out.push((format!("{name}!{prefix}"), doc));
    }
    Ok(out)
}

/// Decodes the design of a document.
pub fn decode_document(doc: &F3dFile, label: &str) -> Result<FileDesign, String> {
    let mut design = decode_streams(doc, label)?;
    resolve_inputs(&mut design, doc, 1);
    Ok(design)
}

// Decoding in two parts (mitcad#103): the import decodes every design's
// streams to choose one, and finds the chosen one's inputs in the bodies'
// history while it reads the bodies themselves.

/// [`decode_document`] without [`resolve_inputs`]: the design streams
/// alone (the timeline, components, features and the blobs' links); quick
/// next to finding the inputs in the bodies' history.
pub fn decode_streams(doc: &F3dFile, label: &str) -> Result<FileDesign, String> {
    let Some(s) = find_design_streams(doc).map_err(|e| format!("{label}: {e}"))? else {
        return Ok(FileDesign {
            label: label.to_string(),
            segment_dir: None,
            dump: None,
        });
    };
    let design = Design::parse(&s.meta, s.bulk).map_err(|e| format!("{label}: {e}"))?;
    let dump = design.dump(label, &s.segment_dir);
    Ok(FileDesign {
        label: label.to_string(),
        dump: Some(dump),
        segment_dir: Some(s.segment_dir),
    })
}

/// The rest of [`decode_document`] after [`decode_streams`]: the inputs
/// named in the streams, found in the bodies' history (on large designs
/// most of the decoding's time). Changes the items' inputs only, not the
/// timeline's order, the items' states and components, the components or
/// the blobs' links. `threads`: how many threads find them (the items one
/// at a time each; the same dump with any number).
pub fn resolve_inputs(design: &mut FileDesign, doc: &F3dFile, threads: usize) {
    if let Some(dump) = &mut design.dump {
        inputs::resolve_on(dump, doc, threads);
    }
}

/// Decodes every design of an `.f3d` or `.f3z` file.
pub fn decode_path(path: &Path) -> Result<Vec<FileDesign>, String> {
    documents(path)?
        .iter()
        .map(|(label, doc)| decode_document(doc, label))
        .collect()
}

#[cfg(test)]
mod tests;
