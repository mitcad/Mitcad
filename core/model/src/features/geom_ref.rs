// SPDX-License-Identifier: MIT
//! References to geometry: the one form in which features take planes,
//! axes, points, faces, bodies, sketch curves and paths ([`GeomRef`],
//! [`PathRef`]), and how they resolve.
//!
//! | JSON | Meaning |
//! |---|---|
//! | `"xy"`, `"xz"`, `"yz"`, `"x"`, `"y"`, `"z"`, `"origin"` | origin datums ([`OriginDatum`]) |
//! | `"F5"` | the datum of a construction feature |
//! | `{"body": "F2.b0", "face": "F2:end(r{c5})"}` | a face of a body (also `"edge"`, `"vertex"`) |
//! | `{"body": "F2.b0"}` | a body |
//! | `{"sketch": "F1", "curve": "c4"}` | a sketch curve; `"curves": [...]` several, neither all of them |
//! | `{"sketch": "F1", "point": "p3"}` | a sketch point |
//! | `{"origin": [x, y, z], "normal": [...], "x_axis": [...]}` | a fixed plane (`x_axis` optional) |
//! | `{"origin": [x, y, z], "direction": [...]}` | a fixed axis |
//! | `{"point": [x, y, z]}` | a fixed point |
//! | `{"mount": "F7:mount(slot_x+_1)"}` | a derived part's mount frame (mitcad#375), or a pulley's `F3:mount(bore)` (mitcad#562): a plane, an axis (its z) or a point (its origin) |
//!
//! A path is `{"body": "F2.b0", "edges": [...]}` (connected edges in
//! order), `{"sketch": "F1", "curve": "c4"}` (a line, arc or circle in its
//! own direction) or `{"start": [x, y, z], "end": [...]}`.
//!
//! Features resolve references through [`EvalContext::plane`],
//! [`EvalContext::axis`], [`EvalContext::point`] and the other methods
//! below, and check them with [`GeomRef::check`] and [`GeomRef::add_to`].
//! The conventions, one rule each:
//!
//! - planes: an origin plane has its [`OriginDatum`] frame; a planar face
//!   (outward normal) and a fixed plane without `x_axis` have the frame of
//!   [`DatumPlane::on_plane`]: the model origin projected onto the plane,
//!   x along +X projected (+Y when the plane faces ±X). Sketches on faces,
//!   mirror planes, primitives and extents to faces all use it;
//! - axes: a straight edge or a sketch line runs from its start to its
//!   end; a circular edge and a cylindrical, conical or toroidal face give
//!   their axis pointing along its largest component
//!   ([`canonical_axis`]), through the circle's centre or the axis point
//!   nearest to the face's centre;
//! - points: a vertex, a circular edge's centre, a straight edge's middle.
//!
//! Earlier files and commands wrote references with a `"type"` tag (F2:
//! `origin_plane`, `plane`, `face`, `body`, `sketch`; F4: `origin`, `face`,
//! `edge`, `vertex`, `construction`, `fixed`, and the paths `sketch_curve`
//! and `line`). They are read as the forms above, which are written back.

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::construction::datum_kind;
use super::sketch::SketchOutput;
use super::{CheckContext, EvalContext, References};
use crate::datum::{
    CurveGeometry, Datum, DatumAxis, DatumKind, DatumPlane, DatumPoint, OriginDatum, PathParameter,
    PathPoint, SurfaceGeometry, SurfacePoint, Vec3,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::profile::SegmentGeometry;
use crate::recompute::Read;
use crate::sketch::geometry::Curve2;
use crate::topo::{EdgeName, FaceName, VertexName};
use crate::transform::Transform;
use mitcad_model_base::internal::datum::{LINEAR, canonical_axis};
use mitcad_vec3::dot;
use mitcad_vec3::{add, norm, scale, sub, unit};

/// A geometric input of a feature (see the module documentation).
#[derive(Debug, Clone, PartialEq)]
pub enum GeomRef {
    Origin(OriginDatum),
    /// The datum of a construction feature.
    Datum(FeatureUid),
    Face {
        body: BodyUid,
        face: FaceName,
    },
    Edge {
        body: BodyUid,
        edge: EdgeName,
    },
    Vertex {
        body: BodyUid,
        vertex: VertexName,
    },
    Body(BodyUid),
    /// Curves of a sketch; all of them when the list is empty.
    SketchCurves {
        sketch: FeatureUid,
        curves: Vec<EntityUid>,
    },
    SketchPoint {
        sketch: FeatureUid,
        point: EntityUid,
    },
    /// A plane through `origin` in model millimetres; `x_axis`, projected
    /// into the plane, orients it.
    FixedPlane {
        origin: Vec3,
        normal: Vec3,
        x_axis: Option<Vec3>,
    },
    FixedAxis {
        origin: Vec3,
        direction: Vec3,
    },
    FixedPoint(Vec3),
    /// A mount frame of a derived part (mitcad#375): its `mount:<name>`
    /// joint origin of the source, `F7:mount(slot_x+_1)`.
    Mount {
        feature: FeatureUid,
        name: String,
    },
}

/// A path: connected edges of one body in order (the path runs from the
/// free end of the first edge), a sketch line, arc or circle in its own
/// direction (a circle starts on its sketch x axis and runs
/// counter-clockwise), several lines and arcs of one sketch joined end to
/// end (the path runs from the free end of the first listed curve, as
/// the paths of sweeps do: `path.rs`), a fixed straight line, a fixed
/// helix, fixed curves (mitcad#548), or the curve of an Intersection Curve
/// (mitcad#1405).
#[derive(Debug, Clone, PartialEq)]
pub enum PathRef {
    Edges {
        body: BodyUid,
        edges: Vec<EdgeName>,
    },
    SketchCurve {
        sketch: FeatureUid,
        curve: EntityUid,
    },
    Line {
        start: Vec3,
        end: Vec3,
    },
    /// Two or more curves of a sketch, joined end to end.
    SketchCurves {
        sketch: FeatureUid,
        curves: Vec<EntityUid>,
    },
    /// A fixed helix (mitcad#385).
    Helix(HelixPath),
    /// Fixed curves in model space joined end to end (mitcad#548): lines,
    /// circle and ellipse arcs, B-splines (an imported object's edges that
    /// no sketch holds, a conical helix as its file stores it).
    Fixed(Vec<FixedCurve>),
    /// The curve of an Intersection Curve (mitcad#1405): `{"feature": "F5"}`,
    /// its pieces in order along it.
    FeatureCurve(FeatureUid),
}

/// A curve of a fixed path (mitcad#548), as [`crate::kernel::Curve3`]
/// holds it: `{"type": "line", "start", "end"}`, `{"type": "conic",
/// "center", "normal", "x_axis", "major", "minor", "start", "end",
/// "closed"}` (a circle when `major == minor`) or `{"type": "spline",
/// "degree", "poles", "weights" (empty: not rational), "knots"}` (the full
/// knot vector, `poles + degree + 1` knots).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FixedCurve {
    Line {
        start: Vec3,
        end: Vec3,
    },
    Conic {
        center: Vec3,
        normal: Vec3,
        x_axis: Vec3,
        major: f64,
        minor: f64,
        start: f64,
        end: f64,
        #[serde(default)]
        closed: bool,
    },
    Spline {
        degree: u32,
        poles: Vec<Vec3>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f64>,
        knots: Vec<f64>,
    },
}

impl FixedCurve {
    /// The fixed counterpart of a kernel curve: None for a point or a
    /// helix (a helix has its own path, [`HelixPath`]).
    pub fn of(curve: &crate::kernel::Curve3) -> Option<Self> {
        use crate::kernel::Curve3;
        Some(match curve {
            Curve3::Line { start, end } => Self::Line {
                start: *start,
                end: *end,
            },
            Curve3::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => Self::Conic {
                center: *center,
                normal: *normal,
                x_axis: *x_axis,
                major: *major,
                minor: *minor,
                start: *start,
                end: *end,
                closed: *closed,
            },
            Curve3::BSpline {
                degree,
                poles,
                weights,
                knots,
            } => Self::Spline {
                degree: *degree,
                poles: poles.clone(),
                weights: weights.clone(),
                knots: knots.clone(),
            },
            Curve3::Point(_) | Curve3::Helix { .. } => return None,
        })
    }

    /// The kernel curve.
    pub fn curve(&self) -> crate::kernel::Curve3 {
        use crate::kernel::Curve3;
        match self {
            Self::Line { start, end } => Curve3::Line {
                start: *start,
                end: *end,
            },
            Self::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => Curve3::Conic {
                center: *center,
                normal: *normal,
                x_axis: *x_axis,
                major: *major,
                minor: *minor,
                start: *start,
                end: *end,
                closed: *closed,
            },
            Self::Spline {
                degree,
                poles,
                weights,
                knots,
            } => Curve3::BSpline {
                degree: *degree,
                poles: poles.clone(),
                weights: weights.clone(),
                knots: knots.clone(),
            },
        }
    }

    /// The curve with its points and directions mapped (a turn).
    pub fn mapped(&self, f: &dyn Fn(Vec3) -> Vec3) -> Self {
        match self {
            Self::Line { start, end } => Self::Line {
                start: f(*start),
                end: f(*end),
            },
            Self::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => Self::Conic {
                center: f(*center),
                normal: f(*normal),
                x_axis: f(*x_axis),
                major: *major,
                minor: *minor,
                start: *start,
                end: *end,
                closed: *closed,
            },
            Self::Spline {
                degree,
                poles,
                weights,
                knots,
            } => Self::Spline {
                degree: *degree,
                poles: poles.iter().map(|p| f(*p)).collect(),
                weights: weights.clone(),
                knots: knots.clone(),
            },
        }
    }

    pub fn check(&self) -> Result<(), String> {
        match self {
            Self::Line { start, end } => {
                finite("fixed line", &[*start, *end].concat())?;
                if norm(sub(*end, *start)) <= LINEAR {
                    return Err("a fixed line has no length".to_owned());
                }
            }
            Self::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => {
                finite(
                    "fixed conic",
                    &[
                        &center[..],
                        &normal[..],
                        &x_axis[..],
                        &[*major, *minor, *start, *end],
                    ]
                    .concat(),
                )?;
                let n = unit(*normal).ok_or("a fixed conic's normal has no length")?;
                if unit(sub(*x_axis, scale(n, dot(*x_axis, n)))).is_none() {
                    return Err("a fixed conic's x axis runs along its normal".to_owned());
                }
                if *major <= LINEAR || *minor <= LINEAR {
                    return Err("a fixed conic's radii must be positive".to_owned());
                }
                if !closed && end <= start {
                    return Err("a fixed conic arc must end after it starts".to_owned());
                }
            }
            Self::Spline {
                degree,
                poles,
                weights,
                knots,
            } => {
                let flat: Vec<f64> = poles.iter().flatten().copied().collect();
                finite(
                    "fixed spline",
                    &[&flat[..], &weights[..], &knots[..]].concat(),
                )?;
                let degree = *degree as usize;
                if degree == 0 || poles.len() < degree + 1 {
                    return Err(format!(
                        "a fixed spline of degree {degree} needs at least {} poles",
                        degree + 1
                    ));
                }
                if knots.len() != poles.len() + degree + 1 {
                    return Err(format!(
                        "a fixed spline of {} poles and degree {degree} needs {} knots, got {}",
                        poles.len(),
                        poles.len() + degree + 1,
                        knots.len()
                    ));
                }
                if knots.windows(2).any(|k| k[1] < k[0]) {
                    return Err("a fixed spline's knots must not decrease".to_owned());
                }
                if !weights.is_empty()
                    && (weights.len() != poles.len() || weights.iter().any(|w| *w <= 0.0))
                {
                    return Err(
                        "a fixed spline's weights are one positive weight per pole".to_owned()
                    );
                }
            }
        }
        Ok(())
    }
}

/// A fixed helical path (mitcad#385): the points `center + radius (cos t
/// x + sin t (axis × x)) + pitch t / 2π axis` (`x` is `x_axis` made square
/// to the axis) for t from `start_angle` to `end_angle` (radians, either
/// way round: the path runs from the first to the second). `pitch` is the
/// rise along the axis per turn counter-clockwise about it, negative for a
/// left-handed helix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelixPath {
    pub center: Vec3,
    pub axis: Vec3,
    pub x_axis: Vec3,
    pub radius: f64,
    pub pitch: f64,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl HelixPath {
    pub fn check(&self) -> Result<(), String> {
        finite(
            "helix",
            &[
                &self.center[..],
                &self.axis[..],
                &self.x_axis[..],
                &[self.radius, self.pitch, self.start_angle, self.end_angle],
            ]
            .concat(),
        )?;
        self.curve().map(|_| ())
    }

    /// The helix as a kernel curve running along the path (its parameter
    /// increasing).
    pub fn curve(&self) -> Result<crate::kernel::Curve3, String> {
        let axis = unit(self.axis).ok_or("the helix's axis has no length")?;
        let x = unit(sub(self.x_axis, scale(axis, dot(self.x_axis, axis))))
            .ok_or("the helix's x axis runs along its axis")?;
        if self.radius <= LINEAR {
            return Err("the helix's radius must be positive".to_owned());
        }
        if self.pitch.abs() <= LINEAR {
            return Err("the helix's pitch must not be zero".to_owned());
        }
        let turn = self.end_angle - self.start_angle;
        let speed = self.radius.hypot(self.pitch / std::f64::consts::TAU);
        if turn.abs() * speed <= LINEAR {
            return Err("the helix path has no length".to_owned());
        }
        let helix = crate::kernel::Curve3::Helix {
            center: self.center,
            axis,
            x_axis: x,
            radius: self.radius,
            pitch: self.pitch,
            start: self.start_angle.min(self.end_angle),
            end: self.start_angle.max(self.end_angle),
        };
        Ok(if turn > 0.0 {
            helix
        } else {
            super::path::reversed(helix)
        })
    }
}

/// What a reference must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    /// An origin plane, a construction plane, a (planar) face or a fixed
    /// plane.
    Plane,
    /// An origin axis, a construction axis, a (straight) edge, a sketch
    /// line or a fixed axis.
    Line,
    /// A line, or an axis of a circular edge, a circle or an arc of a
    /// sketch, or a face of revolution.
    Axis,
    /// The origin point, a construction point, a vertex, the centre or
    /// middle of an edge, a sketch point or a fixed point.
    Point,
    /// A face of a body.
    Face,
    /// An edge or a face of a body (a circle's or a sphere's centre).
    EdgeOrFace,
    /// What a face feature works against: a plane, a face, a body or
    /// sketch curves.
    Tool,
    /// A surface to replace faces with: a plane, a face of any surface
    /// type or a body (its faces).
    Surface,
    /// A face, or a plane (an origin plane, a construction plane or a fixed
    /// plane: a planar face is its whole plane): what an axis normal to a
    /// face at a point is measured from.
    FaceOrPlane,
}

impl Want {
    fn describe(self) -> &'static str {
        match self {
            Self::Plane => "a plane or a planar face",
            Self::Line => "an axis or a straight edge",
            Self::Axis => "an axis, an edge or a face with an axis",
            Self::Point => "a point or a vertex",
            Self::Face => "a face",
            Self::EdgeOrFace => "an edge or a face",
            Self::Tool => "a plane, a face, a body or sketch curves",
            Self::Surface => "a plane, a face or a body",
            Self::FaceOrPlane => "a face or a plane",
        }
    }

    fn datum(self) -> Option<DatumKind> {
        match self {
            Self::Plane | Self::Tool | Self::Surface | Self::FaceOrPlane => Some(DatumKind::Plane),
            Self::Line | Self::Axis => Some(DatumKind::Axis),
            Self::Point => Some(DatumKind::Point),
            Self::Face | Self::EdgeOrFace => None,
        }
    }

    /// Whether a reference of this form can be what is wanted (origin and
    /// construction datums are checked by their kind).
    fn admits(self, reference: &GeomRef) -> bool {
        use GeomRef as G;
        match reference {
            G::Origin(_) => self.datum().is_some(),
            G::Datum(_) => true,
            G::Face { .. } => matches!(
                self,
                Self::Plane
                    | Self::Axis
                    | Self::Face
                    | Self::EdgeOrFace
                    | Self::Tool
                    | Self::Surface
                    | Self::FaceOrPlane
            ),
            G::Edge { .. } => matches!(
                self,
                Self::Line | Self::Axis | Self::Point | Self::EdgeOrFace
            ),
            G::Vertex { .. } | G::SketchPoint { .. } | G::FixedPoint(_) => self == Self::Point,
            G::Body(_) => matches!(self, Self::Tool | Self::Surface),
            G::SketchCurves { .. } => matches!(self, Self::Line | Self::Axis | Self::Tool),
            G::FixedPlane { .. } => matches!(
                self,
                Self::Plane | Self::Tool | Self::Surface | Self::FaceOrPlane
            ),
            G::FixedAxis { .. } => matches!(self, Self::Line | Self::Axis),
            G::Mount { .. } => matches!(
                self,
                Self::Plane | Self::Line | Self::Axis | Self::Point | Self::FaceOrPlane
            ),
        }
    }
}

fn article(kind: DatumKind) -> &'static str {
    match kind {
        DatumKind::Axis => "an",
        DatumKind::Plane | DatumKind::Point => "a",
    }
}

fn point_name(point: EntityUid) -> String {
    format!("p{}", point.0)
}

/// Whether `name` can name a mount frame: ASCII letters, digits and `_`,
/// `+`, `-` and `.` (`slot_x+_1`).
pub(crate) fn valid_mount_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_+-.".contains(c))
}

/// Reads a mount frame's reference, `F7:mount(slot_x+_1)`.
pub(crate) fn parse_mount(text: &str) -> Result<(FeatureUid, String), String> {
    let bad = || {
        format!(
            "invalid mount frame “{text}”: expected a derived part's frame like \"F7:mount(end_a)\""
        )
    };
    let (feature, rest) = text.trim().split_once(':').ok_or_else(bad)?;
    let name = rest
        .strip_prefix("mount(")
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(bad)?;
    let feature = feature.parse::<FeatureUid>().map_err(|_| bad())?;
    if !valid_mount_name(name) {
        return Err(bad());
    }
    Ok((feature, name.to_owned()))
}

impl fmt::Display for GeomRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Origin(origin) => write!(f, "the origin {}", origin.display_name()),
            Self::Datum(uid) => write!(f, "{uid}"),
            Self::Face { face, .. } => write!(f, "face {face}"),
            Self::Edge { edge, .. } => write!(f, "edge {edge}"),
            Self::Vertex { vertex, .. } => write!(f, "vertex {vertex}"),
            Self::Body(body) => write!(f, "body {body}"),
            Self::SketchCurves { sketch, curves } => match curves.as_slice() {
                [] => write!(f, "the curves of {sketch}"),
                [curve] => write!(f, "curve {} of {sketch}", curve.curve_name()),
                _ => {
                    let names: Vec<String> = curves.iter().map(|c| c.curve_name()).collect();
                    write!(f, "curves {} of {sketch}", names.join(", "))
                }
            },
            Self::SketchPoint { sketch, point } => {
                write!(f, "point {} of {sketch}", point_name(*point))
            }
            Self::FixedPlane { .. } => f.write_str("the fixed plane"),
            Self::FixedAxis { .. } => f.write_str("the fixed axis"),
            Self::FixedPoint(_) => f.write_str("the fixed point"),
            Self::Mount { feature, name } => write!(f, "frame {feature}:mount({name})"),
        }
    }
}

fn finite(what: &str, values: &[f64]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(format!("the {what} must be finite"))
    }
}

impl GeomRef {
    /// Adds the features it refers to.
    pub fn add_to(&self, references: &mut References) {
        match self {
            Self::Origin(_) | Self::FixedPlane { .. } | Self::FixedAxis { .. } => {}
            Self::FixedPoint(_) => {}
            Self::Datum(uid) => {
                references.features.insert(*uid);
            }
            Self::Mount { feature, .. } => {
                references.features.insert(*feature);
            }
            Self::Face { body, face } => {
                references.body(*body);
                references.features.extend(face.features());
            }
            Self::Edge { body, edge } => {
                references.body(*body);
                references.edges([edge]);
            }
            Self::Vertex { body, vertex } => {
                references.body(*body);
                for face in vertex.faces() {
                    references.features.extend(face.features());
                }
            }
            Self::Body(body) => references.body(*body),
            Self::SketchCurves { sketch, .. } | Self::SketchPoint { sketch, .. } => {
                references.features.insert(*sketch);
            }
        }
    }

    /// Checks that it refers to something of the wanted kind before the
    /// feature (only evaluation can tell whether a face is planar or an
    /// edge straight).
    pub fn check(&self, ctx: &CheckContext<'_>, want: Want) -> Result<(), String> {
        let wrong = || format!("{self} is not {}", want.describe());
        if !want.admits(self) {
            return Err(wrong());
        }
        let features = match self {
            Self::Origin(origin) => {
                return if Some(origin.kind()) == want.datum() {
                    Ok(())
                } else {
                    Err(wrong())
                };
            }
            Self::Datum(uid) => {
                let entry = ctx.feature(*uid)?;
                return match (datum_kind(&entry.def), want.datum()) {
                    (Some(kind), Some(wanted)) if kind == wanted => Ok(()),
                    (Some(kind), _) => Err(format!(
                        "{} ({uid}) is a construction {kind}, not {}",
                        entry.name,
                        want.describe()
                    )),
                    (None, _) => Err(format!(
                        "{} ({uid}) is not construction geometry",
                        entry.name
                    )),
                };
            }
            Self::Face { body, face } => {
                ctx.body(*body)?;
                face.features()
            }
            Self::Edge { body, edge } => {
                ctx.body(*body)?;
                edge.features()
            }
            Self::Vertex { body, vertex } => {
                ctx.body(*body)?;
                vertex.faces().iter().flat_map(FaceName::features).collect()
            }
            Self::Body(body) => return ctx.body(*body),
            Self::SketchCurves { sketch, curves } => {
                return check_sketch_curves(ctx, *sketch, curves, want);
            }
            Self::SketchPoint { sketch, point } => {
                let (entry, def) = ctx.sketch(*sketch)?;
                return match def.entity(*point) {
                    Some(entity) if entity.is_point() => Ok(()),
                    _ => Err(format!(
                        "{} has no point {}",
                        entry.name,
                        point_name(*point)
                    )),
                };
            }
            Self::FixedPlane {
                origin,
                normal,
                x_axis,
            } => {
                finite("plane", &[*origin, *normal].concat())?;
                let n = unit(*normal).ok_or("the plane's normal is zero")?;
                if let Some(x) = x_axis {
                    finite("plane", x)?;
                    unit(sub(*x, scale(n, dot(*x, n))))
                        .ok_or("the plane's x axis must not be parallel to its normal")?;
                }
                return Ok(());
            }
            Self::FixedAxis { origin, direction } => {
                finite("axis", &[*origin, *direction].concat())?;
                return unit(*direction)
                    .map(|_| ())
                    .ok_or_else(|| "the axis direction is zero".to_owned());
            }
            Self::FixedPoint(point) => return finite("point", point),
            Self::Mount { feature, name } => {
                let entry = ctx.feature(*feature)?;
                return match &entry.def {
                    super::FeatureDef::DerivedPart(_) if valid_mount_name(name) => Ok(()),
                    super::FeatureDef::DerivedPart(_) => {
                        Err(format!("{name} is no name of a mount frame"))
                    }
                    // A pulley's frame on its axis (mitcad#562).
                    super::FeatureDef::Pulley(_) if name == super::pulley::MOUNT => Ok(()),
                    super::FeatureDef::Pulley(_) => Err(format!(
                        "a pulley has one mount frame, {}",
                        super::pulley::MOUNT
                    )),
                    // A printable bearing's bore axis and faces (mitcad#577).
                    super::FeatureDef::PrintableBearing(_)
                        if super::printable_bearing::MOUNTS.contains(&name.as_str()) =>
                    {
                        Ok(())
                    }
                    super::FeatureDef::PrintableBearing(_) => Err(format!(
                        "a bearing's mount frames are {}",
                        super::printable_bearing::MOUNTS.join(", ")
                    )),
                    _ => Err(format!(
                        "{} ({feature}) is not a derived part: it has no mount frames",
                        entry.name
                    )),
                };
            }
        };
        for feature in features {
            ctx.feature(feature).map_err(|e| format!("{self}: {e}"))?;
        }
        Ok(())
    }
}

fn check_sketch_curves(
    ctx: &CheckContext<'_>,
    sketch: FeatureUid,
    curves: &[EntityUid],
    want: Want,
) -> Result<(), String> {
    let (entry, def) = ctx.sketch(sketch)?;
    let line = matches!(want, Want::Line | Want::Axis);
    if line && curves.len() != 1 {
        return Err(format!(
            "an axis is one line of a sketch, not {}",
            GeomRef::SketchCurves {
                sketch,
                curves: curves.to_vec()
            }
        ));
    }
    for curve in curves {
        let entity = def.entity(*curve);
        // Tools are the curves that can bound profiles; axes may be
        // construction or centre lines.
        let usable = entity.is_some_and(|e| !e.is_point() && (line || !e.construction));
        if !usable {
            return Err(format!(
                "{} has no curve {}",
                entry.name,
                curve.curve_name()
            ));
        }
        // An axis may also be a circle or an arc: the line through its
        // centre normal to the sketch.
        let round = want == Want::Axis
            && matches!(
                entity.map(|e| &e.kind),
                Some(
                    crate::sketch::EntityKind::Circle { .. }
                        | crate::sketch::EntityKind::Arc { .. }
                )
            );
        if line
            && !round
            && !matches!(
                entity.map(|e| &e.kind),
                Some(crate::sketch::EntityKind::Line { .. })
            )
        {
            return Err(format!(
                "curve {} of {} is not a line",
                curve.curve_name(),
                entry.name
            ));
        }
    }
    Ok(())
}

impl PathRef {
    pub fn add_to(&self, references: &mut References) {
        match self {
            Self::Edges { body, edges } => {
                references.body(*body);
                references.edges(edges);
            }
            Self::SketchCurve { sketch, .. } | Self::SketchCurves { sketch, .. } => {
                references.features.insert(*sketch);
            }
            // Intersection curves (mitcad#1405).
            Self::FeatureCurve(feature) => {
                references.features.insert(*feature);
            }
            Self::Line { .. } | Self::Helix(_) | Self::Fixed(_) => {}
        }
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            Self::Edges { body, edges } => {
                if edges.is_empty() {
                    return Err("the path has no edges".to_owned());
                }
                ctx.body(*body)?;
                ctx.edges(edges).map_err(|e| format!("path: {e}"))
            }
            Self::SketchCurve { sketch, curve } => {
                let (entry, def) = ctx.sketch(*sketch)?;
                if def.entity(*curve).is_some_and(|e| !e.is_point()) {
                    Ok(())
                } else {
                    Err(format!(
                        "{} has no curve {}",
                        entry.name,
                        curve.curve_name()
                    ))
                }
            }
            Self::SketchCurves { sketch, curves } => {
                let (entry, def) = ctx.sketch(*sketch)?;
                if curves.len() < 2 {
                    return Err("a path of several curves needs at least two".to_owned());
                }
                for (i, curve) in curves.iter().enumerate() {
                    if curves[..i].contains(curve) {
                        return Err(format!(
                            "curve {} is listed more than once",
                            curve.curve_name()
                        ));
                    }
                    if !def.entity(*curve).is_some_and(|e| !e.is_point()) {
                        return Err(format!(
                            "{} has no curve {}",
                            entry.name,
                            curve.curve_name()
                        ));
                    }
                }
                Ok(())
            }
            Self::Line { start, end } => {
                finite("path", &[*start, *end].concat())?;
                if norm(sub(*end, *start)) > LINEAR {
                    Ok(())
                } else {
                    Err("the path has no length".to_owned())
                }
            }
            Self::Helix(helix) => helix.check(),
            // Fixed curves (mitcad#548).
            Self::Fixed(curves) => {
                if curves.is_empty() {
                    return Err("a fixed path needs a curve".to_owned());
                }
                for (i, curve) in curves.iter().enumerate() {
                    curve
                        .check()
                        .map_err(|e| format!("fixed curve {}: {e}", i + 1))?;
                }
                Ok(())
            }
            // Intersection curves (mitcad#1405).
            Self::FeatureCurve(feature) => {
                let entry = ctx.feature(*feature)?;
                if matches!(entry.def, super::FeatureDef::IntersectionCurve(_)) {
                    Ok(())
                } else {
                    Err(format!(
                        "{} ({feature}) makes no curve: a path names an intersection curve",
                        entry.name
                    ))
                }
            }
        }
    }
}

// Resolution.

/// Where references resolve: a feature being evaluated, which records what
/// it reads for the recompute cache, or the document at the marker.
#[doc(hidden)]
pub trait Resolver {
    type Kernel: Kernel;

    fn kernel(&self) -> &Self::Kernel;
    /// The display name of a feature, for messages.
    fn name(&self, uid: FeatureUid) -> String;
    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String>;
    fn shape(&mut self, body: BodyUid) -> Result<<Self::Kernel as Kernel>::Shape, String>;
    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String>;
    /// A face, edge or vertex was found under another name after a sketch
    /// change (`renamed.rs`, mitcad#272): a feature warns; `maker` made
    /// the renamed face.
    fn renamed(&mut self, _what: &str, _stored: &str, _found: &str, _maker: FeatureUid) {}
    /// A derived part's mount frame (mitcad#375), in the coordinates of its
    /// component.
    fn mount(&mut self, feature: FeatureUid, name: &str) -> Result<Transform, String> {
        Err(format!(
            "the frame {name} of {} is not available here",
            self.name(feature)
        ))
    }

    /// The curve of an Intersection Curve (mitcad#1405), its pieces in
    /// order along it, in the feature's coordinates.
    fn feature_curve(
        &mut self,
        feature: FeatureUid,
    ) -> Result<Arc<Vec<crate::sweeps::PathCurve>>, String> {
        Err(format!(
            "the curve of {} is not available here",
            self.name(feature)
        ))
    }
}

fn not_a(resolver: &impl Resolver, uid: FeatureUid, datum: &Datum, wanted: &str) -> String {
    let kind = datum.kind();
    format!(
        "{} is {} {kind}, not {wanted}",
        resolver.name(uid),
        article(kind)
    )
}

fn surface_of(
    r: &mut impl Resolver,
    body: BodyUid,
    face: &FaceName,
) -> Result<SurfaceGeometry, String> {
    let shape = r.shape(body)?;
    let found = super::renamed::face_for(r, &shape, face)?;
    r.kernel()
        .face_geometry(&shape, &found)
        .map_err(|e| format!("face {face}: {e}"))
}

fn curve_of(
    r: &mut impl Resolver,
    body: BodyUid,
    edge: &EdgeName,
) -> Result<CurveGeometry, String> {
    let shape = r.shape(body)?;
    let found = super::renamed::edge_for(r, &shape, edge)?;
    r.kernel()
        .edge_geometry(&shape, &found)
        .map_err(|e| format!("edge {edge}: {e}"))
}

pub(crate) fn resolve_plane(
    r: &mut impl Resolver,
    reference: &GeomRef,
) -> Result<DatumPlane, String> {
    let datum = match reference {
        GeomRef::Origin(origin) => origin.datum(),
        GeomRef::Datum(uid) => r.datum(*uid)?,
        GeomRef::Face { body, face } => {
            return match surface_of(r, *body, face)? {
                SurfaceGeometry::Plane { origin, normal } => {
                    Ok(DatumPlane::on_plane(origin, normal))
                }
                other => Err(format!("face {face} is not planar ({})", other.kind())),
            };
        }
        GeomRef::FixedPlane {
            origin,
            normal,
            x_axis,
        } => {
            let normal = unit(*normal).ok_or("the plane's normal is zero")?;
            return Ok(DatumPlane::from_normal(*origin, normal, *x_axis));
        }
        GeomRef::Mount { feature, name } => {
            let [origin, x_axis, y_axis, _] = crate::joints::frame_parts(&r.mount(*feature, name)?);
            return Ok(DatumPlane {
                origin,
                x_axis,
                y_axis,
            });
        }
        _ => return Err(format!("{reference} is not a plane")),
    };
    match (datum, reference) {
        (Datum::Plane(plane), _) => Ok(plane),
        (other, GeomRef::Datum(uid)) => Err(not_a(r, *uid, &other, "a plane")),
        _ => Err(format!("{reference} is not a plane")),
    }
}

pub(crate) fn resolve_axis(
    r: &mut impl Resolver,
    reference: &GeomRef,
) -> Result<DatumAxis, String> {
    let datum = match reference {
        GeomRef::Origin(origin) => origin.datum(),
        GeomRef::Datum(uid) => r.datum(*uid)?,
        GeomRef::Edge { body, edge } => {
            return match curve_of(r, *body, edge)? {
                CurveGeometry::Line { start, end } => DatumAxis::through(start, end)
                    .ok_or_else(|| format!("edge {edge} has no length")),
                CurveGeometry::Circle { center, normal, .. } => Ok(DatumAxis {
                    origin: center,
                    direction: canonical_axis(unit(normal).ok_or("the circle has no normal")?),
                }),
                other => Err(format!(
                    "edge {edge} is neither straight nor circular ({})",
                    other.kind()
                )),
            };
        }
        GeomRef::Face { body, face } => {
            return match surface_of(r, *body, face)? {
                SurfaceGeometry::Cylinder { origin, axis, .. }
                | SurfaceGeometry::Cone { origin, axis, .. }
                | SurfaceGeometry::Torus {
                    center: origin,
                    axis,
                    ..
                } => Ok(DatumAxis {
                    origin,
                    direction: axis,
                }),
                other => Err(format!("face {face} has no axis ({})", other.kind())),
            };
        }
        GeomRef::SketchCurves { sketch, curves } if curves.len() == 1 => {
            return sketch_line(r, *sketch, curves[0]);
        }
        GeomRef::FixedAxis { origin, direction } => {
            return Ok(DatumAxis {
                origin: *origin,
                direction: unit(*direction).ok_or("the axis direction is zero")?,
            });
        }
        GeomRef::Mount { feature, name } => {
            let [origin, _, _, direction] = crate::joints::frame_parts(&r.mount(*feature, name)?);
            return Ok(DatumAxis { origin, direction });
        }
        _ => return Err(format!("{reference} is not an axis")),
    };
    match (datum, reference) {
        (Datum::Axis(axis), _) => Ok(axis),
        (other, GeomRef::Datum(uid)) => Err(not_a(r, *uid, &other, "an axis")),
        _ => Err(format!("{reference} is not an axis")),
    }
}

/// A line of a sketch from its start to its end: the solved line, else
/// the profile segment of the curve.
fn sketch_line(
    r: &mut impl Resolver,
    sketch: FeatureUid,
    curve: EntityUid,
) -> Result<DatumAxis, String> {
    let output = r.sketch(sketch)?;
    let frame = output.frame;
    let ends = match output.solved.curves.get(&curve) {
        Some(Curve2::Line { a, b }) => Some((frame.point(*a), frame.point(*b))),
        // A circle or an arc as an axis: the line through its centre
        // normal to the sketch's plane.
        Some(Curve2::Circle { center, .. } | Curve2::Arc { center, .. }) => {
            return Ok(DatumAxis {
                origin: frame.point(*center),
                direction: unit(frame.normal()).ok_or("the sketch has no normal")?,
            });
        }
        Some(_) => None,
        None => output
            .regions
            .iter()
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.segments)
            .find(|s| s.key.curve.entity() == Some(curve))
            .and_then(|s| match s.geometry {
                SegmentGeometry::Line { start, end } => {
                    Some((frame.point(start), frame.point(end)))
                }
                _ => None,
            }),
    };
    ends.and_then(|(a, b)| DatumAxis::through(a, b))
        .ok_or_else(|| format!("{} has no line {}", r.name(sketch), curve.curve_name()))
}

pub(crate) fn resolve_point(r: &mut impl Resolver, reference: &GeomRef) -> Result<Vec3, String> {
    let datum = match reference {
        GeomRef::Origin(origin) => origin.datum(),
        GeomRef::Datum(uid) => r.datum(*uid)?,
        GeomRef::Vertex { body, vertex } => {
            let shape = r.shape(*body)?;
            let found = super::renamed::vertex_for(r, &shape, vertex)?;
            return r
                .kernel()
                .vertex_point(&shape, &found)
                .map_err(|e| format!("vertex {vertex}: {e}"));
        }
        GeomRef::Edge { body, edge } => {
            return match curve_of(r, *body, edge)? {
                CurveGeometry::Circle { center, .. } => Ok(center),
                CurveGeometry::Line { start, end } => Ok(scale(add(start, end), 0.5)),
                other => Err(format!(
                    "edge {edge} is neither straight nor circular ({})",
                    other.kind()
                )),
            };
        }
        GeomRef::SketchPoint { sketch, point } => {
            let output = r.sketch(*sketch)?;
            return output
                .solved
                .points
                .get(point)
                .map(|p| output.frame.point(*p))
                .ok_or_else(|| format!("{} has no point {}", r.name(*sketch), point_name(*point)));
        }
        GeomRef::FixedPoint(point) => return Ok(*point),
        GeomRef::Mount { feature, name } => return Ok(r.mount(*feature, name)?.translation),
        _ => return Err(format!("{reference} is not a point")),
    };
    match (datum, reference) {
        (Datum::Point(point), _) => Ok(point.point),
        (other, GeomRef::Datum(uid)) => Err(not_a(r, *uid, &other, "a point")),
        _ => Err(format!("{reference} is not a point")),
    }
}

pub(crate) fn resolve_surface(
    r: &mut impl Resolver,
    reference: &GeomRef,
) -> Result<SurfaceGeometry, String> {
    let GeomRef::Face { body, face } = reference else {
        return Err(format!("{reference} is not a face"));
    };
    surface_of(r, *body, face)
}

pub(crate) fn resolve_curve(
    r: &mut impl Resolver,
    reference: &GeomRef,
) -> Result<CurveGeometry, String> {
    let GeomRef::Edge { body, edge } = reference else {
        return Err(format!("{reference} is not an edge"));
    };
    curve_of(r, *body, edge)
}

pub(crate) fn resolve_face_point(
    r: &mut impl Resolver,
    reference: &GeomRef,
    near: Vec3,
) -> Result<SurfacePoint, String> {
    let GeomRef::Face { body, face } = reference else {
        // A plane stands for a planar face: its whole plane.
        let plane =
            resolve_plane(r, reference).map_err(|_| format!("{reference} is not a face"))?;
        let n = plane.normal();
        return Ok(SurfacePoint {
            point: sub(near, scale(n, dot(sub(near, plane.origin), n))),
            normal: n,
        });
    };
    let shape = r.shape(*body)?;
    let found = super::renamed::face_for(r, &shape, face)?;
    let face = &found;
    // A planar face is its whole plane (mitcad#186: the reference model of an
    // axis perpendicular to a face at a point outside it goes through the
    // point's projection onto the plane).
    if let Ok(SurfaceGeometry::Plane { origin, normal }) = r.kernel().face_geometry(&shape, face)
        && let Some(n) = unit(normal)
    {
        let d = dot(sub(near, origin), n);
        return Ok(SurfacePoint {
            point: sub(near, scale(n, d)),
            normal: n,
        });
    }
    r.kernel()
        .face_point_normal(&shape, face, near)
        .map_err(|e| format!("face {face}: {e}"))
}

pub(crate) fn resolve_path(
    r: &mut impl Resolver,
    path: &PathRef,
    at: PathParameter,
) -> Result<PathPoint, String> {
    match path {
        PathRef::Edges { body, edges } => {
            let shape = r.shape(*body)?;
            let mut found = Vec::with_capacity(edges.len());
            for edge in edges {
                found.push(
                    super::renamed::edge_for(r, &shape, edge).map_err(|e| format!("path: {e}"))?,
                );
            }
            r.kernel()
                .path_point(&shape, &found, at)
                .map_err(|e| format!("path: {e}"))
        }
        PathRef::SketchCurves { sketch, curves } => {
            Ok(chain_point(&sketch_chain(r, *sketch, curves)?, at))
        }
        // Along an Intersection Curve (mitcad#1405), measured along it.
        PathRef::FeatureCurve(feature) => {
            let curve = r.feature_curve(*feature)?;
            Ok(super::intersection_curve::CurveChain::new(&curve)?.path_point(at))
        }
        _ => Ok(path_curve(r, path)?.path_point(at)),
    }
}

/// The lines and arcs of a sketch joined end to end, in order along the
/// path (`path.rs`: it runs from the free end of the first listed curve).
fn sketch_chain(
    r: &mut impl Resolver,
    sketch: FeatureUid,
    curves: &[EntityUid],
) -> Result<Vec<PathCurve>, String> {
    use super::path::{chain, curve3};
    use crate::kernel::Curve3;
    use crate::sweeps::PathCurve as Piece;
    let output = r.sketch(sketch)?;
    let mut pieces = Vec::new();
    for id in curves {
        let curve = output
            .solved
            .curves
            .get(id)
            .ok_or_else(|| format!("{} has no curve {}", r.name(sketch), id.curve_name()))?;
        pieces.push(Piece {
            name: id.curve_name(),
            curve: curve3(&output.frame, curve),
        });
    }
    chain(pieces)?
        .into_iter()
        .map(|piece| match piece.curve {
            Curve3::Line { start, end } => PathCurve::line(start, end),
            Curve3::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed: false,
            } if (major - minor).abs() <= LINEAR => {
                let y_axis = mitcad_vec3::cross(normal, x_axis);
                let (sin, cos) = start.sin_cos();
                Ok(PathCurve::Arc {
                    center,
                    x: add(scale(x_axis, cos), scale(y_axis, sin)),
                    y: add(scale(x_axis, -sin), scale(y_axis, cos)),
                    radius: major,
                    sweep: end - start,
                })
            }
            _ => Err(format!("curve {} is not a line or an arc", piece.name)),
        })
        .collect()
}

/// A point of a chain of curves, as [`PathCurve::path_point`] gives one of
/// a single curve: a fraction of the whole length, a length from the
/// start (a vertex between two curves takes the tangent of the first),
/// or the nearest point; beyond the ends it continues straight.
fn chain_point(chain: &[PathCurve], at: PathParameter) -> PathPoint {
    let total: f64 = chain.iter().map(PathCurve::length).sum();
    let s = match at {
        PathParameter::Fraction(f) => f * total,
        PathParameter::Length(l) => l,
        PathParameter::Near(p) => {
            let mut best = (f64::INFINITY, 0.0);
            let mut before = 0.0;
            for piece in chain {
                let local = piece.nearest(p);
                let distance = norm(sub(piece.point(local), p));
                if distance < best.0 - 1e-12 {
                    best = (distance, before + local);
                }
                before += piece.length();
            }
            best.1
        }
    };
    let mut before = 0.0;
    for (i, piece) in chain.iter().enumerate() {
        let length = piece.length();
        let last = i + 1 == chain.len();
        if s <= before + length || last {
            return piece.path_point(PathParameter::Length(s - before));
        }
        before += length;
    }
    unreachable!("a chain has curves")
}

/// Lines and arcs of a sketch joined end to end, by arc length along the
/// path (a path pattern along a chain, mitcad#464), with the sketch's
/// normal that copies turned along the path turn about.
pub(crate) struct PathChain {
    pieces: Vec<PathCurve>,
    pub normal: Vec3,
}

impl PathChain {
    /// The chain of the curves (see [`sketch_chain`]).
    pub(crate) fn of_sketch(
        r: &mut impl Resolver,
        sketch: FeatureUid,
        curves: &[EntityUid],
    ) -> Result<Self, String> {
        let pieces = sketch_chain(r, sketch, curves)?;
        let normal = r.sketch(sketch)?.frame.normal();
        Ok(Self { pieces, normal })
    }

    pub(crate) fn length(&self) -> f64 {
        self.pieces.iter().map(PathCurve::length).sum()
    }

    /// Whether the chain ends where it starts (mitcad#673): within 1e-6 of
    /// the size of its end points, as [`sketch_chain`] joins its curves.
    pub(crate) fn closed(&self) -> bool {
        let (Some(first), Some(last)) = (self.pieces.first(), self.pieces.last()) else {
            return false;
        };
        let (a, b) = (first.point(0.0), last.point(last.length()));
        let size = a.iter().chain(&b).fold(1.0f64, |m, v| m.max(v.abs()));
        self.pieces.len() > 1 && norm(sub(a, b)) <= 1e-6 * size
    }

    /// The arc length of the chain's point nearest to `p` (a vertex
    /// between two curves counts to the first).
    pub(crate) fn nearest(&self, p: Vec3) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        let mut before = 0.0;
        for piece in &self.pieces {
            let local = piece.nearest(p);
            let distance = norm(sub(piece.point(local), p));
            if distance < best.0 - 1e-12 {
                best = (distance, before + local);
            }
            before += piece.length();
        }
        best.1
    }

    /// The point and the unit tangent at arc length `s`; before the start
    /// and past the end the first and the last curve go on (straight, or
    /// around their circle), and a vertex between two curves takes the
    /// tangent of the first. Along a closed chain `s` goes round it again
    /// (mitcad#673).
    pub(crate) fn at(&self, s: f64) -> (Vec3, Vec3) {
        let s = if self.closed() {
            s.rem_euclid(self.length())
        } else {
            s
        };
        let mut before = 0.0;
        for (i, piece) in self.pieces.iter().enumerate() {
            let length = piece.length();
            if s <= before + length || i + 1 == self.pieces.len() {
                let local = s - before;
                return (piece.point(local), piece.tangent(local));
            }
            before += length;
        }
        unreachable!("a chain has curves")
    }
}

/// The curve of a sketch-curve or line path.
pub(crate) fn path_curve(r: &mut impl Resolver, path: &PathRef) -> Result<PathCurve, String> {
    let (sketch, curve) = match path {
        PathRef::Line { start, end } => return PathCurve::line(*start, *end),
        PathRef::Helix(helix) => return PathCurve::helix(helix),
        PathRef::SketchCurve { sketch, curve } => (*sketch, *curve),
        PathRef::Edges { .. } => {
            return Err("a path of edges has no analytic curve".to_owned());
        }
        PathRef::SketchCurves { .. } => {
            return Err("a path of several curves is not one curve".to_owned());
        }
        PathRef::Fixed(_) => {
            return Err("a path of fixed curves has no analytic curve".to_owned());
        }
        PathRef::FeatureCurve(_) => {
            return Err("an intersection curve has no analytic curve".to_owned());
        }
    };
    let output = r.sketch(sketch)?;
    let frame = output.frame;
    // The whole solved curve (construction curves too).
    let geometry = output
        .solved
        .curves
        .get(&curve)
        .map(|c| c.whole())
        .ok_or_else(|| format!("{} has no curve {}", r.name(sketch), curve.curve_name()))?;
    let in_3d = |v: [f64; 2]| -> Vec3 {
        std::array::from_fn(|i| v[0] * frame.x_axis[i] + v[1] * frame.y_axis[i])
    };
    let arc = |center: [f64; 2], radius: f64, start: f64, sweep: f64| PathCurve::Arc {
        center: frame.point(center),
        x: in_3d([start.cos(), start.sin()]),
        y: in_3d([-start.sin(), start.cos()]),
        radius,
        sweep,
    };
    match geometry {
        SegmentGeometry::Line { start, end } => {
            PathCurve::line(frame.point(start), frame.point(end))
        }
        SegmentGeometry::Circle {
            center,
            radius,
            start,
            // A whole turn from where the circle starts: `arc` takes the
            // sweep, not the end angle.
        } => Ok(arc(center, radius, start, std::f64::consts::TAU)),
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let sweep = (end_angle - start_angle).rem_euclid(std::f64::consts::TAU);
            Ok(arc(center, radius, start_angle, sweep))
        }
        _ => Err(format!(
            "curve {} is not a line, an arc or a circle",
            curve.curve_name()
        )),
    }
}

/// A sketch-curve or line path in model space, by arc length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathCurve {
    Line {
        start: Vec3,
        direction: Vec3,
        length: f64,
    },
    /// Counter-clockwise about `x × y` from `x`, `sweep` radians.
    Arc {
        center: Vec3,
        x: Vec3,
        y: Vec3,
        radius: f64,
        sweep: f64,
    },
    /// A helix (mitcad#385): [`crate::kernel::Curve3::Helix`] from its
    /// parameter `start` to `end`.
    Helix {
        center: Vec3,
        axis: Vec3,
        x: Vec3,
        radius: f64,
        pitch: f64,
        start: f64,
        end: f64,
    },
}

impl PathCurve {
    fn helix(helix: &HelixPath) -> Result<Self, String> {
        match helix.curve()? {
            crate::kernel::Curve3::Helix {
                center,
                axis,
                x_axis,
                radius,
                pitch,
                start,
                end,
            } => Ok(Self::Helix {
                center,
                axis,
                x: x_axis,
                radius,
                pitch,
                start,
                end,
            }),
            _ => unreachable!("a helix path's curve is a helix"),
        }
    }

    /// A helix's arc length per unit of its parameter.
    fn speed(radius: f64, pitch: f64) -> f64 {
        radius.hypot(pitch / std::f64::consts::TAU)
    }

    fn line(start: Vec3, end: Vec3) -> Result<Self, String> {
        let delta = sub(end, start);
        let direction = unit(delta).ok_or("the path has no length")?;
        Ok(Self::Line {
            start,
            direction,
            length: norm(delta),
        })
    }

    pub fn length(&self) -> f64 {
        match self {
            Self::Line { length, .. } => *length,
            Self::Arc { radius, sweep, .. } => radius * sweep,
            Self::Helix {
                radius,
                pitch,
                start,
                end,
                ..
            } => (end - start) * Self::speed(*radius, *pitch),
        }
    }

    /// The point at arc length `s`; before the start and past the end the
    /// path goes on straight or around the circle.
    pub fn point(&self, s: f64) -> Vec3 {
        match self {
            Self::Line {
                start, direction, ..
            } => add(*start, scale(*direction, s)),
            Self::Arc {
                center,
                x,
                y,
                radius,
                ..
            } => {
                let t = s / radius;
                add(
                    *center,
                    add(scale(*x, radius * t.cos()), scale(*y, radius * t.sin())),
                )
            }
            Self::Helix {
                center,
                axis,
                x,
                radius,
                pitch,
                start,
                ..
            } => super::path::helix_point(
                *center,
                *axis,
                *x,
                (*radius, *pitch),
                start + s / Self::speed(*radius, *pitch),
            ),
        }
    }

    /// The unit tangent at arc length `s`.
    fn tangent(&self, s: f64) -> Vec3 {
        match self {
            Self::Line { direction, .. } => *direction,
            Self::Arc { x, y, radius, .. } => {
                let t = s / radius;
                add(scale(*x, -t.sin()), scale(*y, t.cos()))
            }
            Self::Helix {
                axis,
                x,
                radius,
                pitch,
                start,
                ..
            } => {
                let speed = Self::speed(*radius, *pitch);
                let t = start + s / speed;
                let y = mitcad_vec3::cross(*axis, *x);
                let d = add(
                    add(scale(*x, -radius * t.sin()), scale(y, radius * t.cos())),
                    scale(*axis, pitch / std::f64::consts::TAU),
                );
                scale(d, 1.0 / speed)
            }
        }
    }

    /// A point of the path as construction geometry takes it: fractions
    /// and lengths beyond the ends continue straight along the end
    /// tangents; `Near` is the nearest point of the path.
    pub fn path_point(&self, at: PathParameter) -> PathPoint {
        let total = self.length();
        let s = match at {
            PathParameter::Fraction(f) => f * total,
            PathParameter::Length(l) => l,
            PathParameter::Near(p) => self.nearest(p),
        };
        let end = s.clamp(0.0, total);
        let tangent = self.tangent(end);
        PathPoint {
            point: add(self.point(end), scale(tangent, s - end)),
            tangent,
        }
    }

    /// The arc length of the path point nearest to `p`.
    pub(crate) fn nearest(&self, p: Vec3) -> f64 {
        match self {
            Self::Line {
                start,
                direction,
                length,
            } => dot(sub(p, *start), *direction).clamp(0.0, *length),
            Self::Arc {
                center,
                x,
                y,
                radius,
                sweep,
            } => {
                let d = sub(p, *center);
                let angle = dot(d, *y)
                    .atan2(dot(d, *x))
                    .rem_euclid(std::f64::consts::TAU);
                if angle <= *sweep {
                    return angle * radius;
                }
                // Past the end: the nearer end.
                let ends = [0.0, *sweep * radius];
                let distance = |s: f64| norm(sub(self.point(s), p));
                if distance(ends[0]) <= distance(ends[1]) {
                    ends[0]
                } else {
                    ends[1]
                }
            }
            // The nearest of points a degree of turn apart, then narrowed
            // down between its neighbours.
            Self::Helix { start, end, .. } => {
                let total = self.length();
                let steps = (((end - start) / 1f64.to_radians()).ceil() as usize).clamp(8, 100_000);
                let distance = |s: f64| norm(sub(self.point(s), p));
                let at = |i: usize| total * i as f64 / steps as f64;
                let best = (0..=steps)
                    .min_by(|a, b| distance(at(*a)).total_cmp(&distance(at(*b))))
                    .unwrap_or(0);
                let (mut lo, mut hi) = (at(best.saturating_sub(1)), at((best + 1).min(steps)));
                for _ in 0..100 {
                    let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                    if distance(a) <= distance(b) {
                        hi = b;
                    } else {
                        lo = a;
                    }
                }
                (lo + hi) / 2.0
            }
        }
    }
}

impl<K: Kernel> Resolver for EvalContext<'_, K> {
    type Kernel = K;

    fn kernel(&self) -> &K {
        self.kernel
    }

    fn name(&self, uid: FeatureUid) -> String {
        self.feature_name(uid)
    }

    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        let datum = EvalContext::datum(self, uid)?;
        // A datum of another component by its link (mitcad#1035): made in
        // that component's coordinates, moved into the feature's.
        Ok(match self.datum_placement(uid)? {
            Some(transform) => placed_datum(datum, &transform),
            None => datum,
        })
    }

    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        let Some(link) = self.geom_links.get(&body).cloned() else {
            return self.body(body);
        };
        let (component, transform) = self
            .linked(&link)
            .map_err(|e| format!("body {body}: {e}"))?;
        let shape = self.body_in(component, body)?;
        self.kernel
            .transform_shape(&shape, &transform, None)
            .map_err(|e| format!("body {body}: {e}"))
    }

    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        let output = EvalContext::sketch(self, uid)?;
        // A sketch of another component by its link (mitcad#1039): its
        // frame moved into the feature's coordinates. Its curves and points
        // are in the sketch's own plane coordinates, so they follow.
        let Some(transform) = self.datum_placement(uid)? else {
            return Ok(output);
        };
        let mut placed = SketchOutput::clone(&output);
        placed.frame = transform.apply_frame(&output.frame);
        placed.base = transform.apply_frame(&output.base);
        Ok(Arc::new(placed))
    }

    fn renamed(&mut self, what: &str, stored: &str, found: &str, maker: FeatureUid) {
        self.found_renamed(what, &stored, &found, maker);
    }

    fn mount(&mut self, feature: FeatureUid, name: &str) -> Result<Transform, String> {
        EvalContext::mount(self, feature, name)
    }

    fn feature_curve(
        &mut self,
        feature: FeatureUid,
    ) -> Result<Arc<Vec<crate::sweeps::PathCurve>>, String> {
        EvalContext::feature_curve(self, feature)
    }
}

impl<K: Kernel> EvalContext<'_, K> {
    /// The placement of a construction datum or a sketch of another
    /// component by the feature's `datum_links` (mitcad#1035, mitcad#1039):
    /// from that component's coordinates into the feature's. None without
    /// a link; an error when the link's source shows another component than
    /// the datum's or sketch's.
    fn datum_placement(&mut self, uid: FeatureUid) -> Result<Option<Transform>, String> {
        let Some(link) = self.datum_links.get(&uid).cloned() else {
            return Ok(None);
        };
        let name = self.feature_name(uid);
        let (component, transform) = self.linked(&link).map_err(|e| format!("{name}: {e}"))?;
        match self.env.feature(uid).map(|f| f.component) {
            Some(own) if own != component => Err(format!(
                "{name} is in component {own}, not in {component} where its link leads"
            )),
            _ => Ok(Some(transform)),
        }
    }
}

/// A datum moved by a rigid motion (a placement from a linked component's
/// coordinates into the feature's).
fn placed_datum(datum: Datum, t: &Transform) -> Datum {
    match datum {
        Datum::Plane(p) => Datum::Plane(DatumPlane {
            origin: t.apply_point(p.origin),
            x_axis: t.apply_vector(p.x_axis),
            y_axis: t.apply_vector(p.y_axis),
        }),
        Datum::Axis(a) => Datum::Axis(DatumAxis {
            origin: t.apply_point(a.origin),
            direction: t.apply_vector(a.direction),
        }),
        Datum::Point(p) => Datum::Point(DatumPoint {
            point: t.apply_point(p.point),
        }),
    }
}

/// References for features: every read is recorded for the recompute
/// cache.
impl<K: Kernel> EvalContext<'_, K> {
    /// The datum of a construction feature before this one.
    pub fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        let output = self.env.datums.get(&uid);
        self.reads.push(Read::Datum(uid, output.map(|d| d.version)));
        output
            .map(|d| d.value)
            .ok_or_else(|| format!("{} has no result", self.feature_name(uid)))
    }

    /// The curve of an Intersection Curve before this one (mitcad#1405),
    /// its pieces in order along it, in this feature's coordinates: one of
    /// its own component as it is, one of another component by the
    /// feature's `datum_links` (a construction feature's, mitcad#1417),
    /// moved there as [`EvalContext::linked_feature_curve`] moves it.
    pub fn feature_curve(
        &mut self,
        feature: FeatureUid,
    ) -> Result<Arc<Vec<crate::sweeps::PathCurve>>, String> {
        if let Some(link) = self.datum_links.get(&feature).cloned() {
            return self.linked_feature_curve(feature, Some(&link));
        }
        self.linked_feature_curve(feature, None)
    }

    /// The curve of an Intersection Curve before this one, in this
    /// feature's coordinates: without a link one of this feature's
    /// component; with one (mitcad#1417: a sketch's projection of it, a
    /// construction feature's `datum_links`) one of the component the
    /// link's source is in, moved from there by the placements at this
    /// point of the timeline, as linked sketches and datums are.
    pub fn linked_feature_curve(
        &mut self,
        feature: FeatureUid,
        link: Option<&crate::links::OccurrenceLink>,
    ) -> Result<Arc<Vec<crate::sweeps::PathCurve>>, String> {
        let result = self.env.result_of(feature);
        let output = result.and_then(|r| r.output.clone());
        self.reads
            .push(Read::Output(feature, output.as_ref().map(|o| o.version)));
        let name = self.feature_name(feature);
        let (component, moved) = match link {
            Some(link) => {
                let (component, transform) =
                    self.linked(link).map_err(|e| format!("{name}: {e}"))?;
                (component, Some(transform))
            }
            None => (self.component(), None),
        };
        if let Some(entry) = self.env.feature(feature)
            && entry.component != component
        {
            return Err(match link {
                Some(link) => format!(
                    "{name} is not in the component where the link {} leads",
                    link.source
                ),
                None => format!(
                    "{name} is in another component: take the curve of an intersection \
                     curve of another component through its occurrence"
                ),
            });
        }
        let failed = result.is_some_and(|r| !r.status.is_ok());
        let curve = match output.and_then(|o| o.curve.clone()) {
            Some(curve) if !failed => curve,
            _ => return Err(format!("{name} has no curve at this point of the timeline")),
        };
        Ok(match moved {
            Some(t) => Arc::new(
                curve
                    .iter()
                    .map(|piece| crate::sweeps::PathCurve {
                        name: piece.name.clone(),
                        curve: crate::sketch::project::transform_curve(&t, &piece.curve),
                    })
                    .collect(),
            ),
            None => curve,
        })
    }

    /// A mount frame of a derived part before this one (mitcad#375), in
    /// the coordinates of that part's component.
    pub fn mount(&mut self, feature: FeatureUid, name: &str) -> Result<Transform, String> {
        let result = self.env.result_of(feature);
        let output = result.and_then(|r| r.output.clone());
        self.reads
            .push(Read::Output(feature, output.as_ref().map(|o| o.version)));
        let failed = result.is_some_and(|r| !r.status.is_ok());
        let frames = output.as_ref().and_then(|o| o.mounts.clone());
        match frames {
            Some(frames) if !failed => frames
                .get(name)
                .copied()
                .ok_or_else(|| format!("{} has no frame {name}", self.feature_name(feature))),
            _ => Err(format!(
                "{} has no frames at this point of the timeline",
                self.feature_name(feature)
            )),
        }
    }

    /// The plane of a construction plane feature (a sketch plane).
    pub fn datum_plane(&mut self, uid: FeatureUid) -> Result<DatumPlane, String> {
        resolve_plane(self, &GeomRef::Datum(uid))
    }

    /// An origin plane, a construction plane, a planar face or a fixed
    /// plane, with its frame.
    pub fn plane(&mut self, reference: &GeomRef) -> Result<DatumPlane, String> {
        resolve_plane(self, reference)
    }

    /// An axis: origin and construction axes, edges, sketch lines, faces
    /// of revolution, fixed axes.
    pub fn axis(&mut self, reference: &GeomRef) -> Result<DatumAxis, String> {
        resolve_axis(self, reference)
    }

    /// A point: the origin, construction points, vertices, edges' centres
    /// or middles, sketch points, fixed points.
    pub fn point(&mut self, reference: &GeomRef) -> Result<Vec3, String> {
        resolve_point(self, reference)
    }

    /// The surface of a face.
    pub fn surface(&mut self, reference: &GeomRef) -> Result<SurfaceGeometry, String> {
        resolve_surface(self, reference)
    }

    /// The curve of an edge.
    pub fn curve(&mut self, reference: &GeomRef) -> Result<CurveGeometry, String> {
        resolve_curve(self, reference)
    }

    /// The point of a face nearest to `near` and the outward normal there.
    pub fn face_point(&mut self, reference: &GeomRef, near: Vec3) -> Result<SurfacePoint, String> {
        resolve_face_point(self, reference, near)
    }

    /// A point of a path and its tangent.
    pub fn path_point(&mut self, path: &PathRef, at: PathParameter) -> Result<PathPoint, String> {
        resolve_path(self, path, at)
    }
}

// Files and commands.

const EXPECTED: &str = "an origin datum (xy, xz, yz, x, y, z, origin), a construction feature \
                        like \"F5\", {\"body\": ...} with an optional \"face\", \"edge\" or \
                        \"vertex\", {\"sketch\": ...} with \"curve\", \"curves\" or \"point\", or \
                        a fixed plane, axis or point, or {\"mount\": \"F7:mount(end_a)\"}";

fn vec3(v: &Vec3) -> Value {
    Value::from(v.to_vec())
}

thread_local! {
    /// Reorient Design (mitcad#502, `reorient.rs`): while set, references
    /// and paths serialize as they are after this turn of the design, so
    /// that every reference a definition holds, wherever it is, turns once.
    static TURNING: std::cell::Cell<Option<crate::reorient::Turn>> =
        const { std::cell::Cell::new(None) };
}

/// The JSON form of `value` with every reference and path in it turned
/// (see [`crate::reorient::Turn::reference`]).
pub(crate) fn to_turned_value<T: Serialize>(
    value: &T,
    turn: crate::reorient::Turn,
) -> serde_json::Result<Value> {
    struct Reset(Option<crate::reorient::Turn>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TURNING.with(|t| t.set(self.0));
        }
    }
    let _reset = Reset(TURNING.with(|t| t.replace(Some(turn))));
    serde_json::to_value(value)
}

impl Serialize for GeomRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match TURNING.with(std::cell::Cell::get) {
            Some(turn) => turn.reference(self).serialize_as_is(serializer),
            None => self.serialize_as_is(serializer),
        }
    }
}

impl GeomRef {
    fn serialize_as_is<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = Map::new();
        let mut put = |key: &str, value: Value| {
            map.insert(key.to_owned(), value);
        };
        match self {
            Self::Origin(origin) => return serializer.serialize_str(origin.id()),
            Self::Datum(uid) => return serializer.collect_str(uid),
            Self::Face { body, face } => {
                put("body", body.to_string().into());
                put("face", face.to_string().into());
            }
            Self::Edge { body, edge } => {
                put("body", body.to_string().into());
                put("edge", edge.to_string().into());
            }
            Self::Vertex { body, vertex } => {
                put("body", body.to_string().into());
                put("vertex", vertex.to_string().into());
            }
            Self::Body(body) => put("body", body.to_string().into()),
            Self::SketchCurves { sketch, curves } => {
                put("sketch", sketch.to_string().into());
                match curves.as_slice() {
                    [] => {}
                    [curve] => put("curve", curve.curve_name().into()),
                    _ => put(
                        "curves",
                        curves.iter().map(|c| Value::from(c.curve_name())).collect(),
                    ),
                }
            }
            Self::SketchPoint { sketch, point } => {
                put("sketch", sketch.to_string().into());
                put("point", point_name(*point).into());
            }
            Self::FixedPlane {
                origin,
                normal,
                x_axis,
            } => {
                put("origin", vec3(origin));
                put("normal", vec3(normal));
                if let Some(x) = x_axis {
                    put("x_axis", vec3(x));
                }
            }
            Self::FixedAxis { origin, direction } => {
                put("origin", vec3(origin));
                put("direction", vec3(direction));
            }
            Self::FixedPoint(point) => put("point", vec3(point)),
            Self::Mount { feature, name } => {
                put("mount", format!("{feature}:mount({name})").into())
            }
        }
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GeomRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        GeomRef::from_value(Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl Serialize for PathRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match TURNING.with(std::cell::Cell::get) {
            Some(turn) => turn.path(self).serialize_as_is(serializer),
            None => self.serialize_as_is(serializer),
        }
    }
}

impl PathRef {
    fn serialize_as_is<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = Map::new();
        match self {
            Self::Edges { body, edges } => {
                map.insert("body".to_owned(), body.to_string().into());
                map.insert(
                    "edges".to_owned(),
                    edges.iter().map(|e| Value::from(e.to_string())).collect(),
                );
            }
            Self::SketchCurve { sketch, curve } => {
                map.insert("sketch".to_owned(), sketch.to_string().into());
                map.insert("curve".to_owned(), curve.curve_name().into());
            }
            Self::Line { start, end } => {
                map.insert("start".to_owned(), vec3(start));
                map.insert("end".to_owned(), vec3(end));
            }
            Self::SketchCurves { sketch, curves } => {
                map.insert("sketch".to_owned(), sketch.to_string().into());
                map.insert(
                    "curves".to_owned(),
                    curves.iter().map(|c| Value::from(c.curve_name())).collect(),
                );
            }
            Self::Helix(helix) => {
                map.insert(
                    "helix".to_owned(),
                    serde_json::to_value(helix).map_err(serde::ser::Error::custom)?,
                );
            }
            Self::Fixed(curves) => {
                map.insert(
                    "fixed_curves".to_owned(),
                    serde_json::to_value(curves).map_err(serde::ser::Error::custom)?,
                );
            }
            Self::FeatureCurve(feature) => {
                map.insert("feature".to_owned(), feature.to_string().into());
            }
        }
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PathRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PathRef::from_value(Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The fields of a reference object, each taken once; what is left over
/// is an unknown field.
struct Fields(Map<String, Value>);

impl Fields {
    fn take<T: serde::de::DeserializeOwned>(&mut self, key: &str) -> Result<Option<T>, String> {
        match self.0.remove(key) {
            None => Ok(None),
            Some(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|e| format!("{key}: {e}")),
        }
    }

    fn need<T: serde::de::DeserializeOwned>(&mut self, key: &str, form: &str) -> Result<T, String> {
        self.take(key)?
            .ok_or_else(|| format!("{form} needs \"{key}\""))
    }

    fn curve(&mut self, key: &str) -> Result<Option<EntityUid>, String> {
        self.take::<String>(key)?
            .map(|text| EntityUid::parse_curve(&text).map_err(|e| e.to_string()))
            .transpose()
    }

    fn curves(&mut self, key: &str) -> Result<Option<Vec<EntityUid>>, String> {
        self.take::<Vec<String>>(key)?
            .map(|texts| {
                texts
                    .iter()
                    .map(|t| EntityUid::parse_curve(t).map_err(|e| e.to_string()))
                    .collect()
            })
            .transpose()
    }

    fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// No fields are left.
    fn done<T>(self, value: T) -> Result<T, String> {
        match self.0.keys().next() {
            Some(key) => Err(format!("unknown field `{key}`")),
            None => Ok(value),
        }
    }
}

fn origin_datum(id: &str, kind: DatumKind) -> Result<OriginDatum, String> {
    OriginDatum::parse(id)
        .filter(|o| o.kind() == kind)
        .ok_or_else(|| format!("unknown origin {kind} “{id}”"))
}

impl GeomRef {
    /// Reads the JSON form, or an earlier `"type"`-tagged one.
    pub fn from_value(value: Value) -> Result<Self, String> {
        match value {
            Value::String(text) => {
                if let Some(origin) = OriginDatum::parse(&text) {
                    Ok(Self::Origin(origin))
                } else if let Ok(uid) = text.parse::<FeatureUid>() {
                    Ok(Self::Datum(uid))
                } else {
                    Err(format!("invalid reference “{text}”: expected {EXPECTED}"))
                }
            }
            Value::Object(map) => {
                let mut fields = Fields(map);
                match fields.take::<String>("type")? {
                    Some(tag) => Self::tagged(&tag, fields),
                    None => Self::untagged(fields),
                }
            }
            other => Err(format!("invalid reference {other}: expected {EXPECTED}")),
        }
    }

    fn untagged(mut f: Fields) -> Result<Self, String> {
        if let Some(sketch) = f.take::<FeatureUid>("sketch")? {
            let curve = f.curve("curve")?;
            let curves = f.curves("curves")?;
            let point = f.take::<String>("point")?;
            let reference = match (curve, curves, point) {
                (Some(curve), None, None) => Self::SketchCurves {
                    sketch,
                    curves: vec![curve],
                },
                (None, curves, None) => Self::SketchCurves {
                    sketch,
                    curves: curves.unwrap_or_default(),
                },
                (None, None, Some(point)) => Self::SketchPoint {
                    sketch,
                    point: mitcad_sketch::internal::point_serde::parse(&point)
                        .map_err(|e| e.to_string())?,
                },
                _ => {
                    return Err(
                        "a reference to a sketch names \"curve\", \"curves\" or \"point\""
                            .to_owned(),
                    );
                }
            };
            return f.done(reference);
        }
        if let Some(body) = f.take::<BodyUid>("body")? {
            let face = f.take::<FaceName>("face")?;
            let edge = f.take::<EdgeName>("edge")?;
            let vertex = f.take::<VertexName>("vertex")?;
            let reference = match (face, edge, vertex) {
                (Some(face), None, None) => Self::Face { body, face },
                (None, Some(edge), None) => Self::Edge { body, edge },
                (None, None, Some(vertex)) => Self::Vertex { body, vertex },
                (None, None, None) => Self::Body(body),
                _ => {
                    return Err(
                        "a reference to a body names one \"face\", \"edge\" or \"vertex\""
                            .to_owned(),
                    );
                }
            };
            return f.done(reference);
        }
        if f.has("normal") {
            let reference = Self::FixedPlane {
                origin: f.need("origin", "a fixed plane")?,
                normal: f.need("normal", "a fixed plane")?,
                x_axis: f.take("x_axis")?,
            };
            return f.done(reference);
        }
        if f.has("direction") {
            let reference = Self::FixedAxis {
                origin: f.need("origin", "a fixed axis")?,
                direction: f.need("direction", "a fixed axis")?,
            };
            return f.done(reference);
        }
        if let Some(point) = f.take::<Vec3>("point")? {
            return f.done(Self::FixedPoint(point));
        }
        if let Some(text) = f.take::<String>("mount")? {
            let (feature, name) = parse_mount(&text)?;
            return f.done(Self::Mount { feature, name });
        }
        match f.0.keys().next() {
            Some(key) => Err(format!(
                "invalid reference: unexpected field `{key}`, expected {EXPECTED}"
            )),
            None => Err(format!("invalid reference {{}}: expected {EXPECTED}")),
        }
    }

    /// The `"type"`-tagged references of F2 (`origin_plane`, `plane`,
    /// `face`, `body`, `sketch`) and F4 (`origin`, `face`, `edge`, `vertex`,
    /// `construction`, `fixed`).
    fn tagged(tag: &str, mut f: Fields) -> Result<Self, String> {
        let reference = match tag {
            "origin" => match (f.take::<String>("plane")?, f.take::<String>("axis")?) {
                (Some(plane), None) => Self::Origin(origin_datum(&plane, DatumKind::Plane)?),
                (None, Some(axis)) => Self::Origin(origin_datum(&axis, DatumKind::Axis)?),
                (None, None) => Self::Origin(OriginDatum::Origin),
                _ => return Err("an origin reference names a plane or an axis".to_owned()),
            },
            "origin_plane" => {
                if f.has("offset") {
                    return Err("an offset belongs to the feature (\"offset\")".to_owned());
                }
                let plane: String = f.need("plane", "an origin plane")?;
                Self::Origin(origin_datum(&plane, DatumKind::Plane)?)
            }
            "construction" => Self::Datum(f.need("feature", "a construction reference")?),
            "face" => {
                let face: FaceName = f.need("face", "a face reference")?;
                match f.take::<BodyUid>("body")? {
                    Some(body) => Self::Face { body, face },
                    None => return Err(format!("face {face} needs its body")),
                }
            }
            "edge" => Self::Edge {
                body: f.need("body", "an edge reference")?,
                edge: f.need("edge", "an edge reference")?,
            },
            "vertex" => Self::Vertex {
                body: f.need("body", "a vertex reference")?,
                vertex: f.need("vertex", "a vertex reference")?,
            },
            "body" => Self::Body(f.need("body", "a body reference")?),
            "sketch" | "sketch_curve" => {
                if f.has("direction") {
                    return Err("a direction belongs to the feature (\"direction\")".to_owned());
                }
                let sketch = f.need("sketch", "a sketch reference")?;
                let curves = match f.curve("curve")? {
                    Some(curve) => vec![curve],
                    None => f.curves("curves")?.unwrap_or_default(),
                };
                Self::SketchCurves { sketch, curves }
            }
            "plane" => Self::FixedPlane {
                origin: f.need("origin", "a fixed plane")?,
                normal: f.need("normal", "a fixed plane")?,
                x_axis: f.take("x_axis")?,
            },
            "fixed" => {
                if f.has("normal") {
                    Self::FixedPlane {
                        origin: f.need("origin", "a fixed plane")?,
                        normal: f.need("normal", "a fixed plane")?,
                        x_axis: f.take("x_axis")?,
                    }
                } else if f.has("direction") {
                    Self::FixedAxis {
                        origin: f.need("origin", "a fixed axis")?,
                        direction: f.need("direction", "a fixed axis")?,
                    }
                } else {
                    Self::FixedPoint(f.need("point", "a fixed point")?)
                }
            }
            other => return Err(format!("unknown reference type “{other}”")),
        };
        f.done(reference)
    }
}

impl PathRef {
    /// Reads the JSON form, or F4's `sketch_curve` and `line`.
    pub fn from_value(value: Value) -> Result<Self, String> {
        let Value::Object(map) = value else {
            return Err(format!(
                "invalid path {value}: expected {{\"body\": ..., \"edges\": [...]}}, \
                 {{\"sketch\": ..., \"curve\": ...}}, {{\"start\": [...], \"end\": [...]}}, \
                 {{\"helix\": {{...}}}}, {{\"fixed_curves\": [...]}} or {{\"feature\": ...}}"
            ));
        };
        let mut f = Fields(map);
        let tag = f.take::<String>("type")?;
        let path = match tag.as_deref() {
            Some("sketch_curve") => Self::sketch_curve(&mut f)?,
            Some("line") => Self::line(&mut f)?,
            Some(other) => return Err(format!("unknown path type “{other}”")),
            None if f.has("edges") => Self::Edges {
                body: f.need("body", "a path of edges")?,
                edges: f.need("edges", "a path of edges")?,
            },
            None if f.has("sketch") && f.has("curves") => Self::SketchCurves {
                sketch: f.need("sketch", "a sketch path")?,
                curves: f.curves("curves")?.unwrap_or_default(),
            },
            None if f.has("sketch") => Self::sketch_curve(&mut f)?,
            None if f.has("helix") => Self::Helix(f.need("helix", "a helix path")?),
            None if f.has("fixed_curves") => {
                Self::Fixed(f.need("fixed_curves", "a path of fixed curves")?)
            }
            // An Intersection Curve (mitcad#1405).
            None if f.has("feature") => {
                Self::FeatureCurve(f.need("feature", "a path of an intersection curve")?)
            }
            None => Self::line(&mut f)?,
        };
        f.done(path)
    }

    fn sketch_curve(f: &mut Fields) -> Result<Self, String> {
        Ok(Self::SketchCurve {
            sketch: f.need("sketch", "a sketch path")?,
            curve: f.curve("curve")?.ok_or("a sketch path needs \"curve\"")?,
        })
    }

    fn line(f: &mut Fields) -> Result<Self, String> {
        Ok(Self::Line {
            start: f.need("start", "a line path")?,
            end: f.need("end", "a line path")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(value: Value) -> GeomRef {
        serde_json::from_value(value).unwrap()
    }

    fn written(reference: &GeomRef) -> Value {
        serde_json::to_value(reference).unwrap()
    }

    #[test]
    fn references_round_trip_through_their_json_forms() {
        for value in [
            json!("xy"),
            json!("z"),
            json!("origin"),
            json!("F5"),
            json!({"body": "F2.b0", "face": "F2:end(r{c1})"}),
            json!({"body": "F2.b0", "edge": "E{F2:a|F2:b}"}),
            json!({"body": "F2.b0"}),
            json!({"sketch": "F1", "curve": "c4"}),
            json!({"sketch": "F1", "curves": ["c1", "c3"]}),
            json!({"sketch": "F1"}),
            json!({"sketch": "F1", "point": "p3"}),
            json!({"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]}),
            json!({"origin": [15.0, 0.0, 0.0], "normal": [1.0, 0.0, 0.0], "x_axis": [0.0, 1.0, 0.0]}),
            json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, 1.0, 0.0]}),
            json!({"point": [1.0, 2.0, 3.0]}),
        ] {
            assert_eq!(written(&read(value.clone())), value);
        }
    }

    #[test]
    fn earlier_tagged_forms_are_read_as_the_unified_ones() {
        for (old, new) in [
            (json!({"type": "origin", "plane": "yz"}), json!("yz")),
            (json!({"type": "origin", "axis": "x"}), json!("x")),
            (json!({"type": "origin"}), json!("origin")),
            (json!({"type": "origin_plane", "plane": "xz"}), json!("xz")),
            (
                json!({"type": "construction", "feature": "F7"}),
                json!("F7"),
            ),
            (
                json!({"type": "face", "body": "F2.b0", "face": "F2:end(r{c1})"}),
                json!({"body": "F2.b0", "face": "F2:end(r{c1})"}),
            ),
            (
                json!({"type": "vertex", "body": "F2.b0", "vertex": "V{F2:a|F2:b|F2:c}"}),
                json!({"body": "F2.b0", "vertex": "V{F2:a|F2:b|F2:c}"}),
            ),
            (
                json!({"type": "body", "body": "F4.b0"}),
                json!({"body": "F4.b0"}),
            ),
            (
                json!({"type": "sketch", "sketch": "F6", "curves": ["c1"]}),
                json!({"sketch": "F6", "curve": "c1"}),
            ),
            (
                json!({"type": "sketch", "sketch": "F6"}),
                json!({"sketch": "F6"}),
            ),
            (
                json!({"type": "plane", "origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]}),
                json!({"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]}),
            ),
            (
                json!({"type": "fixed", "origin": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0]}),
                json!({"origin": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0]}),
            ),
            (
                json!({"type": "fixed", "point": [50.0, 0.0, 0.0]}),
                json!({"point": [50.0, 0.0, 0.0]}),
            ),
        ] {
            assert_eq!(written(&read(old)), new);
        }
        let path: PathRef =
            serde_json::from_value(json!({"type": "sketch_curve", "sketch": "F9", "curve": "c1"}))
                .unwrap();
        assert_eq!(
            serde_json::to_value(&path).unwrap(),
            json!({"sketch": "F9", "curve": "c1"})
        );
        let line: PathRef = serde_json::from_value(
            json!({"type": "line", "start": [0.0, 0.0, 0.0], "end": [1.0, 0.0, 0.0]}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&line).unwrap(),
            json!({"start": [0.0, 0.0, 0.0], "end": [1.0, 0.0, 0.0]})
        );
    }

    #[test]
    fn malformed_references_are_explained() {
        for (value, expected) in [
            (json!("XY"), "invalid reference “XY”"),
            (json!(3), "invalid reference 3"),
            (
                json!({"body": "F2.b0", "face": "F2:a", "edge": "y"}),
                "invalid name y",
            ),
            (
                json!({"body": "F2.b0", "face": "F2:a", "edge": "E{F2:a|F2:b}"}),
                "names one \"face\", \"edge\" or \"vertex\"",
            ),
            (
                json!({"type": "fixed", "point": [0, 0, 0], "x": 1}),
                "unknown field `x`",
            ),
            (json!({"type": "face", "face": "F2:a"}), "needs its body"),
            (
                json!({"type": "origin_plane", "plane": "yz", "offset": 5}),
                "belongs to the feature",
            ),
            (
                json!({"type": "origin", "plane": "x"}),
                "unknown origin plane “x”",
            ),
            (json!({"type": "warp"}), "unknown reference type “warp”"),
        ] {
            let error = serde_json::from_value::<GeomRef>(value).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn a_path_of_several_sketch_curves_round_trips() {
        let value = json!({"sketch": "F3", "curves": ["c2", "c1", "c4"]});
        let path: PathRef = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&path, PathRef::SketchCurves { curves, .. } if curves.len() == 3));
        assert_eq!(serde_json::to_value(&path).unwrap(), value);
    }

    #[test]
    fn a_helix_path_round_trips_and_gives_points_along_it() {
        // (mitcad#385) Two turns of radius 10 about z, rising 5 a turn.
        let value = json!({"helix": {"center": [0.0, 0.0, 0.0], "axis": [0.0, 0.0, 1.0],
            "x_axis": [1.0, 0.0, 0.0], "radius": 10.0, "pitch": 5.0,
            "start_angle": 0.0, "end_angle": 4.0 * std::f64::consts::PI}});
        let path: PathRef = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(&path, PathRef::Helix(h) if h.radius == 10.0));
        assert_eq!(serde_json::to_value(&path).unwrap(), value);
        let curve = PathCurve::helix(match &path {
            PathRef::Helix(h) => h,
            _ => unreachable!(),
        })
        .unwrap();
        let tau = std::f64::consts::TAU;
        let speed = 10f64.hypot(5.0 / tau);
        assert!((curve.length() - 2.0 * tau * speed).abs() < 1e-9);
        // Half way: a turn round, 5 up; its tangent along y and rising.
        let half = curve.path_point(PathParameter::Fraction(0.5));
        assert!(norm(sub(half.point, [10.0, 0.0, 5.0])) < 1e-9, "{half:?}");
        let rise = 5.0 / tau / speed;
        assert!(norm(sub(half.tangent, [0.0, 10.0 / speed, rise])) < 1e-9);
        // The nearest point to one beside the first turn's quarter.
        let near = curve.path_point(PathParameter::Near([0.0, 12.0, 1.25]));
        assert!(norm(sub(near.point, [0.0, 10.0, 1.25])) < 1e-6, "{near:?}");
        // Unknown fields and a helix of no pitch are refused.
        let bad = json!({"helix": {"center": [0.0, 0.0, 0.0], "axis": [0.0, 0.0, 1.0],
            "x_axis": [1.0, 0.0, 0.0], "radius": 10.0, "pitch": 5.0,
            "start_angle": 0.0, "end_angle": 1.0, "turns": 2}});
        assert!(serde_json::from_value::<PathRef>(bad).is_err());
    }

    #[test]
    fn a_path_of_fixed_curves_round_trips_and_is_checked() {
        // (mitcad#548) A line, then a quadratic spline on from its end.
        let value = json!({"fixed_curves": [
            {"type": "line", "start": [0.0, 0.0, 0.0], "end": [10.0, 0.0, 0.0]},
            {"type": "spline", "degree": 2,
             "poles": [[10.0, 0.0, 0.0], [15.0, 0.0, 0.0], [15.0, 5.0, 5.0]],
             "knots": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]}]});
        let path: PathRef = serde_json::from_value(value.clone()).unwrap();
        let PathRef::Fixed(curves) = &path else {
            panic!("not fixed curves: {path:?}");
        };
        assert_eq!(curves.len(), 2);
        assert!(curves.iter().all(|c| c.check().is_ok()));
        assert_eq!(serde_json::to_value(&path).unwrap(), value);
        // The kernel curve and back.
        let spline = curves[1].curve();
        assert!(matches!(
            &spline,
            crate::kernel::Curve3::BSpline { degree: 2, .. }
        ));
        assert_eq!(FixedCurve::of(&spline).as_ref(), Some(&curves[1]));
        // A spline short of knots, a conic of no radius.
        let short = FixedCurve::Spline {
            degree: 2,
            poles: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 1.0, 1.0],
        };
        assert!(short.check().unwrap_err().contains("needs 6 knots"));
        let flat = FixedCurve::Conic {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 0.0,
            minor: 0.0,
            start: 0.0,
            end: 1.0,
            closed: false,
        };
        assert!(flat.check().is_err());
    }

    #[test]
    fn chains_of_curves_give_points_along_their_whole_length() {
        let line = |a: Vec3, b: Vec3| PathCurve::line(a, b).unwrap();
        let chain = [
            line([0.0; 3], [10.0, 0.0, 0.0]),
            line([10.0, 0.0, 0.0], [10.0, 20.0, 0.0]),
        ];
        let half = chain_point(&chain, PathParameter::Fraction(0.5));
        assert_eq!(half.point, [10.0, 5.0, 0.0]);
        assert_eq!(half.tangent, [0.0, 1.0, 0.0]);
        // A corner takes the tangent of the curve before it.
        let corner = chain_point(&chain, PathParameter::Length(10.0));
        assert_eq!(corner.point, [10.0, 0.0, 0.0]);
        assert_eq!(corner.tangent, [1.0, 0.0, 0.0]);
        let beyond = chain_point(&chain, PathParameter::Length(35.0));
        assert_eq!(beyond.point, [10.0, 25.0, 0.0]);
        let before = chain_point(&chain, PathParameter::Length(-4.0));
        assert_eq!(before.point, [-4.0, 0.0, 0.0]);
        let near = chain_point(&chain, PathParameter::Near([12.0, 18.0, 5.0]));
        assert_eq!(near.point, [10.0, 18.0, 0.0]);
    }

    #[test]
    fn closed_chains_go_round_again() {
        // mitcad#673: a square 10 a side, 40 long, closed where it began.
        let line = |a: Vec3, b: Vec3| PathCurve::line(a, b).unwrap();
        let corners = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
        ];
        let pieces: Vec<PathCurve> = (0..4)
            .map(|i| line(corners[i], corners[(i + 1) % 4]))
            .collect();
        let square = PathChain {
            pieces: pieces.clone(),
            normal: [0.0, 0.0, 1.0],
        };
        assert!(square.closed());
        assert_eq!(square.at(45.0).0, [5.0, 0.0, 0.0]);
        assert_eq!(square.at(-5.0).0, [0.0, 5.0, 0.0]);
        assert_eq!(square.at(25.0), square.at(65.0));
        // The point nearest a point, by arc length.
        assert_eq!(square.nearest([3.0, -2.0, 4.0]), 3.0);
        assert_eq!(square.nearest([-1.0, 7.0, 0.0]), 33.0);
        // Open, it goes on straight past its ends.
        let open = PathChain {
            pieces: pieces[..3].to_vec(),
            normal: [0.0, 0.0, 1.0],
        };
        assert!(!open.closed());
        assert_eq!(open.at(45.0).0, [-15.0, 10.0, 0.0]);
    }

    #[test]
    fn line_and_arc_paths_continue_straight_past_their_ends() {
        let line = PathCurve::line([0.0; 3], [10.0, 0.0, 0.0]).unwrap();
        let p = line.path_point(PathParameter::Fraction(1.5));
        assert_eq!(p.point, [15.0, 0.0, 0.0]);
        assert_eq!(
            line.path_point(PathParameter::Near([4.0, 3.0, 0.0])).point,
            [4.0, 0.0, 0.0]
        );
        let quarter = PathCurve::Arc {
            center: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            radius: 10.0,
            sweep: std::f64::consts::FRAC_PI_2,
        };
        let end = quarter.path_point(PathParameter::Fraction(1.0));
        assert!(norm(sub(end.point, [0.0, 10.0, 0.0])) < 1e-12);
        assert!(norm(sub(end.tangent, [-1.0, 0.0, 0.0])) < 1e-12);
        let beyond = quarter.path_point(PathParameter::Length(quarter.length() + 5.0));
        assert!(norm(sub(beyond.point, [-5.0, 10.0, 0.0])) < 1e-12);
        let near = quarter.path_point(PathParameter::Near([20.0, 20.0, 0.0]));
        let half = std::f64::consts::FRAC_1_SQRT_2 * 10.0;
        assert!(norm(sub(near.point, [half, half, 0.0])) < 1e-9);
    }
}
