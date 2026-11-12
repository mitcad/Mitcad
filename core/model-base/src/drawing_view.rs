// SPDX-License-Identifier: MIT
//! What the geometry kernel works out for drawings (mitcad#471): the
//! projected edges of a view and the round holes of a body (mitcad#777:
//! below the model, next to the [`Kernel`] trait).

use mitcad_vec3::{cross, dot};

use crate::datum::Vec3;
#[cfg(doc)]
use crate::kernel::Kernel;
use crate::topo::FaceName;

/// A point in a view's or on the sheet's plane.
pub type P2 = [f64; 2];

/// A plane cutting a section view: what lies on its normal side goes (as
/// Section Analysis cuts), and the view looks at the cut along -normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutPlane {
    pub origin: Vec3,
    /// A unit vector.
    pub normal: Vec3,
}

/// Input of [`crate::Kernel::project_view`]: an orthographic view from far
/// away along `-toward_eye` (unit vectors at right angles). The view's 2D
/// coordinates are those of the plane through the model origin at right
/// angles to `toward_eye`: x along `x_axis`, y along `toward_eye × x_axis`.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewSpec {
    pub toward_eye: Vec3,
    pub x_axis: Vec3,
    /// Also the hidden edges.
    pub hidden: bool,
    /// Also the edges between faces that meet tangentially.
    pub smooth: bool,
    /// Exact edges first; false: a triangulation's only.
    pub exact: bool,
    /// With `exact`: the quick path where the exact edges are slow, on
    /// bodies with free-form faces (mitcad#962) or with many B-spline
    /// edges against many faces (mitcad#1033): a fine triangulation's
    /// edges, drawn as lines and arcs where they follow them (the
    /// projection is then `polygonal`). Drawings' views take it; cut sheets
    /// do not, as they write the curves as they are.
    pub quick: bool,
    /// A section view: `toward_eye` is the plane's normal.
    pub section: Option<CutPlane>,
    /// The largest distance of a polyline from the curve it stands for, mm.
    pub deflection: f64,
    /// The view's scale on its sheet (sheet millimetres per model
    /// millimetre), when it is on one: the quick path's lines then lie
    /// within [`SHEET_DEVIATION`] of the edges on the sheet
    /// ([`Self::quick_deviation`], mitcad#1671). Without it they follow the
    /// size of the bodies.
    pub scale: Option<f64>,
}

/// How far a line the quick path draws may lie from the edge it stands for
/// on the sheet, mm: under a tenth of a thin line (0.18 to 0.25 mm), so
/// that a view's straight edges stay straight and its rounded corners
/// round however large the assembly (mitcad#1671).
pub const SHEET_DEVIATION: f64 = 0.02;

impl ViewSpec {
    /// How far the quick path's lines may lie from the edges, model mm:
    /// [`SHEET_DEVIATION`] at the view's scale, 0.1 mm at 1:5; at least the
    /// deflection. None without a scale: the kernel then lets them deviate
    /// by a 2000th of the bodies' size. The kernel never draws them coarser
    /// than that either.
    pub fn quick_deviation(&self) -> Option<f64> {
        self.scale
            .filter(|s| s.is_finite() && *s > 0.0)
            .map(|s| (SHEET_DEVIATION / s).max(self.deflection))
    }

    /// The view's y axis.
    pub fn y_axis(&self) -> Vec3 {
        cross(self.toward_eye, self.x_axis)
    }

    /// The view coordinates of a model point.
    pub fn project(&self, p: Vec3) -> P2 {
        [dot(p, self.x_axis), dot(p, self.y_axis())]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEdgeKind {
    /// An edge where faces meet at an angle, or a boundary.
    Sharp,
    /// An edge between faces that meet tangentially.
    Smooth,
    /// A silhouette of a curved face.
    Outline,
    /// A thread's root line drawn by convention (ISO 6410, mitcad#1112):
    /// continuous narrow. Never from the kernel: the drawing adds it in
    /// place of a thread's crests and helices
    /// (`mitcad_model_drawing::convention`).
    ThreadRoot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewCurve {
    Line(P2, P2),
    /// Counter-clockwise from `start` to `end` (radians; a whole circle from
    /// 0 to 2 pi).
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Polyline(Vec<P2>),
    /// A B-spline (mitcad#853): a projected B-spline or Bézier curve as
    /// itself, another curve (an ellipse, ...) within 1e-6 mm. Where the
    /// exact curve is not needed it is drawn, cut and picked as its
    /// polyline `points`.
    Spline(ViewSpline),
}

/// A B-spline in a view's plane, from its start to its end: `degree`, its
/// full knot vector (`poles.len() + degree + 1` knots), its poles and
/// their weights (empty when it is not rational), and a polyline of it
/// within the view's deflection (`points`, its ends the curve's).
#[derive(Debug, Clone, PartialEq)]
pub struct ViewSpline {
    pub degree: u32,
    pub knots: Vec<f64>,
    pub poles: Vec<P2>,
    pub weights: Vec<f64>,
    pub points: Vec<P2>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewEdge {
    pub visible: bool,
    pub kind: ViewEdgeKind,
    pub curve: ViewCurve,
    /// The shape whose edge it is: its index among those
    /// [`crate::Kernel::project_view`] was given (mitcad#1112: a thread's
    /// convention replaces only its own body's lines). None for a line the
    /// drawing adds, or when not known. The kernel records the shape, not
    /// the face: its hidden-line removal makes new edges.
    pub source: Option<usize>,
}

/// A face a section's plane cuts: its closed loops (the last point not
/// repeated) in view coordinates.
pub type CutFace = Vec<Vec<P2>>;

/// What [`crate::Kernel::project_view`] gives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewProjection {
    pub edges: Vec<ViewEdge>,
    pub cuts: Vec<CutFace>,
    /// Made from a triangulation (asked for, or the exact edges failed).
    pub polygonal: bool,
}

/// A round hole of a body ([`crate::Kernel::round_holes`]): coaxial
/// cylinders with the material all round outside them, the narrowest the
/// drilled wall, a wider one or a cone that meets it on the side it opens
/// to its counterbore or countersink.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundHole {
    /// The drilled wall's face.
    pub face: Option<FaceName>,
    /// On the axis where the hole opens; a hole open at both ends opens
    /// towards +Z, else +Y, else +X, or to its counterbore.
    pub opening: Vec3,
    /// The axis into the material (a unit vector).
    pub direction: Vec3,
    pub diameter: f64,
    pub through: bool,
    /// From the opening to the end of the drilled wall.
    pub depth: f64,
    /// Diameter and depth.
    pub counterbore: Option<(f64, f64)>,
    /// Diameter and the full included angle (radians).
    pub countersink: Option<(f64, f64)>,
}
