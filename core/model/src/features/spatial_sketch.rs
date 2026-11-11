// SPDX-License-Identifier: MIT
//! 3D Sketch (mitcad#1521): points with three coordinates and curves
//! through them in a frame that follows a support, solved by the spatial
//! system. The definition, the solver adapter and the output are in the
//! sketch crate (`mitcad_sketch::spatial`, re-exported here); what needs the
//! document stays here: the references, the checks against the timeline
//! and the evaluation.
//!
//! Evaluation finds the support's frame as a planar sketch finds its plane
//! (an origin plane, a construction plane of the same component, or a
//! planar face of its bodies, followed through a topology change that
//! renamed it), with the sketch's own frame in it when it has one (a
//! converted planar sketch's, mitcad#1544), made orthonormal. Then every
//! linked reference record's
//! source (mitcad#1543), each found in one place ([`source_geometry`]):
//! the origin; a vertex or an edge of a body made earlier (found by its
//! topological name, followed through a sketch change that renamed it when
//! that is unambiguous, never replaced by a nearby point; a name that
//! matches several edges is refused); a point or curve of an earlier
//! sketch, planar or 3D; the curve of an Intersection Curve. Geometry of
//! another component is found there and moved into the sketch's
//! component through the record's occurrence link, by the placements at
//! the sketch's point of the timeline, as a planar projection's is
//! (mitcad#100). An attachment's target is the source point, or the start,
//! end or middle of the source curve, in the local frame (`R^T (V - O)`);
//! a reference curve is the source curve in the local frame
//! ([`local_reference_curve`]). A projected record (Project) takes the
//! source along the sketch's normal onto its plane first, as a planar
//! sketch projects it. A point on a face (On Face, mitcad#1549) has the
//! face's surface in the local frame ([`local_surface`]): a plane,
//! cylinder, cone, sphere or torus is solved in closed form; any other
//! face through the kernel's closest-point projection, as the plane
//! touching it at the projection of the point, solved again from the
//! projection of the solved point until the point lies on the face. The
//! solve starts from the stored positions with the
//! driving dimensions' parameters. Every read is recorded for the recompute
//! cache, so a moved support or source, a moved occurrence, or a changed
//! parameter, evaluates the sketch again.
//!
//! A failure (a support or source not found, a bad value, conflicting
//! relations, invalid geometry) fails the feature: its definition stays as
//! it is, the output keeps the frame and the failure but no geometry, and
//! nothing later takes the last successful solve for the current one (a
//! lost source is a reference failure: no silent detach, no last geometry
//! kept).
//!
//! The sketch plane in planar form (mitcad#1550): a solved sketch's output
//! has, besides [`SpatialSketchOutput`], a planar sketch output of what
//! lies in its sketch plane ([`planar_view`]), whose closed loops bound
//! profile regions as a planar sketch's curves do (with the supporting
//! face's edges, texts, derived offsets), so Extrude, Revolve and every
//! other profile feature take them ([`super::CheckContext::profile_sketch`]).
//! A converted planar sketch's planar records act as in the planar sketch:
//! its plane of another component is found through its link, its
//! placement moves the frame on the plane, and its kept linked projections
//! follow their sources before the solve (their entities moved where the
//! sources are now, held there by their in-plane fixes).
//!
//! The checks before the solve refuse a source that does not come before
//! the sketch in the timeline, and name a later one that uses the sketch,
//! directly or through other features, as a link cycle.

pub use mitcad_sketch::spatial::*;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::construction::datum_kind;
use super::sketch::{LinkedReferences, OwnRegions, SketchDef, SketchOutput, sketch_entity_curve};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureEntry, FeatureInfo, FeatureOutput,
    References,
};
use crate::datum::{DatumKind, SurfaceGeometry};
use crate::features::SketchPlane;
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
use crate::links::OccurrenceLink;
// Messages in plain words with their places, the ids for agents
// (mitcad#1644).
use crate::notice::{FeatureError, said, said_after, said_around, said_at};
use crate::parameters::{ParamId, Parameters};
use crate::profile::SketchFrame;
use crate::recompute::Read;
use crate::sketch::regions::{RegionCurve, face_bounded_regions, regions_and_layout};
use crate::sketch::spatial_planar::planar_view;
use crate::sketch::{ConstraintUid, SketchEntityRef, project};
use crate::topo::{FaceName, TopoName};
use crate::transform::Transform;

impl FeatureInfo for SpatialSketchDef {
    fn references(&self) -> References {
        let mut references = References::default();
        match &self.support {
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
        for source in sources(self) {
            references.features.extend(source.features());
        }
        // The planes of Symmetric and On Plane relations (mitcad#1661).
        for (_, plane) in self.plane_records() {
            references.features.extend(plane.features());
        }
        // The kept linked projections (mitcad#1550).
        for projection in &self.planar.projections {
            references
                .features
                .extend(super::sketch::topo_features(&projection.source));
            if let Some(body) = projection.body {
                references.body(body);
            }
            references.features.extend(projection.after);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.check_definition()?;
        // The sketch's own component, when it is in the timeline already.
        let own = ctx
            .all
            .iter()
            .find(|f| f.uid == ctx.uid)
            .map(|f| f.component);
        match &self.support {
            SketchPlane::Face { face, body } => {
                for feature in face.features() {
                    ctx.feature(feature).map_err(|e| {
                        said_around(
                            |m| format!("the support: {m}"),
                            &format!("support {face}: "),
                            &e,
                        )
                    })?;
                }
                if let Some(body) = body {
                    ctx.body(*body)?;
                }
            }
            SketchPlane::Construction(uid) => {
                let entry = ctx.feature(*uid)?;
                if datum_kind(&entry.def) != Some(DatumKind::Plane) {
                    return Err(said(
                        format!("{} is not a construction plane", entry.name),
                        format!("{} ({uid}) is not a construction plane", entry.name),
                    ));
                }
                // Another component's plane only through a kept link
                // (mitcad#1550).
                if self.planar.plane_link.is_none() && own.is_some_and(|own| own != entry.component)
                {
                    return Err(said(
                        format!(
                            "{} is a construction plane of another component; a 3D sketch's \
                             support is in its own component",
                            entry.name
                        ),
                        format!(
                            "{} ({uid}) is a construction plane of another component; a 3D \
                             sketch's support is in its own component",
                            entry.name
                        ),
                    ));
                }
            }
            SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
        }
        for (id, point, source) in self.attachments() {
            check_source(ctx, source).map_err(|e| {
                about_source(
                    About::Attachment,
                    &format!("{id}: p{} attached to {source}: ", point.0),
                    &e,
                    source,
                )
            })?;
        }
        for (id, source) in self.reference_curves() {
            check_source(ctx, source).map_err(|e| {
                about_source(
                    About::ReferenceCurve,
                    &format!("c{}: reference curve of {source}: ", id.0),
                    &e,
                    source,
                )
            })?;
        }
        for (id, point, source) in self.face_records() {
            check_source(ctx, source).map_err(|e| {
                about_source(
                    About::Face,
                    &format!("{id}: p{} on {source}: ", point.0),
                    &e,
                    source,
                )
            })?;
        }
        // The planes of Symmetric and On Plane relations (mitcad#1661).
        for (id, plane) in self.plane_records() {
            check_plane(ctx, plane, own).map_err(|e| {
                FeatureError::from_text(&said_around(
                    |m| format!("the plane of a relation: {m}"),
                    &format!("{id}: {plane}: "),
                    &e,
                ))
                .with_places(plane.places())
                .text()
            })?;
        }
        // The kept linked projections (mitcad#1550), as a planar sketch
        // checks its own.
        for projection in &self.planar.projections {
            let what = |e: String| {
                said_around(
                    |m| format!("a kept projection: {m}"),
                    &format!("projection of {}: ", projection.source),
                    &e,
                )
            };
            for feature in super::sketch::topo_features(&projection.source) {
                ctx.feature(feature).map_err(what)?;
            }
            match &projection.source {
                crate::sketch::ProjectionSource::Sketch(entity) => {
                    let entry = ctx.feature(entity.sketch).map_err(what)?;
                    if !matches!(
                        entry.def,
                        FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_)
                    ) {
                        return Err(what(not_a_sketch(entry)));
                    }
                }
                crate::sketch::ProjectionSource::Curve(feature) => {
                    let entry = ctx.feature(*feature).map_err(what)?;
                    if !matches!(entry.def, FeatureDef::IntersectionCurve(_)) {
                        return Err(what(format!("{} makes no curve", entry.name)));
                    }
                }
                crate::sketch::ProjectionSource::Topo(_) => {}
            }
            if let Some(body) = projection.body {
                ctx.body(body)?;
            }
            if let Some(after) = projection.after {
                ctx.feature(after).map_err(what)?;
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        SpatialSketchDef::check_values(self, value)
    }
}

/// What a source's failure is about, in words (mitcad#1644): one whole
/// message for each, inner's message its value.
#[derive(Clone, Copy)]
enum About {
    /// The source itself (inner's message as it is).
    Source,
    Attachment,
    ReferenceCurve,
    Face,
}

impl About {
    fn said(self, message: &str) -> String {
        match self {
            Self::Source => message.to_owned(),
            Self::Attachment => format!("the source of an attachment: {message}"),
            Self::ReferenceCurve => format!("the source of a reference curve: {message}"),
            Self::Face => format!("the face a point lies on: {message}"),
        }
    }
}

/// The failure `inner` of a record's `source` (mitcad#1644): a person
/// reads what it is `about` and inner's message with the source's vertex,
/// edge or face painted, agents `named` (with the ids) before inner's
/// agent text.
fn about_source(about: About, named: &str, inner: &str, source: &SpatialSource) -> String {
    FeatureError::from_text(&said_around(|m| about.said(m), named, inner))
        .with_places(source.places())
        .text()
}

/// The refusal of a feature that is no sketch where a sketch is wanted.
fn not_a_sketch(entry: &FeatureEntry) -> String {
    said(
        format!("{} is not a sketch", entry.name),
        format!("{} ({}) is not a sketch", entry.name, entry.uid),
    )
}

/// The sources of a sketch's linked reference records: its attachments',
/// its reference curves' and its points' on faces (mitcad#1549).
fn sources(def: &SpatialSketchDef) -> Vec<&SpatialSource> {
    def.attachments()
        .map(|(_, _, source)| source)
        .chain(def.reference_curves().map(|(_, source)| source))
        .chain(def.face_records().map(|(_, _, source)| source))
        .collect()
}

/// The features its records name through links to other components
/// (mitcad#1543, as a planar sketch's projections, mitcad#100): the
/// document checks that they are in the component each link leads to.
impl LinkedReferences for SpatialSketchDef {
    fn linked_references(&self) -> Vec<(FeatureUid, &OccurrenceLink)> {
        let mut out = Vec::new();
        for source in sources(self) {
            if let Some(link) = source.link() {
                out.extend(source.features().into_iter().map(|uid| (uid, link)));
            }
        }
        // A relation's plane on another component's face (mitcad#1661).
        for (_, plane) in self.plane_records() {
            if let SpatialPlaneRef::Face(source) = plane
                && let Some(link) = source.link()
            {
                out.extend(source.features().into_iter().map(|uid| (uid, link)));
            }
        }
        // A kept plane of another component and kept linked projections
        // (mitcad#1550), as a planar sketch's.
        if let Some(link) = &self.planar.plane_link {
            match &self.support {
                SketchPlane::Face { face, body } => {
                    out.extend(face.features().into_iter().map(|uid| (uid, link)));
                    if let Some(body) = body {
                        out.push((body.feature, link));
                    }
                }
                SketchPlane::Construction(uid) => out.push((*uid, link)),
                SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
            }
        }
        for projection in &self.planar.projections {
            if let Some(link) = &projection.link {
                let mut features = super::sketch::topo_features(&projection.source);
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

/// Checks the plane of a Symmetric or On Plane relation against the
/// timeline (mitcad#1661): a construction plane or a sketch of the sketch's
/// own component before it, or a face source as a record's.
fn check_plane(
    ctx: &CheckContext<'_>,
    plane: &SpatialPlaneRef,
    own: Option<crate::ids::ComponentUid>,
) -> Result<(), String> {
    let other = |entry: &FeatureEntry| own.is_some_and(|own| own != entry.component);
    match plane {
        SpatialPlaneRef::Frame(_) => Ok(()),
        SpatialPlaneRef::Face(source) => check_source(ctx, source),
        SpatialPlaneRef::Construction(uid) => {
            let entry = source_feature(ctx, *uid)?;
            if datum_kind(&entry.def) != Some(DatumKind::Plane) {
                return Err(said(
                    format!("{} is not a construction plane", entry.name),
                    format!("{} ({uid}) is not a construction plane", entry.name),
                ));
            }
            if other(entry) {
                return Err(said(
                    format!(
                        "{} is a construction plane of another component",
                        entry.name
                    ),
                    format!("{} ({uid}) is of another component", entry.name),
                ));
            }
            Ok(())
        }
        SpatialPlaneRef::Sketch(uid) => {
            let entry = source_feature(ctx, *uid)?;
            if !matches!(
                entry.def,
                FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_)
            ) {
                return Err(not_a_sketch(entry));
            }
            if other(entry) {
                return Err(said(
                    format!("{} is a sketch of another component", entry.name),
                    format!("{} ({uid}) is of another component", entry.name),
                ));
            }
            Ok(())
        }
    }
}

/// Checks a record's source against the timeline: its features come
/// before the sketch, its body is made by one of them, another sketch's
/// entity is of a sketch (planar or 3D) and an Intersection Curve's curve
/// of an Intersection Curve.
fn check_source(ctx: &CheckContext<'_>, source: &SpatialSource) -> Result<(), String> {
    for feature in source.features() {
        source_feature(ctx, feature)?;
    }
    if let Some(body) = source.body() {
        ctx.body(body)?;
    }
    match source {
        SpatialSource::Sketch { entity, .. } => {
            let entry = source_feature(ctx, entity.sketch)?;
            if !matches!(
                entry.def,
                FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_)
            ) {
                return Err(not_a_sketch(entry));
            }
        }
        SpatialSource::Curve { feature, .. } => {
            let entry = source_feature(ctx, *feature)?;
            if !matches!(entry.def, FeatureDef::IntersectionCurve(_)) {
                return Err(said(
                    format!("{} makes no curve", entry.name),
                    format!("{} ({}) makes no curve", entry.name, entry.uid),
                ));
            }
        }
        SpatialSource::Origin
        | SpatialSource::Vertex { .. }
        | SpatialSource::Edge { .. }
        | SpatialSource::Face { .. } => {}
    }
    Ok(())
}

/// A feature a source names: one before the sketch. A later one that uses
/// the sketch, directly or through the features it uses (in any
/// component), would make a link cycle and is refused as one.
fn source_feature<'a>(
    ctx: &'a CheckContext<'_>,
    uid: FeatureUid,
) -> Result<&'a FeatureEntry, String> {
    ctx.feature(uid).map_err(|error| {
        if uid == ctx.uid || !uses(ctx, uid, ctx.uid) {
            return error;
        }
        let found = ctx
            .all
            .iter()
            .find(|f| f.uid == uid)
            .map(|f| f.name.clone());
        let name = found.clone().unwrap_or_else(|| uid.to_string());
        // The feature's name for a person (mitcad#1644), its uid for agents.
        let said_text = match found {
            Some(name) => {
                format!("{name} uses this 3D sketch: a reference to it would make a link cycle")
            }
            None => {
                "a later feature uses this 3D sketch: a reference to it would make a link cycle"
                    .to_owned()
            }
        };
        said(
            said_text,
            format!(
                "{name} ({uid}) uses this 3D sketch: a reference to it would make a link cycle"
            ),
        )
    })
}

/// Whether the feature `from` uses the feature `to`, directly or through
/// the features it uses.
fn uses(ctx: &CheckContext<'_>, from: FeatureUid, to: FeatureUid) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(uid) = stack.pop() {
        if !seen.insert(uid) {
            continue;
        }
        let Some(entry) = ctx.index.find(ctx.all, uid) else {
            continue;
        };
        for next in entry.def.info().references().features {
            if next == to {
                return true;
            }
            stack.push(next);
        }
    }
    false
}

impl<K: Kernel> Evaluate<K> for SpatialSketchDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (output, sketch) = evaluate_sketch(self, ctx);
        // The values the driven dimensions measured, into their reference
        // parameters (mitcad#1661): the features after it use them.
        let measured = output
            .solved
            .as_ref()
            .map(|solved| {
                self.dimensions
                    .iter()
                    .filter_map(|d| Some((d.reference?, *solved.status.measured.get(&d.id)?)))
                    .collect()
            })
            .unwrap_or_default();
        let result = match &output.failure {
            Some(failure) => Err(failure.message.clone()),
            None => Ok(FeatureOutput {
                sketch,
                measured,
                ..FeatureOutput::default()
            }),
        };
        // Kept with the result also when it failed (the frame and the
        // failure, no geometry): the edits and queries read it.
        ctx.spatial = Some(output);
        result
    }
}

/// The frames of a 3D sketch in its component: the support's plane, the
/// base (with the sketch's own frame) and the sketch's frame (with its
/// placement on the plane), the last two orthonormal.
struct Frames {
    plane: SketchFrame,
    base: SketchFrame,
    frame: SketchFrame,
}

/// The frame of the support in the sketch's component, orthonormal: found
/// through a kept link when the plane is another component's, with the
/// sketch's own frame and its kept placement (mitcad#1550).
fn support_frame<K: Kernel>(
    def: &SpatialSketchDef,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Frames, String> {
    let plane = match (
        &def.planar.plane_link,
        &def.support,
        def.support.origin_frame(),
    ) {
        (Some(link), support, _) => super::sketch::linked_frame(support, link, ctx)?,
        (None, _, Some(frame)) => frame,
        (None, SketchPlane::Construction(uid), None) => {
            if let Some(entry) = ctx.env.feature(*uid)
                && entry.component != ctx.component()
            {
                return Err(format!(
                    "{} is a construction plane of another component",
                    entry.name
                ));
            }
            ctx.datum_plane(*uid)?.frame()
        }
        (None, support, None) => super::sketch::face_frame(support, ctx)?,
    };
    // A converted planar sketch's own frame in its support (mitcad#1544).
    let base = match &def.frame {
        Some(own) => own.apply(&plane),
        None => plane,
    };
    let frame = match &def.planar.placement {
        Some(placement) => placement.resolve(&mut |id| ctx.param(id))?.apply(&base),
        None => base,
    };
    Ok(Frames {
        plane,
        base: orthonormal_frame(&base)?,
        frame: orthonormal_frame(&frame)?,
    })
}

/// The shell of a planar sketch with the 3D sketch's support, plane link,
/// own frame and placement and kept projections, and the planar entities
/// of its points and curves (`planar_entities`): what the planar machinery
/// that follows projections and bounds profiles with a face's edges reads.
fn planar_shell(def: &SpatialSketchDef) -> SketchDef {
    SketchDef {
        plane: def.support.clone(),
        plane_link: def.planar.plane_link.clone(),
        frame: def.frame.clone(),
        entities: def.planar_entities(),
        projections: def.planar.projections.clone(),
        ..SketchDef::default()
    }
}

/// The kept linked projections (mitcad#1550) followed to their sources
/// where they are now, as a planar sketch follows its own: the definition
/// with their entities moved there, when they moved; their in-plane fixes
/// hold them there in the solve.
fn follow_kept<K: Kernel>(
    def: &SpatialSketchDef,
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Option<SpatialSketchDef>, String> {
    if def.planar.projections.is_empty() {
        return Ok(None);
    }
    let shell = planar_shell(def);
    let project::Followed {
        def: followed,
        reports,
    } = project::follow_links(&shell, frame, &mut |projection| {
        super::sketch::source_curves(projection, frame, ctx)
    })?;
    for report in &reports {
        if let Some(warning) = shell
            .projections
            .get(report.projection)
            .and_then(|p| report.warning(p))
        {
            ctx.warn(warning);
        }
    }
    Ok(followed.map(|followed| {
        let mut moved = def.clone();
        moved.take_planar_positions(&followed.entities, &def.planar.projected());
        moved
    }))
}

/// The planar sketch output of what lies in a solved 3D sketch's sketch
/// plane (mitcad#1550): its closed loops' profile regions as a planar
/// sketch's (with its texts, its derived offsets made again and the
/// supporting face's edges), its frames and solved planar geometry.
fn planar_output<K: Kernel>(
    def: &SpatialSketchDef,
    output: &SpatialSketchOutput,
    frames: &Frames,
    ctx: &mut EvalContext<'_, K>,
) -> Result<SketchOutput, String> {
    let mut flat = planar_view(def, output);
    if flat.def.offsets.iter().any(|o| o.derived) {
        crate::sketch::offset::regenerate(
            &flat.def.offsets,
            &flat.def.entities,
            &mut flat.solved,
            &mut |id| ctx.param(id),
        )?;
    }
    let valued;
    let planar = if flat.def.text_values.is_empty() {
        &flat.def
    } else {
        valued = flat
            .def
            .with_parameter_texts(&mut |id| ctx.param_text(id))?;
        &valued
    };
    let texts = planar.text_outlines(&flat.solved, ctx.kernel)?;
    for outline in texts.iter().filter(|t| t.fallback) {
        if let Some(text) = planar.texts.iter().find(|t| t.id == outline.id) {
            ctx.warn(said(
                format!(
                    "the font {} of a text is not installed; {} is used",
                    text.font, outline.family
                ),
                format!(
                    "text t{}: the font {} is not installed; {} is used",
                    text.id.0, text.font, outline.family
                ),
            ));
        }
    }
    let mut curves = planar.region_curves_of(&flat.solved, &texts);
    curves.extend(
        flat.reference_curves
            .iter()
            .map(|(id, curve)| RegionCurve::new(*id, curve.clone())),
    );
    let shell = SketchDef {
        plane: def.support.clone(),
        plane_link: def.planar.plane_link.clone(),
        ..SketchDef::default()
    };
    let boundary = super::sketch::face_boundary(&shell, &frames.plane, &frames.frame, ctx)?;
    let (region_info, layout, own) = if boundary.is_empty() {
        let (region_info, layout) = regions_and_layout(&curves);
        (region_info, layout, OwnRegions::default())
    } else {
        let own = curves.clone();
        curves.extend(boundary);
        let (region_info, layout) = face_bounded_regions(&curves);
        (region_info, layout, OwnRegions::new(own))
    };
    Ok(SketchOutput {
        frame: frames.frame,
        base: frames.base,
        regions: region_info.iter().map(|r| r.profile.clone()).collect(),
        region_info,
        layout: Arc::new(layout),
        solved: Arc::new(flat.solved),
        texts: Arc::new(texts),
        moved: Arc::new(Vec::new()),
        follow: Arc::new(Vec::new()),
        own: Arc::new(own),
        retrimmed: None,
    })
}

/// What a 3D sketch's evaluation gave (mitcad#1543: another sketch's
/// source), read for the recompute cache; an error when it did not solve.
pub(crate) fn spatial_output<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
) -> Result<Arc<SpatialSketchOutput>, String> {
    let output = ctx.env.result_of(uid).and_then(|r| r.output.clone());
    ctx.reads
        .push(Read::Output(uid, output.as_ref().map(|o| o.version)));
    let name = ctx.feature_name(uid);
    match output.and_then(|o| o.spatial.clone()) {
        Some(spatial) if spatial.is_solved() => Ok(spatial),
        Some(spatial) => Err(match &spatial.failure {
            Some(failure) => format!("{name} failed: {}", failure.message),
            None => format!("{name} did not solve"),
        }),
        None => Err(format!(
            "{name} has no result at this point of the timeline"
        )),
    }
}

/// A point or curve of an earlier sketch, planar (mitcad#904) or 3D
/// (mitcad#1543), in its component's coordinates; None when the sketch
/// has no such entity, an error when it has no result.
pub(crate) fn sketch_entity_geometry<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    entity: SketchEntityRef,
) -> Result<Option<Curve3>, String> {
    let spatial = matches!(
        ctx.env.feature(entity.sketch).map(|f| &f.def),
        Some(FeatureDef::SpatialSketch(_))
    );
    if spatial {
        let output = spatial_output(ctx, entity.sketch)?;
        return Ok(output.entity_curve(entity.entity));
    }
    let output = ctx.sketch(entity.sketch)?;
    Ok(sketch_entity_curve(&output, entity.entity))
}

/// A body of the sketch's component, or of the component `link` leads to
/// with the transform from there into the sketch's component.
fn linked_body<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    link: Option<&OccurrenceLink>,
) -> Result<(K::Shape, Option<Transform>), String> {
    match link {
        Some(link) => {
            let (component, transform) = ctx.linked(link)?;
            Ok((ctx.body_in(component, body)?, Some(transform)))
        }
        None => Ok((ctx.body(body)?, None)),
    }
}

/// One curve of the pieces a source gives: the piece, or the pieces
/// joined end to end. `shown` names the source in words, `what` with its
/// names for agents (mitcad#1644).
fn one_curve(curves: Vec<Curve3>, shown: &str, what: &str) -> Result<Curve3, String> {
    match curves.len() {
        0 => Err(said(
            format!("{shown} gives no curve"),
            format!("{what} gives no curve"),
        )),
        1 => Ok(curves.into_iter().next().expect("one curve")),
        n => project::join_curves(&curves).ok_or_else(|| {
            said(
                format!("{shown} gives {n} curves that do not join into one"),
                format!("{what} gives {n} curves that do not join into one"),
            )
        }),
    }
}

/// A source's geometry in the sketch's component's coordinates
/// (mitcad#1543), found where it is now: a point (the origin of the
/// component, a vertex, another sketch's point) or a curve (an edge,
/// another sketch's curve, an Intersection Curve's curve), moved from
/// another component by the record's link. Every kind of source is found
/// here; [`source_target`] and [`source_curve`] take it into the local
/// frame.
fn source_geometry<K: Kernel>(
    source: &SpatialSource,
    ctx: &mut EvalContext<'_, K>,
) -> Result<Curve3, String> {
    let moved = |transform: Option<Transform>, curve: Curve3| match transform {
        Some(t) => project::transform_curve(&t, &curve),
        None => curve,
    };
    match source {
        SpatialSource::Origin => Ok(Curve3::Point([0.0; 3])),
        SpatialSource::Vertex { body, vertex, link } => {
            let (shape, transform) = linked_body(ctx, *body, link.as_ref())?;
            let found = super::renamed::vertex_for(ctx, &shape, vertex)?;
            let at = ctx.kernel.vertex_point(&shape, &found).map_err(|e| {
                about_source(
                    About::Source,
                    &format!("vertex {vertex}: "),
                    &e.to_string(),
                    source,
                )
            })?;
            Ok(moved(transform, Curve3::Point(at)))
        }
        SpatialSource::Edge {
            body, edge, link, ..
        } => {
            let (shape, transform) = linked_body(ctx, *body, link.as_ref())?;
            let found = super::renamed::edge_for(ctx, &shape, edge)?;
            let count = ctx.kernel.count_edges(&shape, &found);
            if count == 0 {
                return Err(said_at(
                    "the edge does not exist at this point of the timeline",
                    format!("edge {edge} does not exist at this point of the timeline"),
                    source.places(),
                ));
            }
            if count > 1 && found.index.is_none() {
                return Err(said_at(
                    format!("the edge reference matches {count} edges"),
                    format!("edge {edge} names {count} edges; name one with #k"),
                    source.places(),
                ));
            }
            let curves = ctx
                .kernel
                .curves_of(&shape, &TopoName::Edge(found))
                .map_err(|e| {
                    about_source(
                        About::Source,
                        &format!("edge {edge}: "),
                        &e.to_string(),
                        source,
                    )
                })?;
            Ok(moved(
                transform,
                one_curve(curves, "the edge", &format!("edge {edge}"))?,
            ))
        }
        SpatialSource::Sketch { entity, link, .. } => {
            let name = ctx.feature_name(entity.sketch);
            let curve = sketch_entity_geometry(ctx, *entity)?.ok_or_else(|| {
                said(
                    format!("{name} has no such point or curve"),
                    format!("{name} has no {}", entity.entity),
                )
            })?;
            let transform = match link {
                Some(link) => Some(ctx.linked(link)?.1),
                None => None,
            };
            Ok(moved(transform, curve))
        }
        // Moved by the link inside (mitcad#1417).
        SpatialSource::Curve { feature, link, .. } => {
            let pieces = ctx.linked_feature_curve(*feature, link.as_ref())?;
            let curves = pieces.iter().map(|p| p.curve.clone()).collect();
            let what = format!("the curve of {}", ctx.feature_name(*feature));
            one_curve(curves, &what, &what)
        }
        // A face is found by face_record (an On Face relation's).
        SpatialSource::Face { face, .. } => Err(said_at(
            "a face is no point or curve: a point lies on it through an On Face relation",
            format!(
                "face {face} is no point or curve: a point lies on it through an On Face relation"
            ),
            source.places(),
        )),
    }
}

/// Where an attachment's source point is, in the sketch's local frame:
/// the origin's is the frame's origin; any other is its source point, or
/// the start, end or middle of its source curve ([`local_source_point`]),
/// projected onto the sketch's plane when `projected`.
fn source_target<K: Kernel>(
    source: &SpatialSource,
    projected: bool,
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<[f64; 3], String> {
    if let SpatialSource::Origin = source {
        return Ok([0.0; 3]);
    }
    let geometry = source_geometry(source, ctx)?;
    local_source_point(frame, &geometry, source.at(), projected)
}

/// Where a reference curve's source curve is, in the sketch's local frame
/// ([`local_reference_curve`]): in true 3D, or projected along the
/// sketch's normal onto its plane when `projected`.
fn source_curve<K: Kernel>(
    source: &SpatialSource,
    projected: bool,
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<SpatialReferenceCurve, String> {
    let geometry = source_geometry(source, ctx)?;
    let curve = if projected {
        projected_curve(&geometry, frame)?
    } else {
        geometry
    };
    local_reference_curve(frame, &curve)
        .ok_or_else(|| "it is a point: a reference curve's source is a curve".to_owned())
}

/// A curve projected along the sketch's normal onto its plane, as a planar
/// sketch's Project makes it (`project.rs`), in the component's
/// coordinates.
fn projected_curve(curve: &Curve3, frame: &SketchFrame) -> Result<Curve3, String> {
    let mut found = project::project(std::slice::from_ref(curve), frame);
    match (found.len(), found.pop()) {
        (1, Some(project::Projected::Curve(c))) => Ok(super::path::curve3(frame, &c)),
        (1, Some(project::Projected::Point(_))) => {
            Err("seen along the sketch's normal it is a point".to_owned())
        }
        _ => Err("it has no curve on the sketch's plane".to_owned()),
    }
}

/// A face of an On Face relation where it is now (mitcad#1549): its body's
/// shape (of another component through the record's link, with the
/// transform from there into the sketch's component) and its name,
/// followed through a change that renamed it.
struct FoundFace<S> {
    shape: S,
    face: FaceName,
    transform: Option<Transform>,
}

/// Finds an On Face relation's face; a body or face that is not there at
/// the sketch's point of the timeline is a reference failure.
fn found_face<K: Kernel>(
    source: &SpatialSource,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FoundFace<K::Shape>, String> {
    let SpatialSource::Face { body, face, link } = source else {
        return Err(said_at(
            "the source is no face",
            format!("{source} is no face"),
            source.places(),
        ));
    };
    let (shape, transform) = linked_body(ctx, *body, link.as_ref())?;
    let found = super::renamed::face_for(ctx, &shape, face)?;
    if let Ok(0) = ctx.kernel.count_faces(&shape, &found) {
        return Err(said_at(
            "the face does not exist at this point of the timeline",
            format!("face {face} does not exist at this point of the timeline"),
            source.places(),
        ));
    }
    Ok(FoundFace {
        shape,
        face: found,
        transform,
    })
}

/// A surface moved by a placement (another component's face seen in the
/// sketch's component).
fn moved_surface(t: &Transform, surface: SurfaceGeometry) -> SurfaceGeometry {
    match surface {
        SurfaceGeometry::Plane { origin, normal } => SurfaceGeometry::Plane {
            origin: t.apply_point(origin),
            normal: t.apply_vector(normal),
        },
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => SurfaceGeometry::Cylinder {
            origin: t.apply_point(origin),
            axis: t.apply_vector(axis),
            radius,
        },
        SurfaceGeometry::Cone {
            origin,
            axis,
            radius,
            half_angle,
        } => SurfaceGeometry::Cone {
            origin: t.apply_point(origin),
            axis: t.apply_vector(axis),
            radius,
            half_angle,
        },
        SurfaceGeometry::Sphere { center, radius } => SurfaceGeometry::Sphere {
            center: t.apply_point(center),
            radius,
        },
        SurfaceGeometry::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => SurfaceGeometry::Torus {
            center: t.apply_point(center),
            axis: t.apply_vector(axis),
            major_radius,
            minor_radius,
        },
        other @ SurfaceGeometry::Other { .. } => other,
    }
}

/// The plane touching a face of another kind than a plane, cylinder,
/// cone, sphere or torus at its point nearest to `local` (the kernel's
/// closest-point projection, in the face's component), in the sketch's
/// local frame, and how far `local` is from that point (millimetres).
fn touching<K: Kernel>(
    found: &FoundFace<K::Shape>,
    frame: &SketchFrame,
    local: [f64; 3],
    ctx: &mut EvalContext<'_, K>,
) -> Result<(SpatialSurface, f64), String> {
    let near = to_component(frame, local);
    let there = match &found.transform {
        Some(t) => crate::assembly::inverse(t).apply_point(near),
        None => near,
    };
    let hit = ctx
        .kernel
        .face_point_normal(&found.shape, &found.face, there)
        .map_err(|e| said_after("", &format!("face {}: ", found.face), &e.to_string()))?;
    let (point, normal) = match &found.transform {
        Some(t) => (t.apply_point(hit.point), t.apply_vector(hit.normal)),
        None => (hit.point, hit.normal),
    };
    let plane = touching_plane(frame, point, normal).ok_or_else(|| {
        said(
            "the face has no normal near the point",
            format!("face {} has no normal near the point", found.face),
        )
    })?;
    let gap = (0..3)
        .map(|k| (near[k] - point[k]).powi(2))
        .sum::<f64>()
        .sqrt();
    Ok((plane, gap))
}

/// An On Face relation's surface in the sketch's local frame
/// (mitcad#1549): the face's own when it is a plane, cylinder, cone,
/// sphere or torus (solved in closed form); for another kind of face the
/// plane touching it nearest to `local` (the point where the solve starts),
/// and `true`: the solve is then repeated from the kernel's projection of
/// the solved point ([`solve_on_faces`]).
fn face_surface<K: Kernel>(
    found: &FoundFace<K::Shape>,
    frame: &SketchFrame,
    local: [f64; 3],
    ctx: &mut EvalContext<'_, K>,
) -> Result<(SpatialSurface, bool), String> {
    let geometry = ctx
        .kernel
        .face_geometry(&found.shape, &found.face)
        .map_err(|e| said_after("", &format!("face {}: ", found.face), &e.to_string()))?;
    let geometry = match &found.transform {
        Some(t) => moved_surface(t, geometry),
        None => geometry,
    };
    if let SurfaceGeometry::Other { .. } = geometry {
        let (plane, _) = touching(found, frame, local, ctx)?;
        return Ok((plane, true));
    }
    local_surface(frame, &geometry)
        .map(|surface| (surface, false))
        .ok_or_else(|| {
            said(
                "the face's surface has no valid values",
                format!(
                    "face {}: its {} surface has no valid values",
                    found.face,
                    geometry.kind()
                ),
            )
        })
}

/// How far a point may be from a face of another kind than a plane,
/// cylinder, cone, sphere or torus (the solve's acceptance tolerance,
/// millimetres), and how many touching planes are tried before the point
/// is said not to reach the face.
const FACE_TOLERANCE: f64 = 1e-6;
const FACE_ROUNDS: usize = 16;

/// A face solved through the kernel's projection: its relation, its point
/// and the face.
struct ProjectedFace<S> {
    id: ConstraintUid,
    point: EntityUid,
    found: FoundFace<S>,
}

/// Solves `model`, and while a point on a face of another kind is not on
/// it, puts it on the plane touching the face at the kernel's projection
/// of the solved point and solves again from there (mitcad#1549). The
/// planes the last solve used are kept in `surfaces`.
fn solve_on_faces<K: Kernel>(
    model: &mut SpatialModel,
    projected: &[ProjectedFace<K::Shape>],
    frame: &SketchFrame,
    surfaces: &mut BTreeMap<ConstraintUid, SpatialSurface>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<SpatialSolved, SpatialFailure> {
    let mut solved = model.solve()?;
    for _ in 0..FACE_ROUNDS {
        let mut moved = false;
        for face in projected {
            let local = solved.points.get(&face.point).copied().unwrap_or([0.0; 3]);
            let (plane, gap) =
                touching(&face.found, frame, local, ctx).map_err(|e| SpatialFailure {
                    kind: SpatialFailureKind::Reference,
                    message: said_after("", &format!("{}: p{} on ", face.id, face.point.0), &e),
                    constraints: vec![face.id],
                    entities: vec![face.point],
                })?;
            if gap > FACE_TOLERANCE {
                model.set_face_surface(face.id, plane)?;
                surfaces.insert(face.id, plane);
                moved = true;
            }
        }
        if !moved {
            return Ok(solved);
        }
        solved = model.solve()?;
    }
    let ids: Vec<ConstraintUid> = projected.iter().map(|f| f.id).collect();
    let points: Vec<EntityUid> = projected.iter().map(|f| f.point).collect();
    Err(SpatialFailure::said(
        SpatialFailureKind::NotConverged,
        format!(
            "points do not reach their faces (each solve on the plane touching a face left them \
             more than {FACE_TOLERANCE} mm off it)"
        ),
        format!(
            "{}: the points do not reach their faces (each solve on the plane touching a face \
             left them more than {FACE_TOLERANCE} mm off it)",
            ids.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ids,
        points,
    ))
}

/// Evaluates a 3D sketch: its output, with the failure when it failed, and
/// the planar output of its sketch plane when it solved.
fn evaluate_sketch<K: Kernel>(
    def: &SpatialSketchDef,
    ctx: &mut EvalContext<'_, K>,
) -> (SpatialSketchOutput, Option<SketchOutput>) {
    let frames = match support_frame(def, ctx) {
        Ok(frames) => frames,
        Err(e) => {
            let failure =
                SpatialFailure::new(SpatialFailureKind::Support, format!("the support: {e}"));
            return (SpatialSketchOutput::failed(None, def, failure), None);
        }
    };
    let frame = frames.frame;
    let followed = match follow_kept(def, &frame, ctx) {
        Ok(followed) => followed,
        Err(e) => {
            let failure = SpatialFailure::new(SpatialFailureKind::Reference, e);
            return (SpatialSketchOutput::failed(Some(frame), def, failure), None);
        }
    };
    let def = followed.as_ref().unwrap_or(def);
    let output = solve_sketch(def, frame, ctx);
    let planar = if output.is_solved() {
        match planar_output(def, &output, &frames, ctx) {
            Ok(planar) => Some(planar),
            Err(e) => {
                ctx.warn(format!("its profiles in the sketch plane: {e}"));
                None
            }
        }
    } else {
        None
    };
    (output, planar)
}

/// The plane of a Symmetric or On Plane relation found outside the sketch
/// (mitcad#1661), in the sketch's local frame: a construction plane's, a
/// planar face's (of another component through its link), or another
/// sketch's plane (a planar sketch's, a 3D sketch's XY plane).
fn relation_plane<K: Kernel>(
    plane: &SpatialPlaneRef,
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<SpatialSurface, String> {
    let (origin, normal) = match plane {
        SpatialPlaneRef::Frame(axis) => return Ok(SpatialPlaneRef::frame_surface(*axis)),
        SpatialPlaneRef::Construction(uid) => frame_plane(&ctx.datum_plane(*uid)?.frame()),
        SpatialPlaneRef::Sketch(uid) => {
            let spatial = matches!(
                ctx.env.feature(*uid).map(|f| &f.def),
                Some(FeatureDef::SpatialSketch(_))
            );
            let sketch_frame = if spatial {
                spatial_output(ctx, *uid)?
                    .frame
                    .ok_or_else(|| format!("{} has no frame", ctx.feature_name(*uid)))?
            } else {
                ctx.sketch(*uid)?.frame
            };
            frame_plane(&sketch_frame)
        }
        SpatialPlaneRef::Face(source) => {
            let found = found_face(source, ctx)?;
            let geometry = ctx
                .kernel
                .face_geometry(&found.shape, &found.face)
                .map_err(|e| said_after("", &format!("face {}: ", found.face), &e.to_string()))?;
            let geometry = match &found.transform {
                Some(t) => moved_surface(t, geometry),
                None => geometry,
            };
            match geometry {
                SurfaceGeometry::Plane { origin, normal } => (origin, normal),
                other => {
                    return Err(said(
                        "the face is not planar",
                        format!(
                            "face {} is a {} surface, not a plane",
                            found.face,
                            other.kind()
                        ),
                    ));
                }
            }
        }
    };
    let n = direction_to_local(frame, normal);
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !(len > 1e-12 && len.is_finite()) {
        return Err("the plane has no normal".to_owned());
    }
    Ok(SpatialSurface::Plane {
        origin: to_local(frame, origin),
        normal: n.map(|x| x / len),
    })
}

/// A sketch frame's plane: its origin and normal (`x x y`).
fn frame_plane(frame: &SketchFrame) -> ([f64; 3], [f64; 3]) {
    let (x, y) = (frame.x_axis, frame.y_axis);
    (
        frame.origin,
        [
            x[1] * y[2] - x[2] * y[1],
            x[2] * y[0] - x[0] * y[2],
            x[0] * y[1] - x[1] * y[0],
        ],
    )
}

/// The driving dimensions of `def` whose values use the reference
/// parameters of its own driven dimensions (mitcad#1661), directly or
/// through other parameters' expressions, with those driven dimensions.
pub(crate) fn own_uses(
    def: &SpatialSketchDef,
    params: &Parameters,
) -> BTreeMap<ConstraintUid, BTreeSet<ConstraintUid>> {
    let own: BTreeMap<ParamId, ConstraintUid> = def
        .dimensions
        .iter()
        .filter_map(|d| Some((d.reference?, d.id)))
        .collect();
    let mut uses = BTreeMap::new();
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

/// How many solves with the driven values of the last one a sketch whose
/// driving dimensions use its own driven ones takes before the values are
/// said to depend on themselves (mitcad#1661).
const OWN_REFERENCE_ROUNDS: usize = 8;

/// Whether a driven value measured again is the one a driving dimension
/// was solved with (mm, rad; relative above 1): solver noise, not a move.
fn same_value(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(1.0)
}

/// Solves `def` (mitcad#1661): at once when no driving dimension uses a
/// driven dimension of the sketch; otherwise in stages. First with those
/// driving dimensions (users) held as driven, so the driven dimensions they
/// use measure without them; then the whole sketch with the users' values
/// taken from the measured references, again with the values the last
/// solve measured until they no longer move (a chain of driven dimensions
/// and users settles in as many solves as it is long). A user that moves
/// what it uses (directly or through others) never settles: a circular
/// dependency, refused naming the dimensions. The reads recorded are those
/// of the last solve, its references at the measured values, which the
/// recompute then sets into them.
fn solve_with_own_references<K: Kernel>(
    def: &SpatialSketchDef,
    inputs: &mut SpatialInputs,
    projected: &[ProjectedFace<K::Shape>],
    frame: &SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> Result<SpatialSolved, SpatialFailure> {
    let uses = own_uses(def, ctx.env.params);
    if uses.is_empty() {
        let mut model = def.build_with(&mut |id| ctx.param(id), inputs)?;
        return solve_on_faces(&mut model, projected, frame, &mut inputs.surfaces, ctx);
    }
    let references: BTreeMap<ConstraintUid, ParamId> = def
        .dimensions
        .iter()
        .filter_map(|d| Some((d.id, d.reference?)))
        .collect();
    let mut held = def.clone();
    for d in held
        .dimensions
        .iter_mut()
        .filter(|d| uses.contains_key(&d.id))
    {
        d.value = None;
    }
    let mut solved = {
        let mut model = held.build_with(&mut |id| ctx.param(id), inputs)?;
        solve_on_faces(&mut model, projected, frame, &mut inputs.surfaces, ctx)?
    };
    let mut known: BTreeMap<ConstraintUid, f64> = BTreeMap::new();
    for _ in 0..OWN_REFERENCE_ROUNDS {
        for d in uses.values().flatten() {
            if let Some(v) = solved.status.measured.get(d) {
                known.insert(*d, *v);
            }
        }
        let mut values = ctx.env.params.clone();
        for (d, v) in &known {
            if let Some(id) = references.get(d) {
                values
                    .set_measured(*id, *v)
                    .map_err(|e| SpatialFailure::new(SpatialFailureKind::Value, e.to_string()))?;
            }
        }
        let mut reads: Vec<(ParamId, Option<u64>)> = Vec::new();
        let mut model = {
            let mut value = |id: ParamId| -> Result<f64, String> {
                reads.push((id, values.read_bits(id)));
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
            def.build_with(&mut value, inputs)?
        };
        solved = solve_on_faces(&mut model, projected, frame, &mut inputs.surfaces, ctx)?;
        let settled = known.iter().all(|(d, v)| {
            solved
                .status
                .measured
                .get(d)
                .is_some_and(|m| same_value(*m, *v))
        });
        if settled {
            for (id, bits) in reads {
                ctx.reads.push(Read::Param(id, bits));
            }
            return Ok(solved);
        }
    }
    let mut involved: Vec<ConstraintUid> = uses.keys().copied().collect();
    involved.extend(uses.values().flatten().copied());
    involved.sort_unstable();
    involved.dedup();
    Err(SpatialFailure::said(
        SpatialFailureKind::Value,
        "a driving dimension uses a driven dimension of the sketch that it moves itself (a \
         circular dependency)",
        format!(
            "{}: a driving dimension uses a driven dimension of the sketch that it moves itself \
             (a circular dependency)",
            involved
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        involved,
        Vec::new(),
    ))
}

/// Finds a 3D sketch's sources in `frame` and solves it.
fn solve_sketch<K: Kernel>(
    def: &SpatialSketchDef,
    frame: SketchFrame,
    ctx: &mut EvalContext<'_, K>,
) -> SpatialSketchOutput {
    let mut targets = BTreeMap::new();
    for (id, point, source, projected) in def.attachment_records() {
        match source_target(source, projected, &frame, ctx) {
            Ok(local) => {
                targets.insert(id, local);
            }
            Err(e) => {
                let failure = SpatialFailure {
                    kind: SpatialFailureKind::Reference,
                    message: about_source(
                        About::Attachment,
                        &format!("{id}: p{} attached to {source}: ", point.0),
                        &e,
                        source,
                    ),
                    constraints: vec![id],
                    entities: vec![point],
                };
                return SpatialSketchOutput::failed(Some(frame), def, failure);
            }
        }
    }
    let mut inputs = SpatialInputs {
        targets,
        ..SpatialInputs::default()
    };
    for (id, source, projected) in def.reference_records() {
        match source_curve(source, projected, &frame, ctx) {
            Ok(curve) => {
                inputs.curves.insert(id, curve);
            }
            Err(e) => {
                let failure = SpatialFailure {
                    kind: SpatialFailureKind::Reference,
                    message: about_source(
                        About::ReferenceCurve,
                        &format!("c{}: reference curve of {source}: ", id.0),
                        &e,
                        source,
                    ),
                    constraints: def.reference_users(id),
                    entities: vec![id],
                };
                return SpatialSketchOutput::failed(Some(frame), def, failure);
            }
        }
    }
    // Points on faces (mitcad#1549): each face found where it is now, its
    // surface in the local frame.
    let mut projected = Vec::new();
    for (id, point, source) in def.face_records() {
        let near = def.position(point).unwrap_or([0.0; 3]);
        let found = found_face(source, ctx).and_then(|found| {
            let (surface, by_projection) = face_surface(&found, &frame, near, ctx)?;
            Ok((found, surface, by_projection))
        });
        match found {
            Ok((found, surface, by_projection)) => {
                inputs.surfaces.insert(id, surface);
                if by_projection {
                    projected.push(ProjectedFace { id, point, found });
                }
            }
            Err(e) => {
                let failure = SpatialFailure {
                    kind: SpatialFailureKind::Reference,
                    message: about_source(
                        About::Face,
                        &format!("{id}: p{} on {source}: ", point.0),
                        &e,
                        source,
                    ),
                    constraints: vec![id],
                    entities: vec![point],
                };
                return SpatialSketchOutput::failed(Some(frame), def, failure);
            }
        }
    }
    // The planes of Symmetric and On Plane relations found outside the
    // sketch (mitcad#1661), in the local frame.
    for (id, plane) in def.plane_records() {
        match relation_plane(plane, &frame, ctx) {
            Ok(surface) => {
                inputs.surfaces.insert(id, surface);
            }
            Err(e) => {
                let failure = SpatialFailure {
                    kind: SpatialFailureKind::Reference,
                    message: FeatureError::from_text(&said_around(
                        |m| format!("the plane of a relation: {m}"),
                        &format!("{id}: {plane}: "),
                        &e,
                    ))
                    .with_places(plane.places())
                    .text(),
                    constraints: vec![id],
                    entities: def
                        .constraint(id)
                        .map(|c| c.kind.points())
                        .unwrap_or_default(),
                };
                return SpatialSketchOutput::failed(Some(frame), def, failure);
            }
        }
    }
    let solved = solve_with_own_references(def, &mut inputs, &projected, &frame, ctx);
    let curves = def.curves.iter().map(|c| (c.id, c.clone())).collect();
    match solved {
        Ok(solved) => {
            // Consistent redundancy is kept and said (it takes no freedom).
            // One warning in words (mitcad#1644): the relations' ids are
            // the agents', the sketch's status lists them.
            match solved.status.redundant.as_slice() {
                [] => {}
                [id] => ctx.warn(said(
                    "a relation is redundant: the other relations already hold what it holds",
                    format!("{id} is redundant: the other relations already hold what it holds"),
                )),
                ids => ctx.warn(said(
                    format!(
                        "{} relations are redundant: the other relations already hold what they \
                         hold",
                        ids.len()
                    ),
                    format!(
                        "{} are redundant: the other relations already hold what they hold",
                        ids.iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )),
            }

            SpatialSketchOutput {
                frame: Some(frame),
                curves,
                targets: inputs.targets,
                references: inputs.curves,
                surfaces: inputs.surfaces,
                solved: Some(solved),
                failure: None,
            }
        }
        Err(failure) => SpatialSketchOutput {
            frame: Some(frame),
            curves,
            targets: inputs.targets,
            references: inputs.curves,
            surfaces: inputs.surfaces,
            solved: None,
            failure: Some(failure),
        },
    }
}
