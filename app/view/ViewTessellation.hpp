// SPDX-License-Identifier: MIT
#pragma once

#include <atomic>
#include <cstddef>

#include <Prs3d_Drawer.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {

// The view's meshes: the chordal error in model units (millimetres), the
// angle between neighbouring facets, and about how many triangles a doubly
// curved face gets at most (mitcad#398). OCCT's relative default grows with
// the whole body's bounds; its derived minimum mesh size can then override
// the angular limit on small curves of a long body. So the chordal error is
// absolute, with an angular limit for circles smaller than it; and a face
// whose deviation would take more triangles than the bound (a sphere of
// metres: millions, which froze the window) meshes with a deviation that
// grows with its size instead (io::face_deviation_floor). Cylinders and
// cones mesh at most as finely as a turn in 500 straight pieces
// (mitcad#438): those of radii over about half a metre, and their edges,
// with a deviation of 2e-5 of the radius, where that is coarser than the
// body's own. Planes go through Delabella's base triangulation, each
// checked (checkedPlaneMeshOptions, mitcad#644). From 0.29.0 that path hung
// on a large design of 6 455 planes (mitcad#846): Delabella's rounded signs
// contradicted each other on nearly cocircular boundary points, and it
// crashed or looped. The final tier was the default insertion
// (defaultPlaneMeshOptions) until OCCT port patch 1070 made the signs exact;
// the design then meshed in 10 of 10 runs on Windows.
//
// By size (mitcad#1275): every face smaller than about 6 cm, whatever its
// body, meshed at 0.01 mm with 5 degrees on its edges; a machine of 257
// bodies took 5.3 million triangles, a part of 23 x 47 x 48 mm 620 000 of
// them (its B-spline faces 56 000 each, past the face bound, which their
// curvature's estimate did not foresee), and window selection and patterns
// of such bodies were slow. Now a body meshes with a deviation of
// kViewSizeDeviation of its bounding box's diagonal (a pixel of it in a
// view of about 1 000 pixels that shows it whole), at least kViewDeviation
// (a part under 10 mm across, as before) and at most kViewDeviationMax,
// in steps of the square root of 2 (io::shape_deviation); its edges with
// kViewAngle (BRepMesh cuts them at half of it: 6 degrees a step, a circle
// of 500 pixels' radius off by less than a pixel), the faces' insides
// with kViewInteriorAngle; and every face that is not a plane is held
// within kViewFaceTriangles, a small one within a quarter of a triangle
// per square of its deviation in its area
// (io::MeshOptions::hold_face_triangles). The view's drawers draw their
// curves with kViewCurveAngle (wireframe and free edges, as before). On
// the reporter's machine (mitcad-cli mesh-compare, the final tier): 780 000
// triangles, the part above 35 000, meshed in 2.4 s.
constexpr double kViewDeviation = 0.01;
constexpr double kViewSizeDeviation = 1e-3;
constexpr double kViewDeviationMax = 0.5;
constexpr double kViewAngle = 12.0 * 3.14159265358979323846 / 180.0;
constexpr double kViewInteriorAngle = 15.0 * 3.14159265358979323846 / 180.0;
constexpr double kViewCurveAngle = 5.0 * 3.14159265358979323846 / 180.0;
constexpr std::size_t kViewFaceTriangles = 20000;

// Small circles (mitcad#879, Preferences > Display > Small circles): a
// design of 2 929 holes in each of its plates took 40.7 million triangles,
// 21 s of meshing and 609 MB of stored meshes, every hole cut in 60 pieces
// (kViewAngle's 6 degree steps). Adaptive, the default: a circle the
// view's deviation alone cuts more coarsely is cut by the deviation, in at
// least kViewSmallCircleSegments pieces a turn (io::MeshOptions::
// small_circle_segments; by the body's deviation, a circle of 365 times it
// or more keeps the 6 degrees, one of 15 times it or less gets 12 pieces).
// Full detail: the angle for every circle, as before. Only the view's
// meshes; the exports' presets mesh as they did.
enum class SmallCircles { Adaptive, FullDetail };
constexpr int kViewSmallCircleSegments = 12;

// The choice the view's meshes are made with now, for the whole process
// (set from the preference at the start and when it changes, read by every
// thread that meshes for the view).
inline std::atomic<int>& viewSmallCirclesChoice() {
  static std::atomic<int> choice{static_cast<int>(SmallCircles::Adaptive)};
  return choice;
}
inline SmallCircles viewSmallCircles() { return static_cast<SmallCircles>(viewSmallCirclesChoice().load()); }

// The view's options with the planes' choice (fast, checked).
inline io::MeshOptions viewMeshOptionsWith(bool fastPlanes, bool checkPlanes) {
  io::MeshOptions options{kViewDeviation, kViewAngle, false, kViewFaceTriangles, fastPlanes, checkPlanes};
  options.size_deviation = kViewSizeDeviation;
  options.size_deviation_max = kViewDeviationMax;
  options.interior_angle = kViewInteriorAngle;
  options.hold_face_triangles = true;
  options.small_circle_segments = viewSmallCircles() == SmallCircles::Adaptive ? kViewSmallCircleSegments : 0;
  return options;
}

// Planes by the default insertion alone: the view's final tier before
// mitcad#644 (mitcad-cli mesh-compare's reference, "default"), and what the
// checked path falls back to for a plane.
inline io::MeshOptions defaultPlaneMeshOptions() { return viewMeshOptionsWith(false, false); }

// Planes through Delabella's base triangulation, each checked
// (io::MeshOptions::check_planes, mitcad#644): a plane left without a
// triangulation, or whose triangles' area is off the area its boundary
// encloses, is triangulated again by the default insertion. The trial over
// every local corpus design (mitcad#644) gave the default's triangulation
// on all but a handful of degenerate or sliver planes, about 5.5 times
// faster on planes and up to 16 times on designs with large perforated
// plates. The view's final tier (mitcad-cli mesh-compare's "checked").
inline io::MeshOptions checkedPlaneMeshOptions() { return viewMeshOptionsWith(true, true); }

// The view's final tier: commit, refinement, opening a design, section
// clips.
inline io::MeshOptions viewMeshOptions() { return checkedPlaneMeshOptions(); }

// The interactive preview tier's meshes (mitcad#635, framework/
// DisplayMeshes): the view's deviations, angles and face bound, with planes
// through Delabella's base triangulation (io::MeshOptions::fast_planes),
// unchecked (a wrong plane is replaced when the final tier meshes the body).
// Chosen from test_preview_meshing --compare on the synthetic 83-hole
// block: as many triangles and nodes as viewMeshOptions in about a tenth
// of the time (a plane of 84 wires: 0.5 s with the default insertion,
// 30 ms), where a coarser deviation and angle took as long for a quarter
// of the triangles (mitcad#635, at 0.01 mm). Its faces record the drawers'
// deviation (io::MeshOptions::size_deviation), so a drawer finds the
// triangulations fine enough and never meshes them again.
inline io::MeshOptions previewMeshOptions() { return viewMeshOptionsWith(true, false); }

// The deviation a shape the view draws meshes with (io::shape_deviation of
// the view's options).
inline double viewShapeDeviation(const TopoDS_Shape& shape) { return io::shape_deviation(shape, viewMeshOptions()); }

// Shared by shaded face boundaries, wireframe and selection highlights:
// the drawers ask for kViewDeviation, which the view's meshes record
// (io::MeshOptions::size_deviation), and draw curves with kViewCurveAngle.
inline void setViewTessellation(const occ::handle<Prs3d_Drawer>& drawer) {
  drawer->SetTypeOfDeflection(Aspect_TOD_ABSOLUTE);
  drawer->SetMaximalChordialDeviation(kViewDeviation);
  drawer->SetDeviationAngle(kViewCurveAngle);
}

// Whether the view may mesh a shape it draws (its presentations' and
// selection's auto-triangulation): not when the job meshed it for display
// to the end (DisplayMeshes) and faces were left without a triangulation.
// Those are faces OCCT's mesher fails on, and meshing them again can take
// without end; the view would do it on the UI thread (mitcad#589). The
// view then draws the faces that have a triangulation. Nor a stand-in of
// the interactive tier (mitcad#635): its meshes are the preview's, and
// the final ones are made for its body by a job.
inline bool viewMeshes(const geometry::Shape& shape) {
  return !shape.display_interactive() && (!shape.display_meshed() || io::has_triangulation(shape.occt()));
}

// Whether a body's drawers mesh it (their auto-triangulation): only a body
// no job meshed for display. One a job meshed to the end in the final tier
// has the view's meshes already, but a drawer finds a face whose deviation
// is coarser than its own (a large face's bound, a large cylinder's)
// untessellated and runs OCCT's mesher over the whole body again to keep
// every face as it is: 0.6-0.8 s on the UI thread for a body of 6 700
// faces, at every open and every change of it (mitcad#878).
inline bool drawerMeshes(const geometry::Shape& shape) {
  return viewMeshes(shape) && !shape.display_meshed();
}

// Makes every presentation mesh with the view's options (the meshing
// algorithm of the whole process; OcctViewer sets it up): the drawer's
// deviation, the body's by its size over it, the view's angles, face bound
// and hold, planes by the default insertion. The view's drawers then mesh
// what io::mesh_in_place(shape, viewMeshOptions()) meshed the same way, and
// keep those triangulations.
inline void useViewMeshing() { io::use_bounded_presentation_meshes(viewMeshOptions()); }

// Sets the small circles' choice for the meshes made from now on (and the
// presentations' meshing with it); returns whether it changed. The meshes
// made before stay as they are: the application meshes the shown bodies
// again (framework/DisplayMeshes, remeshForDisplay).
inline bool setViewSmallCircles(SmallCircles choice) {
  if (viewSmallCirclesChoice().exchange(static_cast<int>(choice)) == static_cast<int>(choice)) {
    return false;
  }
  useViewMeshing();
  return true;
}

} // namespace mitcad
