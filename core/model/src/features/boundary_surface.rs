// SPDX-License-Identifier: MIT
//! Boundary Surface (mitcad#455): one surface through curves, as a new
//! surface body of one face `<feature>:boundary`. Either `boundary`, a
//! closed loop of boundaries (one closed chain, or chains end to end; the
//! chains of sketch curves, of one sketch or several, joined in loop order
//! whatever order they are listed in, mitcad#2198) that the surface fills,
//! meeting the
//! face beside each chain of edges (or the plane of each chain of sketch
//! curves) with its continuity; or a network: curves `u` one way (two or
//! more, the surface lofted through them in order; the first or the last
//! may be a point) and `v` crossing them (none, or two or more: a Coons
//! patch of two each way, a Gordon surface through more). Each curve is a
//! chain of sketch curves, a chain of a body's edges (named, or free edges
//! of openings) or a point. The bodies are not changed.
//!
//! A loop may have `guides` (mitcad#2526): sketch curves inside it that
//! the filled face runs through (a bowl's depth inside its rim). The loop
//! alone bounds the face; the guides neither bound nor split it, and each
//! of their curves is a guide of its own (they need not join).

use std::marker::PhantomData;

use mitcad_model_base::curves::tolerance;
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
    /// A loop's guide curves inside it (mitcad#2526): sketch curves the
    /// face runs through, neither bounding nor splitting it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<BoundaryCurveRef>,
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
            guides: clone,
            values: phantom,
        }
    }

    fn all(&self) -> impl Iterator<Item = &BoundaryCurveRef> {
        self.boundary
            .iter()
            .chain(&self.u)
            .chain(&self.v)
            .chain(&self.guides)
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
        if !self.guides.is_empty() && self.boundary.is_empty() {
            return Err(
                "guide curves are for a boundary loop; a network runs through its curves"
                    .to_owned(),
            );
        }
        for guide in &self.guides {
            if guide.sketch.is_none() || guide.body.is_some() || guide.point.is_some() {
                return Err("a guide curve of a boundary loop is sketch curves".to_owned());
            }
            if guide.curves.is_empty() {
                return Err("a guide curve of a boundary loop needs its curves".to_owned());
            }
            if !guide.continuity.is_g0() {
                return Err(
                    "a guide curve has no continuity: the surface runs through it".to_owned(),
                );
            }
            for curve in &guide.curves {
                if self
                    .boundary
                    .iter()
                    .any(|b| b.sketch == guide.sketch && b.curves.contains(curve))
                {
                    return Err(format!(
                        "{} is both a boundary curve and a guide curve of the loop",
                        curve.curve_name()
                    ));
                }
            }
        }
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

/// A curve of a boundary loop read: sketch curves as the runs they make
/// (mitcad#2198: the loop may pass a sketch more than once, its curves
/// there in one entry), each a chain of its own; anything else as
/// [`read`] reads it.
fn read_loop<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    curve: &BoundaryCurveRef,
) -> Result<Vec<Read<K::Shape>>, String> {
    let Some(curves) = curve.sketch_curves() else {
        return Ok(vec![read(ctx, curve)?]);
    };
    let (frame, pieces) = curves.curves(ctx)?;
    if pieces.is_empty() {
        // Refused as a single chain is ("the path has no curves").
        path::chain(pieces)?;
        return Ok(Vec::new());
    }
    runs(pieces)
        .into_iter()
        .map(|run| {
            Ok(Read::Curves {
                chain: path::chain(run)?,
                origin: frame.origin,
                normal: frame.normal(),
                continuity: curve.continuity,
            })
        })
        .collect()
}

/// A loop's guide curves read (mitcad#2526): each entry's sketch curves
/// as they are listed, not joined (each curve is a guide of its own), on
/// its sketch's plane; a guide has no continuity.
fn read_guide<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    guide: &BoundaryCurveRef,
) -> Result<Read<K::Shape>, String> {
    let curves = guide
        .sketch_curves()
        .ok_or("a guide curve of a boundary loop is sketch curves")?;
    let (frame, pieces) = curves.curves(ctx)?;
    if pieces.is_empty() {
        return Err("a guide curve of a boundary loop needs its curves".to_owned());
    }
    Ok(Read::Curves {
        chain: pieces,
        origin: frame.origin,
        normal: frame.normal(),
        continuity: PatchContinuity::G0,
    })
}

/// The curves grouped into the runs they make, joined at their ends, in
/// the order of each run's first listed curve; a closed curve is a run by
/// itself.
fn runs(pieces: Vec<PathCurve>) -> Vec<Vec<PathCurve>> {
    let tolerance = tolerance(&pieces);
    let ends: Vec<Option<([f64; 3], [f64; 3])>> =
        pieces.iter().map(|p| path::curve_ends(&p.curve)).collect();
    let near = |p: [f64; 3], q: [f64; 3]| distance(p, q) <= tolerance;
    let touch = |i: usize, j: usize| match (ends[i], ends[j]) {
        (Some((a, b)), Some((c, d))) => near(a, c) || near(a, d) || near(b, c) || near(b, d),
        _ => false,
    };
    let mut run_of: Vec<Option<usize>> = vec![None; pieces.len()];
    let mut count = 0;
    for first in 0..pieces.len() {
        if run_of[first].is_some() {
            continue;
        }
        run_of[first] = Some(count);
        let mut stack = vec![first];
        while let Some(i) = stack.pop() {
            for (j, run) in run_of.iter_mut().enumerate() {
                if run.is_none() && touch(i, j) {
                    *run = Some(count);
                    stack.push(j);
                }
            }
        }
        count += 1;
    }
    let mut runs: Vec<Vec<PathCurve>> = vec![Vec::new(); count];
    for (piece, run) in pieces.into_iter().zip(run_of) {
        runs[run.expect("each curve in a run")].push(piece);
    }
    runs
}

fn distance(p: [f64; 3], q: [f64; 3]) -> f64 {
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

/// The two ends of a chain of curves in order (each running along it,
/// [`path::chain`]); none for a closed chain.
fn chain_ends(chain: &[PathCurve]) -> Option<([f64; 3], [f64; 3])> {
    let (start, _) = path::curve_ends(&chain.first()?.curve)?;
    let (_, end) = path::curve_ends(&chain.last()?.curve)?;
    Some((start, end))
}

/// Takes the items in `order` (each index once).
fn reorder<T>(items: Vec<T>, order: &[usize]) -> Vec<T> {
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order
        .iter()
        .map(|&i| slots[i].take().expect("each item once"))
        .collect()
}

/// A boundary loop in the order its chains join (mitcad#2198): the chains
/// of sketch curves, which may be of several sketches (planar and 3D) and
/// listed in any order, joined end to end. Without chains of edges they
/// must close into one loop (refused in words otherwise, naming the sketch
/// whose curves end with nothing going on); with one chain of edges, chains
/// of curves that run as one open path from end to end are put in its
/// order before the edges, which close the loop. Anything else (a closed
/// chain, several chains of edges, chains that join no other way) is kept
/// as listed, for the kernel to join. `sketches` names each entry's sketch
/// (empty for edges).
fn loop_order<S>(read: Vec<Read<S>>, sketches: &[String]) -> Result<Vec<Read<S>>, String> {
    let mut chains: Vec<usize> = Vec::new();
    let mut ends: Vec<([f64; 3], [f64; 3])> = Vec::new();
    let mut edges: Vec<usize> = Vec::new();
    for (i, r) in read.iter().enumerate() {
        match r {
            Read::Curves { chain, .. } => match chain_ends(chain) {
                Some(e) => {
                    chains.push(i);
                    ends.push(e);
                }
                None => return Ok(read),
            },
            Read::Edges(..) => edges.push(i),
            Read::Point(_) => return Ok(read),
        }
    }
    if chains.is_empty() || edges.len() > 1 {
        return Ok(read);
    }
    let size = ends
        .iter()
        .flat_map(|(a, b)| a.iter().chain(b))
        .fold(1.0_f64, |m, v| m.max(v.abs()));
    let tolerance = 1e-6 * size;
    let near = |p: [f64; 3], q: [f64; 3]| distance(p, q) <= tolerance;
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut node_of = |p: [f64; 3]| -> usize {
        if let Some(n) = nodes.iter().position(|q| near(*q, p)) {
            return n;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let links: Vec<(usize, usize)> = ends
        .iter()
        .map(|(a, b)| (node_of(*a), node_of(*b)))
        .collect();
    let mut degree = vec![0usize; nodes.len()];
    for (a, b) in &links {
        degree[*a] += 1;
        degree[*b] += 1;
    }
    let point = |n: usize| {
        let p = nodes[n];
        format!("({:.6}, {:.6}, {:.6})", p[0], p[1], p[2])
    };
    let free: Vec<usize> = (0..nodes.len()).filter(|n| degree[*n] == 1).collect();
    if edges.is_empty() {
        if let Some(&n) = free.first() {
            let chain = links
                .iter()
                .position(|(a, b)| *a == n || *b == n)
                .expect("a free end is a chain's");
            return Err(format!(
                "the boundary curves do not close into a loop: the curves of {} end at {} with no curve going on",
                sketches[chains[chain]],
                point(n)
            ));
        }
        if let Some(n) = (0..nodes.len()).find(|n| degree[*n] > 2) {
            return Err(format!(
                "the boundary curves do not make one loop: more than two of them meet at {}",
                point(n)
            ));
        }
    } else if free.len() != 2 || degree.iter().any(|d| *d > 2) {
        return Ok(read);
    }
    // Along the chains from the first one's start (a closed loop) or from
    // a free end (an open path the edges close).
    let mut node = free.first().copied().unwrap_or(links[0].0);
    let mut used = vec![false; links.len()];
    let mut order: Vec<usize> = Vec::new();
    while let Some(c) =
        (0..links.len()).find(|c| !used[*c] && (links[*c].0 == node || links[*c].1 == node))
    {
        used[c] = true;
        node = if links[c].0 == node {
            links[c].1
        } else {
            links[c].0
        };
        order.push(chains[c]);
    }
    if order.len() != chains.len() {
        if edges.is_empty() {
            return Err(
                "the boundary curves do not make one loop: they close into several".to_owned(),
            );
        }
        return Ok(read);
    }
    order.extend(edges);
    Ok(reorder(read, &order))
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

/// A curve of a network as agents read it: `Sketch2 c4`, `Sketch2 c4 c5
/// and its chain`, `edges E{…} of F2.b0`, `the point (5, 5, 10)`.
fn network_curve<K: Kernel>(ctx: &EvalContext<'_, K>, curve: &BoundaryCurveRef) -> String {
    if let Some([x, y, z]) = curve.point {
        return format!("the point ({x}, {y}, {z})");
    }
    if let Some(body) = curve.body {
        if curve.edges.is_empty() {
            return format!("free edges of {body}");
        }
        let edges: Vec<String> = curve.edges.iter().map(ToString::to_string).collect();
        return format!("edges {} of {body}", edges.join(", "));
    }
    let sketch = curve
        .sketch
        .map(|s| ctx.feature_name(s))
        .unwrap_or_default();
    let curves: Vec<String> = curve.curves.iter().map(|c| c.curve_name()).collect();
    let chain = if curve.chain { " and its chain" } else { "" };
    format!("{sketch} {}{chain}", curves.join(" "))
}

/// The network's curves by number as the kernel's messages count them
/// (`curve one way 2`, `crossing curve 1`, from 1), with their sketches
/// and curves (mitcad#2333): `curves one way: 1 Sketch1 c1, 2 Sketch1 c2;
/// crossing curves: 1 Sketch2 c1, 2 Sketch3 c1`.
fn network_legend<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    u: &[BoundaryCurveRef],
    v: &[BoundaryCurveRef],
) -> String {
    let side = |curves: &[BoundaryCurveRef]| {
        curves
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{} {}", i + 1, network_curve(ctx, c)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("curves one way: {}; crossing curves: {}", side(u), side(v))
}

/// Whether the kernel's failure has a detail for agents (the geometry
/// kernel's failures in words: a pair of the network's curves that does
/// not meet or curves no surface runs through, named by number).
fn names_network_curves(text: &str) -> bool {
    text.contains(crate::notice::PLACED_TAIL)
}

/// The kernel's failure `text` with the network's legend after its detail
/// (agents read which sketch's curve `curve one way 2` is); the message
/// and the places as they were.
fn with_legend(text: &str, legend: &str) -> String {
    let error = crate::notice::FeatureError::from_text(text);
    let detail = format!("{} ({legend})", error.agent_text());
    error.with_detail(detail).text()
}

impl<K: Kernel> Evaluate<K> for BoundarySurfaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        // A network's failure names its curves by number (mitcad#2333).
        let legend = (!self.v.is_empty()).then(|| network_legend(ctx, &self.u, &self.v));
        let read_all = |ctx: &mut EvalContext<'_, K>, curves: &[BoundaryCurveRef]| {
            curves
                .iter()
                .map(|c| read(ctx, c))
                .collect::<Result<Vec<_>, _>>()
        };
        // The loop's chains, each with its sketch's name (for messages).
        let mut boundary = Vec::new();
        let mut sketches: Vec<String> = Vec::new();
        for curve in &self.boundary {
            let name = curve
                .sketch
                .map(|s| ctx.feature_name(s))
                .unwrap_or_default();
            for r in read_loop(ctx, curve)? {
                boundary.push(r);
                sketches.push(name.clone());
            }
        }
        let boundary = loop_order(boundary, &sketches)?;
        let u = read_all(ctx, &self.u)?;
        let v = read_all(ctx, &self.v)?;
        let guides = self
            .guides
            .iter()
            .map(|g| read_guide(ctx, g))
            .collect::<Result<Vec<_>, _>>()?;
        let (boundary_openings, u_openings, v_openings, guide_openings) = (
            openings_of(&boundary),
            openings_of(&u),
            openings_of(&v),
            openings_of(&guides),
        );
        let boundary = curves_of(&boundary, &boundary_openings);
        let u = curves_of(&u, &u_openings);
        let v = curves_of(&v, &v_openings);
        let guides = curves_of(&guides, &guide_openings);
        let spec = BoundarySpec {
            feature: ctx.uid,
            curves: if boundary.is_empty() {
                BoundaryCurves::Network { u: &u, v: &v }
            } else {
                BoundaryCurves::Loop {
                    boundary: &boundary,
                    guides: &guides,
                }
            },
        };
        let surface = ctx
            .kernel
            .boundary_surface(&spec)
            .map_err(kernel_error)
            .map_err(|text| match &legend {
                Some(legend) if names_network_curves(&text) => with_legend(&text, legend),
                _ => text,
            })?;
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
