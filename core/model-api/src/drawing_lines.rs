// SPDX-License-Identifier: MIT
//! Widths and patterns chosen in a drawing's view for the lines of a
//! projected edge, of a body, or of the bodies an occurrence places
//! (mitcad#1546; commands.md, "Line styles in drawing views"): the command
//! `edit_drawing_line`, and the pens a view's choices give its lines when
//! the sheet is drawn.
//!
//! A view keeps its choices (`DrawingView::lines`) by the model's names: an
//! edge by its body (where an occurrence places it) and its topological
//! name, so that the choice follows the model as an annotation's pick does.
//! When the view is drawn, each chosen edge is found again by its name
//! (`Kernel::edge_geometry`), seen as the view sees it, and the projected
//! lines of its body that lie on it take its pen
//! (`mitcad_model_drawing::compose::LinePens`); the kernel's hidden-line
//! removal keeps no edge names, so the lines are matched by where they lie.
//! A chosen edge that is gone, or a body the view no longer shows, is a
//! warning of the view; the choice is kept.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::drawing_annotations::placed_curve;
use super::drawings::Shown;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::topo::EdgeName;
use mitcad_model_drawing::annotate::{self, PenPattern, PenWidth, SketchPen};
use mitcad_model_drawing::compose::{EdgeSeen, LinePens};
use mitcad_model_drawing::def::{DrawingView, LineStyle};
use mitcad_model_drawing::sheet::Pen;
use mitcad_model_drawing::{P2, ViewSpec};

/// How far (view mm) a projected line may lie from a chosen edge and be one
/// of its lines; a view projected from a triangulation allows its
/// deflection.
const LINE_TOLERANCE: f64 = 1e-3;

/// `edit_drawing_line`: a width and a pattern for the lines of an edge, a
/// body or an occurrence in one view, or (`reset`) the view's own again.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EditLineCommand {
    drawing: String,
    view: u32,
    /// A sheet point on the view: the edge a click there picks, and `of`
    /// what (its line, its body's lines, its occurrence's).
    #[serde(default)]
    at: Option<P2>,
    #[serde(default)]
    of: Option<LineOf>,
    /// Instead of `at`: what is named, as a pick names it.
    #[serde(default)]
    body: Option<BodyUid>,
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    #[serde(default)]
    edge: Option<EdgeName>,
    #[serde(default)]
    width: Option<PenWidth>,
    #[serde(default)]
    pattern: Option<PenPattern>,
    /// The view's own lines again: the choice removed.
    #[serde(default)]
    reset: bool,
}

/// What of the edge picked at `at` a line style is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LineOf {
    /// The edge's lines.
    #[default]
    Edge,
    /// Every line of its body.
    Body,
    /// Every line of the bodies its occurrence (a component placed in the
    /// view) places.
    Component,
}

/// The pen of a chosen width and pattern.
fn pen_of(line: &LineStyle) -> Pen {
    annotate::sketch_pen(SketchPen::new(line.width, line.pattern))
}

/// The document's methods of this module used across the api (mitcad#777).
pub(super) trait DrawingLinesOps<K: Kernel> {
    /// `edit_drawing_line`: one undo step (`Edit line in view 1 of
    /// Drawing1`, `Reset line in view 1 of Drawing1`).
    fn edit_drawing_line(&mut self, c: EditLineCommand) -> Result<Value, ApiError>;

    /// The pens a view's choices give its lines (`shown`: the bodies it
    /// projected, in order; `polygonal`: projected from a triangulation),
    /// with a warning for each choice it cannot find.
    fn line_pens(
        &self,
        view: &DrawingView,
        shown: &[Shown],
        spec: &ViewSpec,
        polygonal: bool,
    ) -> (LinePens, Vec<String>);
}

impl<K: Kernel> DrawingLinesOps<K> for Document<K> {
    fn edit_drawing_line(&mut self, c: EditLineCommand) -> Result<Value, ApiError> {
        let old = self.find_drawing(&c.drawing)?.clone();
        if old.cut_sheet.is_some() {
            return Err(ApiError(format!(
                "{} is a cut sheet: its parts' lines are cut lines",
                old.name
            )));
        }
        let (si, view) = old
            .find_view(c.view)
            .ok_or_else(|| ApiError(format!("{} has no view {}", old.name, c.view)))?;
        let view = view.clone();
        let evaluated = self.evaluate_views(&old, si);
        let (body, occurrence, edge) = match c.at {
            Some(at) => {
                if c.body.is_some() || c.occurrence.is_some() || c.edge.is_some() {
                    return Err(ApiError(
                        "a line is given by `at` or by `body`, `occurrence` and `edge`, not both"
                            .to_owned(),
                    ));
                }
                let seen = self
                    .view_at(&old, si, &evaluated, c.view)
                    .map_err(ApiError)?;
                let pick = self.edge_pick_at(&seen, at).ok_or_else(|| {
                    ApiError(format!(
                        "no edge of view {} at {:.2}, {:.2}",
                        c.view, at[0], at[1]
                    ))
                })?;
                let occurrence = pick.occurrence.clone().filter(|p| !p.is_empty());
                match c.of.unwrap_or_default() {
                    LineOf::Edge => (Some(pick.body), occurrence, pick.edge.clone()),
                    LineOf::Body => (Some(pick.body), occurrence, None),
                    LineOf::Component => {
                        if occurrence.is_none() {
                            return Err(ApiError(format!(
                                "the body {} lies in no component of the view: choose the body",
                                pick.body
                            )));
                        }
                        (None, occurrence, None)
                    }
                }
            }
            None => {
                if c.of.is_some() {
                    return Err(ApiError("`of` goes with `at`".to_owned()));
                }
                (
                    c.body,
                    c.occurrence.clone().filter(|p| !p.is_empty()),
                    c.edge.clone(),
                )
            }
        };
        let mut style = LineStyle {
            body,
            occurrence,
            edge,
            width: PenWidth::Thick,
            pattern: PenPattern::Continuous,
        };
        style.check().map_err(ApiError)?;
        let existing = view.lines.iter().position(|l| l.same_target(&style));
        let mut changed = old.clone();
        let i = changed.sheets[si]
            .view_index(c.view)
            .expect("the view is on its sheet");
        let lines = &mut changed.sheets[si].views[i].lines;
        if c.reset {
            if c.width.is_some() || c.pattern.is_some() {
                return Err(ApiError(
                    "a reset takes no width or pattern: the view's own lines come back".to_owned(),
                ));
            }
            let Some(k) = existing else {
                return Ok(json!({"drawing": old.uid, "view": c.view, "line": null}));
            };
            lines.remove(k);
            self.set_drawing(
                old.uid,
                format!("Reset line in view {} of {}", c.view, old.name),
                changed,
            )?;
            return Ok(json!({"drawing": old.uid, "view": c.view, "line": null}));
        }
        if c.width.is_none() && c.pattern.is_none() {
            return Err(ApiError(
                "give the line's width or pattern, or reset it".to_owned(),
            ));
        }
        // What is named must be in the view: a body it shows (where the
        // occurrence places it), its edge, or an occurrence of its bodies.
        let shown: Vec<&Shown> = evaluated
            .view_shown
            .get(&c.view)
            .map(|own| own.iter().filter_map(|k| evaluated.shown.get(*k)).collect())
            .unwrap_or_default();
        if !shown.iter().any(|s| style.rank(s.body, &s.path).is_some()) {
            return Err(ApiError(format!(
                "view {} does not show {}",
                c.view,
                style.target_text()
            )));
        }
        if let (Some(body), Some(edge)) = (style.body, &style.edge) {
            let shape = self
                .body_shape(body)
                .ok_or_else(|| ApiError(format!("the body {body} is gone")))?;
            self.kernel()
                .edge_geometry(shape, edge)
                .map_err(|_| ApiError(format!("the body {body} has no edge {edge}")))?;
        }
        if let Some(k) = existing {
            style.width = lines[k].width;
            style.pattern = lines[k].pattern;
        }
        style.width = c.width.unwrap_or(style.width);
        style.pattern = c.pattern.unwrap_or(style.pattern);
        match existing {
            Some(k) => lines[k] = style.clone(),
            None => lines.push(style.clone()),
        }
        self.set_drawing(
            old.uid,
            format!("Edit line in view {} of {}", c.view, old.name),
            changed,
        )?;
        Ok(json!({"drawing": old.uid, "view": c.view, "line": style}))
    }

    fn line_pens(
        &self,
        view: &DrawingView,
        shown: &[Shown],
        spec: &ViewSpec,
        polygonal: bool,
    ) -> (LinePens, Vec<String>) {
        let mut pens = LinePens {
            tolerance: if polygonal {
                spec.deflection.max(LINE_TOLERANCE)
            } else {
                LINE_TOLERANCE
            },
            ..LinePens::default()
        };
        let mut warnings = Vec::new();
        if view.lines.is_empty() {
            return (pens, warnings);
        }
        // Each body's pen: of the choices that name it, the closest (the
        // body's own, then its deepest occurrence's).
        for (i, s) in shown.iter().enumerate() {
            let best = view
                .lines
                .iter()
                .filter(|l| l.edge.is_none())
                .filter_map(|l| l.rank(s.body, &s.path).map(|rank| (rank, l)))
                .min_by_key(|(rank, _)| *rank);
            if let Some((_, line)) = best {
                pens.bodies.push((i, pen_of(line)));
            }
        }
        for line in view.lines.iter().filter(|l| l.edge.is_none()) {
            if !shown.iter().any(|s| line.rank(s.body, &s.path).is_some()) {
                warnings.push(format!(
                    "the view no longer shows {}: its line style is kept",
                    line.target_text()
                ));
            }
        }
        // Each edge's lines: those of its body that lie on it as the view
        // sees it now.
        for line in &view.lines {
            let (Some(body), Some(edge)) = (line.body, &line.edge) else {
                continue;
            };
            let Some(i) = shown
                .iter()
                .position(|s| s.body == body && s.path.as_slice() == line.path())
            else {
                warnings.push(format!(
                    "the view no longer shows {}: its line style is kept",
                    line.target_text()
                ));
                continue;
            };
            let curve = self
                .body_shape(body)
                .and_then(|shape| self.kernel().edge_geometry(shape, edge).ok());
            let Some(curve) = curve else {
                warnings.push(format!(
                    "{} is gone: its line style is kept",
                    line.target_text()
                ));
                continue;
            };
            let placed = placed_curve(&curve, &shown[i].transform);
            let seen = match &placed {
                CurveGeometry::Other { start, end, .. } => {
                    EdgeSeen::Ends(spec.project(*start), spec.project(*end))
                }
                _ => {
                    match annotate::seen_curve(&placed, None, |p| spec.project(p), spec.toward_eye)
                    {
                        Ok(seen) => EdgeSeen::Seen(seen),
                        Err(_) => continue,
                    }
                }
            };
            pens.edges.push((i, seen, pen_of(line)));
        }
        (pens, warnings)
    }
}
