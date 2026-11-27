// SPDX-License-Identifier: MIT
//! A cut sheet's cutting paths and its plasma program (mitcad#2553;
//! commands.md, "Cutting a cut sheet with a plasma torch"): the parts'
//! loops where they lie on the sheet, moved half the kerf away from the
//! material once (`cut_sheet::kerf_compensated`, as the DXF and SVG
//! exports move them; lines stay lines and arcs arcs, mitcad#2570, so that
//! the program cuts arcs and circles as G2 and G3), ordered with their
//! pierce points and lead-ins by
//! `mitcad_cam::sheet_cut::plan`, and written as grbl 1.1's G-code for a
//! plasma torch by `sheet_cut::post_grbl_plasma`. Bend lines and marks are
//! never cut.
//!
//! What keeps a layout from being cut is a problem in words (no names, ids
//! or coordinates; the parts it concerns are given apart, for the
//! application to show them): parts that overlap or lie closer together
//! than the kerf, a part off the sheet, a part that cannot be worked out
//! or whose body is gone, a hole narrower than the kerf, a kerf bend's
//! slits, settings out of range. `post_cut_sheet` is refused while there
//! is one.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{CutSheetsLocal, Layout, yes};
use crate::ApiError;
use mitcad_cam::sheet_cut::{self, EdgeArc, PlasmaSettings, SheetCutPlan, SheetLoop};
use mitcad_model::Document;
use mitcad_model::kernel::Kernel;
use mitcad_model_drawing::cut_sheet::{self, OutlineLoop, Piece};
use mitcad_model_drawing::cut_sheet_def::{CutSheet, MarkKind};
use mitcad_model_drawing::def::Drawing;
use mitcad_model_drawing::sheet::{Anchor, Group, Item, Pen};

/// The controller the program is written for.
pub(super) const CONTROLLER: &str = "grbl 1.1";
/// The cutting process.
pub(super) const PROCESS: &str = "plasma";

/// The `cut_sheet_cutting` query: a cut sheet's cutting paths and what
/// keeps it from being cut.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CuttingQuery {
    drawing: String,
    /// Settings to try in place of the cut sheet's (nothing is kept).
    #[serde(default)]
    cutting: Option<PlasmaSettings>,
    /// A kerf to try in place of the cut sheet's, mm.
    #[serde(default)]
    kerf: Option<f64>,
    /// Each cut's path; left out with false.
    #[serde(default = "yes")]
    geometry: bool,
}

/// `post_cut_sheet`: the program written to a file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PostCutSheetCommand {
    pub(super) drawing: String,
    pub(super) path: String,
    /// Settings in place of the cut sheet's (nothing is kept).
    #[serde(default)]
    pub(super) cutting: Option<PlasmaSettings>,
    /// A kerf in place of the cut sheet's, mm (nothing is kept).
    #[serde(default)]
    pub(super) kerf: Option<f64>,
}

/// The cut sheet with a kerf asked for in place of its own.
fn with_kerf(mut cut: CutSheet, kerf: Option<f64>) -> Result<CutSheet, ApiError> {
    if kerf.is_some() {
        cut.kerf = super::check_kerf(kerf)?;
    }
    Ok(cut)
}

/// What keeps a layout from being cut.
pub(super) struct Problem {
    /// In words, for the person cutting.
    pub(super) message: String,
    /// The parts it concerns (ids), for the application to show them.
    pub(super) parts: Vec<u32>,
}

/// A layout's cutting paths and its problems.
pub(super) struct Cutting {
    pub(super) settings: PlasmaSettings,
    pub(super) kerf: f64,
    pub(super) plan: SheetCutPlan,
    pub(super) problems: Vec<Problem>,
    /// Bend lines left out of the cut paths.
    pub(super) bend_lines: usize,
    /// Engraved and scored marks left out of the cut paths.
    pub(super) marks: usize,
}

fn problem(message: &str, parts: Vec<u32>) -> Problem {
    Problem {
        message: message.to_owned(),
        parts,
    }
}

/// Whether the beam's paths keep half the kerf `d` from every edge of the
/// part: a hole or a notch narrower than the kerf brings a path (moved
/// along its edges' normals) nearer to another edge, or turns it inside
/// out. Lines and arcs are measured exactly (an arc's path lies `d` from
/// the arc itself, nearer to the lines that would stand for it).
fn paths_keep_clear(loops: &[OutlineLoop], paths: &[OutlineLoop], d: f64) -> bool {
    let least = d * (1.0 - 1e-6) - 1e-9;
    paths.iter().all(|path| {
        path.polygon().iter().all(|p| {
            loops
                .iter()
                .flat_map(|l| &l.pieces)
                .all(|piece| piece.distance(*p) >= least)
        })
    })
}

/// An arc no longer than this (radians) is a line between its ends.
const STRAIGHT: f64 = 1e-6;

/// A part's moved loop as the edges the torch follows (mitcad#2570): its
/// lines and arcs as they are, the points of its other pieces as lines.
fn sheet_loop(part: u32, l: &OutlineLoop) -> SheetLoop {
    let mut points = Vec::with_capacity(l.pieces.len());
    let mut arcs = Vec::with_capacity(l.pieces.len());
    for piece in &l.pieces {
        match piece {
            Piece::Line(a, _) => {
                points.push(*a);
                arcs.push(None);
            }
            Piece::Arc {
                center,
                radius,
                sweep,
                ..
            } if *radius > 0.0 && sweep.abs() > STRAIGHT => {
                points.push(piece.start());
                arcs.push(Some(EdgeArc {
                    center: *center,
                    ccw: *sweep > 0.0,
                }));
            }
            _ => {
                let flat = piece.flattened();
                for p in &flat[..flat.len() - 1] {
                    points.push(*p);
                    arcs.push(None);
                }
            }
        }
    }
    SheetLoop {
        part,
        outer: l.outer,
        points,
        arcs,
    }
}

/// The cutting paths of a laid out cut sheet with `settings`.
pub(super) fn cutting(layout: &Layout, cut: &CutSheet, settings: &PlasmaSettings) -> Cutting {
    let kerf = cut.kerf.unwrap_or(0.0);
    let mut problems = Vec::new();
    if let Err(e) = settings.check() {
        problems.push(problem(&e.message, Vec::new()));
    }
    if layout.parts.is_empty() {
        problems.push(problem(
            "The cut sheet has no component to cut: add Flat Patterns or components first.",
            Vec::new(),
        ));
    }
    // Each part that can be cut, with its paths.
    let mut paths: Vec<(u32, Vec<OutlineLoop>)> = Vec::new();
    let mut bend_lines = 0;
    let mut marks = 0;
    for s in &layout.parts {
        let id = s.part.id;
        match &s.view {
            Err(_) if s.missing => problems.push(problem(
                "A component's body is gone: delete the component from the cut sheet or bring the body back.",
                vec![id],
            )),
            Err(_) => problems.push(problem(
                "A component's outline cannot be worked out: the cut sheet's warning says why.",
                vec![id],
            )),
            Ok(_) if s.loops.is_empty() => problems.push(problem(
                "A component has no outline to cut.",
                vec![id],
            )),
            Ok(_) => {
                if s.off_sheet {
                    problems.push(problem(
                        "A component reaches past the edge of the sheet: move it onto the sheet or make the material larger.",
                        vec![id],
                    ));
                }
                let compensated = cut_sheet::kerf_compensated(&s.loops, kerf);
                if kerf > 0.0 && !paths_keep_clear(&s.loops, &compensated, kerf / 2.0) {
                    problems.push(problem(
                        "A component has a hole or notch narrower than the kerf: the torch cannot cut it to size. Make it wider or the kerf smaller.",
                        vec![id],
                    ));
                }
                if s.marks.iter().any(|m| m.kind == MarkKind::Cut) {
                    problems.push(problem(
                        "A component has a kerf bend's slits, which a plasma torch cannot cut: export the cut sheet as DXF for a laser instead.",
                        vec![id],
                    ));
                }
                bend_lines += s.bends.len();
                marks += s.marks.iter().filter(|m| m.kind != MarkKind::Cut).count();
                paths.push((id, compensated));
            }
        }
    }
    for (i, a) in layout.parts.iter().enumerate() {
        for b in &layout.parts[i + 1..] {
            if a.overlaps.contains(&b.part.id) {
                problems.push(problem(
                    "Two components overlap: move them apart.",
                    vec![a.part.id, b.part.id],
                ));
            }
        }
    }
    // The beam's paths of two parts that do not overlap still run into
    // each other when the parts lie closer together than the kerf.
    if kerf > 0.0 {
        for (i, (a, pa)) in paths.iter().enumerate() {
            for (b, pb) in &paths[i + 1..] {
                let overlapping = layout
                    .parts
                    .iter()
                    .any(|s| s.part.id == *a && s.overlaps.contains(b));
                if !overlapping && cut_sheet::overlap(pa, pb) {
                    problems.push(problem(
                        "Two components lie closer together than the kerf: leave more room between them.",
                        vec![*a, *b],
                    ));
                }
            }
        }
    }
    let loops: Vec<SheetLoop> = paths
        .iter()
        .flat_map(|(id, loops)| loops.iter().map(move |l| sheet_loop(*id, l)))
        .collect();
    let plan = sheet_cut::plan(&loops, [cut.width, cut.height], settings);
    Cutting {
        settings: settings.clone(),
        kerf,
        plan,
        problems,
        bend_lines,
        marks,
    }
}

/// The cut sheet's own settings, or the defaults.
pub(super) fn settings_of(cut: &CutSheet, asked: Option<&PlasmaSettings>) -> PlasmaSettings {
    asked.or(cut.cutting.as_ref()).cloned().unwrap_or_default()
}

fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// The query's answer.
pub(super) fn cutting_json(drawing: &Drawing, c: &Cutting, geometry: bool) -> Value {
    let cuts: Vec<Value> = c
        .plan
        .cuts
        .iter()
        .enumerate()
        .map(|(k, cut)| {
            let mut value = json!({
                "order": k + 1,
                "part": cut.part,
                "outer": cut.outer,
                "pierce": cut.pierce,
                "start": cut.start,
                "lead_in": round(cut.lead_in()),
                "length": round(cut.length()),
                "arcs": cut.arc_count(),
            });
            if geometry {
                value["path"] = json!(cut.points());
            }
            value
        })
        .collect();
    json!({
        "drawing": drawing.uid,
        "name": drawing.name,
        "ready": c.problems.is_empty(),
        "problems": c.problems.iter().map(|p| json!({"message": p.message, "parts": p.parts}))
            .collect::<Vec<_>>(),
        "process": PROCESS,
        "controller": CONTROLLER,
        "settings": c.settings,
        "kerf": c.kerf,
        "cuts": cuts,
        "pierces": c.plan.cuts.len(),
        "holes": c.plan.cuts.iter().filter(|cut| !cut.outer).count(),
        "cut_length": round(c.plan.cut_length()),
        "travel_length": round(c.plan.travel_length()),
        "arc_time": round(c.plan.arc_time(&c.settings)),
        "left_out": {"bend_lines": c.bend_lines, "marks": c.marks},
    })
}

/// The cutting paths drawn over the sheet (the `drawing` query's
/// `cutting_paths`): a `cut-path` group of the paths and lead-ins (pen
/// `thin`), the moves across (pen `hidden`), each pierce point a small
/// circle with its place in the order beside it.
pub(super) fn cutting_groups(c: &Cutting) -> Vec<Group> {
    let mut paths = Group::new("cut-path");
    paths.id = Some("cut-paths".to_owned());
    let mut at = [0.0, 0.0];
    for (k, cut) in c.plan.cuts.iter().enumerate() {
        paths.line(Pen::Hidden, at, cut.pierce);
        paths.line(Pen::Thin, cut.pierce, cut.start);
        paths.items.push(Item::Polyline {
            pen: Pen::Thin,
            points: cut.points(),
            closed: false,
        });
        paths.items.push(Item::Arc {
            pen: Pen::Thin,
            center: cut.pierce,
            radius: 0.6,
            start: 0.0,
            end: std::f64::consts::TAU,
        });
        paths.text(
            [cut.pierce[0] + 1.0, cut.pierce[1] + 1.0],
            2.5,
            0.0,
            Anchor::Start,
            (k + 1).to_string(),
        );
        at = cut.start;
    }
    if !c.plan.cuts.is_empty() {
        paths.line(Pen::Hidden, at, [0.0, 0.0]);
    }
    vec![paths]
}

/// `cut_sheet_cutting`.
pub(crate) fn cutting_query<K: Kernel>(
    doc: &Document<K>,
    query: &CuttingQuery,
) -> Result<Value, ApiError> {
    let (drawing, cut) = doc.find_cut_sheet(&query.drawing)?;
    let cut = with_kerf(cut, query.kerf)?;
    let layout = doc.lay_out(&drawing);
    let settings = settings_of(&cut, query.cutting.as_ref());
    let c = cutting(&layout, &cut, &settings);
    Ok(cutting_json(&drawing, &c, query.geometry))
}

/// `post_cut_sheet`: the program written to `path`, refused while the
/// layout has a problem.
pub(super) fn post<K: Kernel>(
    doc: &Document<K>,
    command: &PostCutSheetCommand,
) -> Result<Value, ApiError> {
    let (drawing, cut) = doc.find_cut_sheet(&command.drawing)?;
    let cut = with_kerf(cut, command.kerf)?;
    let layout = doc.lay_out(&drawing);
    let settings = settings_of(&cut, command.cutting.as_ref());
    let c = cutting(&layout, &cut, &settings);
    if !c.problems.is_empty() {
        let words: Vec<&str> = c.problems.iter().map(|p| p.message.as_str()).collect();
        return Err(ApiError(format!(
            "The cut sheet cannot be cut yet. {}",
            words.join(" ")
        )));
    }
    let trim = |v: f64| mitcad_model_drawing::trim_number(v, 3);
    let notes = vec![
        format!(
            "Sheet {} x {} mm; work zero: its lower left corner on its top surface",
            trim(cut.width),
            trim(cut.height)
        ),
        if c.kerf > 0.0 {
            format!(
                "Kerf {} mm: the paths are moved half of it off the parts",
                trim(c.kerf)
            )
        } else {
            "No kerf: the paths follow the parts' outlines".to_owned()
        },
        format!(
            "{} cuts, {} holes; bend lines and marks are not cut",
            c.plan.cuts.len(),
            c.plan.cuts.iter().filter(|cut| !cut.outer).count()
        ),
    ];
    let text = sheet_cut::post_grbl_plasma(&c.plan, &c.settings, &drawing.name, &notes)
        .map_err(|e| ApiError(e.message))?;
    std::fs::write(&command.path, &text)
        .map_err(|e| ApiError(format!("cannot write {}: {e}", command.path)))?;
    let mut answer = cutting_json(&drawing, &c, false);
    answer["path"] = json!(command.path);
    answer["lines"] = json!(text.lines().count());
    Ok(answer)
}
