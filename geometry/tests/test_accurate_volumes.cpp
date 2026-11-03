// SPDX-License-Identifier: MIT
// Volumes and areas as MitCad reports them (geometry::mass_properties and
// geometry::faces_area, the analysis library's integrals of mitcad#140),
// mitcad#1200: common bodies against their exact values, and bodies with
// loft and offset faces against a reference integration, each within 1e-6
// (relative). OCCT's default rule (BRepGProp's VolumeProperties without an
// accuracy) was found up to 0.3 % off on the lean-in shells of a
// polygon-to-circle loft (mitcad#1193); its values are printed beside, not
// checked.
//
// The reference: OCCT's adaptive Gauss-Kronrod integration for the curved
// faces, and for a plane face (p - at).N A / 3 with its area A from a
// dense polygon through its boundary curves in the parameter plane, each
// wire in order and the ends of consecutive curves joined. Not OCCT's
// integration of the whole body: the inner cap an offset makes at the
// circle's end is bounded by intersection curves (tolerance 8e-5 mm) whose
// ends lie up to 8.5e-5 mm apart in the parameter plane, and OCCT
// integrates along the curves alone, so each gap leaves out the strip
// between it and the face's first parameter: 1.2e-3 mm2 of the cap's 157.6
// on the shell of the loft to a circle of radius 8, 7.7e-3 mm3 (3e-6 of
// the shell). MitCad's integral joins the gaps (mitcad#325) and agrees
// with the polygon to 1e-9.
//
// A mesh's volume is checked too, within 1e-3: a mesh comes near the
// exact volume only as its deflection goes to zero, so it cannot confirm
// 1e-6, but it is a measure owing nothing to either integration and shows
// an error of OCCT's default rule's size.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Domain.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepGProp_VinertGK.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

// The measures MitCad reports for a body.
mitcad::geometry::MassProperties reported(const TopoDS_Shape& occt) {
  const mitcad::geometry::Shape shape(occt);
  return mitcad::geometry::mass_properties(shape);
}

double relative(double value, double reference) {
  return std::abs(value - reference) / std::max(std::abs(reference), 1e-300);
}

// The body's reported volume and area against exact values.
void check_exact(const char* name, const TopoDS_Shape& body, double volume, double area) {
  const mitcad::geometry::MassProperties measured = reported(body);
  GProp_GProps fixed;
  BRepGProp::VolumeProperties(body, fixed);
  std::printf("%s: volume %.9f (exact %.9f, %.2e off; OCCT's default rule %.2e off), area %.9f (exact %.9f)\n", name,
              measured.volume, volume, relative(measured.volume, volume), relative(fixed.Mass(), volume),
              measured.area, area);
  CHECK_NEAR_TOL(measured.volume, volume, 1e-6);
  CHECK_NEAR_TOL(measured.area, area, 1e-6);
}

// The vertical edge of a box at (x, y).
TopoDS_Edge vertical_edge(const TopoDS_Shape& box, double x, double y) {
  for (TopExp_Explorer it(box, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X() - x) < 1e-9 && std::abs(a.Y() - y) < 1e-9 && std::abs(b.X() - x) < 1e-9 &&
        std::abs(b.Y() - y) < 1e-9) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

void test_common_bodies() {
  // A box.
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, 20, 10)).Shape();
  check_exact("box 30 x 20 x 10", box, 6000.0, 2.0 * (600.0 + 300.0 + 200.0));

  // A cylinder.
  const double r = 7.0;
  const double h = 25.0;
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(3, -2, 1), gp_Dir(0, 0, 1)), r, h).Shape();
  check_exact("cylinder r 7, h 25", cylinder, kPi * r * r * h, 2.0 * kPi * r * r + 2.0 * kPi * r * h);

  // A sphere.
  const double s = 6.0;
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(gp_Pnt(1, 2, 3), s).Shape();
  check_exact("sphere r 6", sphere, 4.0 / 3.0 * kPi * s * s * s, 4.0 * kPi * s * s);

  // A box 40 x 30 x 20 with its vertical edge at (40, 30) rounded, radius 4:
  // the corner's square of r x r becomes a quarter disc along the edge's 20
  // mm, the two side faces each lose a strip r wide, the top and bottom
  // the square less the quarter disc, and the rounding is a quarter of a
  // cylinder.
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 30, 20)).Shape();
  const TopoDS_Edge edge = vertical_edge(block, 40.0, 30.0);
  CHECK(!edge.IsNull());
  if (edge.IsNull()) {
    return;
  }
  BRepFilletAPI_MakeFillet rounding(block);
  const double f = 4.0;
  const double length = 20.0;
  rounding.Add(f, edge);
  rounding.Build();
  CHECK(rounding.IsDone());
  if (!rounding.IsDone()) {
    return;
  }
  const double corner = (1.0 - kPi / 4.0) * f * f;
  check_exact("box 40 x 30 x 20 rounded along an edge, r 4", rounding.Shape(), 24000.0 - corner * length,
              2.0 * (1200.0 + 800.0 + 600.0) - 2.0 * f * length - 2.0 * corner + 0.5 * kPi * f * length);
}

// The area of faces that are no body (geometry::faces_area, a section's
// caps): the box's six faces, and each face once.
void test_faces_area() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(30, 20, 10)).Shape();
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer it(box, TopAbs_FACE); it.More(); it.Next()) {
    faces.push_back(TopoDS::Face(it.Current()));
  }
  CHECK(faces.size() == 6);
  CHECK_NEAR_TOL(mitcad::geometry::faces_area(faces), 2200.0, 1e-9);
  faces.push_back(faces.front());
  CHECK_NEAR_TOL(mitcad::geometry::faces_area(faces), 2200.0, 1e-9);
  // A cylinder's side, curved.
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), 5.0, 12.0).Shape();
  std::vector<TopoDS_Face> side;
  for (TopExp_Explorer it(cylinder, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() == GeomAbs_Cylinder) {
      side.push_back(TopoDS::Face(it.Current()));
    }
  }
  CHECK(side.size() == 1);
  CHECK_NEAR_TOL(mitcad::geometry::faces_area(side), 2.0 * kPi * 5.0 * 12.0, 1e-9);
}

// A 20 mm square at z = 0 to a circle of `radius` at z = 40 (ThruSections,
// as mitcad#1184's and mitcad#1193's tests build it). With a radius under
// the square's half diagonal (14.14) its walls lean in towards the circle
// at the corners.
TopoDS_Shape square_loft(double radius) {
  BRepBuilderAPI_MakePolygon square(gp_Pnt(-10, -10, 0), gp_Pnt(10, -10, 0), gp_Pnt(10, 10, 0),
                                    gp_Pnt(-10, 10, 0), true);
  const gp_Ax2 axes(gp_Pnt(0, 0, 40), gp_Dir(0, 0, 1), gp_Dir(std::cos(kPi / 4), std::sin(kPi / 4), 0));
  const TopoDS_Wire circle =
      BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(axes, radius)).Edge()).Wire();
  BRepOffsetAPI_ThruSections maker(true, false, 1.0e-6);
  maker.CheckCompatibility(true);
  maker.AddWire(square.Wire());
  maker.AddWire(circle);
  maker.Build();
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The plane face of `body` at z = `z`.
TopoDS_Face cap_at(const TopoDS_Shape& body, double z) {
  for (TopExp_Explorer it(body, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() == GeomAbs_Plane &&
        std::abs(std::abs(surface.Plane().Axis().Direction().Z()) - 1.0) < 1e-9 &&
        std::abs(surface.Plane().Location().Z() - z) < 1e-6) {
      return face;
    }
  }
  return TopoDS_Face();
}

// A plane face's volume integral about `at`: (p - at).N A / 3, A from a
// polygon of 4000 points per boundary curve in the parameter plane (a
// plane's parameters are lengths), each wire in its order, the ends of
// consecutive curves joined by straight segments.
double plane_face_volume(const TopoDS_Face& face, const gp_Pnt& at) {
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  double area = 0.0;
  for (TopExp_Explorer w(forward, TopAbs_WIRE); w.More(); w.Next()) {
    std::vector<gp_Pnt2d> polygon;
    for (BRepTools_WireExplorer e(TopoDS::Wire(w.Current()), forward); e.More(); e.Next()) {
      const TopoDS_Edge& edge = e.Current();
      double first = 0.0;
      double last = 0.0;
      const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, forward, first, last);
      CHECK(!curve.IsNull());
      if (curve.IsNull()) {
        continue;
      }
      constexpr int kPoints = 4000;
      for (int i = 0; i <= kPoints; ++i) {
        const double t = edge.Orientation() == TopAbs_REVERSED ? last - (last - first) * i / kPoints
                                                               : first + (last - first) * i / kPoints;
        polygon.push_back(curve->Value(t));
      }
    }
    for (std::size_t i = 0; i < polygon.size(); ++i) {
      const gp_Pnt2d& a = polygon[i];
      const gp_Pnt2d& b = polygon[(i + 1) % polygon.size()];
      area += 0.5 * (a.X() * b.Y() - b.X() * a.Y());
    }
  }
  const gp_Pln plane = BRepAdaptor_Surface(forward).Plane();
  const gp_Vec normal = gp_Vec(plane.Position().XDirection()).Crossed(gp_Vec(plane.Position().YDirection()));
  const double volume = gp_Vec(at, plane.Location()).Dot(normal) * area / 3.0;
  return face.Orientation() == TopAbs_REVERSED ? -volume : volume;
}

// The reference volume of a body (see the top) and the Gauss-Kronrod
// part's error estimate relative to it; -1 when that integration failed.
struct Reference {
  double volume = 0.0;
  double error = 0.0;
};

Reference reference_volume(const TopoDS_Shape& body) {
  const gp_Pnt at(0.0, 0.0, 20.0);
  Reference result;
  double error = 0.0;
  bool failed = false;
  for (TopExp_Explorer it(body, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (face.Orientation() != TopAbs_FORWARD && face.Orientation() != TopAbs_REVERSED) {
      continue;
    }
    if (BRepAdaptor_Surface(face).GetType() == GeomAbs_Plane) {
      result.volume += plane_face_volume(face, at);
      continue;
    }
    BRepGProp_Face surface(true);
    surface.Load(face);
    BRepGProp_VinertGK kronrod;
    kronrod.SetLocation(at);
    double own = 0.0;
    if (face.NbChildren() == 0) {
      own = kronrod.Perform(surface, 1.0e-9, false, false);
    } else {
      BRepGProp_Domain domain(face);
      own = kronrod.Perform(surface, domain, 1.0e-9, false, false);
    }
    failed = failed || own < 0.0;
    error += own;
    result.volume += kronrod.Mass();
  }
  result.error = failed ? -1.0 : error / std::max(std::abs(result.volume), 1e-300);
  return result;
}

// The body's reported volume against the reference integration (the
// Gauss-Kronrod part's error estimate under 1e-7) within 1e-6, and against
// a mesh's within 1e-3. Meshes the body (adds triangulations to its
// faces), so it is the last measure taken of it.
void check_against_integration(const char* name, const TopoDS_Shape& body) {
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const mitcad::geometry::MassProperties measured = reported(body);
  const Reference reference = reference_volume(body);
  GProp_GProps whole;
  BRepGProp::VolumePropertiesGK(body, whole, 1.0e-9, false, true);
  GProp_GProps fixed;
  BRepGProp::VolumeProperties(body, fixed);
  std::printf("%s: volume %.9f, reference %.9f (error estimate %.2e, %.2e off); OCCT's Gauss-Kronrod over the "
              "whole body %.2e off, its default rule %.2e off\n",
              name, measured.volume, reference.volume, reference.error, relative(measured.volume, reference.volume),
              relative(whole.Mass(), reference.volume), relative(fixed.Mass(), reference.volume));
  CHECK(measured.volume > 0.0);
  CHECK(reference.error >= 0.0 && reference.error <= 1e-7);
  CHECK_NEAR_TOL(measured.volume, reference.volume, 1e-6);

  const BRepMesh_IncrementalMesh mesh(body, 1.0e-3, false, 0.1, true);
  CHECK(mesh.IsDone());
  GProp_GProps meshed;
  BRepGProp::VolumeProperties(body, meshed, false, false, true);
  std::printf("%s: a mesh of deflection 1e-3 mm encloses %.9f, %.2e off\n", name, meshed.Mass(),
              relative(meshed.Mass(), measured.volume));
  CHECK_NEAR_TOL(meshed.Mass(), measured.volume, 1e-3);
}

// Shells `body` inwards by t with its cap at z = `open` removed, with OCCT
// alone (mitcad#1193: with only the square's end removed the lean-in lofts
// shell, both ends removed still fail).
TopoDS_Shape thick_solid(const TopoDS_Shape& body, double open, double t) {
  const TopoDS_Face cap = cap_at(body, open);
  CHECK(!cap.IsNull());
  if (cap.IsNull()) {
    return TopoDS_Shape();
  }
  NCollection_List<TopoDS_Shape> closing;
  closing.Append(cap);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, closing, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& failure) {
    std::printf("the shell: %s\n", failure.what());
  }
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The closed inward offset of `body` by t (mitcad#1193: valid for the
// lean-in lofts).
TopoDS_Shape inward_offset(const TopoDS_Shape& body, double t) {
  BRepOffsetAPI_MakeOffsetShape maker;
  try {
    OCC_CATCH_SIGNALS
    maker.PerformByJoin(body, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
  } catch (const Standard_Failure& failure) {
    std::printf("the offset: %s\n", failure.what());
  }
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

void test_lofts_and_their_shells() {
  for (const double radius : {8.0, 10.0}) {
    char name[96];
    // (Each body is meshed by its check, so the shells and offsets are
    // made of a loft of their own.)
    std::snprintf(name, sizeof name, "square to a circle of radius %.0f", radius);
    check_against_integration(name, square_loft(radius));
    std::snprintf(name, sizeof name, "square to a circle of radius %.0f, shelled 1 mm, square end open", radius);
    check_against_integration(name, thick_solid(square_loft(radius), 0.0, 1.0));
    std::snprintf(name, sizeof name, "square to a circle of radius %.0f, offset 1 mm inwards", radius);
    check_against_integration(name, inward_offset(square_loft(radius), 1.0));
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_common_bodies", test::test_common_bodies);
  test::guarded("test_faces_area", test::test_faces_area);
  test::guarded("test_lofts_and_their_shells", test::test_lofts_and_their_shells);
  if (test::failures == 0) {
    std::puts("test_accurate_volumes: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
