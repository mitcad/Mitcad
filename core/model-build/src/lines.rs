// SPDX-License-Identifier: MIT
//! Profiles along sketch lines (mitcad#1965; `commands.md`, "BUILD: profile
//! assemblies", *Along sketch lines*): `build_place` with `lines` places one
//! member of the chosen part along each straight line picked (lines of one
//! or more sketches, whole sketches, `chain` adding the lines joined to
//! them end to end), each a member along an edge (`edge.rs`) whose edge is
//! the sketch line: it follows the line when the sketch changes. The
//! joints get their conditions as `corners` says (mitcad#2200,
//! `conditions.rs`): an L a shared miter (`mitre`, the default) or a butt
//! (`butt`), a T its stem to the other member's near side, three or more
//! ends at a point the first line's member running through; `none` leaves
//! them square. The conditions are kept with the joints. Arcs, circles and
//! splines are refused in words: members go along straight lines only. All
//! of it is one undo step.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::conditions::{ConditionsCrateOps, Corners, LineJoint, line_joints};
use crate::edge::{Align, AlongCrateOps, AlongEdge};
use crate::geometry::{V3, dot, norm, scale, sub, unit};
use crate::place::{Plan, refuse};
use crate::prelude::*;
use crate::{PlaceInput, Placed};
use mitcad_model::features::edge_frame::{SketchLine, sketch_line};
use mitcad_model::features::{FeatureDef, GeomRef};
use mitcad_model::ids::{ComponentUid, EntityUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, ModelError};

/// How near two line ends must be to meet, mm.
pub(crate) const MEET: f64 = 1e-3;

/// Where members along sketch lines go (`build_place`'s `lines`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlongLines {
    /// A path from the BUILD host to the component the sketches are in;
    /// empty for its own.
    #[serde(default)]
    pub occurrence: String,
    /// Sketch lines (`{"sketch", "curve"}`), curves of a sketch
    /// (`{"sketch", "curves"}`) or whole sketches (`{"sketch"}`).
    pub curves: Vec<GeomRef>,
    /// The lines joined end to end to the picked ones, in their sketch,
    /// too: the chain they are in.
    #[serde(default)]
    pub chain: bool,
    /// How each member's section sits on its line; centered when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    /// The joints' conditions (mitcad#2200): `mitre` (the default) or
    /// `butt` at the L's, the T's to the near side; `none` square. `true`
    /// and `false` read as `mitre` and `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
}

/// A sketch line a member goes along, in the BUILD host's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub sketch: FeatureUid,
    pub curve: EntityUid,
    pub start: V3,
    pub end: V3,
}

impl Line {
    fn edge(&self) -> GeomRef {
        GeomRef::SketchCurves {
            sketch: self.sketch,
            curves: vec![self.curve],
        }
    }

    fn length(&self) -> f64 {
        norm(sub(self.end, self.start))
    }

    /// Whether `p` is on the line between its ends, not at them.
    pub(crate) fn holds_inside(&self, p: V3) -> bool {
        let d = sub(self.end, self.start);
        let length = norm(d);
        let Some(u) = unit(d) else {
            return false;
        };
        let along = dot(sub(p, self.start), u);
        along > MEET
            && along < length - MEET
            && norm(sub(sub(p, self.start), scale(u, along))) <= MEET
    }
}

fn near(a: V3, b: V3) -> bool {
    norm(sub(a, b)) <= MEET
}

/// The straight curves of a sketch a whole-sketch pick takes: its curves
/// that are no construction, no reference geometry and no axis.
fn sketch_curves<K: Kernel>(
    doc: &Document<K>,
    sketch: FeatureUid,
) -> Result<Vec<EntityUid>, ModelError> {
    let entry = doc
        .feature(sketch)
        .ok_or_else(|| refuse("the sketch is gone: pick it again"))?;
    match &entry.def {
        FeatureDef::Sketch(def) => {
            let left_out: BTreeSet<EntityUid> = def
                .entities
                .iter()
                .filter(|e| e.construction || e.reference || e.axis.is_some())
                .map(|e| e.id)
                .collect();
            let output =
                mitcad_model::features::geom_ref::Resolver::sketch(&mut doc.resolver(), sketch)
                    .map_err(refuse)?;
            Ok(output
                .solved
                .curves
                .keys()
                .filter(|c| !left_out.contains(c))
                .copied()
                .collect())
        }
        FeatureDef::SpatialSketch(def) => Ok(def
            .curves
            .iter()
            .filter(|c| !c.construction)
            .map(|c| c.id)
            .collect()),
        _ => Err(refuse(format!(
            "{} is no sketch: pick sketch lines or a sketch",
            entry.name
        ))),
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait LinesCrateOps<K: Kernel> {
    /// The sketch lines `lines` picks, in order, each once; refused in
    /// words when one is not straight.
    fn plan_lines(&self, lines: &AlongLines) -> Result<Vec<Line>, ModelError>;

    /// One member's `build_place` input along a line.
    fn line_input(&self, input: &PlaceInput, lines: &AlongLines, line: &Line) -> PlaceInput;

    /// The plans of the members along the lines, their corners square
    /// (for the preview).
    fn preview_lines(&self, input: &PlaceInput) -> Result<Vec<Plan>, ModelError>;

    /// Each member's `build_place` input along the lines `input.lines`
    /// picks, as [`LinesBuild::line_inputs`].
    fn member_inputs(&self, input: &PlaceInput) -> Result<Vec<PlaceInput>, ModelError>;

    /// Places a member along each line and treats the corners
    /// (`build_place` with `lines`), in one undo step.
    fn build_place_lines(&mut self, input: &PlaceInput) -> Result<Placed, ModelError>;
}

impl<K: Kernel> LinesCrateOps<K> for Document<K> {
    fn plan_lines(&self, lines: &AlongLines) -> Result<Vec<Line>, ModelError> {
        if lines.curves.is_empty() {
            return Err(refuse("pick the sketch lines the members go along"));
        }
        let path = self.path_from_host(&lines.occurrence)?;
        let placement = self.path_transform(&path);
        let mut resolver = self.resolver();
        let mut seen = BTreeSet::new();
        let mut found = Vec::new();
        let mut sketches = Vec::new();
        let mut line_of = |sketch: FeatureUid,
                           curve: EntityUid|
         -> Result<Option<Line>, ModelError> {
            match sketch_line(&mut resolver, sketch, curve).map_err(refuse)? {
                SketchLine::Line { start, end, .. } => Ok(Some(Line {
                    sketch,
                    curve,
                    start: placement.apply_point(start),
                    end: placement.apply_point(end),
                })),
                SketchLine::Curved(what) => Err(refuse(format!(
                    "a picked sketch curve is {what}: members go along straight lines only; leave it out"
                ))),
            }
        };
        for pick in &lines.curves {
            let GeomRef::SketchCurves { sketch, curves } = pick else {
                return Err(refuse(
                    "members go along sketch lines: pick lines of a sketch, or a whole sketch",
                ));
            };
            if !sketches.contains(sketch) {
                sketches.push(*sketch);
            }
            let curves = if curves.is_empty() {
                sketch_curves(self, *sketch)?
            } else {
                curves.clone()
            };
            for curve in curves {
                if seen.insert((*sketch, curve))
                    && let Some(line) = line_of(*sketch, curve)?
                {
                    found.push(line);
                }
            }
        }
        // The chain: the straight lines of those sketches joined end to
        // end to what is found, over and over.
        if lines.chain {
            let mut others = Vec::new();
            for sketch in &sketches {
                for curve in sketch_curves(self, *sketch)? {
                    if seen.contains(&(*sketch, curve)) {
                        continue;
                    }
                    if let Ok(SketchLine::Line { start, end, .. }) =
                        sketch_line(&mut resolver, *sketch, curve)
                    {
                        others.push(Line {
                            sketch: *sketch,
                            curve,
                            start: placement.apply_point(start),
                            end: placement.apply_point(end),
                        });
                    }
                }
            }
            loop {
                let joined = others.iter().position(|o| {
                    found.iter().any(|f| {
                        f.sketch == o.sketch
                            && [f.start, f.end]
                                .iter()
                                .any(|p| near(*p, o.start) || near(*p, o.end))
                    })
                });
                match joined {
                    Some(i) => found.push(others.remove(i)),
                    None => break,
                }
            }
        }
        if found.is_empty() {
            return Err(refuse(
                "the sketch has no straight lines to place members along",
            ));
        }
        if found.iter().any(|l| l.length() <= MEET) {
            return Err(refuse("a picked sketch line has no length"));
        }
        Ok(found)
    }

    fn line_input(&self, input: &PlaceInput, lines: &AlongLines, line: &Line) -> PlaceInput {
        PlaceInput {
            lines: None,
            along: Some(AlongEdge {
                occurrence: lines.occurrence.clone(),
                edge: line.edge(),
                face: None,
                align: lines.align,
                inset: None,
                start: None,
                end: None,
                lift: None,
            }),
            ..input.clone()
        }
    }

    fn preview_lines(&self, input: &PlaceInput) -> Result<Vec<Plan>, ModelError> {
        self.member_inputs(input)?
            .iter()
            .map(|member| self.plan_place(member))
            .collect()
    }

    fn member_inputs(&self, input: &PlaceInput) -> Result<Vec<PlaceInput>, ModelError> {
        let lines = input
            .lines
            .as_ref()
            .ok_or_else(|| refuse("no sketch lines"))?;
        check_lines(input)?;
        Ok(self
            .plan_lines(lines)?
            .iter()
            .map(|line| self.line_input(input, lines, line))
            .collect())
    }

    fn build_place_lines(&mut self, input: &PlaceInput) -> Result<Placed, ModelError> {
        let lines = input
            .lines
            .clone()
            .ok_or_else(|| refuse("no sketch lines"))?;
        check_lines(input)?;
        let found = self.plan_lines(&lines)?;
        // Every member planned before anything changes: a line one cannot
        // go along refuses the whole command.
        for line in &found {
            self.plan_place(&self.line_input(input, &lines, line))?;
        }
        let depth = self.undo_depth();
        let mut placed: Option<Placed> = None;
        let mut members: Vec<(ComponentUid, Line)> = Vec::new();
        let mut label = String::new();
        for line in &found {
            let member = self.line_input(input, &lines, line);
            let along = member.along.clone().expect("along a line");
            let made = match self.build_place_along(&member, &along) {
                Ok(made) => made,
                Err(e) => {
                    // Back to before the command.
                    if self.undo_depth() > depth {
                        self.merge_undo(depth, "Place along sketch lines");
                        let _ = self.undo();
                    }
                    return Err(e);
                }
            };
            if label.is_empty() {
                label = self.undo_label().unwrap_or("Place").to_owned();
            }
            if let Some(c) = made.component {
                members.push((c, line.clone()));
            }
            match &mut placed {
                None => placed = Some(made),
                Some(p) => {
                    p.features.extend(made.features);
                    p.warnings.extend(made.warnings);
                }
            }
        }
        let mut placed = placed.ok_or_else(|| refuse("no member was placed"))?;
        let count = members.len();
        let label = format!("{label} along sketch lines: {count}");
        // The joints' conditions, kept with them (mitcad#2200).
        let lines_placed: Vec<Line> = members.iter().map(|(_, l)| l.clone()).collect();
        let joints = line_joints(&lines_placed, input.flip, lines.corners.unwrap_or_default());
        if !joints.is_empty() {
            let components: Vec<ComponentUid> = members.iter().map(|(c, _)| *c).collect();
            let left = self.treat_line_joints(&joints, &components, &label)?;
            placed.warnings.extend(left);
        }
        self.merge_undo(depth, &label);
        placed.members = members.iter().map(|(c, _)| *c).collect();
        placed.place = "lines";
        Ok(placed)
    }
}

/// The document's public BUILD methods of members along sketch lines.
pub trait LinesBuild<K: Kernel> {
    /// Each member's `build_place` input along the lines `input.lines`
    /// picks (a member along an edge whose edge is the line), in order;
    /// for the preview, whose members' corners are square.
    fn line_inputs(&self, input: &PlaceInput) -> Result<Vec<PlaceInput>, ModelError>;

    /// The joints of the lines `input.lines` picks and the conditions
    /// `corners` gives them (mitcad#2200), members by their index in the
    /// lines' order; for the preview.
    fn line_joints(&self, input: &PlaceInput) -> Result<Vec<LineJoint>, ModelError>;
}

impl<K: Kernel> LinesBuild<K> for Document<K> {
    fn line_inputs(&self, input: &PlaceInput) -> Result<Vec<PlaceInput>, ModelError> {
        self.member_inputs(input)
    }

    fn line_joints(&self, input: &PlaceInput) -> Result<Vec<LineJoint>, ModelError> {
        let lines = input
            .lines
            .as_ref()
            .ok_or_else(|| refuse("no sketch lines"))?;
        let found = self.plan_lines(lines)?;
        Ok(line_joints(
            &found,
            input.flip,
            lines.corners.unwrap_or_default(),
        ))
    }
}

/// Refuses the fields members along sketch lines do not take: the lines
/// give their places and lengths.
pub(crate) fn check_lines(input: &PlaceInput) -> Result<(), ModelError> {
    let refused = [
        ("at", input.at.is_some()),
        ("to", input.to.is_some()),
        ("length", input.length.is_some()),
        ("spacing", input.spacing.is_some()),
        ("mitre", input.mitre.is_some()),
        ("tilt", input.tilt.is_some()),
        ("along", input.along.is_some()),
        ("lie", input.lie.is_some()),
    ];
    match refused.iter().find(|(_, given)| *given) {
        Some((field, _)) => Err(refuse(format!(
            "members along sketch lines take their places and lengths from the lines: leave out `{field}`"
        ))),
        None => Ok(()),
    }
}
