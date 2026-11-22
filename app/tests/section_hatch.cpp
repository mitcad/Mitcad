// SPDX-License-Identifier: MIT
// The hatch over Section Analysis' caps (mitcad#2146,
// view/SectionHatch.hpp): every vertex of its lines lies in the section
// plane within 0.000001 mm and inside a cap, for caps of 10 mm and of
// 1000 mm, in a plane along the axes and in a slanted one far from the
// origin, at 45 and at 135 degrees; an L-shaped cap's lines stop at its
// inside corner; the measure of the distance from the plane reads a hatch
// moved off it.
#include "UnitTests.hpp"
#include "view/SectionHatch.hpp"

#include <cmath>
#include <cstdio>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "section_hatch.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// What the ticket asks: in the plane within a millionth of a millimetre.
constexpr double kInPlane = 1.0e-6;

// The point at (u, v) of the plane's own coordinates.
gp_Pnt at(const gp_Ax3& plane, double u, double v) {
  return plane.Location().Translated(gp_Vec(plane.XDirection()) * u + gp_Vec(plane.YDirection()) * v);
}

// A cap: the polygon of (u, v) corners in the plane, as a face.
TopoDS_Face capOf(const gp_Ax3& plane, const std::vector<std::pair<double, double>>& corners) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& [u, v] : corners) {
    polygon.Add(at(plane, u, v));
  }
  polygon.Close();
  return BRepBuilderAPI_MakeFace(gp_Pln(plane), polygon.Wire(), true).Face();
}

// The caps of a cut of `size` mm: a square and, beside it, an L whose
// inside corner a line has to stop at.
TopoDS_Shape capsOf(const gp_Ax3& plane, double size) {
  const double s = size;
  TopoDS_Compound caps;
  BRep_Builder builder;
  builder.MakeCompound(caps);
  builder.Add(caps, capOf(plane, {{0, 0}, {s, 0}, {s, s}, {0, s}}));
  builder.Add(caps, capOf(plane, {{2 * s, 0}, {3 * s, 0}, {3 * s, 0.4 * s}, {2.4 * s, 0.4 * s}, {2.4 * s, s},
                                  {2 * s, s}}));
  return caps;
}

// Whether `point` lies on one of the caps' faces (inside or on the outline).
bool onACap(const TopoDS_Shape& caps, const gp_Pnt& point, double tolerance) {
  for (TopExp_Explorer faces(caps, TopAbs_FACE); faces.More(); faces.Next()) {
    BRepClass_FaceClassifier classifier(TopoDS::Face(faces.Current()), point, tolerance);
    if (classifier.State() != TopAbs_OUT) {
      return true;
    }
  }
  return false;
}

void hatchTests(const char* name, const gp_Ax3& plane, double size, bool mirrored) {
  const TopoDS_Shape caps = capsOf(plane, size);
  const TopoDS_Shape hatch = mitcad::sectionHatch(caps, plane, mirrored);
  CHECK(!hatch.IsNull());
  if (hatch.IsNull()) {
    std::fprintf(stderr, "  (%s, %g mm)\n", name, size);
    return;
  }
  const gp_Pln pln(plane);
  const gp_Vec x(plane.XDirection());
  const gp_Vec y(plane.YDirection());
  const gp_Vec along = mirrored ? (x - y) / std::sqrt(2.0) : (x + y) / std::sqrt(2.0);
  int lines = 0;
  int offPlane = 0;
  int outside = 0;
  int crooked = 0;
  for (TopExp_Explorer edges(hatch, TopAbs_EDGE); edges.More(); edges.Next()) {
    ++lines;
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(TopoDS::Edge(edges.Current()), first, last);
    const gp_Pnt a = BRep_Tool::Pnt(first);
    const gp_Pnt b = BRep_Tool::Pnt(last);
    for (const gp_Pnt& p : {a, b}) {
      if (pln.Distance(p) > kInPlane) {
        ++offPlane;
      }
    }
    // The caps' outline is straight: their mesh's boundary is the outline
    // itself, so the lines end on it (rounding aside), and none runs
    // across the L's notch between its arms.
    for (int k = 0; k <= 8; ++k) {
      const gp_Pnt p = a.Translated(gp_Vec(a, b) * (k / 8.0));
      if (!onACap(caps, p, size * 1e-9 + kInPlane)) {
        ++outside;
      }
    }
    const gp_Vec direction(a, b);
    if (direction.Magnitude() <= 0.0 || std::abs(std::abs(direction.Normalized().Dot(along)) - 1.0) > 1e-9) {
      ++crooked;
    }
  }
  CHECK(lines >= 8);
  CHECK(offPlane == 0);
  CHECK(outside == 0);
  CHECK(crooked == 0);
  CHECK(mitcad::sectionHatchPlaneDistance(hatch, plane) <= kInPlane);
  if (offPlane != 0 || outside != 0 || crooked != 0) {
    std::fprintf(stderr, "  (%s, %g mm, %s: %d lines, %d vertices off the plane, %d outside the caps, %d crooked)\n",
                 name, size, mirrored ? "135 deg" : "45 deg", lines, offPlane, outside, crooked);
  }
  // The measure reads a hatch moved off the plane: 1.75 mm along its
  // normal, as the old lift put a 350 mm cut's lines.
  gp_Trsf lift;
  lift.SetTranslation(gp_Vec(plane.Direction()) * 1.75);
  const TopoDS_Shape lifted = BRepBuilderAPI_Transform(hatch, lift, true).Shape();
  CHECK(std::abs(mitcad::sectionHatchPlaneDistance(lifted, plane) - 1.75) < 1e-9);
}

void planeTests(const char* name, const gp_Ax3& plane) {
  for (const double size : {10.0, 1000.0}) {
    hatchTests(name, plane, size, false);
    hatchTests(name, plane, size, true);
  }
}

} // namespace

int sectionHatchTests() {
  // Along the axes: the XY plane, and YZ moved 10 mm along X.
  planeTests("xy", gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)));
  planeTests("yz at 10", gp_Ax3(gp_Pnt(10, 0, 0), gp_Dir(1, 0, 0), gp_Dir(0, 1, 0)));
  // Slanted and far from the origin, where rounding shows first.
  const gp_Dir normal(1, 2, 3);
  const gp_Dir xDirection = gp_Dir(gp_Vec(normal).Crossed(gp_Vec(0, 0, 1)));
  planeTests("slanted", gp_Ax3(gp_Pnt(1234.5, -678.9, 4321.0), normal, xDirection));
  // No caps, no hatch; a null hatch is 0 off the plane.
  const gp_Ax3 xy;
  CHECK(mitcad::sectionHatch(TopoDS_Shape(), xy, false).IsNull());
  CHECK(mitcad::sectionHatchPlaneDistance(TopoDS_Shape(), xy) == 0.0);
  return failures;
}
MITCAD_UNIT_TEST(sectionHatchTests);
