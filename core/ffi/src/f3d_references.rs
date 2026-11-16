// SPDX-License-Identifier: MIT
//! The documents an `.f3d` design refers to, imported from local files
//! before the design is replayed, to fill its components of other
//! documents (mitcad#196): where the files are is
//! `mitcad_import::reference_folder`, the filling
//! `mitcad_import::references`. Each document is imported once per import
//! and shared by every occurrence and level that refers to it (mitcad#229).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use mitcad_dump::Dump;
use mitcad_import::components::external_documents;
use mitcad_import::reference_folder::{Found, MAX_DEPTH, Resolver, stem_name};
use mitcad_import::references::Referenced;
use mitcad_import::{DesignReport, ReferenceReport};
use mitcad_model::{BodyUid, OccurrenceUid};

use crate::f3d_import::TimelineOptions;
use crate::kernel::OcctKernel;

/// A document imported from its file for the references to it.
pub(crate) struct Imported {
    document: crate::Document,
    /// Its occurrences by their GUIDs.
    guids: HashMap<String, Vec<OccurrenceUid>>,
    /// Its occurrences by their object ids in its file.
    objects: HashMap<u64, OccurrenceUid>,
    /// Its bodies by the stored bodies they match
    /// ([`Referenced::stored_bodies`]).
    stored: HashMap<String, BodyUid>,
    /// Its bodies by the body objects its file names them by (mitcad#324).
    entities: std::collections::BTreeMap<u64, BodyUid>,
    /// Its import's timeline items by outcome.
    items: BTreeMap<String, usize>,
    /// The documents it refers to in turn.
    references: Vec<ReferenceReport>,
}

/// A document imported for references, or why it could not be.
type ImportedOrWhy = Result<Arc<Imported>, String>;

/// The documents one import has imported for references, by their files
/// and designs (`<canonical file>!<design label>`): each once, or why it
/// could not be.
#[derive(Clone, Default)]
pub(crate) struct ReferenceCache(Arc<Mutex<HashMap<String, ImportedOrWhy>>>);

impl std::fmt::Debug for ReferenceCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.0.lock().map_or(0, |m| m.len());
        write!(f, "ReferenceCache({n} documents)")
    }
}

impl ReferenceCache {
    fn get(&self, key: &str) -> Option<ImportedOrWhy> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn insert(&self, key: String, imported: ImportedOrWhy) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, imported);
    }
}

/// A document the design refers to, imported from its file.
pub(crate) struct Loaded {
    /// Its URN in the design.
    urn: String,
    /// The name its components take.
    name: String,
    imported: Arc<Imported>,
}

/// The documents a design refers to, imported before the design is
/// replayed ([`crate::Document::load_references`]).
#[derive(Default)]
pub(crate) struct References {
    loaded: Vec<Loaded>,
    /// What became of each document; `linked` and the components once the
    /// design's import has filled them ([`References::finish`]).
    entries: Vec<ReferenceReport>,
    /// What looking for the files said.
    warnings: Vec<String>,
}

impl References {
    /// The documents imported, by their URNs, for the design's import.
    pub(crate) fn sources(&self) -> HashMap<String, Referenced<'_, OcctKernel>> {
        self.loaded
            .iter()
            .map(|l| {
                (
                    l.urn.clone(),
                    Referenced {
                        document: &l.imported.document.0,
                        occurrence_guids: &l.imported.guids,
                        occurrence_objects: &l.imported.objects,
                        name: l.name.clone(),
                        stored_bodies: &l.imported.stored,
                        entity_bodies: &l.imported.entities,
                    },
                )
            })
            .collect()
    }

    /// Whether the design refers to documents (imported or not).
    pub(crate) fn has_documents(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Puts what became of the documents into the design's report once its
    /// import has filled the components from them (`document` is the
    /// design's).
    pub(crate) fn finish(
        &self,
        path: &str,
        document: &mitcad_model::Document<OcctKernel>,
        design: &mut DesignReport,
    ) {
        design.warnings.extend(self.warnings.iter().cloned());
        let mut entries = self.entries.clone();
        for e in &mut entries {
            let inserted: Vec<_> = design
                .components
                .inserted
                .iter()
                .filter(|i| i.document == e.document)
                .collect();
            e.occurrences = inserted.iter().map(|i| i.occurrences).sum();
            e.components = inserted
                .iter()
                .map(|i| document.assembly().name(i.component))
                .collect();
            let linked: Vec<String> = design
                .components
                .link
                .linked
                .iter()
                .filter(|l| l.document == e.document)
                .map(|l| l.name.clone())
                .collect();
            if !linked.is_empty() {
                e.status = "linked".to_owned();
                e.components = linked;
            }
            if let Some((_, why)) = design
                .components
                .link
                .failed
                .iter()
                .find(|(d, _)| *d == e.document)
            {
                e.status = "failed".to_owned();
                e.note = Some(why.clone());
            }
        }
        let missing: Vec<String> = entries
            .iter()
            .filter(|e| e.status != "linked")
            .map(|e| format!("{} ({})", e.title(), e.status))
            .collect();
        if !missing.is_empty() {
            design.warnings.push(format!(
                "components of other documents stay empty, their documents not imported: {}; \
                 the documents are looked for in the folder next to the design ({}), in the \
                 folders and files given for them, and by the GUIDs their files list",
                missing.join(", "),
                Path::new(path)
                    .with_extension("refs")
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
        }
        if !design.components.link.positions_left_out.is_empty() {
            let left: Vec<String> = design
                .components
                .link
                .positions_left_out
                .iter()
                .map(|(why, n)| format!("{n} ({why})"))
                .collect();
            design.warnings.push(format!(
                "captured positions inside components of other documents left out: {}",
                left.join(", ")
            ));
        }
        design.references = entries;
    }
}

impl crate::Document {
    /// Imports the documents the design (decoded in `dump`, the design
    /// `label` of the file) refers to, each from its file with its own
    /// references, so that the design's components standing for their
    /// components can be filled from them when it is replayed
    /// ([`mitcad_import::import_design_with`]); the report says where each
    /// document was found and how it came in. A document already imported
    /// by this import (for another reference or level) is not imported
    /// again (mitcad#229).
    pub(crate) fn load_references(
        &mut self,
        path: &str,
        label: &str,
        options: &TimelineOptions,
        dump: &Dump,
    ) -> References {
        let mut out = References::default();
        let documents = external_documents(dump);
        if documents.is_empty() {
            return out;
        }
        let mut resolver = Resolver::new(
            Path::new(path),
            &options.references,
            &options.reference_files,
        );
        out.warnings.extend(resolver.warnings().iter().cloned());
        let here = std::fs::canonicalize(path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_owned());
        // A design is known by its file and, in an `.f3z` package, its
        // label (the label `Found::design` names it by).
        let key = |file: &str, label: Option<&str>| format!("{file}!{}", label.unwrap_or(""));
        let is_package = Path::new(path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("f3z"));
        let mut stack = options.stack.clone();
        stack.push(key(&here, is_package.then_some(label)));
        let avoid = avoided(path, label, &stack);
        let cache = options.reference_cache.clone().unwrap_or_default();
        let trace = std::env::var_os("MITCAD_IMPORT_TRACE").is_some();
        for external in documents {
            let urn = external.urn;
            let found = if urn.is_empty() {
                Found::default()
            } else {
                resolver.resolve_avoiding(&urn, external.guid.as_deref(), &avoid)
            };
            let mut entry = ReferenceReport {
                document: urn.clone(),
                name: found.name.clone(),
                version: found.version,
                file: found
                    .file
                    .as_ref()
                    .map(|f| f.to_string_lossy().into_owned()),
                found: found.how.map(str::to_owned),
                status: "missing".to_owned(),
                note: found.note.clone(),
                ..ReferenceReport::default()
            };
            let Some(file) = found.file.clone() else {
                if options.skipped_references.contains(&urn) {
                    entry.status = "skipped".to_owned();
                }
                out.entries.push(entry);
                continue;
            };
            let file = file.to_string_lossy().into_owned();
            if entry.name.is_none() {
                entry.name = Some(stem_name(&file));
            }
            let canonical = std::fs::canonicalize(&file)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| file.clone());
            let this = key(&canonical, found.design.as_deref());
            if stack.contains(&this) {
                entry.status = "failed".to_owned();
                entry.note = Some("it refers back to a design being imported".to_owned());
                out.entries.push(entry);
                continue;
            }
            if stack.len() >= MAX_DEPTH {
                entry.status = "failed".to_owned();
                entry.note = Some(format!(
                    "references of references are followed {MAX_DEPTH} levels deep"
                ));
                out.entries.push(entry);
                continue;
            }
            let imported = match cache.get(&this) {
                Some(imported) => {
                    if trace {
                        eprintln!("import: the document {urn} again, imported before");
                    }
                    imported
                }
                None => {
                    if trace {
                        eprintln!(
                            "import: the document {urn} from {file} ({})",
                            found.design.as_deref().unwrap_or("its design")
                        );
                    }
                    let sub_options = TimelineOptions {
                        design: found.design.clone(),
                        dump: None,
                        text: false,
                        report_path: None,
                        list: false,
                        list_references: false,
                        learn: None,
                        references: resolver.folders(),
                        reference_files: resolver.files(),
                        stack: [stack.clone(), vec![this.clone()]].concat(),
                        reference_cache: Some(cache.clone()),
                        // (The items of the design opened only, mitcad#379.)
                        accept_deviations: None,
                        ..options.clone()
                    };
                    let imported = self.import_reference(&file, &sub_options);
                    cache.insert(this, imported.clone());
                    imported
                }
            };
            match imported {
                Ok(imported) => {
                    entry.items = imported.items.clone();
                    entry.references = imported.references.clone();
                    out.loaded.push(Loaded {
                        urn,
                        name: entry.name.clone().unwrap_or_else(|| stem_name(&file)),
                        imported,
                    });
                    // ("linked" once the design's import has filled them.)
                    entry.status = "found".to_owned();
                }
                Err(why) => {
                    entry.status = "failed".to_owned();
                    entry.note = Some(why);
                }
            }
            out.entries.push(entry);
        }
        out
    }

    /// Imports a document the design refers to from `file` into a document
    /// of its own (a stop of this import stops it too).
    fn import_reference(
        &self,
        file: &str,
        options: &TimelineOptions,
    ) -> Result<Arc<Imported>, String> {
        let mut document = crate::Document(mitcad_model::Document::new(OcctKernel));
        document.0.set_monitor(self.0.monitor().cloned());
        let mut sub = document.import_timeline(file, options).map_err(|e| e.0)?;
        if sub.designs.is_empty() {
            return Err("the file holds no design".to_owned());
        }
        let d = sub.designs.remove(0);
        // Its bodies by the stored bodies they match: the report's
        // `F36.b0 Body3` is the body `F36.b0`.
        let stored = d
            .bodies
            .iter()
            .filter_map(|b| {
                let uid = b.mitcad.as_deref()?.split_whitespace().next()?;
                Some((b.file.clone(), uid.parse::<BodyUid>().ok()?))
            })
            .collect();
        Ok(Arc::new(Imported {
            items: d
                .counts()
                .into_iter()
                .map(|(o, n)| (o.as_str().to_owned(), n))
                .collect(),
            references: d.references.clone(),
            guids: d.components.occurrence_guids.clone(),
            objects: d.components.occurrence_objects.clone(),
            stored,
            entities: d.body_entities.clone(),
            document,
        }))
    }
}

/// The files of the documents the design refers to (found as
/// [`crate::Document::load_references`] finds them), opened, with the GUIDs
/// its inputs name their entities with (lower case) and its keys for them:
/// for the entities it names in them (`mitcad_f3d_design::resolve_external_inputs`).
/// `label` is the design's label in its file.
pub(crate) fn external_files(
    path: &str,
    label: &str,
    options: &TimelineOptions,
    dump: &Dump,
) -> Vec<(String, String, mitcad_f3d_file::F3dFile)> {
    let mut documents = external_documents(dump);
    // The derives' source designs, for their placements (mitcad#324).
    for d in mitcad_import::derive::source_documents(dump) {
        if !documents.iter().any(|x| x.urn == d.urn) {
            documents.push(d);
        }
    }
    let mut out = Vec::new();
    if documents.is_empty() {
        return out;
    }
    let mut resolver = Resolver::new(
        Path::new(path),
        &options.references,
        &options.reference_files,
    );
    let avoid = avoided(path, label, &options.stack);
    for d in documents {
        let Some(guid) = d.guid.as_deref().map(str::to_lowercase) else {
            continue;
        };
        let found = resolver.resolve_avoiding(&d.urn, d.guid.as_deref(), &avoid);
        let Some(file) = found.file else {
            continue;
        };
        let Ok(designs) = mitcad_f3d_design::package(&file) else {
            continue;
        };
        let wanted = found.design;
        let pick = designs.labels().position(|label| match &wanted {
            Some(w) => label == w.as_str() || label.ends_with(&format!("!{w}")),
            None => true,
        });
        // Only the design picked is inflated (mitcad#923).
        if let Some(index) = pick
            && let Ok((_, f)) = designs.into_document(index)
        {
            out.push((guid, d.key.clone().unwrap_or_default(), f));
        }
    }
    out
}

/// The labels of the designs of the package `path` that are being imported
/// (`label` and those on the import's `stack`): not taken for a document
/// by the GUIDs their manifests list (mitcad#229).
fn avoided(path: &str, label: &str, stack: &[String]) -> Vec<String> {
    let here = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned());
    let prefix = format!("{here}!");
    let mut out = vec![label.to_owned()];
    for k in stack {
        if let Some(l) = k.strip_prefix(&prefix)
            && !l.is_empty()
            && !out.iter().any(|o| o == l)
        {
            out.push(l.to_owned());
        }
    }
    out
}
