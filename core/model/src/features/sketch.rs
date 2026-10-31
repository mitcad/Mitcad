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
use crate::kernel::{Curve3, Kernel};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::sketch::{Projection, ProjectionSource, SketchEntityRef, project};
use crate::topo::{FaceName, TopoName};
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
            for feature in topo_features(&projection.source) {
                ctx.feature(feature)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
            }
            // Another sketch's curve or point (mitcad#904): of a sketch
            // before this one.
            if let ProjectionSource::Sketch(entity) = &projection.source {
                ctx.sketch(entity.sketch)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
            }
            if let Some(body) = projection.body {
                ctx.body(body)?;
            }
            if let Some(after) = projection.after {
                ctx.feature(after)
                    .map_err(|e| format!("projection of {}: {e}", projection.source))?;
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
        let names: BTreeMap<ParamId, String> = def
            .dimensions
            .iter()
            .filter_map(|d| d.value)
            .map(|id| (id, ctx.param_name(id)))
            .collect();
        let name = |id| names.get(&id).cloned().unwrap_or_default();
        let solved = match &followed {
            None => def.solve_with(&mut |id| ctx.param(id), &name),
            Some(followed) => crate::sketch::project::solve_followed(
                self,
                followed,
                &mut |id| ctx.param(id),
                &name,
            ),
        }
        .map_err(|e| e.message)?;
        // A line or round curve the constraints collapsed leaves no region
        // (mitcad#365).
        if let Some(warning) = solved.status.degenerate_warning() {
            ctx.warn(warning);
        }
        // Where linked projections and what hangs on them are now.
        let moved = match &followed {
            None => Vec::new(),
            Some(followed) => {
                let mut solution = followed.clone();
                solved.store(&mut solution.entities);
                crate::sketch::project::moved_entities(self, &solution)
            }
        };
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
        let (region_info, layout) = def.regions_and_layout_of(&solved, &texts);
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
            }),
            ..FeatureOutput::default()
        })
    }
}

/// The model curves of a projection's source, in the sketch's component's
/// coordinates (moved there from another component's by its link,
/// mitcad#100): its curves, or for section curves where the sketch's plane
/// (`frame`) cuts it (mitcad#389).
fn source_curves<K: Kernel>(
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

/// The model curve of another sketch's curve or point (mitcad#904), where
/// that sketch has it now, in this sketch's component's coordinates (moved
/// there from another component's by `link`); none when the sketch no
/// longer has the entity: the projection is lost and keeps its last
/// geometry, as one of a body edge that is gone.
fn sketch_entity_curves<K: Kernel>(
    entity: &SketchEntityRef,
    link: Option<&OccurrenceLink>,
    ctx: &mut super::EvalContext<'_, K>,
) -> Result<Vec<Curve3>, String> {
    let output = ctx
        .sketch(entity.sketch)
        .map_err(|e| format!("projection of {entity}: {e}"))?;
    let curve = sketch_entity_curve(&output, entity.entity);
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
fn face_frame<K: Kernel>(
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
fn linked_frame<K: Kernel>(
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
                .map_err(|e| format!("the sketch plane: face {face}: {e}"))?
            {
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
