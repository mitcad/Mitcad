// SPDX-License-Identifier: MIT
//! Surface modelling (mitcad#455): what the kernel is given to make and
//! change surface bodies, family by family. The features are in
//! `features/`; the kernel's methods are grouped under "Surface modelling"
//! in [`crate::kernel::Kernel`].

use serde::{Deserialize, Serialize};

use crate::datum::Vec3;
use crate::ids::FeatureUid;
use crate::topo::{EdgeName, FaceName};

/// Free edges of a body (they have no names of their own): the edges of
/// the open boundary with this index among
/// [`crate::kernel::Kernel::open_boundaries`], only those of the named
/// faces when any are given, and of those only the edge nearest each point
/// in `at` when any are given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpeningEdges<'a> {
    pub index: usize,
    pub faces: &'a [FaceName],
    pub at: &'a [Vec3],
}

// Offset Surface, Ruled Surface and Extend.

/// Which way a ruled surface leaves the chain of edges it hangs off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuledMode {
    /// Along the normal of the face beside each edge.
    #[default]
    Normal,
    /// Continuing the face beside each edge, square to the edge, away from
    /// the face.
    Tangent,
    /// At `angle` from the face's normal (or from `direction`), turned
    /// towards the tangent mode's direction.
    Angle,
    /// Along a fixed direction.
    Direction,
}

impl RuledMode {
    pub fn is_normal(&self) -> bool {
        *self == Self::Normal
    }
}

/// How neighbouring strips of a ruled surface meet at a sharp corner of
/// the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuledCorners {
    /// The strips' ends lean to meet along the corner's bisector.
    #[default]
    Mitre,
    /// A sector turned about the corner fills the gap.
    Round,
}

impl RuledCorners {
    pub fn is_mitre(&self) -> bool {
        *self == Self::Mitre
    }
}

/// Input of [`crate::kernel::Kernel::ruled_surface`]: a strip `distance`
/// wide hanging off a chain of the body's edges.
#[derive(Debug, Clone, Copy)]
pub struct RuledSpec<'a, S> {
    pub feature: FeatureUid,
    pub body: &'a S,
    /// Edges in no particular order that join into chains (a strip each), named and of
    /// openings.
    pub edges: &'a [EdgeName],
    pub openings: &'a [OpeningEdges<'a>],
    /// The face beside the edges where two meet there (else the one face
    /// of each edge).
    pub face: Option<&'a FaceName>,
    pub distance: f64,
    pub mode: RuledMode,
    /// Radians (the angle mode).
    pub angle: f64,
    /// The fixed direction (direction mode) or the angle's reference.
    pub direction: Option<Vec3>,
    /// The other way.
    pub flip: bool,
    pub corners: RuledCorners,
}

/// How Extend continues a surface past its edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtendMode {
    /// The surface itself, continued.
    #[default]
    Natural,
    /// Tangent to the surface across the edge (G1).
    Tangent,
    /// Tangent and of the same curvature across the edge (G2).
    Curvature,
}

impl ExtendMode {
    pub fn is_natural(&self) -> bool {
        *self == Self::Natural
    }

    /// `natural`, `tangent` or `curvature`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Natural => "natural",
            Self::Tangent => "tangent",
            Self::Curvature => "curvature",
        }
    }
}

// Trim, Untrim, Unstitch and Reverse Normal.

/// What a trim cuts a surface body with: a face operation's tool (a
/// plane, a face extended along its surface, a body, sketch regions swept
/// along a direction) or chains of sketch curves (open or closed) swept
/// both ways along `direction`.
pub enum TrimTool<'a, S> {
    Tool(crate::kernel::ToolInput<'a, S>),
    Curves {
        chains: &'a [Vec<crate::sweeps::PathCurve>],
        direction: Vec3,
    },
}

/// Input of [`crate::kernel::Kernel::trim_surface`].
pub struct TrimSpec<'a, S> {
    pub body: &'a S,
    pub tool: TrimTool<'a, S>,
    /// A face tool extended along its surface.
    pub extend: bool,
    /// Points that pick the pieces kept: the piece nearest each.
    pub keep: &'a [Vec3],
}

// Blend Surface, Boundary Surface, Match Surface and Fair Surface.

/// Edges of a body that join into one chain: named edges and free edges
/// of openings.
#[derive(Debug, Clone, Copy)]
pub struct BodyEdges<'a, S> {
    pub body: &'a S,
    pub edges: &'a [EdgeName],
    pub openings: &'a [OpeningEdges<'a>],
    /// The face beside the chain where an edge lies between two faces
    /// (else each edge's one face).
    pub face: Option<&'a FaceName>,
}

/// One end of a blend ([`BlendSpec`]).
#[derive(Debug, Clone, Copy)]
pub struct BlendEnd<'a, S> {
    pub edges: BodyEdges<'a, S>,
    /// How much of the face beside the chain the blend repeats: through
    /// the edges (G0), tangent (G1) or also curved as the face (G2).
    pub continuity: crate::surfaces::PatchContinuity,
    /// How full the blend leaves this end: 0 the plain bridge, more fuller,
    /// less (down to -0.9) flatter.
    pub bulge: f64,
}

/// Input of [`crate::kernel::Kernel::blend_surface`]: a surface bridging
/// two chains of edges.
#[derive(Debug, Clone, Copy)]
pub struct BlendSpec<'a, S> {
    pub feature: FeatureUid,
    pub from: BlendEnd<'a, S>,
    pub to: BlendEnd<'a, S>,
}

/// A boundary of a boundary surface ([`BoundarySpec`]).
#[derive(Debug, Clone, Copy)]
pub enum BoundaryCurve<'a, S> {
    /// A chain of sketch curves, in order end to end, on the sketch's
    /// plane (`origin`, `normal`): G1 and G2 make the surface tangent to
    /// that plane along the curves.
    Curves {
        chain: &'a [crate::sweeps::PathCurve],
        origin: Vec3,
        normal: Vec3,
        continuity: crate::surfaces::PatchContinuity,
    },
    /// A chain of a body's edges, met with `continuity` by the face beside
    /// them.
    Edges {
        edges: BodyEdges<'a, S>,
        continuity: crate::surfaces::PatchContinuity,
    },
    /// A side collapsed to a point.
    Point(Vec3),
}

/// The curves a boundary surface runs through.
#[derive(Debug, Clone, Copy)]
pub enum BoundaryCurves<'a, S> {
    /// A closed loop of boundaries, in order, filled; the face runs through
    /// the `guides` inside it (mitcad#2526: chains of sketch curves, each
    /// curve a guide of its own, none for a plain fill), which neither
    /// bound nor split it.
    Loop {
        boundary: &'a [BoundaryCurve<'a, S>],
        guides: &'a [BoundaryCurve<'a, S>],
    },
    /// Curves one way (`u`, two or more, lofted through in order) and
    /// curves crossing them (`v`, none or two or more): a Coons patch of
    /// two each way, a Gordon surface through more.
    Network {
        u: &'a [BoundaryCurve<'a, S>],
        v: &'a [BoundaryCurve<'a, S>],
    },
}

/// Input of [`crate::kernel::Kernel::boundary_surface`].
#[derive(Debug, Clone, Copy)]
pub struct BoundarySpec<'a, S> {
    pub feature: FeatureUid,
    pub curves: BoundaryCurves<'a, S>,
}

/// How Fair Surface moves a B-spline face's control points
/// ([`crate::kernel::Kernel::fair_surface`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fairing {
    /// The share of a full smoothing step each iteration takes (0 to 1).
    pub strength: f64,
    pub iterations: u32,
    /// Also holds the row of control points next to each edge, so the face
    /// keeps its tangent there.
    pub keep_edges: bool,
    /// The farthest a control point may move (mm).
    pub tolerance: f64,
}

// Curves of free edges: a loft's section of a surface body's free edges
// (mitcad#2395).

/// A free edge's curve as [`crate::kernel::Kernel::opening_curves`] gives
/// it: in model space, as [`crate::kernel::Kernel::curves_of`] gives a
/// named edge's, with the name of the one face the edge bounds (none when
/// that face has no name).
#[derive(Debug, Clone, PartialEq)]
pub struct FreeEdgeCurve {
    pub curve: crate::kernel::Curve3,
    pub face: Option<FaceName>,
}
