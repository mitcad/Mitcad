// SPDX-License-Identifier: MIT
//! The model's [`Kernel`] implemented with the OCCT geometry library. One
//! CXX bridge per operation family, all sharing the opaque `Shape` type of
//! [`shape`], so work on different families touches different files. The
//! C++ halves are in `core/cpp/bridge/`.

pub mod boolean;
pub mod dressup;
pub mod exchange;
pub mod extrude;
pub mod profile;
pub mod query;
pub mod shape;
pub mod sketch_ref;
pub mod transform;

// Face operations (F2).
pub mod faceops;
// Construction geometry and analysis (F5).
pub mod analysis;
pub mod datum;
// Profile features (F1).
pub mod profile_feature;
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
pub mod sweeps;
// Sketch text (P3).
pub mod text;
// The result store (P7d).
pub mod persist;
// Cancellation inside an operation (P7e).
pub mod cancel;
// Triangle meshes for 3D printing (3MF export, mitcad#13).
pub mod mesh;

use std::sync::Arc;

use cxx::SharedPtr;
use mitcad_model::analysis::{
    CompareOptions, Comparison, Interference, Measurement, PhysicalProperties, Section, Selection,
    Separation,
};
use mitcad_model::datum::{
    CurveGeometry, PathParameter, PathPoint, SurfaceGeometry, SurfacePoint, Vec3,
};
use mitcad_model::exchange::{
    ExportBody, ExportFormat, ExportOptions, LengthUnit, ReadOptions, StepSchema,
};
use mitcad_model::{BodyKind, ImportBody};
use mitcad_model::{
    BooleanOp, BooleanOutput, BooleanPiece, BoundingBox, Chamfer, ChamferSize, EdgeInfo, EdgeName,
    ExtrudeSpec, FaceInfo, FaceName, FeatureUid, Kernel, KernelError, MassProperties,
    ProfileRegion, SketchFrame,
};
use mitcad_model::{Cylinder, ExtrudeFeatureSpec, HoleSpec, Plane, RevolveSpec, ThreadSpec};
use mitcad_model::{Instance, PrimitiveShape, PrimitiveSpec, TopoName, Transform};
// Face operations (F2).
use mitcad_model::{Datum, DatumPlane, VertexName};
use mitcad_model::{DraftSpec, FilletSet, FilletSize, ShellSpec, ToolInput};
// .f3d import (T1d).
use mitcad_model::EdgeMiddle;
use mitcad_model::FacePoints;
// Cancellation inside an operation (P7e).
use mitcad_model::RecomputeMonitor;

pub use shape::ffi::Shape;

fn failed(error: cxx::Exception) -> KernelError {
    KernelError::Failed(error.what().to_owned())
}

fn occt(shape: &SharedPtr<Shape>) -> Result<&Shape, KernelError> {
    shape
        .as_ref()
        .ok_or_else(|| KernelError::failed("the body has no shape"))
}

fn names(edges: &[EdgeName]) -> Vec<String> {
    edges.iter().map(ToString::to_string).collect()
}

fn shapes(list: &shape::ffi::ShapeList) -> Vec<SharedPtr<Shape>> {
    (0..list.size()).map(|i| list.at(i)).collect()
}

fn face_names(faces: &[FaceName]) -> Vec<String> {
    faces.iter().map(ToString::to_string).collect()
}

/// A [`ToolInput`] in the form of the face operation bridge.
struct Tool {
    spec: faceops::ffi::ToolSpec,
    shapes: cxx::UniquePtr<shape::ffi::ShapeList>,
    frame: profile::ffi::Frame,
    regions: Vec<profile::ffi::Region>,
}

impl Tool {
    fn new(input: &ToolInput<'_, SharedPtr<Shape>>) -> Result<Self, KernelError> {
        let mut tool = Tool {
            spec: faceops::ffi::ToolSpec {
                kind: faceops::ffi::ToolKind::Plane,
                origin: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                face: String::new(),
                curves: Vec::new(),
                extend: false,
            },
            shapes: shape::ffi::new_shape_list(),
            frame: profile::frame(&SketchFrame::XY),
            regions: Vec::new(),
        };
        match input {
            ToolInput::Plane { origin, normal } => {
                tool.spec.origin = *origin;
                tool.spec.normal = *normal;
            }
            ToolInput::Face { shape, face } => {
                occt(shape)?;
                tool.spec.kind = faceops::ffi::ToolKind::Face;
                tool.spec.face = face.to_string();
                tool.shapes.pin_mut().push((*shape).clone());
            }
            ToolInput::Body { shape } => {
                occt(shape)?;
                tool.spec.kind = faceops::ffi::ToolKind::Body;
                tool.shapes.pin_mut().push((*shape).clone());
            }
            ToolInput::Curves {
                frame,
                regions,
                curves,
                direction,
            } => {
                tool.spec.kind = faceops::ffi::ToolKind::Curves;
                tool.spec.normal = *direction;
                tool.spec.curves = curves.iter().map(|c| c.curve_name()).collect();
                tool.frame = profile::frame(frame);
                tool.regions = regions.iter().map(profile::region).collect();
            }
        }
        Ok(tool)
    }
}

pub struct OcctKernel;

impl Kernel for OcctKernel {
    type Shape = SharedPtr<Shape>;

    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<Self::Shape, KernelError> {
        let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
        let sweep = extrude::ffi::Sweep {
            direction: spec.direction,
            start: spec.start,
            end: spec.end,
        };
        extrude::ffi::extrude(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &regions,
            &sweep,
        )
        .map_err(failed)
    }

    fn profile(
        &self,
        frame: &SketchFrame,
        regions: &[ProfileRegion],
    ) -> Result<Self::Shape, KernelError> {
        let regions: Vec<profile::ffi::Region> = regions.iter().map(profile::region).collect();
        profile::ffi::profile_shape(&profile::frame(frame), &regions).map_err(failed)
    }

    fn solids(&self, shape: &Self::Shape) -> Result<Vec<Self::Shape>, KernelError> {
        let list = query::ffi::solids(occt(shape)?).map_err(failed)?;
        Ok(shapes(&list))
    }

    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&Self::Shape],
        tool: &Self::Shape,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for target in targets {
            occt(target)?;
            list.pin_mut().push((*target).clone());
        }
        let tool = occt(tool)?;
        let result = match op {
            BooleanOp::Join => boolean::ffi::boolean_join(&list, tool),
            BooleanOp::Cut => boolean::ffi::boolean_cut(&list, tool),
            BooleanOp::Intersect => boolean::ffi::boolean_intersect(&list, tool),
        }
        .map_err(failed)?;
        Ok(BooleanOutput {
            pieces: (0..result.piece_count())
                .map(|i| BooleanPiece {
                    shape: result.piece(i),
                    sources: result.piece_sources(i),
                })
                .collect(),
            touched: (0..targets.len()).map(|i| result.touched(i)).collect(),
        })
    }

    fn fillet(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        sets: &[FilletSet<'_>],
        rolling_ball_corners: bool,
    ) -> Result<Self::Shape, KernelError> {
        let sets: Vec<dressup::ffi::FilletSet> = sets
            .iter()
            .map(|set| {
                let mut out = dressup::ffi::FilletSet {
                    edges: names(set.edges),
                    faces: face_names(set.faces),
                    kind: dressup::ffi::FilletKind::Constant,
                    value: 0.0,
                    value2: 0.0,
                    start_vertex: String::new(),
                    mid: Vec::new(),
                    reference_face: String::new(),
                    flip: false,
                    curvature: set.curvature,
                    weight: set.weight,
                    tangent_chain: set.tangent_chain,
                };
                match set.size {
                    FilletSize::Constant { radius } => out.value = radius,
                    FilletSize::ChordLength { length } => {
                        out.kind = dressup::ffi::FilletKind::ChordLength;
                        out.value = length;
                    }
                    FilletSize::Variable {
                        start,
                        end,
                        start_vertex,
                        mid,
                    } => {
                        out.kind = dressup::ffi::FilletKind::Variable;
                        out.value = start;
                        out.value2 = end;
                        out.start_vertex =
                            start_vertex.map(ToString::to_string).unwrap_or_default();
                        out.mid = mid.iter().flat_map(|&(p, r)| [p, r]).collect();
                    }
                    FilletSize::Asymmetric {
                        distance1,
                        distance2,
                        reference,
                        flip,
                    } => {
                        out.kind = dressup::ffi::FilletKind::Asymmetric;
                        out.value = distance1;
                        out.value2 = distance2;
                        out.reference_face = reference.map(ToString::to_string).unwrap_or_default();
                        out.flip = flip;
                    }
                }
                out
            })
            .collect();
        dressup::ffi::fillet(
            &feature.to_string(),
            occt(body)?,
            &sets,
            rolling_ball_corners,
        )
        .map_err(failed)
    }

    fn chamfer(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        sets: &[Chamfer<'_>],
        corner: mitcad_model::features::ChamferCorner,
    ) -> Result<Self::Shape, KernelError> {
        let sets: Vec<dressup::ffi::ChamferSet> = sets
            .iter()
            .map(|set| {
                let (kind, distance, value2) = match set.size {
                    ChamferSize::EqualDistance { distance } => {
                        (dressup::ffi::ChamferKind::EqualDistance, distance, 0.0)
                    }
                    ChamferSize::TwoDistances {
                        distance1,
                        distance2,
                    } => (
                        dressup::ffi::ChamferKind::TwoDistances,
                        distance1,
                        distance2,
                    ),
                    ChamferSize::DistanceAngle { distance, angle } => {
                        (dressup::ffi::ChamferKind::DistanceAngle, distance, angle)
                    }
                };
                dressup::ffi::ChamferSet {
                    edges: names(set.edges),
                    faces: face_names(set.faces),
                    kind,
                    distance,
                    value2,
                    reference_face: set.reference.map(ToString::to_string).unwrap_or_default(),
                    flip: set.flip,
                    tangent_chain: set.tangent_chain,
                }
            })
            .collect();
        use mitcad_model::features::ChamferCorner;
        let corner = match corner {
            ChamferCorner::Chamfer => dressup::ffi::ChamferCornerKind::Chamfer,
            ChamferCorner::Miter => dressup::ffi::ChamferCornerKind::Miter,
            ChamferCorner::Blend => dressup::ffi::ChamferCornerKind::Blend,
        };
        dressup::ffi::chamfer(&feature.to_string(), occt(body)?, &sets, corner).map_err(failed)
    }

    // Face operations (F2).

    fn shell(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        spec: &ShellSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        let params = faceops::ffi::ShellParams {
            inside: spec.inside,
            outside: spec.outside,
            tangent_chain: spec.tangent_chain,
            rounded: spec.rounded,
        };
        faceops::ffi::shell(
            &feature.to_string(),
            occt(body)?,
            &face_names(spec.faces),
            &params,
        )
        .map_err(failed)
    }

    fn draft(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        spec: &DraftSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let plane = Tool::new(&spec.plane)?;
        let params = faceops::ffi::DraftParams {
            angle: spec.angle,
            two_sided: spec.angle2.is_some(),
            angle2: spec.angle2.unwrap_or(0.0),
            flip: spec.flip,
            tangent_chain: spec.tangent_chain,
        };
        faceops::ffi::draft(
            &feature.to_string(),
            occt(body)?,
            &face_names(spec.faces),
            &plane.spec,
            &plane.shapes,
            &params,
        )
        .map_err(failed)
    }

    fn offset_faces(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
        distance: f64,
    ) -> Result<Self::Shape, KernelError> {
        faceops::ffi::offset_faces(
            &feature.to_string(),
            occt(body)?,
            &face_names(faces),
            distance,
        )
        .map_err(failed)
    }

    fn delete_faces(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
    ) -> Result<Self::Shape, KernelError> {
        faceops::ffi::delete_faces(&feature.to_string(), occt(body)?, &face_names(faces))
            .map_err(failed)
    }

    fn replace_faces(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
        target: &ToolInput<'_, Self::Shape>,
        tangent_chain: bool,
    ) -> Result<Self::Shape, KernelError> {
        let target = Tool::new(target)?;
        faceops::ffi::replace_faces(
            &feature.to_string(),
            occt(body)?,
            &face_names(faces),
            &target.spec,
            &target.shapes,
            tangent_chain,
        )
        .map_err(failed)
    }

    fn split_body(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        tool: &ToolInput<'_, Self::Shape>,
        extend: bool,
    ) -> Result<Vec<Self::Shape>, KernelError> {
        let mut tool = Tool::new(tool)?;
        tool.spec.extend = extend;
        let pieces = faceops::ffi::split_body(
            &feature.to_string(),
            occt(body)?,
            &tool.spec,
            &tool.shapes,
            &tool.frame,
            &tool.regions,
        )
        .map_err(failed)?;
        Ok(shapes(&pieces))
    }

    fn split_faces(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
        tool: &ToolInput<'_, Self::Shape>,
        extend: bool,
    ) -> Result<Self::Shape, KernelError> {
        let mut tool = Tool::new(tool)?;
        tool.spec.extend = extend;
        faceops::ffi::split_faces(
            &feature.to_string(),
            occt(body)?,
            &face_names(faces),
            &tool.spec,
            &tool.shapes,
            &tool.frame,
            &tool.regions,
        )
        .map_err(failed)
    }

    fn count_edges(&self, shape: &Self::Shape, edge: &EdgeName) -> usize {
        shape
            .as_ref()
            .map_or(0, |s| query::ffi::count_edges(s, &edge.to_string()))
    }

    fn count_faces(&self, shape: &Self::Shape, face: &FaceName) -> Result<usize, KernelError> {
        Ok(query::ffi::count_faces(occt(shape)?, &face.to_string()))
    }

    fn faces(&self, shape: &Self::Shape) -> Result<Vec<FaceInfo>, KernelError> {
        let faces = query::ffi::faces(occt(shape)?).map_err(failed)?;
        Ok(faces
            .into_iter()
            .map(|f| FaceInfo {
                names: f.names,
                surface: f.surface,
                area: f.area,
            })
            .collect())
    }

    fn edges(&self, shape: &Self::Shape) -> Result<Vec<EdgeInfo>, KernelError> {
        let edges = query::ffi::edges(occt(shape)?).map_err(failed)?;
        Ok(edges
            .into_iter()
            .map(|e| EdgeInfo {
                name: (!e.name.is_empty()).then_some(e.name),
                curve: e.curve,
                length: e.length,
            })
            .collect())
    }

    fn mass_properties(&self, shape: &Self::Shape) -> Result<MassProperties, KernelError> {
        let props = query::ffi::mass_properties(occt(shape)?).map_err(failed)?;
        Ok(MassProperties {
            volume: props.volume,
            area: props.area,
            center: props.center,
        })
    }

    fn bounding_box(&self, shape: &Self::Shape) -> Result<Option<BoundingBox>, KernelError> {
        let bounds = query::ffi::bounding_box(occt(shape)?).map_err(failed)?;
        Ok((!bounds.empty).then_some(BoundingBox {
            min: bounds.min,
            max: bounds.max,
        }))
    }

    // Imported bodies and data exchange (exchange.rs).

    fn import_brep(
        &self,
        feature: FeatureUid,
        data: &[u8],
        first_face: u32,
    ) -> Result<(Self::Shape, u32), KernelError> {
        let shape =
            exchange::ffi::import_brep(&feature.to_string(), data, first_face).map_err(failed)?;
        let faces = u32::try_from(exchange::ffi::face_count(occt(&shape)?))
            .map_err(|_| KernelError::failed("the body has too many faces"))?;
        Ok((shape, faces))
    }

    fn brep_data(&self, shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        exchange::ffi::brep_data(occt(shape)?).map_err(failed)
    }

    fn compound(&self, shapes: &[Self::Shape]) -> Result<Self::Shape, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for shape in shapes {
            occt(shape)?;
            list.pin_mut().push(shape.clone());
        }
        exchange::ffi::compound(&list).map_err(failed)
    }

    fn body_kind(&self, shape: &Self::Shape) -> Result<BodyKind, KernelError> {
        Ok(match exchange::ffi::body_kind(occt(shape)?) {
            exchange::ffi::BodyKind::Solid => BodyKind::Solid,
            exchange::ffi::BodyKind::Sheet => BodyKind::Sheet,
            exchange::ffi::BodyKind::Mesh => BodyKind::Mesh,
            _ => BodyKind::Empty,
        })
    }

    fn read_file(
        &self,
        path: &str,
        options: &ReadOptions,
    ) -> Result<Vec<ImportBody<Self::Shape>>, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        let infos =
            exchange::ffi::read_file(path, options.unit_mm, list.pin_mut()).map_err(failed)?;
        Ok(infos
            .into_iter()
            .zip(shapes(&list))
            .map(|(info, shape)| ImportBody {
                name: (!info.name.is_empty()).then_some(info.name),
                color: info.has_color.then_some(info.color),
                shape,
            })
            .collect())
    }

    fn write_file(
        &self,
        path: &str,
        bodies: &[ExportBody<'_, Self::Shape>],
        options: &ExportOptions,
    ) -> Result<(), KernelError> {
        use exchange::ffi::{
            BodyInfo, BodyPlacement, ExportFormat as Format, ExportSettings, LengthUnit as Unit,
        };
        let mut list = shape::ffi::new_shape_list();
        let mut infos = Vec::with_capacity(bodies.len());
        for body in bodies {
            occt(body.shape)?;
            list.pin_mut().push(body.shape.clone());
            infos.push(BodyInfo {
                name: body.name.to_owned(),
                has_color: body.color.is_some(),
                color: body.color.unwrap_or_default(),
                placements: body
                    .placements
                    .iter()
                    .map(|p| BodyPlacement {
                        rotation: std::array::from_fn(|i| p.transform.linear[i / 3][i % 3]),
                        translation: p.transform.translation,
                        name: p.name.clone(),
                    })
                    .collect(),
            });
        }
        let tolerance = options.refinement.tolerance();
        let settings = ExportSettings {
            format: match options.format {
                ExportFormat::Step => Format::Step,
                ExportFormat::Iges => Format::Iges,
                ExportFormat::Stl => Format::Stl,
                ExportFormat::Obj => Format::Obj,
                ExportFormat::Brep => Format::Brep,
                // The model writes 3MF from triangle_mesh.
                ExportFormat::ThreeMf => return Err(KernelError::Unsupported("3MF files")),
            },
            ap242: options.step_schema == StepSchema::Ap242,
            unit: match options.unit {
                LengthUnit::Millimeter => Unit::Millimeter,
                LengthUnit::Centimeter => Unit::Centimeter,
                LengthUnit::Meter => Unit::Meter,
                LengthUnit::Inch => Unit::Inch,
                LengthUnit::Foot => Unit::Foot,
            },
            deviation: tolerance.deviation,
            angle: tolerance.angle,
            ascii: options.ascii,
        };
        exchange::ffi::write_file(path, &list, &infos, &settings).map_err(failed)
    }

    // Transforms, patterns, mirrors, combine and primitives (F4).

    fn transform_shape(
        &self,
        shape: &Self::Shape,
        map: &Transform,
        instance: Option<Instance>,
    ) -> Result<Self::Shape, KernelError> {
        let affine = transform::ffi::Affine {
            linear: std::array::from_fn(|i| map.linear[i / 3][i % 3]),
            translation: map.translation,
        };
        let rename = instance.map(|i| i.prefix()).unwrap_or_default();
        transform::ffi::transform_shape(occt(shape)?, &affine, &rename).map_err(failed)
    }

    fn unite(&self, shapes: &[&Self::Shape]) -> Result<Self::Shape, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for shape in shapes {
            occt(shape)?;
            list.pin_mut().push((*shape).clone());
        }
        transform::ffi::unite(&list).map_err(failed)
    }

    fn face_tool(
        &self,
        body: &Self::Shape,
        faces: &[FaceName],
    ) -> Result<(Self::Shape, BooleanOp), KernelError> {
        let faces: Vec<String> = faces.iter().map(ToString::to_string).collect();
        let tool = transform::ffi::face_tool(occt(body)?, &faces).map_err(failed)?;
        let op = if tool.material() {
            BooleanOp::Join
        } else {
            BooleanOp::Cut
        };
        Ok((tool.tool(), op))
    }

    fn primitive(&self, spec: &PrimitiveSpec) -> Result<Self::Shape, KernelError> {
        use transform::ffi::{PrimitiveInput, PrimitiveKind};
        let input = match spec.shape {
            PrimitiveShape::Box {
                length,
                width,
                height,
            } => PrimitiveInput {
                kind: PrimitiveKind::Box,
                sizes: [length, width, height],
            },
            PrimitiveShape::Cylinder { radius, height } => PrimitiveInput {
                kind: PrimitiveKind::Cylinder,
                sizes: [radius, 0.0, height],
            },
            PrimitiveShape::Sphere { radius } => PrimitiveInput {
                kind: PrimitiveKind::Sphere,
                sizes: [radius, 0.0, 0.0],
            },
            PrimitiveShape::Torus {
                major_radius,
                minor_radius,
            } => PrimitiveInput {
                kind: PrimitiveKind::Torus,
                sizes: [major_radius, minor_radius, 0.0],
            },
        };
        transform::ffi::primitive(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &input,
        )
        .map_err(failed)
    }

    // Construction geometry (F5).

    fn face_geometry(
        &self,
        shape: &Self::Shape,
        face: &FaceName,
    ) -> Result<SurfaceGeometry, KernelError> {
        datum::ffi::datum_face_geometry(occt(shape)?, &face.to_string())
            .map(datum::surface)
            .map_err(failed)
    }

    fn edge_geometry(
        &self,
        shape: &Self::Shape,
        edge: &EdgeName,
    ) -> Result<CurveGeometry, KernelError> {
        datum::ffi::datum_edge_geometry(occt(shape)?, &edge.to_string())
            .map(datum::curve)
            .map_err(failed)
    }

    fn vertex_point(&self, shape: &Self::Shape, vertex: &VertexName) -> Result<Vec3, KernelError> {
        datum::ffi::datum_vertex_point(occt(shape)?, &vertex.to_string())
            .map(|p| p.xyz)
            .map_err(failed)
    }

    fn path_point(
        &self,
        shape: &Self::Shape,
        edges: &[EdgeName],
        at: PathParameter,
    ) -> Result<PathPoint, KernelError> {
        let (mode, value, near) = datum::path_parameter(at);
        let point = datum::ffi::datum_path_point(
            occt(shape)?,
            &names(edges),
            mode,
            value,
            &datum::vec(near),
        )
        .map_err(failed)?;
        Ok(PathPoint {
            point: point.point,
            tangent: point.direction,
        })
    }

    fn face_point_normal(
        &self,
        shape: &Self::Shape,
        face: &FaceName,
        near: Vec3,
    ) -> Result<SurfacePoint, KernelError> {
        let point =
            datum::ffi::datum_face_point(occt(shape)?, &face.to_string(), &datum::vec(near))
                .map_err(failed)?;
        Ok(SurfacePoint {
            point: point.point,
            normal: point.direction,
        })
    }

    fn datum_shape(&self, datum: &Datum, size: f64) -> Result<Self::Shape, KernelError> {
        match datum {
            Datum::Plane(plane) => datum::ffi::datum_plane_shape(
                &datum::vec(plane.origin),
                &datum::vec(plane.normal()),
                &datum::vec(plane.x_axis),
                size,
            ),
            Datum::Axis(axis) => datum::ffi::datum_axis_shape(
                &datum::vec(axis.origin),
                &datum::vec(axis.direction),
                size,
            ),
            Datum::Point(point) => datum::ffi::datum_point_shape(&datum::vec(point.point)),
        }
        .map_err(failed)
    }

    // Analysis (F5).

    fn physical_properties(
        &self,
        shape: &Self::Shape,
        density: f64,
    ) -> Result<PhysicalProperties, KernelError> {
        analysis::ffi::analysis_properties(occt(shape)?, density)
            .map(analysis::properties)
            .map_err(failed)
    }

    fn measure(&self, selection: &Selection<'_, Self::Shape>) -> Result<Measurement, KernelError> {
        analysis::ffi::analysis_measure(occt(selection.shape)?, &sub_name(selection))
            .map(analysis::measurement)
            .map_err(failed)
    }

    fn measure_between(
        &self,
        a: &Selection<'_, Self::Shape>,
        b: &Selection<'_, Self::Shape>,
    ) -> Result<Separation, KernelError> {
        analysis::ffi::analysis_between(occt(a.shape)?, &sub_name(a), occt(b.shape)?, &sub_name(b))
            .map(analysis::separation)
            .map_err(failed)
    }

    fn interferences(
        &self,
        bodies: &[&Self::Shape],
        min_volume: f64,
    ) -> Result<Vec<Interference<Self::Shape>>, KernelError> {
        let found = analysis::ffi::analysis_interferences(&*shape_list(bodies)?, min_volume)
            .map_err(failed)?;
        Ok((0..found.count())
            .map(|i| Interference {
                first: found.first(i),
                second: found.second(i),
                volume: found.volume(i),
                shape: found.shape(i),
            })
            .collect())
    }

    fn section(
        &self,
        shape: &Self::Shape,
        plane: &DatumPlane,
    ) -> Result<Section<Self::Shape>, KernelError> {
        let section = analysis::ffi::analysis_section(occt(shape)?, &analysis_plane(plane))
            .map_err(failed)?;
        Ok(Section {
            curves: section.curves(),
            faces: section.faces(),
            edge_count: section.edge_count(),
            face_count: section.face_count(),
            length: section.length(),
            area: section.area(),
        })
    }

    fn clip(&self, shape: &Self::Shape, plane: &DatumPlane) -> Result<Self::Shape, KernelError> {
        analysis::ffi::analysis_clip(occt(shape)?, &analysis_plane(plane)).map_err(failed)
    }

    fn compare_step(
        &self,
        shapes: &[&Self::Shape],
        path: &str,
        options: &CompareOptions,
    ) -> Result<Comparison, KernelError> {
        analysis::ffi::analysis_compare_step(
            &*shape_list(shapes)?,
            path,
            options.step_body.as_deref().unwrap_or(""),
            options.samples,
            options.fuzzy,
        )
        .map(analysis::comparison)
        .map_err(failed)
    }

    // Profile features (F1).

    fn extrude_feature(
        &self,
        spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
        let mut bodies = profile_feature::Bodies::default();
        let input = profile_feature::extrude_input(spec, &mut bodies)?;
        profile_feature::ffi::extrude_feature(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &regions,
            &input,
            bodies.list(),
        )
        .map_err(failed)
    }

    fn revolve(&self, spec: &RevolveSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
        let mut bodies = profile_feature::Bodies::default();
        let input = profile_feature::revolve_input(spec, &mut bodies)?;
        profile_feature::ffi::revolve(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &regions,
            &input,
            bodies.list(),
        )
        .map_err(failed)
    }

    fn hole_tool(&self, spec: &HoleSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        let mut bodies = profile_feature::Bodies::default();
        let input = profile_feature::hole_input(spec, &mut bodies)?;
        profile_feature::ffi::hole_tool(&spec.feature.to_string(), &input, bodies.list())
            .map_err(failed)
    }

    fn modeled_thread(
        &self,
        body: &Self::Shape,
        spec: &ThreadSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        profile_feature::ffi::modeled_thread(
            &spec.feature.to_string(),
            occt(body)?,
            &profile_feature::thread_input(spec),
        )
        .map_err(failed)
    }

    fn face_plane(&self, shape: &Self::Shape, face: &FaceName) -> Result<Plane, KernelError> {
        profile_feature::ffi::face_plane(occt(shape)?, &face.to_string())
            .map(profile_feature::plane)
            .map_err(failed)
    }

    fn face_cylinder(&self, shape: &Self::Shape, face: &FaceName) -> Result<Cylinder, KernelError> {
        profile_feature::ffi::face_cylinder(occt(shape)?, &face.to_string())
            .map(profile_feature::cylinder)
            .map_err(failed)
    }

    // Projected geometry (S1).

    fn curves_of(
        &self,
        shape: &Self::Shape,
        name: &TopoName,
    ) -> Result<Vec<mitcad_model::Curve3>, KernelError> {
        let curves =
            sketch_ref::ffi::model_curves(occt(shape)?, &name.to_string()).map_err(failed)?;
        Ok(curves.iter().map(sketch_ref::curve).collect())
    }

    // FreeCAD import (.FCStd).

    fn indexed_element(
        &self,
        shape: &Self::Shape,
        kind: mitcad_model::ElementKind,
        index: usize,
    ) -> Result<Option<mitcad_model::IndexedElement>, KernelError> {
        let kind = match kind {
            mitcad_model::ElementKind::Face => b'F',
            mitcad_model::ElementKind::Edge => b'E',
            mitcad_model::ElementKind::Vertex => b'V',
        };
        let element =
            sketch_ref::ffi::indexed_element(occt(shape)?, kind, index).map_err(failed)?;
        Ok(element.found.then(|| mitcad_model::IndexedElement {
            name: (!element.name.is_empty()).then(|| element.name.clone()),
            curves: element.curves.iter().map(sketch_ref::curve).collect(),
        }))
    }

    // .f3d import (T1).

    fn boundary_distances(
        &self,
        shape: &Self::Shape,
        points: &[Vec3],
    ) -> Result<Vec<f64>, KernelError> {
        let flat: Vec<f64> = points.iter().flatten().copied().collect();
        analysis::ffi::analysis_boundary_distances(occt(shape)?, &flat).map_err(failed)
    }

    fn points_inside(
        &self,
        shape: &Self::Shape,
        points: &[Vec3],
    ) -> Result<Vec<bool>, KernelError> {
        let flat: Vec<f64> = points.iter().flatten().copied().collect();
        analysis::ffi::analysis_points_inside(occt(shape)?, &flat).map_err(failed)
    }

    fn face_count(&self, shape: &Self::Shape) -> Result<usize, KernelError> {
        analysis::ffi::analysis_face_count(occt(shape)?).map_err(failed)
    }

    fn edge_middles(&self, shape: &Self::Shape) -> Result<Vec<EdgeMiddle>, KernelError> {
        let middles = analysis::ffi::analysis_edge_middles(occt(shape)?).map_err(failed)?;
        Ok(middles
            .into_iter()
            .map(|m| EdgeMiddle {
                name: m.name,
                point: m.point,
                tangent: m.tangent,
                length: m.length,
            })
            .collect())
    }

    fn face_points(&self, shape: &Self::Shape) -> Result<Vec<FacePoints>, KernelError> {
        let faces = analysis::ffi::analysis_face_points(occt(shape)?).map_err(failed)?;
        Ok(faces
            .into_iter()
            .map(|f| FacePoints {
                name: f.name,
                points: f.points.as_chunks::<3>().0.to_vec(),
            })
            .collect())
    }

    fn compare_shapes(
        &self,
        a: &[&Self::Shape],
        b: &[&Self::Shape],
        options: &CompareOptions,
    ) -> Result<Comparison, KernelError> {
        analysis::ffi::analysis_compare_shapes(
            &*shape_list(a)?,
            &*shape_list(b)?,
            options.samples,
            options.fuzzy,
        )
        .map(analysis::comparison)
        .map_err(failed)
    }

    // Sweeps, lofts, pipes, coils, ribs and webs (F3; sweeps.rs).

    fn sweep(&self, spec: &mitcad_model::SweepSpec<'_>) -> Result<Self::Shape, KernelError> {
        sweeps::sweep(spec)
    }

    fn loft(
        &self,
        spec: &mitcad_model::LoftSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        sweeps::loft(spec)
    }

    fn pipe(&self, spec: &mitcad_model::PipeSpec<'_>) -> Result<Self::Shape, KernelError> {
        sweeps::pipe(spec)
    }

    fn coil(&self, spec: &mitcad_model::CoilSpec) -> Result<Self::Shape, KernelError> {
        sweeps::coil(spec)
    }

    fn rib(
        &self,
        spec: &mitcad_model::RibSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        sweeps::rib(spec)
    }

    // Sketch text (P3).

    fn font_glyphs(
        &self,
        request: &mitcad_model::FontRequest<'_>,
    ) -> Result<mitcad_model::FontGlyphs, KernelError> {
        text::font_glyphs(request)
    }

    // Model warnings (P9).

    fn notes(&self, shape: &Self::Shape) -> Vec<String> {
        occt(shape).map_or_else(|_| Vec::new(), dressup::ffi::shape_notes)
    }

    // The result store (P7d).

    fn shape_bytes(&self, shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        let bytes = persist::ffi::shape_bytes(occt(shape)?).map_err(failed)?;
        Ok(bytes.as_bytes().to_vec())
    }

    fn shape_from_bytes(&self, bytes: &[u8]) -> Result<Self::Shape, KernelError> {
        persist::ffi::shape_from_bytes(bytes).map_err(failed)
    }

    fn shape_memory(&self, shape: &Self::Shape) -> u64 {
        occt(shape).map_or(0, persist::ffi::shape_memory)
    }

    // Cancellation inside an operation (P7e).

    fn interruptible<R>(&self, monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
        cancel::interruptible(monitor, f)
    }

    // Triangle meshes for 3D printing (3MF export, mitcad#13).

    fn triangle_mesh(
        &self,
        shape: &Self::Shape,
        tolerance: &mitcad_model::exchange::MeshTolerance,
    ) -> Result<mitcad_model::exchange::TriangleMesh, KernelError> {
        let mesh = mesh::ffi::triangle_mesh(occt(shape)?, tolerance.deviation, tolerance.angle)
            .map_err(failed)?;
        Ok(mitcad_model::exchange::TriangleMesh {
            vertices: mesh.vertices.as_chunks::<3>().0.to_vec(),
            triangles: mesh.triangles.as_chunks::<3>().0.to_vec(),
        })
    }

    // FreeCAD helices (mitcad#4; sweeps.rs).

    fn helix(&self, spec: &mitcad_model::HelixSpec<'_>) -> Result<Self::Shape, KernelError> {
        sweeps::helix(spec)
    }
}

/// The name of a selection's sub-shape, empty for the whole body.
fn sub_name<S>(selection: &Selection<'_, S>) -> String {
    selection.name.map(ToString::to_string).unwrap_or_default()
}

fn shape_list(
    shapes: &[&SharedPtr<Shape>],
) -> Result<cxx::UniquePtr<shape::ffi::ShapeList>, KernelError> {
    let mut list = shape::ffi::new_shape_list();
    for shape in shapes {
        occt(shape)?;
        list.pin_mut().push((*shape).clone());
    }
    Ok(list)
}

fn analysis_plane(plane: &DatumPlane) -> analysis::ffi::AnalysisPlane {
    analysis::ffi::AnalysisPlane {
        origin: plane.origin,
        normal: plane.normal(),
    }
}
