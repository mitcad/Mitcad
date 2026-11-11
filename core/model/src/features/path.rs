// SPDX-License-Identifier: MIT
//! Paths of the sweep family (F3): what a sweep or a pipe runs along, a
//! sweep's guide rail, a loft's centre line and rails, and the curves of
//! ribs and webs. A path is curves of a sketch (S1's entity ids, the
//! sketch-curves form of [`GeomRef`](super::GeomRef)) or a [`PathRef`]:
//!
//! | JSON | Meaning |
//! |---|---|
//! | `{"sketch": "F1", "curves": ["c1", "c2"]}` | the curves, in any order, joined end to end |
//! | `{"sketch": "F1", "curves": ["c1"], "chain": true}` | also every curve joined to them end to end (chain selection; construction curves only when listed) |
//! | `{"sketch": "F1", "curves": ["c1", "c2"], "reversed": true}` | the same curves, the other way (mitcad#1550) |
//! | `{"sketch": "F1", "curve": "c1"}` | one curve ([`PathRef`]) |
//! | `{"body": "F2.b0", "edges": ["E{…}", …]}` | connected edges of a body ([`PathRef`]) |
//! | `{"start": [x, y, z], "end": [x, y, z]}` | a fixed line ([`PathRef`]) |
//! | `{"feature": "F5"}` | the curve of an Intersection Curve (mitcad#1405, [`PathRef`]) |
//! | `{"spatial_sketch": "F3", "curves": ["c3", "c6"], "reversed": false}` | curves of a 3D sketch (mitcad#1523, [`SpatialPath`]) |
//!
//! Evaluation gives the curves in model space in order along the path,
//! each running along it ([`PathCurve`]). The path starts at the free end
//! of the first listed curve when that curve ends the chain; otherwise it
//! runs along the first listed curve's own direction (a closed chain from
//! that curve's start). `reversed` runs it the other way: a 3D sketch
//! turned back into 2D keeps its paths' direction with it where the chain
//! of their curves would run the other way (mitcad#1550). A path may not
//! branch. A 3D sketch's path is ordered by the sketch's joints and runs
//! along its first curve's own direction, or against it
//! (`spatial_path.rs`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use mitcad_vec3::distance;

// The curves' plain geometry, below the model (mitcad#777).
use super::geom_ref::PathRef;
use super::spatial_path::SpatialPath;
use super::{CheckContext, EvalContext, FeatureDef, References, SketchDef, SketchOutput, is_false};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
use crate::profile::SketchFrame;
use crate::sketch::geometry::Curve2;
use crate::sweeps::PathCurve;
use crate::topo::{EdgeName, TopoName};
use mitcad_model_base::curves::tolerance;
pub(crate) use mitcad_model_base::curves::{
    bspline_point, conic_point, curve_ends, helix_point, is_closed, reversed, reversed_path,
};

const TAU: f64 = std::f64::consts::TAU;

/// Curves of a sketch (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchCurves {
    pub sketch: FeatureUid,
    #[serde(with = "curves_serde")]
    pub curves: Vec<EntityUid>,
    /// Adds the curves joined to them end to end.
    #[serde(default, skip_serializing_if = "is_false")]
    pub chain: bool,
    /// Runs the other way from the chain of the curves (mitcad#1550): a
    /// path only, not the curves as a set.
    #[serde(default, skip_serializing_if = "is_false")]
    pub reversed: bool,
}

/// A path: curves of a sketch, a [`PathRef`] (edges of a body, one
/// sketch curve, a fixed line), or curves of a 3D sketch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CurvePath {
    Sketch(SketchCurves),
    Path(PathRef),
    /// Curves of a 3D sketch (mitcad#1523).
    Spatial(SpatialPath),
}

impl<'de> Deserialize<'de> for CurvePath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = Value::deserialize(deserializer)?;
        if value.get("spatial_sketch").is_some() {
            serde_json::from_value(value)
                .map(Self::Spatial)
                .map_err(D::Error::custom)
        } else if value.get("curves").is_some() || value.get("chain").is_some() {
            serde_json::from_value(value)
                .map(Self::Sketch)
                .map_err(D::Error::custom)
        } else {
            PathRef::from_value(value)
                .map(Self::Path)
                .map_err(D::Error::custom)
        }
    }
}

/// Curve ids in files and commands: `["c1", "c2"]`.
pub(crate) mod curves_serde {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(ids: &[EntityUid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(|id| id.curve_name()))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

/// Curves of a 3D sketch named in a planar sketch's form where that form
/// means a path (mitcad#1527, mitcad#1550): a path takes a 3D sketch's
/// curves in the typed form ([`SpatialPath`], `{"spatial_sketch": ...}`).
/// The inputs that grow from curves in their sketch's plane (thin walls,
/// ribs and webs, curves swept or extruded as a surface) take a 3D
/// sketch's curves in this form ([`SketchCurves::check_in_plane`]). Ok for
/// any other feature, which [`CheckContext::sketch`] then tells.
pub(crate) fn not_spatial(ctx: &CheckContext<'_>, sketch: FeatureUid) -> Result<(), String> {
    match ctx.feature(sketch) {
        Ok(entry) if matches!(entry.def, FeatureDef::SpatialSketch(_)) => Err(format!(
            "{} ({sketch}) is a 3D sketch: as a path its curves are \
             {{\"spatial_sketch\": \"{sketch}\", \"curves\": [...]}}",
            entry.name
        )),
        _ => Ok(()),
    }
}

/// The listed curves of the 3D sketch `def` (named `name`) that the
/// planar output of its sketch plane (`solved`, mitcad#1550) does not
/// have: those that do not lie in its sketch plane, refused naming the
/// sketch and the curves.
fn off_the_plane(
    def: &super::spatial_sketch::SpatialSketchDef,
    curves: &[EntityUid],
    solved: &crate::sketch::solve::Solved,
    name: &str,
) -> Result<(), String> {
    let off: Vec<String> = curves
        .iter()
        .filter(|id| def.curve(**id).is_some() && !solved.curves.contains_key(*id))
        .map(|id| id.curve_name())
        .collect();
    // Whole messages for one curve and several (mitcad#1644): in words for
    // a person, the curves' ids for agents.
    match off.as_slice() {
        [] => Ok(()),
        [one] => Err(crate::notice::said(
            format!(
                "a curve of {name} does not lie in its sketch plane: walls and surfaces grow \
                 from a 3D sketch's curves in its sketch plane"
            ),
            format!(
                "{name}'s {one} does not lie in its sketch plane: walls and surfaces grow from \
                 a 3D sketch's curves in its sketch plane"
            ),
        )),
        _ => Err(crate::notice::said(
            format!(
                "{} curves of {name} do not lie in its sketch plane: walls and surfaces grow \
                 from a 3D sketch's curves in its sketch plane",
                off.len()
            ),
            format!(
                "{name}'s curves {} do not lie in its sketch plane: walls and surfaces grow \
                 from a 3D sketch's curves in its sketch plane",
                off.join(", ")
            ),
        )),
    }
}

impl SketchCurves {
    pub fn add_references(&self, references: &mut References) {
        references.features.insert(self.sketch);
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.curves.is_empty() {
            return Err("no curves selected".to_owned());
        }
        not_spatial(ctx, self.sketch)?;
        let (entry, def) = ctx.sketch(self.sketch)?;
        for (i, curve) in self.curves.iter().enumerate() {
            if self.curves[..i].contains(curve) {
                return Err(format!(
                    "curve {} is listed more than once",
                    curve.curve_name()
                ));
            }
            if !def.entity(*curve).is_some_and(|e| !e.is_point()) {
                return Err(format!(
                    "{} has no curve {}",
                    entry.name,
                    curve.curve_name()
                ));
            }
        }
        Ok(())
    }

    /// [`Self::check`] for the inputs that take curves of a 3D sketch in
    /// this form too (mitcad#1550): curves that lie anywhere in space, as a
    /// set rather than a path (a boundary surface's curves, a surface
    /// loft's sections of curves, an Intersection Curve's chains). Chain
    /// selection in a 3D sketch adds the curves joined to them at the
    /// sketch's joints ([`super::spatial_path`]).
    pub fn check_any(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if let Ok(entry) = ctx.feature(self.sketch)
            && let FeatureDef::SpatialSketch(def) = &entry.def
        {
            return super::spatial_path::check_curves(def, &self.curves, &entry.name);
        }
        self.check(ctx)
    }

    /// [`Self::check`] for the inputs that grow from curves in their
    /// sketch's plane (a rib's or a web's curves, thin walls along curves,
    /// the curves a surface extrusion, revolution or sweep sweeps): a
    /// planar sketch's curves, or a 3D sketch's (mitcad#1550), as its
    /// profiles are taken. A 3D sketch's must lie in its sketch plane when
    /// the feature is evaluated ([`Self::curves_on`] refuses the others,
    /// naming the sketch); its reference curves are refused here.
    pub fn check_in_plane(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if let Ok(entry) = ctx.feature(self.sketch)
            && let FeatureDef::SpatialSketch(def) = &entry.def
        {
            return super::spatial_path::check_curves(def, &self.curves, &entry.name);
        }
        self.check(ctx)
    }

    /// The sketch's frame and the curves in model space, the listed ones
    /// first in their order, then those `chain` adds. A 3D sketch's
    /// ([`Self::check_any`]): its support frame and its curves where it
    /// has them now.
    pub fn curves<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(SketchFrame, Vec<PathCurve>), String> {
        if matches!(
            ctx.env.feature(self.sketch).map(|f| &f.def),
            Some(FeatureDef::SpatialSketch(_))
        ) {
            return super::spatial_path::curve_set(ctx, self.sketch, &self.curves, self.chain);
        }
        self.in_plane(ctx)
    }

    /// The sketch's frame and the curves in its plane in model space, the
    /// listed ones first in their order, then those `chain` adds: what
    /// grows from curves in their sketch's plane takes
    /// ([`Self::check_in_plane`]). A 3D sketch's are its curves in its
    /// sketch plane, in the planar form its profiles have (mitcad#1550).
    pub fn in_plane<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(SketchFrame, Vec<PathCurve>), String> {
        let frame = ctx.sketch(self.sketch)?.frame;
        Ok((frame, self.curves_on(ctx, &frame)?))
    }

    /// [`Self::in_plane`] with the sketch placed by `frame` (its own frame
    /// moved, as a pattern's copy moves it). A 3D sketch's curves out of
    /// its sketch plane are refused, naming the sketch (mitcad#1550).
    pub(crate) fn curves_on<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        frame: &SketchFrame,
    ) -> Result<Vec<PathCurve>, String> {
        let output = ctx.sketch(self.sketch)?;
        let name = ctx.feature_name(self.sketch);
        if let Some(FeatureDef::SpatialSketch(def)) = ctx.env.feature(self.sketch).map(|f| &f.def) {
            off_the_plane(def, &self.curves, &output.solved, &name)?;
        }
        let construction = if self.chain {
            construction_curves(ctx, self.sketch)
        } else {
            BTreeSet::new()
        };
        self.pieces_of(&output.solved, frame, &construction, &name)
    }

    /// The pieces of [`Self::curves_on`] from a solved sketch: the listed
    /// curves first in their order, then those `chain` adds, `construction`
    /// left out; `name` is the sketch's, for messages.
    fn pieces_of(
        &self,
        solved: &crate::sketch::solve::Solved,
        frame: &SketchFrame,
        construction: &BTreeSet<EntityUid>,
        name: &str,
    ) -> Result<Vec<PathCurve>, String> {
        for id in &self.curves {
            if !solved.curves.contains_key(id) {
                return Err(format!("{name} has no curve {}", id.curve_name()));
            }
        }
        let mut ids = self.curves.clone();
        if self.chain {
            let candidates: Vec<(EntityUid, &Curve2)> = solved
                .curves
                .iter()
                .filter(|(id, _)| !construction.contains(id) && !ids.contains(id))
                .map(|(id, c)| (*id, c))
                .collect();
            ids.extend(joined(&ids, solved, &candidates));
        }
        // A curve the same as an earlier one (an edge projected twice,
        // mitcad#741) is one piece: two would branch at its ends.
        let mut size: f64 = 1.0;
        for id in &ids {
            if let Some((a, b)) = solved.curves[id].ends() {
                size = size
                    .max(a[0].abs())
                    .max(a[1].abs())
                    .max(b[0].abs())
                    .max(b[1].abs());
            }
        }
        let mut distinct: Vec<EntityUid> = Vec::new();
        for id in ids {
            if !distinct.iter().any(|k| {
                mitcad_sketch::internal::same_curve(
                    &solved.curves[k],
                    &solved.curves[&id],
                    1e-6 * size,
                )
            }) {
                distinct.push(id);
            }
        }
        let ids = distinct;
        let pieces = ids
            .iter()
            .map(|id| PathCurve {
                name: id.curve_name(),
                curve: curve3(frame, &solved.curves[id]),
            })
            .collect();
        Ok(pieces)
    }
}

/// The construction curves of a sketch, which chained selection skips.
fn construction_curves<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    sketch: FeatureUid,
) -> BTreeSet<EntityUid> {
    ctx.env
        .features
        .iter()
        .find(|f| f.uid == sketch)
        .map(|f| match &f.def {
            FeatureDef::Sketch(def) => def
                .entities
                .iter()
                .filter(|e| e.construction)
                .map(|e| e.id)
                .collect(),
            // A 3D sketch's (mitcad#1550).
            FeatureDef::SpatialSketch(def) => def
                .curves
                .iter()
                .filter(|c| c.construction)
                .map(|c| c.id)
                .collect(),
            _ => BTreeSet::new(),
        })
        .unwrap_or_default()
}

/// The candidates joined to the curves end to end, directly or through
/// other candidates, in the order they are reached.
fn joined(
    seeds: &[EntityUid],
    solved: &crate::sketch::solve::Solved,
    candidates: &[(EntityUid, &Curve2)],
) -> Vec<EntityUid> {
    let mut ends: Vec<[f64; 2]> = Vec::new();
    let mut size: f64 = 1.0;
    for id in seeds {
        if let Some((a, b)) = solved.curves.get(id).and_then(Curve2::ends) {
            ends.extend([a, b]);
        }
    }
    for (_, curve) in candidates {
        if let Some((a, b)) = curve.ends() {
            size = size
                .max(a[0].abs())
                .max(a[1].abs())
                .max(b[0].abs())
                .max(b[1].abs());
        }
    }
    let tolerance = 1e-6 * size;
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) <= tolerance;
    let mut added = Vec::new();
    let mut taken = vec![false; candidates.len()];
    loop {
        let mut grew = false;
        for (i, (id, curve)) in candidates.iter().enumerate() {
            let Some((a, b)) = curve.ends() else {
                continue;
            };
            if taken[i] || !ends.iter().any(|e| near(*e, a) || near(*e, b)) {
                continue;
            }
            taken[i] = true;
            grew = true;
            ends.extend([a, b]);
            added.push(*id);
        }
        if !grew {
            return added;
        }
    }
}

impl CurvePath {
    pub fn add_references(&self, references: &mut References) {
        match self {
            Self::Sketch(curves) => curves.add_references(references),
            Self::Path(path) => path.add_to(references),
            Self::Spatial(path) => path.add_references(references),
        }
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        // A planar path's form naming a 3D sketch (mitcad#1523): the typed
        // form is the one that takes its curves.
        if let Self::Sketch(SketchCurves { sketch, .. })
        | Self::Path(
            PathRef::SketchCurve { sketch, .. } | PathRef::SketchCurves { sketch, .. },
        ) = self
            && let Ok(entry) = ctx.feature(*sketch)
            && matches!(entry.def, FeatureDef::SpatialSketch(_))
        {
            return Err(format!(
                "{} ({sketch}) is a 3D sketch: its curves are a path as {{\"spatial_sketch\": \
                 \"{sketch}\", \"curves\": [...]}}",
                entry.name
            ));
        }
        match self {
            Self::Sketch(curves) => curves.check(ctx),
            Self::Path(path) => path.check(ctx),
            Self::Spatial(path) => path.check(ctx),
        }
    }

    /// The 3D sketch this path runs along (mitcad#1523), if any.
    pub fn spatial(&self) -> Option<&SpatialPath> {
        match self {
            Self::Spatial(path) | Self::Path(PathRef::Spatial(path)) => Some(path),
            _ => None,
        }
    }

    /// The path's curves in order, each running along it.
    pub fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<PathCurve>, String> {
        // A sketch's path that runs the other way from the chain of its
        // curves (mitcad#1550).
        let backwards = match self {
            Self::Sketch(curves) => curves.reversed,
            Self::Path(
                PathRef::SketchCurve { reversed, .. } | PathRef::SketchCurves { reversed, .. },
            ) => *reversed,
            _ => false,
        };
        let pieces = match self {
            // In order along it already, by the sketch's joints.
            Self::Spatial(path) => return path.resolve(ctx),
            Self::Sketch(curves) => curves.curves(ctx)?.1,
            Self::Path(PathRef::Edges { body, edges }) => edge_curves(ctx, *body, edges)?,
            Self::Path(PathRef::SketchCurve { sketch, curve, .. }) => {
                let curves = SketchCurves {
                    sketch: *sketch,
                    curves: vec![*curve],
                    chain: false,
                    reversed: false,
                };
                curves.curves(ctx)?.1
            }
            Self::Path(PathRef::SketchCurves { sketch, curves, .. }) => {
                let curves = SketchCurves {
                    sketch: *sketch,
                    curves: curves.clone(),
                    chain: false,
                    reversed: false,
                };
                curves.curves(ctx)?.1
            }
            Self::Path(PathRef::Line { start, end }) => vec![PathCurve {
                name: "line".to_owned(),
                curve: Curve3::Line {
                    start: *start,
                    end: *end,
                },
            }],
            Self::Path(PathRef::Helix(helix)) => vec![PathCurve {
                name: "helix".to_owned(),
                curve: helix.curve()?,
            }],
            // Fixed curves (mitcad#548), named by their place in the list.
            Self::Path(PathRef::Fixed(curves)) => curves
                .iter()
                .enumerate()
                .map(|(i, c)| PathCurve {
                    name: format!("fixed{}", i + 1),
                    curve: c.curve(),
                })
                .collect(),
            // An Intersection Curve's (mitcad#1405): in order along it already.
            Self::Path(PathRef::FeatureCurve(feature)) => {
                return Ok(ctx.feature_curve(*feature)?.as_ref().clone());
            }
            Self::Path(PathRef::Spatial(path)) => return path.resolve(ctx),
        };
        let path = chain(pieces)?;
        Ok(if backwards { reversed_path(path) } else { path })
    }
}

/// What turning `sketch` into a 3D sketch in place (mitcad#1544) makes of
/// a path along its curves (mitcad#1523): the typed path of the 3D sketch
/// with the same curve ids and `chain`, running the way the planar path
/// ran through its first listed curve (found from the planar sketch's
/// `def` and last `output`). None when the path does not run along curves
/// of `sketch`; an error when its curves do not make a path. A closed path
/// stays closed (mitcad#1550): a circle or an ellipse runs along its own
/// direction, counter-clockwise about the sketch's normal, as it did.
///
/// A 3D sketch joins curves at shared and coincident points only; a
/// planar chain joined by position alone joins no more once converted.
pub fn spatial_rewrite(
    path: &CurvePath,
    sketch: FeatureUid,
    def: &SketchDef,
    output: &SketchOutput,
) -> Option<Result<SpatialPath, String>> {
    let curves = match path {
        CurvePath::Sketch(curves) if curves.sketch == sketch => curves.clone(),
        CurvePath::Path(PathRef::SketchCurve {
            sketch: s,
            curve,
            reversed,
        }) if *s == sketch => SketchCurves {
            sketch,
            curves: vec![*curve],
            chain: false,
            reversed: *reversed,
        },
        CurvePath::Path(PathRef::SketchCurves {
            sketch: s,
            curves,
            reversed,
        }) if *s == sketch => SketchCurves {
            sketch,
            curves: curves.clone(),
            chain: false,
            reversed: *reversed,
        },
        _ => return None,
    };
    Some(rewritten(&curves, def, output))
}

/// [`spatial_rewrite`] of a planar path along curves of the sketch.
fn rewritten(
    curves: &SketchCurves,
    def: &SketchDef,
    output: &SketchOutput,
) -> Result<SpatialPath, String> {
    let solved = &output.solved;
    let construction: BTreeSet<EntityUid> = def
        .entities
        .iter()
        .filter(|e| e.construction)
        .map(|e| e.id)
        .collect();
    let name = curves.sketch.to_string();
    let pieces = curves.pieces_of(solved, &output.frame, &construction, &name)?;
    let path = chain(pieces)?;
    let first = curves.curves[0];
    let along = path
        .iter()
        .find(|p| p.name == first.curve_name())
        .ok_or_else(|| {
            crate::notice::said(
                "the first curve is not on the path",
                format!("{} is not on the path", first.curve_name()),
            )
        })?;
    let own = curve_ends(&curve3(&output.frame, &solved.curves[&first]));
    let run = curve_ends(&along.curve);
    let against = match (own, run) {
        // A closed curve (a circle, an ellipse, a closed spline) runs its
        // own way.
        (None, _) => false,
        (Some((own_start, own_end)), Some((start, _))) => {
            distance(start, own_end) < distance(start, own_start)
        }
        (Some(_), None) => {
            return Err(crate::notice::said(
                "the first curve has no ends",
                format!("{} has no ends", first.curve_name()),
            ));
        }
    };
    // A planar path that runs the other way (mitcad#1550) turns too.
    Ok(SpatialPath {
        sketch: curves.sketch,
        curves: curves.curves.clone(),
        reversed: against != curves.reversed,
        chain: curves.chain,
    })
}

/// A path a feature holds, as [`for_each_path`] hands it over.
pub(crate) enum HeldPath<'a> {
    Curves(&'a mut CurvePath),
    Ref(&'a mut PathRef),
}

/// Every path a feature holds, with the input's name: a Pipe's path; a
/// Sweep's path, guide rails and auxiliary curve; a loft's centre line,
/// rails and auxiliary curve; a path pattern's path; a construction
/// plane's or point's path (mitcad#1550).
pub(crate) fn for_each_path(
    feature: &mut FeatureDef,
    f: &mut dyn FnMut(&str, HeldPath<'_>) -> Result<(), String>,
) -> Result<(), String> {
    use super::construction::{PlaneDefinition, PointDefinition};
    use super::sweep::SweepFrame;
    match feature {
        FeatureDef::Pipe(pipe) => f("path", HeldPath::Curves(&mut pipe.path))?,
        FeatureDef::Sweep(sweep) => {
            f("path", HeldPath::Curves(&mut sweep.path))?;
            if let Some(rail) = &mut sweep.guide_rail {
                f("guide rail", HeldPath::Curves(rail))?;
            }
            if let Some(rail) = &mut sweep.guide_rail2 {
                f("second guide rail", HeldPath::Curves(rail))?;
            }
            if let Some(SweepFrame::Auxiliary { curve, .. }) = &mut sweep.frame {
                f("auxiliary curve", HeldPath::Curves(curve))?;
            }
        }
        FeatureDef::Loft(loft) => {
            if let Some(line) = &mut loft.centerline {
                f("center line", HeldPath::Curves(line))?;
            }
            for (i, rail) in loft.rails.iter_mut().enumerate() {
                f(&format!("rail {}", i + 1), HeldPath::Curves(rail))?;
            }
            if let Some(SweepFrame::Auxiliary { curve, .. }) = &mut loft.frame {
                f("auxiliary curve", HeldPath::Curves(curve))?;
            }
        }
        FeatureDef::PathPattern(pattern) => f("path", HeldPath::Ref(&mut pattern.path))?,
        FeatureDef::ConstructionPlane(plane) => match &mut plane.definition {
            PlaneDefinition::AlongPath { path, .. }
            | PlaneDefinition::NormalAtPoint { path, .. } => f("path", HeldPath::Ref(path))?,
            _ => {}
        },
        FeatureDef::ConstructionPoint(point) => {
            if let PointDefinition::AlongPath { path, .. } = &mut point.definition {
                f("path", HeldPath::Ref(path))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// [`spatial_rewrite`] for the paths of a feature along curves of
/// `sketch` (mitcad#1544, mitcad#1550: every path [`for_each_path`]
/// finds). `Ok(true)` when it changed any; an error, naming the input, for
/// a path that cannot be a 3D sketch's. Other features' uses of the sketch
/// are not looked at: the curves that grow from the sketch's plane (a
/// Sweep's curves to sweep, a rib's, thin walls) keep their planar form,
/// which a 3D sketch's curves in its sketch plane have too (mitcad#1550).
pub fn rewrite_spatial_paths(
    feature: &mut FeatureDef,
    sketch: FeatureUid,
    def: &SketchDef,
    output: &SketchOutput,
) -> Result<bool, String> {
    let mut changed = false;
    for_each_path(feature, &mut |what: &str, held: HeldPath<'_>| {
        let planar = match &held {
            HeldPath::Curves(path) => (**path).clone(),
            HeldPath::Ref(path) => CurvePath::Path((**path).clone()),
        };
        let Some(rewritten) = spatial_rewrite(&planar, sketch, def, output) else {
            return Ok(());
        };
        let rewritten = rewritten.map_err(|e| format!("{what}: {e}"))?;
        match held {
            HeldPath::Curves(path) => *path = CurvePath::Spatial(rewritten),
            HeldPath::Ref(path) => *path = PathRef::Spatial(rewritten),
        }
        changed = true;
        Ok(())
    })?;
    Ok(changed)
}

/// The paths of a feature through fillets of the 3D sketch `sketch`
/// (mitcad#1548): a path of a 3D sketch's curves ([`for_each_path`]) that
/// lists both lines a fillet rounded (`rounded`: the lines and the arc)
/// takes the arc too, so that it still runs through the corner; a chain
/// selection finds it by itself. True when any changed.
pub fn take_fillet_arcs(
    feature: &mut FeatureDef,
    sketch: FeatureUid,
    rounded: &[([EntityUid; 2], EntityUid)],
) -> bool {
    let mut changed = false;
    let _ = for_each_path(feature, &mut |_: &str, held: HeldPath<'_>| {
        let path = match held {
            HeldPath::Curves(CurvePath::Spatial(path))
            | HeldPath::Curves(CurvePath::Path(PathRef::Spatial(path)))
            | HeldPath::Ref(PathRef::Spatial(path)) => path,
            _ => return Ok(()),
        };
        if path.sketch == sketch && !path.chain {
            for ([a, b], arc) in rounded {
                if path.curves.contains(a) && path.curves.contains(b) && !path.curves.contains(arc)
                {
                    path.curves.push(*arc);
                    changed = true;
                }
            }
        }
        Ok(())
    });
    changed
}

/// The curves of a body's edges, in the order listed.
fn edge_curves<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    edges: &[EdgeName],
) -> Result<Vec<PathCurve>, String> {
    let shape = ctx.body(body)?;
    edge_curves_of(ctx, body, &shape, edges)
}

/// A path of edges of `shape`, the shape of `body` as the caller reads it
/// (a body of another component moved into the feature's coordinates,
/// mitcad#161), joined as [`CurvePath::resolve`] joins them.
pub(crate) fn resolve_edges_on<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    edges: &[EdgeName],
) -> Result<Vec<PathCurve>, String> {
    chain(edge_curves_of(ctx, body, shape, edges)?)
}

/// [`edge_curves`] of a given shape of the body.
fn edge_curves_of<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    edges: &[EdgeName],
) -> Result<Vec<PathCurve>, String> {
    let mut pieces = Vec::new();
    for edge in edges {
        // An edge a sketch change renamed is found again (mitcad#272); the
        // piece keeps the stored name.
        let found = super::renamed::edge_for(ctx, shape, edge).map_err(|e| format!("path: {e}"))?;
        let curves = ctx
            .kernel
            .curves_of(shape, &TopoName::Edge(found))
            .map_err(|e| format!("path: {e}"))?;
        let mut curves = curves.into_iter();
        match (curves.next(), curves.next()) {
            (None, _) => return Err(format!("path: {}", ctx.missing_edge_on(body, shape, edge))),
            (Some(curve), None) => pieces.push(PathCurve {
                name: edge.to_string(),
                curve,
            }),
            (Some(_), Some(_)) => {
                return Err(format!(
                    "path: {edge} names several edges; name one with #k"
                ));
            }
        }
    }
    Ok(pieces)
}

/// A sketch curve in model space.
pub(crate) fn curve3(frame: &SketchFrame, curve: &Curve2) -> Curve3 {
    let normal = frame.normal();
    let direction = |angle: f64| -> [f64; 3] {
        std::array::from_fn(|i| angle.cos() * frame.x_axis[i] + angle.sin() * frame.y_axis[i])
    };
    let after = |start: f64, end: f64| if end <= start { end + TAU } else { end };
    let conic = |center, x_axis, major, minor, start, end, closed| Curve3::Conic {
        center: frame.point(center),
        normal,
        x_axis,
        major,
        minor,
        start,
        end,
        closed,
    };
    match curve {
        Curve2::Line { a, b } => Curve3::Line {
            start: frame.point(*a),
            end: frame.point(*b),
        },
        Curve2::Circle {
            center,
            radius,
            start,
        } => conic(
            *center,
            frame.x_axis,
            *radius,
            *radius,
            *start,
            start + TAU,
            true,
        ),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => conic(
            *center,
            frame.x_axis,
            *radius,
            *radius,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Ellipse {
            center,
            major,
            minor,
            rotation,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            0.0,
            TAU,
            true,
        ),
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            rotation,
            start,
            end,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Nurbs(n) => Curve3::BSpline {
            degree: n.degree as u32,
            poles: n.control.iter().map(|p| frame.point(*p)).collect(),
            weights: n.weights.clone(),
            knots: n.knots.clone(),
        },
    }
}

/// Orders the curves into one chain along the path (see the module
/// documentation for its direction).
pub(crate) fn chain(pieces: Vec<PathCurve>) -> Result<Vec<PathCurve>, String> {
    if pieces.is_empty() {
        return Err("the path has no curves".to_owned());
    }
    let ends: Vec<Option<([f64; 3], [f64; 3])>> =
        pieces.iter().map(|p| curve_ends(&p.curve)).collect();
    if let Some(i) = ends.iter().position(Option::is_none) {
        if pieces.len() == 1 {
            return Ok(pieces);
        }
        return Err(format!(
            "{} is a closed curve; it can only be a path by itself",
            pieces[i].name
        ));
    }
    let tolerance = tolerance(&pieces);
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut node_of = |p: [f64; 3]| -> usize {
        if let Some(i) = nodes.iter().position(|q| distance(*q, p) <= tolerance) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let links: Vec<(usize, usize)> = ends
        .iter()
        .map(|e| {
            let (a, b) = e.expect("open");
            (node_of(a), node_of(b))
        })
        .collect();
    let mut degree = vec![0usize; nodes.len()];
    for (i, (a, b)) in links.iter().enumerate() {
        if a == b {
            return Err(format!("{} closes on itself", pieces[i].name));
        }
        degree[*a] += 1;
        degree[*b] += 1;
    }
    if let Some(node) = degree.iter().position(|d| *d > 2) {
        let p = nodes[node];
        return Err(format!(
            "the path branches at ({:.6}, {:.6}, {:.6})",
            p[0], p[1], p[2]
        ));
    }
    let free = |node: usize| degree[node] == 1;
    let (a0, b0) = links[0];
    let start = if pieces.len() > 1 && free(a0) {
        a0
    } else if pieces.len() > 1 && free(b0) {
        b0
    } else if degree.contains(&1) {
        // The first curve lies inside the chain: back from its start to the
        // free end.
        let mut used = vec![false; pieces.len()];
        used[0] = true;
        let mut node = a0;
        while let Some(i) =
            (0..links.len()).find(|i| !used[*i] && (links[*i].0 == node || links[*i].1 == node))
        {
            used[i] = true;
            node = if links[i].0 == node {
                links[i].1
            } else {
                links[i].0
            };
        }
        node
    } else {
        a0
    };
    let mut used = vec![false; pieces.len()];
    let mut order: Vec<(usize, bool)> = Vec::new();
    let mut node = start;
    loop {
        let incident = |i: usize| !used[i] && (links[i].0 == node || links[i].1 == node);
        let next = if incident(0) && (links[0].0 == node || order.is_empty()) {
            Some(0)
        } else {
            (0..links.len()).find(|i| incident(*i))
        };
        let Some(i) = next else {
            break;
        };
        used[i] = true;
        let forward = links[i].0 == node;
        node = if forward { links[i].1 } else { links[i].0 };
        order.push((i, forward));
    }
    if let Some(i) = used.iter().position(|u| !u) {
        // In plain words, an edge painted; the curve's id or the edge's
        // name for agents (mitcad#1612).
        let name = &pieces[i].name;
        let mut error = crate::notice::FeatureError::new(
            "the path's curves are not connected: a curve does not join the others",
        )
        .with_detail(format!(
            "the path's curves are not connected: {name} does not join the others"
        ));
        if let Ok(crate::topo::TopoName::Edge(edge)) = name.parse::<crate::topo::TopoName>() {
            error = error.at(crate::notice::NoticePlace::edge(edge.to_string()));
        }
        return Err(error.text());
    }
    let mut pieces: Vec<Option<PathCurve>> = pieces.into_iter().map(Some).collect();
    Ok(order
        .into_iter()
        .map(|(i, forward)| {
            let piece = pieces[i].take().expect("each piece once");
            if forward {
                piece
            } else {
                PathCurve {
                    name: piece.name,
                    curve: reversed(piece.curve),
                }
            }
        })
        .collect())
}

/// The curves of thin walls (mitcad#463) as chains: runs of curves joined
/// end to end through points where exactly two of them meet, each in order
/// and running along it ([`chain`]), in the order of their first listed
/// curves (`pieces` in the order listed). A closed chain runs
/// counter-clockwise about the sketch normal; an open one so that it bends
/// to the left (with its chord it encloses an area counter-clockwise), a
/// straight one the way the first listed of its curves was drawn.
pub(crate) fn wall_chains(
    pieces: Vec<PathCurve>,
    frame: &SketchFrame,
) -> Result<Vec<Vec<PathCurve>>, String> {
    if pieces.is_empty() {
        return Err("no curves selected".to_owned());
    }
    let tolerance = tolerance(&pieces);
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut node_of = |p: [f64; 3]| -> usize {
        if let Some(i) = nodes.iter().position(|q| distance(*q, p) <= tolerance) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    // The end nodes of each open curve; None for a closed one.
    let links: Vec<Option<(usize, usize)>> = pieces
        .iter()
        .map(|p| curve_ends(&p.curve).map(|(a, b)| (node_of(a), node_of(b))))
        .map(|l| l.filter(|(a, b)| a != b))
        .collect();
    let mut degree = vec![0usize; nodes.len()];
    for (a, b) in links.iter().flatten() {
        degree[*a] += 1;
        degree[*b] += 1;
    }
    let mut used: Vec<bool> = links.iter().map(Option::is_none).collect();
    let mut runs: Vec<Vec<usize>> = (0..pieces.len())
        .filter(|i| links[*i].is_none())
        .map(|i| vec![i])
        .collect();
    // From piece `i` away from `node`, on through points where two meet.
    let walk = |mut i: usize, mut node: usize, used: &mut Vec<bool>| -> Vec<usize> {
        let mut run = Vec::new();
        loop {
            used[i] = true;
            run.push(i);
            let (a, b) = links[i].expect("open");
            node = if a == node { b } else { a };
            if degree[node] != 2 {
                return run;
            }
            let next = (0..links.len())
                .find(|j| !used[*j] && links[*j].is_some_and(|(a, b)| a == node || b == node));
            match next {
                Some(j) => i = j,
                None => return run,
            }
        }
    };
    for (node, d) in degree.iter().enumerate() {
        if *d == 2 {
            continue;
        }
        while let Some(i) = (0..links.len())
            .find(|i| !used[*i] && links[*i].is_some_and(|(a, b)| a == node || b == node))
        {
            runs.push(walk(i, node, &mut used));
        }
    }
    // What is left are closed loops of several curves.
    while let Some(i) = used.iter().position(|u| !u) {
        let start = links[i].expect("open").0;
        runs.push(walk(i, start, &mut used));
    }
    for run in &mut runs {
        run.sort_unstable();
    }
    runs.sort_unstable_by_key(|run| run[0]);
    runs.into_iter()
        .map(|run| {
            let first = pieces[run[0]].clone();
            let path = chain(run.iter().map(|i| pieces[*i].clone()).collect())?;
            Ok(counter_clockwise(path, &first, frame))
        })
        .collect()
}

/// The chain run counter-clockwise or bending to the left (see
/// [`wall_chains`]); `first` is its first listed curve as drawn.
fn counter_clockwise(
    path: Vec<PathCurve>,
    first: &PathCurve,
    frame: &SketchFrame,
) -> Vec<PathCurve> {
    let normal = frame.normal();
    let mut points: Vec<[f64; 3]> = Vec::new();
    for piece in &path {
        points.extend(samples(&piece.curve));
    }
    let length: f64 = points.windows(2).map(|w| distance(w[0], w[1])).sum();
    // Twice the area the points and the chord back to the first enclose,
    // positive counter-clockwise about the normal.
    let mut area = 0.0;
    for (k, p) in points.iter().enumerate() {
        let q = points[(k + 1) % points.len()];
        area += mitcad_vec3::dot(mitcad_vec3::cross(*p, q), normal);
    }
    let backwards = if area.abs() > 1e-9 * length * length {
        area < 0.0
    } else {
        // Straight: as the first listed curve was drawn.
        let drawn = curve_ends(&first.curve);
        let run = path
            .iter()
            .find(|p| p.name == first.name)
            .and_then(|p| curve_ends(&p.curve));
        matches!((drawn, run), (Some((a, _)), Some((b, _))) if distance(a, b) > 1e-9 * length.max(1.0))
    };
    if backwards { reversed_path(path) } else { path }
}

/// Points along the curve from its start to its end.
pub(crate) fn samples(curve: &Curve3) -> Vec<[f64; 3]> {
    const N: usize = 32;
    let along = |f: &dyn Fn(f64) -> [f64; 3], t0: f64, t1: f64| -> Vec<[f64; 3]> {
        (0..=N)
            .map(|k| f(t0 + (t1 - t0) * k as f64 / N as f64))
            .collect()
    };
    match curve {
        Curve3::Point(p) => vec![*p],
        Curve3::Line { start, end } => vec![*start, *end],
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            ..
        } => along(
            &|t| conic_point(*center, *normal, *x_axis, (*major, *minor), t),
            *start,
            *end,
        ),
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return poles.clone();
            }
            along(
                &|t| bspline_point(p, poles, weights, knots, t),
                knots[p],
                knots[poles.len()],
            )
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => along(
            &|t| helix_point(*center, *axis, *x_axis, (*radius, *pitch), t),
            *start,
            *end,
        ),
    }
}

/// The plane through the first point of `curves` square to the path at
/// its start (mitcad#548: where a surface sweep of fixed curves, which lie
/// in no plane of their own, starts along the path).
pub(crate) fn plane_at_start(
    path: &[PathCurve],
    curves: &[PathCurve],
) -> Result<SketchFrame, String> {
    let along = path.first().map(|p| samples(&p.curve)).unwrap_or_default();
    let tangent = along
        .windows(2)
        .find_map(|w| {
            let d = [w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]];
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            (length > 1e-12).then(|| [d[0] / length, d[1] / length, d[2] / length])
        })
        .ok_or("the path has no direction at its start")?;
    let origin = curves
        .first()
        .and_then(|c| samples(&c.curve).first().copied())
        .ok_or("no curves to sweep")?;
    // Any direction square to the tangent as the plane's x axis.
    let other = if tangent[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let x = mitcad_vec3::cross(tangent, other);
    let length = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let x = [x[0] / length, x[1] / length, x[2] / length];
    Ok(SketchFrame {
        origin,
        x_axis: x,
        y_axis: mitcad_vec3::cross(tangent, x),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, a: [f64; 3], b: [f64; 3]) -> PathCurve {
        PathCurve {
            name: name.to_owned(),
            curve: Curve3::Line { start: a, end: b },
        }
    }

    fn names(path: &[PathCurve]) -> Vec<String> {
        path.iter()
            .map(|p| {
                let Some((a, b)) = curve_ends(&p.curve) else {
                    return p.name.clone();
                };
                format!("{}:{:?}->{:?}", p.name, a, b)
            })
            .collect()
    }

    #[test]
    fn chains_start_at_the_free_end_of_the_first_curve() {
        let c1 = line("c1", [0.0; 3], [40.0, 0.0, 0.0]);
        let c2 = line("c2", [60.0, 20.0, 0.0], [40.0, 0.0, 0.0]);
        let c3 = line("c3", [60.0, 20.0, 0.0], [60.0, 60.0, 0.0]);
        let path = chain(vec![c1.clone(), c3.clone(), c2.clone()]).unwrap();
        assert_eq!(
            names(&path),
            [
                "c1:[0.0, 0.0, 0.0]->[40.0, 0.0, 0.0]",
                "c2:[40.0, 0.0, 0.0]->[60.0, 20.0, 0.0]",
                "c3:[60.0, 20.0, 0.0]->[60.0, 60.0, 0.0]"
            ]
        );
        // The last curve first: from its free end backwards.
        let path = chain(vec![c3.clone(), c1.clone(), c2.clone()]).unwrap();
        assert_eq!(path[0].name, "c3");
        assert_eq!(curve_ends(&path[0].curve).unwrap().0, [60.0, 60.0, 0.0]);
        // A curve inside the chain: along its own direction (c2 runs from
        // (60, 20) to (40, 0), so the path starts at c3's free end).
        let path = chain(vec![c2, c1, c3]).unwrap();
        assert_eq!(
            path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["c3", "c2", "c1"]
        );
        assert_eq!(curve_ends(&path[2].curve).unwrap().1, [0.0; 3]);
    }

    #[test]
    fn branches_gaps_and_closed_curves_are_errors() {
        let a = line("c1", [0.0; 3], [10.0, 0.0, 0.0]);
        let b = line("c2", [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
        let c = line("c3", [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]);
        let far = line("c4", [50.0, 0.0, 0.0], [60.0, 0.0, 0.0]);
        let error = chain(vec![a.clone(), b.clone(), c]).unwrap_err();
        assert!(error.contains("branches at (10.000000"), "{error}");
        let error = chain(vec![a.clone(), far]).unwrap_err();
        assert!(error.contains("c4 does not join the others"), "{error}");
        let circle = PathCurve {
            name: "c5".to_owned(),
            curve: Curve3::Conic {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 5.0,
                minor: 5.0,
                start: 0.0,
                end: TAU,
                closed: true,
            },
        };
        assert!(is_closed(&chain(vec![circle.clone()]).unwrap()));
        let error = chain(vec![a, circle]).unwrap_err();
        assert!(error.contains("c5 is a closed curve"), "{error}");
    }

    #[test]
    fn a_closed_chain_runs_along_its_first_curve() {
        let square = [
            line("c1", [0.0; 3], [10.0, 0.0, 0.0]),
            line("c2", [10.0, 10.0, 0.0], [10.0, 0.0, 0.0]),
            line("c3", [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]),
            line("c4", [0.0, 10.0, 0.0], [0.0; 3]),
        ];
        let path = chain(square.to_vec()).unwrap();
        assert!(is_closed(&path));
        assert_eq!(
            path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["c1", "c2", "c3", "c4"]
        );
        assert_eq!(curve_ends(&path[1].curve).unwrap().1, [10.0, 10.0, 0.0]);
    }

    #[test]
    fn reversed_curves_run_between_the_same_points() {
        let arc = Curve3::Conic {
            center: [1.0, 2.0, 3.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 2.0,
            start: 0.3,
            end: 2.0,
            closed: false,
        };
        let spline = Curve3::BSpline {
            degree: 2,
            poles: vec![[0.0; 3], [1.0, 2.0, 0.0], [3.0, 2.0, 1.0], [4.0, 0.0, 0.0]],
            weights: vec![1.0, 2.0, 1.0, 1.0],
            knots: vec![0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0],
        };
        let helix = Curve3::Helix {
            center: [1.0, 2.0, 3.0],
            axis: [0.0, 0.0, 1.0],
            x_axis: [0.0, 1.0, 0.0],
            radius: 4.0,
            pitch: -3.0,
            start: -1.0,
            end: 9.0,
        };
        for curve in [arc, spline, helix] {
            let (a, b) = curve_ends(&curve).unwrap();
            let (c, d) = curve_ends(&reversed(curve.clone())).unwrap();
            assert!(
                distance(a, d) < 1e-12 && distance(b, c) < 1e-12,
                "{curve:?}"
            );
        }
        let ends = curve_ends(&Curve3::BSpline {
            degree: 1,
            poles: vec![[0.0; 3], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.5, 1.0, 1.0],
        })
        .unwrap();
        assert_eq!(ends, ([0.0; 3], [4.0, 0.0, 0.0]));
    }

    #[test]
    fn a_helix_path_runs_from_its_start_angle_to_its_end_angle() {
        use super::super::geom_ref::HelixPath;
        // A quarter turn more than one, rising 8 a turn about z through
        // (1, 2, 3), from the x axis (made square to the axis).
        let mut helix = HelixPath {
            center: [1.0, 2.0, 3.0],
            axis: [0.0, 0.0, 2.0],
            x_axis: [1.0, 0.0, 0.5],
            radius: 5.0,
            pitch: 8.0,
            start_angle: 0.0,
            end_angle: 1.25 * TAU,
        };
        let close = |a: [f64; 3], b: [f64; 3]| distance(a, b) < 1e-9;
        let (a, b) = curve_ends(&helix.curve().unwrap()).unwrap();
        assert!(close(a, [6.0, 2.0, 3.0]), "{a:?}");
        assert!(close(b, [1.0, 7.0, 13.0]), "{b:?}");
        // The other way round: the same helix, from its end angle back.
        std::mem::swap(&mut helix.start_angle, &mut helix.end_angle);
        let curve = helix.curve().unwrap();
        let Curve3::Helix { start, end, .. } = curve else {
            panic!("{curve:?}")
        };
        assert!(start < end);
        let (c, d) = curve_ends(&curve).unwrap();
        assert!(close(c, b) && close(d, a), "{c:?} {d:?}");
        // Left-handed: the same turn, below its axis' origin at 2.5π.
        helix.pitch = -8.0;
        let (_, d) = curve_ends(&helix.curve().unwrap()).unwrap();
        assert!(close(d, [6.0, 2.0, 3.0]), "{d:?}");
        let (c, _) = curve_ends(&helix.curve().unwrap()).unwrap();
        assert!(close(c, [1.0, 7.0, -7.0]), "{c:?}");
        // No pitch, no radius, an x axis along the axis: errors.
        for (pitch, radius, x) in [
            (0.0, 5.0, [1.0, 0.0, 0.0]),
            (8.0, 0.0, [1.0, 0.0, 0.0]),
            (8.0, 5.0, [0.0, 0.0, 1.0]),
        ] {
            let bad = HelixPath {
                pitch,
                radius,
                x_axis: x,
                ..helix.clone()
            };
            assert!(bad.check().is_err(), "{bad:?}");
        }
    }
}
