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
            SketchPlane::Face { face, body } => {
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
            SketchPlane::Face { face, body } => {
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
                if !matches!(entry.def, super::FeatureDef::IntersectionCurve(_)) {
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
                SketchPlane::Face { face, body } => {
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
/// parameters' expressions, with those driven dimensions.
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
    for d in &def.dimensions {
        let Some(value) = d.value else {
            continue;
        };
        let mut found = BTreeSet::new();
        let mut seen = BTreeSet::from([value]);
        let mut todo = vec![value];
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
        if !found.is_empty() {
            uses.insert(d.id, found);
        }
    }
    uses
}

/// Solves a sketch whose driving dimensions use its own driven ones
/// (`uses`, mitcad#1116) in stages ([`crate::sketch::driven`]): each stage
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
        let plane = match (&self.plane_link, &self.plane, self.plane.origin_frame()) {
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
        } = crate::sketch::project::follow_links(self, &frame, &mut |projection| {
            source_curves(projection, &frame, ctx)
        })?;
        // Linked projections that could not follow say so (mitcad#232).
        for report in &follow {
            if let Some(warning) = self
                .projections
                .get(report.projection)
                .and_then(|p| report.warning(p))
            {
                ctx.warn(warning);
            }
        }
        let def = followed.as_ref().unwrap_or(self);
        // Every parameter's own name for the errors (mitcad#1314: a driven
        // dimension's reference and an offset's distance too, not only the
        // driving dimensions' values).
        let params = ctx.env.params;
        let name = |id: ParamId| params.name(id);
        // Driving dimensions that use the sketch's own driven ones are
        // solved after them (mitcad#1116).
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
        let boundary = face_boundary(self, &plane, &frame, ctx)?;
        let (region_info, layout, own) = if boundary.is_empty() {
            let (region_info, layout) = def.regions_and_layout_of(&solved, &texts);
            (region_info, layout, OwnRegions::default())
        } else {
            let own = def.region_curves_of(&solved, &texts);
            let mut curves = own.clone();
            curves.extend(boundary);
            let (region_info, layout) = crate::sketch::regions::face_bounded_regions(&curves);
            // The sketch's own regions too, for references to a region the
            // face's edges cut (mitcad#1182).
            (region_info, layout, OwnRegions::new(own))
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
            }),
            ..FeatureOutput::default()
        })
    }
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
    let (_, shape, renamed) = face_body(ctx.kernel, face, shapes)?;
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
        unreachable!("only a face plane is found on the bodies");
    };
    let shapes = match body {
        Some(body) => vec![(*body, ctx.body(*body)?)],
        None => ctx.bodies(),
    };
    let (body, _, found) = face_body(ctx.kernel, face, shapes)?;
    if let Some(found) = &found {
        ctx.found_renamed("face", face, found, face.feature);
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
fn face_body<K: Kernel>(
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
            let (_, shape, found) = face_body(ctx.kernel, face, shapes)?;
            if let Some(found) = &found {
                ctx.found_renamed("face", face, found, face.feature);
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
        SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {
            unreachable!("origin planes have a frame")
        }
    }
}
