// SPDX-License-Identifier: MIT
// The box of a face whose surface has no value at points of its parameter
// rectangle outside the face (mitcad#2633).
//
// A reported body (a loft, rounded, shelled, with a modelled thread) could
// not be cut by Section Analysis: measuring the body's box failed with
// OCCT's "GeomAdaptor_Surface::EvalD0: evaluation failed", so the plane's
// cut and its measurements failed at every distance. OCCT's optimal box of
// a face samples its surface over the whole parameter rectangle its
// boundary spans, also beyond the face, and an offset surface (as a
// shell's inner faces have) has no value where the surface it is offset
// from has no normal.
//
// Here the face is the diamond of test_trimmed_box.cpp on the cap
// z = 4u(1-u) + 4v(1-v) (x = u, y = v): |u - 0.5| + |v - 0.5| <= 0.45, its
// top z = 2 at its centre inside it, its lowest z = 1.19 at its corners,
// its boundary's highest z = 1.595. The surface throws OCCT's
// Geom_UndefinedValue wherever |u - 0.5| + |v - 0.5| > 0.55: in the corners
// of the face's parameter rectangle [0.05, 0.95]^2, never inside the face.
// OCCT's optimal box of it fails (checked, so that the case keeps the
// failure the report met); trimmed_face_box, trimmed_box, bounding_box and
// face_bounding_boxes give the face's exact extent within 1e-6.
//
// The distinction Section Analysis makes from the box (clip_side; the
// bridge cuts no body whose box the plane passes): a plane at z = 1.8,
// which crosses the face only through its interior above every edge, cuts
// it; planes at z = 2.1 and z = 1.0 pass it, keeping or removing it whole
// by the side their normal points to.

#include <array>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepLib.hxx>
#include <Bnd_Box.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Geometry.hxx>
#include <Geom_Surface.hxx>
#include <Geom_UndefinedValue.hxx>
#include <NCollection_Array2.hxx>
#include <Standard_Failure.hxx>
#include <Standard_NotImplemented.hxx>
#include <Standard_Type.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/analysis/common.hpp"
#include "mitcad/analysis/section.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/trimmed_box.hpp"
#include "bundled_test.hpp"

namespace {

using mitcad::geometry::BoundingBox;
using mitcad::geometry::Shape;

int failures = 0;

void check(bool ok, const std::string& what) {
  if (!ok) {
    std::fprintf(stderr, "check failed: %s\n", what.c_str());
    ++failures;
  }
}

// xmin, ymin, zmin, xmax, ymax, zmax.
using Sides = std::array<double, 6>;

const char* const kSideNames[6] = {"xmin", "ymin", "zmin", "xmax", "ymax", "zmax"};

Sides sides_of(const Bnd_Box& box) {
  Sides s{};
  box.Get(s[0], s[1], s[2], s[3], s[4], s[5]);
  return s;
}

Sides sides_of(const BoundingBox& box) {
  return {box.min.X(), box.min.Y(), box.min.Z(), box.max.X(), box.max.Y(), box.max.Z()};
}

void print_sides(const char* name, const Sides& s) {
  std::printf("  %-28s x [%.6f, %.6f] y [%.6f, %.6f] z [%.6f, %.6f]\n", name, s[0], s[3], s[1], s[4], s[2], s[5]);
}

void check_near(const Sides& got, const Sides& want, double tolerance, const std::string& what) {
  for (std::size_t k = 0; k < 6; ++k) {
    check(std::abs(got[k] - want[k]) <= tolerance,
          what + ": " + kSideNames[k] + " " + std::to_string(got[k]) + ", not " + std::to_string(want[k]));
  }
}

// A surface that is `basis` within |u - 0.5| + |v - 0.5| <= `reach` and has
// no value beyond: its evaluation throws there, as OCCT's offset surface
// throws where the surface it is offset from has no normal. OCCT reads it
// as a surface of a kind it does not know, evaluated through these calls.
class UndefinedBeyond final : public Geom_Surface {
public:
  UndefinedBeyond(const occ::handle<Geom_Surface>& basis, double reach) : m_basis(basis), m_reach(reach) {}

  void UReverse() override { throw Standard_NotImplemented("the test surface is not reversed"); }
  double UReversedParameter(const double u) const override { return u; }
  void VReverse() override { throw Standard_NotImplemented("the test surface is not reversed"); }
  double VReversedParameter(const double v) const override { return v; }
  void Bounds(double& u1, double& u2, double& v1, double& v2) const override { m_basis->Bounds(u1, u2, v1, v2); }
  bool IsUClosed() const override { return false; }
  bool IsVClosed() const override { return false; }
  bool IsUPeriodic() const override { return false; }
  bool IsVPeriodic() const override { return false; }
  occ::handle<Geom_Curve> UIso(const double u) const override { return m_basis->UIso(u); }
  occ::handle<Geom_Curve> VIso(const double v) const override { return m_basis->VIso(v); }
  GeomAbs_Shape Continuity() const override { return m_basis->Continuity(); }
  bool IsCNu(const int n) const override { return m_basis->IsCNu(n); }
  bool IsCNv(const int n) const override { return m_basis->IsCNv(n); }

  gp_Pnt EvalD0(const double u, const double v) const override {
    defined_at(u, v);
    return m_basis->EvalD0(u, v);
  }
  ResD1 EvalD1(const double u, const double v) const override {
    defined_at(u, v);
    return m_basis->EvalD1(u, v);
  }
  ResD2 EvalD2(const double u, const double v) const override {
    defined_at(u, v);
    return m_basis->EvalD2(u, v);
  }
  ResD3 EvalD3(const double u, const double v) const override {
    defined_at(u, v);
    return m_basis->EvalD3(u, v);
  }
  gp_Vec EvalDN(const double u, const double v, const int nu, const int nv) const override {
    defined_at(u, v);
    return m_basis->EvalDN(u, v, nu, nv);
  }

  void Transform(const gp_Trsf& t) override { m_basis->Transform(t); }
  occ::handle<Geom_Geometry> Copy() const override {
    return new UndefinedBeyond(occ::down_cast<Geom_Surface>(m_basis->Copy()), m_reach);
  }

  DEFINE_STANDARD_RTTI_INLINE(UndefinedBeyond, Geom_Surface)

private:
  void defined_at(double u, double v) const {
    if (std::abs(u - 0.5) + std::abs(v - 0.5) > m_reach) {
      throw Geom_UndefinedValue("the test surface has no value here");
    }
  }

  occ::handle<Geom_Surface> m_basis;
  double m_reach;
};

// The degree-2 Bezier surface x = u, y = v, z = 4u(1-u) + 4v(1-v).
occ::handle<Geom_BezierSurface> cap() {
  const std::array<double, 3> a = {0.0, 2.0, 0.0};
  NCollection_Array2<gp_Pnt> poles(1, 3, 1, 3);
  for (int i = 0; i < 3; ++i) {
    for (int j = 0; j < 3; ++j) {
      poles.SetValue(i + 1, j + 1,
                     gp_Pnt(0.5 * i, 0.5 * j, a[static_cast<std::size_t>(i)] + a[static_cast<std::size_t>(j)]));
    }
  }
  return new Geom_BezierSurface(poles);
}

// The face of `s` within the polygon of parameter points `corners`
// (counter-clockwise), its edges' curves in space built from their
// parameter lines.
TopoDS_Face polygon_face(const occ::handle<Geom_Surface>& s, const std::vector<gp_Pnt2d>& corners) {
  BRepBuilderAPI_MakeWire wire;
  for (std::size_t k = 0; k < corners.size(); ++k) {
    const gp_Pnt2d& a = corners[k];
    const gp_Pnt2d& b = corners[(k + 1) % corners.size()];
    const occ::handle<Geom2d_Line> line = new Geom2d_Line(a, gp_Dir2d(b.XY() - a.XY()));
    wire.Add(BRepBuilderAPI_MakeEdge(line, s, 0.0, a.Distance(b)).Edge());
  }
  TopoDS_Face face = BRepBuilderAPI_MakeFace(s, wire.Wire(), true).Face();
  BRepLib::BuildCurves3d(face, 1.0e-7, GeomAbs_C1, 14, 0);
  return face;
}

mitcad::analysis::Bounds bounds_of(const BoundingBox& box) {
  mitcad::analysis::Bounds bounds;
  bounds.min = {box.min.X(), box.min.Y(), box.min.Z()};
  bounds.max = {box.max.X(), box.max.Y(), box.max.Z()};
  bounds.empty = box.empty;
  return bounds;
}

const char* side_name(mitcad::analysis::ClipSide side) {
  switch (side) {
  case mitcad::analysis::ClipSide::Kept:
    return "kept whole";
  case mitcad::analysis::ClipSide::Removed:
    return "removed whole";
  case mitcad::analysis::ClipSide::Cut:
    return "cut";
  }
  return "?";
}

void check_plane(const mitcad::analysis::Bounds& box, double z, double normal, mitcad::analysis::ClipSide want) {
  mitcad::analysis::Plane plane;
  plane.origin = {0.3, 0.6, z};
  plane.normal = {0.0, 0.0, normal};
  const mitcad::analysis::ClipSide got = mitcad::analysis::clip_side(box, plane);
  std::printf("  the plane at z = %.2f, normal %+.0f z: %s\n", z, normal, side_name(got));
  check(got == want, "the plane at z = " + std::to_string(z) + ": " + side_name(got) + ", not " + side_name(want));
}

void test_undefined_beyond_the_face() {
  const occ::handle<Geom_Surface> surface = new UndefinedBeyond(cap(), 0.55);
  const TopoDS_Face diamond =
      polygon_face(surface, {gp_Pnt2d(0.5, 0.05), gp_Pnt2d(0.95, 0.5), gp_Pnt2d(0.5, 0.95), gp_Pnt2d(0.05, 0.5)});
  const Sides want = {0.05, 0.05, 1.19, 0.95, 0.95, 2.0};
  std::printf("a diamond on a cap whose surface has no value beyond it\n");

  // The reported failure: OCCT's optimal box evaluates the surface beyond
  // the face.
  bool occt_failed = false;
  try {
    Bnd_Box occt;
    BRepBndLib::AddOptimal(diamond, occt, false, false);
    print_sides("OCCT's optimal box", sides_of(occt));
  } catch (const Standard_Failure& failure) {
    occt_failed = true;
    std::printf("  OCCT's optimal box fails: %s\n", failure.what());
  }
  check(occt_failed, "OCCT's optimal box of the face fails, as on the reported body");

  try {
    const Sides trimmed = sides_of(mitcad::geometry::trimmed_face_box(diamond));
    print_sides("the trimmed face's box", trimmed);
    print_sides("its extent", want);
    check_near(trimmed, want, 1e-6, "trimmed_face_box");
    check_near(sides_of(mitcad::geometry::trimmed_box(diamond)), want, 1e-6, "trimmed_box");
  } catch (const Standard_Failure& failure) {
    check(false, std::string("the trimmed boxes fail: ") + failure.what());
  }
  try {
    const Shape shape(diamond);
    const BoundingBox box = mitcad::geometry::bounding_box(shape);
    check(!box.empty, "bounding_box: not empty");
    check_near(sides_of(box), want, 1e-6, "bounding_box");
    const std::vector<BoundingBox> boxes = mitcad::geometry::face_bounding_boxes(shape, {0});
    check(boxes.size() == 1 && !boxes[0].empty, "face_bounding_boxes: one face box");
    if (boxes.size() == 1 && !boxes[0].empty) {
      check_near(sides_of(boxes[0]), want, 1e-6, "face_bounding_boxes");
    }

    // What Section Analysis tells from the box.
    const mitcad::analysis::Bounds bounds = bounds_of(box);
    check_plane(bounds, 1.8, 1.0, mitcad::analysis::ClipSide::Cut);
    check_plane(bounds, 1.8, -1.0, mitcad::analysis::ClipSide::Cut);
    check_plane(bounds, 2.1, 1.0, mitcad::analysis::ClipSide::Kept);
    check_plane(bounds, 2.1, -1.0, mitcad::analysis::ClipSide::Removed);
    check_plane(bounds, 1.0, 1.0, mitcad::analysis::ClipSide::Removed);
    check_plane(bounds, 1.0, -1.0, mitcad::analysis::ClipSide::Kept);
  } catch (const std::exception& error) {
    check(false, std::string("the body's box fails: ") + error.what());
  }
}

} // namespace

MITCAD_BUNDLED_TEST(trimmed_box_undefined_surface) {
  test_undefined_beyond_the_face();
  if (failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("ok\n");
  return 0;
}
