// SPDX-License-Identifier: MIT
//! The geometry kernel interface. The model never touches B-rep data itself;
//! features call these operations, and the OCCT implementation lives in the
//! bridge (`core/ffi/src/kernel/`). Methods are grouped by operation family.
//! A new family gets methods with a default `Err(KernelError::Unsupported)`,
//! so test kernels only implement what their tests use.

use std::fmt;
use std::sync::Arc;

use crate::analysis::{
    CompareOptions, Comparison, Interference, Measurement, PhysicalProperties, Section, Selection,
    Separation,
};
use crate::datum::{
    CurveGeometry, Datum, DatumPlane, PathParameter, PathPoint, SurfaceGeometry, SurfacePoint, Vec3,
};
use crate::exchange::{BodyKind, ExportBody, ExportOptions, ImportBody, ReadOptions};
use crate::ids::{EntityUid, FeatureUid};
use crate::profile::{ProfileRegion, SketchFrame};
use crate::topo::{EdgeName, FaceName, TopoName, VertexName};
use crate::transform::{Instance, PrimitiveSpec, Transform};
// Sweeps, lofts, pipes, coils, ribs and webs (F3).
use crate::sweeps::{CoilSpec, LoftSpec, PipeSpec, RibSpec, SweepSpec};
// Cancellation inside an operation (P7e).
use crate::monitor::RecomputeMonitor;
// Triangle meshes for 3D printing (3MF export, mitcad#13).
use crate::exchange::{MeshTolerance, TriangleMesh};
// FreeCAD helices (mitcad#4).
use crate::sweeps::HelixSpec;

/// A kernel goes to the threads that evaluate features beside a recompute
/// or an import ([`Kernel::fork`], mitcad#95, mitcad#842), so it is `Send`,
/// and those threads share its shapes, so they are `Send + Sync`.
pub trait Kernel: Send {
    /// A kernel-owned shape with face names (see [`crate::topo`]). Cloning
    /// must be cheap (a shared handle); shapes are never modified.
    type Shape: Clone + Send + Sync;

    // Named material domains, exact trimming and chart reconstruction.
    fn sheet_trim_domain(
        &self,
        _spec: &crate::sheet_charts::SheetTrimSpec<'_>,
    ) -> Result<crate::sheet_charts::SheetTrimResult, KernelError> {
        Err(KernelError::Unsupported("sheet domain trimming"))
    }

    fn sheet_shape(
        &self,
        _spec: &crate::sheet_charts::SheetShapeSpec<'_>,
        _prefix: &str,
    ) -> Result<crate::sheet_charts::SheetGeometry<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("sheet shape construction"))
    }

    // Profiles and extrusion.

    /// Sweeps the regions along the direction between the offsets; faces
    /// are named `<feature>:side(<segment>)`, `start(<region>)` and
    /// `end(<region>)`. Touching regions fuse; the result may hold several
    /// solids.
    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<Self::Shape, KernelError>;

    /// The faces of regions, for display.
    fn profile(
        &self,
        _frame: &SketchFrame,
        _regions: &[ProfileRegion],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("profile display"))
    }

    // Solids and booleans.

    /// The solids of a shape with their face names, in a deterministic
    /// geometric order.
    fn solids(&self, shape: &Self::Shape) -> Result<Vec<Self::Shape>, KernelError>;

    /// Join, Cut or Intersect of a tool with target bodies (see
    /// [`BooleanOutput`]).
    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&Self::Shape],
        tool: &Self::Shape,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError>;

    // Fillets and chamfers (edge sets, F2).

    /// Rounds the edge sets. New faces are named `<feature>:fillet(<edge>)`
    /// (also along the edges a tangent chain adds) and
    /// `<feature>:corner(<vertex>)`. Without rolling ball corners, a vertex
    /// where three rounded edges meet gets a setback corner. Asymmetric and
    /// curvature continuous sets whose chains meet other rounded edges are
    /// unsupported.
    fn fillet(
        &self,
        feature: FeatureUid,
        body: &Self::Shape,
        sets: &[FilletSet<'_>],
        rolling_ball_corners: bool,
    ) -> Result<Self::Shape, KernelError>;

    /// Bevels the edge sets; new faces are `<feature>:chamfer(<edge>)` and
    /// `<feature>:corner(<vertex>)`. The corner type shapes the vertices
    /// where three or more bevelled edges meet (a miter corner has no face
    /// of its own).
    fn chamfer(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _sets: &[Chamfer<'_>],
        _corner: ChamferCorner,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("chamfer"))
    }

    // Face operations (F2): shell, draft, offset, delete, replace, split.
    // Errors whose message starts with "unsupported:" mark options the
    // kernel lacks (the importer falls back to the stored body).

    /// Hollows the body. The outside of the wall keeps the face names; new
    /// faces are `<feature>:offset(<face>)` inside the wall,
    /// `<feature>:offset_cap(<face>)` where a removed face was, and
    /// `<feature>:offset(<edge or vertex>)` for rounded joins.
    fn shell(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _spec: &ShellSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("shell"))
    }

    /// Tilts faces about the fixed plane (see [`DraftSpec`]); they keep
    /// their names, pieces split at the plane get `#k`.
    fn draft(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _spec: &DraftSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("draft"))
    }

    /// Moves faces along their normals (positive out of the material); the
    /// neighbours follow along their own surfaces. The faces keep their names.
    fn offset_faces(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _distance: f64,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("offset faces"))
    }

    /// Removes faces and heals the body by extending their neighbours.
    fn delete_faces(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("delete face"))
    }

    /// Replaces faces with a target (a plane, a face or a body's faces, of
    /// any surface type); the neighbours extend or shorten to it, and the
    /// new face is `<feature>:replace(<face>)`.
    fn replace_faces(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _target: &ToolInput<'_, Self::Shape>,
        _tangent_chain: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("replace face"))
    }

    /// Splits the body into solids, ordered along a planar tool's normal
    /// (else geometrically); the cut faces are `<feature>:split` or
    /// `<feature>:split(<tool face>)`. Fails unless the tool divides it.
    fn split_body(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _tool: &ToolInput<'_, Self::Shape>,
        _extend: bool,
    ) -> Result<Vec<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("split body"))
    }

    /// Splits faces along the tool; the pieces keep the name with `#k`.
    fn split_faces(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _tool: &ToolInput<'_, Self::Shape>,
        _extend: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("split face"))
    }

    // Queries.

    /// The number of edges a name resolves to: all edges between matching
    /// faces, or one with `#k`.
    fn count_edges(&self, shape: &Self::Shape, edge: &EdgeName) -> usize;

    /// The number of faces a name resolves to (all pieces without `#k`).
    fn count_faces(&self, _shape: &Self::Shape, _face: &FaceName) -> Result<usize, KernelError> {
        Err(KernelError::Unsupported("face lookup"))
    }

    fn faces(&self, _shape: &Self::Shape) -> Result<Vec<FaceInfo>, KernelError> {
        Err(KernelError::Unsupported("face listing"))
    }

    fn edges(&self, _shape: &Self::Shape) -> Result<Vec<EdgeInfo>, KernelError> {
        Err(KernelError::Unsupported("edge listing"))
    }

    fn mass_properties(&self, _shape: &Self::Shape) -> Result<MassProperties, KernelError> {
        Err(KernelError::Unsupported("mass properties"))
    }

    /// None for an empty shape.
    fn bounding_box(&self, _shape: &Self::Shape) -> Result<Option<BoundingBox>, KernelError> {
        Err(KernelError::Unsupported("bounding boxes"))
    }

    /// A box that holds the shape, not necessarily the tight one: for
    /// tests of whether shapes can meet. Kernels where the tight box is
    /// costly on bodies of many curved faces give a cheaper one.
    fn enclosing_box(&self, shape: &Self::Shape) -> Result<Option<BoundingBox>, KernelError> {
        self.bounding_box(shape)
    }

    // Imported bodies and data exchange (base features, import, export).

    /// A body from B-rep data ([`Kernel::brep_data`]) with face i named
    /// `<feature>:import(<first_face + i>)` in the kernel's face order, and
    /// its number of faces.
    fn import_brep(
        &self,
        _feature: FeatureUid,
        _data: &[u8],
        _first_face: u32,
    ) -> Result<(Self::Shape, u32), KernelError> {
        Err(KernelError::Unsupported("imported bodies"))
    }

    /// The B-rep data of a shape (without face names).
    fn brep_data(&self, _shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        Err(KernelError::Unsupported("B-rep data"))
    }

    /// One shape holding several, with their face names.
    fn compound(&self, _shapes: &[Self::Shape]) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("compounds"))
    }

    fn body_kind(&self, _shape: &Self::Shape) -> Result<BodyKind, KernelError> {
        Err(KernelError::Unsupported("body kinds"))
    }

    /// The bodies of a STEP, IGES, BRep, STL or OBJ file (by extension), in
    /// millimetres.
    fn read_file(
        &self,
        _path: &str,
        _options: &ReadOptions,
    ) -> Result<Vec<ImportBody<Self::Shape>>, KernelError> {
        Err(KernelError::Unsupported("reading files"))
    }

    /// Writes bodies to a file of `options.format`, each once per placement
    /// ([`ExportBody::placements`]; STEP as the instances of a part in an
    /// assembly) or once as it is without placements.
    fn write_file(
        &self,
        _path: &str,
        _bodies: &[ExportBody<'_, Self::Shape>],
        _options: &ExportOptions,
    ) -> Result<(), KernelError> {
        Err(KernelError::Unsupported("writing files"))
    }

    // Transforms, patterns, mirrors, combine and primitives (F4).

    /// A copy of the shape mapped by `transform`: a rigid motion, a mirror
    /// or a scale (a non-uniform scale turns curved faces into B-splines).
    /// Face names stay, or `instance` renames every face (see [`Instance`]).
    fn transform_shape(
        &self,
        _shape: &Self::Shape,
        _transform: &Transform,
        _instance: Option<Instance>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("transforms"))
    }

    /// The shape moved by `transform` for a Move (mitcad#838): a rotation
    /// and a translation as a location on the same shape, whose faces keep
    /// the display meshes they have, so the view meshes nothing again; the
    /// input is not changed. A kernel without locations moves a copy, as
    /// [`Kernel::transform_shape`] does (the same geometry and names).
    fn move_shape(
        &self,
        shape: &Self::Shape,
        transform: &Transform,
        instance: Option<Instance>,
    ) -> Result<Self::Shape, KernelError> {
        self.transform_shape(shape, transform, instance)
    }

    /// The union of the shapes with their face names: one shape of one or
    /// more solids (disjoint shapes stay separate solids).
    fn unite(&self, _shapes: &[&Self::Shape]) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("unions"))
    }

    /// The closed solid that faces of a body bound together with planar
    /// caps across the openings where they meet the rest of the body, for
    /// patterns and mirrors of faces. The faces keep their names. Join when
    /// the solid is material of the body (a boss), Cut when it is empty (a
    /// pocket or a hole).
    fn face_tool(
        &self,
        _body: &Self::Shape,
        _faces: &[FaceName],
    ) -> Result<(Self::Shape, BooleanOp), KernelError> {
        Err(KernelError::Unsupported("patterns of faces"))
    }

    /// A box, cylinder, sphere or torus with faces `<feature>:bottom`, `top`
    /// and `side<i>` (see [`PrimitiveShape`]).
    fn primitive(&self, _spec: &PrimitiveSpec) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("primitives"))
    }

    // Construction geometry (F5): analytic descriptions of named faces,
    // edges and vertices for datums and references (see `crate::datum`).
    // A name must resolve to one sub-shape, except that the pieces of a
    // split face may be named together when they lie on one surface.

    fn face_geometry(
        &self,
        _shape: &Self::Shape,
        _face: &FaceName,
    ) -> Result<SurfaceGeometry, KernelError> {
        Err(KernelError::Unsupported("face geometry"))
    }

    fn edge_geometry(
        &self,
        _shape: &Self::Shape,
        _edge: &EdgeName,
    ) -> Result<CurveGeometry, KernelError> {
        Err(KernelError::Unsupported("edge geometry"))
    }

    fn vertex_point(
        &self,
        _shape: &Self::Shape,
        _vertex: &VertexName,
    ) -> Result<Vec3, KernelError> {
        Err(KernelError::Unsupported("vertex positions"))
    }

    /// A point of a path: the edges joined in the order given, running from
    /// the free end of the first edge.
    fn path_point(
        &self,
        _shape: &Self::Shape,
        _edges: &[EdgeName],
        _at: PathParameter,
    ) -> Result<PathPoint, KernelError> {
        Err(KernelError::Unsupported("paths"))
    }

    /// The point of the face (any piece of the name) nearest to `near`,
    /// with the outward normal there.
    fn face_point_normal(
        &self,
        _shape: &Self::Shape,
        _face: &FaceName,
        _near: Vec3,
    ) -> Result<SurfacePoint, KernelError> {
        Err(KernelError::Unsupported("face normals"))
    }

    /// A shape showing a datum: a square face of side `size` centred on a
    /// plane's origin, an edge of length `size` centred on an axis's
    /// origin, or a vertex.
    fn datum_shape(&self, _datum: &Datum, _size: f64) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("datum display"))
    }

    // Analysis (F5): physical properties, measurement, interference,
    // sections and comparison (see `crate::analysis` for units).

    /// Density in g/cm³.
    fn physical_properties(
        &self,
        _shape: &Self::Shape,
        _density: f64,
    ) -> Result<PhysicalProperties, KernelError> {
        Err(KernelError::Unsupported("physical properties"))
    }

    fn measure(&self, _selection: &Selection<'_, Self::Shape>) -> Result<Measurement, KernelError> {
        Err(KernelError::Unsupported("measurement"))
    }

    fn measure_between(
        &self,
        _a: &Selection<'_, Self::Shape>,
        _b: &Selection<'_, Self::Shape>,
    ) -> Result<Separation, KernelError> {
        Err(KernelError::Unsupported("measurement"))
    }

    /// Symmetric Hausdorff distance of two bounded convex planar polygon
    /// faces. `None` means the exact maximum is unavailable for this geometry.
    /// The minimum distance remains available through `measure_between`.
    fn maximum_face_gap(
        &self,
        _a: &Selection<'_, Self::Shape>,
        _b: &Selection<'_, Self::Shape>,
    ) -> Result<Option<f64>, KernelError> {
        Err(KernelError::Unsupported("maximum face gap"))
    }

    /// Every pair of bodies that overlap by more than `min_volume` (mm³);
    /// bodies that only touch do not interfere.
    fn interferences(
        &self,
        _bodies: &[&Self::Shape],
        _min_volume: f64,
    ) -> Result<Vec<Interference<Self::Shape>>, KernelError> {
        Err(KernelError::Unsupported("interference"))
    }

    fn section(
        &self,
        _shape: &Self::Shape,
        _plane: &DatumPlane,
    ) -> Result<Section<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("sections"))
    }

    /// The shape without the half-space on the plane's normal side, as
    /// section analysis shows it; the shape itself when the plane leaves it
    /// whole.
    fn clip(&self, _shape: &Self::Shape, _plane: &DatumPlane) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("sections"))
    }

    /// The shapes together (A) against the bodies of a STEP file (B).
    fn compare_step(
        &self,
        _shapes: &[&Self::Shape],
        _path: &str,
        _options: &CompareOptions,
    ) -> Result<Comparison, KernelError> {
        Err(KernelError::Unsupported("comparison with STEP files"))
    }

    // Profile features (F1): extrude options, revolve, hole, thread, and the
    // geometry of the faces and edges they refer to.

    /// The extrude feature (see [`ExtrudeFeatureSpec`]). The default
    /// handles what [`Kernel::extrude`] can: distances from a start offset,
    /// without tapers or thin walls.
    fn extrude_feature(
        &self,
        spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        simple_extrude(self, spec)
    }

    /// Turns regions about an axis (see [`RevolveSpec`]).
    fn revolve(&self, _spec: &RevolveSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("revolve"))
    }

    /// The tool a hole feature cuts with (see [`HoleSpec`]).
    fn hole_tool(&self, _spec: &HoleSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("holes"))
    }

    /// The body with threads built on cylindrical faces (see [`ThreadSpec`]).
    fn modeled_thread(
        &self,
        _body: &Self::Shape,
        _spec: &ThreadSpec<'_>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("modelled threads"))
    }

    /// The middle of a planar face (all pieces of a split one) on its
    /// plane, with the normal pointing out of the material: where holes
    /// start and alignments put a face. Frames of faces come from
    /// [`Kernel::face_geometry`] and `DatumPlane::on_plane`.
    fn face_plane(&self, _shape: &Self::Shape, _face: &FaceName) -> Result<Plane, KernelError> {
        Err(KernelError::Unsupported("face planes"))
    }

    fn face_cylinder(
        &self,
        _shape: &Self::Shape,
        _face: &FaceName,
    ) -> Result<Cylinder, KernelError> {
        Err(KernelError::Unsupported("face cylinders"))
    }

    // Projected geometry (S1).

    /// The curves of what a name resolves to, in model space: an edge's
    /// curve (every edge for a name without `#k`), a face's boundary edges
    /// or a vertex's point. Empty when the shape has nothing by that name.
    fn curves_of(
        &self,
        _shape: &Self::Shape,
        _name: &TopoName,
    ) -> Result<Vec<Curve3>, KernelError> {
        Err(KernelError::Unsupported("edge geometry"))
    }

    // .f3d import (T1): replayed bodies against the file's.

    /// The distance of each point from the shape's boundary (its faces),
    /// mm: zero on a face, positive inside and outside; within 1e-5 of a
    /// face, the distance to that face (another may be nearer still).
    fn boundary_distances(
        &self,
        _shape: &Self::Shape,
        _points: &[Vec3],
    ) -> Result<Vec<f64>, KernelError> {
        Err(KernelError::Unsupported("distances to faces"))
    }

    /// Whether each point lies inside the shape's solids (on the boundary
    /// counts as outside).
    fn points_inside(
        &self,
        _shape: &Self::Shape,
        _points: &[Vec3],
    ) -> Result<Vec<bool>, KernelError> {
        Err(KernelError::Unsupported("point classification"))
    }

    /// The number of faces of a shape, without measuring them (T1: body
    /// signatures).
    fn face_count(&self, shape: &Self::Shape) -> Result<usize, KernelError> {
        self.faces(shape).map(|f| f.len())
    }

    /// Every named edge's middle (half its length along it from its
    /// start) with the unit tangent there and its length, in the shape's
    /// edge order (T1d: the edges a fillet consumed). The default asks
    /// [`Kernel::path_point`] edge by edge.
    fn edge_middles(&self, shape: &Self::Shape) -> Result<Vec<EdgeMiddle>, KernelError> {
        let mut middles = Vec::new();
        for edge in self.edges(shape)? {
            let Some(name) = edge.name else {
                continue;
            };
            let Ok(parsed) = name.parse::<EdgeName>() else {
                continue;
            };
            let at = PathParameter::Fraction(0.5);
            if let Ok(p) = self.path_point(shape, std::slice::from_ref(&parsed), at) {
                middles.push(EdgeMiddle {
                    name,
                    point: p.point,
                    tangent: p.tangent,
                    length: edge.length,
                });
            }
        }
        Ok(middles)
    }

    /// The shapes `a` together against the shapes `b` together, as
    /// [`Kernel::compare_step`] compares with a file (`step_bodies` stays
    /// empty).
    fn compare_shapes(
        &self,
        _a: &[&Self::Shape],
        _b: &[&Self::Shape],
        _options: &CompareOptions,
    ) -> Result<Comparison, KernelError> {
        Err(KernelError::Unsupported("comparison of bodies"))
    }

    // Sweeps, lofts, pipes, coils, ribs and webs (F3; inputs in
    // `crate::sweeps`).

    /// Profiles swept along a path (see [`SweepSpec`]).
    fn sweep(&self, _spec: &SweepSpec<'_>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("sweeps"))
    }

    /// A solid through sections (see [`LoftSpec`]).
    fn loft(&self, _spec: &LoftSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("lofts"))
    }

    /// A section swept along a path (see [`PipeSpec`]).
    fn pipe(&self, _spec: &PipeSpec<'_>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("pipes"))
    }

    /// A helix or a spiral with a section (see [`CoilSpec`]).
    fn coil(&self, _spec: &CoilSpec) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("coils"))
    }

    /// The tool of a rib or a web (see [`RibSpec`]).
    fn rib(&self, _spec: &RibSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("ribs and webs"))
    }

    // Sketch text (P3).

    /// The glyph outlines of a text's characters (see [`FontGlyphs`]).
    fn font_glyphs(&self, _request: &FontRequest<'_>) -> Result<FontGlyphs, KernelError> {
        Err(KernelError::Unsupported("sketch text"))
    }

    // Model warnings (P9).

    /// What the geometry gave up to build a shape (a fillet made 0.1 %
    /// smaller than asked); the feature shows them as warnings, yellow in
    /// the timeline. None for most shapes.
    fn notes(&self, _shape: &Self::Shape) -> Vec<String> {
        Vec::new()
    }

    // Replace face of curved faces (P5): its .f3d import.

    /// Points inside every named face, spread over it, in the shape's face
    /// order: the faces whose points the next history state no longer has
    /// on its faces are the ones a replace face replaced.
    fn face_points(&self, _shape: &Self::Shape) -> Result<Vec<FacePoints>, KernelError> {
        Err(KernelError::Unsupported("points inside faces"))
    }

    // The result store (P7d): computed shapes kept on disk.

    /// The shape as bytes with its face names, notes and what it measured
    /// of itself, for [`Kernel::shape_from_bytes`] of the same build.
    fn shape_bytes(&self, _shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        Err(KernelError::Unsupported("storing shapes"))
    }

    /// A shape from [`Kernel::shape_bytes`]; damaged bytes are an error.
    fn shape_from_bytes(&self, _bytes: &[u8]) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("reading stored shapes"))
    }

    /// The memory a shape takes, estimated in bytes, for the memory cache's
    /// budget; parts it shares with other shapes are counted too. 0 when
    /// the kernel cannot tell.
    fn shape_memory(&self, _shape: &Self::Shape) -> u64 {
        0
    }

    // Cancellation inside an operation (P7e).

    /// Runs `f`, one evaluation of a recompute with a monitor: the kernel's
    /// long operations on this thread may stop early once `monitor` is
    /// cancelled, failing; the recompute then drops what the evaluation
    /// returned (`crate::monitor`). Kernels whose operations do not stop
    /// just run `f`.
    fn interruptible<R>(&self, _monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
        f()
    }

    // FreeCAD import (.FCStd): elements by FreeCAD's index names.

    /// The face, edge or vertex `index` (from 0) of a shape in OCCT's
    /// `TopExp::MapShapes` order, which FreeCAD's `Face<n>`, `Edge<n>` and
    /// `Vertex<n>` count from 1: its name (None where its faces have none)
    /// and its curves, as [`Kernel::curves_of`] gives them for the name (a
    /// face's boundary edges, an edge's curve, a vertex's point). None when
    /// the shape has fewer.
    fn indexed_element(
        &self,
        _shape: &Self::Shape,
        _kind: ElementKind,
        _index: usize,
    ) -> Result<Option<IndexedElement>, KernelError> {
        Err(KernelError::Unsupported("elements by index"))
    }

    // Triangle meshes for 3D printing (3MF export, mitcad#13).

    /// The body as one triangle mesh in millimetres, triangulated within
    /// the tolerance without changing the shape: vertices shared by the
    /// triangles that meet there, so a solid's mesh is closed, and triangles
    /// counter-clockwise seen from outside. A mesh body's own triangles as
    /// they are.
    fn triangle_mesh(
        &self,
        _shape: &Self::Shape,
        _tolerance: &MeshTolerance,
    ) -> Result<TriangleMesh, KernelError> {
        Err(KernelError::Unsupported("triangle meshes"))
    }

    // FreeCAD helices (mitcad#4; the input in `crate::sweeps`).

    /// A profile turned about an axis as it moves along it (see
    /// [`HelixSpec`]).
    fn helix(&self, _spec: &HelixSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("helices"))
    }

    // Segment crossings (the `.f3d` import's thread angles, mitcad#68).

    /// Where the segment from `from` to `to` crosses the shape's faces, in
    /// order along it: the fraction of the way, and whether it enters the
    /// material there (else it leaves it). Where it only touches a face is
    /// left out. Much quicker than classifying points near spline faces
    /// ([`Kernel::points_inside`]): only the faces near the segment are
    /// intersected.
    fn segment_crossings(
        &self,
        _shape: &Self::Shape,
        _from: Vec3,
        _to: Vec3,
    ) -> Result<Vec<(f64, bool)>, KernelError> {
        Err(KernelError::Unsupported("segment crossings"))
    }

    // Removed material (the import's history-based guesses, mitcad#85).

    /// `before` minus `after` for two near copies of a body (a replayed
    /// body and the one stored for its next history state), as the pieces
    /// of [`Kernel::boolean`]'s cut: worked out only where the two differ
    /// by more than `slack` mm, so that the faces both have, nearly but not
    /// exactly, are not intersected (a boolean of the whole bodies can take
    /// minutes on free-form faces). Material within `slack` of the faces
    /// both have is left out.
    fn removed_material(
        &self,
        _before: &Self::Shape,
        _after: &Self::Shape,
        _slack: f64,
    ) -> Result<Vec<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("removed material"))
    }

    // Near copies joined (mirrors of nearly symmetric bodies, mitcad#88).

    /// The join of `body` with a near copy of it (the mirror image of a
    /// nearly symmetric body), as [`Kernel::boolean`]'s join of the one
    /// target `body`: worked out only where the two differ by more than
    /// `slack` mm, so that the faces both have, nearly but not exactly, are
    /// not intersected (a boolean of the whole shapes can take minutes on
    /// free-form faces and often goes wrong there). Material of the copy
    /// within `slack` of the body's faces is left out; where no face
    /// differs by more, the result is the body. None when `copy` is not a
    /// near copy of `body` (more than half the faces of either differ from
    /// the other's): the caller joins the whole shapes.
    fn join_near_copy(
        &self,
        _body: &Self::Shape,
        _copy: &Self::Shape,
        _slack: f64,
    ) -> Result<Option<BooleanOutput<Self::Shape>>, KernelError> {
        Err(KernelError::Unsupported("joins of near copies"))
    }

    // Every face's and edge's geometry at once (the `.f3d` import's joint
    // sides on large bodies, mitcad#87).

    /// Each face's names (as [`Kernel::faces`]) and surface (as
    /// [`Kernel::face_geometry`], but `Other` for surfaces that are not
    /// planes, cylinders, cones, spheres or tori, planar splines too), in
    /// one pass instead of finding each face by its name.
    fn face_geometries(
        &self,
        _shape: &Self::Shape,
    ) -> Result<Vec<(Vec<String>, SurfaceGeometry)>, KernelError> {
        Err(KernelError::Unsupported("face geometries"))
    }

    /// Each edge's name (as [`Kernel::edges`]) and curve (as
    /// [`Kernel::edge_geometry`]; `Other` for degenerate edges), in one
    /// pass.
    fn edge_geometries(
        &self,
        _shape: &Self::Shape,
    ) -> Result<Vec<(Option<String>, CurveGeometry)>, KernelError> {
        Err(KernelError::Unsupported("edge geometries"))
    }

    // Definitions evaluated in parallel (the .f3d import, mitcad#95).

    /// A kernel for another thread that evaluates features on this
    /// kernel's shapes at the same time as this one (the model's
    /// `Document::fork`):
    /// its own settings and state, the same geometry. None (the default)
    /// when the kernel cannot work on another thread: the caller then
    /// evaluates on this one.
    fn fork(&self) -> Option<Self>
    where
        Self: Sized,
    {
        None
    }

    // A shape's validity (the `.ipt` import's replayed bodies, mitcad#60).

    /// Whether the geometry kernel's checker finds the shape valid.
    fn is_valid(&self, _shape: &Self::Shape) -> Result<bool, KernelError> {
        Err(KernelError::Unsupported("validity checks"))
    }

    // An operation's new faces (the `.f3d` import's geometric check of an
    // item's change, mitcad#138).

    /// The faces of `after` that none of the shapes `before` has: the faces
    /// an operation made or changed. A face counts as one `before` has when
    /// it is the same face, or one on the same kind of surface with the same
    /// area and centre (a body rebuilt from the same data). Each with its
    /// area and up to `count` points spread over it (the middles of the
    /// cells of a grid over its parameters that lie inside it), in the
    /// shape's face order.
    fn new_faces(
        &self,
        _before: &[&Self::Shape],
        _after: &Self::Shape,
        _count: usize,
    ) -> Result<Vec<NewFace>, KernelError> {
        Err(KernelError::Unsupported("new faces"))
    }

    // Measures of other programs' integration (the FreeCAD import's
    // reference check, mitcad#139).

    /// The volume, area and centre a shape has with the geometry kernel's
    /// plain fixed-point integration of every face, as other programs on
    /// the same kernel measure it ([`Kernel::mass_properties`] integrates
    /// some faces more exactly): the volume of its solids (each positive),
    /// the centre of the solids by volume, else of the faces by area.
    fn fixed_point_properties(&self, _shape: &Self::Shape) -> Result<MassProperties, KernelError> {
        Err(KernelError::Unsupported("fixed-point measures"))
    }

    // Sweeps along guide surfaces (mitcad#157; the input in `crate::sweeps`).

    /// A sweep whose profile turns with the normal of the faces its path
    /// lies on (see [`crate::sweeps::GuideSurface`]); `spec.guide` is None
    /// and its twist and taper are not used.
    fn surface_sweep(
        &self,
        _spec: &SweepSpec<'_>,
        _surfaces: &[crate::sweeps::GuideSurface<'_, Self::Shape>],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("sweeps along guide surfaces"))
    }

    // Shaded meshes of faces (USDZ export, mitcad#123).

    /// The body's faces triangulated within the tolerance without changing
    /// the shape, for display: each face its own vertices with unit normals
    /// out of the body (the surface's), so edges stay sharp and curved
    /// faces smooth; triangles counter-clockwise seen from outside, face by
    /// face in the order [`Kernel::faces`] lists the faces. A mesh body's
    /// triangles faceted, each with its own vertices.
    fn face_meshes(
        &self,
        _shape: &Self::Shape,
        _tolerance: &MeshTolerance,
    ) -> Result<crate::exchange::FaceMeshes, KernelError> {
        Err(KernelError::Unsupported("face meshes"))
    }

    // Crossings of several segments (the `.f3d` import's probes, mitcad#181).

    /// [`Kernel::segment_crossings`] of several segments at once (each
    /// face's set-up made once for all of them), in their order.
    fn segments_crossings(
        &self,
        _shape: &Self::Shape,
        _segments: &[(Vec3, Vec3)],
    ) -> Result<Vec<Vec<(f64, bool)>>, KernelError> {
        Err(KernelError::Unsupported("crossings of several segments"))
    }

    // The faces a run of features made, copied by a pattern's optimized
    // compute option.

    /// What features did to a body: the material `after` has and `before`
    /// lacks, and the material `before` has and `after` lacks, as closed
    /// solids bounded by the faces the features made (the unchanged faces
    /// of the two bodies coincide and are the solids' seam with the body).
    /// Joined to (or cut from) a body at another place, a moved solid
    /// gives that body the features' faces there. Face names carry through
    /// from both bodies; slivers along the faces the two share are left
    /// out. Both empty when the bodies do not differ.
    fn feature_changes(
        &self,
        _before: &Self::Shape,
        _after: &Self::Shape,
    ) -> Result<FeatureChanges<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("the faces of features"))
    }

    /// The parts of copies of added material that stand on `body`: each
    /// copy is a solid with its seam moved to the same place
    /// ([`AddedMaterial`]). The material over a part of a planar seam
    /// face that no planar face of the body lies under (coplanar, facing
    /// it) is cut off, along the seam's normal; a copy whose seam lies
    /// wholly on the body's faces is as it is, one with nothing under it
    /// is None; a copy without a seam is as it is.
    fn supported_copies(
        &self,
        _body: &Self::Shape,
        _copies: &[(&Self::Shape, Option<&Self::Shape>)],
    ) -> Result<Vec<Option<Self::Shape>>, KernelError> {
        Err(KernelError::Unsupported("copies standing on a body"))
    }

    // Face boxes (the `.f3d` import's face lookups through a point,
    // mitcad#119).

    /// A box (minimum and maximum corner, mm) that holds each face of the
    /// shape, in the order of [`Kernel::faces`]: a point farther than a
    /// distance from a face's box is farther from the face. A face may get
    /// an unbounded box.
    fn face_boxes(&self, _shape: &Self::Shape) -> Result<Vec<[Vec3; 2]>, KernelError> {
        Err(KernelError::Unsupported("face boxes"))
    }

    // Base features that adopt the shapes their data was written from
    // (mitcad#244).

    /// The body [`Kernel::import_brep`] reads from `shape`'s B-rep data
    /// ([`Kernel::brep_data`]), made of `shape` itself without writing and
    /// reading the data: the same faces in the same order, face i named
    /// `<feature>:import(<first_face + i>)`, and its number of faces. It
    /// may share `shape`'s sub-shapes and keep what was measured of it that
    /// does not depend on face names.
    fn adopt_shape(
        &self,
        _feature: FeatureUid,
        _shape: &Self::Shape,
        _first_face: u32,
    ) -> Result<(Self::Shape, u32), KernelError> {
        Err(KernelError::Unsupported("adopted bodies"))
    }

    // Tangent chains (the application's hover of Fillet, Chamfer and the
    // face tools, mitcad#290).

    /// The tangent chains of the shape: with `faces` false the groups of
    /// two or more named edges that a fillet or chamfer with a tangent
    /// chain rounds together when one of them is selected (the edges that
    /// meet end to end in the same direction, within 0.01 rad, between
    /// faces at an angle); with `faces` the groups of faces connected
    /// through smooth edges, which a shell, draft or replace face with a
    /// tangent chain takes together (each face by its first name). Every
    /// edge or face is in at most one group; one in none stands alone.
    fn tangent_chains(
        &self,
        _shape: &Self::Shape,
        _faces: bool,
    ) -> Result<Vec<Vec<String>>, KernelError> {
        Err(KernelError::Unsupported("tangent chains"))
    }

    // Bodies from their boundaries and from triangles (the IFC import's
    // breps and face sets, mitcad#273; the inputs in `crate::boundary`).

    /// The body a boundary bounds: a solid of each closed shell, a sheet
    /// body of the others; faces that do not build are left out and
    /// reported (an error when none builds).
    fn boundary_body(
        &self,
        _body: &crate::boundary::BoundaryBody,
    ) -> Result<(Self::Shape, crate::boundary::BoundaryReport), KernelError> {
        Err(KernelError::Unsupported("bodies from boundaries"))
    }

    /// A mesh body of triangles (indices into `points`, counter-clockwise
    /// seen from outside), as an STL file gives one.
    fn triangle_body(
        &self,
        _points: &[[f64; 3]],
        _triangles: &[[u32; 3]],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("mesh bodies from triangles"))
    }

    // Contacts of a new Extrude's tool: the operation it chooses until the
    // user picks one (mitcad#289).

    /// How the tool lies against each body ([`ToolContact`]), with the
    /// contact test of Join. Cheap enough to follow a dragged distance: the
    /// caller leaves out bodies whose boxes the tool's does not reach.
    fn tool_contacts(
        &self,
        _tool: &Self::Shape,
        _bodies: &[&Self::Shape],
    ) -> Result<Vec<ToolContact>, KernelError> {
        Err(KernelError::Unsupported("tool contacts"))
    }

    // The volume a change makes, from the faces it changed (the `.ipt`
    // import's check of an item's change, mitcad#235).

    /// The volume an operation added (negative: removed) as its changed
    /// faces bound it, mm³: the faces of `after` that `before` does not
    /// have (as [`Kernel::new_faces`] tells them) less those of `before`
    /// that `after` does not have, integrated about one point. The faces
    /// both have cancel exactly, so a large face the change left alone
    /// adds none of its integration's noise. Also how uncertain that is
    /// from the changed faces' edges: a third of each edge's length times
    /// its tolerance beyond 1e-7 mm times its distance from the point
    /// (stored faces trimmed by approximated curves, mitcad#235).
    fn changed_volume(
        &self,
        _before: &[&Self::Shape],
        _after: &[&Self::Shape],
    ) -> Result<(f64, f64), KernelError> {
        Err(KernelError::Unsupported("changed volume"))
    }

    // Joins within a tolerance (an importer's bodies that match a file's
    // only within its geometric tolerance, mitcad#298).

    /// [`Kernel::boolean`]'s join, widened where it fails exactly: a tool
    /// that touches no target still joins the targets within `tolerance`
    /// mm of it, a tool solid the exact join leaves on its own beside a
    /// target within it is merged, and a join whose result is invalid is
    /// retried; each with the smallest fuzzy value that works, up to
    /// `tolerance`. A join that works exactly is the same as `boolean`'s.
    fn join_within(
        &self,
        _targets: &[&Self::Shape],
        _tool: &Self::Shape,
        _tolerance: f64,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("joins within a tolerance"))
    }

    // Cuts within a tolerance (an importer's bodies that match a file's
    // only within its geometric tolerance, mitcad#235).

    /// [`Kernel::boolean`]'s cut, widened where it fails exactly: a cut of
    /// a target that gives an error, an invalid result, a result whose
    /// solids could not be built with all the faces or one with pieces of
    /// the tool left inside it is retried with the
    /// smallest fuzzy value up to `tolerance` that gives a valid result of
    /// all its faces. A cut that works exactly is the same as `boolean`'s.
    fn cut_within(
        &self,
        _targets: &[&Self::Shape],
        _tool: &Self::Shape,
        _tolerance: f64,
    ) -> Result<BooleanOutput<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("cuts within a tolerance"))
    }

    // BUILD: the face names of derived parts (mitcad#375).

    /// The shape with its faces renamed, its geometry shared: a face name
    /// listed in `renames` takes the name it is paired with; every other
    /// name `n` becomes `<prefix>(n)` (a prefix such as `F7:part`), or stays
    /// when `prefix` is empty. Names are matched whole, `#k` included.
    fn rename_faces(
        &self,
        _shape: &Self::Shape,
        _prefix: &str,
        _renames: &[(FaceName, FaceName)],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("renaming faces"))
    }

    // Section curves of sketches (mitcad#389).

    /// Where `plane` cuts the faces `face` resolves to (every face of a name
    /// without `#k`; all of the shape's faces when None), in model space:
    /// a curve per piece, except that B-spline pieces which meet end to end
    /// where no other piece ends (a thread's flanks cut along its axis)
    /// become one B-spline, exactly. Each with the faces its pieces lie on.
    /// Empty when the plane misses them.
    fn section_curves(
        &self,
        _shape: &Self::Shape,
        _face: Option<&FaceName>,
        _plane: &DatumPlane,
    ) -> Result<Vec<SectionCurve>, KernelError> {
        Err(KernelError::Unsupported("section curves"))
    }

    // Unions within a tolerance (an importer's bodies whose faces lie on
    // each other only within its geometric tolerance, mitcad#397).

    /// [`Kernel::unite`], widened where it fails exactly: a union whose
    /// result is invalid is retried with the smallest fuzzy value up to
    /// `tolerance` mm that gives a valid one. A union that works exactly is
    /// the same as `unite`'s.
    fn unite_within(
        &self,
        _shapes: &[&Self::Shape],
        _tolerance: f64,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("unions within a tolerance"))
    }

    // Section Analysis of many bodies at once (mitcad#438). The defaults
    // take one after another; the OCCT kernel works them out on all cores.

    /// [`Kernel::section`] of each shape at its plane, in order.
    fn section_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Section<Self::Shape>, KernelError>> {
        items
            .iter()
            .map(|(shape, plane)| self.section(shape, plane))
            .collect()
    }

    /// [`Kernel::clip`] of each shape at its plane, in order.
    fn clip_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Self::Shape, KernelError>> {
        items
            .iter()
            .map(|(shape, plane)| self.clip(shape, plane))
            .collect()
    }

    // Contact sets: bodies that stop each other while joints move
    // (mitcad#454; `mitcad_model::contact`).

    /// For each pair of bodies, each placed by its rigid transform, the
    /// volume (mm³) where they pass through each other: 0 when they are
    /// apart or only touch (faces within `tolerance`, mm, of each other
    /// count as touching). The caller leaves out pairs whose boxes are
    /// apart; the kernel may work the pairs out at once.
    fn overlap_volumes(
        &self,
        _pairs: &[PlacedPair<'_, Self::Shape>],
        _tolerance: f64,
    ) -> Result<Vec<f64>, KernelError> {
        Err(KernelError::Unsupported("contact between moving bodies"))
    }

    // Emboss: profiles wrapped onto cylinders (mitcad#458; the planar
    // emboss is an extrusion).

    /// The tool of an emboss on a cylindrical face (see [`EmbossWrapSpec`]).
    fn wrapped_emboss(&self, _spec: &EmbossWrapSpec<'_>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("profiles wrapped onto cylinders"))
    }

    // Thin extrusions along sketch curves (mitcad#463).

    /// [`Kernel::extrude_feature`] of a thin extrusion whose walls follow
    /// chains of sketch curves instead of the boundaries of regions
    /// (`spec.regions` is empty; both sides have a wall). Each chain holds
    /// its curves in order, each running along it, and is open or closed.
    /// A closed chain's side 1 lies outside the area it encloses and side 2
    /// inside, as a region's; an open chain's side 1 lies to its right as
    /// it runs (the sketch normal up) and side 2 to its left, and its ends
    /// are square to it. Faces: `outer(<curve>)` (side 1) and
    /// `inner(<curve>)` (side 2) along each curve, `tip0(<curve>)` and
    /// `tip1(<curve>)` at an open chain's start (on its first curve) and
    /// end (its last), and the caps as an extrusion's, keyed by the
    /// chain's curves (`start(r{c1,c2})`). Walls that meet or cross make
    /// one solid.
    fn extrude_curves(
        &self,
        _spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
        _chains: &[Vec<crate::sweeps::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported(
            "thin extrusions along sketch curves",
        ))
    }

    // Boundary Fill: solids of the cells tool bodies enclose (mitcad#457;
    // `features/boundary_fill.rs`).

    /// The cell each point picks among the tools: all of the space inside
    /// every tool that holds the point and outside every other tool (one
    /// shape, of several solids where that space falls apart), in the
    /// order of the points. The cells come from one division of space by
    /// all the tools' faces, so cells that meet share their faces; each
    /// face keeps the names of the tool faces it is a piece of. A point
    /// that no tool holds, or that lies on a tool's boundary, is an error.
    /// The tools are not changed.
    fn boundary_cells(
        &self,
        _tools: &[&Self::Shape],
        _points: &[Vec3],
    ) -> Result<Vec<BoundaryCell<Self::Shape>>, KernelError> {
        Err(KernelError::Unsupported("boundary fill"))
    }

    // Patch, Stitch and Thicken: surface bodies filled, sewn and made solid
    // (mitcad#456; `crate::surfaces`).

    /// One surface body per region, in the regions' order: the region's
    /// planar face `<feature>:patch(<region>)` on the sketch's plane, its
    /// normal the sketch's.
    fn patch_regions(
        &self,
        _feature: FeatureUid,
        _frame: &SketchFrame,
        _regions: &[ProfileRegion],
    ) -> Result<Vec<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("patches of sketch regions"))
    }

    /// The loops of the body's free edges (edges of one face only: the open
    /// boundaries of a surface body), in a deterministic geometric order.
    fn open_boundaries(
        &self,
        _shape: &Self::Shape,
    ) -> Result<Vec<crate::surfaces::OpenBoundary>, KernelError> {
        Err(KernelError::Unsupported("open boundaries"))
    }

    /// A surface body of one face, `<feature>:patch`, filling a closed loop
    /// of the body's edges ([`crate::surfaces::PatchLoopSpec`]): a plane
    /// when the loop lies in one and the patch only passes through its
    /// edges, else a surface fitted to the edges that meets the face beside
    /// each edge as its continuity asks and passes through the points. The
    /// body is not changed.
    fn patch_loop(
        &self,
        _spec: &crate::surfaces::PatchLoopSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("patches of edge loops"))
    }

    /// The shapes' faces sewn together where their edges lie within
    /// `tolerance` (mm; 0 for the kernel's own precision): one shape, a
    /// solid where the faces close up, else a surface body. The faces keep
    /// their names.
    fn stitch(
        &self,
        _shapes: &[&Self::Shape],
        _tolerance: f64,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("stitch"))
    }

    /// A surface body made solid by a wall `thickness` thick (mm, not 0)
    /// along its faces' normals, against them when negative, or half of
    /// it to each side when `symmetric`. The faces of the wall are
    /// `<feature>:front(<face>)` on the side the face's normal points to,
    /// `<feature>:back(<face>)` on the other and `<feature>:wall(<face>)`
    /// along the face's free edges.
    fn thicken(
        &self,
        _feature: FeatureUid,
        _shape: &Self::Shape,
        _thickness: f64,
        _symmetric: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("thicken"))
    }

    // Surface modelling, surface sweeps: Extrude, Revolve, Sweep and Loft
    // making surface bodies (mitcad#455; `crate::surface_modelling`).

    /// The shape's faces but `feature`'s faces in one of the `roles` (a
    /// sweep's caps `start`, `end` and `mid`), as a surface body: one shell
    /// per shell of the shape they were in. The faces keep their names.
    fn open_faces(
        &self,
        _shape: &Self::Shape,
        _feature: FeatureUid,
        _roles: &[&str],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface sweeps"))
    }

    /// A surface body swept straight from chains of curves (each in order,
    /// running along it; open or closed; `features::path::wall_chains`):
    /// from the start, side one's
    /// distance along `spec.direction` and side two's against it. The
    /// start is the profile plane moved along the sketch normal; ends at
    /// targets and tapers are not taken, the regions are not used. Faces
    /// `<feature>:side(<curve>)`.
    fn extrude_sheet(
        &self,
        _spec: &ExtrudeFeatureSpec<'_, Self::Shape>,
        _chains: &[Vec<crate::sweeps::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface extrusions of curves"))
    }

    /// A surface body turned from chains of curves about `spec.axis`, by
    /// `angle1` and `angle2` the other way; no target, the regions are not
    /// used. Faces `<feature>:side(<curve>)`.
    fn revolve_sheet(
        &self,
        _spec: &RevolveSpec<'_, Self::Shape>,
        _chains: &[Vec<crate::sweeps::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface revolutions of curves"))
    }

    /// A surface body swept from chains of curves along `spec.path`, with
    /// its extents, orientation and end scale; no twist, taper or guide
    /// rail, the regions are not used. Faces `<feature>:side(<curve>)`.
    fn sweep_sheet(
        &self,
        _spec: &SweepSpec<'_>,
        _chains: &[Vec<crate::sweeps::PathCurve>],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface sweeps of curves"))
    }

    /// A surface body lofted through sections in order, each one chain of
    /// curves (all open or all closed), smooth or `ruled`; `closed` runs
    /// from the last section back to the first. Faces
    /// `<feature>:side(<curve>)` after the first section's curves.
    fn loft_sheet(
        &self,
        _feature: FeatureUid,
        _sections: &[Vec<crate::sweeps::PathCurve>],
        _ruled: bool,
        _closed: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface lofts of curves"))
    }

    // Surface modelling: Offset Surface, Ruled Surface and Extend
    // (mitcad#455; `crate::surface_modelling`).

    /// A new surface body of the faces (every face when empty; with
    /// `chain` also the faces joined to them through smooth edges) copied
    /// `distance` along their normals (the faces' orientations applied;
    /// against them when negative; 0 copies them). Faces
    /// `<feature>:offset(<face>)`. The body is not changed.
    fn offset_surface(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _distance: f64,
        _chain: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("offset surfaces"))
    }

    /// A new surface body: a ruled strip off a chain of the body's edges
    /// ([`crate::surface_modelling::RuledSpec`]), a face
    /// `<feature>:ruled(<edge>)` per edge and `<feature>:corner(<vertex>)`
    /// for a round corner. The body is not changed.
    fn ruled_surface(
        &self,
        _spec: &crate::surface_modelling::RuledSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("ruled surfaces"))
    }

    /// The body with the faces along the edges (free edges of a surface
    /// body, named or of openings, each on a side of its face's parameter
    /// range) extended `distance` past them: the surface continued, or
    /// tangent or curved as it is across the edge. The extended faces keep
    /// their names.
    fn extend_surface(
        &self,
        _body: &Self::Shape,
        _edges: &[EdgeName],
        _openings: &[crate::surface_modelling::OpeningEdges<'_>],
        _distance: f64,
        _mode: crate::surface_modelling::ExtendMode,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("surface extensions"))
    }

    // Surface modelling: Trim, Untrim, Unstitch and Reverse Normal
    // (mitcad#455; `crate::surface_modelling`).

    /// The surface body cut by the tool
    /// ([`crate::surface_modelling::TrimSpec`]) and only the pieces kept
    /// that the keep points pick (the piece nearest each; pieces are what
    /// hangs together once cut). The faces keep their names (`#k` for
    /// pieces).
    fn trim_surface(
        &self,
        _spec: &crate::surface_modelling::TrimSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("trimmed surfaces"))
    }

    /// A face restored towards its surface: without its holes (`holes`)
    /// and with its outer boundary the surface's own bounds (`outer`; a
    /// surface without bounds: the face's span). With `in_place` the body
    /// with the face replaced (its name kept), else a new surface body of
    /// one face `<feature>:untrim(<face>)`.
    fn untrim_surface(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _face: &FaceName,
        _holes: bool,
        _outer: bool,
        _in_place: bool,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("untrimmed surfaces"))
    }

    /// The body taken apart: what is left once the named faces are taken
    /// out (None when nothing is), then each named face as a surface body
    /// of its own, in the order named. The faces keep their names.
    fn unstitch(
        &self,
        _body: &Self::Shape,
        _faces: &[FaceName],
    ) -> Result<Unstitched<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("unstitch"))
    }

    /// The body with the faces (every face when none is named) turned
    /// over; names kept.
    fn reverse_faces(
        &self,
        _body: &Self::Shape,
        _faces: &[FaceName],
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("reversed faces"))
    }

    // Surface modelling: Blend Surface, Boundary Surface, Match Surface and
    // Fair Surface (mitcad#455; `crate::surface_modelling`).

    /// A new surface body of one face `<feature>:blend` bridging the two
    /// chains of edges ([`crate::surface_modelling::BlendSpec`]): each end
    /// leaves its chain repeating the face beside it up to its continuity.
    /// The bodies are not changed.
    fn blend_surface(
        &self,
        _spec: &crate::surface_modelling::BlendSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("blend surfaces"))
    }

    /// A new surface body of one face `<feature>:boundary` through the
    /// curves ([`crate::surface_modelling::BoundarySpec`]). The bodies of
    /// edge curves are not changed.
    fn boundary_surface(
        &self,
        _spec: &crate::surface_modelling::BoundarySpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("boundary surfaces"))
    }

    /// The surface body with its face (its only face when none is named;
    /// one bounded by its surface's parameter range alone) rebuilt: the
    /// side of the face nearest the chain of the target's edges moved onto
    /// that chain, meeting the target's face beside it with `continuity`,
    /// the opposite side kept. The face keeps its name; the body's faces
    /// are sewn again.
    fn match_surface(
        &self,
        _body: &Self::Shape,
        _face: Option<&FaceName>,
        _target: &crate::surface_modelling::BodyEdges<'_, Self::Shape>,
        _continuity: crate::surfaces::PatchContinuity,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("matched surfaces"))
    }

    /// The body with its B-spline faces among `faces` (every face when
    /// none is named) faired ([`crate::surface_modelling::Fairing`]): their
    /// inner control points moved to smooth the faces, their boundaries
    /// kept. Faces of other surface types stay as they are; the faces keep
    /// their names.
    fn fair_surface(
        &self,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _fairing: crate::surface_modelling::Fairing,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("faired surfaces"))
    }
    // Plastic part features: the band of a lip or a groove (mitcad#459;
    // `features::plastic`).

    /// The band along one loop of a planar rim face, swept along the
    /// face's outward normal ([`RimBandSpec`]): the tool a lip joins and a
    /// groove cuts.
    fn rim_band(&self, _spec: &RimBandSpec<'_, Self::Shape>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("lip and groove bands"))
    }

    // Drawings: views with hidden lines removed and round holes
    // (mitcad#471; `mitcad_model::drawing`).

    /// The edges of the shapes seen along the view, with hidden lines
    /// removed (see [`crate::drawing_view::ViewSpec`]); a section view's cut
    /// faces as loops. The shapes are not changed.
    fn project_view(
        &self,
        _shapes: &[&Self::Shape],
        _spec: &crate::drawing_view::ViewSpec,
    ) -> Result<crate::drawing_view::ViewProjection, KernelError> {
        Err(KernelError::Unsupported("drawing views"))
    }

    /// The round holes of a solid body ([`crate::drawing_view::RoundHole`]), in a
    /// deterministic geometric order; none for other bodies.
    fn round_holes(
        &self,
        _shape: &Self::Shape,
    ) -> Result<Vec<crate::drawing_view::RoundHole>, KernelError> {
        Err(KernelError::Unsupported("round holes"))
    }

    // Gears: toothed profiles swept along their axis or ruled to an apex
    // (mitcad#472; `features::gear`).

    /// A gear's profile swept along its axis ([`GearSweepSpec`]): straight,
    /// turned about the axis (helical teeth) or moved across it (a helical
    /// rack's), in steps.
    fn gear_sweep(&self, _spec: &GearSweepSpec<'_>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("gears"))
    }

    /// A straight bevel gear's teeth ruled to its apex and trimmed by its
    /// blank ([`GearConeSpec`]).
    fn gear_cone(&self, _spec: &GearConeSpec<'_>) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("bevel gears"))
    }

    // Sketch text shaped by the model (mitcad#473; `sketch::fonts`).

    /// The file of an installed font family and style ([`FontSource`]),
    /// which the model reads to shape a text that needs it (right-to-left
    /// or joined scripts). An error when the family is not installed.
    fn font_source(&self, _request: &FontRequest<'_>) -> Result<FontSource, KernelError> {
        Err(KernelError::Unsupported("font files of installed fonts"))
    }

    /// A conical face ([`ConeFace`]): sketch profiles wrap onto it by its
    /// development (`EmbossWrapSpec::half_angle`).
    fn face_cone(&self, _shape: &Self::Shape, _face: &FaceName) -> Result<ConeFace, KernelError> {
        Err(KernelError::Unsupported("face cones"))
    }

    // Faces moved by a rigid motion (mitcad#343).

    /// Moves the faces by the rigid `motion`, each keeping its surface,
    /// moved; their neighbours stay on their surfaces and extend or shorten
    /// up to them. Every face keeps its names; a motion that would change
    /// the body's faces is an error.
    fn move_faces(
        &self,
        _feature: FeatureUid,
        _body: &Self::Shape,
        _faces: &[FaceName],
        _motion: &Transform,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("move faces"))
    }

    // Section Analysis off the UI thread (mitcad#438).

    /// What another thread may read of each shape, at its plane, while this
    /// thread goes on using the shapes (the view meshes them, a model job
    /// meshes shapes that share their faces): `Some(copy)`, sharing no
    /// sub-shape with the shape (the same face names), for a shape the
    /// plane may cut or touch; `None` for one the plane passes farther than
    /// its tolerances, whose [`Kernel::clip`] keeps or removes it whole and
    /// whose [`Kernel::section`] is empty, told from what the shape keeps
    /// of itself (its box, measured now) without reading its sub-shapes.
    /// Run on the thread that owns the shapes. The default copies nothing
    /// (a kernel whose shapes share nothing): every shape as it is.
    fn detach_all(
        &self,
        items: &[(&Self::Shape, DatumPlane)],
    ) -> Vec<Result<Option<Self::Shape>, KernelError>> {
        items
            .iter()
            .map(|(shape, _)| Ok(Some((*shape).clone())))
            .collect()
    }

    // Extrusions of a planar body face (mitcad#535; `features::extrude`).

    /// A planar face of a body swept as an extrusion's profile (see
    /// [`FaceExtrudeSpec`]); the body is left as it is. A face that is not
    /// planar, has no surface (a mesh body's) or is not one face is an
    /// error.
    fn extrude_face(
        &self,
        _spec: &FaceExtrudeSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("extrusions of body faces"))
    }

    // Revolutions of a planar body face (mitcad#539; `features::revolve`).

    /// A planar face of a body turned as a revolution's profile (see
    /// [`FaceRevolveSpec`]); the body is left as it is. A face that is not
    /// planar, has no surface (a mesh body's), is not one face, crosses
    /// the axis or has the axis through it is an error.
    fn revolve_face(
        &self,
        _spec: &FaceRevolveSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("revolutions of body faces"))
    }

    // Sweeps of a planar body face (mitcad#543; `features::sweep`).

    /// A planar face of a body swept along a path as a sweep's profile:
    /// `spec` without regions (its frame unused: the face's plane is the
    /// profile's), the face as [`crate::sweeps::ProfileFace`] gives it; the
    /// body is left as it is. Faces are named as a sweep's: `side(<edge>)`
    /// by the edge's name in the body (an edge without one by its index
    /// among the face's edges), the caps `start(<key>)` and `end(<key>)`. A
    /// face that is not planar, has no surface or is not one face is an
    /// error.
    fn face_sweep(
        &self,
        _spec: &SweepSpec<'_>,
        _face: &crate::sweeps::ProfileFace<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("sweeps of body faces"))
    }

    // The lumps of a body cut lump by lump (mitcad#658; `features::base`).

    /// How many solids the shape holds, without making a shape of each
    /// ([`Kernel::solids`] names every face of each).
    fn solid_count(&self, _shape: &Self::Shape) -> Result<usize, KernelError> {
        Err(KernelError::Unsupported("solid counts"))
    }

    // Boxes of single faces and edges (the `faces` and `edges` queries'
    // geometry, mitcad#714).

    /// The tight axis-aligned box of each face at these indices of the
    /// shape's face order ([`Kernel::faces`]), as [`Kernel::bounding_box`]
    /// measures a body; None for a face without geometry.
    fn face_bounding_boxes(
        &self,
        _shape: &Self::Shape,
        _faces: &[usize],
    ) -> Result<Vec<Option<BoundingBox>>, KernelError> {
        Err(KernelError::Unsupported("face bounding boxes"))
    }

    /// The same of the edges at these indices of [`Kernel::edges`]' order;
    /// None for a degenerate edge.
    fn edge_bounding_boxes(
        &self,
        _shape: &Self::Shape,
        _edges: &[usize],
    ) -> Result<Vec<Option<BoundingBox>>, KernelError> {
        Err(KernelError::Unsupported("edge bounding boxes"))
    }

    // The extended tool of a split or trim, for its preview (mitcad#712;
    // `features::face_refs::extended_tools`).

    /// The faces [`Kernel::split_body`], [`Kernel::split_faces`] and
    /// [`Kernel::trim_surface`] cut with when their targets lie in
    /// `bounds`: a plane as a face across the box, a face grown along its
    /// surface with `extend` (as those operations grow it, the same
    /// construction), a body's faces, sketch curves swept through the box;
    /// clipped to `bounds`. The tool's shapes are left as they are; an
    /// error when nothing of the tool lies in the box.
    fn extended_tool(
        &self,
        _tool: &ToolInput<'_, Self::Shape>,
        _extend: bool,
        _bounds: &BoundingBox,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("extended tools"))
    }

    // Crossing points of a sketch's Intersect (mitcad#736; `sketch/project.rs`).

    /// Where `plane` crosses the edges `edge` resolves to (every edge of a
    /// name without `#k`), in model space, each point once: a point where
    /// an edge passes through the plane or touches it, and the ends of an
    /// edge (or of a piece of it) that lies in the plane. Empty when the
    /// plane misses them.
    fn section_points(
        &self,
        _shape: &Self::Shape,
        _edge: &EdgeName,
        _plane: &DatumPlane,
    ) -> Result<Vec<[f64; 3]>, KernelError> {
        Err(KernelError::Unsupported("section points"))
    }

    // Features evaluated on worker threads (the recompute, mitcad#842).

    /// Runs `f`, the work of a thread that evaluates features on a fork of
    /// this kernel ([`Kernel::fork`]) beside the recompute's own thread:
    /// the kernel sets the thread up as it does the threads it computes on
    /// (the OCCT kernel: its crash handlers). Kernels that need nothing
    /// just run `f`.
    fn worker_thread<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }

    // Warm opens from the result store (mitcad#858): display meshes kept
    // with the bodies, and stored shapes unpacked when something reads them.

    /// The display triangulations the application's view meshing left in
    /// the shape (a shape meshed for display to the end), as bytes for
    /// [`Kernel::restore_display_mesh`] of the same build.
    fn display_mesh_bytes(&self, _shape: &Self::Shape) -> Result<Vec<u8>, KernelError> {
        Err(KernelError::Unsupported("storing display meshes"))
    }

    /// Gives the shape the display triangulations of
    /// [`Kernel::display_mesh_bytes`] of the same body's shape, and marks it
    /// meshed for display; how many faces were given one (0 when the shape
    /// was meshed already). Bytes of another shape write nothing: an error.
    fn restore_display_mesh(
        &self,
        _shape: &Self::Shape,
        _bytes: &[u8],
    ) -> Result<usize, KernelError> {
        Err(KernelError::Unsupported("restoring display meshes"))
    }

    /// How a stored shape ([`Kernel::shape_bytes`]) is made later, without
    /// the kernel at hand, and what stands in a recompute's body states
    /// for one not made yet (the result store's lazy restore,
    /// `crate::store` in the model). Unsupported: stored results are read
    /// whole.
    fn shape_unpacker(&self) -> Result<ShapeUnpacker<Self::Shape>, KernelError> {
        Err(KernelError::Unsupported("unpacking stored shapes later"))
    }

    // Feature inputs drawn where they were (mitcad#957): a Fillet's or a
    // Chamfer's picked edges as polylines, kept with its result.

    /// The edges `edges` names (every edge a name resolves to) and the
    /// edges bounding the faces `faces` names, each edge once, as polylines
    /// in the shape's coordinates: points along each edge's curve, its ends
    /// included, no farther than `deflection` (mm) from it and turning by at
    /// most `angle` (radians) from one piece to the next. A degenerate edge
    /// is left out; a name the shape lacks is an error. The shape is left as
    /// it is.
    fn edge_polylines(
        &self,
        _shape: &Self::Shape,
        _edges: &[EdgeName],
        _faces: &[FaceName],
        _deflection: f64,
        _angle: f64,
    ) -> Result<Vec<Vec<[f64; 3]>>, KernelError> {
        Err(KernelError::Unsupported("edge polylines"))
    }

    // Dogbones: relief cuts at a body's inside corners for a router bit
    // (mitcad#1011; `features::dogbone`).

    /// The reliefs of a dogbone ([`DogboneSpec`]) as one tool shape, a
    /// cylinder of the tool's diameter along each inside corner edge it
    /// takes, placed so that the tool reaches the sharp corner: the tool a
    /// dogbone cuts from the body. The body is left as it is. A picked edge
    /// that is not straight or not an inside (concave) corner is an error
    /// that says so; picks that hold no such edge are an error too.
    fn dogbone_tool(
        &self,
        _spec: &DogboneSpec<'_, Self::Shape>,
    ) -> Result<Self::Shape, KernelError> {
        Err(KernelError::Unsupported("dogbones"))
    }
}

/// [`Kernel::shape_unpacker`]: `unpack` makes a shape of
/// [`Kernel::shape_bytes`] as [`Kernel::shape_from_bytes`] does, on any
/// thread; `stand_in` makes a shape that nothing reads, which a body state
/// holds until the shape is made.
pub struct ShapeUnpacker<S> {
    pub unpack: fn(&[u8]) -> Result<S, KernelError>,
    pub stand_in: fn() -> S,
}

impl<S> Clone for ShapeUnpacker<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for ShapeUnpacker<S> {}

/// A conical face ([`Kernel::face_cone`], mitcad#473).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConeFace {
    pub apex: [f64; 3],
    /// From the apex along the axis, where the radius grows (a unit vector).
    pub axis: [f64; 3],
    /// Radians, between 0 and π/2.
    pub half_angle: f64,
    /// The material lies outside the cone (a countersink's wall).
    pub internal: bool,
}

/// An installed font's file ([`Kernel::font_source`], mitcad#473).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontSource {
    /// The installed family's name.
    pub family: String,
    pub path: String,
    /// The face in a collection.
    pub index: u32,
    /// The style asked for is made by slanting an upright face.
    pub synthetic_italic: bool,
}

/// Input of [`Kernel::rim_band`] (mitcad#459). The face `face` of `body`
/// must be planar. The band follows its outer loop (`outside`) or its
/// largest opening: an inner loop with no material over the face just
/// inside it (the cavity under a shelled body's rim, not the foot of a
/// boss standing on the face); a face without an opening, or with two
/// openings alike in size (the second more than half the largest's area),
/// is refused. In the face's plane the band runs
/// from the loop `width` into the face to `free` beyond the loop, away
/// from the face (0: on the loop), with sharp corners where the offset
/// loops' edges meet; along the outward normal from `start` to `end`
/// (offsets from the face's plane, `start < end`). Faces are named
/// `<feature>:band_side(<edge>)` along the offset into the face and
/// `<feature>:band_edge(<edge>)` along the loop (`<edge>` the face's edge
/// they follow, or its number in the loop when it has no name),
/// `<feature>:band_top` at `end` and `<feature>:band_bottom` at `start`; a
/// wall the offset made of no edge of the loop is `<feature>:band_wall(<k>)`.
#[derive(Debug, Clone, Copy)]
pub struct RimBandSpec<'a, S> {
    pub feature: FeatureUid,
    pub body: &'a S,
    pub face: &'a FaceName,
    pub outside: bool,
    pub width: f64,
    pub free: f64,
    pub start: f64,
    pub end: f64,
}

/// Two bodies, each placed by a rigid transform from its own coordinates
/// into common ones ([`Kernel::overlap_volumes`]).
#[derive(Debug, Clone, Copy)]
pub struct PlacedPair<'a, S> {
    pub a: &'a S,
    pub a_placement: Transform,
    pub b: &'a S,
    pub b_placement: Transform,
}

/// A curve where a plane cuts faces ([`Kernel::section_curves`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SectionCurve {
    pub curve: Curve3,
    /// The faces it lies on, by their names (unnamed faces left out), in
    /// the order of its pieces.
    pub faces: Vec<FaceName>,
}

/// How a tool lies against a body ([`Kernel::tool_contacts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolContact {
    /// Farther from the body than Join's contact distance.
    Apart,
    /// Touching or overlapping it, its inside outside the body (a profile
    /// on a face of the body extruded away from it).
    Touches,
    /// The inside of a solid of the tool lies inside the body (extruded
    /// into it).
    Inside,
}

/// The material features added to and removed from a body
/// ([`Kernel::feature_changes`]).
#[derive(Debug, Clone)]
pub struct FeatureChanges<S> {
    pub added: Vec<AddedMaterial<S>>,
    pub removed: Vec<S>,
}

/// Material features added: the solid and its seam, the planar faces of
/// the solid that lie on faces of the body it stands on (None when there
/// are none).
#[derive(Debug, Clone)]
pub struct AddedMaterial<S> {
    pub solid: S,
    pub seam: Option<S>,
}

/// What [`Kernel::indexed_element`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Face,
    Edge,
    Vertex,
}

/// A face, edge or vertex by its index (see [`Kernel::indexed_element`]).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedElement {
    pub name: Option<String>,
    pub curves: Vec<Curve3>,
}

/// Points inside a named face (see [`Kernel::face_points`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FacePoints {
    /// Its first name.
    pub name: String,
    pub points: Vec<[f64; 3]>,
}

/// A face an operation made or changed (see [`Kernel::new_faces`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NewFace {
    /// mm².
    pub area: f64,
    /// Points inside it, spread over it; none for a face too thin for the
    /// grid.
    pub points: Vec<[f64; 3]>,
}

/// Input of [`Kernel::font_glyphs`]: a font family (empty for the bundled
/// default font), its style and the characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontRequest<'a> {
    pub family: &'a str,
    pub bold: bool,
    pub italic: bool,
    pub text: &'a str,
}

/// The outlines of a text's characters in em units (the font size is 1),
/// one glyph per character of the request (`char`s, newlines included).
/// A font that is not installed falls back to the bundled one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontGlyphs {
    /// The family the outlines come from.
    pub family: String,
    /// True when the requested family was not found.
    pub fallback: bool,
    /// Above the baseline (positive) and below it (negative).
    pub ascender: f64,
    pub descender: f64,
    /// The height of the capital letters above the baseline; 0 when the
    /// kernel does not say (see `SketchEdit::text_outlines`).
    pub cap_height: f64,
    /// From one baseline to the next.
    pub line_spacing: f64,
    pub glyphs: Vec<Glyph>,
}

/// One character's outline: closed contours of Bézier pieces (two control
/// points for a line, three or four for a quadratic or cubic curve), each
/// piece starting where the one before it ends, with the pen at the origin
/// on the baseline and y up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Glyph {
    /// Where the pen goes next: the glyph's advance, kerning with the next
    /// character included.
    pub advance: f64,
    pub contours: Vec<Vec<Vec<[f64; 2]>>>,
}

/// A curve in model space, for projection into sketches.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve3 {
    Point([f64; 3]),
    Line {
        start: [f64; 3],
        end: [f64; 3],
    },
    /// A circle (`major == minor`) or an ellipse: `center + major cos t
    /// x_axis + minor sin t (normal × x_axis)` for t from `start` to `end`
    /// (the whole curve when `closed`).
    Conic {
        center: [f64; 3],
        normal: [f64; 3],
        x_axis: [f64; 3],
        major: f64,
        minor: f64,
        start: f64,
        end: f64,
        closed: bool,
    },
    /// A B-spline with its full knot vector (`poles.len() + degree + 1`
    /// knots); empty weights for a non-rational one.
    BSpline {
        degree: u32,
        poles: Vec<[f64; 3]>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    },
    /// A helix (a helical path, mitcad#385): `center + radius (cos t x_axis
    /// + sin t (axis × x_axis)) + pitch t / 2π axis` for t from `start` to
    /// `end` (`start < end`; a unit `axis`, `x_axis` at right angles to
    /// it). `pitch` is the rise per turn, negative for a left-handed helix.
    Helix {
        center: [f64; 3],
        axis: [f64; 3],
        x_axis: [f64; 3],
        radius: f64,
        pitch: f64,
        start: f64,
        end: f64,
    },
}

/// Input of [`Kernel::extrude`]: the regions swept along `direction` (a unit
/// vector) from `start` to `end` (offsets along it from the sketch plane,
/// `start < end`), named after `feature`.
#[derive(Debug, Clone, Copy)]
pub struct ExtrudeSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    pub direction: [f64; 3],
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    Join,
    Cut,
    Intersect,
}

/// Result of [`Kernel::boolean`]. `touched[i]` tells whether target i took
/// part; an untouched target is unchanged and has no piece, and a touched
/// target without a piece was removed. Join fuses the tool with every
/// target it touches; a tool solid touching nothing is a piece without
/// sources. Cut and Intersect may split a target into several pieces.
#[derive(Debug, Clone)]
pub struct BooleanOutput<S> {
    pub pieces: Vec<BooleanPiece<S>>,
    pub touched: Vec<bool>,
}

/// A solid of a boolean result and the targets (indices) whose material it
/// contains, in increasing order. Pieces are in geometric order.
#[derive(Debug, Clone)]
pub struct BooleanPiece<S> {
    pub shape: S,
    pub sources: Vec<usize>,
}

/// Chamfer sizes, millimetres and radians. Two distances and distance-angle
/// measure the first distance (and the angle, from that face) on the
/// reference face of each edge (see [`Chamfer`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChamferSize {
    EqualDistance { distance: f64 },
    TwoDistances { distance1: f64, distance2: f64 },
    DistanceAngle { distance: f64, angle: f64 },
}

/// A chamfer edge set: the named edges and every edge of the named faces.
/// The first distance is measured on the edge's face that matches
/// `reference`, or without one on the first face of the edge's name;
/// `flip` takes the other face. Without `tangent_chain`, an edge whose
/// tangent chain reaches an unselected edge is unsupported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chamfer<'a> {
    pub edges: &'a [EdgeName],
    pub faces: &'a [FaceName],
    pub size: ChamferSize,
    pub reference: Option<&'a FaceName>,
    pub flip: bool,
    pub tangent_chain: bool,
}

/// The corner type where three or more chamfered edges meet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChamferCorner {
    /// The geometry kernel's own patch.
    #[default]
    Chamfer,
    /// The bevels continued until they meet.
    Miter,
    /// A smooth patch tangent to the bevels and the faces.
    Blend,
}

// Inputs of the F2 operations (fillet sets, shell, draft, face operations,
// split).

/// A fillet edge set: the named edges and every edge of the named faces.
/// `curvature` asks for curvature continuous (G2) cross-sections, shaped
/// by `weight` (the tangency weight, 0.1 to 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilletSet<'a> {
    pub edges: &'a [EdgeName],
    pub faces: &'a [FaceName],
    pub size: FilletSize<'a>,
    pub tangent_chain: bool,
    pub curvature: bool,
    pub weight: f64,
}

/// Millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilletSize<'a> {
    Constant {
        radius: f64,
    },
    /// The width across the rounding.
    ChordLength {
        length: f64,
    },
    /// From `start` at `start_vertex` (or at the kernel's start of the
    /// chain) to `end` at the other end of the chain, through the `mid`
    /// radii (relative position along the chain from the start, radius),
    /// smoothly.
    Variable {
        start: f64,
        end: f64,
        start_vertex: Option<&'a VertexName>,
        mid: &'a [(f64, f64)],
    },
    /// `distance1` from the edge on its face that matches `reference` (or
    /// on the first face of the edge's name), `distance2` on the other;
    /// `flip` swaps the faces.
    Asymmetric {
        distance1: f64,
        distance2: f64,
        reference: Option<&'a FaceName>,
        flip: bool,
    },
    /// The radius a clamped B-spline of the position along the chain
    /// (mitcad#464): 0 at `start_vertex` (or at the kernel's start of the
    /// chain), 1 at its other end, by arc length; `knots` its full knot
    /// vector and `radii` its control radii (degree `knots.len() -
    /// radii.len() - 1`, 1 to 3).
    Spline {
        start_vertex: Option<&'a VertexName>,
        knots: &'a [f64],
        radii: &'a [f64],
    },
}

/// Removed faces (none: a closed void) and the wall thickness inside and
/// outside the surface; `rounded` rounds the outer corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellSpec<'a> {
    pub faces: &'a [FaceName],
    pub inside: f64,
    pub outside: f64,
    pub tangent_chain: bool,
    pub rounded: bool,
}

/// Faces tilted by `angle` (radians) about their intersection with the
/// fixed plane. The pull direction is the plane's normal, or from a face of
/// a body into its material, reversed by `flip`; a positive angle removes
/// material on the pull side. With `angle2` the faces split at the plane and
/// the other side tilts by `angle2` against the pull direction.
pub struct DraftSpec<'a, S> {
    pub faces: &'a [FaceName],
    pub plane: ToolInput<'a, S>,
    pub angle: f64,
    pub angle2: Option<f64>,
    pub flip: bool,
    pub tangent_chain: bool,
}

/// What a face operation works against.
pub enum ToolInput<'a, S> {
    /// A plane through `origin`; `normal` is a unit vector.
    Plane {
        origin: [f64; 3],
        normal: [f64; 3],
    },
    /// A face (all pieces of a split face) of a body.
    Face {
        shape: &'a S,
        face: &'a FaceName,
    },
    Body {
        shape: &'a S,
    },
    /// The curves of the sketch regions swept both ways along `direction`.
    Curves {
        frame: SketchFrame,
        regions: &'a [ProfileRegion],
        curves: &'a [EntityUid],
        direction: [f64; 3],
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FaceInfo {
    /// Sorted; empty for an unnamed face.
    pub names: Vec<String>,
    /// `plane`, `cylinder`, `cone`, `sphere`, `torus`, `bspline`, ...
    pub surface: String,
    pub area: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeInfo {
    pub name: Option<String>,
    /// `line`, `circle`, `ellipse`, `bspline`, ...
    pub curve: String,
    pub length: f64,
}

/// The middle of a named edge (see [`Kernel::edge_middles`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeMiddle {
    pub name: String,
    pub point: [f64; 3],
    /// The unit tangent there.
    pub tangent: [f64; 3],
    pub length: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MassProperties {
    pub volume: f64,
    pub area: f64,
    pub center: [f64; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundingBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl BoundingBox {
    pub fn corners(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        (0..8).map(|i| {
            std::array::from_fn(|axis| {
                if i & (1 << axis) == 0 {
                    self.min[axis]
                } else {
                    self.max[axis]
                }
            })
        })
    }

    pub fn overlaps(&self, other: &BoundingBox, tolerance: f64) -> bool {
        (0..3).all(|i| {
            self.min[i] <= other.max[i] + tolerance && other.min[i] <= self.max[i] + tolerance
        })
    }

    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: std::array::from_fn(|i| self.min[i].min(other.min[i])),
            max: std::array::from_fn(|i| self.max[i].max(other.max[i])),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelError {
    /// The operation failed; the message is for the user.
    Failed(String),
    /// The kernel does not implement this operation.
    Unsupported(&'static str),
}

impl KernelError {
    /// What the message of an operation that ran out of memory says
    /// (mitcad#80): the geometry kernel turns an allocation that fails into
    /// an error "<operation>: out of memory", and the .f3d import then
    /// tries no more definitions.
    pub const OUT_OF_MEMORY: &'static str = "out of memory";

    pub fn failed(message: impl Into<String>) -> Self {
        Self::Failed(message.into())
    }

    /// Whether an error's message (of a kernel operation, or of a feature
    /// one failed) says the operation ran out of memory.
    pub fn is_out_of_memory(message: &str) -> bool {
        message.contains(Self::OUT_OF_MEMORY)
    }
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(message) => f.write_str(message),
            Self::Unsupported(what) => write!(f, "the geometry kernel does not support {what}"),
        }
    }
}

impl std::error::Error for KernelError {}

// Profile features (F1).

/// An unbounded plane through `origin`; `normal` is a unit vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

/// A directed line through `origin`; `direction` is a unit vector.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Axis {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

/// A cylindrical face (all pieces of a split one).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cylinder {
    /// On the cylinder's axis at the face's low end along the axis of the
    /// cylinder's surface, pointing along that axis.
    pub axis: Axis,
    pub radius: f64,
    /// The face's extent along the axis.
    pub length: f64,
    /// The material lies outside (a hole's wall).
    pub internal: bool,
}

/// What a sweep runs up to or starts from ("to object" and "from
/// object").
#[derive(Debug, Clone)]
pub enum Target<S> {
    Plane(Plane),
    /// A face of the body: its surface continued past its edges with
    /// `extend` (`isChained = false` in .f3d designs), else the face and the faces
    /// next to it.
    Face {
        body: S,
        face: FaceName,
        extend: bool,
    },
    /// A body: to its first face reached, or `through` it to the far side.
    Body {
        body: S,
        through: bool,
    },
}

/// A target moved by `offset`: a plane or a planar face along its normal
/// (turned to point along the sweep), anything else along the sweep. A
/// positive offset makes the sweep longer.
#[derive(Debug, Clone)]
pub struct Bound<S> {
    pub target: Target<S>,
    pub offset: f64,
}

/// Where a thin extrusion's wall lies relative to the profile curves: on
/// side 1 (away from the region's material: outside the outer loop, into a
/// hole), centred on them, on side 2, or on both sides by thicknesses of
/// their own (`both`, mitcad#463: [`ThinWall::thickness`] on side 1 and
/// [`ThinWall::thickness2`] on side 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallLocation {
    Side1,
    Center,
    Side2,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThinWall {
    pub location: WallLocation,
    pub thickness: f64,
    /// The thickness on side 2 of a wall on both sides; 0 otherwise.
    pub thickness2: f64,
}

#[derive(Debug, Clone)]
pub enum SideEnd<S> {
    /// From the start, > 0. With two sides one of them may be negative
    /// (mitcad#765, signed two-sided extents): it ends on the other side's
    /// side, and the solid is the other side's from there on, its cap there
    /// named as this side's far cap and the other side tapered from the
    /// start as without it. Two distances then add up to more than zero.
    Distance(f64),
    Target(Bound<S>),
}

/// One side of an extrusion.
#[derive(Debug, Clone)]
pub struct ExtrudeSide<S> {
    pub end: SideEnd<S>,
    /// Radians; a positive taper widens the profile away from the start
    /// (every loop moves away from the material by t tan(taper) at distance
    /// t: the outside grows, holes shrink).
    pub taper: f64,
    /// A thin extrusion's wall; both sides have one or neither.
    pub thin: Option<ThinWall>,
}

#[derive(Debug, Clone)]
pub enum ExtrudeStart<S> {
    /// The profile plane moved along the sketch normal.
    Offset(f64),
    /// The profile projected along the sketch normal onto a plane or planar
    /// face, moved along its normal by the bound's offset.
    Object(Bound<S>),
}

/// Input of [`Kernel::extrude_feature`]: side one runs along `direction`
/// (a unit vector, the sketch normal or its reverse), side two against it.
/// Faces are named `side(<segment>)`, `outer(<segment>)` and
/// `inner(<segment>)` (thin walls), `start(<region>)` and `end(<region>)`:
/// with one side the caps at the start and the far end, with two sides the
/// far caps of side one and side two (`startFaces` and `endFaces` in .f3d designs).
/// Faces of a target where the extrusion ends are `end` (`start` for side
/// one of two).
#[derive(Debug, Clone)]
pub struct ExtrudeFeatureSpec<'a, S> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    pub direction: [f64; 3],
    pub start: ExtrudeStart<S>,
    pub side1: ExtrudeSide<S>,
    pub side2: Option<ExtrudeSide<S>>,
}

/// Input of [`Kernel::revolve`]: side one turns by `angle1` radians (right
/// hand about the axis direction), side two by `angle2` the other way, or
/// side one up to `target` (a body's first face, or through it to its last
/// face, mitcad#539); at most a full turn in total. Faces are named as an
/// extrusion's; a full turn has no caps.
#[derive(Debug, Clone)]
pub struct RevolveSpec<'a, S> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    pub axis: Axis,
    pub angle1: f64,
    pub angle2: Option<f64>,
    pub target: Option<Target<S>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HoleShape {
    Simple,
    Counterbore {
        diameter: f64,
        depth: f64,
    },
    /// `angle`: the cone's full included angle.
    Countersink {
        diameter: f64,
        angle: f64,
    },
    /// A counterbore whose floor is a cone of the full `angle` narrowing to
    /// the hole (mitcad#4).
    Counterdrill {
        diameter: f64,
        depth: f64,
        angle: f64,
    },
}

#[derive(Debug, Clone)]
pub enum HoleEnd<S> {
    /// The depth to the shoulder; the drill point is beyond it.
    Distance(f64),
    /// Past every body in [`HoleSpec::bodies`], with a flat end.
    ThroughAll,
    /// The cylindrical part ends where the axis first meets the target.
    Target(Bound<S>),
}

/// Input of [`Kernel::hole_tool`]: one solid of revolution per position
/// (the start point, pointing into the material), faces named
/// `<feature>:hole<i>.<part>`: `wall`, `tip` (drill point or flat bottom),
/// `cbore_wall`, `cbore_floor`, `csink`, `cdrill` (a counterdrill's cone)
/// and `top` (at the start).
#[derive(Debug, Clone)]
pub struct HoleSpec<'a, S> {
    pub feature: FeatureUid,
    pub positions: &'a [Axis],
    pub diameter: f64,
    pub shape: HoleShape,
    /// The drill point's full angle at the wall's end; None for a flat
    /// bottom.
    pub tip_angle: Option<f64>,
    pub end: HoleEnd<S>,
    pub bodies: Vec<S>,
    /// The wall's angle to the axis, radians: positive narrows the hole as
    /// it goes deeper (0: a cylinder).
    pub taper: f64,
}

/// Input of [`Kernel::modeled_thread`]: the profile of `pitch` whose
/// flanks lie at `flank_angle` to the radial direction (30 degrees: the
/// 60-degree profile of ISO 68-1 and Unified; 15 degrees: the trapezoidal
/// one of ISO 2901, mitcad#893), half a pitch wide at `pitch_diameter`,
/// with flat crests and roots at the `major` and `minor` diameters, built
/// on each face: along the threaded part the material becomes the thread
/// (removed beyond the profile, added where the face lies inside the
/// crest). A thread of `starts` starts (mitcad#893) has that many grooves
/// evenly spaced round the axis, a pitch apart along it, each a helix of
/// lead `starts * pitch`; its profile in every section through the axis is
/// a single-start thread's of the same pitch.
/// `angle` (radians) turns the thread about its axis from where its
/// groove (an internal thread's tooth) spans the first half pitch from the
/// face's low end at the reference direction (the X axis projected across
/// the axis; Y for axes near X; `geometry/include/mitcad/geometry/thread.hpp`).
/// `part` limits it to (length, offset, from the high end of the
/// cylinder's axis); otherwise it covers the face and runs out through its
/// free ends. The thread's faces are named `<feature>:thread(<face>)`.
/// Extra clearance (mitcad#195, mm, 0 or more): `radial_clearance` moves
/// the whole profile along the radius away from the material (an internal
/// thread's diameters grow by twice it, an external thread's shrink), then
/// `flank_clearance` moves each flank that far perpendicular to itself
/// into the tooth, keeping the flank angle, the pitch and the crest and
/// root radii (`features::thread::check_clearance` gives the limits): each
/// flank moves `flank_clearance / cos(flank_angle)` along the axis.
/// `groove` false builds no profile, only the `chamfer` (a cosmetic
/// thread's end chamfers, mitcad#304), or the `taper`'s cone.
///
/// A taper pipe thread (`taper`, NPT and ISO 7-1, mitcad#733): `major`,
/// `minor` and `pitch_diameter` are the profile's diameters where the
/// thread starts, and every diameter changes by `taper.per_length` per
/// length along the axis away from there: smaller into an internal thread,
/// larger along an external one (1:16). The thread starts at the end of
/// its `part`; over the whole face at its free end (nothing of the body
/// beyond it: a hole's mouth, a shaft's end) where only one end is free,
/// else at the high end when `taper.high_end`, the low end otherwise. A
/// cosmetic one (`groove` false) cuts the cone of its minor diameter out of
/// an internal thread's face (the tapered bore), or the material outside
/// the cone of its major diameter off an external one, over its part; the
/// cone's faces are named `<feature>:thread_taper(<face>)`. A modelled one
/// sweeps the profile, its flanks square to the axis, along the tapered
/// helix of its pitch diameter, whose angle is measured at the start.
#[derive(Debug, Clone)]
pub struct ThreadSpec<'a> {
    pub feature: FeatureUid,
    pub faces: &'a [FaceName],
    pub pitch: f64,
    pub major: f64,
    pub minor: f64,
    pub pitch_diameter: f64,
    pub right_handed: bool,
    pub angle: f64,
    /// Per face (in the order of `faces`), the angle it is turned by
    /// instead of `angle` (a pattern's copies, mitcad#733); empty: `angle`
    /// for every face.
    pub face_angles: &'a [f64],
    pub part: Option<(f64, f64, bool)>,
    pub radial_clearance: f64,
    pub flank_clearance: f64,
    pub groove: bool,
    pub chamfer: Option<ThreadChamfer>,
    pub taper: Option<ThreadTaper>,
    /// The number of starts, 1 or more (mitcad#893): the helices' lead is
    /// `starts * pitch`.
    pub starts: u32,
    /// The flanks' angle to the radial direction, radians: half the
    /// profile's included angle (30 degrees for the 60-degree profile, 15
    /// for the trapezoidal one; mitcad#893).
    pub flank_angle: f64,
}

/// The taper of a taper pipe thread (see [`ThreadSpec`], mitcad#733).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThreadTaper {
    /// The diameters' change per length along the axis (1/16).
    pub per_length: f64,
    /// Over the whole face with both or neither end free: whether the
    /// thread starts at the high end of the axis of the face's cylinder.
    pub high_end: bool,
}

/// The end chamfers of a thread (mitcad#304): a cone cut at an end of each
/// face, before the profile, so that the first turn starts on the cone.
/// An external thread's cone widens from `diameter` at the end into the
/// material at `angle` (radians, to the axis), and everything outside it
/// goes; an internal thread's (a countersink) narrows from `diameter` at
/// the end, and everything inside it goes. `ends` holds per face (in the
/// order of the faces) whether its low and its high end along the axis of
/// the face's cylinder are chamfered; the kernel chamfers such an end only
/// where nothing of the body lies beyond it (a free end). The cones are
/// named `<feature>:thread_chamfer_low(<face>)` and
/// `<feature>:thread_chamfer_high(<face>)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadChamfer {
    pub angle: f64,
    pub diameter: f64,
    pub ends: Vec<(bool, bool)>,
}

/// [`Kernel::extrude_feature`] with [`Kernel::extrude`]: a prism between
/// two offsets along the direction.
#[doc(hidden)]
pub fn simple_extrude<K: Kernel + ?Sized>(
    kernel: &K,
    spec: &ExtrudeFeatureSpec<'_, K::Shape>,
) -> Result<K::Shape, KernelError> {
    let unsupported = KernelError::Unsupported("this extrusion (targets, tapers or thin walls)");
    let distance = |side: &ExtrudeSide<K::Shape>| match side.end {
        SideEnd::Distance(d) if side.taper == 0.0 && side.thin.is_none() => Ok(d),
        _ => Err(unsupported.clone()),
    };
    let ExtrudeStart::Offset(offset) = spec.start else {
        return Err(unsupported);
    };
    let normal = spec.frame.normal();
    // + 0.0 turns -0.0 into 0.0.
    let base = offset * crate::profile::dot(normal, spec.direction) + 0.0;
    let one = distance(&spec.side1)?;
    let mut extrusion = ExtrudeSpec {
        feature: spec.feature,
        frame: spec.frame,
        regions: spec.regions,
        direction: spec.direction,
        start: base,
        end: base + one,
    };
    if let Some(side2) = &spec.side2 {
        // Along the reversed direction, so that the cap at the end of side
        // one is the start.
        let two = distance(side2)?;
        extrusion.direction = spec.direction.map(|v| -v);
        extrusion.start = -(base + one);
        extrusion.end = -(base - two);
    }
    kernel.extrude(&extrusion)
}

// Emboss: profiles wrapped onto cylinders (mitcad#458).

/// Input of [`Kernel::wrapped_emboss`]: the regions laid onto the cylinder
/// of `axis` and `radius` by arc length and grown radially from `radius`
/// to `to_radius` (larger: out from the axis; smaller: in towards it).
///
/// The sketch plane is parallel to the axis. A sketch point at distance `s`
/// across the axis (along `axis.direction` × the sketch side's radial
/// direction, from the line where the axis lies under the sketch) and `z`
/// along it goes to the angle `s / radius` about the axis from the radial
/// direction towards the sketch plane, at `z`: lengths along the
/// circumference are the sketch's on the cylinder of `radius`. A profile
/// must stay within one turn.
///
/// Faces are named as an extrusion's: `side(<segment>)` grown from a
/// profile segment, `start(<region>)` on the cylinder of `radius` and
/// `end(<region>)` on that of `to_radius`.
///
/// On a cone (`half_angle` not zero, mitcad#473) the sketch is the cone's
/// development, laid against it along the generator under the sketch's
/// origin: `axis` starts at the apex and points where the radius grows,
/// `radius` is the cone's radius at the sketch origin's height along the
/// axis, and `to_radius - radius` is the growth along the cone's normals
/// (outwards when positive). A sketch point at `s` across the axis and `t`
/// along it (from the origin's height) goes to the distance `hypot(s, t +
/// ρ0)` from the apex along the generator turned `atan2(s, t + ρ0) /
/// sin(half_angle)` about the axis, ρ0 = `radius / sin(half_angle)`:
/// lengths and angles on the cone are the sketch's.
#[derive(Debug, Clone, Copy)]
pub struct EmbossWrapSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub regions: &'a [ProfileRegion],
    pub axis: Axis,
    pub radius: f64,
    pub to_radius: f64,
    /// Radians: 0 for a cylinder.
    pub half_angle: f64,
}

// Boundary Fill (mitcad#457).

/// The cell a point picks ([`Kernel::boundary_cells`]): its solids and the
/// tools that hold it (indices into the tools, in increasing order), which
/// tell two points in one cell apart from points in two.
#[derive(Debug, Clone)]
pub struct BoundaryCell<S> {
    pub shape: S,
    pub holders: Vec<usize>,
}

/// What [`Kernel::unstitch`] gives: the body left when the faces are taken
/// out (None when nothing is) and each named face as a surface body.
pub type Unstitched<S> = (Option<S>, Vec<S>);

// Gears (mitcad#472).

/// One stretch of a [`GearSweepSpec`]: `length` along the axis, while the
/// profile turns `twist` radians about the axis (right-handed about the
/// frame's normal: a right-hand helix) and moves `shift` along the frame's
/// x axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearStep {
    pub length: f64,
    pub twist: f64,
    pub shift: f64,
}

/// Input of [`Kernel::gear_sweep`]: closed loops of named curves in the
/// frame's coordinates (the first the outer boundary, the others holes),
/// turned by `turn` about the axis (the frame's normal through its origin)
/// and moved `start` along it, swept by the steps one after another into
/// one solid. A face swept from a curve is named `<feature>:<curve's
/// name>` (pieces of several steps `#k`), the caps `<feature>:<start_name>`
/// and `<feature>:<end_name>`.
#[derive(Debug, Clone, Copy)]
pub struct GearSweepSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub loops: &'a [Vec<crate::segments::NamedSegment>],
    pub turn: f64,
    pub start: f64,
    pub steps: &'a [GearStep],
    pub start_name: &'a str,
    pub end_name: &'a str,
}

/// Input of [`Kernel::gear_cone`]: the heel section's loops in the frame's
/// plane, ruled to their copy scaled by `scale` about the apex (on the
/// axis, `apex` along the frame's normal), and that solid's common part
/// with the blank: the closed loop `blank` in (radius, height along the
/// normal) coordinates turned about the axis. Faces are named after the
/// curves they come from, as [`Kernel::gear_sweep`] names them; the heel
/// and toe caps `<feature>:back` and `<feature>:front`.
#[derive(Debug, Clone, Copy)]
pub struct GearConeSpec<'a> {
    pub feature: FeatureUid,
    pub frame: SketchFrame,
    pub loops: &'a [Vec<crate::segments::NamedSegment>],
    pub apex: f64,
    pub scale: f64,
    pub blank: &'a [crate::segments::NamedSegment],
}

// Extrusions of a planar body face (mitcad#535).

/// Input of [`Kernel::extrude_face`]: the planar face `face` of `body`, its
/// bounded area with its holes, swept along its outward normal (against it
/// with `reversed`) by `side1` from the face moved `start` along its
/// outward normal, and with `side2` against that direction by `side2` too
/// (both > 0, or one of two negative with `side1 + side2 > 0` as a
/// [`SideEnd::Distance`] of two sides may be, mitcad#765). Faces are named
/// after `feature` as an extrusion's:
/// `side(<edge>)` from each edge of the face, by the edge's name in the
/// body (an edge without one, a surface body's free edge, by its index
/// among the face's edges), and the caps `start(<key>)` and `end(<key>)`:
/// with one side the cap at the start and the far one, with two sides the
/// far caps of side one and of side two.
#[derive(Debug, Clone)]
pub struct FaceExtrudeSpec<'a, S> {
    pub feature: FeatureUid,
    pub body: &'a S,
    /// The face's name in the body: the reference as it is found there.
    pub face: &'a FaceName,
    /// The name the caps are keyed by: the reference the feature stores.
    pub key: &'a FaceName,
    pub reversed: bool,
    pub start: f64,
    pub side1: f64,
    pub side2: Option<f64>,
    /// A direction of its own (mitcad#541), not in the face's plane: the
    /// sweep goes along it (against it with `reversed`) instead of the
    /// normal, the distances measured along it. A face that is not planar
    /// needs one, and takes no taper and no start offset.
    pub direction: Option<[f64; 3]>,
    /// Each side's taper (radians), as an extrusion's: positive widens the
    /// face away from it (its outside grows, its holes shrink); along a
    /// direction by `t tan(taper)` in the face's plane at distance `t`.
    pub taper1: f64,
    pub taper2: f64,
}

// Revolutions of a planar body face (mitcad#539).

/// Input of [`Kernel::revolve_face`]: the planar face `face` of `body`, its
/// bounded area with its holes, turned about `axis` by `angle1` (radians,
/// positive, the right hand way about the axis direction) and, with `angle2`,
/// that much the other way (>= 0); together at most a full turn. Faces are
/// named after `feature` as a revolution's: `side(<edge>)` from each edge
/// of the face off the axis, by the edge's name in the body (an edge
/// without one by its index among the face's edges), and the caps
/// `start(<key>)` and `end(<key>)`: with one side the face where it is and
/// the far end, with two sides the ends of side one and of side two; a
/// full turn has none.
#[derive(Debug, Clone)]
pub struct FaceRevolveSpec<'a, S> {
    pub feature: FeatureUid,
    pub body: &'a S,
    /// The face's name in the body: the reference as it is found there.
    pub face: &'a FaceName,
    /// The name the caps are keyed by: the reference the feature stores.
    pub key: &'a FaceName,
    pub axis: Axis,
    pub angle1: f64,
    pub angle2: Option<f64>,
}

// Dogbones (mitcad#1011).

/// Where a dogbone's relief lies at its corner ([`DogboneSpec`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DogboneKind {
    /// Along the corner's bisector: the tool moved into the corner until
    /// its circle passes through the sharp corner.
    #[default]
    Diagonal,
    /// Along a side (a T-bone): the tool kept on one face of the corner
    /// and moved along it until its circle passes through the sharp
    /// corner, notching the other face.
    AlongSide,
}

/// Input of [`Kernel::dogbone_tool`]. The edges the dogbone takes: the
/// edges `edges` names (each must be a straight inside corner); of the
/// faces `faces` names, their edges that are straight inside corners
/// parallel to `axis`, and, of a planar face across `axis` (a pocket's
/// floor, a top face), every such edge of the body with an end on the
/// face; without edges and faces, every such edge of the body. For each
/// edge a cylinder of radius `diameter / 2` runs along the whole edge,
/// its axis parallel to the edge:
///
/// - [`DogboneKind::Diagonal`]: its centre on the corner's bisector,
///   `diameter / 2 - extra` from the corner, so that its circle passes
///   `extra` beyond the sharp corner;
/// - [`DogboneKind::AlongSide`]: its centre `diameter / 2` off one face of
///   the corner (into the corner's opening) and `extra` beyond the corner
///   along that face: the face picked when the edge came from a picked face
///   of the corner, otherwise the longer of the two faces (the one that
///   reaches farther from the edge).
///
/// Faces are named after `feature`: `dogbone(<edge>)` along the cylinder,
/// `dogbone_start(<edge>)` and `dogbone_end(<edge>)` its ends, `<edge>`
/// the edge's name in the body (its index among the body's edges when it
/// has none). The cylinders are one compound, which may overlap.
#[derive(Debug, Clone, Copy)]
pub struct DogboneSpec<'a, S> {
    pub feature: FeatureUid,
    pub body: &'a S,
    pub edges: &'a [EdgeName],
    pub faces: &'a [FaceName],
    /// The router bit's axis, a unit vector.
    pub axis: [f64; 3],
    pub diameter: f64,
    pub extra: f64,
    pub kind: DogboneKind,
}
