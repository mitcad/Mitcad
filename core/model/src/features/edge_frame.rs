// SPDX-License-Identifier: MIT
//! Edge frame (mitcad#1814; `commands.md`, "BUILD: timber construction",
//! *Members along an edge*): two frames along a straight edge of a body
//! lying on a planar face beside it, named `EdgeFrame<n>`. BUILD's Timber
//! places a member along an edge on them: the member's joint is on the
//! `start` frame and its bound length runs from `start` to the plane of
//! the `end` frame, so that the member follows the edge and the face when
//! the body changes (its length, width or thickness).
//!
//! The edge and the face are geometry of the component the occurrence path
//! ends in (empty: the feature's own component), taken into the feature's
//! component through the occurrences' placements at this point of the
//! timeline, as a bound length's are. Both frames have their z axis along
//! the edge, from its start to its end, their x axis across the face away
//! from the edge (into the face) and their y axis the face's outward normal.
//! The edge's start is the end that makes them right-handed: looking along
//! the edge with the face's normal up, the face lies on the left. The edge
//! is projected onto the face's plane.
//!
//! - `inset`: across the face, away from the edge (0: at the edge, negative
//!   out over it);
//! - `start`, `end`: along the edge from each of its ends (positive
//!   shortens, negative runs past the end);
//! - `lift`: off the face along its normal (0: on it).
//!
//! A curved edge, a face that is not planar, an edge that does not run
//! along the face and offsets that leave nothing of the edge fail the
//! feature in words; a lost reference fails it as other features' lost
//! references do.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::bound_length::{InComponent, path_placement};
use super::geom_ref::{GeomRef, Resolver, Want};
use super::joint::OccurrencePath;
use super::mapping::map_params;
use super::pattern::none;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use crate::kernel::Kernel;
use crate::notice::{NoticePlace, about_place, placed, said};
use crate::parameters::ParamId;
use crate::transform::Transform;
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

/// The name of the frame at the edge's start (after `start`).
pub const START: &str = "start";
/// The name of the frame at the edge's end (before `end`).
pub const END: &str = "end";
/// The names of an edge frame's mount frames.
pub const MOUNTS: [&str; 2] = [START, END];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeFrameDef<P = ParamId> {
    /// A path from the feature's component to the component the edge and
    /// the face are in; empty for its own.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    /// A straight edge.
    pub edge: GeomRef,
    /// The planar face the frames lie on.
    pub face: GeomRef,
    /// Across the face, away from the edge, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub inset: Option<P>,
    /// Along the edge from its start, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub start: Option<P>,
    /// Along the edge from its end, back towards the start, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub end: Option<P>,
    /// Off the face along its normal, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub lift: Option<P>,
}

impl<P> EdgeFrameDef<P> {
    pub const TYPE: &'static str = "edge_frame";
    pub const BASE_NAME: &'static str = "EdgeFrame";

    map_params! {
        EdgeFrameDef {
            occurrence: clone,
            edge: clone,
            face: clone,
            inset: option,
            start: option,
            end: option,
            lift: option,
        }
    }
}

impl FeatureInfo for EdgeFrameDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.edge.add_to(&mut references);
        self.face.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match &self.edge {
            GeomRef::Edge { .. } => self
                .edge
                .check(ctx, Want::Line)
                .map_err(|e| format!("edge: {e}"))?,
            other => {
                return Err(said(
                    "an edge frame runs along an edge: pick a straight edge",
                    format!("edge: {other} is no edge"),
                ));
            }
        }
        match &self.face {
            GeomRef::Face { .. } => self
                .face
                .check(ctx, Want::Face)
                .map_err(|e| format!("face: {e}"))?,
            other => {
                return Err(said(
                    "an edge frame lies on a face: pick the flat face beside the edge",
                    format!("face: {other} is no face"),
                ));
            }
        }
        Ok(())
    }
}

/// The offsets of an edge frame, in mm.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeOffsets {
    pub inset: f64,
    pub start: f64,
    pub end: f64,
    pub lift: f64,
}

/// An edge frame worked out: its two frames (see the module) in the
/// coordinates of the component the edge and the face are in, and their
/// distance along the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeRun {
    pub start: Transform,
    pub end: Transform,
    pub length: f64,
    /// The whole edge's length along it.
    pub edge_length: f64,
}

/// Works out an edge frame's frames from a straight edge and a planar face
/// (see the module), in the coordinates of their component. Shared by the
/// feature's evaluation and BUILD's planning, so that a preview is where
/// the placed member goes.
pub fn edge_run<R: Resolver>(
    r: &mut R,
    edge: &GeomRef,
    face: &GeomRef,
    offsets: &EdgeOffsets,
) -> Result<EdgeRun, String> {
    let GeomRef::Face {
        body: face_body,
        face: face_name,
    } = face
    else {
        return Err(said(
            "a member along an edge lies on a face: pick the flat face beside the edge",
            format!("face: {face} is no face"),
        ));
    };
    let GeomRef::Edge {
        body: edge_body,
        edge: edge_name,
    } = edge
    else {
        return Err(said(
            "a member along an edge needs a straight edge: pick one",
            format!("edge: {edge} is no edge"),
        ));
    };
    // The face's plane and middle.
    let shape = r.shape(*face_body)?;
    let found_face = crate::features::renamed::face_for(r, &shape, face_name)?;
    let face_place = || NoticePlace::face(found_face.to_string()).in_body(&face_body.to_string());
    let (origin, normal) = match r.kernel().face_geometry(&shape, &found_face) {
        Ok(SurfaceGeometry::Plane { origin, normal }) => (origin, normal),
        Ok(other) => {
            return Err(placed(
                "the face is not flat: a member along an edge lies on a flat face",
                face_place(),
                format!("face: {found_face} is not planar ({})", other.kind()),
            ));
        }
        // A kernel that tells only the plane of a face.
        Err(e) => match r.kernel().face_plane(&shape, &found_face) {
            Ok(plane) => (plane.origin, plane.normal),
            Err(_) => return Err(about_place(face_place(), &e.to_string())),
        },
    };
    let middle = r
        .kernel()
        .face_plane(&shape, &found_face)
        .map_or(origin, |p| p.origin);
    let n = unit(normal).ok_or_else(|| {
        placed(
            "the face has no normal",
            face_place(),
            format!("face: {found_face} has no normal"),
        )
    })?;
    // The edge, straight.
    let shape = r.shape(*edge_body)?;
    let found_edge = crate::features::renamed::edge_for(r, &shape, edge_name)?;
    let edge_place = || NoticePlace::edge(found_edge.to_string()).in_body(&edge_body.to_string());
    let (a, b) = match r.kernel().edge_geometry(&shape, &found_edge) {
        Ok(CurveGeometry::Line { start, end }) => (start, end),
        Ok(_) => {
            return Err(placed(
                "the edge is curved: a member along an edge needs a straight edge",
                edge_place(),
                format!("edge: {found_edge} is not straight"),
            ));
        }
        Err(e) => return Err(about_place(edge_place(), &e.to_string())),
    };
    let along = unit(sub(b, a)).ok_or_else(|| {
        placed(
            "the edge has no length",
            edge_place(),
            format!("edge: {found_edge} has no length"),
        )
    })?;
    if dot(along, n).abs() > 1e-6 {
        return Err(placed(
            "the edge does not run along the face: pick an edge of the face's plane",
            edge_place(),
            format!("edge: {found_edge} is not parallel to face {found_face}"),
        ));
    }
    // Onto the face's plane.
    let onto = |p: Vec3| sub(p, scale(n, dot(sub(p, origin), n)));
    let (a, b) = (onto(a), onto(b));
    // The face lies on the left looking from the start along the edge, its
    // normal up: x (into the face) = n × z.
    let mut z = along;
    let mut x = cross(n, z);
    let (mut p0, mut p1) = (a, b);
    let mid = scale(add(a, b), 0.5);
    if dot(x, sub(middle, mid)) < 0.0 {
        z = scale(z, -1.0);
        x = scale(x, -1.0);
        std::mem::swap(&mut p0, &mut p1);
    }
    let edge_length = norm(sub(p1, p0));
    let length = edge_length - offsets.start - offsets.end;
    if length.is_nan() || length <= 1e-6 {
        return Err(said(
            "the offsets at the ends leave nothing of the edge: make Start and End shorter",
            format!(
                "the edge is {} mm long; start {} and end {} leave {} mm",
                crate::build::round(edge_length),
                crate::build::round(offsets.start),
                crate::build::round(offsets.end),
                crate::build::round(length)
            ),
        ));
    }
    let across = add(scale(x, offsets.inset), scale(n, offsets.lift));
    let start = crate::joints::frame(add(add(p0, scale(z, offsets.start)), across), x, z);
    let end = crate::joints::frame(add(sub(p1, scale(z, offsets.end)), across), x, z);
    Ok(EdgeRun {
        start,
        end,
        length,
        edge_length,
    })
}

impl EdgeFrameDef {
    /// The offsets' values.
    fn offsets<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<EdgeOffsets, String> {
        let mut value = |id: Option<ParamId>| match id {
            Some(id) => ctx.param(id),
            None => Ok(0.0),
        };
        Ok(EdgeOffsets {
            inset: value(self.inset)?,
            start: value(self.start)?,
            end: value(self.end)?,
            lift: value(self.lift)?,
        })
    }
}

impl<K: Kernel> Evaluate<K> for EdgeFrameDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let offsets = self.offsets(ctx)?;
        let (at, placement) = path_placement(ctx, &self.occurrence)?;
        let run = {
            let mut resolver = InComponent {
                ctx: &mut *ctx,
                component: at,
            };
            edge_run(&mut resolver, &self.edge, &self.face, &offsets)?
        };
        let mut mounts = BTreeMap::new();
        mounts.insert(START.to_owned(), placement.after(&run.start));
        mounts.insert(END.to_owned(), placement.after(&run.end));
        Ok(FeatureOutput {
            mounts,
            ..FeatureOutput::default()
        })
    }
}
