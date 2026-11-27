// SPDX-License-Identifier: MIT
// A rounding of an overhang's bottom edge that starts where an earlier
// rounding ends (GitHub mitcad/Mitcad#572, mitcad#2276), rebuilt with sizes
// taken from the report's picture: a block 60 x 60, 20 high (the ledge) and
// on its top a block 90 x 40, 30 high, set back 20 from the lower block's
// front and flush with it at the left and the back; the upper block reaches
// 30 past the lower block's end face at x = 60 (the overhang). The front
// edge where the upper block's front face meets the ledge's top and the
// overhang's underside is one line at y = 20, z = 20: concave from x = 0 to
// 60 (rounded first with 5, F3), convex from 60 to 90 (F4).
//
// F3 adds material in front of the upper block's front face; F4 takes
// material away behind it, so they do not overlap. F4 ends flat in the plane
// of the lower block's end face (x = 60), that face grown up behind the
// front face to meet it: the end face reaches up the front face only to
// F3's top edge and not at all behind it, so the geometry kernel's own
// rounding finds no face to end on and left the body open at every size.
// The fillet ends the chain on that plane (ended_on_end_planes in
// dressup.cpp). Each radius must build one valid closed solid, F3's faces
// as they were, the volume exact: F3 adds 25 (1 - pi / 4) along 60, F4 takes
// r^2 (1 - pi / 4) away along 30. A radius larger than the upper block is
// high above the ledge either builds or fails in words, never that the
// result is not a closed solid. The kernel's own result for each radius is
// printed first, for the log.

#include <cmath>
#include <cstdio>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kFirst = 5.0; // F3's radius
constexpr double kEnd = 60.0;  // the lower block's end face, x
constexpr double kTip = 90.0;  // the overhang's end, x
constexpr double kLine = 20.0; // the front edge's y and z
// The cross-section a rounding of r adds or takes away, over r^2.
const double kCorner = 1.0 - kPi / 4.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props, 1e-9);
  return props.Mass();
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

// One solid, every shell of it closed, valid.
bool closed_solid(const TopoDS_Shape& shape) {
  if (count_of(shape, TopAbs_SOLID) != 1 || !BRepCheck_Analyzer(shape).IsValid()) {
    return false;
  }
  int shells = 0;
  for (TopExp_Explorer ex(shape, TopAbs_SHELL); ex.More(); ex.Next()) {
    ++shells;
    if (!BRep_Tool::IsClosed(ex.Current())) {
      return false;
    }
  }
  return shells > 0;
}

std::string notes(const Shape& shape) {
  std::string text;
  for (const std::string& note : shape.notes()) {
    text += (text.empty() ? "" : "; ") + note;
  }
  return text;
}

// A rectangle from (x, y), its sides named as a sketch names them: `names`
// bottom, right, top and left.
Region named_rectangle(double x, double y, double width, double height, const std::vector<std::string>& names,
                       const std::string& region) {
  Region r = rectangle_region(x, y, width, height);
  r.name = region;
  for (std::size_t i = 0; i < 4; ++i) {
    r.loops[0].segments[i].name = names[i];
  }
  return r;
}

ShapePtr extruded(const std::string& feature, Region region, double base, double height) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.frame.origin = gp_Pnt(0.0, 0.0, base);
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = height;
  return extrude(spec);
}

// The body: F1 the lower block (x 0 to 60, y 0 to 60, z 0 to 20), F2 the
// upper one (x 0 to 90, y 20 to 60, z 20 to 50), joined.
ShapePtr ledge_body() {
  const ShapePtr lower = extruded(
      "F1", named_rectangle(0, 0, 60, 60, {"c1[c4,c2]", "c2[c1,c3]", "c3[c2,c4]", "c4[c3,c1]"},
                            "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"),
      0.0, 20.0);
  const ShapePtr upper = extruded(
      "F2", named_rectangle(0, 20, 90, 40, {"c5[c8,c6]", "c6[c5,c7]", "c7[c6,c8]", "c8[c7,c5]"},
                            "r{c5[c8,c6],c6[c5,c7],c7[c6,c8],c8[c7,c5]}"),
      20.0, 30.0);
  return boolean(BooleanOp::Join, {lower.get()}, *upper).pieces.at(0).shape;
}

// The straight edge whose middle is at `middle`, or null.
TopoDS_Edge line_edge_at(const TopoDS_Shape& shape, const gp_Pnt& middle) {
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge e = TopoDS::Edge(ex.Current());
    BRepAdaptor_Curve c(e);
    if (c.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (m.Distance(middle) < 1e-7) {
      return e;
    }
  }
  return TopoDS_Edge();
}

FilletSet set_at(const Shape& body, const gp_Pnt& middle, double radius) {
  FilletSet set;
  set.radius = radius;
  const TopoDS_Edge e = line_edge_at(body.occt(), middle);
  CHECK(!e.IsNull());
  const int index = e.IsNull() ? -1 : body.edge_index(e);
  CHECK(index >= 0);
  if (index >= 0) {
    set.edges.push_back(body.edge_name(index));
  }
  return set;
}

// The middles of the two parts of the front edge.
const gp_Pnt kConcaveMiddle(0.5 * kEnd, kLine, kLine);
const gp_Pnt kConvexMiddle(0.5 * (kEnd + kTip), kLine, kLine);

// The names of the faces whose names start with `start`, and their area.
struct Named {
  std::set<std::string> names;
  double area = 0.0;
};

Named named_faces(const Shape& shape, const std::string& start) {
  Named out;
  for (int f = 0; f < shape.face_count(); ++f) {
    bool counted = false;
    for (const std::string& name : shape.face_names(f)) {
      if (name.rfind(start, 0) == 0) {
        out.names.insert(name);
        if (!counted) {
          out.area += area_of(shape.face(f));
          counted = true;
        }
      }
    }
  }
  return out;
}

// The face of `shape` that `point` lies on (within 1e-6), or null.
TopoDS_Face face_through(const TopoDS_Shape& shape, const gp_Pnt& point) {
  const TopoDS_Shape vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepExtrema_DistShapeShape distance(vertex, ex.Current());
    if (distance.IsDone() && distance.Value() < 1e-6) {
      return TopoDS::Face(ex.Current());
    }
  }
  return TopoDS_Face();
}

// F3, the concave part rounded with 5.
ShapePtr first_rounding() {
  const ShapePtr body = ledge_body();
  CHECK_NEAR_TOL(volume_of(body->occt()), 60.0 * 60.0 * 20.0 + 90.0 * 40.0 * 30.0, 1e-9);
  ShapePtr first;
  try {
    first = fillet("F3", *body, {set_at(*body, kConcaveMiddle, kFirst)});
  } catch (const std::exception& failure) {
    std::printf("  F3: %s\n", failure.what());
  }
  CHECK(first != nullptr);
  if (first) {
    CHECK(closed_solid(first->occt()));
    CHECK_NEAR_TOL(volume_of(first->occt()), volume_of(body->occt()) + kFirst * kFirst * kCorner * kEnd, 1e-7);
  }
  return first;
}

// The geometry kernel's own rounding of the convex part after F3, printed.
void test_kernel_alone() {
  const ShapePtr first = first_rounding();
  if (!first) {
    return;
  }
  for (const double radius : {3.0, 5.0, 8.0, 12.0}) {
    const TopoDS_Shape copy = BRepBuilderAPI_Copy(first->occt()).Shape();
    const TopoDS_Edge e = line_edge_at(copy, kConvexMiddle);
    if (e.IsNull()) {
      std::printf("  kernel %g: no edge\n", radius);
      continue;
    }
    BRepFilletAPI_MakeFillet maker(copy);
    maker.Add(radius, e);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::printf("  kernel %g: threw %s\n", radius, failure.what());
      continue;
    }
    if (!maker.IsDone()) {
      std::printf("  kernel %g: failed, %d faulty contours\n", radius, maker.NbFaultyContours());
      continue;
    }
    const TopoDS_Shape made = maker.Shape();
    std::printf("  kernel %g: built, closed solid %d, volume %.9f\n", radius, closed_solid(made) ? 1 : 0,
                volume_of(made));
  }
}

// The reported case: F4 at 3, 5, 8 and 12 builds one valid closed solid at
// its exact size, its end flat on the end plane, F3's faces unchanged, the
// input untouched.
void test_overhang_rounded_from_end_plane() {
  const ShapePtr first = first_rounding();
  if (!first) {
    return;
  }
  const double before = volume_of(first->occt());
  const int faces_before = count_of(first->occt(), TopAbs_FACE);
  const Named first_faces = named_faces(*first, "F3:fillet(");
  CHECK(!first_faces.names.empty());
  for (const double radius : {3.0, 5.0, 8.0, 12.0}) {
    ShapePtr second;
    try {
      second = fillet("F4", *first, {set_at(*first, kConvexMiddle, radius)});
    } catch (const std::exception& failure) {
      std::printf("  F4 %g: %s\n", radius, failure.what());
    }
    CHECK(second != nullptr);
    // (The input is not modified.)
    CHECK_NEAR_TOL(volume_of(first->occt()), before, 1e-12);
    CHECK(count_of(first->occt(), TopAbs_FACE) == faces_before);
    if (!second) {
      continue;
    }
    std::printf("F4 %g: volume %.9f, %d faces, notes: %s\n", radius, volume_of(second->occt()),
                count_of(second->occt(), TopAbs_FACE), notes(*second).c_str());
    CHECK(closed_solid(second->occt()));
    CHECK(second->notes().empty());
    CHECK_NEAR_TOL(volume_of(second->occt()), before - radius * radius * kCorner * (kTip - kEnd), 1e-7);
    // F4's faces run from the end plane to the overhang's end.
    Bnd_Box box;
    int made = 0;
    for (int f = 0; f < second->face_count(); ++f) {
      for (const std::string& name : second->face_names(f)) {
        if (name.rfind("F4:fillet(", 0) == 0) {
          BRepBndLib::AddOptimal(second->face(f), box, false, false);
          ++made;
          break;
        }
      }
    }
    CHECK(made >= 1);
    if (made >= 1) {
      double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
      box.Get(x0, y0, z0, x1, y1, z1);
      CHECK(std::abs(x0 - kEnd) < 1e-5);
      CHECK(std::abs(x1 - kTip) < 1e-5);
    }
    // The end flat on the end plane: the corner the rounding takes away,
    // at x = 60, lies on a plane square to x.
    const TopoDS_Face end =
        face_through(second->occt(), gp_Pnt(kEnd, kLine + 0.2 * radius, kLine + 0.2 * radius));
    CHECK(!end.IsNull());
    if (!end.IsNull()) {
      const BRepAdaptor_Surface surface(end, false);
      CHECK(surface.GetType() == GeomAbs_Plane);
      if (surface.GetType() == GeomAbs_Plane) {
        CHECK(surface.Plane().Axis().Direction().IsParallel(gp::DX(), 1e-9));
      }
    }
    // F3's faces as they were.
    const Named kept = named_faces(*second, "F3:fillet(");
    CHECK(kept.names == first_faces.names);
    CHECK_NEAR_TOL(kept.area, first_faces.area, 1e-9);
  }
}

// A radius larger than the upper block is high above the ledge (30) and
// smaller than the overhang's underside is deep (40): built as one valid
// closed solid, or a failure that says why in words, never that the
// result is not a closed solid.
void test_too_large_in_words() {
  const ShapePtr first = first_rounding();
  if (!first) {
    return;
  }
  try {
    const ShapePtr second = fillet("F4", *first, {set_at(*first, kConvexMiddle, 35.0)});
    CHECK(second != nullptr);
    if (second) {
      std::printf("F4 35: built, volume %.9f, notes: %s\n", volume_of(second->occt()), notes(*second).c_str());
      CHECK(closed_solid(second->occt()));
      CHECK(volume_of(second->occt()) < volume_of(first->occt()));
    }
  } catch (const std::exception& failure) {
    const std::string shown = failure.what();
    std::printf("F4 35: %s\n", shown.c_str());
    CHECK(shown.find("not a closed solid") == std::string::npos);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_overhang_end_plane) {
  test::guarded("test_kernel_alone", test::test_kernel_alone);
  test::guarded("test_overhang_rounded_from_end_plane", test::test_overhang_rounded_from_end_plane);
  test::guarded("test_too_large_in_words", test::test_too_large_in_words);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("all checks passed\n");
  return 0;
}
