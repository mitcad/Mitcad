// SPDX-License-Identifier: MIT
//! Finger Joint (mitcad#1939; commands.md "Finger Joint"): laser-sheet
//! plates that meet at an edge get alternating fingers along it, each
//! plate's fingers in the other's slots.
//!
//! The plates are bodies whose faces lie along the component's axes (as a
//! box of plates is drawn). Two plates meet where their boxes overlap (a
//! corner prism, each plate drawn to the outside of the joint) or touch
//! (one plate's end against the other's face). The joint's region is the
//! overlap, or for touching plates the end's footprint taken through the
//! other plate's thickness; it runs along its longest side. That length is
//! divided into fingers of `finger_width` (default 2.5 times the joint's
//! thickness): the odd count nearest to the length over the width (the
//! larger on a tie), or `count` when given; the first plate (the second
//! with `flip`) takes the fingers at both ends of an odd count. `end_margin`
//! keeps that much at each end whole for the first plate.
//!
//! Each plate's fingers keep their size; the other plate's slots are wider
//! by `clearance` (default 0.1 mm) on each side along the joint, so that
//! every finger has the clearance on both sides in its slot. Touching
//! plates: the end's plate grows its fingers into the other plate, which
//! loses the slots. Where three plates meet (a box's corner), the part of
//! the joint inside the third plate is not divided: it stays with the
//! plate listed first among the three and the others lose it.
//!
//! With two bodies the joint is theirs; with more, every pair that meets
//! is joined. Bodies keep their ids. Refused with the reason: fewer than
//! two bodies, plates that do not meet, a count that is not a whole number,
//! fingers narrower than twice the clearance, a plate split by its slots.

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{
    BooleanOp, BoundingBox, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{RegionKey, SegmentKey};

/// The default clearance per side, mm.
pub const DEFAULT_CLEARANCE: f64 = 0.1;
/// The most fingers along one joint.
pub const MAX_FINGERS: f64 = 1000.0;
/// Below this, two plates' boxes do not overlap or touch, mm.
const SLACK: f64 = 1e-6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct FingerJointDef<P = ParamId> {
    /// The plates: two, or every pair that meets among more.
    pub bodies: Vec<BodyUid>,
    /// Default 2.5 times the joint's thickness.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub finger_width: Option<P>,
    /// How many fingers along each joint; by the width when left out.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub count: Option<P>,
    /// Per side; default 0.1 mm.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
    /// Kept whole at each end of a joint for the first plate; default 0.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub end_margin: Option<P>,
    /// The second plate of each pair takes the end fingers.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

impl<P> FingerJointDef<P> {
    pub const TYPE: &'static str = "finger_joint";
    pub const BASE_NAME: &'static str = "FingerJoint";

    map_params! {
        FingerJointDef {
            bodies: clone,
            finger_width: option,
            count: option,
            clearance: option,
            end_margin: option,
            flip: copy,
        }
    }
}

/// A joint's sizes, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FingerSizes {
    pub finger_width: Option<f64>,
    pub count: Option<usize>,
    pub clearance: f64,
    pub end_margin: f64,
    pub flip: bool,
}

impl FingerSizes {
    fn new(
        def: &FingerJointDef,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<Self, String> {
        let mut optional = |id: Option<ParamId>| -> Result<Option<f64>, String> {
            id.map(&mut *value).transpose()
        };
        let finger_width = optional(def.finger_width)?;
        let count = optional(def.count)?;
        let clearance = optional(def.clearance)?.unwrap_or(DEFAULT_CLEARANCE);
        let end_margin = optional(def.end_margin)?.unwrap_or(0.0);
        if let Some(width) = finger_width
            && (!width.is_finite() || width <= 0.0)
        {
            return Err(format!(
                "the finger width must be greater than zero, got {width}"
            ));
        }
        let count = match count {
            None => None,
            Some(n) => {
                if !n.is_finite()
                    || (n - n.round()).abs() > 1e-9
                    || !(1.0..=MAX_FINGERS).contains(&n.round())
                {
                    return Err(mitcad_model_base::internal::parameters::count_error(
                        format!(
                            "the count of fingers must be a whole number from 1 to {MAX_FINGERS}, got {n}"
                        ),
                        n,
                    ));
                }
                Some(n.round() as usize)
            }
        };
        if !clearance.is_finite() || clearance < 0.0 {
            return Err(format!(
                "the clearance must not be negative, got {clearance}"
            ));
        }
        if !end_margin.is_finite() || end_margin < 0.0 {
            return Err(format!(
                "the end margin must not be negative, got {end_margin}"
            ));
        }
        Ok(Self {
            finger_width,
            count,
            clearance,
            end_margin,
            flip: def.flip,
        })
    }
}

/// What one plate does with one box of a joint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FingerCut {
    /// The plate loses the box (a slot).
    Cut,
    /// The plate gains the box (a finger grown into the other plate).
    Join,
}

/// One box of a planned joint: the plate (an index of the boxes given)
/// and what it does with the box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FingerBox {
    pub plate: usize,
    pub op: FingerCut,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

/// The joint of two plates whose boxes meet: its region, the axis it runs
/// along and, for touching plates, which plate's end touches.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Meeting {
    min: [f64; 3],
    max: [f64; 3],
    along: usize,
    /// The plate whose end touches the other's face; None where they
    /// overlap.
    end: Option<usize>,
}

fn meeting(boxes: &[BoundingBox], a: usize, b: usize) -> Option<Meeting> {
    let (p, q) = (&boxes[a], &boxes[b]);
    let min: [f64; 3] = std::array::from_fn(|i| p.min[i].max(q.min[i]));
    let max: [f64; 3] = std::array::from_fn(|i| p.max[i].min(q.max[i]));
    let extent: [f64; 3] = std::array::from_fn(|i| max[i] - min[i]);
    if extent.iter().any(|e| *e < -SLACK) {
        return None;
    }
    let touching: Vec<usize> = (0..3).filter(|i| extent[*i] <= SLACK).collect();
    match touching.len() {
        0 => {
            let along = longest(extent);
            Some(Meeting {
                min,
                max,
                along,
                end: None,
            })
        }
        1 => {
            // The plate thinner across the touching plane is the one whose
            // face is touched; the other's end touches it, and the joint
            // runs through the face's plate.
            let n = touching[0];
            let thick = |b: &BoundingBox| b.max[n] - b.min[n];
            let (end, face) = if thick(p) >= thick(q) { (a, b) } else { (b, a) };
            let mut min = min;
            let mut max = max;
            min[n] = boxes[face].min[n];
            max[n] = boxes[face].max[n];
            let mut along_extent = extent;
            along_extent[n] = 0.0;
            Some(Meeting {
                min,
                max,
                along: longest(along_extent),
                end: Some(end),
            })
        }
        _ => None,
    }
}

fn longest(extent: [f64; 3]) -> usize {
    (0..3)
        .max_by(|a, b| extent[*a].total_cmp(&extent[*b]))
        .unwrap_or(0)
}

/// Whether two boxes overlap with material (positive extent) on every
/// axis.
fn overlaps(a: [f64; 3], b: [f64; 3], c: &BoundingBox) -> bool {
    (0..3).all(|i| a[i].max(c.min[i]) < b[i].min(c.max[i]) - SLACK)
}

/// The odd count nearest to `ratio`, the larger on a tie, at least 1.
pub fn odd_count(ratio: f64) -> usize {
    2 * (ratio / 2.0).floor().max(0.0) as usize + 1
}

/// The boxes of every joint among plates whose axis-aligned boxes are
/// `boxes`, in the order of the pairs (with two plates, theirs; with more,
/// every pair that meets). Refused in words when no pair meets or the
/// fingers are too narrow for the clearance.
pub fn plan(boxes: &[BoundingBox], sizes: &FingerSizes) -> Result<Vec<FingerBox>, String> {
    let mut out = Vec::new();
    let mut joints = 0;
    for a in 0..boxes.len() {
        for b in a + 1..boxes.len() {
            let Some(m) = meeting(boxes, a, b) else {
                continue;
            };
            joints += 1;
            plan_pair(boxes, a, b, &m, sizes, &mut out)?;
        }
    }
    if joints == 0 {
        return Err(if boxes.len() == 2 {
            "the two plates do not meet: a finger joint needs plates that overlap at their \
             edge or touch end to face"
                .into()
        } else {
            "none of the plates meet: a finger joint needs plates that overlap at their edges \
             or touch end to face"
                .into()
        });
    }
    Ok(out)
}

fn plan_pair(
    boxes: &[BoundingBox],
    a: usize,
    b: usize,
    m: &Meeting,
    sizes: &FingerSizes,
    out: &mut Vec<FingerBox>,
) -> Result<(), String> {
    let l = m.along;
    let (first, second) = if sizes.flip { (b, a) } else { (a, b) };
    let span = |lo: f64, hi: f64| {
        let mut min = m.min;
        let mut max = m.max;
        min[l] = lo;
        max[l] = hi;
        (min, max)
    };
    // The parts of the joint inside a third plate's box, at its ends: kept
    // by the plate listed first among the three.
    let mut lo = m.min[l];
    let mut hi = m.max[l];
    for (k, other) in boxes.iter().enumerate() {
        if k == a || k == b || !overlaps(m.min, m.max, other) {
            continue;
        }
        let (from, to) = (other.min[l].max(m.min[l]), other.max[l].min(m.max[l]));
        let owner = a.min(b).min(k);
        let (min, max) = span(from, to);
        for plate in [a, b] {
            if plate != owner && m.end != Some(plate) {
                out.push(FingerBox {
                    plate,
                    op: FingerCut::Cut,
                    min,
                    max,
                });
            }
        }
        if let Some(end) = m.end
            && owner != k
        {
            let face = if end == a { b } else { a };
            if owner == end {
                out.push(FingerBox {
                    plate: end,
                    op: FingerCut::Join,
                    min,
                    max,
                });
                out.push(FingerBox {
                    plate: face,
                    op: FingerCut::Cut,
                    min,
                    max,
                });
            }
        }
        if (from - m.min[l]).abs() <= SLACK {
            lo = lo.max(to);
        } else if (to - m.max[l]).abs() <= SLACK {
            hi = hi.min(from);
        }
    }
    let margin = sizes.end_margin;
    let (start, end) = (lo + margin, hi - margin);
    let length = end - start;
    if length <= SLACK {
        return Ok(());
    }
    let thickness = (0..3)
        .filter(|i| *i != l)
        .map(|i| m.max[i] - m.min[i])
        .fold(f64::INFINITY, f64::min);
    let width = sizes.finger_width.unwrap_or(2.5 * thickness);
    let n = sizes.count.unwrap_or_else(|| odd_count(length / width));
    let step = length / n as f64;
    let c = sizes.clearance;
    if step <= 2.0 * c + SLACK {
        return Err(format!(
            "the fingers ({} mm) are too narrow for the clearance of {} mm on each side",
            super::bearing_sizes::short_number(step),
            super::bearing_sizes::short_number(c)
        ));
    }
    // The margins at the ends are the first plate's.
    let mut segments: Vec<(f64, f64, usize)> = Vec::new();
    if margin > 0.0 {
        segments.push((lo, start, first));
    }
    for k in 0..n {
        let owner = if k % 2 == 0 { first } else { second };
        segments.push((
            start + k as f64 * step,
            start + (k + 1) as f64 * step,
            owner,
        ));
    }
    if margin > 0.0 {
        segments.push((end, hi, first));
    }
    for (from, to, owner) in segments {
        let other = if owner == a { b } else { a };
        match m.end {
            // Overlapping plates: the owner keeps the segment; the other
            // loses it, wider by the clearance where its own finger is the
            // first plate's slot (the first plate's slots are exact).
            None => {
                let grow = if owner == first { c } else { 0.0 };
                let (min, max) = span((from - grow).max(m.min[l]), (to + grow).min(m.max[l]));
                out.push(FingerBox {
                    plate: other,
                    op: FingerCut::Cut,
                    min,
                    max,
                });
            }
            // Touching plates: the end's plate grows its fingers into the
            // face's plate, which loses them with the clearance; the face's
            // own segments stay as they are.
            Some(end) => {
                if owner == end {
                    let (min, max) = span(from, to);
                    out.push(FingerBox {
                        plate: end,
                        op: FingerCut::Join,
                        min,
                        max,
                    });
                    let (min, max) = span((from - c).max(m.min[l]), (to + c).min(m.max[l]));
                    out.push(FingerBox {
                        plate: other,
                        op: FingerCut::Cut,
                        min,
                        max,
                    });
                }
            }
        }
    }
    Ok(())
}

impl FeatureInfo for FingerJointDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.bodies {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.bodies.len() < 2 {
            return Err("a finger joint needs at least two plates".into());
        }
        for (i, body) in self.bodies.iter().enumerate() {
            ctx.body(*body)?;
            if self.bodies[..i].contains(body) {
                return Err(format!("the plate {body} is listed twice"));
            }
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        FingerSizes::new(self, &mut |id| Ok(value(id))).map(|_| ())
    }
}

impl<K: Kernel> Evaluate<K> for FingerJointDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let sizes = FingerSizes::new(self, &mut |id| ctx.param(id))?;
        let mut shapes = Vec::with_capacity(self.bodies.len());
        let mut boxes = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            let shape = ctx.surface_body(*body)?;
            let bounds = ctx
                .kernel
                .bounding_box(&shape)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("the plate {body} is empty"))?;
            shapes.push(shape);
            boxes.push(bounds);
        }
        // Plates along the component's axes are planned exactly from their
        // boxes; plates at other angles from their slabs (mitcad#1939).
        // (A plate whose slab cannot be read is planned from its box.)
        let slabs: Option<Vec<Slab>> = self
            .bodies
            .iter()
            .zip(&shapes)
            .map(|(body, shape)| slab(ctx, *body, shape).ok())
            .collect();
        let slabs = slabs.filter(|s| !s.iter().all(Slab::along_axes));
        let Some(slabs) = slabs else {
            let planned = plan(&boxes, &sizes)?;
            let changes = apply_boxes(ctx, &self.bodies, shapes, &planned, "finger")?;
            return Ok(FeatureOutput {
                changes,
                ..FeatureOutput::default()
            });
        };
        let cuts = oriented(ctx, &shapes, &slabs, &sizes)?;
        let changes = apply_cuts(ctx, &self.bodies, shapes, cuts)?;
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

/// A plate as a slab: its largest planar face's plane (`n` its outward
/// normal), that face's axes (`x` along its longest straight edge) and its
/// extent along them from `origin`, and its thickness behind the face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slab {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub n: [f64; 3],
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub thickness: f64,
}

impl Slab {
    /// Whether its axes lie along the component's.
    pub fn along_axes(&self) -> bool {
        [self.x, self.y, self.n]
            .iter()
            .all(|a| a.iter().any(|c| (c.abs() - 1.0).abs() < 1e-9))
    }

    /// Its corners.
    pub fn corners(&self) -> Vec<[f64; 3]> {
        let mut out = Vec::with_capacity(8);
        for u in [self.min[0], self.max[0]] {
            for v in [self.min[1], self.max[1]] {
                for w in [0.0, -self.thickness] {
                    out.push(std::array::from_fn(|k| {
                        self.origin[k] + self.x[k] * u + self.y[k] * v + self.n[k] * w
                    }));
                }
            }
        }
        out
    }

    /// Its extent along a direction.
    pub fn along(&self, d: [f64; 3]) -> [f64; 2] {
        self.corners()
            .iter()
            .fold([f64::INFINITY, f64::NEG_INFINITY], |r, c| {
                let t = mitcad_vec3::dot(*c, d);
                [r[0].min(t), r[1].max(t)]
            })
    }
}

/// A body's slab (see [`Slab`]); refused in words for a body without a
/// planar face.
fn slab<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
) -> Result<Slab, String> {
    let faces = ctx.kernel.faces(shape).map_err(|e| e.to_string())?;
    let largest = faces
        .iter()
        .filter(|f| f.surface == "plane")
        .filter_map(|f| {
            Some((
                f.names.first()?.parse::<crate::topo::FaceName>().ok()?,
                f.area,
            ))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .ok_or_else(|| format!("the plate {body} has no planar face"))?
        .0;
    let plane = ctx
        .kernel
        .face_plane(shape, &largest)
        .map_err(|e| e.to_string())?;
    let n = mitcad_vec3::unit_div(plane.normal).ok_or("a plate's face has no normal")?;
    let curves = ctx
        .kernel
        .curves_of(shape, &crate::topo::TopoName::Face(largest))
        .map_err(|e| e.to_string())?;
    let face = super::mortise_tenon::EndFace::new(plane.origin, n, &curves, false)?;
    // Its thickness: through the body from the face's middle.
    let mid: [f64; 3] = std::array::from_fn(|k| {
        face.origin[k]
            + face.x[k] * (face.min[0] + face.max[0]) / 2.0
            + face.y[k] * (face.min[1] + face.max[1]) / 2.0
    });
    let reach = (face.max[0] - face.min[0]) + (face.max[1] - face.min[1]) + 1.0;
    let from = mitcad_vec3::add(mid, mitcad_vec3::scale(n, 1e-3));
    let to = mitcad_vec3::add(mid, mitcad_vec3::scale(n, -reach));
    let crossings = ctx
        .kernel
        .segment_crossings(shape, from, to)
        .map_err(|e| e.to_string())?;
    let thickness = crossings
        .iter()
        .find(|(_, enters)| !enters)
        .map(|(t, _)| t * (reach + 1e-3) - 1e-3)
        .filter(|t| *t > SLACK)
        .ok_or_else(|| format!("the plate {body} has no thickness behind its largest face"))?;
    Ok(Slab {
        origin: face.origin,
        x: face.x,
        y: face.y,
        n,
        min: face.min,
        max: face.max,
        thickness,
    })
}

/// The joints of plates at any angle (mitcad#1939): for each pair whose
/// solids overlap, the overlap is divided along the line where their
/// planes meet (the cross product of their normals), the segments' slabs
/// (wider by the clearance where Finger Joint's plan widens them) taken
/// within the overlap and cut from the plate that does not own them;
/// where a third plate's overlap reaches the joint's end, that end stays
/// with the plate listed first of the three. The cuts per plate (an index
/// of `shapes`).
fn oriented<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    shapes: &[K::Shape],
    slabs: &[Slab],
    sizes: &FingerSizes,
) -> Result<Vec<(usize, FingerCut, K::Shape)>, String> {
    let reach = slabs
        .iter()
        .flat_map(Slab::corners)
        .flatten()
        .fold(0.0f64, |r, c| r.max(c.abs()))
        * 4.0
        + 10.0;
    let mut out = Vec::new();
    let mut joints = 0;
    for a in 0..shapes.len() {
        for b in a + 1..shapes.len() {
            let along = mitcad_vec3::cross(slabs[a].n, slabs[b].n);
            let Some(d) = mitcad_vec3::unit_div(along).filter(|_| mitcad_vec3::norm(along) > 1e-6)
            else {
                continue;
            };
            let overlap = ctx
                .boolean(BooleanOp::Intersect, &[&shapes[a]], &shapes[b])
                .map_err(|e| e.to_string())?;
            let mut pieces: Vec<K::Shape> = overlap.pieces.into_iter().map(|p| p.shape).collect();
            // Plates that only touch, one's end against the other's face:
            // the joint is the face's plate where the end's plate's slab,
            // carried on past its end, runs into it; the end's plate
            // grows its fingers there (mitcad#1939).
            let mut end_plate = None;
            if pieces.is_empty() {
                for (from, into) in [(a, b), (b, a)] {
                    // Only where its end lies on one of the other's faces.
                    let face = &slabs[into];
                    let at = mitcad_vec3::dot(face.origin, face.n);
                    let ends = slabs[from].along(face.n);
                    let touches = [at, at - face.thickness]
                        .iter()
                        .any(|p| ends.iter().any(|e| (e - p).abs() < 1e-4));
                    if !touches {
                        continue;
                    }
                    let carried = extended(ctx, &slabs[from], reach)?;
                    let met = ctx
                        .boolean(BooleanOp::Intersect, &[&shapes[into]], &carried)
                        .map_err(|e| e.to_string())?;
                    let found: Vec<K::Shape> = met.pieces.into_iter().map(|p| p.shape).collect();
                    if !found.is_empty() {
                        pieces = found;
                        end_plate = Some(from);
                        break;
                    }
                }
            }
            if pieces.is_empty() {
                continue;
            }
            joints += 1;
            let region = if pieces.len() == 1 {
                pieces.into_iter().next().expect("one")
            } else {
                ctx.kernel.compound(&pieces).map_err(|e| e.to_string())?
            };
            let (ia, ib) = (slabs[a].along(d), slabs[b].along(d));
            let (mut lo, mut hi) = (ia[0].max(ib[0]), ia[1].min(ib[1]));
            let (first, second) = if sizes.flip { (b, a) } else { (a, b) };
            // A slab across the joint from `u0` to `u1` along it, kept to
            // the overlap.
            let piece =
                |ctx: &mut EvalContext<'_, K>, u0: f64, u1: f64| -> Result<K::Shape, String> {
                    let e = mitcad_vec3::unit_div(mitcad_vec3::cross(slabs[a].n, d))
                        .ok_or("no axis")?;
                    let region_2d = rectangle(1, [u0, -reach], [u1, reach]);
                    let tool = ctx
                        .kernel
                        .extrude_feature(&ExtrudeFeatureSpec {
                            feature: ctx.uid,
                            frame: SketchFrame {
                                origin: [0.0; 3],
                                x_axis: d,
                                y_axis: e,
                            },
                            regions: std::slice::from_ref(&region_2d),
                            direction: mitcad_vec3::cross(d, e),
                            start: ExtrudeStart::Offset(-reach),
                            side1: ExtrudeSide {
                                end: SideEnd::Distance(2.0 * reach),
                                taper: 0.0,
                                thin: None,
                            },
                            side2: None,
                        })
                        .map_err(|e| e.to_string())?;
                    let kept = ctx
                        .boolean(BooleanOp::Intersect, &[&tool], &region)
                        .map_err(|e| e.to_string())?;
                    let pieces: Vec<K::Shape> = kept.pieces.into_iter().map(|p| p.shape).collect();
                    if pieces.len() == 1 {
                        Ok(pieces.into_iter().next().expect("one"))
                    } else {
                        ctx.kernel.compound(&pieces).map_err(|e| e.to_string())
                    }
                };
            // The ends inside a third plate stay with the first of the three.
            for (k, other) in slabs.iter().enumerate() {
                if k == a || k == b {
                    continue;
                }
                let touched = ctx
                    .boolean(BooleanOp::Intersect, &[&region], &shapes[k])
                    .map_err(|e| e.to_string())?;
                if touched.pieces.is_empty() {
                    continue;
                }
                let ic = other.along(d);
                let (from, to) = (ic[0].max(lo), ic[1].min(hi));
                if to - from <= SLACK {
                    continue;
                }
                let owner = a.min(b).min(k);
                let corner = piece(ctx, from, to)?;
                match end_plate {
                    None => {
                        for plate in [a, b] {
                            if plate != owner {
                                out.push((plate, FingerCut::Cut, corner.clone()));
                            }
                        }
                    }
                    Some(end) if owner == end => {
                        let face = if end == a { b } else { a };
                        out.push((end, FingerCut::Join, corner.clone()));
                        out.push((face, FingerCut::Cut, corner));
                    }
                    Some(_) => {}
                }
                if (from - lo).abs() <= SLACK {
                    lo = to;
                } else if (to - hi).abs() <= SLACK {
                    hi = from;
                }
            }
            let (start, end) = (lo + sizes.end_margin, hi - sizes.end_margin);
            let length = end - start;
            if length <= SLACK {
                continue;
            }
            let thickness = slabs[a].thickness.min(slabs[b].thickness);
            let width = sizes.finger_width.unwrap_or(2.5 * thickness);
            let n = sizes.count.unwrap_or_else(|| odd_count(length / width));
            let step = length / n as f64;
            let c = sizes.clearance;
            if step <= 2.0 * c + SLACK {
                return Err(format!(
                    "the fingers ({} mm) are too narrow for the clearance of {} mm on each side",
                    super::bearing_sizes::short_number(step),
                    super::bearing_sizes::short_number(c)
                ));
            }
            let mut segments: Vec<(f64, f64, usize)> = Vec::new();
            if sizes.end_margin > 0.0 {
                segments.push((lo, start, first));
            }
            for k in 0..n {
                let owner = if k % 2 == 0 { first } else { second };
                segments.push((
                    start + k as f64 * step,
                    start + (k + 1) as f64 * step,
                    owner,
                ));
            }
            if sizes.end_margin > 0.0 {
                segments.push((end, hi, first));
            }
            for (from, to, owner) in segments {
                let other = if owner == a { b } else { a };
                match end_plate {
                    None => {
                        let grow = if owner == first { c } else { 0.0 };
                        let cut = piece(ctx, (from - grow).max(lo), (to + grow).min(hi))?;
                        out.push((other, FingerCut::Cut, cut));
                    }
                    // The end's fingers grow into the face's plate, which
                    // loses them with the clearance.
                    Some(end) if owner == end => {
                        out.push((end, FingerCut::Join, piece(ctx, from, to)?));
                        let cut = piece(ctx, (from - c).max(lo), (to + c).min(hi))?;
                        out.push((other, FingerCut::Cut, cut));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    if joints == 0 {
        return Err(
            "the plates do not meet: plates at an angle are joined where they overlap at their              edge; draw each to the outside of the joint"
                .into(),
        );
    }
    Ok(out)
}

/// Each plate's fingers joined, then its slots cut, at once; the plates
/// keep their ids.
fn apply_cuts<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &[BodyUid],
    mut shapes: Vec<K::Shape>,
    cuts: Vec<(usize, FingerCut, K::Shape)>,
) -> Result<Vec<BodyChange<K::Shape>>, String> {
    let mut changes: Vec<BodyChange<K::Shape>> = Vec::new();
    for op in [FingerCut::Join, FingerCut::Cut] {
        for (plate, body) in bodies.iter().enumerate() {
            let tools: Vec<K::Shape> = cuts
                .iter()
                .filter(|(p, o, _)| *p == plate && *o == op)
                .map(|(_, _, s)| s.clone())
                .collect();
            if tools.is_empty() {
                continue;
            }
            let tool = one_tool(ctx, tools)?;
            let (boolean, what) = match op {
                FingerCut::Join => (BooleanOp::Join, "fingers"),
                FingerCut::Cut => (BooleanOp::Cut, "fingers' slots"),
            };
            for change in apply_tool(ctx, boolean, &[(*body, shapes[plate].clone())], &tool, what)?
            {
                match change {
                    BodyChange::Set(uid, shape) if uid == *body => {
                        shapes[plate] = shape.clone();
                        changes.retain(|c| !matches!(c, BodyChange::Set(u, _) if u == body));
                        changes.push(BodyChange::Set(uid, shape));
                    }
                    BodyChange::Set(..) => {
                        return Err(format!(
                            "the fingers' slots cut the plate {body} apart; make the fingers \
                             narrower or the plate wider"
                        ));
                    }
                    other => changes.push(other),
                }
            }
        }
    }
    Ok(changes)
}

/// A slab carried on in its own plane far past its edges: a plate's
/// thickness layer, to find where its end runs into another plate.
fn extended<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    slab: &Slab,
    reach: f64,
) -> Result<K::Shape, String> {
    let region = rectangle(
        1,
        [slab.min[0] - reach, slab.min[1] - reach],
        [slab.max[0] + reach, slab.max[1] + reach],
    );
    ctx.kernel
        .extrude_feature(&ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame: SketchFrame {
                origin: slab.origin,
                x_axis: slab.x,
                y_axis: slab.y,
            },
            regions: std::slice::from_ref(&region),
            direction: mitcad_vec3::scale(slab.n, -1.0),
            start: ExtrudeStart::Offset(0.0),
            side1: ExtrudeSide {
                end: SideEnd::Distance(slab.thickness),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        })
        .map_err(|e| e.to_string())
}

/// Joins and cuts each plate's boxes (its fingers first, then its slots);
/// the plates keep their ids. Refused where a plate's slots cut it apart.
pub(super) fn apply_boxes<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &[BodyUid],
    mut shapes: Vec<K::Shape>,
    planned: &[FingerBox],
    what: &str,
) -> Result<Vec<BodyChange<K::Shape>>, String> {
    let mut changes = Vec::new();
    let mut curve = 1u32;
    for op in [FingerCut::Join, FingerCut::Cut] {
        for (plate, body) in bodies.iter().enumerate() {
            let tools: Vec<K::Shape> = planned
                .iter()
                .filter(|b| b.plate == plate && b.op == op)
                .map(|b| {
                    let shape = box_shape(ctx, b.min, b.max, curve);
                    curve += 4;
                    shape
                })
                .collect::<Result<_, _>>()?;
            if tools.is_empty() {
                continue;
            }
            let tool = one_tool(ctx, tools)?;
            let (boolean, name) = match op {
                FingerCut::Join => (BooleanOp::Join, what.to_owned()),
                FingerCut::Cut => (BooleanOp::Cut, format!("{what}s' slots")),
            };
            let made = apply_tool(
                ctx,
                boolean,
                &[(*body, shapes[plate].clone())],
                &tool,
                &name,
            )?;
            for change in made {
                match change {
                    BodyChange::Set(uid, shape) if uid == *body => {
                        shapes[plate] = shape.clone();
                        changes.retain(|c: &BodyChange<K::Shape>| {
                            !matches!(c, BodyChange::Set(u, _) if u == body)
                        });
                        changes.push(BodyChange::Set(uid, shape));
                    }
                    BodyChange::Set(..) => {
                        return Err(format!(
                            "the {what}s' slots cut the plate {body} apart; make the fingers \
                             narrower or the plate wider"
                        ));
                    }
                    other => changes.push(other),
                }
            }
        }
    }
    Ok(changes)
}

/// Several tools as one shape: their union, as tools that overlap (a
/// corner cut by two joints, a slot and its pocket) make one shape a
/// boolean takes; a compound where the kernel has no unions.
pub(super) fn one_tool<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    mut tools: Vec<K::Shape>,
) -> Result<K::Shape, String> {
    if tools.len() == 1 {
        return Ok(tools.remove(0));
    }
    let refs: Vec<&K::Shape> = tools.iter().collect();
    match ctx.kernel.unite(&refs) {
        Ok(shape) => Ok(shape),
        Err(crate::kernel::KernelError::Unsupported(_)) => {
            ctx.kernel.compound(&tools).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// An axis-aligned box from `min` to `max` (the component's axes), made
/// by extruding its rectangle in the XY plane: lines `c<first>` to
/// `c<first + 3>`.
pub(super) fn box_shape<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    min: [f64; 3],
    max: [f64; 3],
    first: u32,
) -> Result<K::Shape, String> {
    let region = rectangle(first, [min[0], min[1]], [max[0], max[1]]);
    ctx.kernel
        .extrude_feature(&ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame: SketchFrame {
                origin: [0.0, 0.0, min[2]],
                x_axis: [1.0, 0.0, 0.0],
                y_axis: [0.0, 1.0, 0.0],
            },
            regions: std::slice::from_ref(&region),
            direction: [0.0, 0.0, 1.0],
            start: ExtrudeStart::Offset(0.0),
            side1: ExtrudeSide {
                end: SideEnd::Distance(max[2] - min[2]),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        })
        .map_err(|e| e.to_string())
}

/// The rectangle from `a` to `b` (its lower left and upper right corners)
/// with lines `c<first>` to `c<first + 3>`, counter-clockwise from the
/// bottom.
pub(super) fn rectangle(first: u32, a: [f64; 2], b: [f64; 2]) -> ProfileRegion {
    polygon(first, &[a, [b[0], a[1]], b, [a[0], b[1]]])
}

/// The closed polygon through `points` with lines `c<first>` on.
pub(super) fn polygon(first: u32, points: &[[f64; 2]]) -> ProfileRegion {
    let n = points.len() as u32;
    let id = |i: u32| EntityUid(first + (i % n));
    let segments: Vec<ProfileSegment> = (0..n)
        .map(|i| ProfileSegment {
            key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
            geometry: SegmentGeometry::Line {
                start: points[i as usize],
                end: points[((i + 1) % n) as usize],
            },
        })
        .collect();
    let key = RegionKey::new(segments.iter().map(|s| s.key.clone())).expect("a loop has segments");
    ProfileRegion {
        key,
        loops: vec![ProfileLoop { segments }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min: [f64; 3], max: [f64; 3]) -> BoundingBox {
        BoundingBox { min, max }
    }

    fn sizes(width: f64) -> FingerSizes {
        FingerSizes {
            finger_width: Some(width),
            count: None,
            clearance: 0.1,
            end_margin: 0.0,
            flip: false,
        }
    }

    fn volume(b: &FingerBox) -> f64 {
        (0..3).map(|i| b.max[i] - b.min[i]).product()
    }

    /// Two 6 mm plywood plates at a right angle, overlapping at the corner
    /// (each drawn to the outside): five fingers of 12 along the 60 mm
    /// edge, the front's three in the side's slots wider by the clearance,
    /// the side's two in the front's exact slots.
    #[test]
    fn two_plates_at_a_right_angle_get_matching_fingers() {
        let front = bounds([0.0, 0.0, 0.0], [100.0, 6.0, 60.0]);
        let side = bounds([0.0, 0.0, 0.0], [6.0, 80.0, 60.0]);
        let planned = plan(&[front, side], &sizes(12.0)).unwrap();
        assert_eq!(planned.len(), 5);
        let side_slots: Vec<_> = planned.iter().filter(|b| b.plate == 1).collect();
        let front_slots: Vec<_> = planned.iter().filter(|b| b.plate == 0).collect();
        assert_eq!(side_slots.len(), 3);
        assert_eq!(front_slots.len(), 2);
        assert!(planned.iter().all(|b| b.op == FingerCut::Cut));
        let z = |b: &FingerBox| [b.min[2], b.max[2]];
        assert!(z(side_slots[0])[0].abs() < 1e-12);
        assert!((z(side_slots[0])[1] - 12.1).abs() < 1e-12);
        assert!((z(side_slots[1])[0] - 23.9).abs() < 1e-12);
        assert!((z(side_slots[1])[1] - 36.1).abs() < 1e-12);
        assert_eq!(z(front_slots[0]), [12.0, 24.0]);
        let side_lost: f64 = side_slots.iter().map(|b| volume(b)).sum();
        let front_lost: f64 = front_slots.iter().map(|b| volume(b)).sum();
        assert!((side_lost - 1310.4).abs() < 1e-9, "{side_lost}");
        assert!((front_lost - 864.0).abs() < 1e-9, "{front_lost}");
        // Every finger fits its slot with the clearance on both sides: the
        // front keeps [0, 12], the side's slot is [0, 12.1]; the side keeps
        // [12.1, 23.9] of its finger [12, 24], the front's slot.
        for k in 0..2 {
            let finger = [side_slots[k].max[2], side_slots[k + 1].min[2]];
            let slot = z(front_slots[k]);
            assert!(((finger[0] - slot[0]) - 0.1).abs() < 1e-12);
            assert!(((slot[1] - finger[1]) - 0.1).abs() < 1e-12);
        }
    }

    /// The front's end against the side's face: its fingers grow into the
    /// side, which loses them with the clearance.
    #[test]
    fn touching_plates_grow_the_end_into_the_face() {
        let front = bounds([6.0, 0.0, 0.0], [100.0, 6.0, 60.0]);
        let side = bounds([0.0, 0.0, 0.0], [6.0, 80.0, 60.0]);
        let planned = plan(&[front, side], &sizes(12.0)).unwrap();
        let joins: Vec<_> = planned.iter().filter(|b| b.op == FingerCut::Join).collect();
        let cuts: Vec<_> = planned.iter().filter(|b| b.op == FingerCut::Cut).collect();
        assert_eq!(joins.len(), 3);
        assert!(
            joins
                .iter()
                .all(|b| b.plate == 0 && b.min[0] == 0.0 && b.max[0] == 6.0)
        );
        assert_eq!(cuts.len(), 3);
        assert!(cuts.iter().all(|b| b.plate == 1));
        let gained: f64 = joins.iter().map(|b| volume(b)).sum();
        let lost: f64 = cuts.iter().map(|b| volume(b)).sum();
        assert!((gained - 1296.0).abs() < 1e-9);
        assert!((lost - 1310.4).abs() < 1e-9);
    }

    #[test]
    fn counts_widths_and_refusals() {
        assert_eq!(odd_count(4.0), 5);
        assert_eq!(odd_count(3.9), 3);
        assert_eq!(odd_count(0.2), 1);
        let a = bounds([0.0, 0.0, 0.0], [100.0, 6.0, 60.0]);
        let far = bounds([200.0, 0.0, 0.0], [206.0, 80.0, 60.0]);
        let error = plan(&[a, far], &sizes(12.0)).unwrap_err();
        assert!(error.starts_with("the two plates do not meet"), "{error}");
        let mut narrow = sizes(12.0);
        narrow.count = Some(400);
        let side = bounds([0.0, 0.0, 0.0], [6.0, 80.0, 60.0]);
        assert!(
            plan(&[a, side], &narrow)
                .unwrap_err()
                .contains("too narrow")
        );
        // The default width is 2.5 times the joint's thickness: 15, so
        // 60 / 15 = 4 gives five fingers.
        let mut default = sizes(1.0);
        default.finger_width = None;
        let planned = plan(&[a, side], &default).unwrap();
        assert_eq!(planned.len(), 5);
    }

    /// A box's corner: three plates meet; the corner cube stays with the
    /// first plate and the joints run between the corners.
    #[test]
    fn a_corner_of_three_plates_stays_with_the_first() {
        let bottom = bounds([0.0, 0.0, 0.0], [100.0, 80.0, 6.0]);
        let front = bounds([0.0, 0.0, 0.0], [100.0, 6.0, 60.0]);
        let side = bounds([0.0, 0.0, 0.0], [6.0, 80.0, 60.0]);
        let planned = plan(&[bottom, front, side], &sizes(12.0)).unwrap();
        // The front-side joint along z starts above the bottom (z = 6).
        let front_side: Vec<_> = planned
            .iter()
            .filter(|b| b.max[0] <= 6.0 && b.max[1] <= 6.0 && b.min[2] >= 6.0 - 1e-9)
            .collect();
        assert!(!front_side.is_empty());
        // The corner cube [0,6]^3 is cut from the front and the side, not
        // from the bottom.
        let corner: Vec<_> = planned
            .iter()
            .filter(|b| b.min == [0.0; 3] && b.max == [6.0, 6.0, 6.0])
            .collect();
        assert!(corner.iter().all(|b| b.plate != 0));
        assert!(!corner.is_empty());
    }
}
