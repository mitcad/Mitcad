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
//! (`core/f3d/tests/design_corpus.rs`). Fields that cannot be decoded are
//! absent, never guessed.
//!
//! Until mitcad#776 this was the module `design` of `mitcad-f3d`, which
//! re-exports it as `mitcad_f3d::design`. Since mitcad#779 the decoding
//! layers ([`stream`], [`decode`], [`sketch`], [`classes`], [`recipe`],
//! [`curve`], [`learn`], [`writer`]) are the crate `mitcad-f3d-decode`,
//! re-exported here under their old paths.
//!
//! ```no_run
//! use mitcad_f3d_design as design;
//! for d in design::decode_path(std::path::Path::new("part.f3d")).unwrap() {
//!     let dump = d.dump.expect("design segment");
//!     for item in dump.timeline_items() {
//!         println!("{:?} {:?}", item.name(), item.object_type());
//!     }
//! }
//! ```

// Appearances and materials of bodies and faces (mitcad#228).
pub mod appearance;
pub mod build;
pub mod inputs;
// The dump IR is the `mitcad-dump` crate (mitcad#776).
pub use mitcad_dump as ir;
mod nonfinite;
// What an `.f3z` package says about its designs (mitcad#229).
pub mod package;
// Where derives' placements without a matrix are (mitcad#324).
pub mod derive_sources;
// The control cages of Form bodies (mitcad#1365).
pub mod cage;
// The streams, records and sketches are decoded by `mitcad-f3d-decode`
// (mitcad#779), re-exported here under their old paths: the learning
// dump's raw records (mitcad#96), the synthetic design streams for tests
// (mitcad#196) and the curves edge inputs store beside their recipes
// (mitcad#306) among them.
pub use mitcad_f3d_decode::{classes, curve, decode, learn, recipe, sketch, stream, writer};
// The design's mesh bodies (mitcad#1366).
pub use mitcad_f3d_decode::mesh;

// Python's character tables, for the names `build` keeps.
use mitcad_f3d_decode::unicode;

use std::path::Path;

pub use ir::Dump;
pub use stream::{MetaError, MetaStream, Segment};

use mitcad_f3d_file::container::F3dFile;
use mitcad_zip::ZipError;

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
/// `.f3d` entries of an `.f3z` package, all inflated at once. Where one
/// at a time does, [`package`] opens them lazily (mitcad#923).
pub fn documents(path: &Path) -> Result<Vec<(String, F3dFile)>, String> {
    package(path)?.into_documents()
}

/// The documents of a file ([`documents`]) and what an `.f3z` package says
/// about them (mitcad#229, [`package::PackageDescription`]).
///
/// The documents of a package are inflated when they are opened
/// ([`Package::open`], [`Package::documents`]), one at a time while each
/// is dropped before the next is opened, never all when the package is
/// read: an `.f3z` with many large designs must not hold them all in
/// memory (mitcad#923).
pub struct Package {
    /// The file: the document itself, or the package holding them.
    file: F3dFile,
    /// The file's path, for messages.
    path: String,
    /// The documents' labels with their entries in a package (`None`: the
    /// file itself).
    entries: Vec<(String, Option<String>)>,
    /// How many of the package's documents are open.
    open: OpenCount,
    /// The root design's label, where `Manifest.json` names it.
    pub root: Option<String>,
    /// The designs `DesignDescription.json` lists that the package holds,
    /// with their labels.
    pub described: Vec<(String, package::DescribedDesign)>,
}

/// How many documents of a [`Package`] are open now and at most so far.
#[derive(Default)]
struct OpenCount {
    now: std::sync::atomic::AtomicUsize,
    peak: std::sync::atomic::AtomicUsize,
}

impl OpenCount {
    fn opened(&self) {
        use std::sync::atomic::Ordering;
        let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
    }

    fn closed(&self) {
        self.now.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// A document of a [`Package`], as an [`F3dFile`] (`Deref`): the file
/// itself, or a document of an `.f3z` package, inflated while this is
/// held.
pub struct PackageDocument<'a> {
    label: &'a str,
    doc: Opened<'a>,
}

enum Opened<'a> {
    File(&'a F3dFile),
    /// A package's document and the count it is open in.
    Nested(F3dFile, &'a OpenCount),
}

impl PackageDocument<'_> {
    /// The document's label (see [`FileDesign::label`]).
    pub fn label(&self) -> &str {
        self.label
    }
}

impl std::ops::Deref for PackageDocument<'_> {
    type Target = F3dFile;

    fn deref(&self) -> &F3dFile {
        match &self.doc {
            Opened::File(file) => file,
            Opened::Nested(file, _) => file,
        }
    }
}

impl Drop for PackageDocument<'_> {
    fn drop(&mut self) {
        if let Opened::Nested(_, count) = &self.doc {
            count.closed();
        }
    }
}

impl Package {
    /// How many documents the file has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The documents' labels, in the package's order.
    pub fn labels(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().map(|(label, _)| label.as_str())
    }

    /// The index of the document labelled `label`.
    pub fn position(&self, label: &str) -> Option<usize> {
        self.entries.iter().position(|(l, _)| l == label)
    }

    /// Opens the document `index`: a package's is inflated now, with the
    /// zip reader's large entry limit (it holds the design's body blobs),
    /// and held until the returned document is dropped.
    pub fn open(&self, index: usize) -> Result<PackageDocument<'_>, String> {
        let (label, entry) = self
            .entries
            .get(index)
            .ok_or_else(|| format!("{}: no document {index}", self.path))?;
        let doc = match entry {
            None => Opened::File(&self.file),
            Some(entry) => {
                let doc = self
                    .file
                    .open_document(entry)
                    .map_err(|e| format!("{}!{entry}: {e}", self.path))?;
                self.open.opened();
                Opened::Nested(doc, &self.open)
            }
        };
        Ok(PackageDocument { label, doc })
    }

    /// The documents, each opened ([`Package::open`]) when the iterator
    /// comes to it: one at a time unless the caller keeps them.
    pub fn documents(&self) -> impl Iterator<Item = Result<PackageDocument<'_>, String>> + '_ {
        (0..self.len()).map(move |index| self.open(index))
    }

    /// The document `index` with its label, owned (the file itself is
    /// moved, not read again).
    pub fn into_document(self, index: usize) -> Result<(String, F3dFile), String> {
        let Package {
            file,
            path,
            mut entries,
            ..
        } = self;
        if index >= entries.len() {
            return Err(format!("{path}: no document {index}"));
        }
        let (label, entry) = entries.swap_remove(index);
        match entry {
            None => Ok((label, file)),
            Some(entry) => file
                .open_document(&entry)
                .map(|doc| (label, doc))
                .map_err(|e| format!("{path}!{entry}: {e}")),
        }
    }

    /// Every document with its label, all inflated at once ([`documents`]).
    pub fn into_documents(self) -> Result<Vec<(String, F3dFile)>, String> {
        let Package {
            file,
            path,
            entries,
            ..
        } = self;
        if let [(label, None)] = entries.as_slice() {
            return Ok(vec![(label.clone(), file)]);
        }
        entries
            .into_iter()
            .filter_map(|(label, entry)| Some((label, entry?)))
            .map(|(label, entry)| {
                file.open_document(&entry)
                    .map(|doc| (label, doc))
                    .map_err(|e| format!("{path}!{entry}: {e}"))
            })
            .collect()
    }

    /// How many of the package's documents are open now (mitcad#923; the
    /// file itself is not counted: it is read when the package is).
    pub fn open_now(&self) -> usize {
        self.open.now.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The most of the package's documents that were open at once.
    pub fn peak_open(&self) -> usize {
        self.open.peak.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The documents of a file and what an `.f3z` package says about them,
/// without inflating any document of a package ([`Package`]).
pub fn package(path: &Path) -> Result<Package, String> {
    let f = F3dFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let name = short_name(path);
    let is_package = path.to_string_lossy().to_lowercase().ends_with(".f3z");
    let mut out = Package {
        file: f,
        path: path.display().to_string(),
        entries: Vec::new(),
        open: OpenCount::default(),
        root: None,
        described: Vec::new(),
    };
    if !is_package {
        out.entries.push((name, None));
        return Ok(out);
    }
    let description = package::PackageDescription::read(&out.file);
    for e in out.file.archive().entries() {
        if !e.name.to_lowercase().ends_with(".f3d") {
            continue;
        }
        let prefix: String = e.name.chars().take(8).collect();
        let label = format!("{name}!{prefix}");
        if description
            .root
            .as_deref()
            .is_some_and(|r| package::is_entry(&e.name, r))
        {
            out.root = Some(label.clone());
        }
        for d in &description.designs {
            if package::is_entry(&e.name, &d.path) {
                out.described.push((label.clone(), d.clone()));
            }
        }
        out.entries.push((label, Some(e.name.clone())));
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
    let mut dump = design.dump(label, &s.segment_dir);
    // The appearances and materials (mitcad#228); their faces and the
    // bodies' records in the blobs come with the inputs.
    dump.f3d.get_or_insert_with(Default::default).appearance =
        Some(appearance::decode(&design.segment, doc));
    // The control cages of Form bodies (mitcad#1365).
    cage::decode_into(&mut dump, &design.segment, doc);
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
        if let Some(a) = dump.f3d.as_mut().and_then(|f| f.appearance.as_mut()) {
            appearance::resolve(a, doc, threads);
        }
    }
}

/// [`resolve_inputs`] for the entities of other documents the design
/// names (mitcad#196): `documents` are those documents' files with the GUIDs
/// the design's inputs name them with (`_f3d.document`) and the design's keys
/// for them; their entities are found in the last state of those files'
/// histories.
pub fn resolve_external_inputs(design: &mut FileDesign, documents: &[inputs::ExternalFile<'_>]) {
    if let Some(dump) = &mut design.dump {
        inputs::resolve_external(dump, documents);
    }
}

/// The placements of the design's derives that keep no matrix, from their
/// source designs' occurrence trees (mitcad#324): `documents` as for
/// [`resolve_external_inputs`], the source designs' files among them.
pub fn resolve_derive_placements(
    design: &mut FileDesign,
    documents: &[inputs::ExternalFile<'_>],
) -> usize {
    design
        .dump
        .as_mut()
        .map_or(0, |dump| derive_sources::resolve(dump, documents))
}

/// Decodes every design of an `.f3d` or `.f3z` file, one document inflated
/// at a time.
pub fn decode_path(path: &Path) -> Result<Vec<FileDesign>, String> {
    package(path)?
        .documents()
        .map(|doc| {
            let doc = doc?;
            decode_document(&doc, doc.label())
        })
        .collect()
}

#[cfg(test)]
mod tests;

/// The largest `Manifest.dat` [`manifest_guids`] reads: a manifest lists a
/// few GUIDs and names (some KB in real files).
pub const MANIFEST_LIMIT: u64 = 16 << 20;

/// The GUIDs (lower case) the `Manifest.dat` files of a document list: an
/// external document's `document_guid` (`ir::ExternalDocument`) is one of
/// those of the file it refers to (mitcad#196). One manifest is inflated
/// at a time, each to at most [`MANIFEST_LIMIT`]; a larger one is skipped
/// (mitcad#923).
pub fn manifest_guids(doc: &F3dFile) -> Vec<String> {
    let mut out = Vec::new();
    for e in doc.archive().entries() {
        if e.name.rsplit('/').next() == Some("Manifest.dat")
            && let Ok(data) = doc.archive().read_with_limit(e, MANIFEST_LIMIT)
        {
            out.extend(build::guid_strings(&data));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A document a design refers to (an external document of its design
/// segment, mitcad#196), as [`referenced_documents`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferencedDocument {
    /// The key references to it carry (16 hex digits).
    pub key: String,
    pub urn: String,
    /// The version the URN names.
    pub version: Option<u32>,
    /// One of the GUIDs of the referenced file's `Manifest.dat`.
    pub document_guid: String,
    /// The design's occurrences of its components.
    pub occurrences: usize,
}

/// The documents a design refers to, with how many occurrences of their
/// components it has, read from the design streams without decoding the
/// design (quick: for asking where they are before an import).
pub fn referenced_documents(doc: &F3dFile) -> Result<Vec<ReferencedDocument>, String> {
    let Some(s) = find_design_streams(doc).map_err(|e| e.to_string())? else {
        return Ok(Vec::new());
    };
    let segment = Segment::parse(&s.meta, s.bulk).map_err(|e| e.to_string())?;
    let mut counts: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for o in segment.objects_of(classes::OCCURRENCE) {
        if let Some(key) = build::occurrence_info(&segment, o.id).and_then(|i| i.context) {
            *counts.entry(key).or_default() += 1;
        }
    }
    Ok(segment
        .meta
        .external
        .iter()
        .map(|x| ReferencedDocument {
            key: format!("{:016x}", x.key),
            urn: x.urn.clone(),
            version: build::urn_version(&x.urn),
            document_guid: x.guid_c.to_lowercase(),
            occurrences: counts.get(&x.key).copied().unwrap_or(0),
        })
        .collect())
}
