// SPDX-License-Identifier: MIT
#pragma once

// Which triangles of a shown body's faces turn their outside towards the
// eye (mitcad#2062), so that the view's pick places ("Pick places:", which
// the UI tests log at every input) need not try a face where a click can
// never pick it. Each try is a pick in the view, and a face was tried at up
// to 26 points: on a large design most of its faces lie behind the others,
// and one list took 30-70 s. A closed shell hides from the eye every
// triangle whose outside turns away from it: the pointer's ray meets the
// shell's near side first, so a click there picks that side. Skipping
// those triangles (and a face that has no other) leaves about half of a
// solid's faces and fallback points untried; the places found stay the
// same kind of place, a point where a click picks the face.
//
// Only what is known for certain is said: a face is in the result only on
// a closed shell whose triangles, turned by its faces' surfaces, enclose a
// positive volume (an inside-out shell, a cavity, an open surface body, a
// mesh body without surfaces, a face without its triangulation or its
// nodes' surface parameters are left out, so they are tried everywhere as
// before), and never with the eye inside the shell's box in a perspective
// view. A triangle nearly edge-on to the eye counts as facing it: a pick
// within its pick width there can still find it. Callers leave out shells
// the view clips (a section), which shows their insides.
//
// OCCT geometry only, for app.unit.

#include <cmath>
#include <cstddef>
#include <optional>
#include <utility>
#include <vector>

#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_Map.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace mitcad {

// The view's camera, in the coordinates the placement moves a body into.
struct PickEye {
  gp_Pnt eye;
  gp_Dir direction; // where the camera looks
  bool perspective = false;
};

// Per face (as TopTools_ShapeMapHasher tells faces apart: the same
// TShape and location) of a closed, outward shell: for each triangle of its
// triangulation (triangle t at index t - 1) whether its outside may face
// the eye. A face that is not in the map may face either way.
using PickFacing = NCollection_DataMap<TopoDS_Shape, std::vector<bool>, TopTools_ShapeMapHasher>;

namespace pick_facing_detail {

// How far past edge-on (the cosine between a triangle's outward normal and
// the way to the eye) a triangle still counts as facing the eye.
constexpr double kEdgeOn = 0.05;

// A face's triangles where the placement puts them: three corners each and
// the outward normal (twice the triangle's area long).
struct FaceTriangles {
  std::vector<gp_Pnt> corners;
  std::vector<gp_Vec> outward;
};

// The face's triangles, turned out of its solid by the surface's normal at
// its largest triangle (an imported triangulation may wind either way,
// mitcad#319), or nothing where that cannot be told.
inline std::optional<FaceTriangles> outwardTriangles(const TopoDS_Face& face, const gp_Trsf& placement) {
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(face, location);
  if (mesh.IsNull() || mesh->NbTriangles() == 0 || !mesh->HasUVNodes()) {
    return std::nullopt;
  }
  TopLoc_Location surfaceLocation;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, surfaceLocation);
  if (surface.IsNull()) {
    return std::nullopt;
  }
  const gp_Trsf toView = placement.Multiplied(location.Transformation());
  FaceTriangles triangles;
  triangles.corners.reserve(static_cast<std::size_t>(mesh->NbTriangles()) * 3);
  triangles.outward.reserve(static_cast<std::size_t>(mesh->NbTriangles()));
  int largest = 0;
  double largestArea = -1.0;
  for (int t = 1; t <= mesh->NbTriangles(); ++t) {
    int a = 0;
    int b = 0;
    int c = 0;
    mesh->Triangle(t).Get(a, b, c);
    const gp_Pnt pa = mesh->Node(a).Transformed(toView);
    const gp_Pnt pb = mesh->Node(b).Transformed(toView);
    const gp_Pnt pc = mesh->Node(c).Transformed(toView);
    const gp_Vec facet = gp_Vec(pa, pb).Crossed(gp_Vec(pa, pc));
    triangles.corners.insert(triangles.corners.end(), {pa, pb, pc});
    triangles.outward.push_back(facet);
    if (facet.SquareMagnitude() > largestArea) {
      largestArea = facet.SquareMagnitude();
      largest = t;
    }
  }
  if (largestArea <= 0.0) {
    return std::nullopt;
  }
  int a = 0;
  int b = 0;
  int c = 0;
  mesh->Triangle(largest).Get(a, b, c);
  const gp_XY uv = (mesh->UVNode(a).XY() + mesh->UVNode(b).XY() + mesh->UVNode(c).XY()) / 3.0;
  GeomLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1e-9);
  if (!props.IsNormalDefined()) {
    return std::nullopt;
  }
  gp_Vec normal(props.Normal());
  normal.Transform(placement.Multiplied(surfaceLocation.Transformation()));
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  const double along = normal.Dot(triangles.outward[static_cast<std::size_t>(largest - 1)]);
  if (along == 0.0) {
    return std::nullopt;
  }
  if (along < 0.0) {
    for (gp_Vec& outward : triangles.outward) {
      outward.Reverse();
    }
  }
  return triangles;
}

} // namespace pick_facing_detail

// The faces of `shape`, moved by `placement`, whose triangles' facing is
// known, with each triangle's.
inline PickFacing pickFacing(const TopoDS_Shape& shape, const gp_Trsf& placement, const PickEye& eye) {
  using pick_facing_detail::FaceTriangles;
  PickFacing facing;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> twice; // faces on more than one shell
  for (TopExp_Explorer shells(shape, TopAbs_SHELL); shells.More(); shells.Next()) {
    const TopoDS_Shape& shell = shells.Current();
    if (!BRep_Tool::IsClosed(shell)) {
      continue;
    }
    std::vector<std::pair<TopoDS_Shape, FaceTriangles>> faces;
    bool known = true;
    double volume = 0.0; // six times the enclosed volume
    Bnd_Box box;
    for (TopExp_Explorer explorer(shell, TopAbs_FACE); explorer.More() && known; explorer.Next()) {
      const TopoDS_Face& face = TopoDS::Face(explorer.Current());
      std::optional<FaceTriangles> triangles = pick_facing_detail::outwardTriangles(face, placement);
      if (!triangles) {
        known = false;
        break;
      }
      for (std::size_t t = 0; t < triangles->outward.size(); ++t) {
        const gp_Pnt& corner = triangles->corners[t * 3];
        volume += gp_Vec(corner.XYZ()).Dot(triangles->outward[t]);
        box.Add(corner);
        box.Add(triangles->corners[t * 3 + 1]);
        box.Add(triangles->corners[t * 3 + 2]);
      }
      faces.emplace_back(face, std::move(*triangles));
    }
    if (!known || faces.empty() || volume <= 0.0) {
      continue;
    }
    if (eye.perspective) {
      Bnd_Box around = box;
      around.Enlarge(1e-6 + 1e-3 * std::sqrt(box.SquareExtent()));
      if (!around.IsOut(eye.eye)) {
        continue;
      }
    }
    const gp_Vec towardsEye = gp_Vec(eye.direction).Reversed();
    for (const auto& [face, triangles] : faces) {
      if (twice.Contains(face)) {
        continue;
      }
      if (facing.IsBound(face)) {
        facing.UnBind(face);
        twice.Add(face);
        continue;
      }
      std::vector<bool> front(triangles.outward.size(), true);
      for (std::size_t t = 0; t < triangles.outward.size(); ++t) {
        const gp_Vec& outward = triangles.outward[t];
        gp_Vec toEye = towardsEye;
        if (eye.perspective) {
          const gp_XYZ middle =
              (triangles.corners[t * 3].XYZ() + triangles.corners[t * 3 + 1].XYZ() + triangles.corners[t * 3 + 2].XYZ()) /
              3.0;
          toEye = gp_Vec(gp_Pnt(middle), eye.eye);
        }
        const double length = outward.Magnitude() * toEye.Magnitude();
        front[t] = length <= 0.0 || outward.Dot(toEye) > -pick_facing_detail::kEdgeOn * length;
      }
      facing.Bind(face, std::move(front));
    }
  }
  return facing;
}

} // namespace mitcad
