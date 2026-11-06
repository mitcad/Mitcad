// SPDX-License-Identifier: MIT
// Roundings that run over cut-outs or protrusions of their faces
// (mitcad#311): a pole of radius 35.1 under a flange bevelled at 45
// degrees, ten flats cut into the pole whose tops lie 0.3 below the edge
// between the bevel and the pole. A rounding of that edge with radius 0.9
// touches the pole 0.373 below the edge (r tan 22.5 degrees), over each
// flat below its top: OCCT's rolling ball failed there. The bodies stored in
// .f3d files keep the rounding on the pole's surface across the flats, one
// torus round the whole edge, trimmed by the flats' tops, which grow out to
// meet it: the rounding of the pole without the flats, with the flats cut
// from it afterwards. With the port's patches 0500 and 0501 (mitcad#237)
// OCCT builds that itself. The same with lugs standing out of a face, which
// the dressup builds with the lugs cleared and joined again.

#include <cmath>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Trsf.hxx>

#include "../src/util.hpp"
#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr int kFlats = 10;
constexpr double kPole = 35.1;   // the pole's radius
constexpr double kEdge = 1.1;    // the height of the edge rounded
constexpr double kTop = 0.8;     // the flats' tops
constexpr double kFlat = 33.645; // the flats' distance from the axis

// The pole with its bevelled flange: a revolved section.
TopoDS_Shape pole() {
  BRepBuilderAPI_MakePolygon section;
  section.Add(gp_Pnt(0, 0, -3));
  section.Add(gp_Pnt(kPole, 0, -3));
  section.Add(gp_Pnt(kPole, 0, kEdge));
  section.Add(gp_Pnt(kPole + 0.5, 0, kEdge + 0.5));
  section.Add(gp_Pnt(0, 0, kEdge + 0.5));
  section.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(section.Wire()).Face();
  return BRepPrimAPI_MakeRevol(face, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape();
}

// The flats cut from a shape: chords of the pole from the bottom to kTop.
TopoDS_Shape without_flats(TopoDS_Shape shape) {
  for (int k = 0; k < kFlats; ++k) {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(kFlat, -12, -2.66), gp_Pnt(40, 12, kTop)).Shape();
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 2.0 * kPi * (k + 0.5) / kFlats);
    shape = BRepAlgoAPI_Cut(shape, BRepBuilderAPI_Transform(box, turn).Shape()).Shape();
  }
  return shape;
}

// The edge between the bevel (a cone) and the pole (a cylinder).
TopoDS_Edge bevel_edge(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& faces = faces_of(i);
    if (faces.Extent() != 2) {
      continue;
    }
    const GeomAbs_SurfaceType a = BRepAdaptor_Surface(TopoDS::Face(faces.First())).GetType();
    const GeomAbs_SurfaceType b = BRepAdaptor_Surface(TopoDS::Face(faces.Last())).GetType();
    if ((a == GeomAbs_Cone && b == GeomAbs_Cylinder) || (a == GeomAbs_Cylinder && b == GeomAbs_Cone)) {
      return TopoDS::Edge(faces_of.FindKey(i));
    }
  }
  return TopoDS_Edge();
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The body with its faces named as an imported body's.
ShapePtr named(const TopoDS_Shape& shape) {
  std::vector<Shape::NamedFace> faces;
  int k = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    faces.push_back({ex.Current(), {"F1:import(" + std::to_string(++k) + ")"}});
  }
  return std::make_shared<Shape>(shape, faces);
}

TopoDS_Shape unify(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain merge(shape);
  merge.Build();
  return merge.Shape();
}

std::string edge_name(const Shape& body, const TopoDS_Edge& edge) { return body.edge_name(body.edge_index(edge)); }

void test_rounding_over_flats_is_the_poles_with_the_flats_cut_again() {
  const ShapePtr body = named(without_flats(pole()));
  const std::string bevel = edge_name(*body, bevel_edge(body->occt()));
  CHECK(!bevel.empty());
  const ShapePtr rounded = fillet("F2", *body, {bevel}, 0.9);
  CHECK(detail::is_valid(rounded->occt()));
  // The geometry kernel builds it itself (port patches 0500 and 0501,
  // mitcad#237: the torus cut round each flat), with no note.
  CHECK(rounded->notes().empty());
  // The pole rounded without the flats, the flats cut from it after.
  const TopoDS_Shape plain = pole();
  BRepFilletAPI_MakeFillet whole(plain);
  whole.Add(0.9, bevel_edge(plain));
  whole.Build();
  CHECK(whole.IsDone());
  const TopoDS_Shape expected = without_flats(whole.Shape());
  CHECK(std::abs(volume_of(rounded->occt()) - volume_of(expected)) < 1.0e-6 * volume_of(expected));
  int faces = 0;
  for (TopExp_Explorer ex(expected, TopAbs_FACE); ex.More(); ex.Next()) {
    ++faces;
  }
  CHECK(rounded->face_count() == faces);
  // One torus round the whole edge, the flats' tops grown out to it and
  // named as before.
  CHECK(faces_named(*rounded, "F2:fillet(" + bevel + ")") == 1);
  int unnamed = 0;
  for (int f = 0; f < rounded->face_count(); ++f) {
    unnamed += rounded->face_names(f).empty() ? 1 : 0;
  }
  CHECK(unnamed == 0);
}

// The other way round, as in the design the case comes from (a bore with
// lugs standing into it): a wall whose face is bevelled at its top edge,
// two lugs standing out of the face up to 0.3 below the bevel's edge. The
// rounding of the (convex) edge with radius 0.9 touches the face 0.373 below
// the edge, over each lug's top: the bodies stored in .f3d files round the
// face as if the lugs were not there and keep the lugs, their tops grown
// out to the rounding.
TopoDS_Shape wall() {
  BRepBuilderAPI_MakePolygon section;
  section.Add(gp_Pnt(0, 0, -3));
  section.Add(gp_Pnt(0, 20, -3));
  section.Add(gp_Pnt(0, 20, kEdge + 0.5));
  section.Add(gp_Pnt(0, 0.5, kEdge + 0.5));
  section.Add(gp_Pnt(0, 0, kEdge));
  section.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(section.Wire()).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(60, 0, 0)).Shape();
}

// The lugs: blocks on the wall's face from the bottom to kTop, reaching
// into the wall (so that, joined to a rounded wall, they fill it up to
// their tops).
TopoDS_Shape with_lugs(const TopoDS_Shape& shape) {
  TopoDS_Shape joined = shape;
  for (const double x : {10.0, 35.0}) {
    joined = BRepAlgoAPI_Fuse(joined, BRepPrimAPI_MakeBox(gp_Pnt(x, -1.5, -3), gp_Pnt(x + 15, 1, kTop)).Shape())
                 .Shape();
  }
  return unify(joined);
}

// The edge between the wall's face (y = 0) and its bevel.
TopoDS_Edge wall_edge(const TopoDS_Shape& shape) {
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.Y()) < 1e-9 && std::abs(b.Y()) < 1e-9 && std::abs(a.Z() - kEdge) < 1e-9 &&
        std::abs(b.Z() - kEdge) < 1e-9) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

void test_rounding_over_lugs_is_the_faces_with_the_lugs_joined_again() {
  const ShapePtr body = named(with_lugs(wall()));
  const std::string bevel = edge_name(*body, wall_edge(body->occt()));
  CHECK(!bevel.empty());
  const ShapePtr rounded = fillet("F2", *body, {bevel}, 0.9);
  CHECK(detail::is_valid(rounded->occt()));
  // (Built with the lugs cleared and put back: no note on how, mitcad#1327.)
  CHECK(rounded->notes().empty());
  // The face rounded without the lugs, the lugs joined to it after.
  const TopoDS_Shape plain = wall();
  BRepFilletAPI_MakeFillet whole(plain);
  whole.Add(0.9, wall_edge(plain));
  whole.Build();
  CHECK(whole.IsDone());
  const TopoDS_Shape expected = with_lugs(whole.Shape());
  CHECK(std::abs(volume_of(rounded->occt()) - volume_of(expected)) < 1.0e-6 * volume_of(expected));
  int faces = 0;
  for (TopExp_Explorer ex(expected, TopAbs_FACE); ex.More(); ex.Next()) {
    ++faces;
  }
  CHECK(rounded->face_count() == faces);
  CHECK(faces_named(*rounded, "F2:fillet(" + bevel + ")") == 1);
  int unnamed = 0;
  for (int f = 0; f < rounded->face_count(); ++f) {
    unnamed += rounded->face_names(f).empty() ? 1 : 0;
  }
  CHECK(unnamed == 0);
}


void test_rounding_within_the_band_does_not_fill_the_flats() {
  // Radius 0.7 touches the pole 0.29 below the edge, above the flats: built
  // as before, without the cut-outs.
  const ShapePtr body = named(without_flats(pole()));
  const std::string bevel = edge_name(*body, bevel_edge(body->occt()));
  const ShapePtr rounded = fillet("F2", *body, {bevel}, 0.7);
  CHECK(detail::is_valid(rounded->occt()));
  CHECK(rounded->notes().empty());
}

} // namespace

void fillet_cutout_tests() {
  guarded("test_rounding_over_flats_is_the_poles_with_the_flats_cut_again",
          test_rounding_over_flats_is_the_poles_with_the_flats_cut_again);
  guarded("test_rounding_over_lugs_is_the_faces_with_the_lugs_joined_again",
          test_rounding_over_lugs_is_the_faces_with_the_lugs_joined_again);
  guarded("test_rounding_within_the_band_does_not_fill_the_flats",
          test_rounding_within_the_band_does_not_fill_the_flats);
}

} // namespace test
