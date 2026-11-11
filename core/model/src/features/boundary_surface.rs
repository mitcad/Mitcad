// SPDX-License-Identifier: MIT
//! Boundary Surface (mitcad#455): one surface through curves, as a new
//! surface body of one face `<feature>:boundary`. Either `boundary`, a
//! closed loop of boundaries in order (one closed chain, or chains end to
//! end) that the surface fills, meeting the
//! face beside each chain of edges (or the plane of each chain of sketch
//! curves) with its continuity; or a network: curves `u` one way (two or
//! more, the surface lofted through them in order; the first or the last
//! may be a point) and `v` crossing them (none, or two or more: a Coons
//! patch of two each way, a Gordon surface through more). Each curve is a
//! chain of sketch curves, a chain of a body's edges (named, or free edges
//! of openings) or a point. The bodies are not changed.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::face_refs::kernel_error;
use super::mapping::map_params;
use super::path::{self, SketchCurves};
use super::sheet_edges::{self, OpeningEdgesRef, ResolvedChain};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::surface_modelling::{
    BodyEdges, BoundaryCurve, BoundaryCurves, BoundarySpec, OpeningEdges,
};
use crate::surfaces::PatchContinuity;
use crate::sweeps::PathCurve;
use crate::topo::{EdgeName, FaceName};

/// Has no values (its points are fixed); `P` only matches the other
/// feature types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundarySurfaceDef<P = ParamId> {
    /// A closed loop of boundaries, in order, that the surface fills.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary: Vec<BoundaryCurveRef>,
    /// Or the network's curves one way, lofted through in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub u: Vec<BoundaryCurveRef>,
    /// The network's curves crossing `u`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub v: Vec<BoundaryCurveRef>,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

/// One curve of a boundary surface: sketch curves (`sketch`, `curves`,
/// `chain`), a body's edges (`body`, `edges`, `openings`, `face`) or a
/// `point`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryCurveRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<FeatureUid>,
    #[serde(
        default,
        with = "super::path::curves_serde",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub curves: Vec<EntityUid>,
    /// Adds the curves joined to them end to end.
    #[serde(default, skip_serializing_if = "is_false")]
    pub chain: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub openings: Vec<OpeningEdgesRef>,
    /// The face beside the edges where an edge lies between two faces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<FaceName>,
    /// A side collapsed to a point (mm, the component's coordinates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<[f64; 3]>,
    /// How a boundary loop meets the face beside the edges (or the
    /// sketch's plane along sketch curves).
    #[serde(default, skip_serializing_if = "PatchContinuity::is_g0")]
    pub continuity: PatchContinuity,
}

impl<P> BoundarySurfaceDef<P> {
    pub const TYPE: &'static str = "boundary_surface";
    pub const BASE_NAME: &'static str = "BoundarySurface";

    map_params! {
        BoundarySurfaceDef {
            boundary: clone,
            u: clone,
            v: clone,
            values: phantom,
        }
    }

    fn all(&self) -> impl Iterator<Item = &BoundaryCurveRef> {
        self.boundary.iter().chain(&self.u).chain(&self.v)
    }
}

impl BoundaryCurveRef {
    fn sketch_curves(&self) -> Option<SketchCurves> {
        self.sketch.map(|sketch| SketchCurves {
            sketch,
            curves: self.curves.clone(),
            chain: self.chain,
            reversed: false,
        })
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let kinds = usize::from(self.sketch.is_some())
            + usize::from(self.body.is_some())
            + usize::from(self.point.is_some());
        if kinds != 1 {
            return Err(
                "a boundary curve is sketch curves, a body's edges or a point, one of them"
                    .to_owned(),
            );
        }
        // Curves of a planar or a 3D sketch (mitcad#1550).
        if let Some(curves) = self.sketch_curves() {
            curves.check_any(ctx)?;
        } else if !self.curves.is_empty() || self.chain {
            return Err("sketch curves of a boundary need their sketch".to_owned());
        }
        match self.body {
            Some(body) => {
                sheet_edges::check_chain(ctx, body, &self.edges, &self.openings, &self.face)?
            }
            None if !self.edges.is_empty() || !self.openings.is_empty() || self.face.is_some() => {
                return Err("edges of a boundary need their body".to_owned());
            }
            None => {}
        }
        if let Some(point) = self.point {
            if !point.iter().all(|v| v.is_finite()) {
                return Err(format!("a boundary point is not finite: {point:?}"));
            }
            if !self.continuity.is_g0() {
                return Err("a boundary point has no continuity".to_owned());
            }
        }
        Ok(())
    }
}

impl FeatureInfo for BoundarySurfaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for curve in self.all() {
            if let Some(sketch) = curve.sketch {
                references.features.insert(sketch);
            }
            if let Some(body) = curve.body {
                sheet_edges::add_chain_references(
                    &mut references,
                    body,
                    &curve.edges,
                    &curve.openings,
                    &curve.face,
                );
            }
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if !self.boundary.is_empty() {
            if !self.u.is_empty() || !self.v.is_empty() {
                return Err(
                    "a boundary surface fills a loop or runs through a network, not both"
                        .to_owned(),
                );
            }
            if self.boundary.iter().any(|c| c.point.is_some()) {
                return Err("a boundary loop is made of curves, not points".to_owned());
            }
        } else {
            if self.u.len() < 2 {
                return Err(
                    "a boundary surface needs a loop of curves, or two curves or more one way"
                        .to_owned(),
                );
            }
            if self.v.len() == 1 {
                return Err("the crossing curves must be two or more".to_owned());
            }
            if self.all().any(|c| !c.continuity.is_g0()) {
                return Err(
                    "continuity is for the curves of a boundary loop; a network runs through its curves"
                        .to_owned(),
                );
            }
            let last = self.u.len() - 1;
            for (i, curve) in self.u.iter().enumerate() {
                if curve.point.is_some() && ((i != 0 && i != last) || !self.v.is_empty()) {
                    return Err(
                        "a point can only be the first or the last of the curves one way, without crossing curves"
                            .to_owned(),
                    );
                }
            }
            if self.v.iter().any(|c| c.point.is_some()) {
                return Err("the crossing curves cannot be points".to_owned());
            }
        }
        for curve in self.all() {
            curve.check(ctx)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

/// A boundary curve with its inputs read.
enum Read<S> {
    Curves {
        chain: Vec<PathCurve>,
        origin: [f64; 3],
        normal: [f64; 3],
        continuity: PatchContinuity,
    },
    Edges(ResolvedChain<S>, PatchContinuity),
    Point([f64; 3]),
}

fn read<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    curve: &BoundaryCurveRef,
) -> Result<Read<K::Shape>, String> {
    if let Some(point) = curve.point {
        return Ok(Read::Point(point));
    }
    if let Some(body) = curve.body {
        let chain =
            sheet_edges::resolve_chain(ctx, body, &curve.edges, &curve.openings, &curve.face)?;
        return Ok(Read::Edges(chain, curve.continuity));
    }
    let curves = curve
        .sketch_curves()
        .ok_or("a boundary curve has no curves")?;
    let (frame, pieces) = curves.curves(ctx)?;
    Ok(Read::Curves {
        chain: path::chain(pieces)?,
        origin: frame.origin,
        normal: frame.normal(),
        continuity: curve.continuity,
    })
}

/// The openings of each read curve, as the kernel takes them.
fn openings_of<S>(read: &[Read<S>]) -> Vec<Vec<OpeningEdges<'_>>> {
    read.iter()
        .map(|r| match r {
            Read::Edges(chain, _) => chain.opening_edges(),
            _ => Vec::new(),
        })
        .collect()
}

fn curves_of<'a, S>(
    read: &'a [Read<S>],
    openings: &'a [Vec<OpeningEdges<'a>>],
) -> Vec<BoundaryCurve<'a, S>> {
    read.iter()
        .zip(openings)
        .map(|(r, openings)| match r {
            Read::Curves {
                chain,
                origin,
                normal,
                continuity,
            } => BoundaryCurve::Curves {
                chain,
                origin: *origin,
                normal: *normal,
                continuity: *continuity,
            },
            Read::Edges(chain, continuity) => BoundaryCurve::Edges {
                edges: BodyEdges {
                    body: &chain.shape,
                    edges: &chain.edges,
                    openings,
                    face: chain.face.as_ref(),
                },
                continuity: *continuity,
            },
            Read::Point(point) => BoundaryCurve::Point(*point),
        })
        .collect()
}

impl<K: Kernel> Evaluate<K> for BoundarySurfaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let read_all = |ctx: &mut EvalContext<'_, K>, curves: &[BoundaryCurveRef]| {
            curves
                .iter()
                .map(|c| read(ctx, c))
                .collect::<Result<Vec<_>, _>>()
        };
        let boundary = read_all(ctx, &self.boundary)?;
        let u = read_all(ctx, &self.u)?;
        let v = read_all(ctx, &self.v)?;
        let (boundary_openings, u_openings, v_openings) =
            (openings_of(&boundary), openings_of(&u), openings_of(&v));
        let boundary = curves_of(&boundary, &boundary_openings);
        let u = curves_of(&u, &u_openings);
        let v = curves_of(&v, &v_openings);
        let spec = BoundarySpec {
            feature: ctx.uid,
            curves: if boundary.is_empty() {
                BoundaryCurves::Network { u: &u, v: &v }
            } else {
                BoundaryCurves::Loop(&boundary)
            },
        };
        let surface = ctx.kernel.boundary_surface(&spec).map_err(kernel_error)?;
        let mut output = FeatureOutput {
            tool: Some(surface.clone()),
            ..FeatureOutput::default()
        };
        output
            .changes
            .push(BodyChange::Set(ctx.new_body(), surface));
        Ok(output)
    }
}
