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
// Shaded meshes of faces (USDZ export, mitcad#123).
pub mod face_mesh;
// Sheet material construction (mitcad#206).
pub mod sheet;
// The faces a run of features made, copied by a pattern.
pub mod face_copy;
// Tangent chains of edges and faces, for the hover (mitcad#290).
pub mod tangent_chain;
// Bodies from their boundaries and from triangles (the IFC import,
// mitcad#273).
pub mod boundary;
// Contacts of a new Extrude's tool (mitcad#289).
pub mod contact;
// BUILD: the face names of derived parts (mitcad#375).
pub mod build;
// Contact sets: bodies that stop each other while joints move (mitcad#454).
pub mod motion_contact;
// Emboss: profiles wrapped onto cylinders (mitcad#458).
pub mod emboss;
// Thin extrusions along chains of sketch curves (mitcad#463).
pub mod extrude_curves;
// Boundary Fill: the cells tool bodies enclose (mitcad#457).
pub mod boundary_fill;
// Patch, Stitch and Thicken: surface bodies filled, sewn and made solid
// (mitcad#456).
pub mod surfaces;
// Surface modelling, surface sweeps (mitcad#455).
pub mod surface_sweeps;
// Surface modelling: Offset Surface, Ruled Surface and Extend (mitcad#455).
pub mod surface_offsets;
// Surface modelling: Trim, Untrim, Unstitch and Reverse Normal (mitcad#455).
pub mod surface_edits;
// Plastic part features: the bands of lips and grooves (mitcad#459).
pub mod plastic;
// Surface modelling: Blend, Boundary, Match and Fair Surface (mitcad#455).
pub mod surface_fills;
// Drawings: views with hidden lines removed and round holes (mitcad#471).
pub mod drawing;
// Gears: toothed profiles swept along their axis or ruled to an apex
// (mitcad#472).
pub mod gear;
// Faces moved by a rigid motion (mitcad#343).
pub mod move_faces;
// A planar face of a body extruded as a profile (mitcad#535).
pub mod face_extrude;
// A planar face of a body revolved as a profile (mitcad#539).
pub mod face_revolve;
// Display meshes kept in the result store (mitcad#858).
pub mod display_mesh;
// C++ exceptions under bridge calls as errors, tested (mitcad#937).
pub mod exception_probe;
// A Fillet's or Chamfer's picked edges as polylines (mitcad#957).
pub mod input_edges;
// Dogbones: the relief cylinders at a body's inside corners (mitcad#1011).
pub mod dogbone;

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
    failed_with(error.what())
}

/// The error of a C++ exception's message (`what`), as `failed` makes it.
fn failed_with(what: &str) -> KernelError {
    match what {
        // A C++ allocation that failed outside the geometry's operations,
        // which say so themselves (mitcad#80): std::bad_alloc's message of
        // libstdc++ and libc++, and of MSVC's library.
        "std::bad_alloc" | "bad allocation" => KernelError::Failed(format!(
            "the geometry kernel: {}",
            KernelError::OUT_OF_MEMORY
        )),
        what => KernelError::Failed(what.to_owned()),
    }
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

    fn sheet_shape(
        &self,
        spec: &mitcad_model::sheet::SheetShapeSpec<'_>,
        _prefix: &str,
    ) -> Result<mitcad_model::sheet::SheetGeometry<Self::Shape>, KernelError> {
        sheet::shape(spec)
    }

    fn sheet_trim_domain(
        &self,
        spec: &mitcad_model::sheet::SheetTrimSpec<'_>,
    ) -> Result<mitcad_model::sheet::SheetTrimResult, KernelError> {
        sheet::trim(spec)
    }

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
                    knots: Vec::new(),
                    radii: Vec::new(),
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
                    FilletSize::Spline {
                        start_vertex,
                        knots,
                        radii,
                    } => {
                        out.kind = dressup::ffi::FilletKind::Spline;
                        out.start_vertex =
                            start_vertex.map(ToString::to_string).unwrap_or_default();
                        out.knots = knots.to_vec();
                        out.radii = radii.to_vec();
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

    fn enclosing_box(&self, shape: &Self::Shape) -> Result<Option<BoundingBox>, KernelError> {
        let bounds = query::ffi::enclosing_box(occt(shape)?).map_err(failed)?;
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
                // And USDZ from face_meshes (mitcad#123).
                ExportFormat::Usdz => return Err(KernelError::Unsupported("USDZ files")),
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
            face_triangles: tolerance.face_triangles,
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

    fn move_shape(
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
        transform::ffi::move_shape(occt(shape)?, &affine, &rename).map_err(failed)
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

    fn maximum_face_gap(
        &self,
        a: &Selection<'_, Self::Shape>,
        b: &Selection<'_, Self::Shape>,
    ) -> Result<Option<f64>, KernelError> {
        analysis::ffi::analysis_face_gap(occt(a.shape)?, &sub_name(a), occt(b.shape)?, &sub_name(b))
            .map(analysis::maximum_face_gap)
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
        Ok(section_of(&section))
    }

    fn clip(&self, shape: &Self::Shape, plane: &DatumPlane) -> Result<Self::Shape, KernelError> {
        let clipped =
            analysis::ffi::analysis_clip(occt(shape)?, &analysis_plane(plane)).map_err(failed)?;
        // Null: the plane leaves the body whole, which is then the body's
        // own shape (the view keeps it drawn as it is, mitcad#438).
        Ok(if clipped.is_null() {
            shape.clone()
        } else {
            clipped
        })
    }

    fn section_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Section<Self::Shape>, KernelError>> {
        let shapes: Vec<&Self::Shape> = items.iter().map(|(shape, _)| *shape).collect();
        let Ok(list) = shape_list(&shapes) else {
            return items
                .iter()
                .map(|(shape, plane)| self.section(shape, plane))
                .collect();
        };
        let planes: Vec<_> = items
            .iter()
            .map(|(_, plane)| analysis_plane(plane))
            .collect();
        match analysis::ffi::analysis_section_all(&list, &planes) {
            Ok(sections) => (0..items.len())
                .map(|i| {
                    let section = sections.section(i);
                    match section.as_ref() {
                        Some(section) => Ok(section_of(section)),
                        None => Err(failed_with(&sections.error(i))),
                    }
                })
                .collect(),
            Err(error) => {
                let error = failed(error);
                items.iter().map(|_| Err(error.clone())).collect()
            }
        }
    }

    fn clip_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Self::Shape, KernelError>> {
        clip_all_on_cores(self, items)
    }

    fn detach_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Option<Self::Shape>, KernelError>> {
        let shapes: Vec<&Self::Shape> = items.iter().map(|(shape, _)| *shape).collect();
        let list = match shape_list(&shapes) {
            Ok(list) => list,
            Err(error) => return items.iter().map(|_| Err(error.clone())).collect(),
        };
        let planes: Vec<_> = items
            .iter()
            .map(|(_, plane)| analysis_plane(plane))
            .collect();
        match analysis::ffi::analysis_detach_all(&list, &planes) {
            Ok(detached) => (0..items.len())
                .map(|i| {
                    let error = detached.error(i);
                    if !error.is_empty() {
                        return Err(failed_with(&error));
                    }
                    let copy = detached.shape(i);
                    // Null: the plane passes the shape.
                    Ok(if copy.is_null() { None } else { Some(copy) })
                })
                .collect(),
            Err(error) => {
                let error = failed(error);
                items.iter().map(|_| Err(error.clone())).collect()
            }
        }
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
            options.seconds.unwrap_or(0.0),
            options.booleans_above,
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
        let mesh = mesh::ffi::triangle_mesh(
            occt(shape)?,
            tolerance.deviation,
            tolerance.angle,
            tolerance.face_triangles,
        )
        .map_err(failed)?;
        Ok(mitcad_model::exchange::TriangleMesh {
            vertices: mesh.vertices.as_chunks::<3>().0.to_vec(),
            triangles: mesh.triangles.as_chunks::<3>().0.to_vec(),
        })
    }

    // FreeCAD helices (mitcad#4; sweeps.rs).

    fn helix(
        &self,
        spec: &mitcad_model::HelixSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        sweeps::helix(spec)
    }

    // Segment crossings (the .f3d import's thread angles, mitcad#68;
    // analysis.rs).

    fn segment_crossings(
        &self,
        shape: &Self::Shape,
        from: Vec3,
        to: Vec3,
    ) -> Result<Vec<(f64, bool)>, KernelError> {
        let segment = [from[0], from[1], from[2], to[0], to[1], to[2]];
        let flat =
            analysis::ffi::analysis_segment_crossings(occt(shape)?, &segment).map_err(failed)?;
        Ok(flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[t, side]| (*t, *side < 0.0))
            .collect())
    }

    // Removed material (boolean.rs, mitcad#85).

    fn removed_material(
        &self,
        before: &Self::Shape,
        after: &Self::Shape,
        slack: f64,
    ) -> Result<Vec<Self::Shape>, KernelError> {
        let result =
            boolean::ffi::removed_material(occt(before)?, occt(after)?, slack).map_err(failed)?;
        Ok((0..result.piece_count()).map(|i| result.piece(i)).collect())
    }

    // Near copies joined (boolean.rs, mitcad#88).

    fn join_near_copy(
        &self,
        body: &Self::Shape,
        copy: &Self::Shape,
        slack: f64,
    ) -> Result<Option<BooleanOutput<Self::Shape>>, KernelError> {
        let result =
            boolean::ffi::join_near_copy(occt(body)?, occt(copy)?, slack).map_err(failed)?;
        // Not a near copy: no piece and the body not touched.
        if !result.touched(0) {
            return Ok(None);
        }
        Ok(Some(BooleanOutput {
            pieces: (0..result.piece_count())
                .map(|i| BooleanPiece {
                    shape: result.piece(i),
                    sources: result.piece_sources(i),
                })
                .collect(),
            touched: vec![true],
        }))
    }

    // Every face's and edge's geometry at once (the .f3d import's joint
    // sides, mitcad#87; datum.rs).

    fn face_geometries(
        &self,
        shape: &Self::Shape,
    ) -> Result<Vec<(Vec<String>, SurfaceGeometry)>, KernelError> {
        let faces = datum::ffi::datum_face_geometries(occt(shape)?).map_err(failed)?;
        Ok(faces
            .into_iter()
            .map(|f| (f.names, datum::surface(f.surface)))
            .collect())
    }

    fn edge_geometries(
        &self,
        shape: &Self::Shape,
    ) -> Result<Vec<(Option<String>, CurveGeometry)>, KernelError> {
        let edges = datum::ffi::datum_edge_geometries(occt(shape)?).map_err(failed)?;
        Ok(edges
            .into_iter()
            .map(|e| {
                (
                    (!e.name.is_empty()).then_some(e.name),
                    datum::curve(e.curve),
                )
            })
            .collect())
    }

    // Definitions evaluated in parallel (the .f3d import, mitcad#95): the
    // kernel holds no state; the geometry's state is per thread (its
    // cancellation scope) or behind locks.

    fn fork(&self) -> Option<Self> {
        Some(OcctKernel)
    }

    // A shape's validity (the `.ipt` import's replayed bodies, mitcad#60).

    fn is_valid(&self, shape: &Self::Shape) -> Result<bool, KernelError> {
        exchange::ffi::shape_is_valid(occt(shape)?).map_err(failed)
    }

    // An operation's new faces (the .f3d import's geometric check,
    // mitcad#138; analysis.rs).

    fn new_faces(
        &self,
        before: &[&Self::Shape],
        after: &Self::Shape,
        count: usize,
    ) -> Result<Vec<mitcad_model::NewFace>, KernelError> {
        let faces = analysis::ffi::analysis_new_faces(&*shape_list(before)?, occt(after)?, count)
            .map_err(failed)?;
        Ok(faces
            .into_iter()
            .map(|f| mitcad_model::NewFace {
                area: f.area,
                points: f.points.as_chunks::<3>().0.to_vec(),
            })
            .collect())
    }

    // Measures of other programs' integration (the FreeCAD import's
    // reference check, mitcad#139; query.rs).

    fn fixed_point_properties(&self, shape: &Self::Shape) -> Result<MassProperties, KernelError> {
        let props = query::ffi::fixed_point_properties(occt(shape)?).map_err(failed)?;
        Ok(MassProperties {
            volume: props.volume,
            area: props.area,
            center: props.center,
        })
    }

    // Sweeps along guide surfaces (mitcad#157; sweeps.rs).

    fn surface_sweep(
        &self,
        spec: &mitcad_model::SweepSpec<'_>,
        surfaces: &[mitcad_model::GuideSurface<'_, Self::Shape>],
    ) -> Result<Self::Shape, KernelError> {
        sweeps::surface_sweep(spec, surfaces)
    }

    // Shaded meshes of faces (USDZ export, mitcad#123; face_mesh.rs).

    fn face_meshes(
        &self,
        shape: &Self::Shape,
        tolerance: &mitcad_model::exchange::MeshTolerance,
    ) -> Result<mitcad_model::exchange::FaceMeshes, KernelError> {
        let mesh = face_mesh::ffi::face_meshes(
            occt(shape)?,
            tolerance.deviation,
            tolerance.angle,
            tolerance.face_triangles,
        )
        .map_err(failed)?;
        Ok(mitcad_model::exchange::FaceMeshes {
            vertices: mesh.vertices.as_chunks::<3>().0.to_vec(),
            normals: mesh.normals.as_chunks::<3>().0.to_vec(),
            triangles: mesh.triangles.as_chunks::<3>().0.to_vec(),
            face_triangles: mesh.face_triangles,
        })
    }

    // Crossings of several segments (the .f3d import's probes, mitcad#181;
    // analysis.rs).

    fn segments_crossings(
        &self,
        shape: &Self::Shape,
        segments: &[(Vec3, Vec3)],
    ) -> Result<Vec<Vec<(f64, bool)>>, KernelError> {
        let flat: Vec<f64> = segments
            .iter()
            .flat_map(|(from, to)| [from[0], from[1], from[2], to[0], to[1], to[2]])
            .collect();
        let out =
            analysis::ffi::analysis_segments_crossings(occt(shape)?, &flat).map_err(failed)?;
        let mut rest = out.as_slice();
        let mut all = Vec::with_capacity(segments.len());
        for _ in segments {
            let (&n, tail) = rest
                .split_first()
                .ok_or_else(|| KernelError::failed("segment crossings: short result"))?;
            let n = n as usize;
            if tail.len() < 2 * n {
                return Err(KernelError::failed("segment crossings: short result"));
            }
            all.push(
                tail[..2 * n]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|[t, side]| (*t, *side < 0.0))
                    .collect(),
            );
            rest = &tail[2 * n..];
        }
        Ok(all)
    }

    // The faces a run of features made (face_copy.rs).

    fn feature_changes(
        &self,
        before: &Self::Shape,
        after: &Self::Shape,
    ) -> Result<mitcad_model::FeatureChanges<Self::Shape>, KernelError> {
        let changes =
            face_copy::ffi::feature_changes(occt(before)?, occt(after)?).map_err(failed)?;
        Ok(mitcad_model::FeatureChanges {
            added: (0..changes.added_count())
                .map(|i| {
                    let seam = changes.added_seam(i);
                    mitcad_model::AddedMaterial {
                        solid: changes.added(i),
                        seam: (!seam.is_null()).then_some(seam),
                    }
                })
                .collect(),
            removed: (0..changes.removed_count())
                .map(|i| changes.removed(i))
                .collect(),
        })
    }

    fn supported_copies(
        &self,
        body: &Self::Shape,
        copies: &[(&Self::Shape, Option<&Self::Shape>)],
    ) -> Result<Vec<Option<Self::Shape>>, KernelError> {
        let mut solids = shape::ffi::new_shape_list();
        let mut seams = shape::ffi::new_shape_list();
        for (solid, seam) in copies {
            occt(solid)?;
            solids.pin_mut().push((*solid).clone());
            seams
                .pin_mut()
                .push(seam.map_or_else(SharedPtr::null, Clone::clone));
        }
        let parts =
            face_copy::ffi::supported_copies(occt(body)?, &solids, &seams).map_err(failed)?;
        Ok((0..parts.size())
            .map(|i| {
                let part = parts.at(i);
                (!part.is_null()).then_some(part)
            })
            .collect())
    }

    // Face boxes (the `.f3d` import's face lookups through a point,
    // mitcad#119; analysis.rs).

    fn face_boxes(&self, shape: &Self::Shape) -> Result<Vec<[Vec3; 2]>, KernelError> {
        let flat = analysis::ffi::analysis_face_boxes(occt(shape)?).map_err(failed)?;
        Ok(flat
            .as_chunks::<6>()
            .0
            .iter()
            .map(|b| [[b[0], b[1], b[2]], [b[3], b[4], b[5]]])
            .collect())
    }

    // Base features that adopt the shapes their data was written from
    // (mitcad#244; exchange.rs).

    fn adopt_shape(
        &self,
        feature: FeatureUid,
        shape: &Self::Shape,
        first_face: u32,
    ) -> Result<(Self::Shape, u32), KernelError> {
        let adopted = exchange::ffi::adopt_body(&feature.to_string(), occt(shape)?, first_face)
            .map_err(failed)?;
        let faces = u32::try_from(exchange::ffi::face_count(occt(&adopted)?))
            .map_err(|_| KernelError::failed("the body has too many faces"))?;
        Ok((adopted, faces))
    }

    // Tangent chains, for the hover (mitcad#290; tangent_chain.rs).

    fn tangent_chains(
        &self,
        shape: &Self::Shape,
        faces: bool,
    ) -> Result<Vec<Vec<String>>, KernelError> {
        let chains = tangent_chain::ffi::tangent_chains(occt(shape)?, faces).map_err(failed)?;
        Ok(chains.into_iter().map(|chain| chain.names).collect())
    }

    // Bodies from their boundaries and from triangles (the IFC import,
    // mitcad#273).

    fn boundary_body(
        &self,
        body: &mitcad_model::boundary::BoundaryBody,
    ) -> Result<(Self::Shape, mitcad_model::boundary::BoundaryReport), KernelError> {
        boundary::boundary_body(body)
    }

    fn triangle_body(
        &self,
        points: &[[f64; 3]],
        triangles: &[[u32; 3]],
    ) -> Result<Self::Shape, KernelError> {
        boundary::triangle_body(points, triangles)
    }

    // Contacts of a new Extrude's tool (mitcad#289; contact.rs).

    fn tool_contacts(
        &self,
        tool: &Self::Shape,
        bodies: &[&Self::Shape],
    ) -> Result<Vec<mitcad_model::ToolContact>, KernelError> {
        let list = shape_list(bodies)?;
        let contacts = contact::ffi::tool_contacts(occt(tool)?, &list).map_err(failed)?;
        Ok(contacts
            .into_iter()
            .map(|c| match c {
                2 => mitcad_model::ToolContact::Inside,
                1 => mitcad_model::ToolContact::Touches,
                _ => mitcad_model::ToolContact::Apart,
            })
            .collect())
    }

    // The volume a change makes from the faces it changed (the .ipt
    // import's change check, mitcad#235; analysis.rs).

    fn changed_volume(
        &self,
        before: &[&Self::Shape],
        after: &[&Self::Shape],
    ) -> Result<(f64, f64), KernelError> {
        let values =
            analysis::ffi::analysis_changed_volume(&*shape_list(before)?, &*shape_list(after)?)
                .map_err(failed)?;
        Ok((values[0], values[1]))
    }

    // Joins within a tolerance (mitcad#298; boolean.rs).

    fn join_within(
        &self,
        targets: &[&Self::Shape],
        tool: &Self::Shape,
        tolerance: f64,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for target in targets {
            occt(target)?;
            list.pin_mut().push((*target).clone());
        }
        let result =
            boolean::ffi::boolean_join_within(&list, occt(tool)?, tolerance).map_err(failed)?;
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

    // Cuts within a tolerance (mitcad#235; boolean.rs).

    fn cut_within(
        &self,
        targets: &[&Self::Shape],
        tool: &Self::Shape,
        tolerance: f64,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for target in targets {
            occt(target)?;
            list.pin_mut().push((*target).clone());
        }
        let result =
            boolean::ffi::boolean_cut_within(&list, occt(tool)?, tolerance).map_err(failed)?;
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

    // BUILD: the face names of derived parts (mitcad#375; build.rs).

    fn rename_faces(
        &self,
        shape: &Self::Shape,
        prefix: &str,
        renames: &[(FaceName, FaceName)],
    ) -> Result<Self::Shape, KernelError> {
        let from: Vec<String> = renames.iter().map(|(f, _)| f.to_string()).collect();
        let to: Vec<String> = renames.iter().map(|(_, t)| t.to_string()).collect();
        build::ffi::rename_faces(occt(shape)?, prefix, &from, &to).map_err(failed)
    }

    // Section curves of sketches (mitcad#389).

    fn section_curves(
        &self,
        shape: &Self::Shape,
        face: Option<&FaceName>,
        plane: &DatumPlane,
    ) -> Result<Vec<mitcad_model::SectionCurve>, KernelError> {
        let name = face.map(ToString::to_string).unwrap_or_default();
        let pieces =
            sketch_ref::ffi::section_curves(occt(shape)?, &name, plane.origin, plane.normal())
                .map_err(failed)?;
        pieces
            .iter()
            .map(|p| {
                let faces = p
                    .faces
                    .iter()
                    .map(|f| {
                        f.parse::<FaceName>()
                            .map_err(|e| KernelError::failed(format!("face name {f}: {e}")))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(mitcad_model::SectionCurve {
                    curve: sketch_ref::curve(&p.curve),
                    faces,
                })
            })
            .collect()
    }

    // Unions within a tolerance (mitcad#397).

    fn unite_within(
        &self,
        shapes: &[&Self::Shape],
        tolerance: f64,
    ) -> Result<Self::Shape, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for shape in shapes {
            occt(shape)?;
            list.pin_mut().push((*shape).clone());
        }
        transform::ffi::unite_within(&list, tolerance).map_err(failed)
    }

    // Contact sets (mitcad#454; motion_contact.rs).

    fn overlap_volumes(
        &self,
        pairs: &[mitcad_model::PlacedPair<'_, Self::Shape>],
        tolerance: f64,
    ) -> Result<Vec<f64>, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        let index = |shape: &Self::Shape,
                     list: &mut cxx::UniquePtr<shape::ffi::ShapeList>|
         -> Result<usize, KernelError> {
            occt(shape)?;
            let at = list.size();
            list.pin_mut().push(shape.clone());
            Ok(at)
        };
        let mut inputs = Vec::with_capacity(pairs.len());
        for pair in pairs {
            inputs.push(motion_contact::ffi::OverlapPair {
                a: index(pair.a, &mut list)?,
                a_placement: motion_contact::rows(&pair.a_placement),
                b: index(pair.b, &mut list)?,
                b_placement: motion_contact::rows(&pair.b_placement),
            });
        }
        motion_contact::ffi::overlap_volumes(&list, &inputs, tolerance).map_err(failed)
    }

    // Emboss: profiles wrapped onto cylinders (mitcad#458; emboss.rs).

    fn wrapped_emboss(
        &self,
        spec: &mitcad_model::EmbossWrapSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
        let wrap = emboss::ffi::EmbossWrap {
            axis_origin: spec.axis.origin,
            axis_direction: spec.axis.direction,
            radius: spec.radius,
            to_radius: spec.to_radius,
            half_angle: spec.half_angle,
        };
        emboss::ffi::wrapped_emboss(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &regions,
            &wrap,
        )
        .map_err(failed)
    }

    // Thin extrusions along chains of sketch curves (mitcad#463; extrude_curves.rs).

    fn extrude_curves(
        &self,
        spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
        chains: &[Vec<mitcad_model::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        extrude_curves::extrude_curves(spec, chains)
    }

    // Boundary Fill (mitcad#457; boundary_fill.rs).

    fn boundary_cells(
        &self,
        tools: &[&Self::Shape],
        points: &[Vec3],
    ) -> Result<Vec<mitcad_model::BoundaryCell<Self::Shape>>, KernelError> {
        let list = shape_list(tools)?;
        let flat: Vec<f64> = points.iter().flatten().copied().collect();
        let cells = boundary_fill::ffi::boundary_cells(&list, &flat).map_err(failed)?;
        Ok((0..cells.cell_count())
            .map(|i| mitcad_model::BoundaryCell {
                shape: cells.cell(i),
                holders: cells.cell_holders(i),
            })
            .collect())
    }

    // Patch, Stitch and Thicken: surface bodies filled, sewn and made solid
    // (mitcad#456; surfaces.rs).

    fn patch_regions(
        &self,
        feature: FeatureUid,
        frame: &SketchFrame,
        regions: &[ProfileRegion],
    ) -> Result<Vec<Self::Shape>, KernelError> {
        let regions: Vec<profile::ffi::Region> = regions.iter().map(profile::region).collect();
        let list =
            surfaces::ffi::patch_regions(&feature.to_string(), &profile::frame(frame), &regions)
                .map_err(failed)?;
        Ok(shapes(&list))
    }

    fn open_boundaries(
        &self,
        shape: &Self::Shape,
    ) -> Result<Vec<mitcad_model::OpenBoundary>, KernelError> {
        surfaces::ffi::open_boundaries(occt(shape)?)
            .map_err(failed)?
            .into_iter()
            .map(surfaces::open_boundary)
            .collect()
    }

    fn patch_loop(
        &self,
        spec: &mitcad_model::PatchLoopSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        surfaces::ffi::patch_loop(
            &spec.feature.to_string(),
            occt(spec.body)?,
            &surfaces::patch_loop(spec),
        )
        .map_err(failed)
    }

    fn stitch(&self, shapes: &[&Self::Shape], tolerance: f64) -> Result<Self::Shape, KernelError> {
        let mut list = shape::ffi::new_shape_list();
        for shape in shapes {
            occt(shape)?;
            list.pin_mut().push((*shape).clone());
        }
        surfaces::ffi::stitch(&list, tolerance).map_err(failed)
    }

    fn thicken(
        &self,
        feature: FeatureUid,
        shape: &Self::Shape,
        thickness: f64,
        symmetric: bool,
    ) -> Result<Self::Shape, KernelError> {
        surfaces::ffi::thicken(&feature.to_string(), occt(shape)?, thickness, symmetric)
            .map_err(failed)
    }

    // Surface modelling, surface sweeps (mitcad#455; surface_sweeps.rs).

    fn open_faces(
        &self,
        shape: &Self::Shape,
        feature: FeatureUid,
        roles: &[&str],
    ) -> Result<Self::Shape, KernelError> {
        let roles: Vec<String> = roles.iter().map(|r| (*r).to_owned()).collect();
        surface_sweeps::ffi::open_faces(occt(shape)?, &feature.to_string(), &roles).map_err(failed)
    }

    fn extrude_sheet(
        &self,
        spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
        chains: &[Vec<mitcad_model::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        let distance = |side: &mitcad_model::ExtrudeSide<Self::Shape>| match side.end {
            mitcad_model::SideEnd::Distance(d) if side.taper == 0.0 => Ok(d),
            mitcad_model::SideEnd::Distance(_) => Err(KernelError::Unsupported(
                "a taper on a surface extrusion of curves",
            )),
            mitcad_model::SideEnd::Target(_) => Err(KernelError::Unsupported(
                "a surface extrusion of curves up to an object",
            )),
        };
        let one = distance(&spec.side1)?;
        let two = spec
            .side2
            .as_ref()
            .map(distance)
            .transpose()?
            .unwrap_or(0.0);
        let shift = match spec.start {
            mitcad_model::ExtrudeStart::Offset(offset) => spec.frame.normal().map(|v| v * offset),
            mitcad_model::ExtrudeStart::Object(_) => {
                return Err(KernelError::Unsupported(
                    "a surface extrusion of curves from an object",
                ));
            }
        };
        let (curves, counts) = surface_sweeps::chains(chains);
        surface_sweeps::ffi::extrude_sheet(
            &spec.feature.to_string(),
            &curves,
            &counts,
            &shift,
            &spec.direction,
            one,
            two,
        )
        .map_err(failed)
    }

    fn revolve_sheet(
        &self,
        spec: &RevolveSpec<'_, Self::Shape>,
        chains: &[Vec<mitcad_model::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        if spec.target.is_some() {
            return Err(KernelError::Unsupported(
                "a surface revolution of curves up to an object",
            ));
        }
        let (curves, counts) = surface_sweeps::chains(chains);
        surface_sweeps::ffi::revolve_sheet(
            &spec.feature.to_string(),
            &curves,
            &counts,
            &spec.axis.origin,
            &spec.axis.direction,
            spec.angle1,
            spec.angle2.unwrap_or(0.0),
        )
        .map_err(failed)
    }

    fn sweep_sheet(
        &self,
        spec: &mitcad_model::SweepSpec<'_>,
        chains: &[Vec<mitcad_model::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        if spec.guide.is_some() || spec.twist != 0.0 || spec.taper != 0.0 || spec.end_scale != 1.0 {
            return Err(KernelError::Unsupported(
                "twists, tapers, end scales and guide rails on a surface sweep of curves",
            ));
        }
        let (curves, counts) = surface_sweeps::chains(chains);
        let path: Vec<sweeps::ffi::SweepCurve> = spec.path.iter().map(sweeps::curve).collect();
        let normal = spec.frame.normal();
        let o = spec.frame.origin;
        let plane = [o[0], o[1], o[2], normal[0], normal[1], normal[2]];
        surface_sweeps::ffi::sweep_sheet(
            &spec.feature.to_string(),
            &curves,
            &counts,
            &path,
            &plane,
            &[spec.extent1, spec.extent2],
            spec.orientation == mitcad_model::SweepOrientation::Parallel,
        )
        .map_err(failed)
    }

    fn loft_sheet(
        &self,
        feature: FeatureUid,
        sections: &[Vec<mitcad_model::PathCurve>],
        ruled: bool,
        closed: bool,
    ) -> Result<Self::Shape, KernelError> {
        let (curves, counts) = surface_sweeps::chains(sections);
        surface_sweeps::ffi::loft_sheet(&feature.to_string(), &curves, &counts, ruled, closed)
            .map_err(failed)
    }

    // Surface modelling: Offset Surface, Ruled Surface and Extend
    // (mitcad#455; surface_offsets.rs).

    fn offset_surface(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
        distance: f64,
        chain: bool,
    ) -> Result<Self::Shape, KernelError> {
        surface_offsets::ffi::offset_surface(
            &feature.to_string(),
            occt(body)?,
            &face_names(faces),
            distance,
            chain,
        )
        .map_err(failed)
    }

    fn ruled_surface(
        &self,
        spec: &mitcad_model::RuledSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        surface_offsets::ffi::ruled_surface(
            &spec.feature.to_string(),
            occt(spec.body)?,
            &surface_offsets::ruled(spec),
        )
        .map_err(failed)
    }

    fn extend_surface(
        &self,
        body: &Self::Shape,
        edges: &[EdgeName],
        openings: &[mitcad_model::OpeningEdges<'_>],
        distance: f64,
        mode: mitcad_model::ExtendMode,
    ) -> Result<Self::Shape, KernelError> {
        surface_offsets::ffi::extend_surface(
            occt(body)?,
            &names(edges),
            &surface_offsets::openings(openings),
            distance,
            surface_offsets::extend_mode(mode),
        )
        .map_err(failed)
    }

    // Surface modelling: Trim, Untrim, Unstitch and Reverse Normal
    // (mitcad#455; surface_edits.rs).

    fn trim_surface(
        &self,
        spec: &mitcad_model::TrimSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let plane = ToolInput::Plane {
            origin: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
        };
        let (input, chains, direction) = match &spec.tool {
            mitcad_model::TrimTool::Tool(input) => (input, &[][..], [0.0, 0.0, 1.0]),
            mitcad_model::TrimTool::Curves { chains, direction } => (&plane, *chains, *direction),
        };
        let tool = Tool::new(input)?;
        let (curves, counts) = surface_sweeps::chains(chains);
        let data = surface_edits::ffi::TrimData {
            by_curves: matches!(spec.tool, mitcad_model::TrimTool::Curves { .. }),
            chains: counts,
            direction,
            extend: spec.extend,
            keep: spec.keep.iter().flatten().copied().collect(),
        };
        surface_edits::ffi::trim_surface(
            occt(spec.body)?,
            &tool.spec,
            &tool.shapes,
            &tool.frame,
            &tool.regions,
            &curves,
            &data,
        )
        .map_err(failed)
    }

    fn untrim_surface(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        face: &FaceName,
        holes: bool,
        outer: bool,
        in_place: bool,
    ) -> Result<Self::Shape, KernelError> {
        surface_edits::ffi::untrim_surface(
            &feature.to_string(),
            occt(body)?,
            &face.to_string(),
            holes,
            outer,
            in_place,
        )
        .map_err(failed)
    }

    fn unstitch(
        &self,
        body: &Self::Shape,
        faces: &[FaceName],
    ) -> Result<mitcad_model::Unstitched<Self::Shape>, KernelError> {
        let list = surface_edits::ffi::unstitch(occt(body)?, &face_names(faces)).map_err(failed)?;
        let mut all = shapes(&list).into_iter();
        let rest = all.next().filter(|s| !s.is_null());
        Ok((rest, all.collect()))
    }

    fn reverse_faces(
        &self,
        body: &Self::Shape,
        faces: &[FaceName],
    ) -> Result<Self::Shape, KernelError> {
        surface_edits::ffi::reverse_faces(occt(body)?, &face_names(faces)).map_err(failed)
    }

    // Plastic part features (mitcad#459; plastic.rs).

    fn rim_band(
        &self,
        spec: &mitcad_model::RimBandSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let list = shape_list(&[spec.body])?;
        let band = plastic::ffi::RimBand {
            feature: spec.feature.to_string(),
            face: spec.face.to_string(),
            outside: spec.outside,
            width: spec.width,
            free: spec.free,
            start: spec.start,
            end: spec.end,
        };
        plastic::ffi::rim_band(&list, &band).map_err(failed)
    }

    // Surface modelling: Blend Surface, Boundary Surface, Match Surface and
    // Fair Surface (mitcad#455; surface_fills.rs).

    fn blend_surface(
        &self,
        spec: &mitcad_model::BlendSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let list = shape_list(&[spec.from.edges.body, spec.to.edges.body])?;
        let end = |end: &mitcad_model::BlendEnd<'_, Self::Shape>, body| {
            surface_fills::ffi::BlendEndData {
                chain: surface_fills::chain(&end.edges, body),
                continuity: surface_fills::continuity(end.continuity),
                bulge: end.bulge,
            }
        };
        surface_fills::ffi::blend_surface(
            &spec.feature.to_string(),
            &list,
            &end(&spec.from, 0),
            &end(&spec.to, 1),
        )
        .map_err(failed)
    }

    fn boundary_surface(
        &self,
        spec: &mitcad_model::BoundarySpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        use mitcad_model::BoundaryCurves;
        let (u, v, fill) = match spec.curves {
            BoundaryCurves::Loop(curves) => (curves, &[][..], true),
            BoundaryCurves::Network { u, v } => (u, v, false),
        };
        let mut bodies: Vec<&Self::Shape> = Vec::new();
        let mut pieces = Vec::new();
        let u = surface_fills::boundary_data(u, &mut bodies, &mut pieces);
        let v = surface_fills::boundary_data(v, &mut bodies, &mut pieces);
        let list = shape_list(&bodies)?;
        surface_fills::ffi::boundary_surface(
            &spec.feature.to_string(),
            &list,
            &pieces,
            fill,
            &u,
            &v,
        )
        .map_err(failed)
    }

    fn match_surface(
        &self,
        body: &Self::Shape,
        face: Option<&FaceName>,
        target: &mitcad_model::BodyEdges<'_, Self::Shape>,
        continuity: mitcad_model::PatchContinuity,
    ) -> Result<Self::Shape, KernelError> {
        let list = shape_list(&[target.body])?;
        surface_fills::ffi::match_surface(
            occt(body)?,
            &face.map(ToString::to_string).unwrap_or_default(),
            &list,
            &surface_fills::chain(target, 0),
            surface_fills::continuity(continuity),
        )
        .map_err(failed)
    }

    fn fair_surface(
        &self,
        body: &Self::Shape,
        faces: &[FaceName],
        fairing: mitcad_model::Fairing,
    ) -> Result<Self::Shape, KernelError> {
        surface_fills::ffi::fair_surface(
            occt(body)?,
            &face_names(faces),
            fairing.strength,
            fairing.iterations,
            fairing.keep_edges,
            fairing.tolerance,
        )
        .map_err(failed)
    }

    // Drawings (mitcad#471; drawing.rs).

    fn project_view(
        &self,
        shapes: &[&Self::Shape],
        spec: &mitcad_model::drawing::ViewSpec,
    ) -> Result<mitcad_model::drawing::ViewProjection, KernelError> {
        let list = shape_list(shapes)?;
        drawing::ffi::project_view(&list, &drawing::view(spec))
            .map(|drawn| drawing::projection(&drawn))
            .map_err(failed)
    }

    fn round_holes(
        &self,
        shape: &Self::Shape,
    ) -> Result<Vec<mitcad_model::drawing::RoundHole>, KernelError> {
        drawing::ffi::round_holes(occt(shape)?)
            .map(|holes| holes.iter().map(drawing::hole).collect())
            .map_err(failed)
    }

    // Gears: toothed profiles swept along their axis or ruled to an apex
    // (mitcad#472; gear.rs).

    fn gear_sweep(
        &self,
        spec: &mitcad_model::GearSweepSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        let steps: Vec<gear::ffi::GearStep> = spec
            .steps
            .iter()
            .map(|s| gear::ffi::GearStep {
                length: s.length,
                twist: s.twist,
                shift: s.shift,
            })
            .collect();
        gear::ffi::gear_sweep(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &gear::region_of(spec.loops),
            spec.turn,
            spec.start,
            &steps,
            spec.start_name,
            spec.end_name,
        )
        .map_err(failed)
    }

    fn gear_cone(&self, spec: &mitcad_model::GearConeSpec<'_>) -> Result<Self::Shape, KernelError> {
        gear::ffi::gear_cone(
            &spec.feature.to_string(),
            &profile::frame(&spec.frame),
            &gear::region_of(spec.loops),
            spec.apex,
            spec.scale,
            &gear::loop_of(spec.blank),
        )
        .map_err(failed)
    }

    // Sketch text shaped by the model (mitcad#473; text.rs).

    fn font_source(
        &self,
        request: &mitcad_model::FontRequest<'_>,
    ) -> Result<mitcad_model::FontSource, KernelError> {
        text::font_source(request)
    }

    fn face_cone(
        &self,
        shape: &Self::Shape,
        face: &FaceName,
    ) -> Result<mitcad_model::ConeFace, KernelError> {
        let cone = emboss::ffi::face_cone(occt(shape)?, &face.to_string()).map_err(failed)?;
        Ok(mitcad_model::ConeFace {
            apex: cone.apex,
            axis: cone.axis,
            half_angle: cone.half_angle,
            internal: cone.internal,
        })
    }

    // Faces moved by a rigid motion (mitcad#343; move_faces.rs).

    fn move_faces(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        faces: &[FaceName],
        motion: &Transform,
    ) -> Result<Self::Shape, KernelError> {
        let affine = transform::ffi::Affine {
            linear: std::array::from_fn(|i| motion.linear[i / 3][i % 3]),
            translation: motion.translation,
        };
        move_faces::ffi::move_faces(
            &feature.to_string(),
            occt(body)?,
            &face_names(faces),
            &affine,
        )
        .map_err(failed)
    }

    // A planar face of a body extruded as a profile (mitcad#535;
    // face_extrude.rs).

    fn extrude_face(
        &self,
        spec: &mitcad_model::FaceExtrudeSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let extrusion = face_extrude::ffi::FaceExtrusion {
            reversed: spec.reversed,
            start: spec.start,
            side1: spec.side1,
            two_sides: spec.side2.is_some(),
            side2: spec.side2.unwrap_or(0.0),
            has_direction: spec.direction.is_some(),
            direction: spec.direction.unwrap_or([0.0; 3]),
            taper1: spec.taper1,
            taper2: spec.taper2,
        };
        face_extrude::ffi::extrude_face(
            &spec.feature.to_string(),
            occt(spec.body)?,
            &spec.face.to_string(),
            &spec.key.to_string(),
            &extrusion,
        )
        .map_err(failed)
    }

    // A planar face of a body revolved as a profile (mitcad#539;
    // face_revolve.rs).

    fn revolve_face(
        &self,
        spec: &mitcad_model::FaceRevolveSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let revolution = face_revolve::ffi::FaceRevolution {
            axis_origin: spec.axis.origin,
            axis_direction: spec.axis.direction,
            angle1: spec.angle1,
            two_sides: spec.angle2.is_some(),
            angle2: spec.angle2.unwrap_or(0.0),
        };
        face_revolve::ffi::revolve_face(
            &spec.feature.to_string(),
            occt(spec.body)?,
            &spec.face.to_string(),
            &spec.key.to_string(),
            &revolution,
        )
        .map_err(failed)
    }

    // A planar face of a body swept along a path (mitcad#543; sweeps.rs).

    fn face_sweep(
        &self,
        spec: &mitcad_model::SweepSpec<'_>,
        face: &mitcad_model::sweeps::ProfileFace<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        sweeps::face_sweep(spec, face)
    }

    fn solid_count(&self, shape: &Self::Shape) -> Result<usize, KernelError> {
        Ok(query::ffi::solid_count(occt(shape)?))
    }

    // Boxes of single faces and edges (the faces and edges queries,
    // mitcad#714; query.rs).

    fn face_bounding_boxes(
        &self,
        shape: &Self::Shape,
        faces: &[usize],
    ) -> Result<Vec<Option<BoundingBox>>, KernelError> {
        let found =
            query::ffi::face_bounding_boxes(occt(shape)?, &item_indices(faces)?).map_err(failed)?;
        Ok(found.into_iter().map(item_box).collect())
    }

    fn edge_bounding_boxes(
        &self,
        shape: &Self::Shape,
        edges: &[usize],
    ) -> Result<Vec<Option<BoundingBox>>, KernelError> {
        let found =
            query::ffi::edge_bounding_boxes(occt(shape)?, &item_indices(edges)?).map_err(failed)?;
        Ok(found.into_iter().map(item_box).collect())
    }

    // The extended tool of a split or trim, for its preview (mitcad#712;
    // faceops.rs, geometry's split.hpp).

    fn extended_tool(
        &self,
        tool: &ToolInput<'_, Self::Shape>,
        extend: bool,
        bounds: &BoundingBox,
    ) -> Result<Self::Shape, KernelError> {
        let mut tool = Tool::new(tool)?;
        tool.spec.extend = extend;
        faceops::ffi::extended_tool(
            &tool.spec,
            &tool.shapes,
            &tool.frame,
            &tool.regions,
            bounds.min,
            bounds.max,
        )
        .map_err(failed)
    }

    // Crossing points of a sketch's Intersect (mitcad#736; sketch_ref.rs,
    // geometry's sketch_ref.hpp).

    fn section_points(
        &self,
        shape: &Self::Shape,
        edge: &EdgeName,
        plane: &DatumPlane,
    ) -> Result<Vec<[f64; 3]>, KernelError> {
        let found = sketch_ref::ffi::section_points(
            occt(shape)?,
            &edge.to_string(),
            plane.origin,
            plane.normal(),
        )
        .map_err(failed)?;
        Ok(found.chunks(3).map(|p| [p[0], p[1], p[2]]).collect())
    }

    // Features evaluated on worker threads (the recompute, mitcad#842): OCCT's
    // crash handlers on the thread, as on the import's workers.

    fn worker_thread<R>(&self, f: impl FnOnce() -> R) -> R {
        exchange::ffi::catch_occt_crashes();
        f()
    }

    // Warm opens from the result store (mitcad#858; display_mesh.rs).

    fn display_mesh_bytes(&self, shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        let bytes = display_mesh::ffi::display_mesh_bytes(occt(shape)?).map_err(failed)?;
        Ok(bytes.as_bytes().to_vec())
    }

    fn restore_display_mesh(
        &self,
        shape: &Self::Shape,
        bytes: &[u8],
    ) -> Result<usize, KernelError> {
        let faces = display_mesh::ffi::restore_display_mesh(occt(shape)?, bytes).map_err(failed)?;
        Ok(usize::try_from(faces).unwrap_or(usize::MAX))
    }

    fn shape_unpacker(
        &self,
    ) -> Result<mitcad_model::kernel::ShapeUnpacker<Self::Shape>, KernelError> {
        Ok(mitcad_model::kernel::ShapeUnpacker {
            unpack: |bytes| persist::ffi::shape_from_bytes(bytes).map_err(failed),
            // Never read: a body's state holds it only until the shape is
            // made (and the kernel's operations refuse a null shape).
            stand_in: SharedPtr::null,
        })
    }

    // Feature inputs drawn where they were (mitcad#957; input_edges.rs,
    // geometry's input_edges.hpp).

    fn edge_polylines(
        &self,
        shape: &Self::Shape,
        edges: &[EdgeName],
        faces: &[FaceName],
        deflection: f64,
        angle: f64,
    ) -> Result<Vec<Vec<[f64; 3]>>, KernelError> {
        let lines = input_edges::ffi::edge_polylines(
            occt(shape)?,
            &names(edges),
            &face_names(faces),
            deflection,
            angle,
        )
        .map_err(failed)?;
        Ok(lines
            .into_iter()
            .map(|line| line.points.chunks(3).map(|p| [p[0], p[1], p[2]]).collect())
            .collect())
    }

    // Dogbones: relief cuts at a body's inside corners (mitcad#1011;
    // dogbone.rs, geometry's dogbone.hpp).

    fn dogbone_tool(
        &self,
        spec: &mitcad_model::kernel::DogboneSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        let dogbone = dogbone::ffi::Dogbone {
            feature: spec.feature.to_string(),
            edges: names(spec.edges),
            faces: face_names(spec.faces),
            axis: spec.axis,
            diameter: spec.diameter,
            extra: spec.extra,
            along_side: spec.kind == mitcad_model::kernel::DogboneKind::AlongSide,
        };
        dogbone::ffi::dogbone_tool(occt(spec.body)?, &dogbone).map_err(failed)
    }
}

/// Face or edge indices for the bridge (mitcad#714).
fn item_indices(items: &[usize]) -> Result<Vec<u32>, KernelError> {
    items
        .iter()
        .map(|&i| u32::try_from(i).map_err(|_| KernelError::failed(format!("no item {i}"))))
        .collect()
}

fn item_box(found: query::ffi::BoundingBox) -> Option<BoundingBox> {
    (!found.empty).then_some(BoundingBox {
        min: found.min,
        max: found.max,
    })
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

fn section_of(section: &analysis::ffi::AnalysisSection) -> Section<SharedPtr<Shape>> {
    Section {
        curves: section.curves(),
        faces: section.faces(),
        edge_count: section.edge_count(),
        face_count: section.face_count(),
        length: section.length(),
        area: section.area(),
    }
}

/// `clip_all` (mitcad#438): the clips on all cores; one by one when the
/// shapes cannot be listed.
fn clip_all_on_cores(
    kernel: &OcctKernel,
    items: &[(&SharedPtr<Shape>, DatumPlane)],
) -> Vec<Result<SharedPtr<Shape>, KernelError>> {
    let shapes: Vec<&SharedPtr<Shape>> = items.iter().map(|(shape, _)| *shape).collect();
    let Ok(list) = shape_list(&shapes) else {
        return items
            .iter()
            .map(|(shape, plane)| kernel.clip(shape, plane))
            .collect();
    };
    let planes: Vec<_> = items
        .iter()
        .map(|(_, plane)| analysis_plane(plane))
        .collect();
    match analysis::ffi::analysis_clip_all(&list, &planes) {
        Ok(clips) => items
            .iter()
            .enumerate()
            .map(|(i, (shape, _))| {
                let error = clips.error(i);
                if !error.is_empty() {
                    return Err(failed_with(&error));
                }
                let clipped = clips.shape(i);
                // Null: the body whole, as `clip` gives it.
                Ok(if clipped.is_null() {
                    (*shape).clone()
                } else {
                    clipped
                })
            })
            .collect(),
        Err(error) => {
            let error = failed(error);
            items.iter().map(|_| Err(error.clone())).collect()
        }
    }
}
