// SPDX-License-Identifier: MIT
//! End cap (mitcad#2243; `commands.md`, "BUILD: profile assemblies", *End
//! caps*): a plate that closes an open end of a tube or a profile member,
//! named `EndCap<n>`. Its outline is the member's outer section: a closed
//! section's (mitcad#2270, mitcad#2287: a tube or a bar whose end face's
//! outer loop is convex, or a hollow section of any outline, its one hole
//! a wall's inside) is that loop ([`outline`]); an open one's (an angle's,
//! a channel's, a beam's, a slot profile's) is the rectangle round the
//! section with the round of each of its corners. It is set in from the
//! edge by `inset` (or `ratio` times the thickness), its corners
//! chamfered by `chamfer` (at least the corners' round). It is
//! `thickness` thick, outward past the end, or inward in place of the
//! member's last `thickness` (the member is cut back by as much).
//!
//! It has two forms:
//!
//! - **On a BUILD profile's end** (`member`, `end`): the feature is the
//!   body of a component of its own, a BUILD part. It reads the member's
//!   derived part (another component's body, at this point of the
//!   timeline) and its end frame (`end_a`, `end_b`), and makes the plate
//!   in that frame's coordinates: x and y across the member, z out of the
//!   end. Its frame `seat` (z towards the member) is joined to the end
//!   frame by a rigid joint, flipped, so the plate follows the member when
//!   it moves, and its section when its size changes. An inward cap's
//!   member is cut back by its derived part (`cut_back`), so the cap only
//!   makes the plate.
//! - **On a planar end face** (`face`) of a body of the feature's own
//!   component (an imported frame member, mitcad#2243): the plate is a new
//!   body of the component on the face; an inward cap cuts the member's
//!   body back itself, by the plate's box.
//!
//! The section is read from the geometry: the end face's boundary (the
//! member's `<F>:end_a` or `<F>:end_b`, or the face), whose outer loop is
//! a closed section's outline when the section is closed; else the box of
//! the member across its axis (a BUILD member) or of the end face's
//! boundary (a face), and the circular edges whose centres lie a radius in
//! from both sides of a corner (its round). A round section makes a disc.
//! A mitered end, an inset that leaves nothing or splits the outline, a
//! chamfer smaller than a corner's round, one that leaves nothing of a
//! side or one on a section without corners fail the feature in words.

mod outline;

pub use outline::Outline;

use serde::{Deserialize, Serialize};

use super::face_refs::kernel_error;
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::pattern::none;
use super::{BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput};
use super::{FeatureDef, References};
use crate::datum::{CurveGeometry, SurfaceGeometry};
use crate::ids::{BodyUid, ComponentUid, EntityUid, FeatureUid};
use crate::kernel::{
    BooleanOp, BoundingBox, Curve3, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, Kernel, SideEnd,
};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{EdgeName, RegionKey, SegmentKey};
use crate::transform::{PrimitiveShape, PrimitiveSpec, Transform};
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

/// The name of an end cap's frame that meets the member's end frame: its
/// origin on the plate's face at the end, z towards the member.
pub const SEAT: &str = "seat";

/// Which end of a BUILD profile a cap closes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CapEnd {
    #[default]
    #[serde(rename = "a")]
    A,
    #[serde(rename = "b")]
    B,
}

impl CapEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }

    /// The end's frame of a profile (`end_a`, `end_b`).
    pub fn frame(self) -> &'static str {
        match self {
            Self::A => "end_a",
            Self::B => "end_b",
        }
    }

    pub fn is_b(self) -> bool {
        self == Self::B
    }

    /// The end's letter as the panels show it.
    pub fn letter(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

/// Where the plate goes: past the end, or in place of the member's last
/// thickness.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapDirection {
    #[default]
    Outward,
    Inward,
}

impl CapDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outward => "outward",
            Self::Inward => "inward",
        }
    }

    pub fn is_outward(&self) -> bool {
        *self == Self::Outward
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndCapDef<P = ParamId> {
    /// A BUILD profile's derived part (in another component), with `end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<FeatureUid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<CapEnd>,
    /// Or a planar end face of a body of the feature's own component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<GeomRef>,
    /// The plate's thickness, mm.
    pub thickness: P,
    /// How far the plate is set in from the outside of the section, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub inset: Option<P>,
    /// Or the inset as a share of the thickness.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub ratio: Option<P>,
    /// The corners' chamfer, mm (at least a rounded corner's radius).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub chamfer: Option<P>,
    #[serde(default, skip_serializing_if = "CapDirection::is_outward")]
    pub direction: CapDirection,
}

impl<P> EndCapDef<P> {
    pub const TYPE: &'static str = "end_cap";
    pub const BASE_NAME: &'static str = "EndCap";

    map_params! {
        EndCapDef {
            member: clone,
            end: clone,
            face: clone,
            thickness: param,
            inset: option,
            ratio: option,
            chamfer: option,
            direction: copy,
        }
    }
}

impl FeatureInfo for EndCapDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(member) = self.member {
            references.features.insert(member);
        }
        if let Some(face) = &self.face {
            face.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match (self.member, &self.face) {
            (Some(member), None) => {
                let entry = ctx.feature(member)?;
                if !matches!(entry.def, FeatureDef::DerivedPart(_)) {
                    return Err(format!(
                        "{} is no profile placed by the Build tab: an end cap closes a profile's end",
                        entry.name
                    ));
                }
                if self.end.is_none() {
                    return Err("an end cap on a profile names its end (a or b)".to_owned());
                }
            }
            (None, Some(face)) => {
                if !matches!(face, GeomRef::Face { .. }) {
                    return Err("an end cap's face is a planar face of a body".to_owned());
                }
                face.check(ctx, Want::Face)
                    .map_err(|e| format!("face: {e}"))?;
            }
            _ => {
                return Err(
                    "an end cap is on a profile's end (member and end) or on a face, one of them"
                        .to_owned(),
                );
            }
        }
        if self.inset.is_some() && self.ratio.is_some() {
            return Err("an end cap's inset is a length or a ratio, not both".to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let thickness = value(self.thickness);
        if thickness.is_nan() || thickness <= 0.0 {
            return Err("the end cap's thickness must be greater than zero".to_owned());
        }
        for (name, v) in [
            ("inset", self.inset),
            ("ratio", self.ratio),
            ("chamfer", self.chamfer),
        ] {
            if let Some(id) = v
                && (value(id).is_nan() || value(id) < 0.0)
            {
                return Err(format!("the end cap's {name} must be zero or more"));
            }
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }
}

/// A section across a member at its end, in the end frame's coordinates:
/// the rectangle round it and the round of each of its corners, in the
/// order (max x, max y), (min x, max y), (min x, min y), (max x, min y);
/// 0 for a sharp corner. A closed section has its outer loop too
/// (mitcad#2270), which the plate follows.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub rounds: [f64; 4],
    pub outer: Option<Outline>,
}

/// The corners of a rectangle in a section's order, each with the signs
/// of its x and y from the middle.
const CORNERS: [[f64; 2]; 4] = [[1.0, 1.0], [-1.0, 1.0], [-1.0, -1.0], [1.0, -1.0]];

impl Section {
    /// The section of the rectangle `min`..`max` with the rounds that the
    /// circles (centre in the section's plane, radius) give its corners: a
    /// circle whose centre lies its radius in from both sides of a corner
    /// rounds it (the largest such circle; a circle a corner's round fits
    /// in, as a full round tube's outer circle is for every corner).
    pub fn new(min: [f64; 2], max: [f64; 2], circles: &[([f64; 2], f64)]) -> Self {
        let size = (max[0] - min[0]).max(max[1] - min[1]).max(1.0);
        let tolerance = 1e-4 * size;
        let half = 0.5 * (max[0] - min[0]).min(max[1] - min[1]);
        let mut rounds = [0.0; 4];
        for (k, [sx, sy]) in CORNERS.iter().enumerate() {
            let corner = [
                if *sx > 0.0 { max[0] } else { min[0] },
                if *sy > 0.0 { max[1] } else { min[1] },
            ];
            for (centre, radius) in circles {
                let r = *radius;
                if r <= tolerance || r > half + tolerance {
                    continue;
                }
                let inward = [corner[0] - sx * r, corner[1] - sy * r];
                if (centre[0] - inward[0]).abs() <= tolerance
                    && (centre[1] - inward[1]).abs() <= tolerance
                {
                    rounds[k] = f64::max(rounds[k], r.min(half));
                }
            }
        }
        Self {
            min,
            max,
            rounds,
            outer: None,
        }
    }

    /// A closed section: its outer loop and the box round it.
    pub fn closed(outer: Outline) -> Self {
        let (min, max) = outer.bounds();
        Self {
            min,
            max,
            rounds: [0.0; 4],
            outer: Some(outer),
        }
    }

    pub fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> f64 {
        self.max[1] - self.min[1]
    }

    /// Whether it is round: square, every corner rounded by half its side,
    /// or a closed section whose outline is a circle.
    pub fn is_round(&self) -> bool {
        if let Some(outer) = &self.outer {
            return outer.is_circle();
        }
        let tolerance = 1e-6 * self.width().max(1.0);
        (self.width() - self.height()).abs() <= tolerance
            && self
                .rounds
                .iter()
                .all(|r| (r - self.width() / 2.0).abs() <= tolerance)
    }
}

/// A corner of a plate: rounded (0: sharp) or chamfered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Corner {
    Round(f64),
    Chamfer(f64),
}

impl Corner {
    /// How far the corner reaches along each side from the rectangle's
    /// corner.
    pub fn reach(self) -> f64 {
        match self {
            Self::Round(r) | Self::Chamfer(r) => r,
        }
    }
}

/// A cap's outline: a rectangle and its four corners, in the section's
/// coordinates and order; a closed section's plate (mitcad#2270) its
/// outline instead, `min` and `max` the box round it and the corners
/// unused (sharp).
#[derive(Debug, Clone, PartialEq)]
pub struct Plate {
    pub min: [f64; 2],
    pub max: [f64; 2],
    pub corners: [Corner; 4],
    pub closed: Option<Outline>,
}

/// Millimetres as messages give them: to a thousandth, no trailing zeros.
fn mm(value: f64) -> String {
    crate::build::round(value)
}

impl Plate {
    /// The plate of a section set in by `inset` and with its corners
    /// chamfered by `chamfer` (none or 0: the section's rounds, shrunk by
    /// the inset); refused in words when it leaves no plate. A closed
    /// section's plate is its outer loop set in and chamfered.
    pub fn new(section: &Section, inset: f64, chamfer: Option<f64>) -> Result<Self, String> {
        if inset.is_nan() || inset < 0.0 {
            return Err("the inset must be zero or more".to_owned());
        }
        if let Some(outer) = &section.outer {
            return outline::plate(section, outer, inset, chamfer);
        }
        let min = [section.min[0] + inset, section.min[1] + inset];
        let max = [section.max[0] - inset, section.max[1] - inset];
        let (w, h) = (max[0] - min[0], max[1] - min[1]);
        if w.is_nan() || h.is_nan() || w <= 1e-6 || h <= 1e-6 {
            return Err(format!(
                "an inset of {} mm leaves nothing of the {} x {} mm section",
                mm(inset),
                mm(section.width()),
                mm(section.height())
            ));
        }
        let half = 0.5 * w.min(h);
        let rounds = section.rounds.map(|r| (r - inset).max(0.0).min(half));
        let corners = match chamfer.filter(|c| *c > 1e-9) {
            None => rounds.map(|r| Corner::Round(if r > 1e-9 { r } else { 0.0 })),
            Some(c) => {
                if section.is_round() {
                    return Err("a round section has no corners to chamfer".to_owned());
                }
                let most = rounds.iter().copied().fold(0.0, f64::max);
                if c < most - 1e-9 {
                    return Err(format!(
                        "the chamfer ({} mm) is less than the corners' round ({} mm): give at \
                         least {} mm",
                        mm(c),
                        mm(most),
                        mm(most)
                    ));
                }
                if 2.0 * c >= w.min(h) - 1e-6 {
                    return Err(format!(
                        "a chamfer of {} mm leaves no straight side of the {} x {} mm plate",
                        mm(c),
                        mm(w),
                        mm(h)
                    ));
                }
                [Corner::Chamfer(c); 4]
            }
        };
        Ok(Self {
            min,
            max,
            corners,
            closed: None,
        })
    }

    /// What tells plates of one designation apart (the cut list's rows):
    /// the corners, or a closed section's outline by its area and pieces.
    pub fn shape_key(&self) -> String {
        match &self.closed {
            Some(outline) => format!("outline a{:.6} n{}", outline.area(), outline.pieces.len()),
            None => format!(
                "{:?}",
                self.corners.map(|c| match c {
                    Corner::Round(r) => format!("r{r:.6}"),
                    Corner::Chamfer(c) => format!("c{c:.6}"),
                })
            ),
        }
    }

    pub fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> f64 {
        self.max[1] - self.min[1]
    }

    /// Whether it is a disc: square with every corner rounded by half its
    /// side.
    pub fn is_disc(&self) -> bool {
        if let Some(outline) = &self.closed {
            return outline.is_circle();
        }
        let tolerance = 1e-9 * self.width().max(1.0);
        (self.width() - self.height()).abs() <= tolerance
            && self.corners.iter().all(
                |c| matches!(c, Corner::Round(r) if (r - self.width() / 2.0).abs() <= tolerance),
            )
    }

    /// Its area, mm²: the rectangle less what the corners take away.
    pub fn area(&self) -> f64 {
        if let Some(outline) = &self.closed {
            return outline.area();
        }
        let corners: f64 = self
            .corners
            .iter()
            .map(|c| match c {
                Corner::Round(r) => (1.0 - std::f64::consts::FRAC_PI_4) * r * r,
                Corner::Chamfer(c) => 0.5 * c * c,
            })
            .sum();
        self.width() * self.height() - corners
    }

    /// The rectangle's corner `k` and the directions of the sides that
    /// come into it and go out of it, counter-clockwise.
    fn corner(&self, k: usize) -> ([f64; 2], [f64; 2], [f64; 2]) {
        let [sx, sy] = CORNERS[k];
        let point = [
            if sx > 0.0 { self.max[0] } else { self.min[0] },
            if sy > 0.0 { self.max[1] } else { self.min[1] },
        ];
        // Counter-clockwise: up the right side into (max, max), left along
        // the top out of it, and so on round.
        let sides = [[0.0, 1.0], [-1.0, 0.0], [0.0, -1.0], [1.0, 0.0]];
        (point, sides[k], sides[(k + 1) % 4])
    }

    /// The outline as segments counter-clockwise: each corner's round or
    /// chamfer and the straight stretch of side after it (left out where
    /// the corners use the whole side).
    fn segments(&self) -> Vec<SegmentGeometry> {
        if let Some(outline) = &self.closed {
            return outline.segments();
        }
        if self.is_disc() {
            return vec![SegmentGeometry::Circle {
                center: [
                    0.5 * (self.min[0] + self.max[0]),
                    0.5 * (self.min[1] + self.max[1]),
                ],
                radius: self.width() / 2.0,
                start: 0.0,
            }];
        }
        let at = |p: [f64; 2], d: [f64; 2], s: f64| [p[0] + d[0] * s, p[1] + d[1] * s];
        let mut out = Vec::new();
        for k in 0..4 {
            let (point, into, out_of) = self.corner(k);
            let a = self.corners[k].reach();
            let entry = at(point, into, -a);
            let exit = at(point, out_of, a);
            if a > 1e-9 {
                out.push(match self.corners[k] {
                    Corner::Round(r) => {
                        let center = at(entry, out_of, r);
                        let angle = |p: [f64; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
                        SegmentGeometry::Arc {
                            center,
                            radius: r,
                            start_angle: angle(entry),
                            end_angle: angle(exit),
                        }
                    }
                    Corner::Chamfer(_) => SegmentGeometry::Line {
                        start: entry,
                        end: exit,
                    },
                });
            }
            // The side on to the next corner's entry.
            let (next, next_into, _) = self.corner((k + 1) % 4);
            let next_entry = at(next, next_into, -self.corners[(k + 1) % 4].reach());
            let d = [next_entry[0] - exit[0], next_entry[1] - exit[1]];
            if (d[0] * d[0] + d[1] * d[1]).sqrt() > 1e-9 {
                out.push(SegmentGeometry::Line {
                    start: exit,
                    end: next_entry,
                });
            }
        }
        out
    }

    /// The outline as a profile region (curves `c1`, `c2`, ...).
    pub fn region(&self) -> ProfileRegion {
        let geometries = self.segments();
        let n = geometries.len() as u32;
        let segments: Vec<ProfileSegment> = if n == 1 {
            vec![ProfileSegment {
                key: SegmentKey::closed(EntityUid(1)),
                geometry: geometries[0].clone(),
            }]
        } else {
            let id = |i: u32| EntityUid(1 + (i % n));
            geometries
                .into_iter()
                .enumerate()
                .map(|(i, geometry)| {
                    let i = i as u32;
                    ProfileSegment {
                        key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
                        geometry,
                    }
                })
                .collect()
        };
        let key = RegionKey::new(segments.iter().map(|s| s.key.clone())).expect("a loop");
        ProfileRegion {
            key,
            loops: vec![ProfileLoop { segments }],
        }
    }

    /// Points round the outline, counter-clockwise, arcs in `steps`
    /// pieces (a preview's polygon).
    pub fn outline(&self, steps: usize) -> Vec<[f64; 2]> {
        if let Some(outline) = &self.closed {
            return outline.points();
        }
        let steps = steps.max(1);
        let mut points = Vec::new();
        for segment in self.segments() {
            match segment {
                SegmentGeometry::Line { start, .. } => points.push(start),
                SegmentGeometry::Arc {
                    center,
                    radius,
                    start_angle,
                    end_angle,
                } => {
                    let mut end = end_angle;
                    if end <= start_angle {
                        end += std::f64::consts::TAU;
                    }
                    for i in 0..steps {
                        let t = start_angle + (end - start_angle) * i as f64 / steps as f64;
                        points.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
                    }
                }
                SegmentGeometry::Circle { center, radius, .. } => {
                    for i in 0..4 * steps {
                        let t = std::f64::consts::TAU * i as f64 / (4 * steps) as f64;
                        points.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
                    }
                }
                _ => {}
            }
        }
        points
    }
}

/// The circles among a body's edges: centre, axis and radius.
fn circles_of<K: Kernel>(kernel: &K, shape: &K::Shape) -> Vec<([f64; 3], [f64; 3], f64)> {
    let curves: Vec<CurveGeometry> = match kernel.edge_geometries(shape) {
        Ok(all) => all.into_iter().map(|(_, c)| c).collect(),
        Err(_) => kernel
            .edges(shape)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| e.name)
            .filter_map(|n| n.parse::<EdgeName>().ok())
            .filter_map(|n| kernel.edge_geometry(shape, &n).ok())
            .collect(),
    };
    curves
        .into_iter()
        .filter_map(|c| match c {
            CurveGeometry::Circle {
                center,
                normal,
                radius,
                ..
            } => Some((center, normal, radius)),
            _ => None,
        })
        .collect()
}

/// A BUILD member's end face: `<F>:end_a` or `<F>:end_b` of its derived
/// part `member`, square or cut back.
pub fn end_face(member: FeatureUid, end: CapEnd) -> crate::topo::FaceName {
    crate::topo::FaceName::new(member, end.frame(), None)
}

/// A member's section at an end (`end` the end frame in the body's
/// coordinates): a closed section's outer loop from its end face's
/// boundary (`face`, mitcad#2270); else its body's box across the end
/// frame's z and its circular edges along that axis.
pub fn member_section<K: Kernel>(
    kernel: &K,
    body: &K::Shape,
    end: &Transform,
    face: &crate::topo::FaceName,
) -> Result<Section, String> {
    let into = crate::build::inverse(end);
    // A kernel that cannot give the boundary leaves the box.
    let boundary: Vec<Curve3> = kernel
        .face_boundary(body, face)
        .unwrap_or_default()
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    if let Some(outer) = outline::outer_loop(&boundary, &into) {
        return Ok(Section::closed(outer));
    }
    let bounds = kernel
        .bounding_box(body)
        .map_err(kernel_error)?
        .ok_or("the member has no extent")?;
    let local = crate::build::moved_box(&bounds, &into);
    let circles: Vec<([f64; 2], f64)> = circles_of(kernel, body)
        .into_iter()
        .filter_map(|(center, normal, radius)| {
            let axis = into.apply_vector(normal);
            (unit(axis)?[2].abs() > 1.0 - 1e-6).then(|| {
                let c = into.apply_point(center);
                ([c[0], c[1]], radius)
            })
        })
        .collect();
    Ok(Section::new(
        [local.min[0], local.min[1]],
        [local.max[0], local.max[1]],
        &circles,
    ))
}

/// Points of a curve, for its box: a line's ends, an arc's samples, a
/// spline's poles.
fn curve_points(curve: &Curve3) -> Vec<[f64; 3]> {
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
            closed,
        } => {
            let y = cross(*normal, *x_axis);
            let (from, to) = if *closed {
                (0.0, std::f64::consts::TAU)
            } else {
                (*start, *end)
            };
            (0..=64)
                .map(|i| {
                    let t = from + (to - from) * f64::from(i) / 64.0;
                    add(
                        *center,
                        add(scale(*x_axis, major * t.cos()), scale(y, minor * t.sin())),
                    )
                })
                .collect()
        }
        Curve3::BSpline { poles, .. } => poles.clone(),
        Curve3::Helix { center, .. } => vec![*center],
    }
}

/// An end face's frame and section: z the face's outward normal, x along
/// its longest straight edge (any for a round face), the section its
/// boundary's outer loop when the section is closed (mitcad#2270,
/// mitcad#2287: its outer loop convex, or its one hole a wall's inside),
/// else the box of its boundary in that frame and its arcs' rounds.
pub fn face_section<K: Kernel>(
    kernel: &K,
    body: &K::Shape,
    face: &crate::topo::FaceName,
) -> Result<(Transform, Section), String> {
    let (origin, normal) = match kernel.face_geometry(body, face).map_err(kernel_error)? {
        SurfaceGeometry::Plane { origin, normal } => (origin, normal),
        _ => return Err("the end cap's face is not planar".to_owned()),
    };
    let z = unit(normal).ok_or("the end cap's face has no normal")?;
    let curves: Vec<Curve3> = kernel
        .face_boundary(body, face)
        .map_err(kernel_error)?
        .into_iter()
        .map(|(_, c)| c)
        .collect();
    if curves.is_empty() {
        return Err("the end cap's face has no edges".to_owned());
    }
    let x = curves
        .iter()
        .filter_map(|c| match c {
            Curve3::Line { start, end } => {
                let d = sub(*end, *start);
                let across = sub(d, scale(z, dot(d, z)));
                Some((norm(across), across))
            }
            _ => None,
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .and_then(|(_, d)| unit(d))
        .unwrap_or_else(|| crate::build::perpendicular(z));
    let frame = crate::build::frame(origin, x, z);
    let into = crate::build::inverse(&frame);
    if let Some(outer) = outline::outer_loop(&curves, &into) {
        return Ok((frame, Section::closed(outer)));
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let mut circles = Vec::new();
    for curve in &curves {
        for p in curve_points(curve) {
            let q = into.apply_point(p);
            for i in 0..2 {
                min[i] = min[i].min(q[i]);
                max[i] = max[i].max(q[i]);
            }
        }
        if let Curve3::Conic {
            center,
            normal,
            major,
            minor,
            ..
        } = curve
            && (major - minor).abs() <= 1e-9 * major.max(1.0)
            && unit(*normal).is_some_and(|n| dot(n, z).abs() > 1.0 - 1e-6)
        {
            let c = into.apply_point(*center);
            circles.push(([c[0], c[1]], *major));
        }
    }
    if !min[0].is_finite() || max[0] <= min[0] || max[1] <= min[1] {
        return Err("the end cap's face has no extent".to_owned());
    }
    Ok((frame, Section::new(min, max, &circles)))
}

/// The frame of a cap's `seat` in its own coordinates: at the origin, x
/// along x, z down (towards the member). A rigid joint, flipped, between it
/// and the member's end frame puts the cap's coordinates on the end
/// frame's.
pub fn seat_frame() -> Transform {
    crate::joints::frame([0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0])
}

/// The plate a cap makes of a section: its outline and how far it is set
/// in, from the definition's values.
pub fn plate_of(
    section: &Section,
    thickness: f64,
    inset: Option<f64>,
    ratio: Option<f64>,
    chamfer: Option<f64>,
) -> Result<Plate, String> {
    if thickness.is_nan() || thickness <= 0.0 {
        return Err("the end cap's thickness must be greater than zero".to_owned());
    }
    let inset = match (inset, ratio) {
        (Some(_), Some(_)) => {
            return Err("an end cap's inset is a length or a ratio, not both".to_owned());
        }
        (Some(i), None) => i,
        (None, Some(r)) if r >= 0.0 => r * thickness,
        (None, Some(_)) => return Err("the end cap's ratio must be zero or more".to_owned()),
        (None, None) => 0.0,
    };
    Plate::new(section, inset, chamfer)
}

impl<K: Kernel> EvalContext<'_, K> {
    /// The component a feature before this one is in, and its definition.
    fn earlier_feature(&self, uid: FeatureUid) -> Option<(ComponentUid, FeatureDef)> {
        self.env
            .features
            .iter()
            .find(|f| f.uid == uid)
            .map(|f| (f.component, f.def.clone()))
    }
}

/// A cap's thickness, inset, ratio and chamfer.
type CapValues = (f64, Option<f64>, Option<f64>, Option<f64>);

impl EndCapDef {
    /// The values the plate is made with: thickness, inset, ratio and
    /// chamfer.
    fn values<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<CapValues, String> {
        let thickness = ctx.positive(self.thickness)?;
        let mut optional = |id: Option<ParamId>| id.map(|id| ctx.param(id)).transpose();
        let inset = optional(self.inset)?;
        let ratio = optional(self.ratio)?;
        let chamfer = optional(self.chamfer)?;
        Ok((thickness, inset, ratio, chamfer))
    }
}

/// Extrudes a plate's outline `thickness` along the frame's normal from
/// the frame's plane.
fn plate_solid<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    plate: &Plate,
    frame: SketchFrame,
    thickness: f64,
) -> Result<K::Shape, String> {
    let region = plate.region();
    ctx.kernel
        .extrude_feature(&ExtrudeFeatureSpec {
            feature: ctx.uid,
            frame,
            regions: std::slice::from_ref(&region),
            direction: frame.normal(),
            start: ExtrudeStart::Offset(0.0),
            side1: ExtrudeSide {
                end: SideEnd::Distance(thickness),
                taper: 0.0,
                thin: None,
            },
            side2: None,
        })
        .map_err(kernel_error)
}

impl<K: Kernel> Evaluate<K> for EndCapDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (thickness, inset, ratio, chamfer) = self.values(ctx)?;
        let uid = ctx.uid;
        let inward = self.direction == CapDirection::Inward;
        if let (Some(member), Some(end)) = (self.member, self.end) {
            let (component, def) = ctx
                .earlier_feature(member)
                .ok_or_else(|| format!("{} is not in the design", ctx.feature_name(member)))?;
            let name = ctx.feature_name(member);
            let FeatureDef::DerivedPart(part) = def else {
                return Err(format!("{name} is no profile placed by the Build tab"));
            };
            // A mitered end gets no cap.
            if let Some(cut) = part.ends.end(end.is_b()) {
                let angle = ctx.param(cut.angle)?;
                if angle.abs() > 1e-12 {
                    return Err(format!(
                        "end {} of {name} is mitered: an end cap goes on a square end",
                        end.letter()
                    ));
                }
            }
            let frame = ctx.mount(member, end.frame())?;
            let body = ctx.body_in(component, BodyUid::new(member, 0))?;
            let section = member_section(ctx.kernel, &body, &frame, &end_face(member, end))?;
            let plate = plate_of(&section, thickness, inset, ratio, chamfer)?;
            // In the end frame's coordinates: z out of the end; an inward
            // plate fills the member's last `thickness`, which its derived
            // part cuts back.
            let sketch = SketchFrame {
                origin: [0.0, 0.0, if inward { -thickness } else { 0.0 }],
                x_axis: [1.0, 0.0, 0.0],
                y_axis: [0.0, 1.0, 0.0],
            };
            let shape = plate_solid(ctx, &plate, sketch, thickness)?;
            let mut mounts = std::collections::BTreeMap::new();
            mounts.insert(SEAT.to_owned(), seat_frame());
            return Ok(FeatureOutput {
                changes: vec![BodyChange::Set(BodyUid::new(uid, 0), shape)],
                mounts,
                ..FeatureOutput::default()
            });
        }
        let Some(GeomRef::Face { body, face }) = &self.face else {
            return Err("an end cap is on a profile's end or on a planar face".to_owned());
        };
        let member = ctx.body(*body)?;
        let (frame, section) = face_section(ctx.kernel, &member, face)?;
        let plate = plate_of(&section, thickness, inset, ratio, chamfer)?;
        let [origin, x, y, z] = crate::build::parts(&frame);
        let start = if inward {
            add(origin, scale(z, -thickness))
        } else {
            origin
        };
        let sketch = SketchFrame {
            origin: start,
            x_axis: x,
            y_axis: y,
        };
        let shape = plate_solid(ctx, &plate, sketch, thickness)?;
        let mut changes = Vec::new();
        if inward {
            // The member cut back by the plate's box, a little wider than
            // the section and past the face.
            let margin = 1.0 + 0.01 * section.width().max(section.height());
            let corner = add(
                start,
                add(
                    scale(x, section.min[0] - margin),
                    scale(y, section.min[1] - margin),
                ),
            );
            let spec = PrimitiveSpec {
                feature: uid,
                frame: SketchFrame {
                    origin: corner,
                    x_axis: x,
                    y_axis: y,
                },
                shape: PrimitiveShape::Box {
                    length: section.width() + 2.0 * margin,
                    width: section.height() + 2.0 * margin,
                    height: thickness + margin,
                },
            };
            let tool = ctx.kernel.primitive(&spec).map_err(kernel_error)?;
            let tool = ctx
                .kernel
                .rename_faces(&tool, &format!("{uid}:cut_back"), &[])
                .map_err(kernel_error)?;
            let output = ctx
                .boolean(BooleanOp::Cut, &[&member], &tool)
                .map_err(|e| format!("the cut back: {}", kernel_error(e)))?;
            if output.touched.first().copied().unwrap_or(false) {
                let piece = output
                    .pieces
                    .into_iter()
                    .find(|p| p.sources.contains(&0))
                    .map(|p| p.shape)
                    .ok_or("the cut back leaves nothing of the member")?;
                changes.push(BodyChange::Set(*body, piece));
            } else {
                ctx.warn("the cut back does not meet the member");
            }
        }
        changes.push(BodyChange::Set(BodyUid::new(uid, 0), shape));
        Ok(FeatureOutput {
            changes,
            ..FeatureOutput::default()
        })
    }
}

/// A BUILD end cap's plate as the lists give it (mitcad#2243).
#[derive(Debug, Clone, PartialEq)]
pub struct CapPlate {
    pub plate: Plate,
    pub thickness: f64,
    /// The member's component and the end.
    pub member: ComponentUid,
    pub end: CapEnd,
    pub direction: CapDirection,
}

impl CapPlate {
    /// The plate's volume, mm³.
    pub fn volume(&self) -> f64 {
        self.plate.area() * self.thickness
    }
}

impl<K: Kernel> crate::document::Document<K> {
    /// The plate of a BUILD end cap's component at the marker: from its
    /// `end_cap` feature's values and the member's body and end frame as
    /// they are now; None for another component or one whose plate cannot
    /// be made (its feature fails then).
    pub fn end_cap_plate(&self, component: ComponentUid) -> Option<CapPlate> {
        let state = self.state();
        let record = state.assembly.component(component)?.build.as_ref()?;
        let uid = record.part?;
        let FeatureDef::EndCap(def) = &state.entry(uid)?.def else {
            return None;
        };
        let (member_part, end) = (def.member?, def.end?);
        let member = state.entry(member_part)?.component;
        let params = self.parameters();
        let value = |id: ParamId| params.value(id);
        let thickness = value(def.thickness)?;
        let inset = def.inset.and_then(value);
        let ratio = def.ratio.and_then(value);
        let chamfer = def.chamfer.and_then(value);
        let frame = self
            .last_result()
            .result(member_part)
            .filter(|r| r.status.is_ok())
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.mounts.as_ref())
            .and_then(|m| m.get(end.frame()).copied())?;
        let body = self.body_shape(BodyUid::new(member_part, 0))?;
        let section =
            member_section(self.kernel(), body, &frame, &end_face(member_part, end)).ok()?;
        let plate = plate_of(&section, thickness, inset, ratio, chamfer).ok()?;
        Some(CapPlate {
            plate,
            thickness,
            member,
            end,
            direction: def.direction,
        })
    }

    /// A BUILD end cap's designation in the lists: `Plate 40 x 40 x 5`,
    /// a disc's `Plate D46.3 x 5` (its diameter and thickness, mm).
    pub fn end_cap_designation(&self, component: ComponentUid) -> Option<String> {
        let cap = self.end_cap_plate(component)?;
        Some(cap.designation())
    }
}

impl CapPlate {
    /// `Plate 40 x 40 x 5`, a disc's `Plate D46.3 x 5`.
    pub fn designation(&self) -> String {
        let size = if self.plate.is_disc() {
            format!("D{}", mm(self.plate.width()))
        } else {
            format!("{} x {}", mm(self.plate.width()), mm(self.plate.height()))
        };
        format!("Plate {size} x {}", mm(self.thickness))
    }
}

/// The box of a plate in its cap's coordinates (z from 0 or −thickness).
pub fn plate_box(plate: &Plate, thickness: f64, direction: CapDirection) -> BoundingBox {
    let z0 = if direction == CapDirection::Inward {
        -thickness
    } else {
        0.0
    };
    BoundingBox {
        min: [plate.min[0], plate.min[1], z0],
        max: [plate.max[0], plate.max[1], z0 + thickness],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tube(r: f64) -> Section {
        // A 40 x 40 tube centred on its axis, its outer corners rounded by
        // r, its inner ones (a wall of 3) by r - 3.
        let o = 20.0 - r;
        let i = 17.0 - (r - 3.0).max(0.0);
        let mut circles = Vec::new();
        for [sx, sy] in CORNERS {
            circles.push(([sx * o, sy * o], r));
            circles.push(([sx * i, sy * i], (r - 3.0).max(0.0)));
        }
        Section::new([-20.0, -20.0], [20.0, 20.0], &circles)
    }

    #[test]
    fn a_square_tube_s_outer_rounds_make_the_plate_s_corners() {
        let section = tube(6.0);
        assert_eq!(section.rounds, [6.0; 4]);
        assert!(!section.is_round());
        let plate = plate_of(&section, 5.0, None, None, None).unwrap();
        let area = 1600.0 - (4.0 - std::f64::consts::PI) * 36.0;
        assert!((plate.area() - area).abs() / area < 1e-12);
        // Inset 1 mm: 38 x 38, the rounds 5.
        let inset = plate_of(&section, 5.0, Some(1.0), None, None).unwrap();
        assert!((inset.width() - 38.0).abs() < 1e-12);
        assert_eq!(inset.corners, [Corner::Round(5.0); 4]);
        // A ratio of the thickness: 0.2 x 5 = 1 mm.
        assert_eq!(
            plate_of(&section, 5.0, None, Some(0.2), None).unwrap(),
            inset
        );
        // A chamfer of 6 mm cuts the rounds away: 2 c² less.
        let chamfered = plate_of(&section, 5.0, None, None, Some(6.0)).unwrap();
        assert!((chamfered.area() - (1600.0 - 2.0 * 36.0)).abs() < 1e-9);
        // The outline's region: four arcs and four sides.
        assert_eq!(plate.region().loops[0].segments.len(), 8);
        assert_eq!(chamfered.region().loops[0].segments.len(), 8);
    }

    #[test]
    fn refusals_are_in_words() {
        let section = tube(6.0);
        let small = plate_of(&section, 5.0, None, None, Some(3.0)).unwrap_err();
        assert!(
            small.contains("less than the corners' round (6 mm)"),
            "{small}"
        );
        let wide = plate_of(&section, 5.0, None, None, Some(20.0)).unwrap_err();
        assert!(wide.contains("leaves no straight side"), "{wide}");
        let deep = plate_of(&section, 5.0, Some(20.0), None, None).unwrap_err();
        assert!(
            deep.contains("leaves nothing of the 40 x 40 mm section"),
            "{deep}"
        );
        assert!(plate_of(&section, 0.0, None, None, None).is_err());
        assert!(plate_of(&section, 5.0, Some(1.0), Some(0.2), None).is_err());
    }

    #[test]
    fn a_round_tube_makes_a_disc_and_no_chamfer() {
        let section = Section::new(
            [-24.15, -24.15],
            [24.15, 24.15],
            &[([0.0, 0.0], 24.15), ([0.0, 0.0], 20.95)],
        );
        assert!(section.is_round());
        let plate = plate_of(&section, 4.0, Some(1.0), None, None).unwrap();
        assert!(plate.is_disc());
        let r: f64 = 23.15;
        assert!((plate.area() - std::f64::consts::PI * r * r).abs() < 1e-9);
        assert_eq!(plate.region().loops[0].segments.len(), 1);
        let refused = plate_of(&section, 4.0, None, None, Some(5.0)).unwrap_err();
        assert!(
            refused.contains("round section has no corners"),
            "{refused}"
        );
    }

    #[test]
    fn an_angle_s_plate_is_the_rectangle_round_it() {
        // An L 50 x 50 x 5 with its heel at (min, min): its root and toe
        // rounds lie inside, so the rectangle's corners stay sharp.
        let section = Section::new(
            [0.0, 0.0],
            [50.0, 50.0],
            &[([12.0, 12.0], 7.0), ([1.5, 48.5], 3.5), ([48.5, 1.5], 3.5)],
        );
        assert_eq!(section.rounds, [0.0; 4]);
        let plate = plate_of(&section, 5.0, None, None, None).unwrap();
        assert!((plate.area() - 2500.0).abs() < 1e-9);
        assert_eq!(plate.region().loops[0].segments.len(), 4);
    }
}
