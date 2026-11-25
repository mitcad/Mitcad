// SPDX-License-Identifier: MIT
//! Sketch: entities, constraints and dimensions on a plane (see
//! [`crate::sketch`] and `docs/architecture.md`). Evaluation finds the
//! plane's frame, follows linked projections, solves from the stored
//! positions with the dimension parameters' values and cuts the profile
//! regions, which extrudes and other profile features use by key.
//!
//! The definition itself is in the sketch crate below the model
//! (`mitcad_sketch::def`, mitcad#780), re-exported here; what needs the
//! document (the references, the checks against the timeline, the
//! evaluation) stays here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub use mitcad_sketch::def::*;

use super::construction::datum_kind;
use super::geom_ref::GeomRef;
use super::{CheckContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::{DatumKind, DatumPlane};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Curve3, Kernel, KernelError};
use crate::links::OccurrenceLink;
use crate::parameters::{ParamId, Parameters};
use crate::profile::SketchFrame;
use crate::sketch::driven::DrivenUses;
use crate::sketch::solve::{SolveError, Solved};
use crate::sketch::{ConstraintUid, Projection, ProjectionSource, SketchEntityRef, project};
use crate::topo::{CurveId, FaceName, TopoName};
use crate::transform::Transform;

impl FeatureInfo for SketchDef {
    fn references(&self) -> References {
        let mut references = References::default();
        match &self.plane {
            SketchPlane::Face { face, body } | SketchPlane::Developed { face, body, .. } => {
                references.features.extend(face.features());
                if let Some(body) = body {
                    references.body(*body);
                }
            }
            SketchPlane::Construction(uid) => {
                references.features.insert(*uid);
            }
            SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
        }
        for projection in &self.projections {
            references
                .features
                .extend(topo_features(&projection.source));
            if let Some(body) = projection.body {
                references.body(body);
            }
            // The earlier feature whose state it reads (mitcad#998).
            references.features.extend(projection.after);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.check_definition()?;
        match &self.plane {
            SketchPlane::Face { face, body } | SketchPlane::Developed { face, body, .. } => {
                for feature in face.features() {
                    ctx.feature(feature)
                        .map_err(|e| format!("plane {face}: {e}"))?;
                }
                if let Some(body) = body {
                    ctx.body(*body)?;
                }
            }
            SketchPlane::Construction(uid) => {
                let entry = ctx.feature(*uid)?;
                if datum_kind(&entry.def) != Some(DatumKind::Plane) {
                    return Err(format!(
                        "{} ({uid}) is not a construction plane",
                        entry.name
                    ));
                }
            }
            SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
        }
        for projection in &self.projections {
            // The source's name is shown as its kind (mitcad#1650).
            for feature in topo_features(&projection.source) {
                ctx.feature(feature)
                    .map_err(|e| format!("projection of {topo}: {e}", topo = projection.source))?;
            }
            // Another sketch's curve or point (mitcad#904): of a sketch
            // before this one, planar or 3D (mitcad#1543).
            if let ProjectionSource::Sketch(entity) = &projection.source {
                let entry = ctx
                    .feature(entity.sketch)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
                if !matches!(
                    entry.def,
                    super::FeatureDef::Sketch(_) | super::FeatureDef::SpatialSketch(_)
                ) {
                    return Err(format!(
                        "projection of {}: {} ({}) is not a sketch",
                        projection.source, entry.name, entry.uid
                    ));
                }
            }
            // An Intersection Curve's curve (mitcad#1405).
            if let ProjectionSource::Curve(feature) = &projection.source {
                let entry = ctx
                    .feature(*feature)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
                if !entry.def.makes_curve() {
                    return Err(format!(
                        "projection of {}: {} makes no curve",
                        projection.source, entry.name
                    ));
                }
            }
            if let Some(body) = projection.body {
                ctx.body(body)?;
            }
            if let Some(after) = projection.after {
                ctx.feature(after)
                    .map_err(|e| format!("projection of {topo}: {e}", topo = projection.source))?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        SketchDef::check_values(self, value)
    }
}

/// The features a sketch names through links to other components
/// (mitcad#100): they need the core's references (mitcad#780).
pub(crate) trait LinkedReferences {
    /// The features the sketch names through links to other components
    /// (mitcad#100), each with its link.
    fn linked_references(&self) -> Vec<(FeatureUid, &OccurrenceLink)>;
}

impl LinkedReferences for SketchDef {
    fn linked_references(&self) -> Vec<(FeatureUid, &OccurrenceLink)> {
        let mut out = Vec::new();
        if let Some(link) = &self.plane_link {
            let mut references = References::default();
            match &self.plane {
                SketchPlane::Face { face, body } | SketchPlane::Developed { face, body, .. } => {
                    references.features.extend(face.features());
                    if let Some(body) = body {
                        references.body(*body);
                    }
                }
                SketchPlane::Construction(uid) => {
                    references.features.insert(*uid);
                }
                SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
            }
            out.extend(references.features.into_iter().map(|uid| (uid, link)));
        }
        for projection in &self.projections {
            if let Some(link) = &projection.link {
                let mut features = topo_features(&projection.source);
                if let Some(body) = projection.body {
                    features.insert(body.feature);
                }
                features.extend(projection.after);
                out.extend(features.into_iter().map(|uid| (uid, link)));
            }
        }
        out
    }
}

/// The driving dimensions of `def` whose values use reference parameters of
/// its own driven dimensions (mitcad#1116), directly or through other
/// parameters' expressions, with those driven dimensions; so do its
/// patterns whose spacings or angle use them (mitcad#2057), keyed by the
/// pattern's id.
pub(crate) fn driven_uses(def: &SketchDef, params: &Parameters) -> DrivenUses {
    let own: BTreeMap<ParamId, ConstraintUid> = def
        .dimensions
        .iter()
        .filter_map(|d| Some((d.reference?, d.id)))
        .collect();
    let mut uses = DrivenUses::new();
    if own.is_empty() {
        return uses;
    }
    let table = params.table();
    // The driven dimensions whose references `values` use.
    let used = |values: &[ParamId]| -> BTreeSet<ConstraintUid> {
        let mut found = BTreeSet::new();
        let mut seen: BTreeSet<ParamId> = values.iter().copied().collect();
        let mut todo = values.to_vec();
        while let Some(p) = todo.pop() {
            if let Some(driven) = own.get(&p) {
                found.insert(*driven);
            }
            for dependency in table.get(p).map(|p| p.dependencies()).unwrap_or_default() {
                if seen.insert(*dependency) {
                    todo.push(*dependency);
                }
            }
        }
        found
    };
    for d in &def.dimensions {
        let Some(value) = d.value else {
            continue;
        };
        let found = used(&[value]);
        if !found.is_empty() {
            uses.insert(d.id, found);
        }
    }
    for p in &def.patterns {
        let values: Vec<ParamId> = p.values().into_iter().copied().collect();
        let found = used(&values);
        if !found.is_empty() {
            uses.insert(p.id, found);
        }
    }
    uses
}

/// Solves a sketch whose driving dimensions or patterns use its own driven
/// ones (`uses`, mitcad#1116, mitcad#2057) in stages
/// ([`crate::sketch::driven`]): each stage
/// leaves out the users whose references are not known yet, and evaluates
/// the parameters with the references known so far at their measured
/// values. `stored` is the definition, `followed` the one whose linked
/// projections moved, if any. The reads recorded are those of the last,
/// whole solve: the users' values as the measured references give them,
/// which the recompute then sets into the references, so that the next
/// recompute finds the same values and keeps the result.
fn solve_using_driven<K: Kernel>(
    stored: &SketchDef,
    followed: Option<&SketchDef>,
    uses: &DrivenUses,
    ctx: &mut super::EvalContext<'_, K>,
    name: &dyn Fn(ParamId) -> String,
) -> Result<Solved, String> {
    let params = ctx.env.params;
    let def = followed.unwrap_or(stored);
    let references: BTreeMap<ConstraintUid, ParamId> = def
        .dimensions
        .iter()
        .filter_map(|d| Some((d.id, d.reference?)))
        .collect();
    let mut reads: Vec<(ParamId, Option<u64>)> = Vec::new();
    let solved = crate::sketch::driven::solve_staged(def, uses, name, &mut |held, known| {
        let local;
        let values: &Parameters = if known.is_empty() {
            params
        } else {
            let mut copy = params.clone();
            for (d, value) in known {
                if let Some(id) = references.get(d) {
                    copy.set_measured(*id, *value)
                        .map_err(|e| SolveError::new(e.to_string()))?;
                }
            }
            local = copy;
            &local
        };
        reads.clear();
        let mut value = |id: ParamId| -> Result<f64, String> {
            reads.push((id, values.read_bits(id)));
            // A text parameter (mitcad#664) gives no number.
            if values.text(id).is_some() {
                return Err(format!(
                    "parameter {} is a text, not a number",
                    values.name(id)
                ));
            }
            values
                .value(id)
                .ok_or_else(|| format!("parameter {} does not exist", values.name(id)))
        };
        let held_stored = stored.holding(held);
        match followed {
            None => held_stored.solve_with(&mut value, name),
            Some(followed) => crate::sketch::project::solve_followed(
                &held_stored,
                &followed.holding(held),
                &mut value,
                name,
            ),
        }
    });
    for (id, bits) in reads {
        ctx.reads.push(crate::recompute::Read::Param(id, bits));
    }
    solved.map_err(|e| e.message)
}

/// The definition with its offsets made again for this evaluation, and its
/// solution, when an offset's curves that vanish at its distance changed
/// outside the sketch: a parameter set elsewhere, a linked projection that
/// followed its source (mitcad#1234,
/// [`crate::sketch::offset::retrim_for_evaluation`]). What the edit would
/// have said becomes the feature's warnings (a constraint on a curve that
/// vanished) and information (the curves that vanish). `solved` is the
/// evaluation's solve of `def` when it solved.
fn retrim<K: Kernel>(
    def: &SketchDef,
    solved: Option<&Solved>,
    ctx: &mut super::EvalContext<'_, K>,
) -> Option<(SketchDef, Solved)> {
    if def.offsets.is_empty() {
        return None;
    }
    if let Some(solved) = solved {
        let mut value = |id: ParamId| ctx.param(id).ok();
        if !crate::sketch::offset::stale_in(def, solved, &mut value) {
            return None;
        }
    }
    let params = ctx.env.params;
    let made = crate::sketch::offset::retrim_for_evaluation(def, params, ctx.uid, solved)?;
    for warning in made.warnings {
        ctx.warn(warning);
    }
    for note in made.notes {
        ctx.inform(note);
    }
    Some((made.def, made.solved))
}

/// The features a projection's source names: those of its topological
/// name, or the sketch whose curve or point it is (mitcad#904).
pub(crate) fn topo_features(source: &ProjectionSource) -> BTreeSet<FeatureUid> {
    match source {
        ProjectionSource::Topo(TopoName::Face(f)) => f.features(),
        ProjectionSource::Topo(TopoName::Edge(e)) => e.features(),
        ProjectionSource::Topo(TopoName::Vertex(v)) => {
            v.faces().iter().flat_map(FaceName::features).collect()
        }
        ProjectionSource::Sketch(entity) => BTreeSet::from([entity.sketch]),
        ProjectionSource::Curve(feature) => BTreeSet::from([*feature]),
    }
}

impl<K: Kernel> Evaluate<K> for SketchDef {
    fn evaluate(
        &self,
        ctx: &mut super::EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        if self.equations.is_empty() {
            return EvaluatePlaced::<K>::evaluate_placed(self, ctx);
        }
        // Equation curves (mitcad#2237): their points where the formulas
        // put them with the parameters now, before everything else; the
        // parameters they name are read, so that a change of one evaluates
        // the sketch again.
        let mut placed = self.clone();
        let warnings =
            crate::sketch::equation::follow_formulas(&mut placed, ctx.env.params.table());
        for name in crate::sketch::equation::used_parameters(self) {
            if let Some(id) = ctx.env.params.find(&name) {
                // A text parameter gives no number; the formula says so.
                let _ = ctx.param(id);
            }
        }
        for warning in warnings {
            ctx.warn(warning);
        }
        EvaluatePlaced::<K>::evaluate_placed(&placed, ctx)
    }
}

/// The sketch's evaluation once its equation curves are placed
/// (mitcad#2237).
trait EvaluatePlaced<K: Kernel> {
    fn evaluate_placed(
        &self,
        ctx: &mut super::EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String>;
}

impl<K: Kernel> EvaluatePlaced<K> for SketchDef {
    fn evaluate_placed(
        &self,
        ctx: &mut super::EvalContext<'_, K>,
    ) -> Result<FeatureOutput<K::Shape>, String> {
        // A sketch on a cylindrical or conical face (mitcad#1768): the
        // face's development, found again at every evaluation; its carrier
        // plane is the sketch's plane.
        let development = if self.plane.is_developed() {
            Some(super::developed::development(&self.plane, ctx)?)
        } else {
            None
        };
        let plane = match (&self.plane_link, &self.plane, self.plane.origin_frame()) {
            _ if development.is_some() => development.as_ref().expect("developed").frame,
            (Some(link), _, _) => linked_frame(&self.plane, link, ctx)?,
            (None, _, Some(frame)) => frame,
            (None, SketchPlane::Construction(uid), None) => ctx.datum_plane(*uid)?.frame(),
            (None, _, None) => face_frame(&self.plane, ctx)?,
        };
        let base = self.place(&plane);
        // Its place on the plane (mitcad#787).
        let frame = match &self.placement {
            Some(placement) => placement.resolve(&mut |id| ctx.param(id))?.apply(&base),
            None => base,
        };
        let crate::sketch::project::Followed {
            def: followed,
            reports: follow,
        } = crate::sketch::project::follow_links_with(self, &frame, &mut |projection| {
            let mut curves = source_curves(projection, &frame, ctx)?;
            // A source edge that is gone: the edges that may have taken its
            // place (mitcad#1784).
            let mut successors = if curves.is_empty() {
                successor_curves(projection, ctx)
            } else {
                Vec::new()
            };
            // On a curved face, the curves' development on the carrier
            // plane, which the planar projection then takes as they are
            // (mitcad#1768).
            if let Some(development) = &development {
                curves = curves
                    .iter()
                    .filter_map(|c| development.flattened(c))
                    .collect();
                for successor in &mut successors {
                    successor.curves = successor
                        .curves
                        .iter()
                        .filter_map(|c| development.flattened(c))
                        .collect();
                }
            }
            Ok(project::Found { curves, successors })
        })?;
        // Linked projections that could not follow say so (mitcad#232);
        // one that followed an edge in its source's place tells it
        // (mitcad#1784).
        for report in &follow {
            let Some(projection) = self.projections.get(report.projection) else {
                continue;
            };
            if let Some(warning) = report.warning(projection) {
                ctx.warn(warning);
            }
            if let Some(info) = report.info(projection) {
                ctx.inform(info);
            }
        }
        let def = followed.as_ref().unwrap_or(self);
        // Every parameter's own name for the errors (mitcad#1314: a driven
        // dimension's reference and an offset's distance too, not only the
        // driving dimensions' values).
        let params = ctx.env.params;
        let name = |id: ParamId| params.name(id);
        // Driving dimensions and patterns that use the sketch's own driven
        // ones are solved after them (mitcad#1116, mitcad#2057).
        let uses = driven_uses(def, ctx.env.params);
        let solving = if uses.is_empty() {
            match &followed {
                None => def.solve_with(&mut |id| ctx.param(id), &name),
                Some(followed) => crate::sketch::project::solve_followed(
                    self,
                    followed,
                    &mut |id| ctx.param(id),
                    &name,
                ),
            }
            .map_err(|e| e.message)
        } else {
            solve_using_driven(self, followed.as_ref(), &uses, ctx, &name)
        };
        // Offsets whose curves that vanish at their distances changed with
        // a parameter set outside the sketch or a projection that followed
        // its source are made again for this evaluation (mitcad#1234).
        let (retrimmed, solved) = match retrim(def, solving.as_ref().ok(), ctx) {
            Some((retrimmed, solved)) => (Some(retrimmed), solved),
            None => (None, solving?),
        };
        // A line or round curve the constraints collapsed leaves no region
        // (mitcad#365).
        if let Some(warning) = solved.status.degenerate_warning() {
            ctx.warn(warning);
        }
        // A line distance between lines that are not parallel holds the
        // middle of one of them only (mitcad#1774).
        if let Some(warning) = solved.status.not_parallel_warning() {
            ctx.warn(warning);
        }
        // Constraints whose geometry an edit of a block removed
        // (mitcad#2110), where that geometry was.
        let lost: Vec<crate::notice::NoticePlace> = self
            .blocks
            .iter()
            .flat_map(|b| &b.lost)
            .map(|l| crate::notice::NoticePlace::Point {
                at: frame.point(l.at),
            })
            .collect();
        match lost.len() {
            0 => {}
            1 => ctx.warn_at(
                "a constraint lost its geometry when its block was edited: the block no longer \
                 has the curve or point it held",
                lost,
            ),
            n => ctx.warn_at(
                format!(
                    "{n} constraints lost their geometry when their block was edited: the block \
                     no longer has the curves or points they held"
                ),
                lost,
            ),
        }
        // Where linked projections and what hangs on them are now (the
        // definition with its offsets made again holds them where they are).
        let moved = match (&followed, &retrimmed) {
            (None, _) | (_, Some(_)) => Vec::new(),
            (Some(followed), None) => {
                let mut solution = followed.clone();
                solved.store(&mut solution.entities);
                crate::sketch::project::moved_entities(self, &solution)
            }
        };
        let def = retrimmed.as_ref().unwrap_or(def);
        // Texts that follow text parameters (mitcad#664) take their values.
        let valued;
        let def = if def.text_values.is_empty() {
            def
        } else {
            valued = def.with_parameter_texts(&mut |id| ctx.param_text(id))?;
            &valued
        };
        let texts = def.text_outlines(&solved, ctx.kernel)?;
        // A font this computer lacks is replaced (P9: with a warning).
        for outline in texts.iter().filter(|t| t.fallback) {
            if let Some(text) = def.texts.iter().find(|t| t.id == outline.id) {
                ctx.warn(format!(
                    "text t{}: the font {} is not installed; {} is used",
                    text.id.0, text.font, outline.family
                ));
            }
        }
        // A glyph outline that cannot bound a profile is told, the letter
        // its place (mitcad#1993).
        for dropped in texts.iter().flat_map(|t| &t.dropped) {
            ctx.warn_at(
                format!(
                    "an outline of the letter {} of a text is not closed, so it bounds no profile",
                    dropped.character
                ),
                vec![letter_place(&frame, dropped.low, dropped.high)],
            );
        }
        let boundary = face_boundary(self, &plane, &frame, ctx)?;
        let curves = def.region_curves_of(&solved, &texts);
        // The face's edges and the outline the sketch got when it was made
        // do not split an area of the user's curves (mitcad#2197).
        let (own_curves, mut automatic) = def.split_automatic(curves.clone());
        automatic.extend(boundary);
        let (region_info, layout, own) = if automatic.is_empty() {
            let (region_info, layout) = crate::sketch::regions::regions_and_layout(&curves);
            (region_info, layout, OwnRegions::default())
        } else {
            let found = crate::sketch::regions::own_area_regions(&own_curves, &automatic);
            // The sketch's own regions too, for references to a region the
            // face's edges cut (mitcad#1182), and the pieces the automatic
            // boundaries cut before (mitcad#2197).
            (
                found.regions,
                found.layout,
                OwnRegions::new(curves).with_earlier(found.earlier),
            )
        };
        // The definition shown, with the stored one it was made from.
        let retrimmed = retrimmed.map(|def| {
            let from = self.clone();
            Arc::new(RetrimmedDef { from, def })
        });
        Ok(FeatureOutput {
            sketch: Some(SketchOutput {
                frame,
                base,
                regions: region_info.iter().map(|r| r.profile.clone()).collect(),
                region_info,
                layout: Arc::new(layout),
                solved: Arc::new(solved),
                texts: Arc::new(texts),
                moved: Arc::new(moved),
                follow: Arc::new(follow),
                own: Arc::new(own),
                retrimmed,
                developed: development.map(Arc::new),
            }),
            ..FeatureOutput::default()
        })
    }
}

/// The place of a letter's outline in the sketch (`low` and `high` its
/// corners in the sketch's coordinates): a box about it in the component's
/// coordinates, a little larger than the outline so that it has a size
/// across the sketch plane too.
fn letter_place(frame: &SketchFrame, low: [f64; 2], high: [f64; 2]) -> crate::notice::NoticePlace {
    let corners = [
        frame.point(low),
        frame.point([high[0], low[1]]),
        frame.point(high),
        frame.point([low[0], high[1]]),
    ];
    let margin = 0.05 * (high[0] - low[0]).max(high[1] - low[1]).max(0.2);
    let min = std::array::from_fn(|i| {
        corners.iter().map(|c| c[i]).fold(f64::INFINITY, f64::min) - margin
    });
    let max = std::array::from_fn(|i| {
        corners
            .iter()
            .map(|c| c[i])
            .fold(f64::NEG_INFINITY, f64::max)
            + margin
    });
    crate::notice::NoticePlace::Box { min, max }
}

/// Implicit profile boundaries of the supporting face. They follow the
/// body at every evaluation and never enter the stored sketch or solve.
pub(crate) fn face_boundary<K: Kernel>(
    def: &SketchDef,
    plane: &SketchFrame,
    frame: &SketchFrame,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<Vec<crate::sketch::regions::RegionCurve>, String> {
    let SketchPlane::Face { face, body } = &def.plane else {
        return Ok(Vec::new());
    };
    let normal = plane.normal();
    let displacement = mitcad_vec3::sub(frame.origin, plane.origin);
    if mitcad_vec3::dot(displacement, normal).abs() > 1e-6
        || mitcad_vec3::norm(mitcad_vec3::cross(frame.normal(), normal)) > 1e-9
    {
        return Ok(Vec::new());
    }
    let (component, transform) = match &def.plane_link {
        Some(link) => {
            let (component, transform) = ctx.linked(link)?;
            (component, Some(transform))
        }
        None => (ctx.component(), None),
    };
    let shapes = match body {
        Some(body) => vec![(*body, ctx.body_in(component, *body)?)],
        None => ctx.bodies_in(component),
    };
    // The face the sketch's plane found (its frame told what it took).
    let SketchFace {
        shape,
        found: renamed,
        ..
    } = sketch_face(ctx.kernel, face, shapes)?;
    let boundary = match ctx
        .kernel
        .face_boundary(&shape, renamed.as_ref().unwrap_or(face))
    {
        Ok(boundary) => boundary,
        // Test kernels and older kernel implementations can still solve
        // face sketches even when they do not provide boundary geometry.
        Err(KernelError::Unsupported(_)) => return Ok(Vec::new()),
        Err(error)
            if {
                let message = error.to_string();
                message.contains("unnamed edge") || message.contains("ambiguous profile boundary")
            } =>
        {
            ctx.warn(format!("face-bounded profiles unavailable: {error}"));
            return Ok(Vec::new());
        }
        Err(error) => return Err(format!("the sketch face boundary: {error}")),
    };
    let mut curves = Vec::new();
    for (edge, curve) in boundary {
        let curve = match &transform {
            Some(transform) => project::transform_curve(transform, &curve),
            None => curve,
        };
        for projected in project::project(&[curve], frame) {
            if let project::Projected::Curve(curve) = projected {
                curves.push(crate::sketch::regions::RegionCurve::new(
                    CurveId::FaceEdge(Box::new(edge.clone())),
                    curve,
                ));
            }
        }
    }
    Ok(curves)
}

/// The curves Project takes of a body element seen along `direction` (the
/// sketch's normal in the shape's coordinates), with whether they are a
/// face's outline: a face's outline (mitcad#1265, [`Kernel::face_outline`]:
/// its boundary edges without the seams off the outline, and its
/// silhouettes); an edge's or a vertex's curves, and a face's boundary
/// edges from a kernel without outlines, as [`Kernel::curves_of`] lists
/// them.
pub(crate) fn projected_curves<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    name: &TopoName,
    direction: [f64; 3],
) -> Result<(Vec<Curve3>, bool), KernelError> {
    if let TopoName::Face(face) = name {
        match kernel.face_outline(shape, face, direction) {
            Ok(curves) => return Ok((curves, true)),
            Err(KernelError::Unsupported(_)) => {}
            Err(error) => return Err(error),
        }
    }
    Ok((kernel.curves_of(shape, name)?, false))
}

/// The model curves of a projection's source, in the sketch's component's
/// coordinates (moved there from another component's by its link,
/// mitcad#100): its curves (a face's outline seen along the sketch's
/// normal for a projection with [`Projection::outline`], mitcad#1265), or
/// for section curves where the sketch's plane (`frame`) cuts it
/// (mitcad#389).
pub(crate) fn source_curves<K: Kernel>(
    projection: &Projection,
    frame: &SketchFrame,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<Vec<Curve3>, String> {
    let source = match &projection.source {
        ProjectionSource::Topo(name) => name,
        ProjectionSource::Sketch(entity) => {
            let curves = sketch_entity_curves(entity, projection.link.as_ref(), ctx)?;
            if !projection.section {
                return Ok(curves);
            }
            // Where the sketch's plane crosses the curve (mitcad#998).
            let normal = frame.normal();
            return Ok(curves
                .iter()
                .flat_map(|c| project::plane_crossings(c, frame.origin, normal))
                .map(Curve3::Point)
                .collect());
        }
        // An Intersection Curve's curve (mitcad#1405), or where the
        // sketch's plane crosses it; another component's through the
        // projection's link (mitcad#1417).
        ProjectionSource::Curve(feature) => {
            let curve = ctx
                .linked_feature_curve(*feature, projection.link.as_ref())
                .map_err(|e| format!("projection of {}: {e}", projection.source))?;
            let curves = curve.iter().map(|p| p.curve.clone());
            if !projection.section {
                return Ok(curves.collect());
            }
            let normal = frame.normal();
            return Ok(curves
                .flat_map(|c| project::plane_crossings(&c, frame.origin, normal))
                .map(Curve3::Point)
                .collect());
        }
    };
    let what = if projection.section {
        "section of"
    } else {
        "projection of"
    };
    let (component, moved) = match &projection.link {
        Some(link) => {
            let (component, t) = ctx
                .linked(link)
                .map_err(|e| format!("{what} {source}: {e}"))?;
            (component, Some(t))
        }
        None => (ctx.component(), None),
    };
    // The sketch's plane where the source is.
    let plane = projection.section.then(|| {
        let plane = DatumPlane {
            origin: frame.origin,
            x_axis: frame.x_axis,
            y_axis: frame.y_axis,
        };
        match &moved {
            Some(t) => project::section_plane(&crate::assembly::inverse(t), &plane),
            None => plane,
        }
    });
    // The sketch's normal where the source is: a face's outline as it is
    // seen along it (mitcad#1265).
    let direction = match &moved {
        Some(t) => crate::assembly::inverse(t).apply_vector(frame.normal()),
        None => frame.normal(),
    };
    let outline = projection.outline;
    let shapes = match (projection.body, projection.after) {
        // As an earlier feature left the body (mitcad#998).
        // A body no feature up to it set: a lost source.
        (Some(body), Some(after)) => match ctx.body_after(after, body) {
            Some(shape) => vec![(body, shape)],
            None => return Ok(Vec::new()),
        },
        (Some(body), None) => vec![(body, ctx.body_in(component, body)?)],
        (None, _) => ctx.bodies_in(component),
    };
    let curves_of = |kernel: &K, shape: &K::Shape, name: &TopoName| match &plane {
        Some(plane) => match name {
            TopoName::Face(face) => kernel
                .section_curves(shape, Some(face), plane)
                .map(|found| found.into_iter().map(|c| c.curve).collect())
                .map_err(|e| format!("{what} {source}: {e}")),
            // Where the plane crosses an edge: points (mitcad#736).
            TopoName::Edge(edge) => kernel
                .section_points(shape, edge, plane)
                .map(|found| found.into_iter().map(Curve3::Point).collect())
                .map_err(|e| format!("{what} {source}: {e}")),
            TopoName::Vertex(_) => Err(format!(
                "{what} {source}: a section's source is a face or an edge"
            )),
        },
        None if outline => projected_curves(kernel, shape, name, direction)
            .map(|(curves, _)| curves)
            .map_err(|e| format!("{what} {source}: {e}")),
        None => kernel
            .curves_of(shape, name)
            .map_err(|e| format!("{what} {source}: {e}")),
    };
    for (_, shape) in shapes {
        let mut curves = curves_of(ctx.kernel, &shape, source)?;
        // A source a sketch change renamed is found again (mitcad#272; a
        // section of a face the plane misses is empty too, and the face is
        // then found as it is named).
        if curves.is_empty()
            && let Ok(Some((found, what, maker))) =
                super::renamed::find_name(ctx.kernel, &shape, source)
        {
            curves = curves_of(ctx.kernel, &shape, &found)?;
            if !curves.is_empty() {
                ctx.found_renamed(what, source, &found, maker);
            }
        }
        // The edges it continues into, joined with it into one curve; a
        // piece gone or no longer joining loses the whole curve.
        if !curves.is_empty() && !projection.chain.is_empty() {
            let mut pieces = std::mem::take(&mut curves);
            for name in &projection.chain {
                pieces.extend(curves_of(ctx.kernel, &shape, name)?);
            }
            if pieces.len() == projection.chain.len() + 1
                && let Some(joined) = project::join_curves(&pieces)
            {
                curves = vec![joined];
            }
        }
        if !curves.is_empty() {
            return Ok(match moved {
                Some(t) => curves
                    .iter()
                    .map(|c| project::transform_curve(&t, c))
                    .collect(),
                None => curves,
            });
        }
    }
    Ok(Vec::new())
}

/// The edges that may have taken the place of a linked projection's
/// source edge that is gone (mitcad#1784): those between the faces it lay
/// between, as their names are now ([`super::projection_successors`]), each
/// with its curves in the sketch's component's coordinates. None for
/// another source, a section, a chain or an outline, and where the body or
/// its link is not there (the source's own lookup says why).
pub(crate) fn successor_curves<K: Kernel>(
    projection: &Projection,
    ctx: &mut super::EvalContext<'_, K>,
) -> Vec<crate::sketch::successor::Successor> {
    let ProjectionSource::Topo(TopoName::Edge(edge)) = &projection.source else {
        return Vec::new();
    };
    if projection.section || projection.outline || !projection.chain.is_empty() {
        return Vec::new();
    }
    let (component, moved) = match &projection.link {
        Some(link) => match ctx.linked(link) {
            Ok((component, t)) => (component, Some(t)),
            Err(_) => return Vec::new(),
        },
        None => (ctx.component(), None),
    };
    let shapes = match (projection.body, projection.after) {
        (Some(body), Some(after)) => match ctx.body_after(after, body) {
            Some(shape) => vec![(body, shape)],
            None => return Vec::new(),
        },
        (Some(body), None) => match ctx.body_in(component, body) {
            Ok(shape) => vec![(body, shape)],
            Err(_) => return Vec::new(),
        },
        (None, _) => ctx.bodies_in(component),
    };
    let mut out = Vec::new();
    for (_, shape) in shapes {
        for name in super::projection_successors::successor_edges(ctx.kernel, &shape, edge) {
            let curves = match ctx.kernel.curves_of(&shape, &TopoName::Edge(name.clone())) {
                Ok(curves) if !curves.is_empty() => curves,
                _ => continue,
            };
            let curves = match &moved {
                Some(t) => curves
                    .iter()
                    .map(|c| project::transform_curve(t, c))
                    .collect(),
                None => curves,
            };
            out.push(crate::sketch::successor::Successor {
                name: name.to_string(),
                curves,
            });
        }
    }
    out
}

/// The model curve of another sketch's curve or point (mitcad#904; a 3D
/// sketch's too, mitcad#1543), where that sketch has it now, in this
/// sketch's component's coordinates (moved there from another component's
/// by `link`); none when the sketch no longer has the entity: the
/// projection is lost and keeps its last geometry, as one of a body edge
/// that is gone.
fn sketch_entity_curves<K: Kernel>(
    entity: &SketchEntityRef,
    link: Option<&OccurrenceLink>,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<Vec<Curve3>, String> {
    let curve = super::spatial_sketch::sketch_entity_geometry(ctx, *entity)
        .map_err(|e| format!("projection of {entity}: {e}"))?;
    let Some(curve) = curve else {
        return Ok(Vec::new());
    };
    Ok(match link {
        Some(link) => {
            let (_, t) = ctx
                .linked(link)
                .map_err(|e| format!("projection of {entity}: {e}"))?;
            vec![project::transform_curve(&t, &curve)]
        }
        None => vec![curve],
    })
}

/// A sketch's curve or point in its component's coordinates, where its
/// evaluation solved it; None when the sketch has no such entity.
pub(crate) fn sketch_entity_curve(
    output: &SketchOutput,
    entity: crate::sketch::Ref,
) -> Option<Curve3> {
    // A sketch on a curved face's: on the face (mitcad#1768).
    if let Some(development) = &output.developed {
        return match entity {
            crate::sketch::Ref::Point(id) => output
                .solved
                .points
                .get(&id)
                .map(|p| Curve3::Point(development.point(*p))),
            crate::sketch::Ref::Curve(id) => output
                .solved
                .curves
                .get(&id)
                .map(|c| super::developed::curve3(development, c)),
        };
    }
    match entity {
        crate::sketch::Ref::Point(id) => output
            .solved
            .points
            .get(&id)
            .map(|p| Curve3::Point(output.frame.point(*p))),
        crate::sketch::Ref::Curve(id) => output
            .solved
            .curves
            .get(&id)
            .map(|c| super::path::curve3(&output.frame, c)),
    }
}

/// The frame of a face plane from the bodies before the sketch.
pub(crate) fn face_frame<K: Kernel>(
    plane: &SketchPlane,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<SketchFrame, String> {
    let SketchPlane::Face { face, body } = plane else {
        // A curved face's sketch has its development (mitcad#1768), a 3D
        // sketch's support is a plane.
        return Err("the sketch plane must be a plane or a planar face".to_owned());
    };
    let shapes = match body {
        Some(body) => vec![(*body, ctx.body(*body)?)],
        None => ctx.bodies(),
    };
    let SketchFace {
        body, found, moved, ..
    } = sketch_face(ctx.kernel, face, shapes)?;
    match (&moved, &found) {
        (Some(moved), _) => ctx.warn(moved.clone()),
        (None, Some(found)) => ctx.found_renamed("face", face, found, face.feature),
        (None, None) => {}
    }
    let face = GeomRef::Face {
        body,
        face: found.unwrap_or_else(|| (**face).clone()),
    };
    Ok(ctx
        .plane(&face)
        .map_err(|e| format!("the sketch plane: {e}"))?
        .frame())
}

/// The body among `shapes` that has the face: exactly one must. A face a
/// sketch change renamed is found again (`renamed.rs`, mitcad#272): then
/// also the name it has now.
pub(crate) fn face_body<K: Kernel>(
    kernel: &K,
    face: &FaceName,
    shapes: Vec<(BodyUid, K::Shape)>,
) -> Result<(BodyUid, K::Shape, Option<FaceName>), String> {
    let several = || format!("plane {face}: the face is on several bodies; name the body");
    let mut found = None;
    let mut missing = Vec::new();
    for (uid, shape) in shapes {
        let count = kernel
            .count_faces(&shape, face)
            .map_err(|e| format!("plane {face}: {e}"))?;
        if count > 0 {
            if found.is_some() {
                return Err(several());
            }
            found = Some((uid, shape, None));
        } else {
            missing.push((uid, shape));
        }
    }
    if let Some(found) = found {
        return Ok(found);
    }
    let mut ambiguous = None;
    for (uid, shape) in missing {
        match super::renamed::find_face(kernel, &shape, face) {
            Ok(Some(renamed)) => {
                if found.is_some() {
                    return Err(several());
                }
                found = Some((uid, shape, Some(renamed)));
            }
            Err(super::renamed::Lost::Ambiguous(message)) => ambiguous = Some(message),
            _ => {}
        }
    }
    match (found, ambiguous) {
        (Some(found), _) => Ok(found),
        (None, Some(message)) => Err(format!("plane {message}")),
        (None, None) => Err(format!(
            "plane {face}: the face does not exist at this point of the timeline"
        )),
    }
}

/// The face a sketch on a planar face lies on, as [`sketch_face`] finds it.
pub(crate) struct SketchFace<S> {
    pub body: BodyUid,
    pub shape: S,
    /// The face's name now, when it is not the stored one.
    pub found: Option<FaceName>,
    /// In words, why the sketch lies on another face than the stored name
    /// names (a piece of it, or the whole face of a piece); None for a face
    /// a sketch change renamed, which `found_renamed` tells (mitcad#272).
    pub moved: Option<String>,
}

/// The face of a sketch on a planar face ([`face_body`]), followed where a
/// change of an earlier feature split it or joined it again (mitcad#2351):
/// a face now split into pieces that do not lie in one plane (a symmetric
/// draft whose fixed plane now crosses it) gives its largest planar piece,
/// the piece most of the face is on; a piece (`…#k`) of a face that is no
/// longer split gives the whole face. Either way `moved` says so. Pieces
/// that lie in one plane stay as they are: the plane is theirs.
pub(crate) fn sketch_face<K: Kernel>(
    kernel: &K,
    face: &FaceName,
    shapes: Vec<(BodyUid, K::Shape)>,
) -> Result<SketchFace<K::Shape>, String> {
    let (body, shape, found) = match face_body(kernel, face, shapes.clone()) {
        Ok(found) => found,
        Err(error) => return whole_face(kernel, face, shapes).ok_or(error),
    };
    let name = found.clone().unwrap_or_else(|| face.clone());
    let pieces = kernel.count_faces(&shape, &name).unwrap_or(1);
    // Pieces on one surface share its geometry; pieces on several do not.
    let apart = pieces > 1
        && matches!(
            kernel.face_geometry(&shape, &name),
            Err(error) if !matches!(error, KernelError::Unsupported(_))
        );
    let piece = if apart {
        kernel
            .faces(&shape)
            .ok()
            .and_then(|faces| largest_piece(&faces, &name))
    } else {
        None
    };
    Ok(match piece {
        Some((piece, count)) => SketchFace {
            body,
            shape,
            moved: Some(format!(
                "face {name} is split into {count} pieces that do not lie in one plane; the \
                 sketch lies on the largest, {piece} (Redefine Sketch Plane puts it on another)"
            )),
            found: Some(piece),
        },
        // One face, pieces in one plane, or no planar piece (the plane's
        // own error then says why).
        None => SketchFace {
            body,
            shape,
            found,
            moved: None,
        },
    })
}

/// The whole face of a piece name (`…#k`) whose face is no longer split:
/// the name without its piece numbers, where it finds exactly one face.
fn whole_face<K: Kernel>(
    kernel: &K,
    face: &FaceName,
    shapes: Vec<(BodyUid, K::Shape)>,
) -> Option<SketchFace<K::Shape>> {
    let mut whole = face.clone();
    while whole.split.pop().is_some() {
        let Ok((body, shape, found)) = face_body(kernel, &whole, shapes.clone()) else {
            continue;
        };
        let name = found.unwrap_or_else(|| whole.clone());
        if kernel.count_faces(&shape, &name).ok() == Some(1) {
            return Some(SketchFace {
                body,
                shape,
                moved: Some(format!(
                    "face {face} is no longer split; the sketch lies on the whole face {name}"
                )),
                found: Some(name),
            });
        }
    }
    None
}

/// The largest planar face of `faces` that is a piece of `name` (named
/// `name#k`, or deeper), with the number of faces its pieces are; of pieces
/// of the same area the first by piece number. None without a planar piece.
fn largest_piece(faces: &[crate::kernel::FaceInfo], name: &FaceName) -> Option<(FaceName, usize)> {
    let mut count = 0;
    let mut best: Option<(f64, FaceName)> = None;
    for info in faces {
        let Some(piece) = info
            .names
            .iter()
            .filter_map(|n| n.parse::<FaceName>().ok())
            .find(|n| n.matches(name) && n.split.len() > name.split.len())
        else {
            continue;
        };
        count += 1;
        if info.surface != "plane" {
            continue;
        }
        let larger = best.as_ref().is_none_or(|(area, held)| {
            let margin = 1e-9 * area.abs().max(1.0);
            info.area > area + margin
                || ((info.area - area).abs() <= margin && piece.split < held.split)
        });
        if larger {
            best = Some((info.area, piece));
        }
    }
    best.map(|(_, piece)| (piece, count))
}

/// The frame of a plane of another component (mitcad#100): found there and
/// moved into the sketch's coordinates. An origin or construction plane
/// keeps its own axes; a face gets the frame every planar face gets
/// ([`SketchPlane::face_frame`]), in the sketch's coordinates.
pub(crate) fn linked_frame<K: Kernel>(
    plane: &SketchPlane,
    link: &OccurrenceLink,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<SketchFrame, String> {
    let (component, to_sketch): (_, Transform) = ctx
        .linked(link)
        .map_err(|e| format!("the sketch plane: {e}"))?;
    if let Some(frame) = plane.origin_frame() {
        return Ok(to_sketch.apply_frame(&frame));
    }
    match plane {
        SketchPlane::Construction(uid) => {
            Ok(to_sketch.apply_frame(&ctx.datum_plane(*uid)?.frame()))
        }
        SketchPlane::Face { face, body } => {
            let shapes = match body {
                Some(body) => vec![(*body, ctx.body_in(component, *body)?)],
                None => ctx.bodies_in(component),
            };
            let SketchFace {
                shape,
                found,
                moved,
                ..
            } = sketch_face(ctx.kernel, face, shapes)?;
            match (&moved, &found) {
                (Some(moved), _) => ctx.warn(moved.clone()),
                (None, Some(found)) => ctx.found_renamed("face", face, found, face.feature),
                (None, None) => {}
            }
            match ctx
                .kernel
                .face_geometry(&shape, found.as_ref().unwrap_or(&**face))
                .map_err(|e| {
                    format!(
                        "the sketch plane: {}",
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    )
                })? {
                crate::datum::SurfaceGeometry::Plane { origin, normal } => {
                    Ok(SketchPlane::face_frame(
                        to_sketch.apply_point(origin),
                        to_sketch.apply_vector(normal),
                    ))
                }
                other => Err(format!(
                    "the sketch plane: face {face} is not planar ({})",
                    other.kind()
                )),
            }
        }
        SketchPlane::Developed { .. } => {
            Err("a sketch on a curved face lies on a face of its own component".to_owned())
        }
        SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {
            unreachable!("origin planes have a frame")
        }
    }
}

#[cfg(test)]
mod piece_tests {
    use super::largest_piece;
    use crate::kernel::FaceInfo;
    use crate::topo::FaceName;

    fn info(names: &[&str], surface: &str, area: f64) -> FaceInfo {
        FaceInfo {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
            surface: surface.to_owned(),
            area,
        }
    }

    fn name(text: &str) -> FaceName {
        text.parse().expect("a face name")
    }

    #[test]
    fn the_largest_planar_piece_of_a_split_face_is_taken() {
        let side = "F2:side(c2[c1,c3])";
        let faces = [
            info(&["F2:end(r{c1})"], "plane", 900.0),
            info(&[&format!("{side}#0")], "plane", 120.0),
            info(&[&format!("{side}#1")], "plane", 380.0),
        ];
        assert_eq!(
            largest_piece(&faces, &name(side)),
            Some((name(&format!("{side}#1")), 2))
        );
        // Pieces of the same area: the first by number.
        let even = [
            info(&[&format!("{side}#1")], "plane", 250.0),
            info(&[&format!("{side}#0")], "plane", 250.0),
        ];
        assert_eq!(
            largest_piece(&even, &name(side)),
            Some((name(&format!("{side}#0")), 2))
        );
        // A curved piece is counted but not taken; no planar piece, none.
        let curved = [
            info(&[&format!("{side}#0")], "cone", 900.0),
            info(&[&format!("{side}#1")], "plane", 100.0),
        ];
        assert_eq!(
            largest_piece(&curved, &name(side)),
            Some((name(&format!("{side}#1")), 2))
        );
        assert_eq!(largest_piece(&curved[..1], &name(side)), None);
        // Another face is no piece of it.
        assert_eq!(largest_piece(&faces[..1], &name(side)), None);
    }
}
