// SPDX-License-Identifier: MIT
//! The `.iam` import (mitcad#60, stage 4): an assembly's occurrences as
//! Mitcad's components and occurrences (`mitcad_ipt::assembly` reads them).
//!
//! - Every distinct part file is imported once with the `.ipt` import, into
//!   a document of its own: its stored bodies, one base feature of a
//!   component of the assembly (the shapes shared), or with `history` its
//!   design replayed and copied into a component
//!   ([`mitcad_model::Document::add_component_copy`]); each occurrence of
//!   the part is an occurrence of that component.
//! - A sub-assembly is a component whose occurrences are placed in it the
//!   same way (each sub-assembly file once).
//! - The placements are the file's (cm become mm); hidden occurrences are
//!   hidden, grounded ones grounded; suppressed occurrences are left out.
//! - Referenced files are found where the assembly saved them, else
//!   relative to the assembly as they were when it was saved, else by the
//!   saved path's tail under the assembly's folders, else by name in the
//!   search folders (the assembly's folder tree and those given); a part
//!   found so is checked to be the document the assembly refers to (its
//!   own list names the version id the occurrence stores). A file not
//!   found is an empty component, so the structure and the placements
//!   stay; the report names it.
//! - Each placement is checked against the transform the file displays
//!   the occurrence with (the graphics segment, where the file keeps it),
//!   and each placed part's bodies against the range box the file stores
//!   for the document: they must lie within it (the stored box is not
//!   tight: curved faces make it larger). The stored centre of the placed
//!   range box is compared too, for information: the file does not always
//!   keep it up to date.
//!
//! The `import_iam` document command (also `import_ipt` with an `.iam`
//! file), `mitcad-cli import-iam`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_ipt::assembly::{Assembly, Occurrence, Reference};
use mitcad_model::api::ApiError;
use mitcad_model::assembly::from_rows;
use mitcad_model::{ComponentUid, Kernel, OccurrenceUid, Transform};

use crate::Document;
use crate::kernel::OcctKernel;

/// The options of `import_iam` (JSON).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
    /// Each part with its design (parameters and features replayed against
    /// its history, as `import_ipt` does), not only its stored bodies.
    /// Replaying every part takes much longer and much more memory.
    #[serde(default)]
    history: bool,
    /// With `history`: the parts' features are not checked against their
    /// history.
    #[serde(default)]
    no_verify: bool,
    /// The parts' features that cannot be replayed are left out.
    #[serde(default)]
    no_fallback: bool,
    /// The parts' final bodies are compared by volume only.
    #[serde(default)]
    no_compare: bool,
    /// Seconds after which each part's remaining features take the file's
    /// bodies.
    #[serde(default)]
    time_limit: Option<f64>,
    /// With `history`: seconds without progress after which a geometry
    /// kernel call in a part's replay counts as hung (`import_ipt`'s
    /// `hang_limit`).
    #[serde(default)]
    hang_limit: Option<f64>,
    /// More folders searched for referenced files by name.
    #[serde(default)]
    search: Vec<String>,
}

/// How deep sub-assemblies are followed.
const MAX_DEPTH: usize = 32;

/// The most files indexed in the search folders.
const MAX_INDEXED: usize = 500_000;

/// Bodies lie within a range box when they reach out of it by at most this
/// share of the box's diagonal, at least [`MIN_TOLERANCE`] mm.
const BOX_TOLERANCE: f64 = 0.01;
const MIN_TOLERANCE: f64 = 0.05;

/// A placement agrees with the displayed one within this (mm, a rotation's
/// difference taken at 1 m).
const DISPLAY_TOLERANCE: f64 = 1e-3;

/// Whether `path` is an assembly file (by the class id of its root
/// storage; files that cannot be read are not).
pub(crate) fn is_assembly(path: &str) -> bool {
    mitcad_ipt::cfb::CompoundFile::open(Path::new(path)).is_ok_and(|c| {
        mitcad_ipt::cfb::guid_text(&c.root().clsid) == mitcad_ipt::assembly::ASSEMBLY_CLSID
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned()
}

/// A path without `.` and `..` (taken away lexically, without following
/// links, so that a Windows path keeps its form).
fn normalized(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A saved path's parts (either separator), without empty ones.
fn parts(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|p| !p.is_empty()).collect()
}

/// How a referenced file was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Found {
    AsSaved,
    Relative,
    Tail,
    Name,
    Missing,
}

impl Found {
    fn text(self) -> &'static str {
        match self {
            Found::AsSaved => "as saved",
            Found::Relative => "relative to the assembly",
            Found::Tail => "by the saved path's tail",
            Found::Name => "by name",
            Found::Missing => "missing",
        }
    }
}

/// A referenced file of the import, once per resolved path (or per saved
/// path when missing).
struct File {
    saved: String,
    path: Option<PathBuf>,
    found: Found,
    assembly: bool,
    /// The component made for it (None: not made, e.g. failed).
    component: Option<ComponentUid>,
    /// Imported, missing, failed (why), or a cycle.
    status: String,
    /// The occurrence's document id is in the found part's own list.
    identity: Option<bool>,
    occurrences: usize,
    /// The part's import report summary.
    part: Option<Value>,
    /// The bodies' bounding box in the component (mm).
    bounds: Option<Option<([f64; 3], [f64; 3])>>,
}

/// One import run.
struct Import<'a> {
    doc: &'a mut Document,
    options: &'a Options,
    roots: Vec<PathBuf>,
    /// Files in the search folders by lower-case name.
    index: Option<HashMap<String, Vec<PathBuf>>>,
    files: Vec<File>,
    /// Files by resolved path (canonical) or saved path when missing.
    by_key: HashMap<String, usize>,
    /// Assemblies being placed (cycles).
    stack: Vec<PathBuf>,
    warnings: Vec<String>,
    /// Occurrences: placed, suppressed, hidden, of missing files, without
    /// a file or placement, non-rigid.
    counts: BTreeMap<&'static str, usize>,
    /// Range box checks: compared, agreeing, largest differences (mm).
    boxes: Checks,
    centers: Checks,
    /// Placements against the displayed ones.
    display: Checks,
    /// Range boxes of parts saved after the assembly (information).
    stale: Checks,
    /// The occurrences (nested ones by path), for the report.
    list: Vec<Value>,
}

#[derive(Default)]
struct Checks {
    compared: usize,
    agree: usize,
    worst: f64,
    failures: Vec<String>,
}

impl Checks {
    fn add(&mut self, difference: f64, tolerance: f64, what: impl FnOnce() -> String) {
        self.compared += 1;
        self.worst = self.worst.max(difference);
        if difference <= tolerance {
            self.agree += 1;
        } else if self.failures.len() < 20 {
            self.failures.push(what());
        }
    }

    fn value(&self) -> Value {
        json!({"compared": self.compared, "agree": self.agree, "worst": self.worst,
               "failures": self.failures})
    }
}

impl Import<'_> {
    fn count(&mut self, what: &'static str) {
        *self.counts.entry(what).or_default() += 1;
    }

    /// Places the occurrences of the assembly at `path` in `parent`
    /// (`prefix`: the occurrence path's names, for the report).
    fn place(&mut self, path: &Path, parent: ComponentUid, prefix: &str, depth: usize) {
        if depth > MAX_DEPTH {
            self.warnings.push(format!(
                "{}: sub-assemblies nested too deeply",
                path.display()
            ));
            return;
        }
        let file = match mitcad_ipt::IptFile::open(path) {
            Ok(f) => f,
            Err(e) => {
                self.warnings.push(format!("{}: {e}", path.display()));
                return;
            }
        };
        if !file.is_assembly() {
            self.warnings.push(format!(
                "{}: the root storage is not an assembly's",
                path.display()
            ));
        }
        let assembly = match mitcad_ipt::assembly::read(&file) {
            Ok(a) => a,
            Err(e) => {
                self.warnings.push(format!("{}: {e}", path.display()));
                return;
            }
        };
        let name = file_name(path);
        self.warnings
            .extend(assembly.warnings.iter().map(|w| format!("{name}: {w}")));
        self.stack.push(path.to_path_buf());
        for occurrence in &assembly.occurrences {
            self.occurrence(path, &assembly, occurrence, parent, prefix, depth);
        }
        self.stack.pop();
    }

    fn occurrence(
        &mut self,
        assembly_path: &Path,
        assembly: &Assembly,
        o: &Occurrence,
        parent: ComponentUid,
        prefix: &str,
        depth: usize,
    ) {
        let reference = o.reference.and_then(|id| assembly.reference(id));
        let name = format!("{prefix}{}", o.name(reference));
        if o.suppressed() {
            self.count("suppressed");
            self.list.push(json!({"name": name, "suppressed": true}));
            return;
        }
        let Some(reference) = reference else {
            self.count("without_file");
            self.warnings
                .push(format!("{name}: the occurrence names no referenced file"));
            return;
        };
        let Some(matrix) = o.transform else {
            self.count("without_placement");
            self.warnings.push(format!("{name}: no placement"));
            return;
        };
        let rows: Vec<Vec<f64>> = (0..3)
            .map(|r| {
                vec![
                    matrix[r][0],
                    matrix[r][1],
                    matrix[r][2],
                    matrix[r][3] * 10.0,
                ]
            })
            .collect();
        let transform = match from_rows(&rows) {
            Ok(t) => t,
            Err(e) => {
                self.count("not_rigid");
                self.warnings
                    .push(format!("{name}: {e}; placed at the origin"));
                Transform::IDENTITY
            }
        };
        let index = self.file(assembly_path, assembly, reference, o);
        let made = match self.files[index].component {
            Some(component) => self
                .doc
                .0
                .add_occurrence(component, parent, transform)
                .map(|o| (component, o))
                .map_err(|e| e.to_string()),
            None => self.make(index, parent, transform, &name, depth),
        };
        let (component, occurrence) = match made {
            Ok(made) => made,
            Err(e) => {
                self.count("not_placed");
                self.warnings.push(format!("{name}: {e}"));
                return;
            }
        };
        self.files[index].occurrences += 1;
        self.count("placed");
        if self.files[index].found == Found::Missing {
            self.count("of_missing_files");
        }
        if o.grounded() {
            let _ = self.doc.0.set_occurrence_grounded(occurrence, true);
        }
        if o.hidden() {
            self.count("hidden");
            let _ = self.doc.0.set_occurrence_visible(occurrence, false);
        }
        let checks = self.check(index, component, occurrence, o, &name);
        self.list.push(json!({
            "name": name,
            "file": index,
            "occurrence": occurrence.to_string(),
            "grounded": o.grounded(),
            "hidden": o.hidden(),
            "checks": checks,
        }));
    }

    /// Makes the component of file `index` with its first occurrence.
    fn make(
        &mut self,
        index: usize,
        parent: ComponentUid,
        transform: Transform,
        name: &str,
        depth: usize,
    ) -> Result<(ComponentUid, mitcad_model::OccurrenceUid), String> {
        let stem = {
            let f = &self.files[index];
            let saved = f.saved.rsplit(['\\', '/']).next().unwrap_or(&f.saved);
            saved.rsplit_once('.').map_or(saved, |(s, _)| s).to_owned()
        };
        let (path, assembly) = (self.files[index].path.clone(), self.files[index].assembly);
        let made = match (&path, assembly) {
            (Some(path), true) => {
                if self.stack.iter().any(|p| p == path) {
                    self.files[index].status = "a cycle: the assembly contains itself".to_owned();
                    return Err(format!("{name}: the assembly contains itself"));
                }
                let made = self
                    .doc
                    .0
                    .add_component(Some(&stem), parent, transform)
                    .map_err(|e| e.to_string())?;
                self.files[index].component = Some(made.0);
                self.files[index].status = "imported".to_owned();
                self.place(path, made.0, &format!("{name}/"), depth + 1);
                made
            }
            (Some(path), false) => match self.part(path) {
                Ok((part, summary)) => {
                    let made = if self.options.history {
                        self.doc
                            .0
                            .add_component_copy(&part.0, Some(&stem), parent, transform)
                            .map_err(|e| e.to_string())?
                    } else {
                        self.bodies_into(&part, &stem, &summary, parent, transform)?
                    };
                    self.files[index].part = Some(summary);
                    self.files[index].status = "imported".to_owned();
                    self.files[index].component = Some(made.0);
                    made
                }
                Err(e) => {
                    self.files[index].status = format!("failed: {e}");
                    let made = self
                        .doc
                        .0
                        .add_component(Some(&stem), parent, transform)
                        .map_err(|e| e.to_string())?;
                    self.files[index].component = Some(made.0);
                    made
                }
            },
            (None, _) => {
                self.files[index].status = "missing".to_owned();
                let made = self
                    .doc
                    .0
                    .add_component(Some(&stem), parent, transform)
                    .map_err(|e| e.to_string())?;
                self.files[index].component = Some(made.0);
                made
            }
        };
        Ok(made)
    }

    /// A part's stored bodies (imported into `part`) as one base feature of
    /// a new component: the bodies' shapes are shared, not copied through
    /// the part's definition, so that a part of thousands of bodies costs
    /// its bodies once.
    fn bodies_into(
        &mut self,
        part: &Document,
        stem: &str,
        summary: &Value,
        parent: ComponentUid,
        transform: Transform,
    ) -> Result<(ComponentUid, OccurrenceUid), String> {
        let bodies: Vec<mitcad_model::ImportBody<_>> = part
            .0
            .bodies()
            .into_iter()
            .map(|b| mitcad_model::ImportBody {
                name: Some(b.name.clone()),
                color: None,
                shape: b.shape.clone(),
            })
            .collect();
        let visible: Vec<bool> = part
            .0
            .bodies()
            .iter()
            .map(|b| part.0.body_attributes(b.uid).visible)
            .collect();
        let made = self
            .doc
            .0
            .add_component(Some(stem), parent, transform)
            .map_err(|e| e.to_string())?;
        if bodies.is_empty() {
            return Ok(made);
        }
        let added = self
            .doc
            .0
            .add_base_features(
                &format!("Import {stem}"),
                vec![mitcad_model::BaseInput {
                    source: Some(stem.to_owned()),
                    component: Some(made.0),
                    ..mitcad_model::BaseInput::new(bodies)
                }],
            )
            .map_err(|e| e.to_string())?;
        if let Some(id) = summary["material"]
            .as_str()
            .and_then(crate::ipt_import::mitcad_material)
        {
            for body in added.iter().flat_map(|f| &f.bodies) {
                let _ = self.doc.0.set_body_material(body.uid, Some(id));
            }
        }
        // The bodies the part keeps hidden stay hidden.
        for (body, visible) in added.iter().flat_map(|f| &f.bodies).zip(visible) {
            if !visible {
                let _ = self.doc.0.set_body_visible(body.uid, false);
            }
        }
        Ok(made)
    }

    /// Imports a part into a document of its own: the document and the
    /// report's summary.
    fn part(&mut self, path: &Path) -> Result<(Document, Value), String> {
        let o = self.options;
        let options = json!({
            "bodies_only": !o.history, "no_verify": o.no_verify,
            "no_fallback": o.no_fallback, "no_compare": o.no_compare,
            "time_limit": o.time_limit, "hang_limit": o.hang_limit,
        });
        let mut part = Document(mitcad_model::Document::new(OcctKernel));
        let text = part
            .import_ipt(&path.to_string_lossy(), &options.to_string())
            .map_err(|e| e.0)?;
        let report: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let imported = report["imported"].as_array().map_or(&[][..], Vec::as_slice);
        let summary = json!({
            "bodies": imported.len(),
            "solids": imported.iter().filter(|b| b["solid"] == true).count(),
            "valid": imported.iter().filter(|b| b["valid"] == true).count(),
            "not_built": report["skipped"].as_array().map_or(0, Vec::len),
            "history": report["history"] == true,
            "counts": report["counts"],
            "features": report["features"],
            "warnings": report["warnings"].as_array().map_or(0, Vec::len),
            "units": report["units"],
            "material": report["material"],
        });
        Ok((part, summary))
    }

    /// The file an occurrence refers to (found once per path).
    fn file(
        &mut self,
        assembly_path: &Path,
        assembly: &Assembly,
        reference: &Reference,
        o: &Occurrence,
    ) -> usize {
        let (path, found) = self.resolve(assembly_path, assembly, reference, o);
        let path = path.map(|p| normalized(&p));
        let key = match &path {
            Some(p) => p
                .canonicalize()
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy()
                .into_owned(),
            None => format!("missing:{}", reference.path.to_lowercase()),
        };
        if let Some(&i) = self.by_key.get(&key) {
            return i;
        }
        let identity = match (&path, o.document) {
            (Some(p), Some(id)) if !reference.is_assembly() => mitcad_ipt::IptFile::open(p)
                .ok()
                .map(|f| f.lists_document(&id)),
            _ => None,
        };
        self.files.push(File {
            saved: reference.path.clone(),
            path,
            found,
            assembly: reference.is_assembly(),
            component: None,
            status: String::new(),
            identity,
            occurrences: 0,
            part: None,
            bounds: None,
        });
        self.by_key.insert(key, self.files.len() - 1);
        self.files.len() - 1
    }

    /// Finds a referenced file (see the module docs).
    fn resolve(
        &mut self,
        assembly_path: &Path,
        assembly: &Assembly,
        reference: &Reference,
        o: &Occurrence,
    ) -> (Option<PathBuf>, Found) {
        let saved = parts(&reference.path);
        let here = assembly_path.parent().unwrap_or(Path::new("."));
        let exists = |p: &Path| p.is_file();
        if exists(Path::new(&reference.path)) {
            return (Some(PathBuf::from(&reference.path)), Found::AsSaved);
        }
        // Relative to the assembly as saved.
        if let Some(own) = &assembly.saved_path {
            let own = parts(own);
            let dir = &own[..own.len().saturating_sub(1)];
            let common = dir
                .iter()
                .zip(&saved)
                .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
                .count();
            if common > 0 {
                let mut p = here.to_path_buf();
                for _ in common..dir.len() {
                    p.push("..");
                }
                for s in &saved[common..] {
                    p.push(s);
                }
                if exists(&p) {
                    return (Some(p), Found::Relative);
                }
            }
        }
        // The saved path's tail under the assembly's folders.
        for start in 1..saved.len() {
            let tail = &saved[start..];
            for base in here.ancestors().take(8) {
                let p = tail.iter().fold(base.to_path_buf(), |p, s| p.join(s));
                if exists(&p) {
                    return (Some(p), Found::Tail);
                }
            }
        }
        // By name in the search folders.
        let name = reference.file_name().to_lowercase();
        let candidates = self.index().get(&name).cloned().unwrap_or_default();
        let best = match candidates.as_slice() {
            [] => None,
            [one] => Some(one.clone()),
            many => {
                // The one that is the referenced document, else the one
                // whose path ends most like the saved one.
                let identical = o.document.and_then(|id| {
                    many.iter()
                        .find(|p| mitcad_ipt::IptFile::open(p).is_ok_and(|f| f.lists_document(&id)))
                        .cloned()
                });
                identical.or_else(|| {
                    many.iter()
                        .max_by_key(|p| {
                            let theirs: Vec<String> = p
                                .iter()
                                .rev()
                                .map(|c| c.to_string_lossy().to_lowercase())
                                .collect();
                            saved
                                .iter()
                                .rev()
                                .zip(&theirs)
                                .take_while(|(a, b)| a.to_lowercase() == **b)
                                .count()
                        })
                        .cloned()
                })
            }
        };
        match best {
            Some(p) => (Some(p), Found::Name),
            None => (None, Found::Missing),
        }
    }

    /// The search folders' files by lower-case name (built once).
    fn index(&mut self) -> &HashMap<String, Vec<PathBuf>> {
        if self.index.is_none() {
            let mut index: HashMap<String, Vec<PathBuf>> = HashMap::new();
            let mut todo: Vec<PathBuf> = self.roots.clone();
            let mut seen = 0usize;
            let mut visited = std::collections::HashSet::new();
            while let Some(dir) = todo.pop() {
                let Ok(canonical) = dir.canonicalize() else {
                    continue;
                };
                if !visited.insert(canonical) {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    seen += 1;
                    if seen > MAX_INDEXED {
                        break;
                    }
                    let path = entry.path();
                    let name = file_name(&path);
                    if name.starts_with('.') {
                        continue;
                    }
                    match entry.file_type() {
                        Ok(t) if t.is_dir() => todo.push(path),
                        Ok(_) => {
                            let lower = name.to_lowercase();
                            if lower.ends_with(".ipt") || lower.ends_with(".iam") {
                                index.entry(lower).or_default().push(path);
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            for list in index.values_mut() {
                list.sort();
            }
            self.index = Some(index);
        }
        self.index.as_ref().expect("built")
    }

    /// The checks of a placed occurrence against the file: its placement in
    /// the document against the transform the file displays it with, and
    /// for a part the bodies' box against the stored range box (and, as
    /// information only, the placed box centre against the stored one,
    /// which the file does not always keep up to date).
    fn check(
        &mut self,
        index: usize,
        component: ComponentUid,
        occurrence: OccurrenceUid,
        o: &Occurrence,
        name: &str,
    ) -> Value {
        let mut out = json!({});
        let placed = self.doc.0.placement(occurrence);
        if let (Some(displayed), Some(placed)) = (o.displayed, placed) {
            let difference = (0..3)
                .map(|r| {
                    let rotation = (0..3)
                        .map(|c| (placed.linear[r][c] - displayed[r][c]).abs())
                        .fold(0.0, f64::max);
                    // A rotation difference counts as its effect at 1 m.
                    (placed.translation[r] - displayed[r][3] * 10.0)
                        .abs()
                        .max(1000.0 * rotation)
                })
                .fold(0.0, f64::max);
            self.display.add(difference, DISPLAY_TOLERANCE, || {
                format!("{name}: placed {difference:.3} mm from where the file displays it")
            });
            out["display_difference"] = json!(difference);
        }
        if self.files[index].assembly || self.files[index].found == Found::Missing {
            return out;
        }
        if self.files[index].bounds.is_none() {
            let bounds = self.bounds(component);
            self.files[index].bounds = Some(bounds);
        }
        let Some(Some((min, max))) = self.files[index].bounds else {
            out["bounds"] = Value::Null;
            return out;
        };
        if let Some(range) = o.range {
            let stored_min = range[0].map(|v| v * 10.0);
            let stored_max = range[1].map(|v| v * 10.0);
            let diagonal = (0..3)
                .map(|k| (stored_max[k] - stored_min[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            let tolerance = (BOX_TOLERANCE * diagonal).max(MIN_TOLERANCE);
            // The stored range box is not tight (curved faces add to it):
            // the bodies must lie within it; how much larger it is, is
            // information.
            let difference = (0..3)
                .flat_map(|k| [stored_min[k] - min[k], max[k] - stored_max[k]])
                .fold(0.0, f64::max);
            let slack = (0..3)
                .flat_map(|k| [min[k] - stored_min[k], stored_max[k] - max[k]])
                .fold(0.0, f64::max);
            // A part saved after the assembly (its version id is not the
            // one the occurrence stores) may have changed since: its stored
            // range box is information only.
            let checks = if self.files[index].identity == Some(false) {
                &mut self.stale
            } else {
                &mut self.boxes
            };
            checks.add(difference, tolerance, || {
                format!("{name}: the bodies reach {difference:.3} mm out of the stored range box")
            });
            out["box_slack"] = json!(slack);
            out["range"] = json!([stored_min, stored_max]);
            out["range_bodies"] = json!(o.range_bodies);
            out["bodies"] = json!(self.doc.0.component_bodies(component).len());
            out["bounds"] = json!([min, max]);
            out["box_difference"] = json!(difference);
            out["box_tolerance"] = json!(tolerance);
            if let (Some(center), Some(placed)) = (o.center, placed) {
                let stored = center.map(|v| v * 10.0);
                let ours = placed.apply_point(std::array::from_fn(|k| (min[k] + max[k]) / 2.0));
                let difference = (0..3)
                    .map(|k| (ours[k] - stored[k]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                self.centers.add(difference, tolerance, || {
                    format!(
                        "{name}: the placed box centre is {difference:.3} mm from the stored one"
                    )
                });
                out["center_difference"] = json!(difference);
            }
        }
        out
    }

    /// The bounding box of a component's shown bodies (mm, in the
    /// component): the stored range box leaves hidden bodies out.
    fn bounds(&self, component: ComponentUid) -> Option<([f64; 3], [f64; 3])> {
        let doc = &self.doc.0;
        let kernel = doc.kernel();
        let mut out: Option<([f64; 3], [f64; 3])> = None;
        for body in doc.component_bodies(component) {
            if !doc.body_attributes(body).visible {
                continue;
            }
            let Some(shape) = doc.body_shape(body) else {
                continue;
            };
            let Ok(Some(b)) = kernel.bounding_box(shape) else {
                continue;
            };
            out = Some(match out {
                None => (b.min, b.max),
                Some((min, max)) => (
                    std::array::from_fn(|k| min[k].min(b.min[k])),
                    std::array::from_fn(|k| max[k].max(b.max[k])),
                ),
            });
        }
        out
    }
}

impl Document {
    fn import_iam_report(&mut self, path: &str, options: &Options) -> Result<Value, ApiError> {
        let started = std::time::Instant::now();
        let top = PathBuf::from(path);
        let name = file_name(&top);
        let fail = |e: &dyn std::fmt::Display| ApiError(format!("{name}: {e}"));
        let file = mitcad_ipt::IptFile::open(&top).map_err(|e| fail(&e))?;
        if !file.is_assembly() {
            return Err(fail(&"not an assembly file"));
        }
        let info = file.document();
        crate::kernel::exchange::ffi::catch_occt_crashes();
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let label = format!("Import {name}");
        let mut warnings = Vec::new();
        let units = self.apply_part_units(&info, empty, &mut warnings)?;
        let here = top.parent().map(Path::to_path_buf).unwrap_or_default();
        let mut roots = vec![here];
        roots.extend(options.search.iter().map(PathBuf::from));
        let mut import = Import {
            doc: self,
            options,
            roots,
            index: None,
            files: Vec::new(),
            by_key: HashMap::new(),
            stack: Vec::new(),
            warnings,
            counts: BTreeMap::new(),
            boxes: Checks::default(),
            centers: Checks::default(),
            display: Checks::default(),
            stale: Checks::default(),
            list: Vec::new(),
        };
        import.place(&top, ComponentUid::ROOT, "", 0);
        let Import {
            files,
            warnings,
            counts,
            boxes,
            centers,
            display,
            stale,
            list,
            ..
        } = import;
        self.0.merge_undo(depth, &label);

        let mut resolution: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &files {
            *resolution.entry(f.found.text()).or_default() += 1;
        }
        let parts =
            |pred: &dyn Fn(&File) -> bool| files.iter().filter(|f| !f.assembly && pred(f)).count();
        let file_values: Vec<Value> = files
            .iter()
            .map(|f| {
                json!({
                    "saved": f.saved,
                    "path": f.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                    "found": f.found.text(),
                    "kind": if f.assembly { "assembly" } else { "part" },
                    "status": f.status,
                    "identity": f.identity,
                    "occurrences": f.occurrences,
                    "part": f.part,
                    "component": f.component.map(|c| c.to_string()),
                })
            })
            .collect();
        let report = json!({
            "file": name,
            "format": "iam",
            "release": info.release,
            "units": units,
            "unit_code": info.length_unit_code,
            "occurrences": counts,
            "parts": {
                "files": parts(&|_| true),
                "imported": parts(&|f| f.status == "imported"),
                "missing": parts(&|f| f.found == Found::Missing),
                "failed": parts(&|f| f.status.starts_with("failed")),
                "identity_confirmed": parts(&|f| f.identity == Some(true)),
                "identity_differs": parts(&|f| f.identity == Some(false)),
            },
            "assemblies": files.iter().filter(|f| f.assembly).count(),
            "resolution": resolution,
            "files": file_values,
            "boxes": boxes.value(),
            "centers": centers.value(),
            "display": display.value(),
            "stale_boxes": stale.value(),
            "pass": boxes.agree == boxes.compared && display.agree == display.compared,
            "occurrence_list": list,
            "warnings": warnings,
            "seconds": started.elapsed().as_secs_f64(),
        });
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(report)
    }

    /// The bridge's `import_ipt` with an assembly, `import_iam`: the report
    /// as JSON, or text.
    pub(crate) fn import_iam(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.without_job(|document| document.import_iam_report(path, &options))?;
        Ok(if options.text {
            summary(&report)
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_iam", "path": ..., ...}` (also `import_ipt` with an
    /// assembly).
    pub(crate) fn import_iam_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_iam needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.without_job(|document| document.import_iam_report(&path, &options))?;
        Ok(json!({
            "file": report["file"],
            "occurrences": report["occurrences"]["placed"],
            "report": report,
        }))
    }
}

/// The import report as text.
fn summary(report: &Value) -> String {
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let o = &report["occurrences"];
    let p = &report["parts"];
    let mut text = format!(
        "Imported assembly {}: {} occurrences placed ({} suppressed, {} hidden, {} of missing files, {} not placed); \
         {} part files ({} imported, {} missing, {} failed; {} identities confirmed, {} differ), {} sub-assemblies; units: {}\n",
        report["file"].as_str().unwrap_or_default(),
        n(&o["placed"]),
        n(&o["suppressed"]),
        n(&o["hidden"]),
        n(&o["of_missing_files"]),
        n(&o["not_placed"]) + n(&o["without_file"]) + n(&o["without_placement"]),
        n(&p["files"]),
        n(&p["imported"]),
        n(&p["missing"]),
        n(&p["failed"]),
        n(&p["identity_confirmed"]),
        n(&p["identity_differs"]),
        n(&report["assemblies"]),
        report["units"].as_str().unwrap_or("mm"),
    );
    let resolution: Vec<String> = report["resolution"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| format!("{v} {k}"))
        .collect();
    text += &format!("  files found: {}\n", resolution.join(", "));
    for (key, what) in [
        ("display", "placements agree with the displayed ones"),
        ("boxes", "parts' bodies lie within their stored range boxes"),
    ] {
        let c = &report[key];
        text += &format!(
            "  check: {} of {} {} (largest difference {:.3} mm)\n",
            n(&c["agree"]),
            n(&c["compared"]),
            what,
            c["worst"].as_f64().unwrap_or_default()
        );
        for f in c["failures"].as_array().into_iter().flatten() {
            text += &format!("  check failed: {}\n", f.as_str().unwrap_or_default());
        }
    }
    // Information only: the range boxes of parts saved after the assembly,
    // and the stored centres, which the file does not always keep up to
    // date.
    for (key, what) in [
        (
            "stale_boxes",
            "parts saved after the assembly lie within their stored range boxes",
        ),
        ("centers", "stored box centres agree"),
    ] {
        let c = &report[key];
        text += &format!(
            "  {} of {} {} (largest difference {:.3} mm)\n",
            n(&c["agree"]),
            n(&c["compared"]),
            what,
            c["worst"].as_f64().unwrap_or_default()
        );
    }
    for f in report["files"].as_array().into_iter().flatten() {
        let part = &f["part"];
        let detail = if part.is_object() {
            let counts: Vec<String> = part["counts"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, v)| format!("{v} {k}"))
                .collect();
            format!(
                ": {} bodies ({} valid, {} not built){}",
                n(&part["bodies"]),
                n(&part["valid"]),
                n(&part["not_built"]),
                if counts.is_empty() {
                    String::new()
                } else {
                    format!("; features {}", counts.join(", "))
                }
            )
        } else {
            String::new()
        };
        text += &format!(
            "  {} {} ({}, {} occurrences{}): {}{}\n",
            f["kind"].as_str().unwrap_or_default(),
            f["path"]
                .as_str()
                .unwrap_or(f["saved"].as_str().unwrap_or_default()),
            f["found"].as_str().unwrap_or_default(),
            n(&f["occurrences"]),
            match f["identity"].as_bool() {
                Some(true) => ", identity confirmed",
                Some(false) => ", identity differs",
                None => "",
            },
            f["status"].as_str().unwrap_or_default(),
            detail
        );
    }
    for w in report["warnings"].as_array().into_iter().flatten() {
        text += &format!("  warning: {}\n", w.as_str().unwrap_or_default());
    }
    text
}
