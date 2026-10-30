// SPDX-License-Identifier: MIT
//! Cut sheets (mitcad#853; commands.md, "Cut sheets"): drawings of their
//! own kind kept with the design's drawings, which collect flat parts at
//! 1:1 and write their outlines as DXF and SVG for cutting. The commands
//! add a cut sheet, change its material, add, move, turn and delete its
//! parts and write it; the `cut_sheet` query gives the parts with their
//! loops and the sheet's warnings, and the `drawing` query draws a cut
//! sheet for the application (`drawings.rs` sends cut sheets here, as do
//! `edit_drawing_view` and `delete_drawing_view` for the parts, and
//! `export_drawing`).
//!
//! A part is worked out whenever it is asked for: its body where its
//! occurrence places it in the drawing's component, the face it is seen
//! along (its largest planar face unless one is chosen), the kernel's
//! projection along the face's normal with hidden lines removed
//! (`Kernel::project_view`) and the body's silhouette there
//! (`mitcad_model_drawing::cut_sheet::silhouette`, the material behind
//! each region of the projection asked of `Kernel::segments_crossings`).
//! The result is kept in the document's drawing cache by the body's shape
//! version, its placement and the face chosen.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::annotations::present;
use super::dispatch::named_enum;
use super::drawings::{Shown, groups_json};
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::ComponentUid;
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::Kernel;
use mitcad_model::topo::FaceName;
use mitcad_model_build::prelude::*;
use mitcad_model_drawing::bspline::Spline2;
use mitcad_model_drawing::cut_sheet::{self, OutlineLoop, PartView, Piece, PlacedPart};
use mitcad_model_drawing::cut_sheet_def::{
    self as def, CutPart, CutSheet, DEFAULT_HEIGHT, DEFAULT_WIDTH, MAX_COUNT,
};
use mitcad_model_drawing::def::{Drawing, DrawingSheet, DrawingUid, Subject, SubjectBody};
use mitcad_model_drawing::{P2, ViewSpec, section_axes};
use mitcad_vec3::{dot, unit_div as unit};

fn yes() -> bool {
    true
}

/// The room left between parts placed automatically, mm.
const GAP: f64 = 5.0;
/// The side of the box a part that cannot be worked out keeps, mm.
const BROKEN_BOX: f64 = 30.0;

/// A body a part is made of: its uid or name, or with the occurrence that
/// places it (a path from the drawing's component).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum PartBody {
    Named(String),
    Placed {
        body: String,
        #[serde(default)]
        occurrence: Option<OccurrencePath>,
    },
}

/// `add_cut_sheet`: a cut sheet in a component's Drawings folder (the
/// root's when left out), its material's size and the parts it starts
/// with.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddCutSheetCommand {
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    bodies: Vec<PartBody>,
    #[serde(default)]
    count: Option<u32>,
}

/// `edit_cut_sheet`: its name and its material's size.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditCutSheetCommand {
    drawing: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
}

/// `add_cut_sheet_parts`: bodies as parts, each `count` times.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AddPartsCommand {
    drawing: String,
    bodies: Vec<PartBody>,
    /// The planar face they are seen along (one body only).
    #[serde(default)]
    face: Option<FaceName>,
    #[serde(default)]
    count: Option<u32>,
    /// Where the first one's middle goes on the sheet; a free place when
    /// left out.
    #[serde(default)]
    at: Option<P2>,
}

/// `edit_cut_sheet_part`: what is given changes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditPartCommand {
    drawing: String,
    part: u32,
    #[serde(default)]
    place: Option<P2>,
    #[serde(default, rename = "move")]
    shift: Option<P2>,
    /// Degrees counter-clockwise.
    #[serde(default)]
    rotation: Option<f64>,
    /// Turned by this many degrees more (90: a quarter turn left).
    #[serde(default)]
    turn: Option<f64>,
    /// A planar face of its body; null for the largest.
    #[serde(default, deserialize_with = "present")]
    face: Option<Option<FaceName>>,
}

/// `export_cut_sheet`: the file and its format (from its extension when
/// left out), with the parts' names on the layer `LABEL` or not.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExportCutSheetCommand {
    drawing: String,
    path: String,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    labels: bool,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum CutSheetCommand {
        "add_cut_sheet" => AddCutSheet(Box<AddCutSheetCommand>),
        "edit_cut_sheet" => EditCutSheet(Box<EditCutSheetCommand>),
        "add_cut_sheet_parts" => AddParts(Box<AddPartsCommand>),
        "edit_cut_sheet_part" => EditPart(Box<EditPartCommand>),
        "delete_cut_sheet_parts" => DeleteParts {
            drawing: String,
            parts: Vec<u32>,
        },
        "export_cut_sheet" => Export(Box<ExportCutSheetCommand>),
    }
}

/// The `cut_sheet` query: a cut sheet's parts, with their loops unless
/// `geometry` is false.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CutSheetQuery {
    drawing: String,
    #[serde(default = "yes")]
    geometry: bool,
}

/// A part moved, turned or seen along another face (`edit_cut_sheet_part`,
/// and `edit_drawing_view` on a cut sheet).
#[derive(Debug, Default)]
pub(super) struct PartEdit {
    pub(super) place: Option<P2>,
    pub(super) shift: Option<P2>,
    pub(super) rotation: Option<f64>,
    pub(super) turn: Option<f64>,
    pub(super) face: Option<Option<FaceName>>,
}

/// A part laid out on its sheet.
struct PartState {
    part: CutPart,
    /// Its body's name (its uid when the body is gone).
    name: String,
    view: Result<Arc<PartView>, String>,
    /// On the sheet; empty when it is not worked out.
    loops: Vec<OutlineLoop>,
    /// Its body (or the occurrence placing it) is gone.
    missing: bool,
    off_sheet: bool,
    overlaps: Vec<u32>,
}

/// A cut sheet's parts laid out, with what is wrong with them.
struct Layout {
    parts: Vec<PartState>,
    warnings: Vec<String>,
}

/// The sheet box of a part, or a box about its place when it is not
/// worked out.
fn part_box(state: &PartState) -> (P2, P2) {
    cut_sheet::bounds(&state.loops).unwrap_or_else(|| {
        let p = state.part.place;
        let h = BROKEN_BOX / 2.0;
        ([p[0] - h, p[1] - h], [p[0] + h, p[1] + h])
    })
}

fn piece_json(piece: &Piece) -> Value {
    match piece {
        Piece::Line(a, b) => json!({"line": [a, b]}),
        Piece::Arc {
            center,
            radius,
            start,
            sweep,
        } => json!({"arc": {"center": center, "radius": radius, "start": start, "sweep": sweep}}),
        Piece::Polyline(points) => json!({"polyline": points}),
        Piece::Spline { .. } => piece.exact().map_or(Value::Null, |s| spline_json(&s)),
    }
}

/// A B-spline as the queries give it (mitcad#853).
fn spline_json(s: &Spline2) -> Value {
    json!({"spline": {"degree": s.degree, "knots": s.knots, "poles": s.poles,
                      "weights": if s.is_rational() { s.weights.clone() } else { Vec::new() }}})
}

fn loops_json(loops: &[OutlineLoop]) -> Value {
    json!(
        loops
            .iter()
            .map(|l| json!({
                "outer": l.outer,
                "pieces": l.pieces.iter().map(piece_json).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    )
}

/// The name a new cut sheet takes in a component's Drawings folder.
fn next_name(drawings: &[Drawing], component: ComponentUid) -> String {
    (1..)
        .map(|n| format!("CutSheet{n}"))
        .find(|name| {
            !drawings
                .iter()
                .any(|d| d.component == component && d.name == *name)
        })
        .expect("a free name")
}

/// The planar face to lie a body on: the largest, the one facing up most
/// (+Z, then +Y, then +X) among those as large.
fn largest(faces: &[(FaceName, f64, Vec3)]) -> Option<(FaceName, Vec3)> {
    let higher = |a: Vec3, b: Vec3| {
        for k in [2, 1, 0] {
            if (a[k] - b[k]).abs() > 1e-9 {
                return a[k] > b[k];
            }
        }
        false
    };
    let mut best: Option<&(FaceName, f64, Vec3)> = None;
    for face in faces {
        best = Some(match best {
            None => face,
            Some(b) => {
                let tie = (face.1 - b.1).abs() <= 1e-6 * b.1.abs().max(1.0);
                if (face.1 > b.1 && !tie) || (tie && higher(face.2, b.2)) {
                    face
                } else {
                    b
                }
            }
        });
    }
    best.map(|(face, _, normal)| (face.clone(), *normal))
}

/// A free place on a sheet for a box of `size` among the boxes there, in
/// rows from the top left; beside the sheet when there is none.
fn free_place(width: f64, height: f64, taken: &[(P2, P2)], size: P2) -> P2 {
    let step = GAP.max(width.max(height) / 200.0);
    let half = [size[0] / 2.0, size[1] / 2.0];
    let free = |c: P2| {
        taken.iter().all(|(lo, hi)| {
            c[0] + half[0] + GAP <= lo[0]
                || c[0] - half[0] - GAP >= hi[0]
                || c[1] + half[1] + GAP <= lo[1]
                || c[1] - half[1] - GAP >= hi[1]
        })
    };
    let mut y = height - GAP - half[1];
    while y >= GAP + half[1] {
        let mut x = GAP + half[0];
        while x <= width - GAP - half[0] {
            if free([x, y]) {
                return [x, y];
            }
            x += step;
        }
        y -= step;
    }
    let right = taken.iter().map(|(_, hi)| hi[0]).fold(width, f64::max);
    [right + GAP + half[0], height - half[1]]
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait CutSheetsOps<K: Kernel> {
    fn run_cut_sheet_command(&mut self, command: CutSheetCommand) -> Result<Value, ApiError>;

    /// `cut_sheet`: the parts with their loops and the sheet's warnings.
    fn cut_sheet_json(&self, query: &CutSheetQuery) -> Result<Value, ApiError>;

    /// The `drawing` query of a cut sheet: its sheet with its parts as
    /// views (`type` `part`) and their outlines as groups.
    fn cut_sheet_drawing_json(&self, drawing: &Drawing, geometry: bool) -> Value;

    /// A part moved, turned or seen along another face.
    fn edit_cut_part(
        &mut self,
        drawing: &str,
        part: u32,
        edit: PartEdit,
    ) -> Result<Value, ApiError>;

    fn delete_cut_parts(&mut self, drawing: &str, parts: &[u32]) -> Result<Value, ApiError>;

    /// Writes a cut sheet as DXF or SVG (`format`, else the path's
    /// extension); `check` refuses a path.
    fn export_cut_sheet_checked(
        &self,
        drawing: &str,
        path: &str,
        format: Option<&str>,
        labels: bool,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError>;
}

/// The document's methods used only in this module.
trait CutSheetsLocal<K: Kernel> {
    /// A part worked out: its body where it is placed, the face it lies
    /// on and its outline.
    fn part_view(&self, component: ComponentUid, part: &CutPart) -> Result<Arc<PartView>, String>;

    /// A cut sheet's parts laid out with their warnings.
    fn lay_out(&self, drawing: &Drawing) -> Layout;

    /// New parts of bodies, `count` of each, placed at `at` (the first)
    /// or in free places.
    fn new_parts(
        &self,
        drawing: &Drawing,
        bodies: &[PartBody],
        face: Option<&FaceName>,
        count: Option<u32>,
        at: Option<P2>,
    ) -> Result<Vec<CutPart>, ApiError>;

    /// A cut sheet by its id or name, and its own part.
    fn find_cut_sheet(&self, text: &str) -> Result<(Drawing, CutSheet), ApiError>;
}

impl<K: Kernel> CutSheetsLocal<K> for Document<K> {
    fn part_view(&self, component: ComponentUid, part: &CutPart) -> Result<Arc<PartView>, String> {
        let subject = Subject::Bodies {
            bodies: vec![SubjectBody {
                body: part.body,
                occurrence: part.occurrence.clone(),
            }],
        };
        let first: Shown = self
            .subject_bodies(component, &subject)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("the body {} is empty", part.body))?;
        let shape = self
            .body_shape(part.body)
            .ok_or_else(|| format!("the body {} is gone", part.body))?;
        let version = self.body_version(part.body).unwrap_or(0);
        let mut fp = Fingerprint::new("cut-sheet-part-v1");
        fp.str(&part.body.to_string()).u128(version);
        for row in &first.transform.linear {
            for v in row {
                fp.f64(*v);
            }
        }
        for v in &first.transform.translation {
            fp.f64(*v);
        }
        fp.str(
            &part
                .face
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
        );
        let key = fp.finish();
        if let Some(view) = self.drawing_cache().part_view(key) {
            return Ok(view);
        }

        // The body's planar faces, their normals where the body is placed.
        let kernel = self.kernel();
        let placed_normal = |normal: Vec3| unit(first.transform.apply_vector(normal));
        let mut faces: Vec<(FaceName, f64, Vec3)> = Vec::new();
        for info in kernel.faces(shape).map_err(|e| e.to_string())? {
            if info.surface != "plane" {
                continue;
            }
            let Some(name) = info.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                continue;
            };
            let Ok(plane) = kernel.face_plane(shape, &name) else {
                continue;
            };
            if let Some(normal) = placed_normal(plane.normal) {
                faces.push((name, info.area, normal));
            }
        }
        let (face, normal) = match &part.face {
            Some(face) => {
                let plane = kernel
                    .face_plane(shape, face)
                    .map_err(|_| format!("{face} is no planar face of {}", first.name))?;
                let normal = placed_normal(plane.normal)
                    .ok_or_else(|| format!("{face} of {} has no normal", first.name))?;
                (face.clone(), normal)
            }
            None => largest(&faces)
                .ok_or_else(|| format!("{} has no planar face to lie on", first.name))?,
        };

        // Seen along the normal from outside the body: the lines seen, and
        // the material behind them asked along lines of sight across the
        // whole body.
        let (toward_eye, x_axis) = section_axes(normal);
        let spec = ViewSpec {
            toward_eye,
            x_axis,
            hidden: false,
            smooth: true,
            exact: true,
            // Cut sheets write the curves as they are (mitcad#853).
            quick: false,
            section: None,
            deflection: 0.01,
        };
        let shown = vec![first.clone()];
        let projection = self.projected(&shown, &spec)?;
        let placed = self.placed_shapes(&shown)?;
        let body = placed
            .first()
            .ok_or_else(|| format!("the body {} is gone", part.body))?;
        let bounds = kernel
            .bounding_box(body)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("the body {} is empty", part.body))?;
        let depths: Vec<f64> = bounds.corners().map(|c| dot(c, normal)).collect();
        let near = depths.iter().copied().fold(f64::INFINITY, f64::min) - 1.0;
        let far = depths.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 1.0;
        let y_axis = spec.y_axis();
        let outline = cut_sheet::silhouette(&projection.edges, |points: &[P2]| {
            let segments: Vec<(Vec3, Vec3)> = points
                .iter()
                .map(|p| {
                    let at = |depth: f64| -> Vec3 {
                        std::array::from_fn(|k| {
                            x_axis[k] * p[0] + y_axis[k] * p[1] + normal[k] * depth
                        })
                    };
                    (at(near), at(far))
                })
                .collect();
            kernel
                .segments_crossings(body, &segments)
                .map(|all| all.iter().map(|c| !c.is_empty()).collect::<Vec<bool>>())
                .map_err(|e| e.to_string())
        })?;
        if outline.loops.is_empty() {
            return Err(format!(
                "nothing of {} is seen along the normal of {face}",
                first.name
            ));
        }
        let view = Arc::new(PartView {
            face: face.to_string(),
            faces: faces
                .iter()
                .map(|(name, area, _)| (name.to_string(), *area))
                .collect(),
            outline,
        });
        self.drawing_cache().keep_part_view(key, Arc::clone(&view));
        Ok(view)
    }

    fn lay_out(&self, drawing: &Drawing) -> Layout {
        let empty = CutSheet::default();
        let cut = drawing.cut_sheet.as_ref().unwrap_or(&empty);
        let mut parts: Vec<PartState> = cut
            .parts
            .iter()
            .map(|part| {
                let view = self.part_view(drawing.component, part);
                let loops = match &view {
                    Ok(view) => cut_sheet::placed(&view.outline, part.place, part.rotation),
                    Err(_) => Vec::new(),
                };
                let missing = match &view {
                    Ok(_) => false,
                    Err(e) => self.body_shape(part.body).is_none() || e.ends_with("is gone"),
                };
                let name = if self.body_shape(part.body).is_some() {
                    self.body_name(part.body)
                } else {
                    part.body.to_string()
                };
                let off_sheet = cut_sheet::off_sheet(&loops, cut.width, cut.height);
                PartState {
                    part: part.clone(),
                    name,
                    view,
                    loops,
                    missing,
                    off_sheet,
                    overlaps: Vec::new(),
                }
            })
            .collect();
        let mut warnings = Vec::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (i, a) in parts.iter().enumerate() {
            for (j, b) in parts.iter().enumerate().skip(i + 1) {
                if cut_sheet::overlap(&a.loops, &b.loops) {
                    pairs.push((i, j));
                }
            }
        }
        for (i, j) in pairs {
            let (a, b) = (parts[i].part.id, parts[j].part.id);
            warnings.push(format!(
                "part {a} ({}) overlaps part {b} ({})",
                parts[i].name, parts[j].name
            ));
            parts[i].overlaps.push(b);
            parts[j].overlaps.push(a);
        }
        for state in &parts {
            match &state.view {
                Err(e) if state.missing => warnings.push(format!(
                    "part {} ({}) is missing: {e}",
                    state.part.id, state.name
                )),
                Err(e) => warnings.push(format!("part {} ({}): {e}", state.part.id, state.name)),
                Ok(_) if state.off_sheet => warnings.push(format!(
                    "part {} ({}) leaves the sheet",
                    state.part.id, state.name
                )),
                Ok(_) => {}
            }
        }
        Layout { parts, warnings }
    }

    fn new_parts(
        &self,
        drawing: &Drawing,
        bodies: &[PartBody],
        face: Option<&FaceName>,
        count: Option<u32>,
        at: Option<P2>,
    ) -> Result<Vec<CutPart>, ApiError> {
        let count = count.unwrap_or(1);
        if count == 0 || count > MAX_COUNT {
            return Err(ApiError(format!(
                "a part is added 1 to {MAX_COUNT} times, not {count}"
            )));
        }
        if face.is_some() && bodies.len() != 1 {
            return Err(ApiError(
                "a face is chosen for one body at a time".to_owned(),
            ));
        }
        let empty = CutSheet::default();
        let cut = drawing.cut_sheet.as_ref().unwrap_or(&empty);
        let mut taken: Vec<(P2, P2)> = self.lay_out(drawing).parts.iter().map(part_box).collect();
        let mut next_id = cut.next_part_id();
        let mut out: Vec<CutPart> = Vec::new();
        for input in bodies {
            let (text, occurrence) = match input {
                PartBody::Named(text) => (text, None),
                PartBody::Placed { body, occurrence } => (body, occurrence.clone()),
            };
            let body = self.body_by_uid_or_name(text)?;
            let mut part = CutPart {
                id: 0,
                body,
                occurrence,
                face: face.cloned(),
                place: [0.0, 0.0],
                rotation: 0.0,
            };
            let view = self.part_view(drawing.component, &part).map_err(ApiError)?;
            let (lo, hi) = view
                .outline
                .bounds()
                .ok_or_else(|| ApiError(format!("{text} has no outline")))?;
            let size = [hi[0] - lo[0], hi[1] - lo[1]];
            for _ in 0..count {
                part.id = next_id;
                next_id += 1;
                part.place = match at {
                    Some(at) if out.is_empty() => at,
                    _ => free_place(cut.width, cut.height, &taken, size),
                };
                let half = [size[0] / 2.0, size[1] / 2.0];
                taken.push((
                    [part.place[0] - half[0], part.place[1] - half[1]],
                    [part.place[0] + half[0], part.place[1] + half[1]],
                ));
                out.push(part.clone());
            }
        }
        Ok(out)
    }

    fn find_cut_sheet(&self, text: &str) -> Result<(Drawing, CutSheet), ApiError> {
        let drawing = self.find_drawing(text)?.clone();
        let cut = drawing
            .cut_sheet
            .clone()
            .ok_or_else(|| ApiError(format!("{} is a drawing, not a cut sheet", drawing.name)))?;
        Ok((drawing, cut))
    }
}

impl<K: Kernel> CutSheetsOps<K> for Document<K> {
    fn run_cut_sheet_command(&mut self, command: CutSheetCommand) -> Result<Value, ApiError> {
        match command {
            CutSheetCommand::AddCutSheet(c) => {
                let component = match &c.component {
                    Some(text) => self.component_by_ref(text)?,
                    None => ComponentUid::ROOT,
                };
                let width = c.width.unwrap_or(DEFAULT_WIDTH);
                let height = c.height.unwrap_or(DEFAULT_HEIGHT);
                def::check_size(width, height).map_err(ApiError)?;
                let name = match &c.name {
                    Some(name) => name.clone(),
                    None => next_name(self.drawings(), component),
                };
                let mut drawing = Drawing {
                    uid: DrawingUid(0),
                    name,
                    component,
                    sheets: vec![DrawingSheet::new("Sheet1")],
                    revisions: Vec::new(),
                    exported: 0,
                    defaults: Default::default(),
                    cut_sheet: Some(CutSheet {
                        width,
                        height,
                        parts: Vec::new(),
                    }),
                };
                let parts = self.new_parts(&drawing, &c.bodies, None, c.count, None)?;
                let ids: Vec<u32> = parts.iter().map(|p| p.id).collect();
                if let Some(cut) = &mut drawing.cut_sheet {
                    cut.parts = parts;
                }
                let uid = self.add_drawing(drawing)?;
                let added = self.find_drawing(&uid.to_string())?.clone();
                let warnings = self.lay_out(&added).warnings;
                Ok(json!({
                    "drawing": uid,
                    "name": added.name,
                    "component": added.component,
                    "parts": ids,
                    "warnings": warnings,
                }))
            }
            CutSheetCommand::EditCutSheet(c) => {
                let (old, mut cut) = self.find_cut_sheet(&c.drawing)?;
                let mut changed = old.clone();
                if let Some(name) = &c.name {
                    changed.name.clone_from(name);
                }
                cut.width = c.width.unwrap_or(cut.width);
                cut.height = c.height.unwrap_or(cut.height);
                def::check_size(cut.width, cut.height).map_err(ApiError)?;
                changed.cut_sheet = Some(cut);
                let label = if c.width.is_none() && c.height.is_none() {
                    format!("Rename {}", old.name)
                } else {
                    format!("Edit {}", old.name)
                };
                self.set_drawing(old.uid, label, changed)?;
                Ok(json!({"drawing": old.uid}))
            }
            CutSheetCommand::AddParts(c) => {
                let (old, mut cut) = self.find_cut_sheet(&c.drawing)?;
                if c.bodies.is_empty() {
                    return Err(ApiError("add_cut_sheet_parts needs bodies".to_owned()));
                }
                let parts = self.new_parts(&old, &c.bodies, c.face.as_ref(), c.count, c.at)?;
                let ids: Vec<u32> = parts.iter().map(|p| p.id).collect();
                let label = match ids.as_slice() {
                    [one] => format!("Add part {one} to {}", old.name),
                    _ => format!("Add {} parts to {}", ids.len(), old.name),
                };
                cut.parts.extend(parts);
                let mut changed = old.clone();
                changed.cut_sheet = Some(cut);
                self.set_drawing(old.uid, label, changed)?;
                let now = self.find_drawing(&old.uid.to_string())?.clone();
                let warnings = self.lay_out(&now).warnings;
                Ok(json!({"drawing": old.uid, "parts": ids, "warnings": warnings}))
            }
            CutSheetCommand::EditPart(c) => self.edit_cut_part(
                &c.drawing,
                c.part,
                PartEdit {
                    place: c.place,
                    shift: c.shift,
                    rotation: c.rotation,
                    turn: c.turn,
                    face: c.face,
                },
            ),
            CutSheetCommand::DeleteParts { drawing, parts } => {
                self.delete_cut_parts(&drawing, &parts)
            }
            CutSheetCommand::Export(c) => self.export_cut_sheet_checked(
                &c.drawing,
                &c.path,
                c.format.as_deref(),
                c.labels,
                &|_: &str| Ok(()),
            ),
        }
    }

    fn edit_cut_part(&mut self, drawing: &str, id: u32, edit: PartEdit) -> Result<Value, ApiError> {
        let (old, mut cut) = self.find_cut_sheet(drawing)?;
        let component = old.component;
        let part = cut
            .part_mut(id)
            .ok_or_else(|| ApiError(format!("{} has no part {id}", old.name)))?;
        let finite = |p: P2| p[0].is_finite() && p[1].is_finite();
        if let Some(place) = edit.place {
            part.place = place;
        }
        if let Some(by) = edit.shift {
            part.place = [part.place[0] + by[0], part.place[1] + by[1]];
        }
        if !finite(part.place) {
            return Err(ApiError("a part's place is two numbers".to_owned()));
        }
        for degrees in [edit.rotation, edit.turn].into_iter().flatten() {
            if !degrees.is_finite() {
                return Err(ApiError(format!("{degrees} is no angle")));
            }
        }
        if let Some(rotation) = edit.rotation {
            part.rotation = def::normalized_degrees(rotation);
        }
        if let Some(turn) = edit.turn {
            part.rotation = def::normalized_degrees(part.rotation + turn);
        }
        let moved = edit.place.is_some() || edit.shift.is_some();
        let turned = edit.rotation.is_some() || edit.turn.is_some();
        let refaced = edit.face.is_some();
        if let Some(face) = edit.face {
            part.face = face;
            let checked = part.clone();
            self.part_view(component, &checked).map_err(ApiError)?;
        }
        let label = match (moved, turned, refaced) {
            (true, false, false) => format!("Move part {id} of {}", old.name),
            (false, true, false) => format!("Turn part {id} of {}", old.name),
            _ => format!("Edit part {id} of {}", old.name),
        };
        let mut changed = old.clone();
        changed.cut_sheet = Some(cut);
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "part": id}))
    }

    fn delete_cut_parts(&mut self, drawing: &str, parts: &[u32]) -> Result<Value, ApiError> {
        let (old, mut cut) = self.find_cut_sheet(drawing)?;
        if parts.is_empty() {
            return Err(ApiError("delete_cut_sheet_parts needs parts".to_owned()));
        }
        if let Some(id) = parts.iter().find(|id| cut.part(**id).is_none()) {
            return Err(ApiError(format!("{} has no part {id}", old.name)));
        }
        cut.parts.retain(|p| !parts.contains(&p.id));
        let label = match parts {
            [one] => format!("Delete part {one} of {}", old.name),
            _ => format!("Delete {} parts of {}", parts.len(), old.name),
        };
        let mut changed = old.clone();
        changed.cut_sheet = Some(cut);
        self.set_drawing(old.uid, label, changed)?;
        Ok(json!({"drawing": old.uid, "deleted": parts}))
    }

    fn export_cut_sheet_checked(
        &self,
        drawing: &str,
        path: &str,
        format: Option<&str>,
        labels: bool,
        check: &dyn Fn(&str) -> Result<(), String>,
    ) -> Result<Value, ApiError> {
        let (drawing, cut) = self.find_cut_sheet(drawing)?;
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        let format = match format.map(str::to_ascii_lowercase).or(extension) {
            Some(f) if f == "dxf" || f == "svg" => f,
            Some(f) => {
                return Err(ApiError(format!(
                    "a cut sheet is written as DXF or SVG, not {f}"
                )));
            }
            None => {
                return Err(ApiError(format!(
                    "{path}: give a .dxf or .svg file or \"format\""
                )));
            }
        };
        check(path).map_err(ApiError)?;
        let layout = self.lay_out(&drawing);
        let parts: Vec<PlacedPart> = layout
            .parts
            .iter()
            .filter(|s| s.view.is_ok())
            .map(|s| PlacedPart {
                id: s.part.id,
                name: s.name.clone(),
                loops: s.loops.clone(),
            })
            .collect();
        if parts.is_empty() {
            return Err(ApiError(format!("{} has no part to cut", drawing.name)));
        }
        let bytes = if format == "dxf" {
            mitcad_dxf::write_string(
                &cut_sheet::dxf(&parts, labels),
                &mitcad_dxf::WriteOptions::default(),
            )
            .map_err(|e| ApiError(format!("cannot write {path}: {e}")))?
            .into_bytes()
        } else {
            cut_sheet::svg(&parts, cut.width, cut.height, labels).into_bytes()
        };
        std::fs::write(path, bytes).map_err(|e| ApiError(format!("cannot write {path}: {e}")))?;
        let outer: usize = parts
            .iter()
            .map(|p| p.loops.iter().filter(|l| l.outer).count())
            .sum();
        let loops: usize = parts.iter().map(|p| p.loops.len()).sum();
        Ok(json!({
            "path": path,
            "format": format,
            "drawing": drawing.uid,
            "name": drawing.name,
            "parts": parts.len(),
            "outer": outer,
            "inner": loops - outer,
            "labels": labels,
            "sheets": 1,
            "width": cut.width,
            "height": cut.height,
            "warnings": layout.warnings,
        }))
    }

    fn cut_sheet_json(&self, query: &CutSheetQuery) -> Result<Value, ApiError> {
        let (drawing, cut) = self.find_cut_sheet(&query.drawing)?;
        let layout = self.lay_out(&drawing);
        let parts: Vec<Value> = layout
            .parts
            .iter()
            .map(|s| part_json(s, query.geometry))
            .collect();
        // How many of each part: the same body, occurrence and face.
        let mut counts: Vec<(String, Value)> = Vec::new();
        for s in &layout.parts {
            let face = s.view.as_ref().map(|v| v.face.clone()).unwrap_or_default();
            let key = format!(
                "{}|{}|{face}",
                s.part.body,
                s.part
                    .occurrence
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            );
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some((_, value)) => {
                    let n = value["count"].as_u64().unwrap_or(0) + 1;
                    value["count"] = json!(n);
                    if let Some(ids) = value["parts"].as_array_mut() {
                        ids.push(json!(s.part.id));
                    }
                }
                None => counts.push((
                    key,
                    json!({"body": s.part.body, "name": s.name, "face": face,
                           "occurrence": s.part.occurrence, "count": 1, "parts": [s.part.id]}),
                )),
            }
        }
        Ok(json!({
            "drawing": drawing.uid,
            "name": drawing.name,
            "component": drawing.component,
            "component_name": self.assembly().name(drawing.component),
            "kind": "cut_sheet",
            "width": cut.width,
            "height": cut.height,
            "parts": parts,
            "counts": counts.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
            "warnings": layout.warnings,
        }))
    }

    fn cut_sheet_drawing_json(&self, drawing: &Drawing, geometry: bool) -> Value {
        let empty = CutSheet::default();
        let cut = drawing.cut_sheet.as_ref().unwrap_or(&empty);
        let layout = self.lay_out(drawing);
        let views: Vec<Value> = layout
            .parts
            .iter()
            .map(|s| {
                let mut view = part_json(s, false);
                view["type"] = json!("part");
                view["label"] = json!(s.name);
                if s.view.is_err() {
                    view["broken"] = json!(true);
                }
                view
            })
            .collect();
        let placed: Vec<PlacedPart> = layout
            .parts
            .iter()
            .filter(|s| s.view.is_ok())
            .map(|s| PlacedPart {
                id: s.part.id,
                name: s.name.clone(),
                loops: s.loops.clone(),
            })
            .collect();
        let sheet_name = drawing
            .sheets
            .first()
            .map_or_else(|| "Sheet1".to_owned(), |s| s.name.clone());
        let mut sheet = json!({
            "name": sheet_name,
            "size": format!("{} x {} mm", mitcad_model_drawing::trim_number(cut.width, 3),
                            mitcad_model_drawing::trim_number(cut.height, 3)),
            "orientation": "landscape",
            "projection": "third_angle",
            "width": cut.width,
            "height": cut.height,
            "title_block": {},
            "template": null,
            "units": "mm",
            "views": views,
            "scale": "1:1",
            "warnings": layout.warnings,
            "annotations": [],
            "title_fields": [],
        });
        if geometry {
            sheet["groups"] = groups_json(&cut_sheet::sheet(
                cut.width,
                cut.height,
                &drawing.name,
                &placed,
            ));
        }
        json!({
            "drawing": drawing.uid,
            "name": drawing.name,
            "component": drawing.component,
            "component_name": self.assembly().name(drawing.component),
            "kind": "cut_sheet",
            "width": cut.width,
            "height": cut.height,
            "sheet": 0,
            "sheets": [sheet],
            "revisions": [],
            "next_revision": "A",
            "datum_letters": [],
            "decimals": null,
            "text_height": null,
        })
    }
}

/// A part as the queries give it.
fn part_json(s: &PartState, geometry: bool) -> Value {
    let (lo, hi) = part_box(s);
    let mut value = json!({
        "id": s.part.id,
        "body": s.part.body,
        "body_name": s.name,
        "occurrence": s.part.occurrence,
        "own_face": s.part.face.is_some(),
        "place": s.part.place,
        "rotation": s.part.rotation,
        "box": [lo, hi],
        "missing": s.missing,
        "off_sheet": s.off_sheet,
        "overlaps": s.overlaps,
    });
    match &s.view {
        Ok(view) => {
            let (vlo, vhi) = view.outline.bounds().unwrap_or(([0.0; 2], [0.0; 2]));
            value["face"] = json!(view.face);
            value["faces"] = json!(
                view.faces
                    .iter()
                    .map(|(face, area)| json!({"face": face, "area": area}))
                    .collect::<Vec<_>>()
            );
            value["size"] = json!([vhi[0] - vlo[0], vhi[1] - vlo[1]]);
            value["outer"] = json!(view.outline.outer());
            value["inner"] = json!(view.outline.inner());
            // Its outline's B-splines on the sheet (mitcad#853).
            value["splines"] = json!(
                s.loops
                    .iter()
                    .flat_map(|l| &l.pieces)
                    .filter_map(Piece::exact)
                    .map(|spline| spline_json(&spline)["spline"].clone())
                    .collect::<Vec<_>>()
            );
            if geometry {
                value["loops"] = loops_json(&s.loops);
            }
        }
        Err(e) => {
            value["face"] = json!(s.part.face);
            value["error"] = json!(e);
        }
    }
    value
}
