// SPDX-License-Identifier: MIT
//! The documents an `.f3d` design refers to, imported from local files
//! before the design is replayed, to fill its components of other
//! documents (mitcad#196): where the files are is
//! `mitcad_import::reference_folder`, the filling
//! `mitcad_import::references`. Each document is imported once per import
//! and shared by every occurrence and level that refers to it (mitcad#229).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

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
    /// Those with their GUIDs and the documents filled in it in turn
    /// ([`Referenced::object_guids`], mitcad#2528).
    object_guids: Arc<mitcad_import::references::ObjectGuids>,
    /// Its bodies by the stored bodies they match
    /// ([`Referenced::stored_bodies`]).
    stored: HashMap<String, BodyUid>,
    /// Its bodies by the body objects its file names them by (mitcad#324).
    entities: std::collections::BTreeMap<u64, BodyUid>,
    /// Its import's timeline items by outcome.
    items: BTreeMap<String, usize>,
    /// The documents it refers to in turn.
    references: Vec<ReferenceReport>,
    /// Its import's fallbacks, light bulbs and joints, for the report
    /// (mitcad#2449).
    fallbacks: Vec<String>,
    light_bulbs: mitcad_import::LightBulbReport,
    joints: mitcad_import::JointReport,
}

/// A document imported for references with how long its import took (s),
/// or why it could not be.
type ImportedOrWhy = Result<(Arc<Imported>, f64), String>;

/// A document the design refers to, as [`crate::Document::load_references`]
/// plans it: its entry as it stands, or its import to run.
enum Planned {
    Done(Box<ReferenceReport>),
    Import(Box<ToImport>),
}

/// A document to import for the design, and once imported, the import
/// (and whether this reference's import made it, not an earlier one).
struct ToImport {
    entry: ReferenceReport,
    urn: String,
    file: String,
    design: Option<String>,
    /// [`ReferenceCache`]'s key.
    key: String,
    options: TimelineOptions,
    result: Mutex<Option<(ImportedOrWhy, bool)>>,
}

/// The documents one import has imported for references, by their files
/// and designs (`<canonical file>!<design label>`): each once, or why it
/// could not be. Documents are imported side by side (mitcad#2449,
/// [`crate::Document::load_references`]): one being imported by another
/// thread is waited for, not imported again.
#[derive(Clone, Default)]
pub(crate) struct ReferenceCache(Arc<(Mutex<CacheState>, Condvar)>);

/// What a [`ReferenceCache`] knows.
#[derive(Default)]
struct CacheState {
    /// The documents imported, or why not.
    done: HashMap<String, ImportedOrWhy>,
    /// Those being imported now.
    pending: HashSet<String>,
    /// What each design being imported waits for: the documents it is
    /// importing or waiting on another thread to import. Waiting where
    /// this leads back to the waiting design would never end (designs that
    /// refer to each other, imported from two sides at once).
    waits: HashMap<String, Vec<String>>,
}

impl CacheState {
    /// Whether `from` waits, directly or through others, for `to`.
    fn reaches(&self, from: &str, to: &str) -> bool {
        let mut seen = HashSet::new();
        let mut todo = vec![from];
        while let Some(k) = todo.pop() {
            if k == to {
                return true;
            }
            if seen.insert(k) {
                todo.extend(self.waits.get(k).into_iter().flatten().map(String::as_str));
            }
        }
        false
    }

    fn unwait(&mut self, waiter: &str, key: &str) {
        if let Some(list) = self.waits.get_mut(waiter) {
            if let Some(i) = list.iter().position(|k| k == key) {
                list.remove(i);
            }
            if list.is_empty() {
                self.waits.remove(waiter);
            }
        }
    }
}

impl std::fmt::Debug for ReferenceCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.0.0.lock().map_or(0, |m| m.done.len());
        write!(f, "ReferenceCache({n} documents)")
    }
}

/// Marks a document imported when its import ends, also by a panic (then
/// as failed), so that no thread waits for it for ever.
struct Finish<'a> {
    cache: &'a ReferenceCache,
    waiter: &'a str,
    key: &'a str,
    result: Option<ImportedOrWhy>,
}

impl Drop for Finish<'_> {
    fn drop(&mut self) {
        let result = self
            .result
            .take()
            .unwrap_or_else(|| Err("its import stopped".to_owned()));
        let (lock, ready) = &*self.cache.0;
        let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
        state.pending.remove(self.key);
        state.done.insert(self.key.to_owned(), result);
        state.unwait(self.waiter, self.key);
        ready.notify_all();
    }
}

impl ReferenceCache {
    /// The document `key` that the import of the design `waiter` needs:
    /// imported before, else imported here with `import` (then true), else
    /// waited for while another thread imports it. Where waiting would
    /// never end (the other import waits, through others, for `waiter`),
    /// it is refused as a document that refers back to a design being
    /// imported, as the import refuses a design on its own stack.
    fn get_or_import(
        &self,
        waiter: &str,
        key: &str,
        import: impl FnOnce() -> ImportedOrWhy,
    ) -> (ImportedOrWhy, bool) {
        let (lock, ready) = &*self.0;
        let mut state = lock.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(done) = state.done.get(key) {
                return (done.clone(), false);
            }
            if !state.pending.contains(key) {
                break;
            }
            if state.reaches(key, waiter) {
                return (
                    Err("it refers back to a design being imported".to_owned()),
                    false,
                );
            }
            state
                .waits
                .entry(waiter.to_owned())
                .or_default()
                .push(key.to_owned());
            state = ready.wait(state).unwrap_or_else(PoisonError::into_inner);
            state.unwait(waiter, key);
        }
        state.pending.insert(key.to_owned());
        state
            .waits
            .entry(waiter.to_owned())
            .or_default()
            .push(key.to_owned());
        drop(state);
        let mut finish = Finish {
            cache: self,
            waiter,
            key,
            result: None,
        };
        let result = import();
        finish.result = Some(result.clone());
        drop(finish);
        (result, true)
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
                        object_guids: Some(l.imported.object_guids.clone()),
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

/// The documents a design refers to as its import plans them
/// ([`plan_references`]).
struct Plan {
    /// Each document's entry as it stands, or its import, in the design's
    /// order.
    planned: Vec<Planned>,
    /// What looking for the files said.
    warnings: Vec<String>,
    /// The files being imported with the design on top.
    stack: Vec<String>,
}

/// Where each document the design (decoded in `dump`, the design `label` of
/// the file `path`) refers to is, and the import that brings it in with its
/// options (`cache`, the import's documents imported so far): first where
/// each is (the resolver learns as it goes, so in the design's order).
/// [`crate::Document::load_references`] imports them;
/// [`replayed_items`] counts what those imports replay (mitcad#2635), so
/// both follow the same documents.
fn plan_references(
    path: &str,
    label: &str,
    options: &TimelineOptions,
    dump: &Dump,
    cache: &ReferenceCache,
) -> Plan {
    let mut plan = Plan {
        planned: Vec::new(),
        warnings: Vec::new(),
        stack: options.stack.clone(),
    };
    let documents = external_documents(dump);
    if documents.is_empty() {
        return plan;
    }
    let mut resolver = Resolver::new(
        Path::new(path),
        &options.references,
        &options.reference_files,
    );
    plan.warnings.extend(resolver.warnings().iter().cloned());
    let here = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned());
    // A design is known by its file and, in an `.f3z` package, its
    // label (the label `Found::design` names it by).
    let key = |file: &str, label: Option<&str>| format!("{file}!{}", label.unwrap_or(""));
    let is_package = Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("f3z"));
    let stack = &mut plan.stack;
    stack.push(key(&here, is_package.then_some(label)));
    let avoid = avoided(path, label, stack);
    let planned = &mut plan.planned;
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
            planned.push(Planned::Done(Box::new(entry)));
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
            planned.push(Planned::Done(Box::new(entry)));
            continue;
        }
        if stack.len() >= MAX_DEPTH {
            entry.status = "failed".to_owned();
            entry.note = Some(format!(
                "references of references are followed {MAX_DEPTH} levels deep"
            ));
            planned.push(Planned::Done(Box::new(entry)));
            continue;
        }
        let options = TimelineOptions {
            design: found.design.clone(),
            dump: None,
            text: false,
            report_path: None,
            list: false,
            list_references: false,
            count_items: false,
            learn: None,
            references: resolver.folders(),
            reference_files: resolver.files(),
            stack: [stack.clone(), vec![this.clone()]].concat(),
            reference_cache: Some(cache.clone()),
            // (The items of the design opened only, mitcad#379.)
            accept_deviations: None,
            // Its items' traces name it (mitcad#2635).
            document_name: entry.name.clone(),
            ..options.clone()
        };
        planned.push(Planned::Import(Box::new(ToImport {
            entry,
            urn,
            file,
            design: found.design,
            key: this,
            options,
            result: Mutex::new(None),
        })));
    }
    plan
}

/// The timeline items an import of `path` with `options` replays
/// (mitcad#2635), read from the files, importing nothing: its design's
/// ([`crate::f3d_import::design_to_replay`]) and those of the documents it
/// refers to that the import imports ([`plan_references`], their own
/// references in turn), each document once however often and on whatever
/// level it is referred to, as the import's [`ReferenceCache`] imports it
/// once. `{"items": <sum>, "designs": [{"file", "design", "items"}]}`, the
/// design opened first, then the documents in the order they are planned.
/// The application's progress dialog counts the items against this sum.
pub(crate) fn replayed_items(
    path: &str,
    options: &TimelineOptions,
) -> Result<serde_json::Value, mitcad_model_api::ApiError> {
    let mut designs = Vec::new();
    let mut seen = HashSet::new();
    count_design(path, options, &mut seen, &mut designs)?;
    let items: u64 = designs
        .iter()
        .map(|d| d["items"].as_u64().unwrap_or(0))
        .sum();
    Ok(serde_json::json!({"items": items, "designs": designs}))
}

/// [`replayed_items`] of one design and, depth first, the documents it
/// refers to that `seen` (their keys) does not hold yet.
fn count_design(
    path: &str,
    options: &TimelineOptions,
    seen: &mut HashSet<String>,
    designs: &mut Vec<serde_json::Value>,
) -> Result<(), mitcad_model_api::ApiError> {
    let (label, dump) = crate::f3d_import::design_to_replay(path, options)?;
    let items = dump.as_ref().map_or(0, |d| d.timeline_items().len());
    designs.push(serde_json::json!({"file": path, "design": label, "items": items}));
    // (Without a design, or without references, the import imports no
    // document either.)
    let Some(dump) = dump.filter(|_| !options.no_references) else {
        return Ok(());
    };
    let plan = plan_references(path, &label, options, &dump, &ReferenceCache::default());
    // (Its design let go before the documents are read, mitcad#923.)
    drop(dump);
    for planned in plan.planned {
        if let Planned::Import(task) = planned
            && seen.insert(task.key.clone())
        {
            // A document whose file does not read is not imported either:
            // it replays nothing.
            let _ = count_design(&task.file, &task.options, seen, designs);
        }
    }
    Ok(())
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
        let cache = options.reference_cache.clone().unwrap_or_default();
        let Plan {
            planned,
            warnings,
            stack,
        } = plan_references(path, label, options, dump, &cache);
        if planned.is_empty() {
            return out;
        }
        out.warnings.extend(warnings);
        let trace = std::env::var_os("MITCAD_IMPORT_TRACE").is_some();
        // First where each document is (plan_references), then their
        // imports, side by side with more than one thread (mitcad#2449),
        // then the entries, again in the design's order: the outcome does
        // not depend on which import ends first.
        let waiter = stack.last().cloned().unwrap_or_default();
        let monitor = self.0.monitor().cloned();
        let import = |task: &ToImport| {
            let (imported, fresh) = cache.get_or_import(&waiter, &task.key, || {
                if trace {
                    eprintln!(
                        "import: the document {} from {} ({})",
                        task.urn,
                        task.file,
                        task.design.as_deref().unwrap_or("its design")
                    );
                }
                let clock = std::time::Instant::now();
                let imported = import_reference(monitor.clone(), &task.file, &task.options);
                if trace {
                    eprintln!(
                        "import: the document {} imported in {:.2} s",
                        task.urn,
                        clock.elapsed().as_secs_f64()
                    );
                }
                imported.map(|i| (i, clock.elapsed().as_secs_f64()))
            });
            if trace && !fresh {
                eprintln!("import: the document {} again, imported before", task.urn);
            }
            *task.result.lock().unwrap_or_else(PoisonError::into_inner) = Some((imported, fresh));
        };
        let tasks: Vec<&ToImport> = planned
            .iter()
            .filter_map(|p| match p {
                Planned::Import(task) => Some(&**task),
                Planned::Done(_) => None,
            })
            .collect();
        // The design opened only: the documents its documents refer to are
        // imported in their threads one after another, so that at most
        // `threads` documents are imported at once.
        let workers = if options.stack.is_empty() {
            options.threads.unwrap_or(1).max(1).min(tasks.len())
        } else {
            1
        };
        if workers > 1 {
            // Each a document at a time, the next one free in the design's
            // order; a thread with the stack of an import's own thread.
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| {
                for _ in 0..workers {
                    let work = || {
                        while let Some(task) = tasks.get(next.fetch_add(1, Ordering::Relaxed)) {
                            import(task);
                        }
                    };
                    let spawned = std::thread::Builder::new()
                        .name("mitcad-import-references".to_owned())
                        .stack_size(crate::f3d_import::IMPORT_STACK)
                        .spawn_scoped(scope, work);
                    // (Not started: the others, or this thread, do its share.)
                    drop(spawned);
                }
            });
        }
        // Those no thread imported (one thread, or none started).
        for task in &tasks {
            let done = task
                .result
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_some();
            if !done {
                import(task);
            }
        }
        for plan in planned {
            let task = match plan {
                Planned::Done(entry) => {
                    out.entries.push(*entry);
                    continue;
                }
                Planned::Import(task) => *task,
            };
            let ToImport {
                mut entry,
                urn,
                file,
                result,
                ..
            } = task;
            let (imported, fresh) = result
                .into_inner()
                .unwrap_or_else(PoisonError::into_inner)
                .unwrap_or_else(|| (Err("its import did not run".to_owned()), false));
            let imported = imported.map(|(imported, seconds)| {
                if fresh {
                    entry.seconds = Some(seconds);
                }
                imported
            });
            match imported {
                Ok(imported) => {
                    entry.items = imported.items.clone();
                    entry.references = imported.references.clone();
                    entry.fallbacks = imported.fallbacks.clone();
                    entry.light_bulbs = Some(imported.light_bulbs.clone());
                    entry.joints = (imported.joints != mitcad_import::JointReport::default())
                        .then(|| imported.joints.clone());
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
}

/// Imports a document the design refers to from `file` into a document of
/// its own (`monitor`, the design's: a stop of the design's import stops
/// it too).
fn import_reference(
    monitor: Option<Arc<mitcad_model::RecomputeMonitor>>,
    file: &str,
    options: &TimelineOptions,
) -> Result<Arc<Imported>, String> {
    let mut document = crate::Document(mitcad_model::Document::new(OcctKernel));
    document.0.set_monitor(monitor);
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
        fallbacks: d
            .items
            .iter()
            .filter(|i| i.outcome == mitcad_import::Outcome::Fallback)
            .map(|i| match &i.note {
                Some(note) => format!("{}: {note}", i.name),
                None => i.name.clone(),
            })
            .collect(),
        light_bulbs: d.light_bulbs.clone(),
        joints: d.joints.clone(),
        guids: d.components.occurrence_guids.clone(),
        objects: d.components.occurrence_objects.clone(),
        object_guids: d.components.object_guids.clone(),
        stored,
        entities: d.body_entities.clone(),
        document,
    }))
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

#[cfg(test)]
mod tests {
    //! The cache of the documents imported for references, as threads
    //! import them side by side (mitcad#2449); failed imports stand for
    //! imported documents.
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn why(result: &ImportedOrWhy) -> String {
        match result {
            Ok(_) => "imported".to_owned(),
            Err(why) => why.clone(),
        }
    }

    #[test]
    fn a_document_is_imported_once() {
        let cache = ReferenceCache::default();
        let (first, fresh) = cache.get_or_import("design", "part", || Err("made".to_owned()));
        assert_eq!((why(&first), fresh), ("made".to_owned(), true));
        let (again, fresh) = cache.get_or_import("other", "part", || panic!("imported twice"));
        assert_eq!((why(&again), fresh), ("made".to_owned(), false));
    }

    #[test]
    fn a_document_another_thread_imports_is_waited_for() {
        let cache = ReferenceCache::default();
        let (started, has_started) = mpsc::channel();
        let (go, may_go) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let c = &cache;
            let first = scope.spawn(move || {
                c.get_or_import("design", "part", || {
                    started.send(()).unwrap();
                    may_go.recv().unwrap();
                    Err("made by the first".to_owned())
                })
            });
            has_started.recv().unwrap();
            let second = scope
                .spawn(move || c.get_or_import("assembly", "part", || panic!("imported twice")));
            // (The second waits; it cannot end before the first.)
            std::thread::sleep(Duration::from_millis(50));
            assert!(!second.is_finished());
            go.send(()).unwrap();
            let (first, fresh) = first.join().unwrap();
            assert_eq!((why(&first), fresh), ("made by the first".to_owned(), true));
            let (second, fresh) = second.join().unwrap();
            assert_eq!(
                (why(&second), fresh),
                ("made by the first".to_owned(), false)
            );
        });
    }

    #[test]
    fn documents_that_refer_to_each_other_do_not_wait_for_ever() {
        // Two threads import `a` and `b`, and each needs the other inside:
        // one of them waits, the other is refused as a design that refers
        // back, and both end.
        let cache = ReferenceCache::default();
        let (a_started, a_has_started) = mpsc::channel();
        let (b_started, b_has_started) = mpsc::channel();
        std::thread::scope(|scope| {
            let c = &cache;
            let a = scope.spawn(move || {
                c.get_or_import("design", "a", || {
                    a_started.send(()).unwrap();
                    b_has_started.recv().unwrap();
                    c.get_or_import("a", "b", || panic!("b imported twice")).0
                })
            });
            let b = scope.spawn(move || {
                c.get_or_import("design", "b", || {
                    b_started.send(()).unwrap();
                    a_has_started.recv().unwrap();
                    c.get_or_import("b", "a", || panic!("a imported twice")).0
                })
            });
            let (a, _) = a.join().unwrap();
            let (b, _) = b.join().unwrap();
            let refused = "it refers back to a design being imported".to_owned();
            assert!(
                why(&a) == refused || why(&b) == refused,
                "{} / {}",
                why(&a),
                why(&b)
            );
        });
    }
}
