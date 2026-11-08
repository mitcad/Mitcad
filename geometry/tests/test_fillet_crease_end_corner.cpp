// SPDX-License-Identifier: MIT
// A rounding whose edge ends where a convex crease of its face begins
// (mitcad#1434), with OCCT's fillet alone.
//
// A cylinder (radius 30, height 30) is cut by a plane parallel to its axis,
// y = -d: the cut face U meets the cylinder along two lines L (along z) at a
// convex crease psi (cos psi = d / 30). A block below U is joined to it, so
// that U is a strip at the corners (1 wide along L, 0.1 along the end
// planes), as where the reported design's other tube meets the cut. The
// rims (the cylinder's edges with the end planes, arcs ending on the lines)
// are rounded with R = 1.5. At each of the four vertices V (an end plane,
// the cylinder and U) the rounding's contact on the cylinder reaches L at
// the section through V, at P, R from V, while its contact on the end
// plane lies inside it. OCCT ran the rim's torus on past V and cut it by
// U, a cut that leaves the strip: the result was invalid. The port's patch
// 1620 (third_party/vcpkg-ports) closes it as 1220's ball turning about a
// crease: the ball turns on about L at P while it touches the end plane (a
// horn torus, its major and minor radius R) until its centre lies in U's
// plane, by pi / 2 - psi; U's corner at V is cut along the ball's circle
// there. Without the block U holds the cut, and the torus cut by U stays
// (as the design's other tube's rims are in the file).
//
// Checks: valid; with the block, the body's faces and the rims' two tori
// and four horn tori; each horn torus about a line along z through a point
// R from a vertex, its area R^2 (pi / 2 - psi) (pi / 2 - 1); U's area less
// four corners of R^2 (1 - pi / 4). At psi of 30 and 45 degrees. Without
// the block: the two tori, no horn torus, U less more than that.
//
// With MITCAD_GEOMETRY_CASES set, the case `tube-end-crease-corners`:
// `body.brep`, the reported design's stored body before the rounding (a
// tube along x whose outside is cut by a plane y = const at a crease of 30
// degrees, x from -15 to 15) and `after.brep`, the stored body after it:
// the outside's rims on the end planes rounded with 1.5; the result has the
// stored body's faces, lies on it and it on the result within 0.01 mm, and
// the material taken is the stored body's.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <initializer_list>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Torus.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kRadius = 30.0; // the cylinder's
constexpr double kHeight = 30.0; // z from 0
constexpr double kR = 1.5;       // the rims' rounding

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++n;
  }
  return n;
}

constexpr double kStrip = 1.0; // the cut face's strip along each line L
constexpr double kFoot = 0.1;  // and along each end plane, with the block

// The cylinder cut by y = -d, the part y > -d kept (its seam on the part
// cut away); with `block`, a block below the cut face joined to it, which
// leaves of the cut face a strip kStrip wide along each line L and kFoot
// wide along each end plane.
TopoDS_Shape cut_cylinder(double d, bool block) {
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)), kRadius, kHeight).Shape();
  const TopoDS_Shape below = BRepPrimAPI_MakeBox(gp_Pnt(-40, -40, -5), gp_Pnt(40, -d, kHeight + 5)).Shape();
  const TopoDS_Shape cut = BRepAlgoAPI_Cut(cylinder, below).Shape();
  if (!block || cut.IsNull()) {
    return cut;
  }
  const double x = std::sqrt(kRadius * kRadius - d * d) - kStrip;
  const TopoDS_Shape foot = BRepPrimAPI_MakeBox(gp_Pnt(-x, -d - 5, kFoot), gp_Pnt(x, -d, kHeight - kFoot)).Shape();
  return BRepAlgoAPI_Fuse(cut, foot).Shape();
}

// Whether face f lies on the plane y = y0.
bool on_plane_y(const TopoDS_Face& f, double y0) {
  const BRepAdaptor_Surface s(f);
  return s.GetType() == GeomAbs_Plane && s.Plane().Axis().Direction().IsParallel(gp_Dir(0, 1, 0), 1e-9) &&
         s.Plane().Distance(gp_Pnt(0, y0, 0)) < 1e-6;
}

double area_on_plane_y(const TopoDS_Shape& shape, double y0) {
  double out = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if (on_plane_y(TopoDS::Face(f.Current()), y0)) {
      out += area_of(f.Current());
    }
  }
  return out;
}

// Whether face f is a plane whose normal is parallel to axis.
bool plane_along(const TopoDS_Face& f, const gp_Dir& axis) {
  const BRepAdaptor_Surface s(f);
  return s.GetType() == GeomAbs_Plane && s.Plane().Axis().Direction().IsParallel(axis, 1e-9);
}

// Whether face f is a cylinder of the radius about an axis parallel to axis.
bool cylinder_along(const TopoDS_Face& f, double radius, const gp_Dir& axis) {
  const BRepAdaptor_Surface s(f);
  return s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - radius) < 1e-6 &&
         s.Cylinder().Axis().Direction().IsParallel(axis, 1e-9);
}

// The edges between a plane square to axis and a cylinder of the radius
// about it rounded with r: null where the fillet fails.
TopoDS_Shape round_rims(const TopoDS_Shape& body, double radius, const gp_Dir& axis, double r) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edge_faces;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  BRepFilletAPI_MakeFillet fillet(body);
  int added = 0;
  for (int i = 1; i <= edge_faces.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& faces = edge_faces(i);
    if (faces.Extent() != 2) {
      continue;
    }
    const TopoDS_Face a = TopoDS::Face(faces.First());
    const TopoDS_Face b = TopoDS::Face(faces.Last());
    if ((plane_along(a, axis) && cylinder_along(b, radius, axis)) ||
        (plane_along(b, axis) && cylinder_along(a, radius, axis))) {
      fillet.Add(r, TopoDS::Edge(edge_faces.FindKey(i)));
      ++added;
    }
  }
  if (added == 0) {
    return TopoDS_Shape();
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
    return TopoDS_Shape();
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

struct HornTori {
  int count = 0;
  double worst_area = 0; // the largest relative difference from the expected area
  double worst_axis = 0; // the largest distance of an axis point from its place
};

// The horn tori (major and minor radius r) of a result: each about a line
// along axis through a point r from one of the vertices given.
HornTori horn_tori(const TopoDS_Shape& shape, double r, const gp_Dir& axis, double area,
                   const std::vector<gp_Pnt>& vertices) {
  HornTori out;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    const BRepAdaptor_Surface s(f);
    if (s.GetType() != GeomAbs_Torus || std::abs(s.Torus().MajorRadius() - r) > 1e-9 ||
        std::abs(s.Torus().MinorRadius() - r) > 1e-9) {
      continue;
    }
    ++out.count;
    out.worst_area = std::max(out.worst_area, std::abs(area_of(f) - area) / area);
    const gp_Ax1 a = s.Torus().Axis();
    double best = 1e100;
    for (const gp_Pnt& v : vertices) {
      // (The centre on the line through the vertex along axis, r from it.)
      for (const double side : {-1.0, 1.0}) {
        best = std::min(best, a.Location().Distance(v.Translated(side * r * gp_Vec(axis))));
      }
    }
    out.worst_axis = std::max(out.worst_axis, best + (a.Direction().IsParallel(axis, 1e-9) ? 0. : 1.));
  }
  return out;
}

// The cut cylinder's rims rounded at a crease psi, with the block (the cut
// face a strip at the corners) or without.
void test_cut_cylinder_rims(double psi_degrees, bool block) {
  const double psi = psi_degrees * kPi / 180.;
  const double d = kRadius * std::cos(psi);
  const TopoDS_Shape body = cut_cylinder(d, block);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const double cut_area = area_on_plane_y(body, -d);
  const TopoDS_Shape result = round_rims(body, kRadius, gp_Dir(0, 0, 1), kR);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    std::printf("psi %.0f, block %d: the rims' rounding failed\n", psi_degrees, block ? 1 : 0);
    return;
  }
  const bool valid = BRepCheck_Analyzer(result).IsValid();
  CHECK(valid);
  const double x0 = kRadius * std::sin(psi);
  const std::vector<gp_Pnt> vertices = {gp_Pnt(-x0, -d, 0), gp_Pnt(x0, -d, 0), gp_Pnt(-x0, -d, kHeight),
                                        gp_Pnt(x0, -d, kHeight)};
  const double horn_area = kR * kR * (0.5 * kPi - psi) * (0.5 * kPi - 1.);
  const HornTori h = horn_tori(result, kR, gp_Dir(0, 0, 1), horn_area, vertices);
  const double cut_less = cut_area - area_on_plane_y(result, -d);
  const double corners = 4 * kR * kR * (1. - 0.25 * kPi);
  std::printf("psi %.0f, block %d: valid %d, %d faces (%d before), %d horn tori (area off by %.2g, axes off by "
              "%.2g), the cut face less %.6f mm2 (four corners %.6f)\n",
              psi_degrees, block ? 1 : 0, valid ? 1 : 0, face_count(result), face_count(body), h.count,
              h.worst_area, h.worst_axis, cut_less, corners);
  if (block) {
    // Two tori along the rims and four horn tori.
    CHECK(face_count(result) == face_count(body) + 6);
    CHECK(h.count == 4);
    CHECK(h.worst_area <= 1e-6);
    CHECK(h.worst_axis <= 1e-6);
    CHECK_NEAR(cut_less, corners);
  } else {
    // The tori cut by the cut face, as before the patch.
    CHECK(face_count(result) == face_count(body) + 2);
    CHECK(h.count == 0);
    CHECK(cut_less > corners + 0.1);
  }
}

void test_cut_cylinder_rims_at_30_degrees() {
  test_cut_cylinder_rims(30., true);
}

void test_cut_cylinder_rims_at_45_degrees() {
  test_cut_cylinder_rims(45., true);
}

void test_cut_cylinder_rims_without_the_block() {
  test_cut_cylinder_rims(30., false);
  test_cut_cylinder_rims(45., false);
}

std::string geometry_cases() {
#ifdef _WIN32
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

TopoDS_Shape read(const std::string& path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  BRepTools::Read(shape, path.c_str(), builder);
  return shape;
}

// The farthest of points inside the faces of `from` from the faces of `to`.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to) {
  double worst = 0.;
  for (TopExp_Explorer f(from, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        BRepExtrema_DistShapeShape dist(BRepBuilderAPI_MakeVertex(s.Value(u, v)).Vertex(), to);
        if (dist.IsDone()) {
          worst = std::max(worst, dist.Value());
        }
      }
    }
  }
  return worst;
}

// The reported design's stored body: two tubes; the one along x (outside
// 30, x from -15 to 15) has its outside cut by a plane y = const where it
// meets the other, at a crease of 30 degrees.
void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string dir = cases + "/tube-end-crease-corners/";
  if (!std::ifstream(dir + "body.brep").good()) {
    std::printf("test_reported_body: skipped (no case tube-end-crease-corners in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  const TopoDS_Shape result = round_rims(shape, kRadius, gp_Dir(1, 0, 0), kR);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(face_count(result) == face_count(after));
  const double out = farthest(result, after);
  const double back = farthest(after, result);
  std::printf("%d faces (the stored body %d); the result on the stored body within %.6f mm, the stored body on "
              "it within %.6f mm\n",
              face_count(result), face_count(after), out, back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  const double taken = volume_of(shape) - volume_of(result);
  const double file = volume_of(shape) - volume_of(after);
  std::printf("the material taken %.6f mm3, the stored body's %.6f\n", taken, file);
  CHECK(std::abs(taken - file) <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_cut_cylinder_rims_at_30_degrees", test::test_cut_cylinder_rims_at_30_degrees);
  test::guarded("test_cut_cylinder_rims_at_45_degrees", test::test_cut_cylinder_rims_at_45_degrees);
  test::guarded("test_cut_cylinder_rims_without_the_block", test::test_cut_cylinder_rims_without_the_block);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_crease_end_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
