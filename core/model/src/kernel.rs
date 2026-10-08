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
pub use crate::features::ChamferCorner;
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

pub trait Kernel {
    /// A kernel-owned shape with face names (see [`crate::topo`]). Cloning
    /// must be cheap (a shared handle); shapes are never modified.
    type Shape: Clone;

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
    /// section analysis shows it.
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
    fn helix(&self, _spec: &HelixSpec<'_>) -> Result<Self::Shape, KernelError> {
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub origin: [f64; 3],
    pub direction: [f64; 3],
}

/// A cylindrical face (all pieces of a split one).
#[derive(Debug, Clone, Copy, PartialEq)]
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
/// hole), centred on them, or on side 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallLocation {
    Side1,
    Center,
    Side2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThinWall {
    pub location: WallLocation,
    pub thickness: f64,
}

#[derive(Debug, Clone)]
pub enum SideEnd<S> {
    /// From the start, > 0.
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
/// side one up to `target`; at most a full turn in total. Faces are named
/// as an extrusion's; a full turn has no caps.
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

/// Input of [`Kernel::modeled_thread`]: the 60-degree profile (ISO 68-1,
/// Unified) of `pitch`, half a pitch wide at `pitch_diameter`, with flat
/// crests and roots at the `major` and `minor` diameters, built on each
/// face: along the threaded part the material becomes the thread (removed
/// beyond the profile, added where the face lies inside the crest).
/// `angle` (radians) turns the thread about its axis from where its
/// groove (an internal thread's tooth) spans the first half pitch from the
/// face's low end at the reference direction (the X axis projected across
/// the axis; Y for axes near X; `geometry/include/mitcad/geometry/thread.hpp`).
/// `part` limits it to (length, offset, from the high end of the
/// cylinder's axis); otherwise it covers the face and runs out through its
/// free ends. The thread's faces are named `<feature>:thread(<face>)`.
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
    pub part: Option<(f64, f64, bool)>,
}

/// [`Kernel::extrude_feature`] with [`Kernel::extrude`]: a prism between
/// two offsets along the direction.
pub(crate) fn simple_extrude<K: Kernel + ?Sized>(
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
