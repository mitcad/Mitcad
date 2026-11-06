// SPDX-License-Identifier: MIT
//! The design's print plates in 3MF files (mitcad#1050,
//! `ExportPlacement::plates`, [`crate::print_plates`]).
//!
//! **What a plate prints.** Each item of the plate, `count` times: a body
//! once per copy (at its first shown placement, turned onto the bed as a
//! piece of its own), an occurrence's printed bodies as assembled (its
//! shown placements, all of them when none is shown) once per copy, one
//! piece each copy. The first plate also prints every placement on no
//! plate (`Document::unplaced`), each a piece of its own. An item whose
//! body or occurrence is gone is left out with a warning. Each placement
//! prints on one plate, its body's (`Document::plate_of_body`,
//! mitcad#1310): an occurrence item leaves out the bodies under it that
//! have an item of their own, or lie under an outer occurrence item, on
//! another plate (with a warning when that leaves it nothing).
//!
//! **Layout.** Within each plate by the export's arranger (`bed.rs`): each
//! piece on its print bed face, else the automatic orientation's, dropped
//! onto the bed, the pieces side by side along X and the row centred on
//! the plate, as Center on Plate and Drop to Bed say; a row wider than the
//! bed wraps into rows along Y, and pieces that still do not fit are named
//! in the plate's warnings (mitcad#1316). Without centring the pieces keep
//! their places, but the further copies of an item each lie beside the one
//! before it along X, `bed::GAP` apart (`Piece::copy`, mitcad#1311), not on
//! top of the first.
//!
//! **Settings.** Every body on a plate takes that plate's values in the
//! settings order (`document/print_settings.rs`, `effective_print_on`).
//!
//! **Files** ([`PlateFiles`]):
//!
//! - one file of every plate, for the slicers that keep several plates in
//!   a project: the Bambu family, each plate where its slicer lays plates
//!   out ([`crate::print_plates::bambu_plate_origin`]) with a `<plate>`
//!   block of its objects and its own plate settings in
//!   `model_settings.config` (`print_files/bambu.rs`); PrusaSlicer 2.9 and
//!   later, each bed where it lays beds out
//!   ([`crate::print_plates::prusa_bed_origin`]), which it reads the beds
//!   from (its project's settings are the beds' own: what differs between
//!   the plates is per object where PrusaSlicer has such a key, else
//!   left out with a warning). Cura has one build plate: refused;
//! - a file per plate (Cura, PrusaSlicer before 2.9): `<name> - <plate
//!   name>.3mf` beside the path given, each plate as a project of its own;
//!   an empty plate writes no file;
//! - one plate, in the path given.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::bed::{self, BedRules, Piece};
use super::{Built3mf, ExportOptions, ExportPlacement, Exported, OnBed, ThreeMfPlate, invalid};
use crate::document::{BodyView, Document, InstanceView, ModelError, PrintTarget};
use crate::exchange::{DEFAULT_PLATE, ExportFormat, LengthUnit, Refinement, RefinementPreset};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_keys::{self, Dialect};
use crate::print_plates::{PlateTarget, PrintPlate, bambu_plate_origin, prusa_bed_origin};
use crate::transform::Transform;

/// How the export writes the design's print plates (mitcad#1050).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateFiles {
    /// Every plate in one file (the Bambu family, PrusaSlicer 2.9 and
    /// later).
    OneFile,
    /// A file per plate beside the path (Cura, PrusaSlicer before 2.9).
    FilePerPlate,
    /// The plate of this index alone, in the path.
    Only(usize),
}

/// A print plate as the export wrote it (mitcad#1050).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportedPlate {
    pub id: String,
    pub name: String,
    /// The file it is in; none for an empty plate of a file per plate.
    pub path: Option<String>,
    /// The bodies it prints, each once, in order.
    pub bodies: Vec<BodyUid>,
    /// Its pieces: the objects a slicer arranges.
    pub pieces: usize,
    /// Where its bed lies in a file of several plates, mm (the first at
    /// the origin).
    pub origin: [f64; 2],
    /// How its bodies were put on the bed.
    pub on_bed: Option<OnBed>,
    /// What the plate leaves out (an item that is gone).
    pub warnings: Vec<String>,
}

/// A placement of a body on a plate: its transform and its occurrence path
/// (None for a body not placed).
type Placed = (Transform, Option<Vec<OccurrenceUid>>);

/// What a plate prints: its bodies in order, each body's placements (the
/// transform and the occurrence path, None for a body not placed) and its
/// pieces, each a name, its members (index of the body, index of its
/// placement) and whether it is a further copy of the piece before it (an
/// item with a count, mitcad#1311).
/// One piece of a plate: its name, its (body, placement) pairs and whether
/// it is a further copy of the piece before it.
type PlatePiece = (String, Vec<(usize, usize)>, bool);

#[derive(Default)]
struct PlateContent {
    bodies: Vec<BodyUid>,
    placed: Vec<Vec<Placed>>,
    pieces: Vec<PlatePiece>,
    warnings: Vec<String>,
}

impl PlateContent {
    /// A body's index, added when new.
    fn body(&mut self, uid: BodyUid) -> usize {
        match self.bodies.iter().position(|b| *b == uid) {
            Some(b) => b,
            None => {
                self.bodies.push(uid);
                self.placed.push(Vec::new());
                self.bodies.len() - 1
            }
        }
    }

    /// Adds a placement of a body: (body index, placement index).
    fn place(&mut self, uid: BodyUid, at: Placed) -> (usize, usize) {
        let b = self.body(uid);
        self.placed[b].push(at);
        (b, self.placed[b].len() - 1)
    }

    fn transforms(&self, uid: BodyUid) -> Vec<Transform> {
        self.bodies
            .iter()
            .position(|b| *b == uid)
            .map(|b| self.placed[b].iter().map(|(t, _)| *t).collect())
            .unwrap_or_default()
    }

    fn paths(&self, uid: BodyUid) -> Vec<Option<Vec<OccurrenceUid>>> {
        self.bodies
            .iter()
            .position(|b| *b == uid)
            .map(|b| self.placed[b].iter().map(|(_, p)| p.clone()).collect())
            .unwrap_or_default()
    }
}

/// A file name of a plate: characters a file name cannot have on every
/// system become `_`.
fn file_part(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_matches('.').trim().to_owned();
    if safe.is_empty() {
        "Plate".to_owned()
    } else {
        safe
    }
}

/// The file of a plate beside `path`: `<name> - <plate name>.3mf`.
fn plate_path(path: &str, plate: &str) -> String {
    let given = Path::new(path);
    let stem = given
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("MitCad");
    let name = format!("{stem} - {}.3mf", file_part(plate));
    given.parent().map_or_else(
        || name.clone(),
        |dir| dir.join(&name).to_string_lossy().into_owned(),
    )
}

/// [`plate_path`], made unique among the files `taken` (compared ignoring
/// case, as file systems may): ` (2)`, ` (3)`, … after the plate's name when
/// another plate's name gives the same file (`A/B` and `A:B`, mitcad#1316).
/// The path is added to `taken`.
fn unique_plate_path(path: &str, plate: &str, taken: &mut Vec<String>) -> String {
    let base = file_part(plate);
    let mut file = plate_path(path, &base);
    let mut n = 2;
    while taken
        .iter()
        .any(|t| t.to_lowercase() == file.to_lowercase())
    {
        file = plate_path(path, &format!("{base} ({n})"));
        n += 1;
    }
    taken.push(file.clone());
    file
}

/// The placements as the design shows them: the shown ones, all of them
/// when none is shown.
fn shown_first(found: Vec<&InstanceView>) -> Vec<&InstanceView> {
    let shown: Vec<&InstanceView> = found.iter().copied().filter(|i| i.visible).collect();
    if shown.is_empty() { found } else { shown }
}

/// The model name of a file: its stem.
fn stem_of(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("MitCad")
        .to_owned()
}

impl<K: Kernel> Document<K> {
    /// The size of the design's printer's plate (the box of its bed
    /// shape), else [`DEFAULT_PLATE`], mm.
    pub(crate) fn plate_size(&self) -> [f64; 2] {
        let effective = self.effective_print(&PrintTarget::Component(ComponentUid::ROOT));
        match effective.value("bed_shape") {
            Some(print_keys::Value::Points(bed)) if !bed.is_empty() => {
                let (min, max) = print_keys::bounds(&bed);
                [max[0] - min[0], max[1] - min[1]]
            }
            _ => DEFAULT_PLATE,
        }
    }

    /// The pieces plate `index` prints, as its export makes them
    /// (`print_plates` query).
    pub(crate) fn plate_piece_count(&self, index: usize, instances: &[InstanceView]) -> usize {
        self.plate_content(index, instances).pieces.len()
    }

    /// A plate's name for warnings, by its index.
    fn plate_label(&self, index: usize) -> String {
        self.state()
            .print_plates
            .get(index)
            .map_or_else(|| format!("plate {}", index + 1), |p| p.name.clone())
    }

    /// What plate `index` prints (see the module's documentation).
    fn plate_content(&self, index: usize, instances: &[InstanceView]) -> PlateContent {
        let plate = &self.state().print_plates[index];
        let mut content = PlateContent::default();
        for item in &plate.items {
            if !self.plate_target_exists(&item.target) {
                content.warnings.push(format!(
                    "{}: {} is not in the design at the timeline marker; left out",
                    plate.name,
                    item.target.text()
                ));
                continue;
            }
            let name = self.plate_target_name(&item.target);
            let copy_name = |n: u32| {
                if item.count > 1 {
                    format!("{name} ({})", n + 1)
                } else {
                    name.clone()
                }
            };
            match &item.target {
                PlateTarget::Body(uid) => {
                    if !self.printed_body(*uid) {
                        content.warnings.push(format!(
                            "{}: {name} is not printed (a surface or empty body); left out",
                            plate.name
                        ));
                        continue;
                    }
                    // A body prints on one plate (`plate_of_body`): an item
                    // of it on another plate as well (a design file written
                    // so) is left out here.
                    if let Some(other) = self.plate_of_body(*uid, None)
                        && other != index
                    {
                        content.warnings.push(format!(
                            "{}: {name} prints on {}; left out",
                            plate.name,
                            self.plate_label(other)
                        ));
                        continue;
                    }
                    let first = shown_first(instances.iter().filter(|i| i.body == *uid).collect())
                        .first()
                        .map(|i| (i.transform, Some(i.path.clone())))
                        .unwrap_or((Transform::IDENTITY, None));
                    for n in 0..item.count {
                        let member = content.place(*uid, first.clone());
                        content.pieces.push((copy_name(n), vec![member], n > 0));
                    }
                }
                PlateTarget::Occurrence(path) => {
                    let placed = shown_first(
                        instances
                            .iter()
                            .filter(|i| i.path.starts_with(path) && self.printed_body(i.body))
                            .collect(),
                    );
                    // Each placement prints on one plate (mitcad#1310): a
                    // body with an item of its own, or under an outer
                    // occurrence item, prints on that item's plate.
                    let elsewhere: Vec<usize> = placed
                        .iter()
                        .filter_map(|i| self.plate_of_body(i.body, Some(&i.path)))
                        .filter(|p| *p != index)
                        .collect();
                    let inside: Vec<&InstanceView> = placed
                        .iter()
                        .copied()
                        .filter(|i| self.plate_of_body(i.body, Some(&i.path)) == Some(index))
                        .collect();
                    if inside.is_empty() {
                        let why = match elsewhere.first() {
                            Some(other) => {
                                format!("'s bodies print on {}", self.plate_label(*other))
                            }
                            None => " places no printed bodies".to_owned(),
                        };
                        content
                            .warnings
                            .push(format!("{}: {name}{why}; left out", plate.name));
                        continue;
                    }
                    for n in 0..item.count {
                        let members: Vec<(usize, usize)> = inside
                            .iter()
                            .map(|i| content.place(i.body, (i.transform, Some(i.path.clone()))))
                            .collect();
                        content.pieces.push((copy_name(n), members, n > 0));
                    }
                }
            }
        }
        // The first plate: what no plate names, each placement a piece.
        if index == 0 {
            for (uid, path) in self.unplaced() {
                let transform = path
                    .as_ref()
                    .and_then(|p| instances.iter().find(|i| i.body == uid && i.path == *p))
                    .map_or(Transform::IDENTITY, |i| i.transform);
                let body = self.body_name(uid);
                let name = match path.as_deref().map(|p| self.assembly().path_name(p)) {
                    Some(at) if !at.is_empty() => format!("{body} ({at})"),
                    _ => body,
                };
                let member = content.place(uid, (transform, path));
                content.pieces.push((name, vec![member], false));
            }
        }
        content
    }

    /// The pieces of a plate's content, each with its plan on the bed
    /// (`bed.rs`, [`Document::bed_plan`]).
    fn plate_pieces(
        &self,
        content: &PlateContent,
        rules: &BedRules<'_>,
    ) -> Result<Vec<Piece>, ModelError> {
        content
            .pieces
            .iter()
            .map(|(name, members, copy)| {
                let mine: Vec<BodyUid> = content
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| members.iter().any(|(m, _)| m == b))
                    .map(|(_, uid)| *uid)
                    .collect();
                let of_piece = |uid: BodyUid| -> Vec<Transform> {
                    members
                        .iter()
                        .filter(|(b, _)| content.bodies[*b] == uid)
                        .map(|(b, k)| content.placed[*b][*k].0)
                        .collect()
                };
                let paths_of_piece = |uid: BodyUid| -> Vec<Option<Vec<OccurrenceUid>>> {
                    members
                        .iter()
                        .filter(|(b, _)| content.bodies[*b] == uid)
                        .map(|(b, k)| content.placed[*b][*k].1.clone())
                        .collect()
                };
                let plan = self.bed_plan(&mine, &of_piece, &paths_of_piece, rules)?;
                Ok(Piece {
                    name: Some(name.clone()),
                    members: members.clone(),
                    plan,
                    copy: *copy,
                })
            })
            .collect()
    }

    /// The 3MF model of one plate's content (no slicer's settings yet),
    /// moved by `shift`, with how its pieces lie.
    #[allow(clippy::too_many_arguments)]
    fn build_plate(
        &self,
        model_name: String,
        plate: &PrintPlate,
        content: &PlateContent,
        tolerance: &crate::exchange::MeshTolerance,
        unit: LengthUnit,
        colors: &BTreeMap<BodyUid, [f64; 3]>,
        dialect: Option<Dialect>,
        rules: &BedRules<'_>,
        shift: [f64; 2],
    ) -> Result<(Built3mf, OnBed), ModelError> {
        let all = self.bodies();
        let chosen: Vec<&BodyView<'_, K::Shape>> = content
            .bodies
            .iter()
            .filter_map(|uid| all.iter().find(|b| b.uid == *uid))
            .collect();
        if chosen.len() != content.bodies.len() {
            return Err(invalid(format!(
                "{}: a body on it is not at the timeline marker",
                plate.name
            )));
        }
        let mut pieces = self.plate_pieces(content, rules)?;
        let color = |uid: BodyUid| {
            colors
                .get(&uid)
                .copied()
                .or_else(|| self.imported_color(uid))
        };
        let paths = |uid: BodyUid| content.paths(uid);
        let mut built = self.build_3mf(
            model_name,
            &chosen,
            true,
            tolerance,
            unit,
            color,
            |uid| content.transforms(uid),
            dialect.map(|d| {
                (
                    d,
                    &paths as &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
                )
            }),
            Some(&mut pieces),
            ThreeMfPlate {
                plate: Some(plate),
                shift,
            },
        )?;
        // What the layout says (pieces that do not fit, mitcad#1316) is
        // the plate's.
        for warning in &mut built.exported.warnings {
            *warning = format!("{}: {warning}", plate.name);
        }
        let on_bed = self.merged_bed(pieces, rules.settle);
        Ok((built, on_bed))
    }

    /// Writes the design's print plates (mitcad#1050; see the module's
    /// documentation). `bodies` must be empty: the plates name what is
    /// printed.
    pub(super) fn export_plates(
        &self,
        path: &str,
        bodies: &[BodyUid],
        options: &ExportOptions,
        colors: &BTreeMap<BodyUid, [f64; 3]>,
        placement: &ExportPlacement,
        files: PlateFiles,
    ) -> Result<Exported, ModelError> {
        if options.format != ExportFormat::ThreeMf {
            return Err(invalid("the print plates go into 3MF files"));
        }
        let plates = &self.state().print_plates;
        if plates.is_empty() {
            return Err(invalid("the design has no print plates"));
        }
        if !bodies.is_empty()
            || placement.within.is_some()
            || !placement.occurrences.is_empty()
            || !placement.whole.is_empty()
            || !placement.bed_faces.is_empty()
        {
            return Err(invalid(
                "the print plates name what is printed: no bodies, occurrence, occurrences, \
                 whole or bed_face with them",
            ));
        }
        if placement.plan_only {
            return Err(invalid(
                "the print plates have no plan; plate_layout shows a plate",
            ));
        }
        if placement.coordinates == Some(super::Coordinates::Component) {
            return Err(invalid(
                "the print plates are put on the bed from the design's coordinates",
            ));
        }
        if let PlateFiles::Only(index) = files
            && index >= plates.len()
        {
            return Err(invalid(format!("there is no plate {}", index + 1)));
        }
        let dialect = options.dialect;
        if files == PlateFiles::OneFile && dialect == Some(Dialect::Cura) {
            return Err(invalid(
                "Cura has one build plate: write a file per plate (plates: file_per_plate)",
            ));
        }
        let tolerance = options.refinement.tolerance();
        if !(tolerance.deviation > 0.0 && tolerance.angle > 0.0) {
            return Err(invalid(
                "the mesh deviation and angle must be greater than zero",
            ));
        }
        let rules = BedRules {
            chosen: &[],
            auto_orient: placement.auto_orient,
            settle: placement.settle,
        };
        let instances = self.instances();
        let contents: Vec<PlateContent> = (0..plates.len())
            .map(|i| self.plate_content(i, &instances))
            .collect();
        let mut exported = Exported::default();
        match files {
            PlateFiles::OneFile => {
                let size = self.plate_size();
                let origin = |i: usize| match dialect {
                    Some(Dialect::PrusaSlicer) => prusa_bed_origin(i, size),
                    _ => bambu_plate_origin(i, plates.len(), size),
                };
                let model_name = stem_of(path);
                let mut model = mitcad_3mf::Model {
                    name: model_name.clone(),
                    unit: super::three_mf(options.unit),
                    application: format!("MitCad {}", env!("CARGO_PKG_VERSION")),
                    ..mitcad_3mf::Model::default()
                };
                let mut settings = Vec::new();
                // Each plate's objects (indices into the model's).
                let mut on_plates: Vec<Vec<usize>> = Vec::new();
                for (i, (plate, content)) in plates.iter().zip(&contents).enumerate() {
                    let mut written = ExportedPlate {
                        id: plate.id.clone(),
                        name: plate.name.clone(),
                        path: Some(path.to_owned()),
                        origin: origin(i),
                        warnings: content.warnings.clone(),
                        ..ExportedPlate::default()
                    };
                    let mut objects_here = Vec::new();
                    if !content.pieces.is_empty() {
                        let (built, on_bed) = self.build_plate(
                            model_name.clone(),
                            plate,
                            content,
                            &tolerance,
                            options.unit,
                            colors,
                            dialect,
                            &rules,
                            origin(i),
                        )?;
                        let offset = model.parts.len();
                        let mut objects = built.model.objects;
                        if objects.is_empty() {
                            // One piece: the object of every placement.
                            let every = built
                                .model
                                .parts
                                .iter()
                                .enumerate()
                                .flat_map(|(p, part)| {
                                    (0..part.placements.len()).map(move |k| (p, k))
                                })
                                .collect();
                            objects.push(mitcad_3mf::BuildObject {
                                name: content
                                    .pieces
                                    .first()
                                    .map_or_else(|| plate.name.clone(), |(n, _, _)| n.clone()),
                                components: every,
                            });
                        }
                        for mut object in objects {
                            for component in &mut object.components {
                                component.0 += offset;
                            }
                            objects_here.push(model.objects.len());
                            model.objects.push(object);
                        }
                        model.parts.extend(built.model.parts);
                        settings.extend(built.settings);
                        written.bodies = built.exported.bodies.clone();
                        written.pieces = content.pieces.len();
                        written.on_bed = Some(on_bed);
                        written
                            .warnings
                            .extend(built.exported.warnings.iter().cloned());
                        merge(&mut exported, built.exported);
                    }
                    on_plates.push(objects_here);
                    exported.warnings.extend(content.warnings.iter().cloned());
                    exported.plates.push(written);
                }
                if model.parts.is_empty() {
                    return Err(invalid("the plates print no solid or mesh bodies"));
                }
                // One object is the model's own (as `write` writes it
                // without objects).
                if model.objects.len() < 2 {
                    model.objects.clear();
                }
                let export_plates: Vec<crate::print_files::ExportPlate<'_>> = plates
                    .iter()
                    .zip(&on_plates)
                    .map(|(plate, objects)| crate::print_files::ExportPlate {
                        name: &plate.name,
                        objects: if model.objects.is_empty() {
                            objects.iter().map(|_| 0).collect()
                        } else {
                            objects.clone()
                        },
                        values: &plate.values,
                    })
                    .collect();
                let plates_written = std::mem::take(&mut exported.plates);
                let built = Built3mf {
                    model,
                    settings,
                    exported,
                };
                let mut out = self.write_3mf(path, built, dialect, &export_plates)?;
                out.plates = plates_written;
                Ok(out)
            }
            PlateFiles::FilePerPlate | PlateFiles::Only(_) => {
                let indices: Vec<usize> = match files {
                    PlateFiles::Only(index) => vec![index],
                    _ => (0..plates.len()).collect(),
                };
                // The file names written, to keep each plate's its own.
                let mut taken: Vec<String> = Vec::new();
                for i in indices {
                    let (plate, content) = (&plates[i], &contents[i]);
                    let mut written = ExportedPlate {
                        id: plate.id.clone(),
                        name: plate.name.clone(),
                        warnings: content.warnings.clone(),
                        ..ExportedPlate::default()
                    };
                    exported.warnings.extend(content.warnings.iter().cloned());
                    if content.pieces.is_empty() {
                        if matches!(files, PlateFiles::Only(_)) {
                            return Err(invalid(format!("{} prints nothing", plate.name)));
                        }
                        exported.plates.push(written);
                        continue;
                    }
                    let file = match files {
                        PlateFiles::Only(_) => path.to_owned(),
                        _ => unique_plate_path(path, &plate.name, &mut taken),
                    };
                    let (built, on_bed) = self.build_plate(
                        stem_of(&file),
                        plate,
                        content,
                        &tolerance,
                        options.unit,
                        colors,
                        dialect,
                        &rules,
                        [0.0, 0.0],
                    )?;
                    let objects = built.model.objects.len();
                    written
                        .warnings
                        .extend(built.exported.warnings.iter().cloned());
                    // The plate's own block, for the slicers that keep
                    // plate settings (the Bambu family).
                    let one = [crate::print_files::ExportPlate {
                        name: &plate.name,
                        objects: (0..objects.max(1)).collect(),
                        values: &plate.values,
                    }];
                    let out = self.write_3mf(&file, built, dialect, &one)?;
                    written.path = Some(file);
                    written.bodies = out.bodies.clone();
                    written.pieces = content.pieces.len();
                    written.on_bed = Some(on_bed);
                    merge(&mut exported, out);
                    exported.plates.push(written);
                }
                if exported.plates.iter().all(|p| p.path.is_none()) {
                    return Err(invalid("the plates print nothing"));
                }
                Ok(exported)
            }
        }
    }

    /// The `plate_layout` query (mitcad#1050): how a plate's pieces lie on
    /// it, drawn beside the design: the bed's outline and every body's
    /// placements there (rows of `[a b c t]`, from the body's component's
    /// coordinates), with the move from the plate's coordinates (the bed
    /// shape's) into the view's. Meshed coarsely: the turns and the row
    /// do not need more.
    pub fn plate_layout_json(&self, index: usize) -> Result<Value, ModelError> {
        let plates = &self.state().print_plates;
        let plate = plates
            .get(index)
            .ok_or_else(|| invalid(format!("there is no plate {}", index + 1)))?;
        let effective = self.effective_print(&PrintTarget::Component(ComponentUid::ROOT));
        let bed: Vec<[f64; 2]> = match effective.value("bed_shape") {
            Some(print_keys::Value::Points(bed)) if bed.len() >= 3 => bed,
            _ => vec![
                [0.0, 0.0],
                [DEFAULT_PLATE[0], 0.0],
                DEFAULT_PLATE,
                [0.0, DEFAULT_PLATE[1]],
            ],
        };
        let (bed_min, bed_max) = print_keys::bounds(&bed);
        // Beside the design: the bed's left edge a fifth of its width
        // right of the shown bodies, its middle level with theirs.
        let instances = self.instances();
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for instance in instances.iter().filter(|i| i.visible) {
            let Some(shape) = self.body_shape(instance.body) else {
                continue;
            };
            let Ok(Some(bbox)) = self.kernel().bounding_box(shape) else {
                continue;
            };
            for corner in bbox.corners() {
                let p = instance.transform.apply_point(corner);
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
        }
        let width = bed_max[0] - bed_min[0];
        let shift = if lo[0].is_finite() {
            [
                hi[0] + width / 5.0 - bed_min[0],
                (lo[1] + hi[1]) / 2.0 - (bed_min[1] + bed_max[1]) / 2.0,
            ]
        } else {
            [
                -(bed_min[0] + bed_max[0]) / 2.0,
                -(bed_min[1] + bed_max[1]) / 2.0,
            ]
        };
        let content = self.plate_content(index, &instances);
        let mut warnings = content.warnings.clone();
        let rules = BedRules {
            chosen: &[],
            auto_orient: true,
            settle: bed::Settle::default(),
        };
        let mut bodies = Vec::new();
        let mut pieces = 0;
        if !content.pieces.is_empty() {
            let tolerance = Refinement::Preset(RefinementPreset::Low).tolerance();
            let (built, _) = self.build_plate(
                String::new(),
                plate,
                &content,
                &tolerance,
                LengthUnit::Millimeter,
                &BTreeMap::new(),
                None,
                &rules,
                shift,
            )?;
            pieces = content.pieces.len();
            warnings.extend(built.exported.warnings.iter().cloned());
            for ((uid, name), part) in built.exported.parts.iter().zip(&built.model.parts) {
                let rows: Vec<Value> = part
                    .placements
                    .iter()
                    .map(|p| {
                        json!(
                            (0..3)
                                .map(|r| {
                                    [
                                        p.linear[r][0],
                                        p.linear[r][1],
                                        p.linear[r][2],
                                        p.translation[r],
                                    ]
                                })
                                .collect::<Vec<_>>()
                        )
                    })
                    .collect();
                bodies.push(json!({"body": uid, "name": name, "transforms": rows}));
            }
        }
        let outline: Vec<[f64; 2]> = bed
            .iter()
            .map(|p| [p[0] + shift[0], p[1] + shift[1]])
            .collect();
        Ok(json!({
            "plate": plate.id,
            "name": plate.name,
            "index": index,
            "bed": outline,
            "shift": shift,
            "pieces": pieces,
            "bodies": bodies,
            "warnings": warnings,
        }))
    }
}

/// Adds what a plate's export wrote to the whole export's account.
fn merge(into: &mut Exported, from: Exported) {
    into.bodies.extend(from.bodies);
    into.skipped.extend(from.skipped);
    into.meshes.extend(from.meshes);
    into.warnings.extend(from.warnings);
    into.slots.extend(from.slots);
    into.printed.extend(from.printed);
    into.parts.extend(from.parts);
}
