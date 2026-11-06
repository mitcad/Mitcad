// SPDX-License-Identifier: MIT
//! Typed definitions of the feature kinds the import maps (SolidCraft's
//! `FeatureKind` variants and the types they use, `crates/doc/src/document.rs`,
//! `construct.rs`). Expressions stay text ([`crate::expr`] translates them);
//! a plain number in an expression field is read as its text.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::document::Mat;
use crate::sketch::Sketch;

pub type Vec3 = [f64; 3];

/// An expression: a string, or a number written as one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Expr(pub String);

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Value::deserialize(d)? {
            Value::String(s) => Ok(Expr(s)),
            Value::Number(n) => Ok(Expr(n.to_string())),
            other => Err(D::Error::custom(format!("an expression, not {other}"))),
        }
    }
}

impl Expr {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An explicit plane: origin and axes (normal x × y).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Plane {
    pub origin: Vec3,
    pub x_dir: Vec3,
    pub y_dir: Vec3,
}

/// Where a sketch lies, or any plane a feature uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlaneRef {
    /// `XY`, `XZ` or `YZ`.
    Origin {
        name: String,
    },
    Custom {
        plane: Plane,
    },
    /// An explicit plane whose origin is length expressions.
    Placed {
        origin: [Expr; 3],
        x_dir: Vec3,
        y_dir: Vec3,
    },
    Offset {
        base: Box<PlaneRef>,
        distance: Expr,
    },
    /// Another plane moved along its normal through a point (format 36).
    ThroughPoint {
        base: Box<PlaneRef>,
        point: Box<GeoRef>,
    },
    /// Turned about a line (world coordinates) by an angle; with `line`, an
    /// edge or round face of the model found again (format 39), the axis
    /// where it was when picked.
    AtAngle {
        base: Box<PlaneRef>,
        axis_origin: Vec3,
        axis_dir: Vec3,
        angle: Expr,
        #[serde(default)]
        line: Option<Box<GeoRef>>,
    },
    /// A construction plane feature, by id (format 40; older designs' names
    /// are ids after the upgrade).
    Construction {
        feature: u64,
    },
    /// Seen from its other side: a half turn about its x axis, or its y
    /// axis with `about_y`.
    Flipped {
        base: Box<PlaneRef>,
        #[serde(default)]
        about_y: bool,
    },
    Midplane {
        a: Box<GeoRef>,
        b: Box<GeoRef>,
    },
    TwoEdges {
        a: Box<GeoRef>,
        b: Box<GeoRef>,
    },
    /// Through a line and a point off it (format 47).
    AxisPoint {
        axis: Box<GeoRef>,
        point: Box<GeoRef>,
    },
    ThreePoints {
        a: Box<GeoRef>,
        b: Box<GeoRef>,
        c: Box<GeoRef>,
    },
    /// Tangent to a round face over `at`; with a `reference` (format 48),
    /// where the face turns towards it, turned by `angle`.
    Tangent {
        face: Box<GeoRef>,
        at: Vec3,
        #[serde(default)]
        reference: Option<Box<GeoRef>>,
        #[serde(default)]
        angle: Option<Expr>,
    },
    AlongPath {
        path: Box<GeoRef>,
        t: Expr,
    },
    Perpendicular {
        plane: Box<GeoRef>,
        line: Box<GeoRef>,
    },
    /// A planar body face picked at `at`; `plane` is the plane it had.
    Face {
        plane: Plane,
        at: Vec3,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        anchor: Option<Value>,
    },
}

/// A construction reference.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeoRef {
    Point {
        p: Vec3,
    },
    Origin,
    Vertex {
        at: Vec3,
        #[serde(default)]
        edges: Option<Vec<String>>,
    },
    Edge {
        at: Vec3,
        #[serde(default)]
        name: Option<String>,
    },
    Face {
        at: Vec3,
        #[serde(default)]
        name: Option<String>,
    },
    Plane {
        plane: PlaneRef,
    },
    World {
        axis: String,
    },
    Axis {
        name: String,
    },
    ConstructPoint {
        name: String,
    },
    SketchPoint {
        sketch: u64,
        point: String,
    },
    SketchCurve {
        sketch: u64,
        curve: String,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisDef {
    ThroughFace { face: GeoRef },
    NormalToFace { face: GeoRef, point: GeoRef },
    TwoPlanes { a: GeoRef, b: GeoRef },
    TwoPoints { a: GeoRef, b: GeoRef },
    Edge { edge: GeoRef },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PointDef {
    At { point: GeoRef },
    TwoEdges { a: GeoRef, b: GeoRef },
    ThreePlanes { a: GeoRef, b: GeoRef, c: GeoRef },
    Center { of: GeoRef },
    EdgeAndPlane { edge: GeoRef, plane: GeoRef },
    AlongPath { path: GeoRef, t: Expr },
}

/// Which closed profiles of a sketch a feature uses.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProfileSel {
    #[default]
    All,
    /// By index in SolidCraft's solved profile list.
    Indices {
        indices: Vec<usize>,
    },
    /// Profiles whose outer loop is made of these curves (or their pieces).
    Curves {
        loops: Vec<Vec<String>>,
    },
    OffsetBoundaries {
        associations: Vec<String>,
    },
    /// Outer boundaries and holes made of exactly these curves.
    WholeBoundaries {
        profiles: Vec<WholeBoundary>,
    },
    /// The profile holding each sketch point.
    Points {
        points: Vec<[f64; 2]>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WholeBoundary {
    pub outer: Vec<String>,
    #[serde(default)]
    pub holes: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    #[default]
    NewBody,
    Join,
    Cut,
    Intersect,
}

impl Operation {
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::NewBody => "new_body",
            Operation::Join => "join",
            Operation::Cut => "cut",
            Operation::Intersect => "intersect",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Positive,
    Negative,
    Symmetric,
}

/// How a symmetric distance is read: per side (older designs) or in
/// total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeConvention {
    #[default]
    LegacyPerSide,
    TotalDistance,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Extent {
    pub distance: Expr,
    #[serde(default)]
    pub convention: ExtrudeConvention,
    #[serde(default)]
    pub direction: Direction,
    #[serde(default)]
    pub distance2: Option<Expr>,
    #[serde(default)]
    pub start_offset: Option<Expr>,
    #[serde(default)]
    pub through_all: bool,
    #[serde(default)]
    pub taper: Option<Expr>,
    /// To Object: the face (or point) picked here.
    #[serde(default)]
    pub to: Option<Vec3>,
    #[serde(default)]
    pub to_offset: Option<Expr>,
    /// From Object.
    #[serde(default)]
    pub from: Option<Vec3>,
    /// How a taper closes sharp corners (format 49): `preserve_curvature`,
    /// `tangent` or `round`; none: the kernel's choice per profile.
    #[serde(default)]
    pub corner: Option<String>,
    /// A Join To Object leaves the body it runs to apart (format 49).
    #[serde(default)]
    pub to_body_apart: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ThinWall {
    pub curves: Vec<String>,
    pub thickness: Expr,
    #[serde(default)]
    pub side: ThinSide,
    #[serde(default)]
    pub thickness2: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinSide {
    #[default]
    Center,
    Inside,
    Outside,
    TwoSides,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SketchFeature {
    pub plane: PlaneRef,
    pub sketch: Sketch,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Extrude {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    pub extent: Extent,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub thin: Option<ThinWall>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AxisRef {
    SketchLine {
        curve: String,
    },
    /// `x` or `y` of the sketch.
    SketchAxis {
        axis: String,
    },
    /// World `X`, `Y` or `Z`.
    World {
        axis: String,
    },
    Line {
        origin: Vec3,
        dir: Vec3,
    },
    /// An edge or round face of the model, found again (format 39).
    Model {
        geo: GeoRef,
    },
    /// A construction axis feature, by name (format 31).
    Construction {
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevolveConvention {
    #[default]
    LegacyAxisStart,
    SketchNormal,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Revolve {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    pub axis: AxisRef,
    pub angle: Expr,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub angle2: Option<Expr>,
    #[serde(default)]
    pub symmetric: bool,
    #[serde(default)]
    pub convention: RevolveConvention,
    #[serde(default)]
    pub to: Option<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilletStyle {
    #[default]
    Constant,
    Chord,
    Curvature,
    Variable {
        radius2: Expr,
        start: Vec3,
        #[serde(default)]
        mid: Vec<(f64, Expr)>,
        #[serde(default)]
        smooth: bool,
    },
    Spline {
        start: Vec3,
        knots: Vec<f64>,
        radii: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Fillet {
    pub edges: Vec<Vec3>,
    pub radius: Expr,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub style: FilletStyle,
    /// `exact`, `reduce` or `taper`.
    #[serde(default)]
    pub fit: Option<String>,
    /// `rolling` or `g2`.
    #[serde(default)]
    pub corner: Option<String>,
    /// Corners set back by a distance along their edges.
    #[serde(default)]
    pub setbacks: Vec<CornerSetback>,
    /// Each picked edge's tangent chain rounded with it (format 49: false
    /// rounds only the picked edges).
    #[serde(default = "yes")]
    pub tangent_chain: bool,
}

/// A fillet corner at `vertex` set back `distance` along its edges.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CornerSetback {
    pub vertex: Vec3,
    pub distance: Expr,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Chamfer {
    pub edges: Vec<Vec3>,
    pub distance: Expr,
    #[serde(default)]
    pub distance2: Option<Expr>,
    #[serde(default)]
    pub angle: Option<Expr>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub body: Option<String>,
    /// As a fillet's (format 49).
    #[serde(default = "yes")]
    pub tangent_chain: bool,
}

fn z_axis() -> Vec3 {
    [0.0, 0.0, 1.0]
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BoxPrimitive {
    pub corner: [Expr; 3],
    pub length: Expr,
    pub width: Expr,
    pub height: Expr,
    #[serde(default)]
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Cylinder {
    pub base: [Expr; 3],
    #[serde(default = "z_axis")]
    pub axis: Vec3,
    pub radius: Expr,
    pub height: Expr,
    #[serde(default)]
    pub operation: Operation,
    /// Placed by distances from up to two straight edges of the face it
    /// stands on (format 41); `base` is where it was placed.
    #[serde(default)]
    pub references: Vec<EdgeDistance>,
}

/// A cone or truncated cone (format 40): its base circle's centre and
/// radius, the radius `height` along `axis` (0: an apex).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Cone {
    pub base: [Expr; 3],
    #[serde(default = "z_axis")]
    pub axis: Vec3,
    pub radius: Expr,
    pub top_radius: Expr,
    pub height: Expr,
    #[serde(default)]
    pub operation: Operation,
}

/// A distance from a straight edge of a face, by a point on the edge: a
/// hole's or a cylinder's reference.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EdgeDistance {
    pub edge: Vec3,
    pub distance: Expr,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sphere {
    pub center: [Expr; 3],
    pub radius: Expr,
    #[serde(default)]
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Torus {
    pub center: [Expr; 3],
    pub major: Expr,
    pub minor: Expr,
    #[serde(default)]
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Combine {
    pub target: String,
    pub tools: Vec<String>,
    pub operation: Operation,
    #[serde(default)]
    pub keep_tools: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternKind {
    Rectangular {
        dir1: Vec3,
        count1: Expr,
        spacing1: Expr,
        #[serde(default)]
        dir2: Option<Vec3>,
        #[serde(default)]
        count2: Option<Expr>,
        #[serde(default)]
        spacing2: Option<Expr>,
        /// `right` (the default), `left` or `center`.
        #[serde(default)]
        alignment: Option<String>,
        #[serde(default)]
        alignment2: Option<String>,
        #[serde(default)]
        reversed: bool,
        #[serde(default)]
        reversed2: bool,
    },
    Circular {
        /// A point on the axis: numbers, or length expressions (format 48).
        origin: [Expr; 3],
        axis: Vec3,
        count: Expr,
        angle: Expr,
        #[serde(default)]
        alignment: Option<String>,
        #[serde(default)]
        reversed: bool,
        /// An edge or round face the copies turn about (format 39).
        #[serde(default)]
        axis_ref: Option<GeoRef>,
        /// A construction axis the copies turn about, by name (format 31).
        #[serde(default)]
        axis_feature: Option<String>,
    },
    Path {
        path_sketch: u64,
        path: Vec<String>,
        count: Expr,
        spacing: Expr,
        #[serde(default)]
        extent: bool,
        #[serde(default)]
        flip: bool,
        #[serde(default)]
        orient: bool,
    },
}

/// The copied features are feature ids (format 40; older designs' names
/// are ids after the upgrade, 0 for a name that found none).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pattern {
    pub features: Vec<u64>,
    pub pattern: PatternKind,
    #[serde(default)]
    pub bodies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Mirror {
    pub features: Vec<u64>,
    pub plane: PlaneRef,
    #[serde(default)]
    pub bodies: Vec<String>,
    #[serde(default)]
    pub combine: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoleKind {
    #[default]
    Simple,
    Drilled {
        tip_angle: Expr,
    },
    /// With a drill point below the bore when `tip_angle` is set (format
    /// 36), else flat.
    Counterbore {
        cb_diameter: Expr,
        cb_depth: Expr,
        #[serde(default)]
        tip_angle: Option<Expr>,
    },
    Countersink {
        cs_diameter: Expr,
        cs_angle: Expr,
        #[serde(default)]
        tip_angle: Option<Expr>,
    },
}

/// A thread's runout at one end (format 49): `standard` (2.5 pitches),
/// `short` (1.25) or a length.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadRunout {
    Standard,
    Short,
    Length { length: Expr },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SketchPoints {
    pub sketch: u64,
    pub ids: Vec<String>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub single: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hole {
    pub position: Vec3,
    #[serde(default)]
    pub at: Option<[Expr; 3]>,
    pub direction: Vec3,
    pub diameter: Expr,
    #[serde(default)]
    pub depth: Option<Expr>,
    #[serde(default)]
    pub hole: HoleKind,
    #[serde(default)]
    pub points: Option<SketchPoints>,
    #[serde(default)]
    pub thread: Option<String>,
    /// The tapped thread's depth from the wall's entry (format 36); none:
    /// the wall's useful length.
    #[serde(default)]
    pub thread_depth: Option<Expr>,
    /// `right` (none) or `left` (format 49).
    #[serde(default)]
    pub hand: Option<String>,
    /// The tapped thread cut into the wall (format 49).
    #[serde(default)]
    pub modeled: bool,
    /// A modelled tapped thread's runout towards the bottom (format 49).
    #[serde(default)]
    pub ease_out: Option<ThreadRunout>,
    #[serde(default)]
    pub to: Option<Vec3>,
    /// Placed by distances from up to two straight edges of its face
    /// (format 35); `position` is where it was placed.
    #[serde(default)]
    pub references: Vec<EdgeDistance>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Thread {
    pub face: Vec3,
    #[serde(default)]
    pub designation: Option<String>,
    #[serde(default)]
    pub length: Option<Expr>,
    #[serde(default)]
    pub modeled: bool,
    /// A point near the end it starts at (format 49).
    #[serde(default)]
    pub from: Option<Vec3>,
    /// Where it runs up to instead of a length: `{"face": …}` or
    /// `{"plane": …}`.
    #[serde(default)]
    pub up_to: Option<Value>,
    /// `metric` (none), `trapezoidal`, `acme`, `buttress`, `buttress_asme`,
    /// `square`, `bspp`, `rp`, `bspt` or `npt`.
    #[serde(default)]
    pub form: Option<String>,
    /// `right` (none), `left` or `both`.
    #[serde(default)]
    pub hand: Option<String>,
    #[serde(default = "one_start")]
    pub starts: u32,
    /// The runout at the end away from where it starts.
    #[serde(default)]
    pub ease_out: Option<ThreadRunout>,
    /// The chamfered free ends: `none` (none), `start` or `both`.
    #[serde(default)]
    pub tip: Option<String>,
    #[serde(default)]
    pub chamfer_angle: Option<Expr>,
    #[serde(default)]
    pub chamfer_depth: Option<Expr>,
    /// A thread without a designation sized by the tap drill its hole
    /// matches (designs before format 50), not by its diameter.
    #[serde(default)]
    pub old_auto_size: bool,
}

fn one_start() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LoftSection {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    #[serde(default)]
    pub point: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Loft {
    pub sections: Vec<LoftSection>,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
    /// Guide rails (format 28).
    #[serde(default)]
    pub rails: Vec<SweepRail>,
    #[serde(default)]
    pub rail_fit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SweepRail {
    pub sketch: u64,
    pub curves: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sweep {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    pub path_sketch: u64,
    pub path: Vec<String>,
    /// `perpendicular` (absent), `parallel` or `{"direction": [x, y, z]}`.
    #[serde(default)]
    pub orientation: Option<Value>,
    #[serde(default)]
    pub twist: Option<Expr>,
    #[serde(default)]
    pub scale: Option<Expr>,
    #[serde(default)]
    pub rails: Vec<SweepRail>,
    #[serde(default)]
    pub rail_fit: Option<String>,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Shell {
    pub faces: Vec<Vec3>,
    pub thickness: Expr,
    #[serde(default)]
    pub body: Option<String>,
    /// `inside` (empty), `outside` or `both`.
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub tangent_chain: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Draft {
    pub faces: Vec<Vec3>,
    pub angle: Expr,
    pub neutral: PlaneRef,
    pub pull: Vec3,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ConstructionPlane {
    pub plane: PlaneRef,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FaceAt {
    pub body: String,
    pub point: Vec3,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Split {
    pub body: String,
    pub plane: PlaneRef,
    #[serde(default)]
    pub tool: Option<FaceAt>,
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ConstructionAxis {
    pub def: AxisDef,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ConstructionPoint {
    pub def: PointDef,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SplitFace {
    pub faces: Vec<Vec3>,
    #[serde(default)]
    pub body: Option<String>,
    pub plane: PlaneRef,
    #[serde(default)]
    pub tool: Option<FaceAt>,
    #[serde(default)]
    pub sketch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Scale {
    pub bodies: Vec<String>,
    #[serde(default)]
    pub origin: Vec3,
    pub factor: Expr,
    #[serde(default)]
    pub factors: Option<[Expr; 3]>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OffsetFace {
    pub faces: Vec<Vec3>,
    pub distance: Expr,
    #[serde(default)]
    pub body: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DeleteFace {
    pub faces: Vec<Vec3>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "yes")]
    pub heal: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ReplaceFace {
    pub faces: Vec<Vec3>,
    pub target: PlaneRef,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Pipe {
    pub path_sketch: u64,
    pub path: Vec<String>,
    pub diameter: Expr,
    #[serde(default)]
    pub wall: Option<Expr>,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Coil {
    pub base: [Expr; 3],
    #[serde(default = "z_axis")]
    pub axis: Vec3,
    /// The sketch whose profile the coil sweeps (format 36), without
    /// `diameter` and `section_size`.
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub profiles: ProfileSel,
    #[serde(default)]
    pub diameter: Option<Expr>,
    pub pitch: Expr,
    pub turns: Expr,
    #[serde(default)]
    pub section_size: Option<Expr>,
    /// `circular` (default) or `square`.
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub position: i8,
    #[serde(default)]
    pub start_angle: Option<Expr>,
    #[serde(default)]
    pub clockwise: bool,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Rib {
    pub sketch: u64,
    pub curves: Vec<String>,
    pub thickness: Expr,
    #[serde(default)]
    pub depth: Option<Expr>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub normal: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Move {
    pub bodies: Vec<String>,
    pub translate: [Expr; 3],
    #[serde(default)]
    pub rotate_axis: Option<Vec3>,
    #[serde(default)]
    pub angle: Option<Expr>,
    #[serde(default)]
    pub copy: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Remove {
    pub bodies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Import {
    pub file: String,
    #[serde(default)]
    pub step: String,
    #[serde(default)]
    pub skip: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MeshData {
    pub name: String,
    pub positions: Vec<f64>,
    pub triangles: Vec<u32>,
    #[serde(default)]
    pub color: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MeshImport {
    pub file: String,
    pub meshes: Vec<MeshData>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BodyTransfer {
    pub bodies: Vec<String>,
    pub from: u64,
    pub to: u64,
    pub transform: Mat,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetBase {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    pub flip: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetContour {
    pub sketch: u64,
    pub curves: Vec<String>,
    pub distance: Expr,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub reverse: bool,
}

/// An edge flange's edge: a point, or a point with its own angle and
/// height.
#[derive(Debug, Clone, PartialEq)]
pub struct FlangeEdge {
    pub point: Vec3,
    pub angle: Option<Expr>,
    pub height: Option<Expr>,
}

impl<'de> Deserialize<'de> for FlangeEdge {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Point(Vec3),
            Own {
                point: Vec3,
                #[serde(default)]
                angle: Option<Expr>,
                #[serde(default)]
                height: Option<Expr>,
            },
        }
        Ok(match Form::deserialize(d)? {
            Form::Point(point) => FlangeEdge {
                point,
                angle: None,
                height: None,
            },
            Form::Own {
                point,
                angle,
                height,
            } => FlangeEdge {
                point,
                angle,
                height,
            },
        })
    }
}

fn inside() -> String {
    "inside".to_owned()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetFlange {
    pub edges: Vec<FlangeEdge>,
    pub height: Expr,
    pub angle: Expr,
    #[serde(default)]
    pub radius: Option<Expr>,
    /// `inside`, `outside` or `middle`; `material_outside` or `tangent`
    /// (format 37).
    #[serde(default = "inside")]
    pub position: String,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub corner_relief_shape: Option<String>,
    #[serde(default)]
    pub corner_relief: Option<Expr>,
    /// Where the heights are measured from (format 37): `inner` or
    /// `tangent`; none: the outer virtual sharp.
    #[serde(default)]
    pub height_from: Option<String>,
}

/// A path flange (format 36): edge flanges on the sheet edge at `edge`
/// along an open chain of sketch lines drawn across it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetPath {
    pub edge: Vec3,
    pub sketch: u64,
    pub curves: Vec<String>,
    #[serde(default)]
    pub radius: Option<Expr>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetHem {
    pub edges: Vec<Vec3>,
    pub length: Expr,
    #[serde(default)]
    pub gap: Option<Expr>,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetFold {
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub curve: Option<String>,
    pub a: Vec3,
    pub b: Vec3,
    pub angle: Expr,
    #[serde(default)]
    pub radius: Option<Expr>,
    pub position: String,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub fixed: Option<Vec3>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetUnfold {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub refold: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetConvert {
    pub body: String,
    pub face: Vec3,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    pub bends: Vec<Vec3>,
}

/// A plastic boss (mitcad#459).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Boss {
    pub position: Vec3,
    #[serde(default = "z_axis")]
    pub direction: Vec3,
    pub diameter: Expr,
    pub height: Expr,
    #[serde(default)]
    pub hole_diameter: Option<Expr>,
    #[serde(default)]
    pub hole_depth: Option<Expr>,
    #[serde(default)]
    pub draft: Option<Expr>,
    #[serde(default)]
    pub fillet: Option<Expr>,
    #[serde(default)]
    pub ribs: Option<Expr>,
    #[serde(default)]
    pub rib_thickness: Option<Expr>,
    #[serde(default)]
    pub rib_length: Option<Expr>,
    #[serde(default)]
    pub rib_offset: Option<Expr>,
    #[serde(default)]
    pub body: Option<String>,
}

/// A plastic lip, or a groove (mitcad#459).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Lip {
    pub face: Vec3,
    pub width: Expr,
    pub height: Expr,
    #[serde(default)]
    pub groove: bool,
    #[serde(default)]
    pub gap: Option<Expr>,
    #[serde(default)]
    pub outside: bool,
    #[serde(default)]
    pub body: Option<String>,
}

/// A plastic snap fit (mitcad#459).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapFit {
    pub position: Vec3,
    #[serde(default = "z_axis")]
    pub direction: Vec3,
    pub hook: Vec3,
    pub length: Expr,
    pub thickness: Expr,
    pub width: Expr,
    pub catch_depth: Expr,
    pub catch_length: Expr,
    /// The base reaching this far below the face; none: the arm starts at
    /// the face.
    #[serde(default)]
    pub base_height: Option<Expr>,
    /// A body of its own instead of joined to the target.
    #[serde(default)]
    pub new_body: bool,
    #[serde(default)]
    pub body: Option<String>,
}

/// A plastic rest (mitcad#459).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Rest {
    pub position: Vec3,
    #[serde(default = "z_axis")]
    pub direction: Vec3,
    #[serde(default)]
    pub along: Option<Vec3>,
    pub width: Expr,
    #[serde(default)]
    pub length: Option<Expr>,
    pub height: Expr,
    #[serde(default)]
    pub draft: Option<Expr>,
    #[serde(default)]
    pub thickness: Option<Expr>,
    #[serde(default)]
    pub body: Option<String>,
}

/// A perforation field (mitcad#461).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetPerforate {
    pub diameter: Expr,
    pub pitch: Expr,
    #[serde(default)]
    pub margin: Option<Expr>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// The field's outline (format 48): profiles of a sketch on a face of
    /// the sheet; none: the whole sheet.
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub profiles: Option<ProfileSel>,
}

/// Profiles raised (or sunk with `deboss`) by `depth` on a planar face,
/// or wrapped onto the cylindrical wall `face` picks (mitcad#458).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Emboss {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: ProfileSel,
    pub depth: Expr,
    #[serde(default)]
    pub deboss: bool,
    #[serde(default)]
    pub face: Option<EmbossFace>,
    #[serde(default)]
    pub targets: Vec<String>,
}

/// The cylindrical wall an emboss wraps onto: a point on it, in the
/// feature's component, and the occurrence paths that place the body and
/// the feature's component (empty: the component itself).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EmbossFace {
    pub body: String,
    pub at: Vec3,
    #[serde(default)]
    pub path: Vec<u64>,
    #[serde(default)]
    pub author_path: Vec<u64>,
}

/// A box around bodies grown by a margin, as a new body (mitcad#460); no
/// bodies: every body.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BoundingSolid {
    #[serde(default)]
    pub bodies: Vec<String>,
    /// "0" when the file has none.
    #[serde(default)]
    pub margin: Option<Expr>,
}

/// Boundary Fill (mitcad#457): the cells the tools enclose, each picked by
/// a point inside it, new bodies or joined, cut or intersected with the
/// feature's participants; `remove_tools` removes the tools first.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BoundaryFill {
    pub tools: Vec<String>,
    pub cells: Vec<Vec3>,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub remove_tools: bool,
}

/// A mesh body rebuilt as planes and cylinders within a tolerance
/// (mitcad#462).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MeshConvert {
    pub body: String,
    pub tolerance: Expr,
}

// Patch, Stitch and Thicken (mitcad#456).

/// How a patch meets the faces beside its loop: the whole loop's
/// continuity and single edges' (`g0` to `g3`), by points on them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PatchContinuity {
    #[serde(default = "g0")]
    pub all: String,
    #[serde(default)]
    pub edges: Vec<EdgeContinuity>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EdgeContinuity {
    pub edge: Vec3,
    pub g: String,
}

fn g0() -> String {
    "g0".to_owned()
}

impl Default for PatchContinuity {
    fn default() -> Self {
        Self {
            all: g0(),
            edges: Vec::new(),
        }
    }
}

/// A patch: regions of a sketch, a closed loop of a body's edges picked
/// by points on them, or both.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Patch {
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub profiles: ProfileSel,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub edges: Vec<Vec3>,
    /// How full a fitted patch's middle is (-0.9 to 3).
    #[serde(default)]
    pub crown: f64,
    #[serde(default)]
    pub continuity: PatchContinuity,
    /// Points a patch on edges passes through.
    #[serde(default)]
    pub points: Vec<Vec3>,
}

/// Bodies sewn into the first, edges within `tolerance` (0: SolidCraft's
/// own, from the bodies' size).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Stitch {
    pub bodies: Vec<String>,
    #[serde(default)]
    pub tolerance: Option<Expr>,
}

/// Surface bodies made solid by a thickness along their normals, half to
/// each side when symmetric.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Thicken {
    pub bodies: Vec<String>,
    pub thickness: Expr,
    #[serde(default)]
    pub symmetric: bool,
    #[serde(default)]
    pub operation: Operation,
    #[serde(default)]
    pub targets: Vec<String>,
}

// Surface modelling: surface bodies and their features (mitcad#455).

/// A surface extrude, revolve or sweep: the walls the boundaries of
/// `profiles` (none: only `curves`) and the chains of `curves` sweep.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceExtrude {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: Option<ProfileSel>,
    #[serde(default)]
    pub curves: Vec<String>,
    pub extent: Extent,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceRevolve {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: Option<ProfileSel>,
    #[serde(default)]
    pub curves: Vec<String>,
    pub axis: AxisRef,
    pub angle: Expr,
    #[serde(default)]
    pub angle2: Option<Expr>,
    #[serde(default)]
    pub symmetric: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceSweep {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: Option<ProfileSel>,
    #[serde(default)]
    pub curves: Vec<String>,
    pub path_sketch: u64,
    pub path: Vec<String>,
    #[serde(default)]
    pub orientation: Option<Value>,
    #[serde(default)]
    pub twist: Option<Expr>,
    #[serde(default)]
    pub scale: Option<Expr>,
    #[serde(default)]
    pub rails: Vec<SweepRail>,
    #[serde(default)]
    pub rail_fit: Option<String>,
}

/// A section of a surface loft: one chain of a sketch (a profile's
/// boundary or curves end to end), or an edge chain of a body.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SheetSection {
    pub sketch: u64,
    #[serde(default)]
    pub profiles: Option<ProfileSel>,
    #[serde(default)]
    pub curves: Vec<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub edges: Vec<Vec3>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceLoft {
    pub sections: Vec<SheetSection>,
    /// At the first and the last section (`g0` to `g2`).
    #[serde(default)]
    pub continuity: Option<[String; 2]>,
    #[serde(default)]
    pub crown: Option<[f64; 2]>,
    /// Guide rails (format 28).
    #[serde(default)]
    pub rails: Vec<SweepRail>,
    #[serde(default)]
    pub rail_fit: Option<String>,
}

/// Faces picked by points copied out along their normals.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceOffset {
    #[serde(default)]
    pub faces: Vec<Vec3>,
    #[serde(default)]
    pub body: Option<String>,
    pub distance: Expr,
    #[serde(default)]
    pub chain: bool,
    #[serde(default)]
    pub tolerance: Option<Expr>,
}

/// A ruled strip off a chain of edges picked by points.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceRuled {
    pub body: String,
    pub edges: Vec<Vec3>,
    pub distance: Expr,
    /// `normal`, `tangent`, `angle` or `direction`.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub angle: Option<Expr>,
    #[serde(default)]
    pub direction: Option<Vec3>,
    #[serde(default)]
    pub flip: bool,
    /// `mitre` or `round`.
    #[serde(default)]
    pub corners: Option<String>,
    #[serde(default)]
    pub face: Option<Vec3>,
}

/// A surface body extended past edges picked by points.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceExtend {
    pub body: String,
    pub edges: Vec<Vec3>,
    pub distance: Expr,
    /// `natural`, `tangent` or `curvature`.
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub up_to: Option<Value>,
    #[serde(default = "yes")]
    pub neighbours: bool,
}

/// Sketch curves swept onto a surface to trim it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TrimCurves {
    pub sketch: u64,
    #[serde(default)]
    pub curves: Vec<String>,
    #[serde(default)]
    pub direction: Option<Vec3>,
}

/// A surface body trimmed by a plane, a face or sketch curves.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceTrim {
    pub body: String,
    pub plane: PlaneRef,
    #[serde(default)]
    pub tool: Option<FaceAt>,
    pub keep: Vec3,
    #[serde(default)]
    pub keep_more: Vec<Vec3>,
    #[serde(default)]
    pub curves: Option<TrimCurves>,
    #[serde(default)]
    pub with: Option<String>,
}

/// A face restored towards its surface.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceUntrim {
    pub face: FaceAt,
    #[serde(default)]
    pub holes: Option<Vec<Vec3>>,
    #[serde(default)]
    pub outer: bool,
    #[serde(default)]
    pub in_place: bool,
}

/// Faces of a body picked by points (every face when none): an unstitch's
/// or a reverse normal's.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BodyFaces {
    pub body: String,
    #[serde(default)]
    pub faces: Vec<Vec3>,
}

/// One end of a blend: a body and points on the edges of its chain.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BlendSide {
    pub body: String,
    #[serde(default)]
    pub edges: Vec<Vec3>,
}

fn g1_pair() -> [String; 2] {
    ["g1".to_owned(), "g1".to_owned()]
}

/// A blend surface bridging two chains of edges (`g0` to `g3` per end).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceBlend {
    pub from: BlendSide,
    pub to: BlendSide,
    #[serde(default = "g1_pair")]
    pub continuity: [String; 2],
    #[serde(default)]
    pub bulge: [f64; 2],
    #[serde(default)]
    pub align: Option<[Vec3; 2]>,
    /// Where each chain's stations start (format 28): a point on each.
    #[serde(default)]
    pub seam: Option<[Vec3; 2]>,
    #[serde(default)]
    pub stitch: bool,
}

/// A boundary patch through two to four curves of a sketch in loop order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceBoundary {
    pub sketch: u64,
    pub curves: Vec<String>,
    #[serde(default)]
    pub orders: Vec<String>,
    #[serde(default)]
    pub crown: f64,
}

/// A curve of a network: sketch curves, a body's edges at points, or a
/// point; `continuity` with the face beside a chain of open edges.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NetworkCurve {
    #[serde(default)]
    pub sketch: Option<u64>,
    #[serde(default)]
    pub curves: Vec<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub edges: Vec<Vec3>,
    #[serde(default)]
    pub point: Option<Vec3>,
    #[serde(default = "g0")]
    pub continuity: String,
}

/// A surface through a network of curves.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceNetwork {
    pub u: Vec<NetworkCurve>,
    #[serde(default)]
    pub v: Vec<NetworkCurve>,
    #[serde(default)]
    pub operation: Operation,
}

fn g2() -> String {
    "g2".to_owned()
}

/// A face of a surface body rebuilt against a chain of another body's
/// edges.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceMatch {
    pub body: String,
    #[serde(default)]
    pub faces: Vec<Vec3>,
    pub target: String,
    pub edges: Vec<Vec3>,
    #[serde(default = "g2")]
    pub order: String,
}

fn one() -> f64 {
    1.0
}

fn one_step() -> u32 {
    1
}

/// B-spline faces of a body faired.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceFair {
    pub body: String,
    #[serde(default)]
    pub faces: Vec<Vec3>,
    #[serde(default = "one")]
    pub strength: f64,
    #[serde(default = "one_step")]
    pub iterations: u32,
    #[serde(default = "yes")]
    pub keep_edges: bool,
    pub tolerance: Expr,
}
