// SPDX-License-Identifier: MIT
//! Named material charts and the two geometry operations that consume them.

use crate::ids::FeatureUid;
use crate::profile::{SegmentGeometry, SketchFrame};
use crate::topo::SegmentKey;
use crate::transform::Transform;
use serde::{Deserialize, Serialize};
use std::fmt;

// Oblique (eccentric) conical panels (mitcad#2491).
#[path = "sheet_oblique_cone.rs"]
mod oblique_cone;
pub use oblique_cone::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SheetDomainId {
    pub feature: FeatureUid,
    pub slot: u32,
}

impl fmt::Display for SheetDomainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.p{}", self.feature, self.slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SheetCurveId {
    Sketch {
        sketch: FeatureUid,
        segment: SegmentKey,
    },
    Generated {
        feature: FeatureUid,
        slot: u32,
    },
}

impl fmt::Display for SheetCurveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sketch { sketch, segment } => write!(f, "{sketch}/{segment}"),
            Self::Generated { feature, slot } => write!(f, "{feature}/g{slot}"),
        }
    }
}

pub type PanelId = SheetDomainId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BendId {
    pub feature: FeatureUid,
    pub slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum SheetDomainRef {
    Panel(PanelId),
    Bend(BendId),
}

impl SheetDomainRef {
    pub fn feature(self) -> FeatureUid {
        match self {
            Self::Panel(id) => id.feature,
            Self::Bend(id) => id.feature,
        }
    }
    pub fn slot(self) -> u32 {
        match self {
            Self::Panel(id) => id.slot,
            Self::Bend(id) => id.slot,
        }
    }
}

/// Directed native support parameters, not a normalized ancestry interval.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParamSpan {
    pub first: f64,
    pub last: f64,
}

impl ParamSpan {
    pub fn reversed(self) -> Self {
        Self {
            first: self.last,
            last: self.first,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurvePieceRef {
    pub curve: SheetCurveId,
    pub piece: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryPieceRef {
    pub domain: SheetDomainRef,
    pub piece: CurvePieceRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundedCurve2 {
    pub geometry: SegmentGeometry,
    pub range: ParamSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialEdge {
    pub piece: CurvePieceRef,
    pub curve: BoundedCurve2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialLoop {
    pub edges: Vec<MaterialEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialRegion {
    pub loops: Vec<MaterialLoop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Isometry2 {
    pub origin: [f64; 2],
    pub x_axis: [f64; 2],
    pub y_axis: [f64; 2],
}

impl Isometry2 {
    pub const IDENTITY: Self = Self {
        origin: [0.0, 0.0],
        x_axis: [1.0, 0.0],
        y_axis: [0.0, 1.0],
    };
    pub fn point(self, p: [f64; 2]) -> [f64; 2] {
        std::array::from_fn(|i| self.origin[i] + p[0] * self.x_axis[i] + p[1] * self.y_axis[i])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AxialMap {
    pub offset: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimDomain {
    pub domain: SheetDomainRef,
    pub regions: Vec<MaterialRegion>,
    pub to_operation: Isometry2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimSeamSide {
    pub boundary: BoundaryPieceRef,
    pub span: ParamSpan,
    pub axial: AxialMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimSeam {
    pub token: u32,
    pub first: TrimSeamSide,
    pub second: TrimSeamSide,
    pub axial_interval: ParamSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetTrimOperation {
    Union,
    Difference,
    Intersection,
}

pub struct SheetTrimSpec<'a> {
    pub operation: SheetTrimOperation,
    pub domains: &'a [TrimDomain],
    pub tools: &'a [MaterialRegion],
    pub seams: &'a [TrimSeam],
    pub tolerance: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrimSource {
    Domain { boundary: BoundaryPieceRef },
    Tool { piece: CurvePieceRef },
}

/// Coverage on the input curve. This is not an affine output-parameter map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub source: TrimSource,
    pub span: ParamSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimEdge {
    pub token: u32,
    pub curve: BoundedCurve2,
    pub lineage: Vec<SourceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimLoop {
    pub edges: Vec<TrimEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimRegion {
    pub token: u32,
    pub loops: Vec<TrimLoop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimDomainResult {
    pub domain: SheetDomainRef,
    pub regions: Vec<TrimRegion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimEndpoint {
    pub domain: SheetDomainRef,
    pub edge_token: u32,
    pub parameter: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeamEventKind {
    Crossing,
    Tangency,
    Endpoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeamSplit {
    pub seam_token: u32,
    pub split_token: u32,
    pub axial: f64,
    pub kind: SeamEventKind,
    pub first: Vec<TrimEndpoint>,
    pub second: Vec<TrimEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimEndpointSpan {
    pub domain: SheetDomainRef,
    pub edge_token: u32,
    pub range: ParamSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimPortSpan {
    pub seam_token: u32,
    pub axial_interval: ParamSpan,
    pub first: TrimEndpointSpan,
    pub second: TrimEndpointSpan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetTrimResult {
    pub domains: Vec<TrimDomainResult>,
    pub splits: Vec<SeamSplit>,
    pub ports: Vec<TrimPortSpan>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CylinderEmbedding {
    pub centre: [f64; 3],
    pub axis: [f64; 3],
    pub radial_start: [f64; 3],
    pub angular_direction: [f64; 3],
    pub inside_radius: f64,
    pub neutral_radius: f64,
    pub angle_extent: f64,
    pub inner_is_front: bool,
}

/// A conical panel's chart (mitcad#2435): the development of its neutral
/// cone about the apex, lengths and angles kept. Chart point `(x, y)` is at
/// the distance `rho = hypot(x, y)` from `apex` along the generator and the
/// developed angle `phi = atan2(y, x)` (taken in `[-w, 2 pi - w)`, `w` half
/// the angle the development leaves free, `pi - angle_extent / 2`), which
/// is the angle `phi / sin(half_angle)` about `axis` from `radial_start`
/// towards `angular_direction`. The material lies from `neutral_depth`
/// inside the neutral cone (towards the axis) to `thickness -
/// neutral_depth` outside it; `neutral_depth` is K t.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConeEmbedding {
    /// The apex of the neutral cone.
    pub apex: [f64; 3],
    /// From the apex where the radius grows (a unit vector).
    pub axis: [f64; 3],
    pub radial_start: [f64; 3],
    pub angular_direction: [f64; 3],
    /// Radians, strictly between 0 and pi/2.
    pub half_angle: f64,
    /// From the inner surface to the neutral one, mm.
    pub neutral_depth: f64,
    /// The developed angle the chart's material lies in, from 0 (radians,
    /// less than `2 pi sin(half_angle)`).
    pub angle_extent: f64,
    pub inner_is_front: bool,
}

impl ConeEmbedding {
    /// The developed angle of chart point `p` in the chart's window.
    pub fn developed_angle(&self, p: [f64; 2]) -> f64 {
        let free = std::f64::consts::PI - self.angle_extent / 2.0;
        let mut phi = p[1].atan2(p[0]);
        if phi < -free {
            phi += std::f64::consts::TAU;
        }
        phi
    }

    /// The model point of chart point `p` at `depth` from the front
    /// surface towards the back (`0..=thickness`).
    pub fn point(&self, p: [f64; 2], depth: f64, thickness: f64) -> [f64; 3] {
        let (frame, offset) = self.frame(p, depth, thickness);
        let rho = p[0].hypot(p[1]);
        let (s, c) = self.half_angle.sin_cos();
        let generator: [f64; 3] = std::array::from_fn(|i| s * frame[0][i] + c * self.axis[i]);
        let normal: [f64; 3] = std::array::from_fn(|i| c * frame[0][i] - s * self.axis[i]);
        std::array::from_fn(|i| self.apex[i] + rho * generator[i] + offset * normal[i])
    }

    /// The radial and the angular direction about the axis at chart point
    /// `p`, and the offset from the neutral cone (outwards, mm) of `depth`
    /// from the front surface.
    pub fn frame(&self, p: [f64; 2], depth: f64, thickness: f64) -> ([[f64; 3]; 2], f64) {
        let theta = self.developed_angle(p) / self.half_angle.sin();
        let (s, c) = theta.sin_cos();
        let radial =
            std::array::from_fn(|i| c * self.radial_start[i] + s * self.angular_direction[i]);
        let angular =
            std::array::from_fn(|i| -s * self.radial_start[i] + c * self.angular_direction[i]);
        let front = if self.inner_is_front {
            -self.neutral_depth
        } else {
            thickness - self.neutral_depth
        };
        let offset = if self.inner_is_front {
            front + depth
        } else {
            front - depth
        };
        ([radial, angular], offset)
    }

    /// The chart point and the depth from the front surface of a model
    /// point, or None at the axis or outside the chart's angle.
    pub fn coordinates(&self, point: [f64; 3], thickness: f64) -> Option<([f64; 2], f64)> {
        let d: [f64; 3] = std::array::from_fn(|i| point[i] - self.apex[i]);
        let along = d[0] * self.axis[0] + d[1] * self.axis[1] + d[2] * self.axis[2];
        let w: [f64; 3] = std::array::from_fn(|i| d[i] - along * self.axis[i]);
        let r = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        if r <= 1e-12 {
            return None;
        }
        let x =
            w[0] * self.radial_start[0] + w[1] * self.radial_start[1] + w[2] * self.radial_start[2];
        let y = w[0] * self.angular_direction[0]
            + w[1] * self.angular_direction[1]
            + w[2] * self.angular_direction[2];
        let mut theta = y.atan2(x);
        let (s, c) = self.half_angle.sin_cos();
        let turn = std::f64::consts::TAU;
        let free = (turn - self.angle_extent / s) / 2.0;
        if theta < -free {
            theta += turn;
        }
        let phi = theta * s;
        if phi < -1e-9 || phi > self.angle_extent + 1e-9 {
            return None;
        }
        let rho = r * s + along * c;
        let offset = r * c - along * s;
        let front = if self.inner_is_front {
            -self.neutral_depth
        } else {
            thickness - self.neutral_depth
        };
        let depth = if self.inner_is_front {
            offset - front
        } else {
            front - offset
        };
        Some(([rho * phi.cos(), rho * phi.sin()], depth))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialParity {
    Positive,
    Negative,
}

impl MaterialParity {
    pub fn sign(self) -> f64 {
        match self {
            Self::Positive => 1.0,
            Self::Negative => -1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SheetEmbedding {
    Plane {
        front_frame: SketchFrame,
        front_to_back_sign: MaterialParity,
    },
    Cylinder(CylinderEmbedding),
    /// A conical panel (mitcad#2435).
    Cone(ConeEmbedding),
    /// An oblique (eccentric) conical panel (mitcad#2491).
    ObliqueCone(ObliqueConeEmbedding),
    /// A Lofted Flange's transition (mitcad#2512).
    Lofted(LoftedEmbedding),
}

/// A Lofted Flange's chart (mitcad#2512): the development of the
/// transition's neutral surface, made of flat facets and of regions
/// formed about corners and arcs, each laid flat in order from the first
/// ruling. The feature builds the folded body itself; the sheet
/// construction never takes this chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoftedEmbedding {
    /// The lines between the transition's regions on its neutral surface,
    /// in order: where they lie in the body and in the chart.
    pub rulings: Vec<LoftedRuling>,
    /// The lines the Flat Pattern marks, in chart coordinates.
    pub lines: Vec<LoftedLine>,
    /// The neutral surface lies `neutral_depth` (K t, mm) from the profile
    /// surface (the front) towards the back.
    pub neutral_depth: f64,
    /// The rules' K-factor the neutral surface follows.
    pub k_factor: f64,
    /// The regions between the facets are formed smooth (die formed), not
    /// folded on a press brake.
    pub die_formed: bool,
}

/// A line of a Lofted Flange's neutral surface from the first profile to
/// the second: its ends in the body (`model`) and in the chart (`chart`),
/// and the unit normal from the front towards the back there (on the
/// region after it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoftedRuling {
    pub model: [[f64; 3]; 2],
    pub chart: [[f64; 2]; 2],
    pub front_to_back: [f64; 3],
    /// How far behind the front `model` lies where that is not the
    /// neutral depth: a bend of the rules' radius whose inside is on the
    /// back develops at the K-factor from its inside (mitcad#2560).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<f64>,
}

/// A line a Lofted Flange's Flat Pattern marks: a fold between facets
/// (`fold`, with its signed angle, positive towards the back, and whether
/// its inside is on the front) or a line where a flat region meets a formed
/// one (angle 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoftedLine {
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub fold: bool,
    pub angle: f64,
    pub inside_front: bool,
    /// A fold made as a bend of this inside radius (mitcad#2560; 0: a
    /// sharp fold): `start`-`end` is its centre line, `tangents` the lines
    /// where it meets the facets beside it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub radius: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tangents: Vec<[[f64; 2]; 2]>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl SheetEmbedding {
    pub fn transformed(&self, t: &Transform) -> Self {
        match self {
            Self::Plane {
                front_frame,
                front_to_back_sign,
            } => Self::Plane {
                front_frame: t.apply_frame(front_frame),
                front_to_back_sign: *front_to_back_sign,
            },
            Self::Cylinder(c) => Self::Cylinder(CylinderEmbedding {
                centre: t.apply_point(c.centre),
                axis: t.apply_vector(c.axis),
                radial_start: t.apply_vector(c.radial_start),
                angular_direction: t.apply_vector(c.angular_direction),
                ..c.clone()
            }),
            Self::Cone(c) => Self::Cone(ConeEmbedding {
                apex: t.apply_point(c.apex),
                axis: t.apply_vector(c.axis),
                radial_start: t.apply_vector(c.radial_start),
                angular_direction: t.apply_vector(c.angular_direction),
                ..c.clone()
            }),
            Self::ObliqueCone(c) => Self::ObliqueCone(ObliqueConeEmbedding {
                apex: t.apply_point(c.apex),
                centre: t.apply_point(c.centre),
                radial_start: t.apply_vector(c.radial_start),
                angular_direction: t.apply_vector(c.angular_direction),
                ..c.clone()
            }),
            Self::Lofted(l) => Self::Lofted(LoftedEmbedding {
                rulings: l
                    .rulings
                    .iter()
                    .map(|r| LoftedRuling {
                        model: r.model.map(|p| t.apply_point(p)),
                        chart: r.chart,
                        front_to_back: t.apply_vector(r.front_to_back),
                        depth: r.depth,
                    })
                    .collect(),
                ..l.clone()
            }),
        }
    }
}

/// A cylindrical or conical face of a body developed into a sheet chart
/// ([`crate::kernel::Kernel::sheet_face_development`], mitcad#2435).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetFaceDevelopment {
    /// An oblique (eccentric) cone (mitcad#2491): the circle its
    /// generators run through, `origin` its apex (the faces' own),
    /// `radial_start` and `angular_direction` in the circle's plane (`axis`
    /// its normal), the angle round the circle; the chart the development
    /// of its neutral surface by its generators
    /// ([`ObliqueConeEmbedding`]). `half_angle` and `neutral_radius` are 0.
    pub oblique: Option<ObliqueCircle>,
    /// A cylinder: a point of its axis; a cone: the apex of the neutral
    /// cone.
    pub origin: [f64; 3],
    /// A cylinder's axis; a cone's, from the apex where its radius grows.
    pub axis: [f64; 3],
    /// Across the axis where the face starts (its least angle about it).
    pub radial_start: [f64; 3],
    /// Across the axis and `radial_start`: the way the face runs round.
    pub angular_direction: [f64; 3],
    /// The neutral cylinder's radius; 0 for a cone.
    pub neutral_radius: f64,
    /// A cone's half angle (radians); 0 for a cylinder.
    pub half_angle: f64,
    /// How far the faces run round their axis (radians, up to a full turn).
    pub turn: f64,
    /// The faces close on themselves round the axis (at a seam edge, or
    /// where the faces meet again).
    pub closed: bool,
    /// The faces' outward normal points to the axis: the front is the
    /// sheet's inner surface.
    pub inner_is_front: bool,
    /// Each face's edges in the chart: a cylinder's `(angle * neutral
    /// radius, along the axis from origin)`, a cone's development about the
    /// apex. A generator is a line and a circle round the axis a line (a
    /// cylinder's) or an arc about the apex (a cone's), from their exact
    /// ends; other edges are B-splines through their exact development,
    /// within 1e-8 mm of it. Such an edge whose face across it is a plane
    /// meeting the generators (an oblique cut) is where that plane meets
    /// the neutral surface, and a generator ending at it ends there
    /// (mitcad#2491).
    pub faces: Vec<Vec<BoundedCurve2>>,
}

/// How a face bends ([`crate::kernel::Kernel::face_bending`],
/// mitcad#2435): the largest Gaussian curvature (1/mm², its magnitude) at
/// points spread over the face, and the size of its box (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBending {
    pub gaussian: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetPatch {
    pub domain: SheetDomainRef,
    pub regions: Vec<MaterialRegion>,
    pub embedding: SheetEmbedding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapePortSide {
    pub boundary: BoundaryPieceRef,
    pub span: ParamSpan,
    pub axial: AxialMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapePort {
    pub token: u32,
    pub axial_interval: ParamSpan,
    pub first: ShapePortSide,
    pub second: ShapePortSide,
}

pub struct SheetShapeSpec<'a> {
    pub thickness: f64,
    pub patches: &'a [SheetPatch],
    pub ports: &'a [ShapePort],
    pub tolerance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RegionMembership {
    pub domain: SheetDomainRef,
    pub region: usize,
}

pub struct SheetSolid<S> {
    pub shape: S,
    pub membership: Vec<RegionMembership>,
}
pub struct SheetGeometry<S> {
    pub solids: Vec<SheetSolid<S>>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BendDomain {
    pub id: BendId,
    pub stationary: BoundaryPieceRef,
    pub moving: BoundaryPieceRef,
    pub folded_embedding: CylinderEmbedding,
    pub signed_angle: f64,
    pub k_factor: f64,
    pub radius: f64,
    pub allowance: f64,
    pub axial_interval: ParamSpan,
    pub rules: FeatureUid,
    pub rules_version: String,
    /// A kerf bend's slit lattice (mitcad#1939, a living hinge): the Flat
    /// Pattern cuts these slits across the bend's developed strip instead
    /// of giving it reliefs. None for every other bend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lattice: Option<SlitLattice>,
}

/// The slit lattice of a kerf bend (mitcad#1939): rows of slits along the
/// bend's axis, `pitch` apart across its developed width, each row slits
/// `slit_length` long with `gap` of material between them, every other
/// row shifted by half a period; `slit_width` is the cut's width (the
/// laser's kerf), which closes as the strip bends. mm.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlitLattice {
    pub slit_length: f64,
    pub gap: f64,
    pub pitch: f64,
    pub slit_width: f64,
}

impl SlitLattice {
    /// The smallest inside radius the lattice bends to, mm: a row's slit
    /// closes at most by its width, so the inside surface (half the
    /// thickness from the neutral surface in the middle) may shorten by
    /// at most `slit_width / pitch`: `(t / 2) / (r + t / 2) <= w / p`, so
    /// `r >= t p / (2 w) - t / 2` (never below zero).
    pub fn min_radius(&self, thickness: f64) -> f64 {
        (thickness * self.pitch / (2.0 * self.slit_width) - thickness / 2.0).max(0.0)
    }

    /// The slits of one row at `u` across a developed strip whose material
    /// along the axis is `intervals` (each `[from, to]`): row `row` (from
    /// 0) starts `gap` in from each interval's start, every other row half
    /// a period later; a slit is clipped to `gap` inside the interval's
    /// ends and left out when shorter than the gap.
    pub fn row(&self, row: usize, intervals: &[[f64; 2]]) -> Vec<[f64; 2]> {
        let period = self.slit_length + self.gap;
        let mut out = Vec::new();
        if period.is_nan() || period <= 0.0 {
            return out;
        }
        for &[from, to] in intervals {
            let (lo, hi) = (from.min(to) + self.gap, from.max(to) - self.gap);
            if hi - lo < self.gap {
                continue;
            }
            let mut start = if row.is_multiple_of(2) {
                lo
            } else {
                lo + period / 2.0 - period
            };
            while start < hi {
                let a = start.max(lo);
                let b = (start + self.slit_length).min(hi);
                if b - a >= self.gap.min(self.slit_length) - 1e-9 && b > a {
                    out.push([a, b]);
                }
                start += period;
            }
        }
        out
    }

    /// Where the rows of a strip `allowance` wide lie across it: as many
    /// rows `pitch` apart as fit, centred.
    pub fn rows(&self, allowance: f64) -> Vec<f64> {
        if self.pitch.is_nan() || self.pitch <= 0.0 || allowance.is_nan() || allowance <= 0.0 {
            return Vec::new();
        }
        let n = (allowance / self.pitch).floor() as usize;
        let n = n.max(1);
        let first = (allowance - (n as f64 - 1.0) * self.pitch) / 2.0;
        (0..n).map(|i| first + i as f64 * self.pitch).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenedBend {
    pub bend: BendId,
    pub by: FeatureUid,
    /// The panel held fixed when `by` opened the bend; its Refold holds the
    /// same panel so that refolding restores the folded placement exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stationary: Option<PanelId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurveLineage {
    pub boundary: BoundaryPieceRef,
    /// Content generation, not persistent identity: fragment aliases apply
    /// only to the exact bounded curve produced by this trim result.
    pub curve_generation: String,
    /// Transitive semantic identities. Source spans below describe only the
    /// immediate trim input; they never imply an ancestral parameter map.
    pub aliases: Vec<CurvePieceRef>,
    pub sources: Vec<SourceSpan>,
}

impl CurveLineage {
    pub fn generation(curve: &BoundedCurve2) -> String {
        let mut fp = crate::fingerprint::Fingerprint::new("sheet-curve-v1");
        serde_json::to_writer(&mut fp, curve).expect("bounded curve serializes");
        format!("{:032x}", fp.finish())
    }
    pub fn applies_to(&self, domain: SheetDomainRef, edge: &MaterialEdge) -> bool {
        self.boundary.domain == domain
            && self.boundary.piece == edge.piece
            && self.curve_generation == Self::generation(&edge.curve)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryRef {
    pub panel: PanelId,
    pub curve: SheetCurveId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<ParamSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BendPortMode {
    VirtualSharp,
    TangentPorts,
    /// Two opposite bends and a straight between them (a jog) joining two
    /// parallel panels of separate bodies (mitcad#280).
    Double,
}

/// Which boundary of a double bend keeps its panel whole (the first bend
/// starts on it); the other panel is cut back to where the second bend
/// ends (mitcad#280).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoubleBendFixed {
    #[default]
    Stationary,
    Moving,
}
