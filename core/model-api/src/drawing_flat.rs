// SPDX-License-Identifier: MIT
//! A base view of a sheet-metal body's Flat Pattern (mitcad#2280,
//! [`Subject::FlatPattern`]; commands.md, "Flat Pattern views").
//!
//! The view shows the body's developed blank (its Flat Pattern with the
//! body's own settings, `Document::developed_flat_pattern`, as the cut
//! sheet and the DXF take it) in place of the folded body: the kernel
//! projects the developed solid seen square on along the pattern's frame
//! (x along the frame's x, y along its y), so that the view is the flat
//! picture the cut sheet and the DXF draw. Its bend lines are added as thin
//! lines (up bends long dash dotted, down bends long dash double dotted,
//! [`mitcad_model_drawing::compose::edge_pen`]), each with its note (the
//! bend's direction, angle and inside radius, the bend table's values) unless
//! the view turns them off. A kerf bend, whose slits stand in for its line,
//! has none. Picks, holes, centre marks and line styles of the view are of
//! the developed solid ([`Shown::flat`]); its bend lines are picked as
//! lines of their own (`ViewPick::bend`).
//!
//! Its direction follows the pattern: whenever the sheet is worked out, a
//! Flat Pattern view's direction is the frame's where the body is placed
//! ([`DrawingFlatOps::current_sheet`]); a body whose pattern cannot be made
//! keeps its view, not drawn, with the reason in words.

use std::borrow::Cow;
use std::sync::Arc;

use serde_json::{Value, json};

use super::drawings::Shown;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::Kernel;
use mitcad_model::sheet::BendId;
use mitcad_model::sheet::flat::{FlatFailure, FlatPattern};
use mitcad_model_drawing::clip::ViewMap;
use mitcad_model_drawing::compose::{self, ViewNote};
use mitcad_model_drawing::def::{Drawing, DrawingSheet, Subject, ViewDirection, ViewKind};
use mitcad_model_drawing::{Units, ViewCurve, ViewEdge, ViewEdgeKind, ViewSpec};
use mitcad_vec3::{cross, unit_div as unit};

/// A body's Flat Pattern as it is evaluated.
pub(super) type Pattern<S> = Arc<Result<FlatPattern<S>, FlatFailure>>;

/// A bend line of a Flat Pattern view (mitcad#2280): its bend and piece,
/// its direction, its ends in the drawing's coordinates, the bend's angle
/// (degrees, without its sign) and inside radius.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BendLine {
    pub(super) bend: BendId,
    pub(super) piece: usize,
    pub(super) up: bool,
    pub(super) start: Vec3,
    pub(super) end: Vec3,
    pub(super) angle: f64,
    pub(super) radius: f64,
}

impl BendLine {
    /// Its middle, in the drawing's coordinates.
    pub(super) fn middle(&self) -> Vec3 {
        std::array::from_fn(|i| (self.start[i] + self.end[i]) / 2.0)
    }
}

pub(super) trait DrawingFlatOps<K: Kernel> {
    /// A body's developed blank as the cut sheet takes it: its Flat
    /// Pattern with its own settings (a new item's when it has none), or
    /// why it cannot be made, in words.
    fn flat_pattern_of(&self, body: BodyUid) -> Result<Pattern<K::Shape>, String>;

    /// What a view projects of a body it shows: the body's shape, or its
    /// Flat Pattern's developed solid.
    fn shown_shape(&self, s: &Shown) -> Result<K::Shape, String>;

    /// What keeps a body's projection and geometry in the caches: its
    /// shape's version, and for a Flat Pattern what its blank is made of.
    fn shown_version(&self, s: &Shown) -> u128;

    /// A Flat Pattern view's direction to the eye and x axis where the
    /// body is placed: along the pattern's frame, so that the view's x and
    /// y are the pattern's.
    fn flat_axes(&self, s: &Shown) -> Result<(Vec3, Vec3), String>;

    /// The sheet as it is drawn: each Flat Pattern view looking along its
    /// pattern's frame now (as it is where the pattern cannot be made).
    fn current_sheet<'a>(
        &self,
        component: ComponentUid,
        sheet: &'a DrawingSheet,
    ) -> Cow<'a, DrawingSheet>;

    /// A Flat Pattern view's bend lines (none for a body shown folded).
    fn bend_lines(&self, s: &Shown) -> Result<Vec<BendLine>, String>;

    /// The lines a Flat Pattern view adds to its projection (its bend lines)
    /// and, with `notes`, their notes in `units`.
    fn flat_lines(
        &self,
        s: &Shown,
        spec: &ViewSpec,
        notes: bool,
        units: Units,
    ) -> Result<(Vec<ViewEdge>, Vec<ViewNote>), String>;

    /// The unit a sheet's notes are written in, under its drawing's
    /// standard.
    fn notes_units(&self, drawing: &Drawing, sheet: &DrawingSheet) -> Units;

    /// A drawn Flat Pattern view's entry in the `drawing` query: its body,
    /// the blank's size in the view, its bends and its bend lines on the
    /// sheet with their pens and notes.
    fn flat_view_json(
        &self,
        s: &Shown,
        spec: &ViewSpec,
        map: &ViewMap,
        size: [f64; 2],
        notes: bool,
        units: Units,
    ) -> Value;
}

impl<K: Kernel> DrawingFlatOps<K> for Document<K> {
    fn flat_pattern_of(&self, body: BodyUid) -> Result<Pattern<K::Shape>, String> {
        if self.body_shape(body).is_none() {
            return Err(mitcad_model::notice::FeatureError::new("a body is gone")
                .with_detail(format!("the body {body} is gone"))
                .text());
        }
        // In words for a person, the ids for agents (mitcad#1741).
        let outcome = self
            .developed_flat_pattern(body)
            .map_err(|e| e.to_string())?;
        if let Err(failure) = &*outcome {
            return Err(mitcad_model::notice::FeatureError::new(format!(
                "the Flat Pattern of {} cannot be made: {}",
                self.body_name(body),
                failure.diagnostics.join("; ")
            ))
            .with_detail(format!(
                "the Flat Pattern of {body} cannot be made: {}",
                failure.agent_reasons()
            ))
            .text());
        }
        Ok(outcome)
    }

    fn shown_shape(&self, s: &Shown) -> Result<K::Shape, String> {
        if s.flat {
            let pattern = self.flat_pattern_of(s.body)?;
            return match &*pattern {
                Ok(pattern) => Ok(pattern.solid.clone()),
                Err(failure) => Err(failure.diagnostics.join("; ")),
            };
        }
        self.body_shape(s.body)
            .cloned()
            .ok_or_else(|| format!("the body {} is gone", s.body))
    }

    fn shown_version(&self, s: &Shown) -> u128 {
        let version = self.body_version(s.body).unwrap_or(0);
        if !s.flat {
            return version;
        }
        // The blank follows the sheet's rules too (a K factor changes it,
        // not the folded body): what it is made of, as it is now.
        let mut fp = Fingerprint::new("drawing-flat-pattern-v1");
        fp.str(&s.body.to_string()).u128(version);
        match self.flat_pattern_of(s.body) {
            Ok(pattern) => {
                if let Ok(pattern) = &*pattern {
                    fp.str(&pattern.sheet_version)
                        .str(&json!(pattern.def).to_string())
                        .f64(pattern.thickness)
                        .f64(pattern.area)
                        .f64(pattern.volume);
                    for v in pattern
                        .frame
                        .origin
                        .iter()
                        .chain(&pattern.frame.x_axis)
                        .chain(&pattern.frame.y_axis)
                    {
                        fp.f64(*v);
                    }
                    for bend in &pattern.bends {
                        fp.f64(bend.angle).f64(bend.radius).f64(bend.allowance);
                        for line in &bend.centerlines {
                            for v in line.start.iter().chain(&line.end) {
                                fp.f64(*v);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                fp.str(&e);
            }
        }
        fp.finish()
    }

    fn flat_axes(&self, s: &Shown) -> Result<(Vec3, Vec3), String> {
        let pattern = self.flat_pattern_of(s.body)?;
        let Ok(pattern) = &*pattern else {
            return Err("the Flat Pattern cannot be made".to_owned());
        };
        let x = unit(s.transform.apply_vector(pattern.frame.x_axis));
        let y = unit(s.transform.apply_vector(pattern.frame.y_axis));
        let (Some(x), Some(y)) = (x, y) else {
            return Err(format!(
                "the Flat Pattern of {} has no frame",
                self.body_name(s.body)
            ));
        };
        let eye = unit(cross(x, y)).ok_or_else(|| {
            format!(
                "the Flat Pattern of {} has no frame",
                self.body_name(s.body)
            )
        })?;
        // At right angles to the eye, after a placement that is not quite.
        let x = unit(cross(cross(eye, x), eye)).unwrap_or(x);
        Ok((eye, x))
    }

    fn current_sheet<'a>(
        &self,
        component: ComponentUid,
        sheet: &'a DrawingSheet,
    ) -> Cow<'a, DrawingSheet> {
        let mut out = Cow::Borrowed(sheet);
        for (i, view) in sheet.views.iter().enumerate() {
            let (ViewKind::Base { direction }, Some(subject @ Subject::FlatPattern { .. })) =
                (&view.kind, &view.subject)
            else {
                continue;
            };
            let Some((toward_eye, x_axis)) = self
                .subject_shown(component, subject)
                .ok()
                .and_then(|(shown, _)| shown.into_iter().next())
                .and_then(|s| self.flat_axes(&s).ok())
            else {
                continue;
            };
            let now = ViewDirection::Given { toward_eye, x_axis };
            if *direction != now {
                out.to_mut().views[i].kind = ViewKind::Base { direction: now };
            }
        }
        out
    }

    fn bend_lines(&self, s: &Shown) -> Result<Vec<BendLine>, String> {
        if !s.flat {
            return Ok(Vec::new());
        }
        let pattern = self.flat_pattern_of(s.body)?;
        let Ok(pattern) = &*pattern else {
            return Ok(Vec::new());
        };
        let frame = &pattern.frame;
        let at = |p: [f64; 2]| {
            let local: Vec3 = std::array::from_fn(|i| {
                frame.origin[i] + p[0] * frame.x_axis[i] + p[1] * frame.y_axis[i]
            });
            s.transform.apply_point(local)
        };
        let mut out = Vec::new();
        // A kerf bend's slits are cuts in place of its line (mitcad#1939),
        // as the cut sheet draws it.
        for bend in pattern.bends.iter().filter(|b| b.slits.is_empty()) {
            for (piece, line) in bend.centerlines.iter().enumerate() {
                out.push(BendLine {
                    bend: bend.bend,
                    piece,
                    up: bend.direction == "up",
                    start: at(line.start),
                    end: at(line.end),
                    angle: bend.angle.to_degrees().abs(),
                    radius: bend.radius,
                });
            }
        }
        Ok(out)
    }

    fn flat_lines(
        &self,
        s: &Shown,
        spec: &ViewSpec,
        notes: bool,
        units: Units,
    ) -> Result<(Vec<ViewEdge>, Vec<ViewNote>), String> {
        let mut edges = Vec::new();
        let mut written = Vec::new();
        for line in self.bend_lines(s)? {
            let (a, b) = (spec.project(line.start), spec.project(line.end));
            edges.push(ViewEdge {
                visible: true,
                kind: ViewEdgeKind::Bend { up: line.up },
                curve: ViewCurve::Line(a, b),
                source: None,
            });
            if notes {
                written.push(ViewNote {
                    at: [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
                    angle: (b[1] - a[1]).atan2(b[0] - a[0]),
                    text: compose::bend_note_text(line.up, line.angle, line.radius, units),
                });
            }
        }
        // A Lofted Flange's facet lines (mitcad#2512): thin, as edges
        // between faces that meet smoothly.
        if s.flat
            && let Ok(pattern) = self.flat_pattern_of(s.body)
            && let Ok(pattern) = &*pattern
        {
            let frame = &pattern.frame;
            let at = |p: [f64; 2]| {
                let local: Vec3 = std::array::from_fn(|i| {
                    frame.origin[i] + p[0] * frame.x_axis[i] + p[1] * frame.y_axis[i]
                });
                spec.project(s.transform.apply_point(local))
            };
            for line in &pattern.facet_lines {
                edges.push(ViewEdge {
                    visible: true,
                    kind: ViewEdgeKind::Smooth,
                    curve: ViewCurve::Line(at(line.start), at(line.end)),
                    source: None,
                });
            }
        }
        Ok((edges, written))
    }

    fn notes_units(&self, drawing: &Drawing, sheet: &DrawingSheet) -> Units {
        sheet
            .units
            .as_deref()
            .and_then(Units::by_symbol)
            .unwrap_or_else(|| Units::of(self.parameters().context().default_length_unit))
            .under(drawing.defaults.standard)
    }

    fn flat_view_json(
        &self,
        s: &Shown,
        spec: &ViewSpec,
        map: &ViewMap,
        size: [f64; 2],
        notes: bool,
        units: Units,
    ) -> Value {
        let lines = self.bend_lines(s).unwrap_or_default();
        let entries: Vec<Value> = lines
            .iter()
            .map(|line| {
                let kind = ViewEdgeKind::Bend { up: line.up };
                json!({
                    "bend": line.bend,
                    "piece": line.piece,
                    "up": line.up,
                    "pen": compose::edge_pen(true, kind).name(),
                    "angle": line.angle,
                    "radius": line.radius,
                    "line": [map.at(spec.project(line.start)), map.at(spec.project(line.end))],
                    "note": notes.then(|| {
                        compose::bend_note_text(line.up, line.angle, line.radius, units)
                    }),
                })
            })
            .collect();
        let mut bends: Vec<BendId> = lines.iter().map(|l| l.bend).collect();
        bends.dedup();
        json!({
            "body": s.body,
            "name": s.name,
            "occurrence": (!s.path.is_empty()).then(|| {
                s.path.iter().map(ToString::to_string).collect::<Vec<_>>().join("/")
            }),
            "size": size,
            "bends": bends.len(),
            "up": lines.iter().filter(|l| l.up).count(),
            "down": lines.iter().filter(|l| !l.up).count(),
            "bend_notes": notes,
            "bend_lines": entries,
        })
    }
}
