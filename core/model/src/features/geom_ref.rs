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
    CurveGeometry, Datum, DatumAxis, DatumKind, DatumPlane, LINEAR, OriginDatum, PathParameter,
    PathPoint, SurfaceGeometry, SurfacePoint, Vec3, add, canonical_axis, norm, scale, sub, unit,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::profile::{SegmentGeometry, dot};
use crate::recompute::Read;
use crate::sketch::geometry::Curve2;
use crate::topo::{EdgeName, FaceName, VertexName};

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
}

/// A path: connected edges of one body in order (the path runs from the
/// free end of the first edge), a sketch line, arc or circle in its own
/// direction (a circle starts on its sketch x axis and runs
/// counter-clockwise), or a fixed straight line.
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
    /// A line, or an axis of a circular edge or a face of revolution.
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
        }
    }

    fn datum(self) -> Option<DatumKind> {
        match self {
            Self::Plane | Self::Tool | Self::Surface => Some(DatumKind::Plane),
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
            ),
            G::Edge { .. } => matches!(
                self,
                Self::Line | Self::Axis | Self::Point | Self::EdgeOrFace
            ),
            G::Vertex { .. } | G::SketchPoint { .. } | G::FixedPoint(_) => self == Self::Point,
            G::Body(_) => matches!(self, Self::Tool | Self::Surface),
            G::SketchCurves { .. } => matches!(self, Self::Line | Self::Axis | Self::Tool),
            G::FixedPlane { .. } => matches!(self, Self::Plane | Self::Tool | Self::Surface),
            G::FixedAxis { .. } => matches!(self, Self::Line | Self::Axis),
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
        if line
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
            Self::SketchCurve { sketch, .. } => {
                references.features.insert(*sketch);
            }
            Self::Line { .. } => {}
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
            Self::Line { start, end } => {
                finite("path", &[*start, *end].concat())?;
                if norm(sub(*end, *start)) > LINEAR {
                    Ok(())
                } else {
                    Err("the path has no length".to_owned())
                }
            }
        }
    }
}

// Resolution.

/// Where references resolve: a feature being evaluated, which records what
/// it reads for the recompute cache, or the document at the marker.
pub(crate) trait Resolver {
    type Kernel: Kernel;

    fn kernel(&self) -> &Self::Kernel;
    /// The display name of a feature, for messages.
    fn name(&self, uid: FeatureUid) -> String;
    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String>;
    fn shape(&mut self, body: BodyUid) -> Result<<Self::Kernel as Kernel>::Shape, String>;
    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String>;
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
    r.kernel()
        .face_geometry(&shape, face)
        .map_err(|e| format!("face {face}: {e}"))
}

fn curve_of(
    r: &mut impl Resolver,
    body: BodyUid,
    edge: &EdgeName,
) -> Result<CurveGeometry, String> {
    let shape = r.shape(body)?;
    r.kernel()
        .edge_geometry(&shape, edge)
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
            return r
                .kernel()
                .vertex_point(&shape, vertex)
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
        return Err(format!("{reference} is not a face"));
    };
    let shape = r.shape(*body)?;
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
            r.kernel()
                .path_point(&shape, edges, at)
                .map_err(|e| format!("path: {e}"))
        }
        _ => Ok(path_curve(r, path)?.path_point(at)),
    }
}

/// The curve of a sketch-curve or line path.
pub(crate) fn path_curve(r: &mut impl Resolver, path: &PathRef) -> Result<PathCurve, String> {
    let (sketch, curve) = match path {
        PathRef::Line { start, end } => return PathCurve::line(*start, *end),
        PathRef::SketchCurve { sketch, curve } => (*sketch, *curve),
        PathRef::Edges { .. } => {
            return Err("a path of edges has no analytic curve".to_owned());
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
        SegmentGeometry::Circle { center, radius } => {
            Ok(arc(center, radius, 0.0, std::f64::consts::TAU))
        }
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
}

impl PathCurve {
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
    fn nearest(&self, p: Vec3) -> f64 {
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
        EvalContext::datum(self, uid)
    }

    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        self.body(body)
    }

    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        EvalContext::sketch(self, uid)
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
                        a fixed plane, axis or point";

fn vec3(v: &Vec3) -> Value {
    Value::from(v.to_vec())
}

impl Serialize for GeomRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
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
        .ok_or_else(|| format!("unknown origin {kind} '{id}'"))
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
                    Err(format!("invalid reference '{text}': expected {EXPECTED}"))
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
                    point: crate::sketch::point_serde::parse(&point).map_err(|e| e.to_string())?,
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
            other => return Err(format!("unknown reference type '{other}'")),
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
                 {{\"sketch\": ..., \"curve\": ...}} or {{\"start\": [...], \"end\": [...]}}"
            ));
        };
        let mut f = Fields(map);
        let tag = f.take::<String>("type")?;
        let path = match tag.as_deref() {
            Some("sketch_curve") => Self::sketch_curve(&mut f)?,
            Some("line") => Self::line(&mut f)?,
            Some(other) => return Err(format!("unknown path type '{other}'")),
            None if f.has("edges") => Self::Edges {
                body: f.need("body", "a path of edges")?,
                edges: f.need("edges", "a path of edges")?,
            },
            None if f.has("sketch") => Self::sketch_curve(&mut f)?,
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
            (json!("XY"), "invalid reference 'XY'"),
            (json!(3), "invalid reference 3"),
            (
                json!({"body": "F2.b0", "face": "F2:a", "edge": "y"}),
                "invalid name 'y'",
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
                "unknown origin plane 'x'",
            ),
            (json!({"type": "warp"}), "unknown reference type 'warp'"),
        ] {
            let error = serde_json::from_value::<GeomRef>(value).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
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
