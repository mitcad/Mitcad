// SPDX-License-Identifier: MIT
#pragma once

// Triangulation of B-rep bodies for mesh export and mesh bodies for display.

#include <array>
#include <cstddef>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

#include <GeomAbs_SurfaceType.hxx>
#include <IMeshTools_MeshAlgoFactory.hxx>
#include <Message_ProgressRange.hxx>
#include <Poly_Triangulation.hxx>
#include <Standard_Handle.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>

namespace mitcad::io {

// Mesh refinement presets for export.
enum class Refinement {
  Low,    // 0.1 mm deviation, 30 degrees between normals, 5 000 triangles a face
  Medium, // 0.03 mm, 15 degrees, 20 000 triangles
  High,   // 0.01 mm, 8 degrees, 50 000 triangles
};

// Tolerances of a triangulation.
struct MeshOptions {
  // Largest distance between the mesh and the surface (surface deviation),
  // in mm; with `relative`, a fraction of each edge's size instead.
  double linear_deflection = 0.03;
  // Largest angle between the normals of neighbouring facets, in radians.
  double angular_deflection = 0.2617993877991494; // 15 degrees
  bool relative = false;
  // About how many triangles a doubly curved face may get at most (0: no
  // limit, `linear_deflection` everywhere). A face whose deviation would
  // give it more meshes with a larger deviation instead, which grows with
  // the face's size (face_deviation_floor), so that a sphere of metres
  // gets about as many triangles as one of centimetres (mitcad#398). The
  // edges keep `linear_deflection`, so neighbouring faces still share
  // their nodes. Small faces are meshed as without a limit. Cylinders and
  // cones (whose triangles are mostly their edges' points) mesh at most as
  // finely as a turn in face_triangles / 40 straight pieces, 500 for the
  // view's 20 000 (mitcad#438): their faces and the edges that bound them
  // mesh with that turn's deviation, 2e-5 of the radius for 500, where it
  // is more than `linear_deflection` (radii over half a metre at 0.01 mm;
  // BRepMesh cuts an edge at half its deviation, so a circle gets about
  // 700 points), an edge at the smallest floor of the faces it bounds
  // (planes having no say) and its own curve's. Smaller cylinders and
  // cones are meshed as without a limit.
  std::size_t face_triangles = 0;
  // Planes triangulated through OCCT's Delabella base triangulation (a
  // sweep-hull Delaunay of the boundary's points) and then the same
  // constrained Delaunay as by default, instead of the default incremental
  // (Watson) insertion of the points: the same nodes, edges and deviation,
  // other triangles. The default's insertion grows much faster than
  // linearly with the points: a plane with 83 small holes (12 000 points
  // at the view's deviation) takes about 0.5 s, Delabella's 30 ms
  // (mitcad#635). The view's interactive preview tier (ViewTessellation.hpp).
  bool fast_planes = false;
  // With fast_planes: each plane's triangulation checked (mitcad#644). A
  // plane that Delabella leaves without one, or whose triangles' area
  // differs from the area its boundary's discretization encloses (the
  // outer wire's polygon less the holes', in the plane's parameters) by
  // more than 0.1 % of it or 0.001 mm², whichever is larger, is
  // triangulated again by the default insertion. Delabella's base
  // triangulation can cover a sliver many times over, or leave a regular
  // face undrawn where the default meshes it; the default's result is
  // kept as it comes, a face it fails on left undrawn. The check is one
  // area sum per plane. The view's final tier (ViewTessellation.hpp).
  bool check_planes = false;

  static MeshOptions from(Refinement refinement);
};

// The area (mm²) a plane's triangulation covers, and the area its edges'
// polygons on it enclose (the outer wire's less the holes'), both in the
// plane's parameters, which are lengths; -1 for both when the face is not
// a plane, has no triangulation with parameter nodes, or an edge has no
// polygon on it. What MeshOptions::check_planes compares, measured on a
// meshed face (tests, mitcad#644).
struct PlaneMeshArea {
  double triangles = -1.0;
  double boundary = -1.0;
};
PlaneMeshArea plane_mesh_area(const TopoDS_Face& face);

// Whether a plane's triangulated area fits the area its boundary encloses
// (MeshOptions::check_planes): within 0.1 % of it or 0.001 mm², whichever
// is larger.
bool plane_area_fits(double triangulated, double boundary);

// For tests (mitcad#644): the algorithms MeshOptions::fast_planes takes a
// plane's triangulation from, in place of Delabella's, for the meshings
// started after the call (a stand-in that fails or covers too much, which
// check_planes must catch); a null handle restores Delabella's.
void use_fast_plane_algorithms_for_tests(const occ::handle<IMeshTools_MeshAlgoFactory>& factory);

// For tests (mitcad#846): the algorithms every face's triangulation is
// taken from in place of the default insertion (also check_planes' second
// triangulation), for the meshings started after the call (a stand-in for
// a face that does not end); a null handle restores the default.
void use_face_algorithms_for_tests(const occ::handle<IMeshTools_MeshAlgoFactory>& factory);

// How long a stopped meshing waits for the faces being triangulated to end
// (mitcad#846): a face still not done by then is abandoned (mesh_in_place).
constexpr double kMeshAbandonSeconds = 2.0;

// The workers of abandoned meshings (mesh_in_place, mitcad#846) that have
// not ended yet, in the whole process: each was triangulating a face, or
// about to take the next, when its meshing was abandoned, and goes on
// inside OCCT's mesher until the face ends (it may never), then releases
// the meshing's data. While this is not 0, a process must end without its
// static destructors (std::_Exit), which would tear down OCCT's and the C++
// runtime's state under them and crash the process at its end (mitcad#936,
// as for the imports' abandoned threads, mitcad#82): the application,
// mitcad-cli and mitcad-geomtool do.
std::size_t abandoned_faces();

// The smallest surface deviation (mm) that keeps a face's mesh within
// about `face_triangles` triangles: proportional to the integral of the
// square root of the surface's Gaussian curvature over the face (a
// sphere's or torus's, zero for planes and extrusions, whose triangle
// count does not grow with their size), estimated over the face's
// parameter range. 0 without a limit or for a face too small to need one
// (its box's diagonal below the deviation times face_triangles divided by
// 3.2). For a cylinder or a cone, the deviation of a turn round it (at the
// cone's largest radius on the face) in face_triangles / 40 straight
// pieces, when that is more than `deviation`, else 0 (mitcad#438).
double face_deviation_floor(const TopoDS_Face& face, double deviation, std::size_t face_triangles);

// A triangulated copy of the shape; the shape itself is not modified (OCCT
// keeps triangulations in the faces, so meshing the original would change
// what other users of the shape see). A mesh body, or any shape with a face
// that has no surface, is returned as it is.
TopoDS_Shape triangulate(const TopoDS_Shape& shape, const MeshOptions& options);

// What one meshing did (mitcad#635): its faces, those whose triangulation
// met the options and was kept, those meshed and those OCCT's mesher failed
// on, how long each of BRepMesh's phases took (milliseconds: building its
// model of the faces, discretizing the edges, healing the model and
// raising faces to their floors, checking the triangulations there are,
// triangulating the faces, writing the triangulations into the faces) and
// the faces that took longest, slowest first.
struct MeshReport {
  std::size_t faces = 0;
  std::size_t kept = 0;
  std::size_t meshed = 0;
  std::size_t failed = 0;
  // Planes triangulated again by the default insertion after Delabella's
  // triangulation was missing or its area off (MeshOptions::check_planes).
  std::size_t planes_redone = 0;
  // Faces whose triangulation had not ended kMeshAbandonSeconds after the
  // meshing was stopped, and was abandoned (mesh_in_place, mitcad#846).
  std::size_t abandoned = 0;
  double build = 0.0;
  double edges = 0.0;
  double heal = 0.0;
  double check = 0.0;
  double triangulate = 0.0;
  double write = 0.0;
  double total = 0.0;
  // Faces meshed, by surface kind ("plane 7, cylinder 2").
  std::vector<std::pair<std::string, std::size_t>> meshed_by_surface;
  struct FaceTime {
    std::string surface;
    int wires = 0;
    int points = 0; // of its wires' discretized edges
    double milliseconds = 0.0;
    TopoDS_Face face; // as the mesher explored it
  };
  std::vector<FaceTime> slowest; // at most 5
  // Set before the meshing: every face's time in `every_face_time`, in no
  // particular order (a face triangulated, whether or not that succeeded;
  // mesh_compare.hpp, mitcad#644).
  bool every_face = false;
  std::vector<FaceTime> every_face_time;
  // One line: "faces=94 kept=0 meshed=94 failed=0 build=... ms; meshed
  // plane 3, cylinder 91; slowest plane wires=84 points=6060 412.3 ms, ...",
  // with "; planes redone 2" when any were and "; abandoned 1" when a face
  // was abandoned.
  std::string line() const;
};

// A surface kind's name in MeshReport's terms: "plane", "cylinder", "cone",
// "sphere", "torus", "bezier", "bspline", "revolution", "extrusion",
// "offset" or "other".
const char* surface_name(GeomAbs_SurfaceType type);

// Whether MITCAD_MESH_TRACE asks for a line per meshing (mitcad#635): the
// view's meshes made on the model's thread (app/framework/DisplayMeshes)
// and, on standard error, every meshing OCCT's presentations ask for
// ("mesh-trace: presentation ...": on the UI thread). Off by default.
bool mesh_tracing();

// How many faces the presentations' meshing algorithm (use_bounded_
// presentation_meshes) has triangulated in this process: a face the view
// had to mesh itself, on the UI thread, because no job meshed it as it
// draws it (tests, mitcad#635).
std::size_t presentation_faces_meshed();

// Triangulates the shape's own faces, as OCCT's viewer does, and keeps
// triangulations that meet the options (each face's deviation raised to its
// face_deviation_floor). For the shapes the 3D view shows: meshed where
// nothing else uses them (the model's worker thread, mitcad#398), the view
// then finds them meshed. Returns false when `range` was stopped (the faces
// meshed so far keep their triangulations); a mesh body is left as it is.
// `report` (optional) gets what the meshing did.
//
// A stop returns within about kMeshAbandonSeconds even when a face's
// triangulation does not end (mitcad#846: OCCT's algorithms test the stop
// between faces, and inside a face only in a part of the default
// insertion). With a range that can be stopped, the faces are triangulated
// on threads of the meshing's own, each on a private copy of its face,
// whose triangulation is moved into the face when it ends. Once stopped,
// the meshing waits that long for the faces under way; a face still not
// done is abandoned: its thread goes on in the background with what it
// needs, and its result is discarded when it ends, never written into the
// shape (MeshReport::abandoned, abandoned_faces). Without such a range,
// OCCT's thread pool as before.
//
// A face whose triangulation throws, whatever the exception (OCCT's, a
// standard one, any other), is left failed (MeshReport::failed) and the
// other faces are meshed (mitcad#936); when that was an allocation that
// failed (std::bad_alloc, OCCT's Standard_OutOfMemory), the meshing then
// throws Error "triangulation: out of memory" (so does triangulate's;
// mesh_all_in_place takes it as any exception of its joint meshing).
bool mesh_in_place(const TopoDS_Shape& shape, const MeshOptions& options,
                   const Message_ProgressRange& range = Message_ProgressRange(), MeshReport* report = nullptr);

// mesh_in_place for several shapes at once (the view's meshes of all the
// bodies of a job, mitcad#438): one meshing of all their faces, which OCCT
// spreads over every core, where mesh_in_place spreads only one shape's
// faces (a design of many small bodies used a core or two). Each face gets
// the triangulation mesh_in_place gives it; a face that several shapes
// share (a body's copy in another placement) is meshed once. Null shapes
// are skipped; a shape with a face without a surface is left as it is.
// When the joint meshing throws, the shapes it did not finish are meshed
// one by one and a shape OCCT fails on is left as it is. Returns how many
// shapes were meshed (or left as they are): all of them unless `range` was
// stopped, then those whose faces all have triangulations. `report`
// (optional) gets what the joint meshing did.
std::size_t mesh_all_in_place(const std::vector<TopoDS_Shape>& shapes, const MeshOptions& options,
                              const Message_ProgressRange& range = Message_ProgressRange(),
                              MeshReport* report = nullptr);

// Gives each face of `shape` that has no triangulation a copy of the
// triangulation of the same face among `donors`, with its edges' polygons
// (mitcad#635): a boolean's result keeps its operands' untouched faces as
// copies sharing their surfaces, curves and parameter curves, and the
// donors are the operands as the view shows them, meshed already. Same
// means on the same surface and location, with the same edges in the same
// order, each on the same curve and parameter curves over the same range
// between the same points; a face any of whose edges has no polygon on the
// donor's triangulation is not taken. Nothing is kept between calls (no
// cache to invalidate): the donors are read, never written, and only the
// nodes, parameters and triangles are read (not the normals a presentation
// adds), so the view may draw them meanwhile; `shape` is written, and must
// be the caller's alone. The caller passes donors meshed with the options
// it meshes with (the view's display meshes): the meshing that follows
// keeps a reused face only when its deviation meets the options, as for
// any face meshed before. Returns how many faces were given one.
std::size_t reuse_triangulations(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& donors);

// Whether every face of the shape has a triangulation.
bool has_triangulation(const TopoDS_Shape& shape);

// Makes OCCT's presentations (StdPrs, AIS) mesh shapes with mesh_in_place's
// algorithm: the drawer's deviation and angle, and each face's deviation
// raised to its floor for `face_triangles` (BRepMesh_DiscretFactory's
// default algorithm, for the whole process).
void use_bounded_presentation_meshes(std::size_t face_triangles);

// Triangle and node count over the faces' triangulations.
struct MeshStats {
  std::size_t faces = 0;
  std::size_t triangles = 0;
  std::size_t nodes = 0;
};
MeshStats mesh_stats(const TopoDS_Shape& shape);

// A mesh body: one face that carries the triangulation and no surface.
// OCCT's viewer displays it like any other face.
TopoDS_Shape mesh_body(const occ::handle<Poly_Triangulation>& triangulation);

// True when the shape has faces and none of them has a surface.
bool is_mesh(const TopoDS_Shape& shape);

// A shape as one triangle mesh for 3D printing (3MF, mitcad#13): the
// vertices of all faces with those that coincide (within 1e-6 mm) merged,
// so the triangles of neighbouring faces share their vertices along the
// edges, and a closed solid gives a closed mesh; triangles that collapse
// in the merge (slivers at degenerate edges) are left out. Triangles are
// counter-clockwise seen from outside (a reversed face's are turned
// round). Millimetres, in the shape's own placement.
struct IndexedMesh {
  std::vector<std::array<double, 3>> vertices;
  std::vector<std::array<std::uint32_t, 3>> triangles;
};

// The shape triangulated with `options` on a copy (see triangulate); a
// mesh body's own triangles. Throws Error when a face has no triangulation.
IndexedMesh indexed_mesh(const TopoDS_Shape& shape, const MeshOptions& options);

} // namespace mitcad::io
