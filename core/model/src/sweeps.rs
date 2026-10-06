// SPDX-License-Identifier: MIT
//! Kernel inputs of the sweep family (F3): profiles swept along paths
//! (sweep), lofts, pipes, coils, ribs and webs. Millimetres and
//! radians, model coordinates. The feature definitions are in
//! `features/{sweep,loft,pipe,coil,rib}.rs`; `commands.md` describes their
//! semantics and face names.

use serde::{Deserialize, Serialize};

use crate::ids::FeatureUid;
use crate::kernel::Curve3;
use crate::profile::{ProfileRegion, SketchFrame};
use crate::topo::FaceName;

/// A curve of a path and its name: a sketch curve (`c3`) or an edge name.
#[derive(Debug, Clone, PartialEq)]
pub struct PathCurve {
    pub name: String,
    pub curve: Curve3,
}

/// How a swept profile turns along its path (the orientation).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepOrientation {
    /// The profile keeps its angle to the path's tangent.
    #[default]
    Perpendicular,
    /// The profile keeps its direction; it only moves along the path.
    Parallel,
}

/// How a guide rail sizes a swept profile (profile scaling): by
/// the ratio of the rail's distance from the path to its distance at the
/// profile, in both directions of the section (`scale`), only towards the
/// rail (`stretch`) or not at all (`none`: the rail only turns it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileScaling {
    #[default]
    Scale,
    Stretch,
    None,
}

/// A guide rail of a sweep.
#[derive(Debug, Clone, Copy)]
pub struct SweepGuide<'a> {
    pub rail: &'a [PathCurve],
    pub scaling: ProfileScaling,
}

/// Input of [`crate::Kernel::sweep`]: the regions swept along the path. The
/// sweep runs from where the path meets the profile's plane (the point
/// nearest to the profile when it does not) `extent1` of the path's length
/// beyond it towards the path's end and `extent2` of the length before it
/// towards its start (both fractions of those parts; on a closed path of
/// the whole length, together at most 1). `twist` turns the profile about
/// the path by that angle over the swept length (right hand about the
/// path's direction); `taper` widens it (positive) or narrows it: the
/// profile point farthest from the path moves outward by `s tan(taper)` at
/// distance `s` along the path. Both are ignored with a guide rail.
///
/// Faces: `side(<segment>)` from each profile segment (`#k` pieces where
/// the path has several curves), caps `start(<region>)` and
/// `end(<region>)`: with the profile at an end of the swept part `start` is
/// at the profile, otherwise `start` ends side one and `end` side two, as
/// for extrusions. A sweep around a whole closed path has no caps.
#[derive(Debug, Clone, Copy)]
pub struct SweepSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    /// Connected curves in order, each running along the path.
    pub path: &'a [PathCurve],
    pub extent1: f64,
    pub extent2: f64,
    pub orientation: SweepOrientation,
    pub twist: f64,
    pub taper: f64,
    pub guide: Option<SweepGuide<'a>>,
}

/// A section of a loft.
#[derive(Debug, Clone, Copy)]
pub enum LoftSection<'a, S> {
    /// A sketch region (its outer loop; regions with holes are not lofted).
    Region {
        frame: SketchFrame,
        region: &'a ProfileRegion,
    },
    /// A face of a body (its outer loop).
    Face { body: &'a S, face: &'a FaceName },
    /// A point, at the start or the end.
    Point([f64; 3]),
}

/// What an end condition of a loft imposes (see [`LoftEnd`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoftEndKind {
    #[default]
    Free,
    Direction,
    Tangent,
    Smooth,
    PointSharp,
    PointTangent,
}

/// How a loft leaves its first section or reaches its last one. The
/// takeoff is the derivative of the surface across the section, from the
/// section into the loft, over the span to the next section `weight` times
/// a length (`commands.md`, `loft`): `Direction` (a sketch region) at
/// `angle` radians from the region's normal, tilted out of the region for
/// a positive angle, the distance between the sections'
/// centres; `Tangent` and `Smooth` (a face) continuing each face next to
/// the section across its edge, G1 or G2, the distance between the
/// sections' corresponding vertices; `PointTangent` (a point) a rounded
/// tip whose tangent plane is at right angles to the line to the next
/// section's centre, each point's distance from that line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoftEnd {
    pub kind: LoftEndKind,
    pub angle: f64,
    pub weight: f64,
}

impl Default for LoftEnd {
    fn default() -> Self {
        Self {
            kind: LoftEndKind::Free,
            angle: 0.0,
            weight: 1.0,
        }
    }
}

/// Input of [`crate::Kernel::loft`]: a surface through the sections in
/// order, made solid. `ruled` joins neighbouring sections with straight
/// lines; `closed` runs from the last section back to the first (no caps).
/// A centre line keeps the sections at right angles to it on the way.
/// Rails are curves the loft's surface passes through: each must meet
/// every section and run through them in order; they split the sections'
/// edges where they meet them. End conditions and rails need an open,
/// smooth loft without a centre line, rails sections that are not points.
///
/// Faces: `side(<key>)` from the edges of the first section that is not a
/// point (`#k` pieces), keyed by segment for a region and by edge name for
/// a face; caps `start(<key>)` and `end(<key>)` at the first and last
/// sections, keyed by the region or the face name (points have none).
#[derive(Debug, Clone)]
pub struct LoftSpec<'a, S> {
    pub feature: FeatureUid,
    pub sections: Vec<LoftSection<'a, S>>,
    pub ruled: bool,
    pub closed: bool,
    pub centerline: Option<&'a [PathCurve]>,
    pub rails: Vec<&'a [PathCurve]>,
    pub start: LoftEnd,
    pub end: LoftEnd,
}

/// The section of a pipe: a circle of diameter `size`, or a square or an
/// equilateral triangle inside a circle of that diameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipeSection {
    #[default]
    Circular,
    Square,
    Triangular,
}

/// Input of [`crate::Kernel::pipe`]: the section at right angles to the
/// path at its start, swept `extent1` of the path's length along it (and on
/// a closed path `extent2` of it backwards from the start). A square's sides
/// and a triangle's first corner follow the path's plane: a corner of the
/// triangle points along the normal of the path in its plane. A `thickness`
/// makes it hollow, the wall inside the section.
///
/// Faces: `side<i>` for the section's edges (`side0` for a circle, the
/// square's and the triangle's sides counter-clockwise about the path),
/// `inner<i>` for a hollow pipe's inside, `start` and `end` caps as for a
/// sweep.
#[derive(Debug, Clone, Copy)]
pub struct PipeSpec<'a> {
    pub feature: FeatureUid,
    pub path: &'a [PathCurve],
    pub extent1: f64,
    pub extent2: f64,
    pub section: PipeSection,
    pub size: f64,
    pub thickness: Option<f64>,
}

/// The section of a coil: a circle of diameter `size`, or a square or an
/// equilateral triangle inside a circle of that diameter, a corner pointing
/// out of the coil (`triangular_external`) or into it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoilSection {
    #[default]
    Circular,
    Square,
    TriangularExternal,
    TriangularInternal,
}

/// Where a coil's section lies relative to its diameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoilPosition {
    /// Inside the diameter, touching it.
    Inside,
    /// Its centre on the diameter.
    #[default]
    OnCenter,
    /// Outside the diameter, touching it.
    Outside,
}

/// Input of [`crate::Kernel::coil`]: a helix about the frame's normal
/// through its origin, starting on its x axis and turning counter-clockwise
/// about the normal (right hand) unless `clockwise`, `revolutions` turns
/// rising `pitch` per turn; a positive `angle` widens it as it rises (a
/// cone). A `spiral` lies in the frame's plane and grows `pitch` in radius
/// per turn. The section is at right angles to the helix.
///
/// Faces: `side<i>` for the section's edges, caps `start` and `end`.
#[derive(Debug, Clone, Copy)]
pub struct CoilSpec {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub diameter: f64,
    pub revolutions: f64,
    pub pitch: f64,
    pub angle: f64,
    pub spiral: bool,
    pub clockwise: bool,
    pub section: CoilSection,
    pub position: CoilPosition,
    pub size: f64,
}

/// Where the thickness of a rib or a web lies: both ways from its curves,
/// on side 1 or on side 2. For a rib side 1 is along the sketch normal; for
/// a web it is to the left of each curve looking along it (the sketch
/// normal up).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThicknessLocation {
    #[default]
    Symmetric,
    Side1,
    Side2,
}

/// Input of [`crate::Kernel::rib`]. A rib grows from one open chain of
/// sketch curves in the sketch plane, at right angles to the chain's chord
/// (to its left, the sketch normal up, or to its right with `flip`); its
/// thickness is across the sketch plane. A web grows from each chain (lines
/// and arcs) along the sketch normal (against it with `flip`); its
/// thickness is in the sketch plane. Either runs `depth`, or without one
/// up to the faces of `bodies` it reaches first; it must then be closed off
/// by them.
///
/// Faces of a rib: `side(<curve>)` along the curves, `wall1` and `wall2`
/// (its sides along and against the sketch normal), `tip0` and `tip1` at
/// the chain's start and end, `end` at the depth. Of a web, per curve:
/// `wall1(<curve>)` and `wall2(<curve>)` (left and right), `tip0(<curve>)`
/// and `tip1(<curve>)`, `start(<curve>)` in the sketch plane and
/// `end(<curve>)` at the depth.
#[derive(Debug, Clone)]
pub struct RibSpec<'a, S> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub web: bool,
    pub chains: &'a [Vec<PathCurve>],
    pub thickness: f64,
    pub location: ThicknessLocation,
    pub depth: Option<f64>,
    pub flip: bool,
    pub bodies: Vec<S>,
}

/// Input of [`crate::Kernel::helix`] (FreeCAD's additive and subtractive
/// helices, mitcad#4): the regions turned about the axis through `origin`
/// along `direction` (a unit vector) while they move along it, a screw
/// motion of `revolutions` turns rising `pitch` each, right-handed about
/// the direction unless `left_handed`. Every section of the result in a
/// plane through the axis is the profile turned there.
///
/// Faces: `side(<segment>)` from each profile segment, caps
/// `start(<region>)` at the profile and `end(<region>)` at the far end.
#[derive(Debug, Clone, Copy)]
pub struct HelixSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    pub origin: [f64; 3],
    pub direction: [f64; 3],
    pub pitch: f64,
    pub revolutions: f64,
    pub left_handed: bool,
}
