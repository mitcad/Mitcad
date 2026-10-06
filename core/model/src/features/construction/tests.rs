// SPDX-License-Identifier: MIT
//! Construction planes, axes and points against analytic expectations,
//! through the JSON commands, with a kernel that knows the geometry of
//! extruded rectangles and circles (`GeoKernel`).

use std::sync::Arc;

use serde_json::{Value, json};

use crate::analysis::PhysicalProperties;
use crate::datum::{
    CurveGeometry, PathParameter, PathPoint, SurfaceGeometry, SurfacePoint, Vec3, add, norm, scale,
    sub, unit,
};
use crate::document::Document;
use crate::ids::FeatureUid;
use crate::kernel::{
    BooleanOp, BooleanOutput, BooleanPiece, BoundingBox, ExtrudeSpec, FilletSet, Kernel,
    KernelError,
};
use crate::profile::{ProfileRegion, SegmentGeometry, SketchFrame, cross, dot};
use crate::testing::{MockKernel, MockShape};
use crate::topo::{EdgeName, FaceName, RoleKey, SegmentKey, VertexName};

/// An extrusion as the kernel received it.
#[derive(Debug)]
struct Prism {
    feature: FeatureUid,
    frame: SketchFrame,
    regions: Vec<ProfileRegion>,
    direction: Vec3,
    start: f64,
    end: f64,
}

impl Prism {
    fn at(&self, uv: [f64; 2], offset: f64) -> Vec3 {
        add(self.frame.point(uv), scale(self.direction, offset))
    }

    fn segment(&self, key: &SegmentKey) -> Option<&SegmentGeometry> {
        self.regions
            .iter()
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.segments)
            .find(|s| &s.key == key)
            .map(|s| &s.geometry)
    }

    /// The offset of a cap face.
    fn cap(&self, face: &FaceName) -> Option<f64> {
        match face.role.as_str() {
            "start" => Some(self.start),
            "end" => Some(self.end),
            _ => None,
        }
    }

    fn side(&self, face: &FaceName) -> Option<&SegmentGeometry> {
        match (&face.role[..], &face.key) {
            ("side", Some(RoleKey::Segment(key))) => self.segment(key),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GeoShape {
    mock: MockShape,
    prisms: Vec<Arc<Prism>>,
}

/// The mock kernel plus geometry: faces, edges and vertices of extruded
/// rectangles and circles (before booleans change them), and physical
/// properties from bounding boxes.
#[derive(Default)]
pub(crate) struct GeoKernel {
    mock: MockKernel,
}

fn failed(message: impl Into<String>) -> KernelError {
    KernelError::Failed(message.into())
}

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9
}

impl GeoKernel {
    fn prism<'a>(&self, shape: &'a GeoShape, face: &FaceName) -> Result<&'a Prism, KernelError> {
        if self.mock.count_faces(&shape.mock, face)? == 0 {
            return Err(failed(format!("the body has no face {face}")));
        }
        shape
            .prisms
            .iter()
            .find(|p| p.feature == face.feature)
            .map(|p| &**p)
            .ok_or_else(|| failed(format!("no geometry for {face}")))
    }

    /// The shared corner of two line sides, in sketch coordinates.
    fn corner(a: &SegmentGeometry, b: &SegmentGeometry) -> Option<[f64; 2]> {
        let (
            SegmentGeometry::Line { start: s1, end: e1 },
            SegmentGeometry::Line { start: s2, end: e2 },
        ) = (a, b)
        else {
            return None;
        };
        [*s1, *e1]
            .into_iter()
            .find(|p| close(*p, *s2) || close(*p, *e2))
    }
}

impl Kernel for GeoKernel {
    type Shape = GeoShape;

    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<GeoShape, KernelError> {
        let mock = self.mock.extrude(spec)?;
        Ok(GeoShape {
            mock,
            prisms: vec![Arc::new(Prism {
                feature: spec.feature,
                frame: spec.frame,
                regions: spec.regions.to_vec(),
                direction: spec.direction,
                start: spec.start,
                end: spec.end,
            })],
        })
    }

    fn solids(&self, shape: &GeoShape) -> Result<Vec<GeoShape>, KernelError> {
        Ok(self
            .mock
            .solids(&shape.mock)?
            .into_iter()
            .map(|mock| GeoShape {
                mock,
                prisms: shape.prisms.clone(),
            })
            .collect())
    }

    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&GeoShape],
        tool: &GeoShape,
    ) -> Result<BooleanOutput<GeoShape>, KernelError> {
        let mocks: Vec<&MockShape> = targets.iter().map(|t| &t.mock).collect();
        let output = self.mock.boolean(op, &mocks, &tool.mock)?;
        Ok(BooleanOutput {
            pieces: output
                .pieces
                .into_iter()
                .map(|piece| {
                    let mut prisms: Vec<Arc<Prism>> = piece
                        .sources
                        .iter()
                        .flat_map(|i| targets[*i].prisms.clone())
                        .collect();
                    prisms.extend(tool.prisms.clone());
                    BooleanPiece {
                        shape: GeoShape {
                            mock: piece.shape,
                            prisms,
                        },
                        sources: piece.sources,
                    }
                })
                .collect(),
            touched: output.touched,
        })
    }

    fn fillet(
        &self,
        feature: FeatureUid,
        body: &GeoShape,
        sets: &[FilletSet<'_>],
        rolling_ball_corners: bool,
    ) -> Result<GeoShape, KernelError> {
        Ok(GeoShape {
            mock: self
                .mock
                .fillet(feature, &body.mock, sets, rolling_ball_corners)?,
            prisms: body.prisms.clone(),
        })
    }

    fn count_edges(&self, shape: &GeoShape, edge: &EdgeName) -> usize {
        self.mock.count_edges(&shape.mock, edge)
    }

    fn count_faces(&self, shape: &GeoShape, face: &FaceName) -> Result<usize, KernelError> {
        self.mock.count_faces(&shape.mock, face)
    }

    fn bounding_box(&self, shape: &GeoShape) -> Result<Option<BoundingBox>, KernelError> {
        self.mock.bounding_box(&shape.mock)
    }

    fn face_geometry(
        &self,
        shape: &GeoShape,
        face: &FaceName,
    ) -> Result<SurfaceGeometry, KernelError> {
        let prism = self.prism(shape, face)?;
        let plane = |point: Vec3, normal: Vec3| {
            let normal = unit(normal).expect("a normal");
            SurfaceGeometry::Plane {
                origin: scale(normal, dot(point, normal)),
                normal,
            }
        };
        if let Some(offset) = prism.cap(face) {
            let normal = if face.role == "end" {
                prism.direction
            } else {
                scale(prism.direction, -1.0)
            };
            return Ok(plane(prism.at([0.0, 0.0], offset), normal));
        }
        match prism.side(face) {
            // Loops run counter-clockwise about the sketch normal.
            Some(SegmentGeometry::Line { start, end }) => {
                let along = sub(prism.frame.point(*end), prism.frame.point(*start));
                Ok(plane(
                    prism.frame.point(*start),
                    cross(along, prism.frame.normal()),
                ))
            }
            Some(SegmentGeometry::Circle { center, radius }) => Ok(SurfaceGeometry::Cylinder {
                origin: prism.at(*center, (prism.start + prism.end) / 2.0),
                axis: prism.frame.normal(),
                radius: *radius,
            }),
            _ => Ok(SurfaceGeometry::Other {
                kind: "other".to_owned(),
            }),
        }
    }

    fn edge_geometry(
        &self,
        shape: &GeoShape,
        edge: &EdgeName,
    ) -> Result<CurveGeometry, KernelError> {
        match self.mock.count_edges(&shape.mock, edge) {
            0 => return Err(failed(format!("the body has no edge {edge}"))),
            1 => {}
            n => return Err(failed(format!("{edge} names {n} edges"))),
        }
        let [a, b] = edge.faces();
        let prism = self.prism(shape, a)?;
        let (cap, side) = match (prism.cap(a), prism.cap(b)) {
            (Some(offset), None) => (Some(offset), b),
            (None, Some(offset)) => (Some(offset), a),
            _ => (None, a),
        };
        let geometry = prism
            .side(side)
            .ok_or_else(|| failed(format!("no geometry for {edge}")))?;
        match (cap, geometry) {
            (Some(offset), SegmentGeometry::Line { start, end }) => Ok(CurveGeometry::Line {
                start: prism.at(*start, offset),
                end: prism.at(*end, offset),
            }),
            (Some(offset), SegmentGeometry::Circle { center, radius }) => {
                let start = prism.at([center[0] + radius, center[1]], offset);
                Ok(CurveGeometry::Circle {
                    center: prism.at(*center, offset),
                    normal: prism.frame.normal(),
                    radius: *radius,
                    start,
                    end: start,
                })
            }
            (None, SegmentGeometry::Circle { center, radius }) => {
                // The seam of the cylinder.
                let at = [center[0] + radius, center[1]];
                Ok(CurveGeometry::Line {
                    start: prism.at(at, prism.start),
                    end: prism.at(at, prism.end),
                })
            }
            (None, first) => {
                let second = prism.side(b).expect("a side");
                let corner = Self::corner(first, second)
                    .ok_or_else(|| failed(format!("{edge} has no corner")))?;
                Ok(CurveGeometry::Line {
                    start: prism.at(corner, prism.start),
                    end: prism.at(corner, prism.end),
                })
            }
            _ => Err(failed(format!("no geometry for {edge}"))),
        }
    }

    fn vertex_point(&self, shape: &GeoShape, vertex: &VertexName) -> Result<Vec3, KernelError> {
        let faces = vertex.faces();
        let prism = self.prism(shape, &faces[0])?;
        let offset = faces
            .iter()
            .find_map(|f| prism.cap(f))
            .ok_or_else(|| failed(format!("{vertex} has no cap")))?;
        let sides: Vec<&SegmentGeometry> = faces.iter().filter_map(|f| prism.side(f)).collect();
        let [first, second] = sides[..] else {
            return Err(failed(format!("{vertex} is not a corner")));
        };
        let corner =
            Self::corner(first, second).ok_or_else(|| failed(format!("{vertex} has no corner")))?;
        Ok(prism.at(corner, offset))
    }

    fn path_point(
        &self,
        shape: &GeoShape,
        edges: &[EdgeName],
        at: PathParameter,
    ) -> Result<PathPoint, KernelError> {
        let [edge] = edges else {
            return Err(failed("paths of one edge only"));
        };
        match self.edge_geometry(shape, edge)? {
            CurveGeometry::Line { start, end } => {
                let length = norm(sub(end, start));
                let tangent = unit(sub(end, start)).expect("a line");
                let along = match at {
                    PathParameter::Fraction(f) => f * length,
                    PathParameter::Length(l) => l,
                    PathParameter::Near(p) => dot(sub(p, start), tangent).clamp(0.0, length),
                };
                Ok(PathPoint {
                    point: add(start, scale(tangent, along)),
                    tangent,
                })
            }
            CurveGeometry::Circle {
                center,
                normal,
                radius,
                start,
                ..
            } => {
                let x = unit(sub(start, center)).expect("a circle");
                let y = cross(normal, x);
                let angle = match at {
                    PathParameter::Fraction(f) => f * std::f64::consts::TAU,
                    PathParameter::Length(l) => l / radius,
                    PathParameter::Near(p) => {
                        let v = sub(p, center);
                        dot(v, y).atan2(dot(v, x))
                    }
                };
                let (sin, cos) = angle.sin_cos();
                Ok(PathPoint {
                    point: add(center, scale(add(scale(x, cos), scale(y, sin)), radius)),
                    tangent: add(scale(x, -sin), scale(y, cos)),
                })
            }
            CurveGeometry::Other { .. } => Err(failed("no geometry")),
        }
    }

    fn face_point_normal(
        &self,
        shape: &GeoShape,
        face: &FaceName,
        near: Vec3,
    ) -> Result<SurfacePoint, KernelError> {
        match self.face_geometry(shape, face)? {
            SurfaceGeometry::Plane { origin, normal, .. } => Ok(SurfacePoint {
                point: sub(near, scale(normal, dot(sub(near, origin), normal))),
                normal,
            }),
            SurfaceGeometry::Cylinder {
                origin,
                axis,
                radius,
            } => {
                let on_axis = add(origin, scale(axis, dot(sub(near, origin), axis)));
                let out = unit(sub(near, on_axis)).ok_or_else(|| failed("on the axis"))?;
                Ok(SurfacePoint {
                    point: add(on_axis, scale(out, radius)),
                    normal: out,
                })
            }
            _ => Err(failed("no geometry")),
        }
    }

    fn physical_properties(
        &self,
        shape: &GeoShape,
        density: f64,
    ) -> Result<PhysicalProperties, KernelError> {
        let bounds = shape.mock.bounds.ok_or_else(|| failed("no bounds"))?;
        let size: Vec3 = std::array::from_fn(|i| bounds.max[i] - bounds.min[i]);
        let volume = size[0] * size[1] * size[2];
        // g/cm³ × mm³: 1 cm³ = 1000 mm³, 1 kg = 1000 g.
        let mass = volume * density * 1e-6;
        Ok(PhysicalProperties {
            volume,
            area: 2.0 * (size[0] * size[1] + size[1] * size[2] + size[0] * size[2]),
            mass,
            center_of_mass: std::array::from_fn(|i| (bounds.max[i] + bounds.min[i]) / 2.0),
            inertia: [[mass, 0.0, 0.0], [0.0, mass, 0.0], [0.0, 0.0, mass]],
            principal_moments: [mass; 3],
            principal_axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        })
    }
}

// Commands and queries.

pub(crate) fn command(doc: &mut Document<GeoKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn command_error(doc: &mut Document<GeoKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} succeeded: {r}"))
        .unwrap_err()
        .to_string()
}

pub(crate) fn query(doc: &Document<GeoKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

/// Adds a construction feature; returns its uid and its feature query.
fn add_construction(
    doc: &mut Document<GeoKernel>,
    kind: &str,
    definition: &Value,
) -> (String, Value) {
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": kind, "definition": definition}}),
    );
    let uid = added["uid"].as_str().unwrap().to_owned();
    let feature = query(doc, json!({"query": "feature", "uid": uid}));
    (uid, feature)
}

/// Adds a construction feature that evaluates; returns its uid.
fn construct(doc: &mut Document<GeoKernel>, kind: &str, definition: Value) -> String {
    let (uid, feature) = add_construction(doc, kind, &definition);
    assert_eq!(
        feature["status"], "ok",
        "{definition}: {}",
        feature["error"]
    );
    uid
}

fn plane(doc: &mut Document<GeoKernel>, definition: Value) -> Value {
    let uid = construct(doc, "construction_plane", definition);
    query(doc, json!({"query": "datum", "uid": uid}))
}

fn axis(doc: &mut Document<GeoKernel>, definition: Value) -> Value {
    let uid = construct(doc, "construction_axis", definition);
    query(doc, json!({"query": "datum", "uid": uid}))
}

fn point(doc: &mut Document<GeoKernel>, definition: Value) -> Value {
    let uid = construct(doc, "construction_point", definition);
    query(doc, json!({"query": "datum", "uid": uid}))
}

/// The error a construction feature fails with.
fn failure(doc: &mut Document<GeoKernel>, kind: &str, definition: Value) -> String {
    let (_, feature) = add_construction(doc, kind, &definition);
    feature["error"]
        .as_str()
        .unwrap_or_else(|| panic!("{definition} did not fail"))
        .to_owned()
}

#[track_caller]
fn assert_near(value: &Value, expected: Vec3) {
    let actual: Vec3 = std::array::from_fn(|i| value[i].as_f64().expect("a number"));
    assert!(
        norm(sub(actual, expected)) < 1e-9,
        "expected {expected:?}, got {actual:?}"
    );
}

const REGION: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const FRONT: &str = "c1[c4,c2]";
const RIGHT: &str = "c2[c1,c3]";
const BACK: &str = "c3[c2,c4]";
const LEFT: &str = "c4[c3,c1]";

fn side(segment: &str) -> String {
    format!("F2:side({segment})")
}

fn top() -> String {
    format!("F2:end({REGION})")
}

fn bottom() -> String {
    format!("F2:start({REGION})")
}

fn face(name: &str) -> Value {
    json!({"body": "F2.b0", "face": name})
}

fn edge(a: &str, b: &str) -> Value {
    let name = EdgeName::new(a.parse().unwrap(), b.parse().unwrap());
    json!({"body": "F2.b0", "edge": name.to_string()})
}

fn vertex(faces: [&str; 3]) -> Value {
    let name = VertexName::new(faces.map(|f| f.parse::<FaceName>().unwrap())).unwrap();
    json!({"body": "F2.b0", "vertex": name.to_string()})
}

/// Sketch1 with a 60 x 40 rectangle at the origin, extruded 20 mm (d3):
/// the block x 0..60, y 0..40, z 0..20 (body F2.b0).
fn block() -> Document<GeoKernel> {
    let mut doc = Document::new(GeoKernel::default());
    command(
        &mut doc,
        json!([{"cmd": "sketch.create"},
               {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
               {"cmd": "add_feature", "def": {"type": "extrude",
                "profiles": [{"sketch": "F1", "region": REGION}],
                "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}]),
    );
    doc
}

/// A 20 mm diameter circle at the origin extruded 40 mm (body F2.b0, side
/// face F2:side(c1)).
fn cylinder() -> Document<GeoKernel> {
    let mut doc = Document::new(GeoKernel::default());
    command(
        &mut doc,
        json!([{"cmd": "sketch.create"},
               {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20},
               {"cmd": "add_feature", "def": {"type": "extrude",
                "profiles": [{"sketch": "F1", "region": "r{c1}"}],
                "extent": {"type": "distance", "distance": 40}, "operation": "new_body"}}]),
    );
    doc
}

// Planes.

#[test]
fn offset_planes_follow_their_reference_and_parameters() {
    let mut doc = block();
    let above = plane(
        &mut doc,
        json!({"type": "offset", "plane": face(&top()), "distance": 10}),
    );
    assert_eq!(above["uid"], "F3");
    assert_eq!(above["name"], "Plane1");
    assert_near(&above["origin"], [0.0, 0.0, 30.0]);
    assert_near(&above["normal"], [0.0, 0.0, 1.0]);
    // A negative offset from the front face (outward -Y) goes into the block.
    let inside = plane(
        &mut doc,
        json!({"type": "offset", "plane": face(&side(FRONT)), "distance": -5}),
    );
    assert_near(&inside["origin"], [0.0, 5.0, 0.0]);
    assert_near(&inside["normal"], [0.0, -1.0, 0.0]);
    // From XZ (x = +X, y = -Z, normal +Y) the frame moves with the plane.
    let xz = plane(
        &mut doc,
        json!({"type": "offset", "plane": "xz", "distance": 20}),
    );
    assert_near(&xz["origin"], [0.0, 20.0, 0.0]);
    assert_near(&xz["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&xz["y_axis"], [0.0, 0.0, -1.0]);
    // A plane from a plane.
    let stacked = plane(
        &mut doc,
        json!({"type": "offset", "plane": "F3", "distance": 2.5}),
    );
    assert_near(&stacked["origin"], [0.0, 0.0, 32.5]);

    // The extrude distance is d3, the offset d4: both move the planes, and
    // only the features that depend on a change evaluate again.
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d3", "value": 25}),
    );
    assert_eq!(changed["recomputed"], 4, "the extrude and the planes on it");
    assert_near(
        &query(&doc, json!({"query": "datum", "uid": "F3"}))["origin"],
        [0.0, 0.0, 35.0],
    );
    assert_near(
        &query(&doc, json!({"query": "datum", "uid": "F6"}))["origin"],
        [0.0, 0.0, 37.5],
    );
    let changed = command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "d4", "value": 1}),
    );
    assert_eq!(changed["recomputed"], 2, "Plane1 and the plane on it");
    assert_near(
        &query(&doc, json!({"query": "datum", "uid": "F6"}))["origin"],
        [0.0, 0.0, 28.5],
    );
    // Undo comes from the cache.
    let undone = command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(undone["recomputed"], 0);
    assert_near(
        &query(&doc, json!({"query": "datum", "uid": "F6"}))["origin"],
        [0.0, 0.0, 37.5],
    );
}

#[test]
fn planes_at_an_angle_turn_about_the_line() {
    let mut doc = block();
    // cons_plane_angle: through X, 30 degrees from XY.
    let tilted = plane(
        &mut doc,
        json!({"type": "angle", "line": "x", "angle": 30f64.to_radians(), "plane": "xy"}),
    );
    let (sin, cos) = 30f64.to_radians().sin_cos();
    assert_near(&tilted["normal"], [0.0, -sin, cos]);
    assert_near(&tilted["origin"], [0.0, 0.0, 0.0]);
    assert_near(&tilted["x_axis"], [1.0, 0.0, 0.0]);
    // About the block's vertical edge at (60, 40), 90 degrees from the
    // front face: the plane through the edge with the normal turned from
    // -Y about +Z to +X.
    let turned = plane(
        &mut doc,
        json!({"type": "angle", "line": edge(&side(RIGHT), &side(BACK)),
               "angle": 90f64.to_radians(), "plane": face(&side(FRONT))}),
    );
    assert_near(&turned["normal"], [1.0, 0.0, 0.0]);
    assert_near(&turned["origin"], [60.0, 40.0, 0.0]);
    let error = failure(
        &mut doc,
        "construction_plane",
        json!({"type": "angle", "line": "z", "angle": 0.5, "plane": "xy"}),
    );
    assert!(
        error.contains("at right angles to the reference plane"),
        "{error}"
    );
}

#[test]
fn tangent_planes_touch_the_cylinder() {
    let mut doc = cylinder();
    let cylinder = json!({"body": "F2.b0", "face": "F2:side(c1)"});
    // cons_plane_tangent: 0 degrees from XZ touches at +Y.
    let tangent = plane(
        &mut doc,
        json!({"type": "tangent", "face": cylinder, "angle": 0, "plane": "xz"}),
    );
    assert_near(&tangent["origin"], [0.0, 10.0, 0.0]);
    assert_near(&tangent["normal"], [0.0, 1.0, 0.0]);
    assert_near(&tangent["x_axis"], [1.0, 0.0, 0.0]);
    let turned = plane(
        &mut doc,
        json!({"type": "tangent", "face": cylinder, "angle": 90f64.to_radians(), "plane": "xz"}),
    );
    assert_near(&turned["origin"], [-10.0, 0.0, 0.0]);
    assert_near(&turned["normal"], [-1.0, 0.0, 0.0]);
    let error = failure(
        &mut doc,
        "construction_plane",
        json!({"type": "tangent", "face": {"body": "F2.b0", "face": "F2:end(r{c1})"},
               "angle": 0, "plane": "xz"}),
    );
    assert!(
        error.contains("cylindrical or conical face, not a plane face"),
        "{error}"
    );
}

#[test]
fn tangent_planes_of_cones_lean_with_the_surface() {
    let base = crate::datum::DatumPlane::from_normal([0.0; 3], [0.0, 1.0, 0.0], None);
    let half = 30f64.to_radians();
    let cone = SurfaceGeometry::Cone {
        origin: [0.0; 3],
        axis: [0.0, 0.0, 1.0],
        radius: 10.0,
        half_angle: half,
    };
    let plane = super::plane::tangent(&cone, &base, 0.0).unwrap();
    let normal = plane.normal();
    assert!(norm(sub(normal, [0.0, half.cos(), -half.sin()])) < 1e-12);
    // The generator through (0, 10, 0) lies in the plane.
    let generator = add([0.0, 10.0, 0.0], [0.0, half.sin(), half.cos()]);
    assert!(plane.distance_to([0.0, 10.0, 0.0]).abs() < 1e-12);
    assert!(plane.distance_to(generator).abs() < 1e-12);
}

#[test]
fn midplanes_lie_between_parallel_and_crossing_planes() {
    let mut doc = block();
    // cons_plane_midplane: between the faces x = 0 and x = 60.
    let middle = plane(
        &mut doc,
        json!({"type": "midplane", "plane1": face(&side(LEFT)), "plane2": face(&side(RIGHT))}),
    );
    assert_near(&middle["origin"], [30.0, 0.0, 0.0]);
    assert_near(&middle["normal"], [-1.0, 0.0, 0.0]);
    // Between the top and the right face: the 45 degree plane through
    // their edge, inside the block.
    let bisector = plane(
        &mut doc,
        json!({"type": "midplane", "plane1": face(&top()), "plane2": face(&side(RIGHT))}),
    );
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert_near(&bisector["normal"], [-s, 0.0, s]);
    assert_near(&bisector["origin"], [60.0, 0.0, 20.0]);
    let error = command_error(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "midplane", "plane1": "xy", "plane2": "xy"}}}),
    );
    assert!(error.contains("the origin XY is selected twice"), "{error}");
}

#[test]
fn planes_through_edges_and_points() {
    let mut doc = block();
    // Two edges of the top face meeting at (60, 0, 20).
    let top_face = plane(
        &mut doc,
        json!({"type": "two_edges", "line1": edge(&top(), &side(FRONT)),
               "line2": edge(&top(), &side(RIGHT))}),
    );
    assert_near(&top_face["normal"], [0.0, 0.0, 1.0]);
    assert_near(&top_face["origin"], [0.0, 0.0, 20.0]);
    assert_near(&top_face["x_axis"], [1.0, 0.0, 0.0]);
    // Two parallel edges: the front top and the back bottom edge.
    let diagonal = plane(
        &mut doc,
        json!({"type": "two_edges", "line1": edge(&top(), &side(FRONT)),
               "line2": edge(&bottom(), &side(BACK))}),
    );
    let n = unit([0.0, 1.0, 2.0]).unwrap();
    assert_near(&diagonal["normal"], n);
    let error = failure(
        &mut doc,
        "construction_plane",
        json!({"type": "two_edges", "line1": edge(&top(), &side(FRONT)),
               "line2": edge(&bottom(), &side(RIGHT))}),
    );
    assert!(
        error.contains("do not lie in one plane (they pass 20.000000 mm apart)"),
        "{error}"
    );

    // cons_plane_three_points: (60,0,0), (0,40,0), (0,0,20): 2x + 3y + 6z = 120.
    let corner = plane(
        &mut doc,
        json!({"type": "three_points",
               "point1": vertex([&side(FRONT), &side(RIGHT), &bottom()]),
               "point2": vertex([&side(BACK), &side(LEFT), &bottom()]),
               "point3": vertex([&side(LEFT), &side(FRONT), &top()])}),
    );
    assert_near(&corner["normal"], [2.0 / 7.0, 3.0 / 7.0, 6.0 / 7.0]);
    assert_near(&corner["origin"], [60.0, 0.0, 0.0]);
    let error = failure(
        &mut doc,
        "construction_plane",
        json!({"type": "three_points", "point1": "origin",
               "point2": vertex([&side(FRONT), &side(RIGHT), &bottom()]),
               "point3": vertex([&side(LEFT), &side(FRONT), &bottom()])}),
    );
    assert!(error.contains("lie on one line"), "{error}");

    // The front bottom edge (along X) and the back top corner.
    let leaning = plane(
        &mut doc,
        json!({"type": "edge_and_point", "line": edge(&bottom(), &side(FRONT)),
               "point": vertex([&side(BACK), &side(LEFT), &top()])}),
    );
    assert_near(&leaning["normal"], unit([0.0, -1.0, 2.0]).unwrap());
    assert_near(&leaning["origin"], [0.0, 0.0, 0.0]);
}

#[test]
fn planes_tangent_to_faces_and_along_paths() {
    let mut doc = cylinder();
    let off = point(&mut doc, json!({"type": "point", "point": "origin"}));
    assert_near(&off["point"], [0.0; 3]);
    let error = failure(
        &mut doc,
        "construction_plane",
        json!({"type": "tangent_at_point", "face": {"body": "F2.b0", "face": "F2:side(c1)"},
               "point": "F3"}),
    );
    assert!(error.contains("on the axis"), "{error}");
    let top_rim = json!({"body": "F2.b0", "edges": [EdgeName::new(
        "F2:end(r{c1})".parse().unwrap(), "F2:side(c1)".parse().unwrap()).to_string()]});
    let quarter = plane(
        &mut doc,
        json!({"type": "along_path", "path": top_rim, "distance": 0.25}),
    );
    assert_near(&quarter["origin"], [0.0, 10.0, 40.0]);
    assert_near(&quarter["normal"], [-1.0, 0.0, 0.0]);

    let mut doc = block();
    let tip = point(
        &mut doc,
        json!({"type": "point", "point": vertex([&side(RIGHT), &side(BACK), &top()])}),
    );
    assert_near(&tip["point"], [60.0, 40.0, 20.0]);
    let touching = plane(
        &mut doc,
        json!({"type": "tangent_at_point", "face": face(&side(FRONT)), "point": "F3"}),
    );
    assert_near(&touching["origin"], [60.0, 0.0, 20.0]);
    assert_near(&touching["normal"], [0.0, -1.0, 0.0]);
    let path = json!({"body": "F2.b0", "edges": [edge(&top(), &side(FRONT))["edge"]]});
    let along = plane(
        &mut doc,
        json!({"type": "along_path", "path": path, "distance": 0.25}),
    );
    assert_near(&along["origin"], [15.0, 0.0, 20.0]);
    assert_near(&along["normal"], [1.0, 0.0, 0.0]);
    let physical = plane(
        &mut doc,
        json!({"type": "along_path", "path": path, "distance": 50, "physical": true}),
    );
    assert_near(&physical["origin"], [50.0, 0.0, 20.0]);
    let normal = plane(
        &mut doc,
        json!({"type": "normal_at_point", "path": path, "point": "F3"}),
    );
    assert_near(&normal["origin"], [60.0, 0.0, 20.0]);
    assert_near(&normal["normal"], [1.0, 0.0, 0.0]);
}

// Axes.

#[test]
fn axes_of_faces_planes_points_and_edges() {
    let mut doc = cylinder();
    let hole = axis(
        &mut doc,
        json!({"type": "circular_face", "face": {"body": "F2.b0", "face": "F2:side(c1)"}}),
    );
    assert_near(&hole["origin"], [0.0, 0.0, 20.0]);
    assert_near(&hole["direction"], [0.0, 0.0, 1.0]);
    let error = failure(
        &mut doc,
        "construction_axis",
        json!({"type": "circular_face", "face": {"body": "F2.b0", "face": "F2:end(r{c1})"}}),
    );
    assert!(
        error.contains("is not a cylinder, cone or torus (plane)"),
        "{error}"
    );

    let mut doc = block();
    // cons_axes: x = 30 and y = 20 meet along +Z.
    construct(
        &mut doc,
        "construction_plane",
        json!({"type": "offset", "plane": "yz", "distance": 30}),
    );
    construct(
        &mut doc,
        "construction_plane",
        json!({"type": "offset", "plane": "xz", "distance": 20}),
    );
    let planes = axis(
        &mut doc,
        json!({"type": "two_planes", "plane1": "F3", "plane2": "F4"}),
    );
    assert_near(&planes["direction"], [0.0, 0.0, 1.0]);
    assert_near(&planes["origin"], [30.0, 20.0, 0.0]);
    let diagonal = axis(
        &mut doc,
        json!({"type": "two_points", "point1": vertex([&side(LEFT), &side(FRONT), &bottom()]),
               "point2": vertex([&side(RIGHT), &side(BACK), &top()])}),
    );
    assert_near(&diagonal["direction"], unit([60.0, 40.0, 20.0]).unwrap());
    let vertical = axis(
        &mut doc,
        json!({"type": "edge", "edge": edge(&side(RIGHT), &side(BACK))}),
    );
    assert_near(&vertical["origin"], [60.0, 40.0, 0.0]);
    assert_near(&vertical["direction"], [0.0, 0.0, 1.0]);
    let above = point(
        &mut doc,
        json!({"type": "edge_and_plane", "line": "F7",
                                        "plane": {"body": "F2.b0", "face": top()}}),
    );
    assert_near(&above["point"], [60.0, 40.0, 20.0]);
    construct(
        &mut doc,
        "construction_point",
        json!({"type": "three_planes", "plane1": "F3", "plane2": "F4",
               "plane3": {"body": "F2.b0", "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}}),
    );
    let at = axis(
        &mut doc,
        json!({"type": "perpendicular_at_point", "face": face(&side(FRONT)), "point": "F9"}),
    );
    assert_near(&at["origin"], [30.0, 0.0, 20.0]);
    assert_near(&at["direction"], [0.0, -1.0, 0.0]);
    let through = axis(
        &mut doc,
        json!({"type": "normal_to_face_at_point", "face": face(&side(FRONT)), "point": "F9"}),
    );
    assert_near(&through["origin"], [30.0, 20.0, 20.0]);
    assert_near(&through["direction"], [0.0, -1.0, 0.0]);
    let error = failure(
        &mut doc,
        "construction_axis",
        json!({"type": "two_planes", "plane1": "xy", "plane2": face(&top())}),
    );
    assert!(error.contains("parallel"), "{error}");
}

// Points.

#[test]
fn points_at_intersections_and_centres() {
    let mut doc = cylinder();
    let centre = point(
        &mut doc,
        json!({"type": "center", "entity": {"body": "F2.b0", "edge": EdgeName::new(
            "F2:end(r{c1})".parse().unwrap(), "F2:side(c1)".parse().unwrap()).to_string()}}),
    );
    assert_near(&centre["point"], [0.0, 0.0, 40.0]);
    // cons_points: the three origin planes meet at the origin.
    let origin = point(
        &mut doc,
        json!({"type": "three_planes", "plane1": "xy", "plane2": "yz", "plane3": "xz"}),
    );
    assert_near(&origin["point"], [0.0; 3]);
    let error = failure(
        &mut doc,
        "construction_point",
        json!({"type": "center", "entity": {"body": "F2.b0", "face": "F2:end(r{c1})"}}),
    );
    assert!(
        error.contains("is not a sphere or a torus (plane)"),
        "{error}"
    );

    let mut doc = block();
    // Extended lines meet: the top front edge and the left front edge.
    let corner = point(
        &mut doc,
        json!({"type": "two_edges", "line1": edge(&top(), &side(FRONT)),
               "line2": edge(&side(LEFT), &side(FRONT))}),
    );
    assert_near(&corner["point"], [0.0, 0.0, 20.0]);
    let skew = failure(
        &mut doc,
        "construction_point",
        json!({"type": "two_edges", "line1": edge(&top(), &side(FRONT)),
               "line2": edge(&side(RIGHT), &side(BACK))}),
    );
    assert!(
        skew.contains("do not meet (they pass 40.000000 mm apart)"),
        "{skew}"
    );
    let crossing = point(
        &mut doc,
        json!({"type": "edge_and_plane", "line": "z",
               "plane": {"body": "F2.b0", "face": top()}}),
    );
    assert_near(&crossing["point"], [0.0, 0.0, 20.0]);
    let parallel = failure(
        &mut doc,
        "construction_point",
        json!({"type": "edge_and_plane", "line": "x", "plane": "xy"}),
    );
    assert!(parallel.contains("parallel to the plane"), "{parallel}");
    let path = json!({"body": "F2.b0", "edges": [edge(&top(), &side(FRONT))["edge"]]});
    let half = point(
        &mut doc,
        json!({"type": "along_path", "path": path, "distance": 0.5}),
    );
    assert_near(&half["point"], [30.0, 0.0, 20.0]);
    let beyond = point(
        &mut doc,
        json!({"type": "along_path", "path": path, "distance": 1.5}),
    );
    assert_near(&beyond["point"], [90.0, 0.0, 20.0]);
}

#[test]
fn fixed_datums_take_their_numbers() {
    let mut doc = Document::new(GeoKernel::default());
    // The frame is normalized and y made perpendicular to x.
    let plane = plane(
        &mut doc,
        json!({"type": "fixed", "origin": [1, 2, 3], "x_axis": [2, 0, 0], "y_axis": [1, 1, 0]}),
    );
    assert_near(&plane["origin"], [1.0, 2.0, 3.0]);
    assert_near(&plane["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&plane["y_axis"], [0.0, 1.0, 0.0]);
    let axis = axis(
        &mut doc,
        json!({"type": "fixed", "origin": [0, 0, 1], "direction": [0, 3, 4]}),
    );
    assert_near(&axis["direction"], [0.0, 0.6, 0.8]);
    let point = point(&mut doc, json!({"type": "fixed", "point": [4, 5, 6]}));
    assert_near(&point["point"], [4.0, 5.0, 6.0]);
    // Flipped: the normal and the direction reverse, the x axis stays.
    let flipped = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane", "flip": true,
               "definition": {"type": "offset", "plane": "xy", "distance": 5}}}),
    );
    let flipped = query(&doc, json!({"query": "datum", "uid": flipped["uid"]}));
    assert_near(&flipped["normal"], [0.0, 0.0, -1.0]);
    assert_near(&flipped["x_axis"], [1.0, 0.0, 0.0]);
    assert_near(&flipped["origin"], [0.0, 0.0, 5.0]);
    let reversed = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_axis", "flip": true,
               "definition": {"type": "edge", "edge": "z"}}}),
    );
    let reversed = query(&doc, json!({"query": "datum", "uid": reversed["uid"]}));
    assert_near(&reversed["direction"], [0.0, 0.0, -1.0]);
    assert_eq!(
        doc.query(&json!({"query": "feature", "uid": "F5"}).to_string())
            .map(|f| serde_json::from_str::<Value>(&f).unwrap()["def"]["flip"].clone())
            .unwrap(),
        json!(true)
    );
    for (kind, definition, expected) in [
        (
            "construction_plane",
            json!({"type": "fixed", "origin": [0, 0, 0], "x_axis": [1, 0, 0], "y_axis": [2, 0, 0]}),
            "the plane's y axis is zero or along its x axis",
        ),
        (
            "construction_axis",
            json!({"type": "fixed", "origin": [0, 0, 0], "direction": [0, 0, 0]}),
            "the axis's direction is zero",
        ),
    ] {
        let error = command_error(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": kind, "definition": definition}}),
        );
        assert!(error.contains(expected), "{error}");
    }
}

// References, timeline and files.

#[test]
fn references_must_be_of_the_right_kind_and_earlier() {
    let mut doc = block();
    construct(
        &mut doc,
        "construction_axis",
        json!({"type": "edge", "edge": edge(&side(RIGHT), &side(BACK))}),
    );
    for (definition, expected) in [
        (
            json!({"type": "offset", "plane": "F3", "distance": 1}),
            "Axis1 (F3) is a construction axis, not a plane or a planar face",
        ),
        (
            json!({"type": "offset", "plane": "F2", "distance": 1}),
            "Extrude1 (F2) is not construction geometry",
        ),
        (
            json!({"type": "offset", "plane": "x", "distance": 1}),
            "the origin X is not a plane or a planar face",
        ),
        (
            json!({"type": "offset", "plane": edge(&top(), &side(FRONT)), "distance": 1}),
            "is not a plane or a planar face",
        ),
        (
            json!({"type": "offset", "plane": "F9", "distance": 1}),
            "feature F9 does not exist",
        ),
        (
            json!({"type": "offset", "plane": {"body": "F1.b0", "face": "F1:end(r{c1})"}, "distance": 1}),
            "Sketch1 (F1) does not create bodies",
        ),
        (
            json!({"type": "offset", "plane": {"body": "F2.b0", "face": "F7:end(r{c1})"}, "distance": 1}),
            "face F7:end(r{c1}): feature F7 does not exist",
        ),
    ] {
        let error = command_error(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "construction_plane", "definition": definition}}),
        );
        assert!(error.contains(expected), "{error}");
    }
    for (reference, expected) in [
        (json!("XY"), "invalid reference 'XY'"),
        (
            json!({"body": "F2.b0"}),
            "body F2.b0 is not a plane or a planar face",
        ),
        (
            json!({"body": "F2.b0", "face": "F2:a", "vertex": "V{F2:a|F2:b|F2:c}"}),
            "names one \"face\", \"edge\" or \"vertex\"",
        ),
        (
            json!({"body": "F2.b0", "face": "F2:a", "edge": "y"}),
            "invalid name 'y'",
        ),
        (json!(3), "invalid reference 3"),
    ] {
        let error = command_error(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "construction_plane",
                   "definition": {"type": "offset", "plane": reference, "distance": 1}}}),
        );
        assert!(error.contains(expected), "{error}");
    }

    // A plane on the axis's face cannot move before the extrude.
    construct(
        &mut doc,
        "construction_plane",
        json!({"type": "offset", "plane": face(&top()), "distance": 5}),
    );
    let error = command_error(
        &mut doc,
        json!({"cmd": "reorder_feature", "uid": "F4", "index": 1}),
    );
    assert!(error.contains("does not come before"), "{error}");
    // Deleting the extrude takes its construction geometry along.
    let error = command_error(&mut doc, json!({"cmd": "delete_feature", "uid": "F2"}));
    assert!(
        error.contains("Extrude1 is used by Axis1, Plane1"),
        "{error}"
    );
}

#[test]
fn failures_pass_to_the_datums_that_depend_on_them() {
    let mut doc = block();
    construct(
        &mut doc,
        "construction_plane",
        json!({"type": "offset", "plane": face(&top()), "distance": 5}),
    );
    construct(
        &mut doc,
        "construction_axis",
        json!({"type": "two_planes", "plane1": "F3", "plane2": "xz"}),
    );
    let suppressed = command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F2"}));
    assert!(
        suppressed["error"]
            .as_str()
            .unwrap()
            .contains("body F2.b0 (from Extrude1) does not exist"),
        "{suppressed}"
    );
    let timeline = query(&doc, json!({"query": "timeline"}));
    assert_eq!(timeline["features"][3]["error"], "Plane1 has no result");
    assert_eq!(query(&doc, json!({"query": "datums"})), json!([]));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        query(&doc, json!({"query": "datums"}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn datum_queries_and_previews() {
    let mut doc = block();
    construct(
        &mut doc,
        "construction_point",
        json!({"type": "point", "point": vertex([&side(RIGHT), &side(BACK), &top()])}),
    );
    let datums = query(&doc, json!({"query": "datums", "origin": true}));
    let datums = datums.as_array().unwrap();
    assert_eq!(datums.len(), 8);
    assert_eq!(
        datums[1],
        json!({"uid": "xz", "name": "XZ", "type": "plane", "origin": [0.0, 0.0, 0.0],
               "normal": [0.0, 1.0, 0.0], "x_axis": [1.0, 0.0, 0.0], "y_axis": [0.0, 0.0, -1.0]})
    );
    assert_eq!(
        datums[7],
        json!({"uid": "F3", "name": "Point1", "type": "point", "point": [60.0, 40.0, 20.0]})
    );
    assert_eq!(
        query(&doc, json!({"query": "datum", "uid": "z"})),
        json!({"uid": "z", "name": "Z", "type": "axis", "origin": [0.0, 0.0, 0.0],
               "direction": [0.0, 0.0, 1.0]})
    );
    let error = doc
        .query(&json!({"query": "datum", "uid": "F2"}).to_string())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Extrude1 (F2) has no datum at the timeline marker"
    );

    let preview: Value = serde_json::from_str(
        &doc.preview(
            &json!({"cmd": "add_feature", "def": {"type": "construction_axis",
                    "definition": {"type": "two_points", "point1": "origin", "point2": "F3"}}})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["status"], "ok");
    assert_eq!(preview["datum"]["type"], "axis");
    assert_near(
        &preview["datum"]["direction"],
        unit([60.0, 40.0, 20.0]).unwrap(),
    );
    let text = doc
        .analysis(&json!({"query": "datums"}).to_string(), false)
        .unwrap();
    assert_eq!(text, "Point1 (F3): point at [60.000, 40.000, 20.000]\n");
}

#[test]
fn construction_features_round_trip_through_files() {
    let mut doc = block();
    construct(
        &mut doc,
        "construction_plane",
        json!({"type": "offset", "plane": face(&top()), "distance": 5}),
    );
    construct(
        &mut doc,
        "construction_axis",
        json!({"type": "edge", "edge": edge(&side(RIGHT), &side(BACK))}),
    );
    construct(
        &mut doc,
        "construction_point",
        json!({"type": "along_path", "path": {"body": "F2.b0",
               "edges": [edge(&top(), &side(FRONT))["edge"]]}, "distance": 0.5, "physical": false}),
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["features"][2],
        json!({"uid": "F3", "name": "Plane1", "type": "construction_plane",
               "definition": {"type": "offset", "plane": face(&top()), "distance": "d4"}})
    );
    assert_eq!(file["features"][4]["definition"]["distance"], "d5");
    assert!(file["features"][4]["definition"].get("physical").is_none());
    let mut loaded = Document::from_json(&saved, GeoKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);
    assert_eq!(
        query(&loaded, json!({"query": "datums"})),
        query(&doc, json!({"query": "datums"}))
    );
    let edited = command(
        &mut loaded,
        json!({"cmd": "edit_feature", "uid": "F3", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 7}}}),
    );
    assert_eq!(edited["parameters"], json!([]), "keeps its own d4");
    assert_near(
        &query(&loaded, json!({"query": "datum", "uid": "F3"}))["origin"],
        [0.0, 0.0, 7.0],
    );
}

// Body attributes and physical properties.

#[test]
fn body_attributes_are_commands_with_undo_and_saved() {
    let mut doc = block();
    let bodies = query(&doc, json!({"query": "bodies"}));
    assert_eq!(bodies, json!([{"uid": "F2.b0", "name": "Body1"}]));
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
    );
    assert_eq!(doc.undo_label(), Some("Hide Body1"));
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}),
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"}),
    );
    assert_eq!(
        query(&doc, json!({"query": "bodies"})),
        json!([{"uid": "F2.b0", "name": "Body1", "visible": false, "material": "aluminum",
                "appearance": "paint_red"}])
    );
    let saved = doc.to_json();
    let file: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(
        file["bodies"],
        json!([{"uid": "F2.b0", "name": "Body1", "visible": false, "material": "aluminum",
                "appearance": "paint_red"}])
    );
    let mut loaded = Document::from_json(&saved, GeoKernel::default()).unwrap();
    loaded.recompute();
    assert_eq!(loaded.to_json(), saved);

    // An unchanged value is no undo step; undo restores the attributes.
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}),
    );
    assert_eq!(doc.undo_label(), Some("Set Appearance of Body1"));
    command(&mut doc, json!({"cmd": "undo"}));
    command(&mut doc, json!({"cmd": "undo"}));
    assert_eq!(
        query(&doc, json!({"query": "bodies"})),
        json!([{"uid": "F2.b0", "name": "Body1", "visible": false}])
    );
    command(
        &mut doc,
        json!({"cmd": "set_body_visible", "uid": "F2.b0", "visible": true}),
    );
    assert_eq!(doc.undo_label(), Some("Show Body1"));
    assert_eq!(doc.to_json(), block().to_json());

    for (command_json, expected) in [
        (
            json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "cheese"}),
            "unknown material 'cheese' (known: steel,",
        ),
        (
            json!({"cmd": "set_body_visible", "uid": "F2.b1", "visible": false}),
            "body F2.b1 does not exist at the timeline marker",
        ),
        (
            json!({"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": " "}),
            "the appearance id is empty",
        ),
    ] {
        let error = command_error(&mut doc, command_json);
        assert!(error.contains(expected), "{error}");
    }
    let bad = saved.replace("\"aluminum\"", "\"cheese\"");
    let error = Document::from_json(&bad, GeoKernel::default())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("bodies[0]: unknown material 'cheese'"),
        "{error}"
    );
}

#[test]
fn physical_properties_use_materials_and_densities() {
    let mut doc = block();
    let steel = query(&doc, json!({"query": "properties"}));
    let body = &steel["bodies"][0];
    assert_eq!(body["material"], "steel");
    assert_eq!(body["density"], 7.85);
    assert_eq!(body["volume"], 48000.0);
    assert!((body["mass"].as_f64().unwrap() - 0.3768).abs() < 1e-12);
    assert_near(&body["center_of_mass"], [30.0, 20.0, 10.0]);
    assert_eq!(steel["total"]["mass"], body["mass"]);

    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}),
    );
    let aluminum = query(&doc, json!({"query": "properties", "density": 1.0}));
    assert_eq!(aluminum["bodies"][0]["material"], "aluminum");
    assert!((aluminum["bodies"][0]["mass"].as_f64().unwrap() - 0.1296).abs() < 1e-12);
    let custom = query(
        &doc,
        json!({"query": "properties", "bodies": ["F2.b0"], "densities": {"F2.b0": 1.0}}),
    );
    assert_eq!(custom["bodies"][0]["material"], Value::Null);
    assert!((custom["bodies"][0]["mass"].as_f64().unwrap() - 0.048).abs() < 1e-12);
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": "F2.b0", "material": null}),
    );
    let water = query(&doc, json!({"query": "properties", "density": 1.0}));
    assert_eq!(water["bodies"][0]["material"], Value::Null);
    assert_eq!(water["bodies"][0]["density"], 1.0);

    let error = doc
        .query(&json!({"query": "properties", "density": 0}).to_string())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "the density must be greater than zero, got 0"
    );
    let error = doc
        .query(&json!({"query": "properties", "bodies": ["F9.b0"]}).to_string())
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "body F9.b0 does not exist at the timeline marker"
    );

    let text = doc
        .analysis(&json!({"query": "properties"}).to_string(), false)
        .unwrap();
    assert!(
        text.starts_with("Body1 (F2.b0): Steel, density 7.85 g/cm3\n"),
        "{text}"
    );
    assert!(
        text.contains("    mass 0.376800 kg, volume 48000.000 mm3"),
        "{text}"
    );
    assert!(text.ends_with("Total: mass 0.376800 kg, volume 48000.000 mm3, centre of mass [30.000, 20.000, 10.000]\n"), "{text}");
}

#[test]
fn kernels_without_analysis_report_it() {
    let doc = {
        let mut doc = Document::new(MockKernel::default());
        doc.command(
            &json!([{"cmd": "sketch.create"},
                    {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 6, "height": 4},
                    {"cmd": "add_feature", "def": {"type": "extrude",
                     "profiles": [{"sketch": "F1", "region": REGION}],
                     "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}}])
            .to_string(),
        )
        .unwrap();
        doc
    };
    for (query, expected) in [
        (
            json!({"query": "properties"}),
            "Body1: the geometry kernel does not support physical properties",
        ),
        (
            json!({"query": "measure", "a": {"body": "F2.b0"}}),
            "F2.b0: the geometry kernel does not support measurement",
        ),
        (
            json!({"query": "measure", "a": {"datum": "xy"}}),
            "XY: the geometry kernel does not support datum display",
        ),
        (
            json!({"query": "interference"}),
            "interference: the geometry kernel does not support interference",
        ),
        (
            json!({"query": "section", "plane": "xy"}),
            "Body1: the geometry kernel does not support sections",
        ),
        (
            json!({"query": "section", "plane": {"origin": [0, 0, 1], "normal": [0, 0, 0]}}),
            "the plane's normal is zero",
        ),
        (
            json!({"query": "compare_step", "file": "x.step"}),
            "x.step: the geometry kernel does not support comparison with STEP files",
        ),
        (
            json!({"query": "measure", "a": {"body": "F2.b0", "datum": "xy"}}),
            "a selection is",
        ),
        (
            json!({"query": "section", "plane": {"body": "F2.b0", "face": "F2:side(c1[c4,c2])"}}),
            "the geometry kernel does not support face geometry",
        ),
    ] {
        let error = doc.query(&query.to_string()).unwrap_err().to_string();
        assert!(error.contains(expected), "{query}: {error}");
    }
}

#[test]
fn scripts_check_query_answers() {
    let mut doc = block();
    let script = json!([
        {"cmd": "add_feature", "def": {"type": "construction_plane",
         "definition": {"type": "offset", "plane": "xy", "distance": 4}}},
        {"expect": {"query": {"query": "datum", "uid": "F3"},
                    "result": {"type": "plane", "origin": [0, 0, 4.000001]}}},
        {"expect": {"query": {"query": "datums"}, "result": [{"name": "Plane1"}]}},
    ]);
    doc.run_script(&script.to_string()).unwrap();
    for (expectation, expected) in [
        (
            json!({"query": {"query": "datum", "uid": "F3"}, "result": {"origin": [0, 0, 5]}}),
            "expected origin[2] = 5, got 4 in the answer to",
        ),
        (
            json!({"query": {"query": "datum", "uid": "F3"}, "result": {"radius": 5}}),
            "expected radius (missing)",
        ),
        (
            json!({"query": {"query": "datums"}, "result": []}),
            "expected the answer with 0 items, got 1",
        ),
        (
            json!({"query": {"query": "datum", "uid": "F3"}, "result": {"type": "axis"}}),
            "expected type = \"axis\", got \"plane\"",
        ),
    ] {
        let error = doc
            .run_script(&json!([{"expect": expectation}]).to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}
