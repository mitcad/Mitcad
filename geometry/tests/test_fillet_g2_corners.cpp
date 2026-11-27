// SPDX-License-Identifier: MIT
// Curvature continuous (G2) fillet corners (mitcad#2332, GitHub
// mitcad/Mitcad#611): where G2 roundings meet at a vertex, every edge there
// rounded, the roundings stop at planes across their edges and a patch of
// one polynomial face per face at the vertex closes the corner.
//
// A 40 x 30 x 20 box, its faces named by where they lie: the three edges at
// its corner (40, 0, 20) rounded 4 mm G2 (the reported case) with Rolling
// Ball and with Setback corners, and all twelve edges; along each boundary
// of each corner patch the normals agree within 0.01 degree and the normal
// curvature across within 1 % (of the rounding's 1 / radius where both are
// about zero, beside a face). Two G2 sets and a tangent continuous (G1) set
// at the corner: the normals agree within 0.01 degree, the result says the
// corner is G1 there. A thin box (5 mm) builds with Rolling Ball corners
// (the setback shortened to its edge) and is refused with Setback corners.
// Two G2 edges at the corner with the upright edge sharp (and one G2, one
// G1): a mitre, the roundings meeting in one crease, tangent to their
// faces. Refused in words: an asymmetric set at the corner.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

constexpr double kWidth = 40.0;
constexpr double kDepth = 30.0;
constexpr double kHeight = 20.0;
constexpr double kRadius = 4.0;
// What the patch keeps to along its boundary: the angle between the
// normals (0.01 degree) and the normal curvature across (1 %).
constexpr double kNormalAngle = 0.01 * kPi / 180.0;
constexpr double kCurvatureShare = 0.01;

Bnd_Box bounds_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  return box;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The box from the origin, its faces named B:x0, B:x1, B:y0, B:y1, B:z0,
// B:z1 by the side they lie on.
ShapePtr box(double height = kHeight) {
  const TopoDS_Shape solid = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), kWidth, kDepth, height).Shape();
  std::vector<Shape::NamedFace> named;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    bounds_of(f.Current()).Get(x1, y1, z1, x2, y2, z2);
    const auto flat = [](double a, double b) { return std::abs(a - b) < 1e-3; };
    std::string name = "B:";
    if (flat(x1, x2)) {
      name += x1 < 1.0 ? "x0" : "x1";
    } else if (flat(y1, y2)) {
      name += y1 < 1.0 ? "y0" : "y1";
    } else {
      name += z1 < 1.0 ? "z0" : "z1";
    }
    named.push_back({f.Current(), {name}});
  }
  return std::make_shared<Shape>(solid, named);
}

// The name of the body's edge from a to b (either way round); empty for none.
std::string edge_named(const Shape& body, const gp_Pnt& a, const gp_Pnt& b) {
  for (int e = 0; e < body.edge_count(); ++e) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(body.edge(e), first, last);
    const gp_Pnt p = BRep_Tool::Pnt(first);
    const gp_Pnt q = BRep_Tool::Pnt(last);
    if ((p.Distance(a) < 1e-6 && q.Distance(b) < 1e-6) || (p.Distance(b) < 1e-6 && q.Distance(a) < 1e-6)) {
      return body.edge_name(e);
    }
  }
  return std::string();
}

// The name of the body's vertex at p; empty for none.
std::string vertex_named(const Shape& body, const gp_Pnt& p) {
  for (int v = 0; v < body.vertex_count(); ++v) {
    if (BRep_Tool::Pnt(body.vertex(v)).Distance(p) < 1e-6) {
      return body.vertex_name(v);
    }
  }
  return std::string();
}

// The three edges at the box's corner (40, 0, h): along its top front,
// its top right and up its front right.
std::vector<std::string> corner_edges(const Shape& body, double height = kHeight) {
  const gp_Pnt corner(kWidth, 0, height);
  return {edge_named(body, corner, gp_Pnt(0, 0, height)), edge_named(body, corner, gp_Pnt(kWidth, kDepth, height)),
          edge_named(body, corner, gp_Pnt(kWidth, 0, 0))};
}

FilletSet g2_set(const std::vector<std::string>& edges, double radius = kRadius) {
  FilletSet set;
  set.edges = edges;
  set.radius = radius;
  set.curvature = true;
  return set;
}

// One closed valid solid.
bool closed_valid_solid(const Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer s(shape.occt(), TopAbs_SOLID); s.More(); s.Next()) {
    ++solids;
  }
  Ancestors faces;
  TopExp::MapShapesAndAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) && faces(i).Extent() != 2) {
      std::fprintf(stderr, "an edge with %d faces\n", faces(i).Extent());
      return false;
    }
  }
  const bool valid = BRepCheck_Analyzer(shape.occt()).IsValid();
  if (solids != 1 || !valid) {
    std::fprintf(stderr, "%d solids, %s\n", solids, valid ? "valid" : "not valid");
  }
  return solids == 1 && valid;
}

bool is_corner_face(const Shape& shape, int face, const std::string& prefix) {
  if (face < 0) {
    return false;
  }
  for (const std::string& name : shape.face_names(face)) {
    if (name.rfind(prefix, 0) == 0) {
      return true;
    }
  }
  return false;
}

// The face's normal out of the material at (u, v), and its normal
// curvature along `direction` (positive where it bends towards the normal).
std::pair<gp_Dir, double> normal_and_curvature(const TopoDS_Face& face, const gp_Pnt2d& uv, const gp_Vec& direction) {
  const BRepAdaptor_Surface surface(face, false);
  BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 2, 1e-9); // (its queries are not const)
  const bool reversed = face.Orientation() == TopAbs_REVERSED;
  const gp_Dir normal = reversed ? props.Normal().Reversed() : props.Normal();
  const gp_Vec su = props.D1U();
  const gp_Vec sv = props.D1V();
  const double e = su.Dot(su);
  const double f = su.Dot(sv);
  const double g = sv.Dot(sv);
  const double tu = direction.Dot(su);
  const double tv = direction.Dot(sv);
  const double det = e * g - f * f;
  const double a = (g * tu - f * tv) / det;
  const double b = (e * tv - f * tu) / det;
  const gp_Vec n(props.Normal());
  const double l = props.D2U().Dot(n);
  const double m = props.DUV().Dot(n);
  const double nn = props.D2V().Dot(n);
  const double first = e * a * a + 2 * f * a * b + g * b * b;
  const double k = (l * a * a + 2 * m * a * b + nn * b * b) / first;
  return {normal, reversed ? -k : k};
}

// How far the corner faces (names starting with `prefix`) keep from their
// neighbours along their boundary: the largest angle between the normals,
// and the largest difference of the normal curvatures across, as a share
// of the larger of them or 1 / kRadius. Edges between two corner faces are
// the patch's own seams: their normals count in `seam_angle`.
struct Smoothness {
  double angle = 0.0;
  double curvature = 0.0;
  double seam_angle = 0.0;
  int boundary_edges = 0;
};

Smoothness smoothness(const Shape& shape, const std::string& prefix) {
  Smoothness found;
  Ancestors faces;
  TopExp::MapShapesAndAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    if (faces(i).Extent() != 2) {
      continue;
    }
    const TopoDS_Face a = TopoDS::Face(faces(i).First());
    const TopoDS_Face b = TopoDS::Face(faces(i).Last());
    const bool corner_a = is_corner_face(shape, shape.face_index(a), prefix);
    const bool corner_b = is_corner_face(shape, shape.face_index(b), prefix);
    if (!corner_a && !corner_b) {
      continue;
    }
    const bool seam = corner_a && corner_b;
    found.boundary_edges += seam ? 0 : 1;
    const BRepAdaptor_Curve curve(edge);
    for (int k = 1; k < 10; ++k) {
      const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 10.0;
      gp_Pnt point;
      gp_Vec tangent;
      curve.D1(t, point, tangent);
      const gp_Pnt2d on_a = BRepAdaptor_Curve2d(edge, a).Value(t);
      const gp_Pnt2d on_b = BRepAdaptor_Curve2d(edge, b).Value(t);
      const gp_Dir normal_a = normal_and_curvature(a, on_a, tangent).first;
      const gp_Vec across = gp_Vec(normal_a).Crossed(tangent).Normalized();
      const auto [na, ka] = normal_and_curvature(a, on_a, across);
      const auto [nb, kb] = normal_and_curvature(b, on_b, across);
      const double angle = na.Angle(nb);
      if (seam) {
        found.seam_angle = std::max(found.seam_angle, angle);
        continue;
      }
      found.angle = std::max(found.angle, angle);
      const double scale = std::max({std::abs(ka), std::abs(kb), 1.0 / kRadius});
      found.curvature = std::max(found.curvature, std::abs(ka - kb) / scale);
    }
  }
  return found;
}

int faces_with_prefix(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    count += is_corner_face(shape, f, prefix) ? 1 : 0;
  }
  return count;
}

// The reported corner: three edges 4 mm G2, Rolling Ball and Setback.
void test_box_corner() {
  const ShapePtr body = box();
  const std::string vertex = vertex_named(*body, gp_Pnt(kWidth, 0, kHeight));
  CHECK(!vertex.empty());
  const std::string prefix = "F5:corner(" + vertex + ")";
  for (const bool rolling : {true, false}) {
    const ShapePtr rounded = fillet("F5", *body, {g2_set(corner_edges(*body))}, rolling);
    CHECK(closed_valid_solid(*rounded));
    // The box's six faces, three roundings and the corner's three faces.
    CHECK(rounded->face_count() == 12);
    CHECK(faces_with_prefix(*rounded, prefix) == 3);
    const double removed = kWidth * kDepth * kHeight - volume_of(rounded->occt());
    CHECK(removed > 0.0 && removed < (kWidth + kDepth + kHeight) * kRadius * kRadius);
    const Smoothness smooth = smoothness(*rounded, prefix);
    if (smooth.angle > kNormalAngle || smooth.curvature > kCurvatureShare || smooth.seam_angle > kNormalAngle) {
      std::fprintf(stderr, "box corner (%s): normals %g rad, curvature %g, seams %g rad\n",
                   rolling ? "rolling ball" : "setback", smooth.angle, smooth.curvature, smooth.seam_angle);
    }
    CHECK(smooth.boundary_edges >= 9);
    CHECK(smooth.angle <= kNormalAngle);
    CHECK(smooth.curvature <= kCurvatureShare);
    CHECK(smooth.seam_angle <= kNormalAngle);
    CHECK(rounded->infos().empty());
  }
}

// All twelve edges: eight corners.
void test_all_edges() {
  const ShapePtr body = box();
  std::vector<std::string> edges;
  for (int e = 0; e < body->edge_count(); ++e) {
    edges.push_back(body->edge_name(e));
  }
  CHECK(edges.size() == 12);
  const ShapePtr rounded = fillet("F5", *body, {g2_set(edges)}, true);
  CHECK(closed_valid_solid(*rounded));
  CHECK(rounded->face_count() == 6 + 12 + 8 * 3);
  CHECK(faces_with_prefix(*rounded, "F5:corner(") == 8 * 3);
  const Smoothness smooth = smoothness(*rounded, "F5:corner(");
  if (smooth.angle > kNormalAngle || smooth.curvature > kCurvatureShare || smooth.seam_angle > kNormalAngle) {
    std::fprintf(stderr, "all edges: normals %g rad, curvature %g, seams %g rad\n", smooth.angle, smooth.curvature,
                 smooth.seam_angle);
  }
  CHECK(smooth.angle <= kNormalAngle);
  CHECK(smooth.curvature <= kCurvatureShare);
  CHECK(smooth.seam_angle <= kNormalAngle);
}

// Two G2 sets (4 mm and 3 mm) and a G1 one (4 mm) at the corner: tangent
// continuous all round, the information says where it is not G2.
void test_g2_meets_g1() {
  const ShapePtr body = box();
  const std::vector<std::string> edges = corner_edges(*body);
  FilletSet tangent;
  tangent.edges = {edges[2]};
  tangent.radius = kRadius;
  const ShapePtr rounded =
      fillet("F5", *body, {g2_set({edges[0]}), g2_set({edges[1]}, 3.0), tangent}, true);
  CHECK(closed_valid_solid(*rounded));
  const std::string prefix = "F5:corner(" + vertex_named(*body, gp_Pnt(kWidth, 0, kHeight)) + ")";
  CHECK(faces_with_prefix(*rounded, prefix) == 3);
  const Smoothness smooth = smoothness(*rounded, prefix);
  if (smooth.angle > kNormalAngle || smooth.seam_angle > kNormalAngle) {
    std::fprintf(stderr, "G2 meets G1: normals %g rad, seams %g rad\n", smooth.angle, smooth.seam_angle);
  }
  CHECK(smooth.angle <= kNormalAngle);
  CHECK(smooth.seam_angle <= kNormalAngle);
  bool said = false;
  for (const std::string& info : rounded->infos()) {
    said = said || info.find("tangent continuous (G1)") != std::string::npos;
  }
  CHECK(said);
}

// A 5 mm thin box: Rolling Ball shortens the setback to fit the 5 mm edge;
// Setback (1.5 x 4 mm) is longer than it.
void test_thin_box() {
  const ShapePtr body = box(5.0);
  const std::vector<FilletSet> sets{g2_set(corner_edges(*body, 5.0))};
  const ShapePtr rounded = fillet("F5", *body, sets, true);
  CHECK(closed_valid_solid(*rounded));
  CHECK(rounded->face_count() == 12);
  CHECK(throws_with([&] { fillet("F5", *body, sets, false); }, "where an edge is shorter than the setback"));
}

void test_refusals() {
  const ShapePtr body = box();
  const std::vector<std::string> edges = corner_edges(*body);
  // An asymmetric set at the corner.
  FilletSet uneven;
  uneven.edges = {edges[2]};
  uneven.size = FilletSize::Asymmetric;
  uneven.radius = 2;
  uneven.radius2 = 3;
  CHECK(throws_with([&] { fillet("F5", *body, {g2_set({edges[0], edges[1]}), uneven}); },
                    "unsupported: asymmetric fillets that meet other rounded edges at a corner"));
}

// The angle between the normals of the two faces of an edge at its middle.
double kink(const Shape& shape, int edge) {
  const std::array<int, 2> faces = shape.edge_faces(edge);
  const TopoDS_Edge& e = shape.edge(edge);
  const BRepAdaptor_Curve curve(e);
  const double t = (curve.FirstParameter() + curve.LastParameter()) / 2.0;
  const TopoDS_Face& a = shape.face(faces[0]);
  const TopoDS_Face& b = shape.face(faces[1]);
  const gp_Vec any(1.0, 0.0, 0.0);
  const gp_Dir na = normal_and_curvature(a, BRepAdaptor_Curve2d(e, a).Value(t), any).first;
  const gp_Dir nb = normal_and_curvature(b, BRepAdaptor_Curve2d(e, b).Value(t), any).first;
  return na.Angle(nb);
}

// Two G2 edges at the corner, the upright edge sharp: each rounding runs on
// to the face beyond and the two meet in a mitre, a crease that carries on
// the sharp edge (as the kernel's own G1 roundings meet there); each
// rounding tangent to its faces. Also with one of them G1.
void test_mitre() {
  const ShapePtr body = box();
  const std::vector<std::string> edges = corner_edges(*body);
  FilletSet tangent;
  tangent.edges = {edges[1]};
  tangent.radius = kRadius;
  for (const bool both : {true, false}) {
    const std::vector<FilletSet> sets =
        both ? std::vector<FilletSet>{g2_set({edges[0], edges[1]})} : std::vector<FilletSet>{g2_set({edges[0]}), tangent};
    const ShapePtr rounded = fillet("F5", *body, sets, true);
    CHECK(closed_valid_solid(*rounded));
    // The box's six faces and the two roundings.
    CHECK(rounded->face_count() == 8);
    const double removed = kWidth * kDepth * kHeight - volume_of(rounded->occt());
    CHECK(removed > 0.0 && removed < (kWidth + kDepth) * kRadius * kRadius);
    int creases = 0;
    double smooth = 0.0;
    for (int e = 0; e < rounded->edge_count(); ++e) {
      const std::array<int, 2> faces = rounded->edge_faces(e);
      if (faces[0] < 0 || faces[1] < 0) {
        continue;
      }
      const bool round_a = is_corner_face(*rounded, faces[0], "F5:fillet(");
      const bool round_b = is_corner_face(*rounded, faces[1], "F5:fillet(");
      if (round_a && round_b) {
        ++creases;
        CHECK(kink(*rounded, e) > 0.1);
      } else if (round_a || round_b) {
        // A contact line (tangent), or a rounding's far end on the box's
        // end face (square): only the contact lines count.
        const double angle = kink(*rounded, e);
        if (angle < 0.5) {
          smooth = std::max(smooth, angle);
        }
      }
    }
    if (creases != 1 || smooth > kNormalAngle) {
      std::fprintf(stderr, "mitre (%s): %d creases, contact lines within %g rad\n", both ? "G2" : "G2 and G1",
                   creases, smooth);
    }
    CHECK(creases == 1);
    CHECK(smooth <= kNormalAngle);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_g2_corners) {
  test::guarded("test_box_corner", test::test_box_corner);
  test::guarded("test_all_edges", test::test_all_edges);
  test::guarded("test_g2_meets_g1", test::test_g2_meets_g1);
  test::guarded("test_thin_box", test::test_thin_box);
  test::guarded("test_mitre", test::test_mitre);
  test::guarded("test_refusals", test::test_refusals);
  if (test::failures == 0) {
    std::puts("test_fillet_g2_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
