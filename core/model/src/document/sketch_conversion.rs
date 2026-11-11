// SPDX-License-Identifier: MIT
//! Turning a planar sketch into a 3D sketch in place (mitcad#1544): the
//! same feature (id, name, place in the timeline, visibility, support and
//! frame) with the definition `spatial_convert` makes of it, the paths of
//! later features along its curves rewritten to the 3D sketch's typed path
//! (Pipe, Sweep: the same curve ids and direction, mitcad#1523), as one
//! undo step. One-way: Undo takes it back.
//!
//! The features that use the sketch's profile regions (Extrude, Revolve, a
//! Sweep's or Loft's profile, every other region user) keep them: the 3D
//! sketch's closed loops in its sketch plane bound the same regions with
//! the same keys (mitcad#1550). Refused as a whole, nothing converted,
//! naming the features in the way: a later feature that uses the sketch in
//! a form a 3D sketch cannot supply, a feature or parameter that uses a
//! driven dimension's reference parameter (a 3D sketch's driven dimensions
//! have none), and a change that would make an evaluated feature fail.
//! Later sketches' linked projections of its points and curves keep
//! following, since a 3D sketch is a projection source too (mitcad#1543).
//!
//! The other way (mitcad#1550, 3D Sketch unchecked): a 3D sketch whose
//! geometry all lies in its sketch plane becomes a planar sketch in place
//! ([`Document::convert_sketch_to_planar`], `spatial_planar`): the same
//! feature, every id, its in-plane relations and dimensions back as planar
//! ones, 3D relations as their planar equivalents, attachments and
//! reference curves as linked projections, its planar records back in
//! place; the paths of later features along its curves rewritten to the
//! planar form with the same curves, each running the way it ran in 3D
//! (`reversed` where the planar chain of its curves runs the other way),
//! and the profile users keep their regions. Refused naming
//! what lies out of the plane, or the later features in the way.
//!
//! The converted sketch starts from the planar sketch's last solved
//! positions, solved with its support and its projections' sources where
//! they are now; a projected curve whose ends are attached the other way
//! round along its source has them swapped first. It must solve, with the
//! planar sketch's freedom.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use super::{Document, ModelError, invalid};
use crate::FeatureStatus;
use crate::features::path::{
    HeldPath, chain, curve_ends, curve3, for_each_path, reversed, rewrite_spatial_paths, samples,
};
use crate::features::spatial_path;
use crate::features::spatial_sketch::{
    SpatialConstraintKind, SpatialCurve, SpatialCurveKind, SpatialReferenceCurve, SpatialSketchDef,
    SpatialSketchOutput, SpatialSketchStatus, SpatialSource,
};
use crate::features::{CurvePath, FeatureDef, FeatureEntry, PathRef, SketchCurves, SpatialPath};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
// Refusals in words, the uids and ids for agents (mitcad#1644).
use crate::notice::{FeatureError, said};
use crate::profile::SketchFrame;
use crate::sketch::ConstraintUid;
use crate::sketch::solve::Solved;
use crate::sketch::spatial_convert::{spatial_from_planar, swap_ends};
use crate::sketch::spatial_edit::SpatialGoals;
use crate::sketch::spatial_planar::planar_from_spatial;
use crate::sweeps::PathCurve;

/// What a conversion kept and made (see `spatial_convert::Converted`).
#[derive(Debug, Clone, PartialEq)]
pub struct SketchConversion {
    /// The planar constraints and dimensions kept as in-plane ones.
    pub in_plane: Vec<ConstraintUid>,
    /// The planar coincidences of two points kept as 3D coincidences.
    pub coincident: Vec<ConstraintUid>,
    /// In-plane fixes made for the planar sketch's fixed entities.
    pub fixes: Vec<ConstraintUid>,
    /// The On Sketch Plane relation and the points it lists.
    pub on_sketch_plane: Option<(ConstraintUid, Vec<EntityUid>)>,
    /// Attachments made for linked projections.
    pub attachments: Vec<ConstraintUid>,
    /// Projected curves that became reference curves.
    pub references: Vec<EntityUid>,
    /// Points of those curves that nothing else used.
    pub dropped: Vec<EntityUid>,
    /// The later features whose paths were rewritten, with their names.
    pub rewritten: Vec<(FeatureUid, String)>,
    /// The 3D sketch's status after its solve.
    pub status: SpatialSketchStatus,
    /// What the 3D sketch keeps in its planar records (mitcad#1550).
    pub kept: Vec<String>,
}

/// What turning a 3D sketch back into a planar one made (mitcad#1550).
#[derive(Debug, Clone, PartialEq)]
pub struct PlanarConversion {
    /// 3D relations and dimensions the plane holds by itself or that have
    /// no planar form (On Sketch Plane, normals and offsets along z, a
    /// spline's driven length), and the fixes that became fixed entities.
    pub dropped: Vec<ConstraintUid>,
    /// Attachments and reference curves that became linked projections.
    pub projected: Vec<String>,
    /// What changed on the way.
    pub notes: Vec<String>,
    /// The later features whose paths were rewritten, with their names.
    pub rewritten: Vec<(FeatureUid, String)>,
}

/// A refused conversion: the error, and the features in its way (uid,
/// name, why) when they refused it.
#[derive(Debug, Clone, PartialEq)]
pub struct SketchConversionError {
    pub error: Box<ModelError>,
    pub blocking: Vec<(FeatureUid, String, String)>,
}

impl From<ModelError> for SketchConversionError {
    fn from(error: ModelError) -> Self {
        Self {
            error: Box::new(error),
            blocking: Vec::new(),
        }
    }
}

impl fmt::Display for SketchConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for SketchConversionError {}

/// The refusal naming the features in the way; `kind` is `3D` or `2D`.
fn refused(
    name: &str,
    kind: &str,
    blocking: Vec<(FeatureUid, String, String)>,
) -> SketchConversionError {
    SketchConversionError {
        error: Box::new(invalid(cannot_become(name, kind, &blocking))),
        blocking,
    }
}

/// That `name` cannot become a `kind` (`3D` or `2D`) sketch for the
/// features `items` (uid, name, why: a text that follows the name, with
/// its tail where it has one), as text (mitcad#1644): a person reads the
/// first feature by its name and why in words and how many more, agents
/// each with its uid and ids.
fn cannot_become(name: &str, kind: &str, items: &[(FeatureUid, String, String)]) -> String {
    let errors: Vec<FeatureError> = items
        .iter()
        .map(|(_, _, why)| FeatureError::from_text(why))
        .collect();
    let named: Vec<String> = items
        .iter()
        .zip(&errors)
        .map(|((uid, feature, _), error)| format!("{feature} ({uid}) {}", error.agent_text()))
        .collect();
    let detail = format!("{name} cannot become a {kind} sketch: {}", named.join("; "));
    let first = match (items.first(), errors.first()) {
        (Some((_, feature, _)), Some(error)) => format!("{feature}: {}", error.message),
        _ => String::new(),
    };
    // One whole message per kind (mitcad#1644).
    let said = match (kind, items.len()) {
        ("3D", 0 | 1) => format!("{name} cannot become a 3D sketch: {first}"),
        ("3D", n) => format!(
            "{name} cannot become a 3D sketch: {first} (and {} more)",
            n - 1
        ),
        (_, 0 | 1) => format!("{name} cannot become a 2D sketch: {first}"),
        (_, n) => format!(
            "{name} cannot become a 2D sketch: {first} (and {} more)",
            n - 1
        ),
    };
    FeatureError::new(said)
        .with_places(errors.into_iter().flat_map(|e| e.places))
        .with_detail(detail)
        .text()
}

/// The paths of a later feature along the 3D sketch `sketch` in the planar
/// form of a 2D sketch's curves, with the same curves (every path
/// `for_each_path` finds: a Pipe's, a Sweep's path, rails and auxiliary
/// curve, a Loft's centre line and rails, a path pattern's, a construction
/// plane's or point's along a path): `true` when it changed any. Each
/// keeps its direction: `against` tells whether the planar chain of the
/// listed curves would run the other way from the 3D path, and the planar
/// path is then `reversed` (mitcad#1550); `chained` gives the curves a
/// chained 3D path takes, for a path reference that lists its curves.
fn rewrite_planar_paths(
    feature: &mut FeatureDef,
    sketch: FeatureUid,
    against: &dyn Fn(&SpatialPath, &[EntityUid]) -> bool,
    chained: &dyn Fn(&SpatialPath) -> Vec<EntityUid>,
) -> bool {
    let mut changed = false;
    let _ = for_each_path(feature, &mut |_: &str, held: HeldPath<'_>| {
        match held {
            HeldPath::Curves(path) => {
                let spatial = match &*path {
                    CurvePath::Spatial(p) | CurvePath::Path(PathRef::Spatial(p))
                        if p.sketch == sketch =>
                    {
                        p.clone()
                    }
                    _ => return Ok(()),
                };
                *path = CurvePath::Sketch(SketchCurves {
                    sketch,
                    curves: spatial.curves.clone(),
                    chain: spatial.chain,
                    reversed: against(&spatial, &spatial.curves),
                });
            }
            HeldPath::Ref(path) => {
                let spatial = match &*path {
                    PathRef::Spatial(p) if p.sketch == sketch => p.clone(),
                    _ => return Ok(()),
                };
                let curves = if spatial.chain {
                    chained(&spatial)
                } else {
                    spatial.curves.clone()
                };
                let reversed = against(&spatial, &curves);
                *path = match curves.as_slice() {
                    [curve] => PathRef::SketchCurve {
                        sketch,
                        curve: *curve,
                        reversed,
                    },
                    _ => PathRef::SketchCurves {
                        sketch,
                        curves,
                        reversed,
                    },
                };
            }
        }
        changed = true;
        Ok(())
    });
    changed
}

/// Whether the planar path of the curves `listed` runs the other way from
/// the 3D path `path` of the sketch `def` (mitcad#1550): the planar pieces
/// are `listed` first, as a 2D sketch's path lists them, then the other
/// curves of the 3D path's route, chained as a 2D sketch's path chains
/// them from the curves of the planar sketch's `planar` solve (an arc
/// about -z runs counter-clockwise there, with its ends swapped); the
/// first curve of the route is compared, run both ways, in the sketch's
/// local coordinates. None when either is no path.
fn runs_against(
    def: &SpatialSketchDef,
    output: &SpatialSketchOutput,
    planar: &Solved,
    name: &str,
    path: &SpatialPath,
    listed: &[EntityUid],
) -> Option<bool> {
    let legs = spatial_path::route(def, path, name, &|p| output.point(p)).ok()?;
    let leg = legs.first()?;
    let own = output.local_curve(leg.curve)?;
    let spatial_run = if leg.forward { own } else { reversed(own) };
    let mut ids: Vec<EntityUid> = listed.to_vec();
    for l in &legs {
        if !ids.contains(&l.curve) {
            ids.push(l.curve);
        }
    }
    let pieces: Vec<PathCurve> = ids
        .iter()
        .map(|id| {
            Some(PathCurve {
                name: id.curve_name(),
                curve: curve3(&SketchFrame::XY, planar.curves.get(id)?),
            })
        })
        .collect::<Option<_>>()?;
    let chained = chain(pieces).ok()?;
    let planar_run = &chained
        .iter()
        .find(|p| p.name == leg.curve.curve_name())?
        .curve;
    Some(!same_way(&spatial_run, planar_run))
}

/// Whether two runs of one curve in the sketch plane go the same way: an
/// open one from the same end, a closed one round the same way about z.
fn same_way(a: &Curve3, b: &Curve3) -> bool {
    let open = |c: &Curve3| {
        curve_ends(c).filter(|(start, end)| mitcad_vec3::distance(*start, *end) > 1e-9)
    };
    match (open(a), open(b)) {
        (Some((a0, _)), Some((b0, b1))) => {
            mitcad_vec3::distance(a0, b0) <= mitcad_vec3::distance(a0, b1)
        }
        _ => turn_about_z(a) * turn_about_z(b) >= 0.0,
    }
}

/// Twice the area a closed curve in the plane z = 0 bounds, positive when
/// it runs counter-clockwise about z.
fn turn_about_z(curve: &Curve3) -> f64 {
    let points = samples(curve);
    (0..points.len())
        .map(|k| {
            let (p, q) = (points[k], points[(k + 1) % points.len()]);
            p[0] * q[1] - p[1] * q[0]
        })
        .sum()
}

impl<K: Kernel> Document<K> {
    /// Turns the planar sketch `uid` into a 3D sketch in place (see the
    /// module documentation), as one undo step `Convert <name> to 3D
    /// Sketch`.
    pub fn convert_sketch_to_spatial(
        &mut self,
        uid: FeatureUid,
    ) -> Result<SketchConversion, SketchConversionError> {
        let position = self.state.require(uid)?;
        let entry = self.state.features[position].clone();
        self.state.editable(entry.component)?;
        let name = entry.name.clone();
        let planar = match &entry.def {
            FeatureDef::Sketch(def) => def.clone(),
            FeatureDef::SpatialSketch(_) => {
                return Err(invalid(said(
                    format!("{name} is a 3D sketch already"),
                    format!(
                        "{name} ({uid}) is a 3D sketch already (sketch3d.convert_to_2d turns it \
                         back into a 2D sketch)"
                    ),
                ))
                .into());
            }
            _ => {
                return Err(invalid(said(
                    format!("{name} is not a sketch"),
                    format!("{name} ({uid}) is not a sketch"),
                ))
                .into());
            }
        };
        let output = self.sketch_output(uid).cloned().ok_or_else(|| {
            invalid(format!(
                "{name} has no result to convert (it failed, or it is suppressed or after the \
                 timeline marker)"
            ))
        })?;

        // A driven dimension's reference parameter that something uses: a
        // 3D sketch's driven dimensions have none.
        let mut blocking = Vec::new();
        let params = &self.state.parameters;
        for id in planar.dimensions.iter().filter_map(|d| d.reference) {
            let param = params.name(id);
            for other in self.state.features.iter().filter(|f| f.uid != uid) {
                if other.def.params().contains(&id) {
                    blocking.push((
                        other.uid,
                        other.name.clone(),
                        format!(
                            "uses {param}, a driven dimension's reference, which a 3D sketch's \
                             driven dimensions do not have"
                        ),
                    ));
                }
            }
            let dependents = params.table().dependents(id);
            if !dependents.is_empty() {
                let names: Vec<String> = dependents.iter().map(|d| params.name(*d)).collect();
                // One whole message for one and for several (mitcad#1644).
                let why = match names.as_slice() {
                    [one] => format!(
                        "has the driven dimension reference {param}, which {one} uses; a 3D \
                         sketch's driven dimensions have no reference parameters"
                    ),
                    _ => format!(
                        "has the driven dimension reference {param}, which the parameters {} \
                         use; a 3D sketch's driven dimensions have no reference parameters",
                        names.join(", ")
                    ),
                };
                blocking.push((uid, name.clone(), why));
            }
        }

        let converted = spatial_from_planar(&planar, Some(&*output.solved))
            .map_err(|e| invalid(format!("{name} cannot become a 3D sketch: {e}")))?;
        let mut def = converted.def.clone();
        let mut next = self.state.clone();
        let edited = FeatureEntry {
            def: FeatureDef::SpatialSketch(def.clone()),
            ..(*entry).clone()
        };
        next.check_feature(position, &edited)
            .map_err(|e| invalid(format!("{name} cannot become a 3D sketch: {e}")))?;
        next.features[position] = Arc::new(edited);

        // The later features: paths rewritten, the rest checked against
        // the 3D sketch; every one in the way is named.
        let mut rewritten = Vec::new();
        for later in position + 1..next.features.len() {
            let other = next.features[later].clone();
            let mut feature = other.def.clone();
            match rewrite_spatial_paths(&mut feature, uid, &planar, &output) {
                Ok(true) => {
                    next.features[later] = Arc::new(FeatureEntry {
                        def: feature,
                        ..(*other).clone()
                    });
                    rewritten.push((other.uid, other.name.clone()));
                }
                Ok(false) => {}
                Err(e) => {
                    blocking.push((other.uid, other.name.clone(), format!("uses it: {e}")));
                    continue;
                }
            }
            let checked = next.features[later].clone();
            if let Err(e) = next.check_feature(later, &checked) {
                blocking.push((other.uid, other.name.clone(), e));
            }
        }
        if !blocking.is_empty() {
            return Err(refused(&name, "3D", blocking));
        }

        // Projected curves whose ends run the other way along their
        // sources: their attachments' ends swapped, from where the sources'
        // ends are found.
        if !converted.ends.is_empty() {
            let probed = self
                .probe_spatial(&next, position)
                .map_err(|e| SketchConversionError::from(*e.error))?;
            let mut swapped = false;
            for pair in &converted.ends {
                let found = |k: ConstraintUid| -> Option<([f64; 3], [f64; 3])> {
                    let point = def.constraints.iter().find_map(|c| match c.kind {
                        SpatialConstraintKind::Attach { point, .. } if c.id == k => Some(point),
                        _ => None,
                    })?;
                    Some((def.point(point)?.at, *probed.targets.get(&k)?))
                };
                let (Some((a, ta)), Some((b, tb))) = (found(pair[0]), found(pair[1])) else {
                    continue;
                };
                let d = mitcad_vec3::distance;
                if d(a, tb) + d(b, ta) < d(a, ta) + d(b, tb) {
                    swap_ends(&mut def, *pair);
                    swapped = true;
                }
            }
            if swapped {
                next.features[position] = Arc::new(FeatureEntry {
                    def: FeatureDef::SpatialSketch(def.clone()),
                    ..(*entry).clone()
                });
            }
        }

        let solved = self
            .settle_spatial(&mut next, position, def, &SpatialGoals::default())
            .map_err(|e| {
                SketchConversionError::from(invalid(format!(
                    "{name} cannot become a 3D sketch: {}",
                    e.error
                )))
            })?;
        next.release_params(uid);

        // Nothing evaluated may fail that did not.
        let failing: BTreeSet<FeatureUid> = self
            .state
            .features
            .iter()
            .filter(|f| matches!(self.status(f.uid), Some(FeatureStatus::Failed(_))))
            .map(|f| f.uid)
            .collect();
        let report = SketchConversion {
            in_plane: converted.in_plane,
            coincident: converted.coincident,
            fixes: converted.fixes,
            on_sketch_plane: converted.on_sketch_plane,
            attachments: converted.attachments,
            references: converted.references,
            dropped: converted.dropped,
            rewritten,
            status: solved.status,
            kept: converted.kept,
        };
        let label = format!("Convert {name} to 3D Sketch");
        let sketch_name = name.clone();
        self.apply_checked(
            move |state| {
                *state = next;
                Ok((label, report))
            },
            move |state, result| {
                let broken: Vec<(FeatureUid, String, String)> = result
                    .results
                    .iter()
                    .filter(|r| !failing.contains(&r.uid))
                    .filter_map(|r| match &r.status {
                        FeatureStatus::Failed(message) => {
                            let feature = state.entry(r.uid).map_or("", |f| f.name.as_str());
                            Some((r.uid, feature.to_owned(), format!("would fail: {message}")))
                        }
                        _ => None,
                    })
                    .collect();
                if broken.is_empty() {
                    Ok(())
                } else {
                    Err(invalid(cannot_become(&sketch_name, "3D", &broken)))
                }
            },
        )
        .map_err(SketchConversionError::from)
    }

    /// Turns the 3D sketch `uid`, whose geometry all lies in its sketch
    /// plane, into a planar sketch in place (see the module documentation),
    /// as one undo step `Convert <name> to 2D Sketch` (mitcad#1550).
    pub fn convert_sketch_to_planar(
        &mut self,
        uid: FeatureUid,
    ) -> Result<PlanarConversion, SketchConversionError> {
        let position = self.state.require(uid)?;
        let entry = self.state.features[position].clone();
        self.state.editable(entry.component)?;
        let name = entry.name.clone();
        let spatial = match &entry.def {
            FeatureDef::SpatialSketch(def) => def.clone(),
            FeatureDef::Sketch(_) => {
                return Err(invalid(said(
                    format!("{name} is a 2D sketch already"),
                    format!("{name} ({uid}) is a 2D sketch already"),
                ))
                .into());
            }
            _ => {
                return Err(invalid(said(
                    format!("{name} is not a sketch"),
                    format!("{name} ({uid}) is not a sketch"),
                ))
                .into());
            }
        };
        let output = self
            .spatial_sketch(uid)
            .filter(|o| o.is_solved())
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "{name} has no result to convert (it failed, or it is suppressed or after the \
                     timeline marker)"
                ))
            })?;
        let attached = self.attached_curves(&spatial, position)?;
        let flat = planar_from_spatial(&spatial, &output, &attached)
            .map_err(|e| invalid(format!("{name} cannot become a 2D sketch: {e}")))?;
        let notes = flat.notes.clone();
        let mut next = self.state.clone();
        let edited = FeatureEntry {
            def: FeatureDef::Sketch(flat.def.clone()),
            ..(*entry).clone()
        };
        next.check_feature(position, &edited)
            .map_err(|e| invalid(format!("{name} cannot become a 2D sketch: {e}")))?;
        next.features[position] = Arc::new(edited);

        // Whether a path would run the other way as a 2D sketch's: the
        // planar chain of its curves as the 2D sketch has them, against the
        // 3D path's route (mitcad#1550). Such a planar path is `reversed`,
        // so that every path keeps its direction.
        let against = |path: &SpatialPath, listed: &[EntityUid]| -> bool {
            runs_against(&spatial, &output, &flat.solved, &name, path, listed).unwrap_or(false)
        };
        // The curves a chained path takes: those of its first curve's chain.
        let chains = spatial_path::chains(&spatial);
        let chained = |path: &SpatialPath| -> Vec<EntityUid> {
            let first = path.curves.first().copied();
            chains
                .iter()
                .find(|c| c.legs.iter().any(|l| Some(l.curve) == first))
                .map(|c| c.legs.iter().map(|l| l.curve).collect())
                .unwrap_or_else(|| path.curves.clone())
        };
        let mut rewritten = Vec::new();
        let mut blocking = Vec::new();
        for later in position + 1..next.features.len() {
            let other = next.features[later].clone();
            let mut feature = other.def.clone();
            if rewrite_planar_paths(&mut feature, uid, &against, &chained) {
                next.features[later] = Arc::new(FeatureEntry {
                    def: feature,
                    ..(*other).clone()
                });
                rewritten.push((other.uid, other.name.clone()));
            }
            let checked = next.features[later].clone();
            if let Err(e) = next.check_feature(later, &checked) {
                blocking.push((other.uid, other.name.clone(), e));
            }
        }
        if !blocking.is_empty() {
            return Err(refused(&name, "2D", blocking));
        }
        next.release_params(uid);

        let failing: BTreeSet<FeatureUid> = self
            .state
            .features
            .iter()
            .filter(|f| matches!(self.status(f.uid), Some(FeatureStatus::Failed(_))))
            .map(|f| f.uid)
            .collect();
        let report = PlanarConversion {
            dropped: flat.dropped,
            projected: flat.projected,
            notes,
            rewritten,
        };
        let label = format!("Convert {name} to 2D Sketch");
        let sketch_name = name.clone();
        self.apply_checked(
            move |state| {
                *state = next;
                Ok((label, report))
            },
            move |state, result| {
                let broken: Vec<(FeatureUid, String, String)> = result
                    .results
                    .iter()
                    .filter(|r| !failing.contains(&r.uid))
                    .filter_map(|r| match &r.status {
                        FeatureStatus::Failed(message) => {
                            let feature = state.entry(r.uid).map_or("", |f| f.name.as_str());
                            Some((r.uid, feature.to_owned(), format!("would fail: {message}")))
                        }
                        _ => None,
                    })
                    .collect();
                if broken.is_empty() {
                    Ok(())
                } else {
                    Err(invalid(cannot_become(&sketch_name, "2D", &broken)))
                }
            },
        )
        .map_err(SketchConversionError::from)
    }

    /// For each attachment of the 3D sketch at `position` to a curve's
    /// start, end or middle: the whole curve in the sketch's local
    /// coordinates, found by evaluating the sketch with it as a reference
    /// curve (Include) beside the others.
    fn attached_curves(
        &mut self,
        def: &SpatialSketchDef,
        position: usize,
    ) -> Result<BTreeMap<ConstraintUid, SpatialReferenceCurve>, SketchConversionError> {
        let mut probe = def.clone();
        let mut next = probe.next_entity().0;
        let mut wanted = Vec::new();
        for (id, _, source) in def.attachments() {
            if matches!(source, SpatialSource::Origin) || source.at().is_none() {
                continue;
            }
            let mut whole = source.clone();
            if let SpatialSource::Edge { at, .. }
            | SpatialSource::Sketch { at, .. }
            | SpatialSource::Curve { at, .. } = &mut whole
            {
                *at = None;
            }
            let curve = EntityUid(next);
            next += 1;
            probe.curves.push(SpatialCurve::new(
                curve,
                SpatialCurveKind::Reference {
                    source: whole,
                    projected: false,
                },
                true,
            ));
            wanted.push((id, curve));
        }
        if wanted.is_empty() {
            return Ok(BTreeMap::new());
        }
        let mut state = self.state.clone();
        let entry = state.features[position].clone();
        state.features[position] = Arc::new(FeatureEntry {
            def: FeatureDef::SpatialSketch(probe),
            ..(*entry).clone()
        });
        let probed = self
            .probe_spatial(&state, position)
            .map_err(|e| SketchConversionError::from(*e.error))?;
        Ok(wanted
            .into_iter()
            .filter_map(|(id, curve)| Some((id, probed.references.get(&curve)?.clone())))
            .collect())
    }
}
