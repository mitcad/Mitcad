// SPDX-License-Identifier: MIT
//! The bodies of a 3MF or STL file on the print bed (mitcad#731,
//! `ExportPlacement::on_bed`): the 3D print export turns them so that a
//! body's print bed face (`document/print_bed.rs`) lies on the bed, its
//! outward normal straight down, puts their lowest point on the bed and
//! centres them on the plate. The design does not move: the export moves
//! the meshes' placements after the design's (a 3MF part's components, an
//! STL file's vertices), and the meshes are the kernel's.
//!
//! The bodies written are one piece, as the 3MF file's one object: they
//! turn together onto the one bed face among them, also when the body is
//! placed several times alike (a component placed again, moved only: the
//! face applies to every occurrence). Bed faces of several bodies, or of
//! placements turned differently, keep the bodies as they are assembled,
//! and so do none. Bodies that are not printed (surface bodies) are left
//! out. A bed face that is gone or no longer planar is reported, and its
//! body is written as the design has it (−Z down) unless another body's
//! face turns the piece.
//!
//! A 3MF file of chosen occurrences (mitcad#856, `ExportPlacement::
//! occurrences`) has objects of its own: two or more placements picked of
//! bodies with usable bed faces are each an object turned onto its face
//! (a nut placed many times, each turned differently), the bodies of an
//! occurrence sent whole (`ExportPlacement::whole`) one object as they are
//! assembled, and the rest the file's own object; each lies on the bed by
//! the rule above, and those turned are set side by side along X, the row
//! centred on the plate (overlaps with objects kept as designed are left
//! to the slicer's arrange). Between two neighbours lies the spacing
//! ([`GAP`] unless the export gives one) plus how far each one's first
//! layer reaches past it in the slicer, its brim or a skirt of its own by
//! the settings it prints with ([`first_layer_reach`], mitcad#1491). A row
//! wider than the bed wraps into rows along Y ([`wrap_rows`],
//! mitcad#1316), the rows centred on the plate too, as far apart; when
//! they are still wider or deeper than the bed, the export warns.
//!
//! A piece kept as assembled because several bed faces apply to it,
//! counted per placement, lists them as its choices (mitcad#857,
//! [`BedChoice`]); a face chosen (`ExportPlacement::bed_faces`: a body and
//! the occurrence of its placement) turns the piece it is in onto it,
//! whatever the rule above says, as one bed face would.
//!
//! A piece without a usable bed face (none, or only faces that are gone)
//! is turned onto the face the automatic orientation finds (mitcad#1109,
//! `orient.rs`, [`BedRules::auto_orient`]), which looks at the piece as a
//! whole; the face is not stored, so a body's own face always wins.
//! Without a planar face the piece can lie on, it stays as the design has
//! it (−Z down). Every piece, turned or not and kept as assembled or not,
//! then has its lowest point dropped onto the bed (z = 0) and the middle
//! of its box centred over the plate's centre, each when the rules ask
//! for it ([`Settle`], both by default).

use std::collections::BTreeMap;

use crate::datum::Vec3;
use crate::document::{Document, ModelError, PrintTarget};
use crate::exchange::{FaceMeshes, Refinement, RefinementPreset};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_keys;
use crate::topo::FaceName;
use crate::transform::Transform;

use super::Exported;
use super::orient;

/// What happens to a piece on the bed after its turn (mitcad#1109): its
/// box's middle over the plate's centre (`center`), its lowest point on
/// the bed, z = 0 (`drop`). Both by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settle {
    pub center: bool,
    pub drop: bool,
}

impl Default for Settle {
    fn default() -> Self {
        Self {
            center: true,
            drop: true,
        }
    }
}

impl Settle {
    /// Whether it moves the piece at all.
    pub fn any(self) -> bool {
        self.center || self.drop
    }
}

/// How the pieces go on the bed: the print bed faces chosen (mitcad#857),
/// the automatic orientation of a piece without a usable face
/// (mitcad#1109) and the settling after the turn.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BedRules<'a> {
    pub chosen: &'a [(BodyUid, Vec<OccurrenceUid>)],
    pub auto_orient: bool,
    pub settle: Settle,
    /// The clearance between two neighbouring pieces' first layers, mm
    /// (mitcad#1491; [`GAP`] by default).
    pub spacing: f64,
}

/// The plate when the design's printer gives no bed shape, mm: the Bambu
/// family's 256 x 256 mm, the slicer the export writes for by default.
pub const DEFAULT_PLATE: [f64; 2] = [256.0, 256.0];

/// How the export put the bodies on the print bed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OnBed {
    /// The body whose print bed face lies on the bed, and the face (as it
    /// was assigned); None when the bodies are written as they are.
    pub face: Option<(BodyUid, String)>,
    /// What moved the bodies after their placements in the design: the
    /// turn of the face down, then the move of the lowest point onto the
    /// bed (z = 0) and of the middle of the box over the plate's centre.
    pub transform: Option<Transform>,
    /// The plate's centre (x, y), mm: of the design's printer's bed shape,
    /// else of [`DEFAULT_PLATE`].
    pub plate: [f64; 2],
    /// Bodies with bed faces, kept as they are assembled (several faces).
    pub kept: Vec<BodyUid>,
    /// Bed faces that cannot be used, and why.
    pub problems: Vec<(BodyUid, String)>,
    /// 3MF (mitcad#856): the file's other objects, each put on the bed by
    /// itself (the fields above are the file's own object's, the bodies
    /// not put apart; no face and no transform when there are none).
    pub objects: Vec<BedObject>,
    /// How many placements of bodies lie on their print bed faces, in
    /// every object.
    pub on_faces: usize,
    /// The bed faces the piece could be turned onto, one per placement,
    /// when several keep it as assembled (mitcad#857); empty otherwise.
    pub choices: Vec<BedChoice>,
    /// Whether `face` is the automatic orientation's (mitcad#1109), not a
    /// print bed face of the body.
    pub auto: bool,
    /// The occurrence path of the placement whose face lies on the bed
    /// (empty for the design itself or a body not placed); None without a
    /// face.
    pub occurrence: Option<Vec<OccurrenceUid>>,
    /// The settling after the turn (mitcad#1109).
    pub settle: Settle,
}

/// An object of a 3MF file put on the bed by itself (mitcad#856): a
/// placement of a body with a print bed face, or the bodies of an
/// occurrence sent whole.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BedObject {
    /// "Nut (Nut:3)", or the occurrence's path name.
    pub name: String,
    /// As in [`OnBed`]: the body and face on the bed, what moved the
    /// object after its placements, the bodies kept as assembled, the bed
    /// faces to choose from, whether the face is the automatic
    /// orientation's and the occurrence of its placement.
    pub face: Option<(BodyUid, String)>,
    pub transform: Option<Transform>,
    pub kept: Vec<BodyUid>,
    pub choices: Vec<BedChoice>,
    pub auto: bool,
    pub occurrence: Option<Vec<OccurrenceUid>>,
}

/// A print bed face a piece kept as assembled could lie on (mitcad#857):
/// the body, the occurrence path of the placement (None for a body that is
/// not placed) and the face as it was assigned.
#[derive(Debug, Clone, PartialEq)]
pub struct BedChoice {
    pub body: BodyUid,
    pub path: Option<Vec<OccurrenceUid>>,
    pub face: String,
}

/// A bed face where a placement puts it: the body, the face as assigned,
/// its outward normal turned by the placement, and the placement's path.
type Down = (BodyUid, String, Vec3, Option<Vec<OccurrenceUid>>);

/// Whether a chosen bed face (a body and the path of its placement, empty
/// for the design itself or a body not placed) is that of this placement.
pub(super) fn is_chosen(
    chosen: &(BodyUid, Vec<OccurrenceUid>),
    body: BodyUid,
    path: Option<&[OccurrenceUid]>,
) -> bool {
    chosen.0 == body && path.unwrap_or(&[]) == chosen.1.as_slice()
}

/// An object of a 3MF file on the print bed (mitcad#856): its placements
/// of the bodies written, and how it lies.
#[derive(Debug, Clone)]
pub(crate) struct Piece {
    /// None for the file's own object (the bodies not put apart).
    pub name: Option<String>,
    /// (index of the body among those written, index of its placement).
    pub members: Vec<(usize, usize)>,
    pub plan: OnBed,
    /// Whether it is a further copy of the piece before it (a print
    /// plate's item with a count, mitcad#1311): without centring, set
    /// beside that one along X, as far apart as neighbours are.
    pub copy: bool,
    /// How far its first layer reaches past it in the slicer, mm: the
    /// furthest of its placements' [`first_layer_reach`] (mitcad#1491).
    pub margin: f64,
    /// The clearance between its first layer and a neighbour's, mm
    /// ([`BedRules::spacing`]).
    pub spacing: f64,
}

/// The clearance between the objects set side by side on the bed, mm,
/// when the export gives none (mitcad#1491: beyond their brims).
pub(super) const GAP: f64 = 5.0;

/// How far (mm) a row may pass the bed's size and still fit: the baseline
/// tolerance of geometric comparisons.
pub(super) const FIT: f64 = 1e-6;

/// A nozzle's width when the printer gives none, mm: a skirt loop's line.
const LINE: f64 = 0.4;

/// How far (mm) the slicer's first layer reaches past a piece's outline by
/// the settings `effective` it prints with (mitcad#1491): its brim
/// (`brim_width` and the brim's gap from the piece) unless `brim_type` is
/// `no_brim` or `inner_only` (`auto_brim` counts, as the slicer adds it to
/// tall, narrow pieces), or a skirt of its own (`skirt_per_object`: its
/// distance and loops, a nozzle's width each), whichever reaches further;
/// 0 without either.
pub(crate) fn first_layer_reach(effective: &crate::document::EffectivePrint) -> f64 {
    let number = |key: &str| match effective.value(key) {
        Some(print_keys::Value::Number(n)) if n.is_finite() && n > 0.0 => n,
        Some(print_keys::Value::Integer(i)) if i > 0 => i as f64,
        _ => 0.0,
    };
    let brim = match effective.value("brim_type") {
        Some(print_keys::Value::Text(kind)) if kind == "no_brim" || kind == "inner_only" => 0.0,
        _ if number("brim_width") > 0.0 => number("brim_width") + number("brim_separation"),
        _ => 0.0,
    };
    let skirt = match effective.value("skirt_per_object") {
        Some(print_keys::Value::Bool(true)) if number("skirts") > 0.0 => {
            let nozzle = number("nozzle_diameter");
            let line = if nozzle > 0.0 { nozzle } else { LINE };
            number("skirt_distance") + number("skirts") * line
        }
        _ => 0.0,
    };
    brim.max(skirt)
}

/// The pieces (`spans`: each one's x and y range, its first layer's reach
/// included) in rows, in order: a piece goes on the row before while the
/// row, with the gaps of `gap`, stays within `width` (mitcad#1316); a
/// piece wider than the bed has a row of its own. Indices into `spans`;
/// at least one row.
pub(super) fn wrap_rows(spans: &[[(f64, f64); 2]], width: f64, gap: f64) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    for (k, [(lo, hi), _]) in spans.iter().enumerate() {
        let size = hi - lo;
        let row = rows.last_mut().expect("a row");
        if row.is_empty() {
            row.push(k);
            used = size;
        } else if used + gap + size <= width + FIT {
            row.push(k);
            used += gap + size;
        } else {
            rows.push(vec![k]);
            used = size;
        }
    }
    rows
}

/// The turn that takes the unit vector `down` straight down (−Z): about
/// the axis square to both, by the angle between them; half a turn about X
/// for straight up.
pub(super) fn turn_down(down: Vec3) -> Transform {
    let target = [0.0, 0.0, -1.0];
    let cos = mitcad_vec3::dot(down, target).clamp(-1.0, 1.0);
    let axis = mitcad_vec3::cross(down, target);
    match Transform::rotation([0.0; 3], axis, cos.acos()) {
        Some(turn) => turn,
        // Along Z already: down, or up.
        None if cos > 0.0 => Transform::IDENTITY,
        None => Transform {
            linear: [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
            translation: [0.0; 3],
        },
    }
}

/// The turn a writer settles a piece with (`transform`, the plan's), or
/// none when it neither turns nor settles: the plan's turn, else no turn
/// when `settle` still moves the piece (mitcad#1109).
pub(super) fn turn_to_settle(transform: Option<Transform>, settle: Settle) -> Option<Transform> {
    transform.or_else(|| settle.any().then_some(Transform::IDENTITY))
}

/// `turn`, then the move that puts the lowest of the turned points on the
/// bed (z = 0) when `settle.drop` and the middle of their box over `plate`
/// when `settle.center`.
pub(super) fn settled(
    turn: &Transform,
    points: impl Iterator<Item = [f64; 3]>,
    plate: [f64; 2],
    settle: Settle,
) -> Transform {
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for point in points {
        let p = turn.apply_point(point);
        min = std::array::from_fn(|i| min[i].min(p[i]));
        max = std::array::from_fn(|i| max[i].max(p[i]));
    }
    if !min.iter().chain(&max).all(|v| v.is_finite()) {
        return *turn;
    }
    let shift = [
        if settle.center {
            plate[0] - (min[0] + max[0]) / 2.0
        } else {
            0.0
        },
        if settle.center {
            plate[1] - (min[1] + max[1]) / 2.0
        } else {
            0.0
        },
        if settle.drop { -min[2] } else { 0.0 },
    ];
    Transform::translation(shift).after(turn)
}

/// The meshing of the bodies for the automatic orientation: coarse, as
/// planar faces are exact at any refinement and the overhangs of curved
/// ones need no more.
fn orient_tolerance() -> crate::exchange::MeshTolerance {
    Refinement::Preset(RefinementPreset::Low).tolerance()
}

/// The face the automatic orientation lays a piece on: the body, the
/// face's name, its outward normal where the placement puts it, and the
/// placement's path.
type AutoFace = (BodyUid, String, Vec3, Option<Vec<OccurrenceUid>>);

impl<K: Kernel> Document<K> {
    /// The centre of the design's printer's plate (the box of its bed
    /// shape), else of [`DEFAULT_PLATE`].
    pub(crate) fn plate_centre(&self) -> [f64; 2] {
        let effective = self.effective_print(&PrintTarget::Component(ComponentUid::ROOT));
        match effective.value("bed_shape") {
            Some(print_keys::Value::Points(bed)) if !bed.is_empty() => {
                let (min, max) = print_keys::bounds(&bed);
                [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0]
            }
            _ => [DEFAULT_PLATE[0] / 2.0, DEFAULT_PLATE[1] / 2.0],
        }
    }

    /// How far the first layer of a piece of these placements (each body
    /// and its occurrence path, None for a body not placed) reaches past
    /// it, mm (mitcad#1491): the furthest [`first_layer_reach`] of the
    /// printed ones, by their settings on `plate` (none: each body's own
    /// plate's).
    pub(crate) fn piece_margin(
        &self,
        placements: impl Iterator<Item = (BodyUid, Option<Vec<OccurrenceUid>>)>,
        plate: Option<&crate::print_plates::PrintPlate>,
    ) -> f64 {
        placements
            .filter(|(uid, _)| self.printed_body(*uid))
            .map(|(uid, path)| {
                let target = PrintTarget::Body(uid, path);
                let effective = match plate {
                    Some(on) => self.effective_print_on(&target, Some(on)),
                    None => self.effective_print(&target),
                };
                first_layer_reach(&effective)
            })
            .fold(0.0, f64::max)
    }

    /// Where the export puts the bodies on the bed: their print bed faces
    /// where `placements` place them (`paths`: the occurrence path of each
    /// placement, None for a body not placed), and the face `chosen` among
    /// them (mitcad#857) or else the one body's face that every placement
    /// turns alike (its turn in `transform`, which the writer completes),
    /// or why they stay as they are, with the faces to choose from; with
    /// no usable face, the automatic orientation's face when the rules
    /// ask for it (mitcad#1109). Two faces chosen among them are refused.
    pub(super) fn bed_plan(
        &self,
        bodies: &[BodyUid],
        placements: &dyn Fn(BodyUid) -> Vec<Transform>,
        paths: &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
        rules: &BedRules<'_>,
    ) -> Result<OnBed, ModelError> {
        let chosen = rules.chosen;
        let mut plan = OnBed {
            plate: self.plate_centre(),
            settle: rules.settle,
            ..OnBed::default()
        };
        // Each bed face where a placement puts it, by its outward normal,
        // with the placement's path; bodies that are not printed (surface
        // bodies) are left out.
        let mut down: Vec<Down> = Vec::new();
        for &uid in bodies {
            if !self.printed_body(uid) {
                continue;
            }
            let Some(bed) = self.print_bed_face(uid) else {
                continue;
            };
            match bed.plane {
                Ok(plane) => {
                    let normal = plane.normal();
                    let at = paths(uid);
                    for (k, placement) in placements(uid).into_iter().enumerate() {
                        let turned = placement.apply_vector(normal);
                        if let Some(unit) = mitcad_vec3::unit(turned) {
                            let path = at.get(k).cloned().flatten();
                            down.push((uid, bed.face.clone(), unit, path));
                        }
                    }
                }
                Err(why) => plan.problems.push((uid, why)),
            }
        }
        // A face chosen (mitcad#857): the piece turns onto it.
        let picked: Vec<&(BodyUid, Vec<OccurrenceUid>)> = chosen
            .iter()
            .filter(|c| {
                down.iter()
                    .any(|(uid, _, _, path)| is_chosen(c, *uid, path.as_deref()))
            })
            .collect();
        if picked.len() > 1 {
            let names: Vec<String> = picked
                .iter()
                .map(|(uid, path)| {
                    let at = self.assembly().path_name(path);
                    if at.is_empty() {
                        self.body_name(*uid)
                    } else {
                        format!("{} in {at}", self.body_name(*uid))
                    }
                })
                .collect();
            return Err(super::invalid(format!(
                "the print bed faces of {} are chosen for one piece; choose one",
                names.join(" and ")
            )));
        }
        let first = match picked.first() {
            Some(c) => down
                .iter()
                .find(|(uid, _, _, path)| is_chosen(c, *uid, path.as_deref())),
            None => down.first(),
        };
        // One face, or one body's face where every placement turns it alike
        // (a component placed again, moved only): the piece turns onto it.
        let Some((uid, face, normal, path)) = first else {
            // None usable: the automatic orientation's face (mitcad#1109),
            // else as the design has it.
            if rules.auto_orient
                && let Some((uid, face, normal, path)) = self.auto_face(bodies, placements, paths)
            {
                plan.face = Some((uid, face));
                plan.transform = Some(turn_down(normal));
                plan.auto = true;
                plan.occurrence = Some(path.unwrap_or_default());
            }
            return Ok(plan);
        };
        let alike =
            |(other, _, n, _): &Down| other == uid && mitcad_vec3::dot(*n, *normal) > 1.0 - 1e-9;
        if !picked.is_empty() || down.iter().all(alike) {
            plan.face = Some((*uid, face.clone()));
            plan.transform = Some(turn_down(*normal));
            plan.on_faces = down.iter().filter(|d| alike(d)).count();
            plan.occurrence = Some(path.clone().unwrap_or_default());
        } else {
            for (uid, face, _, path) in &down {
                if !plan.kept.contains(uid) {
                    plan.kept.push(*uid);
                }
                plan.choices.push(BedChoice {
                    body: *uid,
                    path: path.clone(),
                    face: face.clone(),
                });
            }
        }
        Ok(plan)
    }

    /// The face the automatic orientation (mitcad#1109, `orient.rs`) lays
    /// the piece of `bodies` at `placements` on, looking at it as a whole:
    /// the printed bodies' faces where each placement puts them, a named
    /// planar face the candidate; None when no planar face supports the
    /// piece (or the kernel cannot list or mesh the faces).
    fn auto_face(
        &self,
        bodies: &[BodyUid],
        placements: &dyn Fn(BodyUid) -> Vec<Transform>,
        paths: &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
    ) -> Option<AutoFace> {
        let tolerance = orient_tolerance();
        let mut piece = FaceMeshes::default();
        let mut planar: Vec<bool> = Vec::new();
        // Of each face of the piece: the body, its name and the path.
        let mut owners: Vec<(BodyUid, Option<String>, Option<Vec<OccurrenceUid>>)> = Vec::new();
        for &uid in bodies {
            if !self.printed_body(uid) {
                continue;
            }
            let Some(shape) = self.body_shape(uid) else {
                continue;
            };
            let (Ok(faces), Ok(mesh)) = (
                self.kernel().faces(shape),
                self.kernel().face_meshes(shape, &tolerance),
            ) else {
                continue;
            };
            // A face's first name that is a face name, for a planar face.
            let listed = faces.len() == mesh.face_triangles.len();
            let names: Vec<Option<String>> = mesh
                .face_triangles
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let face = faces.get(i).filter(|f| listed && f.surface == "plane")?;
                    face.names
                        .iter()
                        .find(|n| n.parse::<FaceName>().is_ok())
                        .cloned()
                })
                .collect();
            let at = paths(uid);
            for (k, placement) in placements(uid).into_iter().enumerate() {
                let first = piece.vertices.len() as u32;
                piece
                    .vertices
                    .extend(mesh.vertices.iter().map(|v| placement.apply_point(*v)));
                piece.normals.extend(
                    mesh.normals
                        .iter()
                        .map(|n| mitcad_vec3::unit(placement.apply_vector(*n)).unwrap_or(*n)),
                );
                // A mirroring placement would turn the triangles inside out.
                let mirrored = placement.determinant() < 0.0;
                piece.triangles.extend(mesh.triangles.iter().map(|t| {
                    let [a, b, c] = t.map(|i| first + i);
                    if mirrored { [a, c, b] } else { [a, b, c] }
                }));
                piece.face_triangles.extend(&mesh.face_triangles);
                let path = at.get(k).cloned().flatten();
                for name in &names {
                    planar.push(name.is_some());
                    owners.push((uid, name.clone(), path.clone()));
                }
            }
        }
        let best = orient::best_bed_face(&piece, &planar)?;
        let (uid, name, path) = owners.get(best.face)?.clone();
        Some((uid, name?, best.normal, path))
    }

    /// The objects of a 3MF file on the print bed (mitcad#856), each with
    /// its plan ([`Document::bed_plan`] of its placements): the
    /// placements of `bodies` (`paths`, the occurrence paths of
    /// `placements`, None for a body not placed) in an occurrence of
    /// `whole` (the outermost) one object each, as assembled; each
    /// placement `picked` (bodies listed with their occurrences) of a body
    /// with a usable bed face an object of its own when there are two or
    /// more; the rest the file's own object (first, when it has any). A
    /// face the rules choose turns the piece it is in (mitcad#857); a piece
    /// without a usable face is oriented automatically when they ask for
    /// it (mitcad#1109).
    #[allow(clippy::type_complexity)]
    pub(super) fn bed_pieces(
        &self,
        bodies: &[BodyUid],
        placements: &dyn Fn(BodyUid) -> Vec<Transform>,
        paths: &dyn Fn(BodyUid) -> Vec<Option<Vec<OccurrenceUid>>>,
        picked: &BTreeMap<BodyUid, Vec<Vec<OccurrenceUid>>>,
        whole: &[Vec<OccurrenceUid>],
        rules: &BedRules<'_>,
    ) -> Result<Vec<Piece>, ModelError> {
        let mut rest = Vec::new();
        let mut wholes: Vec<Vec<(usize, usize)>> = vec![Vec::new(); whole.len()];
        let mut apart: Vec<(usize, usize, Vec<OccurrenceUid>)> = Vec::new();
        for (b, &uid) in bodies.iter().enumerate() {
            let usable = picked.contains_key(&uid)
                && self.printed_body(uid)
                && self.print_bed_face(uid).is_some_and(|f| f.plane.is_ok());
            for (k, path) in paths(uid).into_iter().enumerate() {
                let Some(path) = path else {
                    rest.push((b, k));
                    continue;
                };
                let outer = whole
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| path.starts_with(w.as_slice()))
                    .min_by_key(|(_, w)| w.len());
                if let Some((w, _)) = outer {
                    wholes[w].push((b, k));
                } else if usable {
                    apart.push((b, k, path));
                } else {
                    rest.push((b, k));
                }
            }
        }
        // One placement on its face goes with the rest, as before: the
        // other bodies turn with it as one piece.
        if apart.len() < 2 {
            rest.extend(apart.drain(..).map(|(b, k, _)| (b, k)));
        }
        let mut pieces = Vec::new();
        if !rest.is_empty() {
            pieces.push((None, rest));
        }
        for (w, members) in wholes.into_iter().enumerate() {
            if !members.is_empty() {
                pieces.push((Some(self.assembly().path_name(&whole[w])), members));
            }
        }
        for (b, k, path) in apart {
            let body = self.body_name(bodies[b]);
            let name = match self.assembly().path_name(&path) {
                occurrence if occurrence.is_empty() => body,
                occurrence => format!("{body} ({occurrence})"),
            };
            pieces.push((Some(name), vec![(b, k)]));
        }
        pieces
            .into_iter()
            .map(|(name, members)| {
                let mine: Vec<BodyUid> = bodies
                    .iter()
                    .enumerate()
                    .filter(|(b, _)| members.iter().any(|(m, _)| m == b))
                    .map(|(_, uid)| *uid)
                    .collect();
                let of_piece = |uid: BodyUid| -> Vec<Transform> {
                    let all = placements(uid);
                    members
                        .iter()
                        .filter(|(b, _)| bodies[*b] == uid)
                        .filter_map(|(_, k)| all.get(*k).copied())
                        .collect()
                };
                let paths_of_piece = |uid: BodyUid| -> Vec<Option<Vec<OccurrenceUid>>> {
                    let all = paths(uid);
                    members
                        .iter()
                        .filter(|(b, _)| bodies[*b] == uid)
                        .map(|(_, k)| all.get(*k).cloned().flatten())
                        .collect()
                };
                let plan = self.bed_plan(&mine, &of_piece, &paths_of_piece, rules)?;
                let margin = self.piece_margin(
                    mine.iter()
                        .flat_map(|&uid| paths_of_piece(uid).into_iter().map(move |p| (uid, p))),
                    None,
                );
                Ok(Piece {
                    name,
                    members,
                    plan,
                    copy: false,
                    margin,
                    spacing: rules.spacing,
                })
            })
            .collect()
    }

    /// The account of a 3MF file's objects on the bed (mitcad#856): the
    /// file's own object's plan (the one piece's when there is one) with
    /// the others as [`OnBed::objects`],
    /// every problem once and the placements on their faces counted.
    pub(super) fn merged_bed(&self, pieces: Vec<Piece>, settle: Settle) -> OnBed {
        let mut merged = OnBed {
            plate: self.plate_centre(),
            settle,
            ..OnBed::default()
        };
        // One piece is the file's own object, whatever it holds.
        let single = pieces.len() == 1;
        for piece in pieces {
            let plan = piece.plan;
            merged.on_faces += plan.on_faces;
            for problem in plan.problems {
                if !merged.problems.contains(&problem) {
                    merged.problems.push(problem);
                }
            }
            match piece.name.filter(|_| !single) {
                None => {
                    merged.face = plan.face;
                    merged.transform = plan.transform;
                    merged.kept = plan.kept;
                    merged.choices = plan.choices;
                    merged.auto = plan.auto;
                    merged.occurrence = plan.occurrence;
                }
                Some(name) => merged.objects.push(BedObject {
                    name,
                    face: plan.face,
                    transform: plan.transform,
                    kept: plan.kept,
                    choices: plan.choices,
                    auto: plan.auto,
                    occurrence: plan.occurrence,
                }),
            }
        }
        merged
    }

    /// The export's account of the bed: the plan as written, and a warning
    /// per bed face that cannot be used.
    pub(super) fn report_bed(&self, exported: &mut Exported, on_bed: Option<OnBed>) {
        let Some(on_bed) = on_bed else {
            return;
        };
        for (uid, why) in &on_bed.problems {
            let name = self.body_name(*uid);
            let warning = if on_bed.face.is_some() {
                format!("{name}: {why}")
            } else {
                format!("{name}: {why}; printed as in the design")
            };
            exported.warnings.push(warning);
        }
        exported.on_bed = Some(on_bed);
    }
}
