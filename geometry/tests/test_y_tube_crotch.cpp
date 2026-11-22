// SPDX-License-Identifier: MIT
// A Y tube's crotch rounded and the tube shelled (mitcad#2140, GitHub
// mitcad/Mitcad#517), built as the reporter's design builds it, without
// the design: a circle of diameter 20 (Sketch2's d4) swept along a line of
// 25 (Sketch1's d1) up the z axis and a tangent quarter arc of radius 30
// (d2, its end 30 from the line, d3) turning towards -x, mirrored about the
// plane x = 0 and joined. The arms (two tori) meet along a concave crease in
// that plane, two edges from (0, -10, 25) over (0, 0, 51.46) to (0, 10,
// 25), whose angle fades to nothing at both ends, where the arms leave the
// cylinder below tangentially.
//
// - The crotch rounded with 6 (the design's Fillet1) and with 50 (the
//   reporter's wish): one valid solid without a note or a smaller radius,
//   the rounding ending where the crease fades (its faces reach both ends)
//   and passing the crotch's middle at the ball's lowest point, z = 25 +
//   sqrt((40 + r)^2 - 30^2) - r.
// - The tube shelled 2 mm inwards with its three ends open, Sharp, without
//   a rounding and after each: one valid solid, the wall 2 mm thick by
//   points in the trunk, both arms and under the crotch, the ends open;
//   without a rounding the volume that of the tube less the same tube of
//   radius 8 (port patch 2140).
// - A shell that the geometry kernel and the cavity both refuse
//   (MITCAD_TEST_OCCT_REFUSES=shell) fails in words, the crotch's edges its
//   places, the kernel's text in the detail only.
//
// For the diagnosis, instead of checking (the traces on the standard
// error): `test_y_tube_crotch --probe` prints the offset's images of the
// tube's faces and edges (BRepOffset_MakeOffset's, with the crease
// intersection traced, MITCAD_TRACE_OFFSET_CREASE, port patch 2140), also
// after the roundings of 6 and 50 split at their creases as the shell
// splits them; `--probe-third` the kernel's shell of the tube and the faulty
// faces of an invalid one, the kernel's fillets at 8 and 50 with their walks
// traced (MITCAD_TRACE_FILLET_WALK, port patch 2141), and MitCad's fillets
// at 6, 8, 20 and 50 with the shells after 6 and 50; `--probe-second` the
// offset's analysis of the tube's edges and the walks at 7 and 8;
// `--probe-first` what each radius and each shell gives.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <functional>
#include <stdexcept>
#include <string>
#include <typeinfo>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgo_Image.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_Analyse.hxx>
#include <BRepOffset_Interval.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <TopoDS_Iterator.hxx>
#include <NCollection_List.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/transform.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kTube = 10.0; // Sketch2's diameter 20
constexpr double kLine = 25.0; // Sketch1's line
constexpr double kBend = 30.0; // Sketch1's arc
constexpr double kWall = 2.0;  // the reporter's Shell, Inside

const char* const kOpenEnds[] = {"F3:start(r{c2})", "F3:end(r{c2})", "F4:inst1(F3:end(r{c2}))"};

ModelCurve line3(const gp_Pnt& a, const gp_Pnt& b) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Line;
  curve.start = a;
  curve.end = b;
  return curve;
}

// The quarter arc about (-30, 0, 25) from (0, 0, 25) up to (-30, 0, 55):
// center + 30 (cos t x + sin t (normal x x)), normal x x = +z.
ModelCurve quarter_arc() {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Conic;
  curve.center = gp_Pnt(-kBend, 0, kLine);
  curve.normal = gp_Dir(0, -1, 0);
  curve.x_axis = gp_Dir(1, 0, 0);
  curve.major = curve.minor = kBend;
  curve.first = 0.0;
  curve.last = kPi / 2;
  return curve;
}

// The design's Sweep1 (F3) of a circle of radius r and its Mirror1 (F4)
// joined to it. The circle starts at +x, so that each arm's seam runs along
// its outer side and both pass the crotch's highest point; `seam` turns
// where it starts (the probe's half turn puts the seams on the arms' inner
// sides, away from the crotch).
ShapePtr y_tube(double r, double seam = 0.0) {
  SweepSpec spec;
  spec.feature = "F3";
  Region circle = circle_region(0.0, 0.0, r);
  circle.loops[0].segments[0].rotation = seam;
  circle.loops[0].segments[0].name = "c2";
  circle.name = "r{c2}";
  spec.regions = {circle};
  spec.path = {{"c2", line3(gp_Pnt(0, 0, 0), gp_Pnt(0, 0, kLine))}, {"c4", quarter_arc()}};
  const ShapePtr arm = sweep(spec);
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  const ShapePtr image = transform_shape(*arm, mirror, "F4:inst1");
  const BooleanResult joined = boolean(BooleanOp::Join, {arm.get()}, *image);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.empty() ? nullptr : joined.pieces.front().shape;
}

int solids(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

TopAbs_State state(const Shape& shape, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(shape.occt(), p, 1e-7);
  return classifier.State();
}

// The crotch's edges: in the plane x = 0 above the cylinder.
std::vector<int> crotch_edges(const Shape& body) {
  std::vector<int> edges;
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(middle.X()) < 1e-6 && middle.Z() > kLine + 1.0) {
      edges.push_back(e);
    }
  }
  return edges;
}

// The rounding of the crotch with r, as the design's Fillet1 makes it: one
// of its edges, the other by the tangent chain.
ShapePtr rounded(const Shape& body, double r) {
  const std::vector<int> crotch = crotch_edges(body);
  if (crotch.empty()) {
    throw std::runtime_error("the tube has no crotch edges");
  }
  FilletSet set;
  set.edges = {body.edge_name(crotch.front())};
  set.radius = r;
  return fillet("F5", body, {set});
}

// Where the rounding with r passes the crotch's middle: the lowest point of
// a ball 10 + r from both arms' paths in the plane x = 0.
double rounding_low_point(double r) { return kLine + std::sqrt((40.0 + r) * (40.0 + r) - kBend * kBend) - r; }

// The shell as the reporter's: its three ends open, 2 mm inside, Sharp,
// Tangent Chain on.
ShapePtr shelled(const Shape& body) {
  ShellSpec spec;
  for (const char* end : kOpenEnds) {
    spec.faces.emplace_back(end);
  }
  spec.inside = kWall;
  spec.tangent_chain = true;
  spec.rounded = false;
  return shell("F6", body, spec);
}

// The wall between 8 and 10 from the path at the point, across `d` (square
// to the path), on both sides: in the wall at 9, in the cavity at 7.5.
void check_wall_across(const Shape& wall, const gp_Pnt& on_path, const gp_Dir& d, const char* where) {
  for (const double side : {1.0, -1.0}) {
    const gp_Vec v = gp_Vec(d) * side;
    const TopAbs_State in_wall = state(wall, on_path.Translated(v * (kTube - 0.5 * kWall)));
    const TopAbs_State in_cavity = state(wall, on_path.Translated(v * (kTube - kWall - 0.5)));
    if (in_wall != TopAbs_IN || in_cavity != TopAbs_OUT) {
      std::fprintf(stderr, "%s, side %g: the wall's point %d, the cavity's point %d (0 in, 1 out)\n", where, side,
                   static_cast<int>(in_wall), static_cast<int>(in_cavity));
    }
    CHECK(in_wall == TopAbs_IN);
    CHECK(in_cavity == TopAbs_OUT);
  }
}

// The checks of a shell of the tube: the wall in the trunk and the arms,
// under the crotch's middle between `outer` and `outer - 2` where the wall
// is square to the z axis there (a rounding), else between `outer` and
// `inner`; the ends open.
// The largest tolerance of the shape's edges and vertices: a shell whose
// edges' curves on their faces collapsed is valid but tens of millimetres
// in tolerance, every point classified on it (mitcad#2140, port patch 2143).
double max_tolerance(const TopoDS_Shape& shape) {
  double tolerance = 0.0;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
  }
  return tolerance;
}

// (Within the geometric tolerance, 0.01 mm, mitcad#224: the offsets of the
// rounding's B-spline faces come out a few micrometres in tolerance.)
constexpr double kShellTolerance = 0.01;

void check_shell(const Shape& wall, double outer, double inner) {
  if (max_tolerance(wall.occt()) > kShellTolerance) {
    std::fprintf(stderr, "shell's largest tolerance %g\n", max_tolerance(wall.occt()));
  }
  CHECK(max_tolerance(wall.occt()) <= kShellTolerance);
  CHECK(valid(wall));
  CHECK(solids(wall.occt()) == 1);
  CHECK(wall.notes().empty());
  for (const std::string& note : wall.notes()) {
    std::fprintf(stderr, "note: %s\n", note.c_str());
  }
  check_wall_across(wall, gp_Pnt(0, 0, 0.5 * kLine), gp::DX(), "trunk across x");
  check_wall_across(wall, gp_Pnt(0, 0, 0.5 * kLine), gp::DY(), "trunk across y");
  for (const double sx : {-1.0, 1.0}) {
    // 80 degrees round the bend: past the rounding of 50, whose contacts
    // reach 70.5 degrees round it (its ball's centre 60 from both paths),
    // the arm's own wall on its outer side too.
    const double a = 80.0 * kPi / 180.0;
    const gp_Pnt on_path(sx * (kBend - kBend * std::cos(a)), 0, kLine + kBend * std::sin(a));
    check_wall_across(wall, on_path, gp_Dir(-sx * std::cos(a), 0, std::sin(a)), "arm across its bend");
    check_wall_across(wall, on_path, gp::DY(), "arm across y");
  }
  CHECK(state(wall, gp_Pnt(0, 0, outer + 0.05)) == TopAbs_OUT);
  CHECK(state(wall, gp_Pnt(0, 0, 0.5 * (outer + inner))) == TopAbs_IN);
  CHECK(state(wall, gp_Pnt(0, 0, inner - 0.05)) == TopAbs_OUT);
  CHECK(state(wall, gp_Pnt(0, 0, 0.5)) == TopAbs_OUT);
  CHECK(state(wall, gp_Pnt(-kBend + 0.5, 0, kLine + kBend)) == TopAbs_OUT);
  CHECK(state(wall, gp_Pnt(kBend - 0.5, 0, kLine + kBend)) == TopAbs_OUT);
}

// The faces of the result named after the rounding (F5:fillet(...)).
std::vector<int> rounding_faces(const Shape& shape) {
  std::vector<int> faces;
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& name : shape.face_names(f)) {
      if (name.rfind("F5:fillet(", 0) == 0) {
        faces.push_back(f);
        break;
      }
    }
  }
  return faces;
}

// Whether a vertex of the faces lies within 1e-3 of the point.
bool faces_reach(const Shape& shape, const std::vector<int>& faces, const gp_Pnt& p) {
  for (const int f : faces) {
    for (TopExp_Explorer it(shape.face(f), TopAbs_VERTEX); it.More(); it.Next()) {
      if (BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).Distance(p) < 1e-3) {
        return true;
      }
    }
  }
  return false;
}

void test_body() {
  const ShapePtr body = y_tube(kTube);
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  CHECK(valid(*body));
  CHECK(solids(body->occt()) == 1);
  CHECK(crotch_edges(*body).size() == 2);
  for (const char* end : kOpenEnds) {
    CHECK(body->find_faces(end).size() == 1);
  }
}

void test_crotch_rounded(double r) {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    return;
  }
  const double before = volume(*body);
  ShapePtr filleted;
  try {
    filleted = rounded(*body, r);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fillet %g: %s\n", r, error.what());
  }
  CHECK(filleted != nullptr);
  if (!filleted) {
    return;
  }
  CHECK(valid(*filleted));
  CHECK(solids(filleted->occt()) == 1);
  CHECK(filleted->notes().empty());
  CHECK(filleted->infos().empty()); // not made 0.1 % smaller
  CHECK(volume(*filleted) > before);
  CHECK_NEAR(volume(*body), before); // the input is left as it was
  const std::vector<int> faces = rounding_faces(*filleted);
  CHECK(!faces.empty());
  // It ends where the crease fades, at both ends.
  CHECK(faces_reach(*filleted, faces, gp_Pnt(0, -kTube, kLine)));
  CHECK(faces_reach(*filleted, faces, gp_Pnt(0, kTube, kLine)));
  // The ball's lowest point at the crotch's middle.
  const double low = rounding_low_point(r);
  CHECK(state(*filleted, gp_Pnt(0, 0, low - 0.02)) == TopAbs_IN);
  CHECK(state(*filleted, gp_Pnt(0, 0, low + 0.02)) == TopAbs_OUT);
}

void test_crotch_rounded_6() { test_crotch_rounded(6.0); }

void test_crotch_rounded_50() { test_crotch_rounded(50.0); }

void test_shell_without_rounding() {
  const ShapePtr body = y_tube(kTube);
  const ShapePtr cavity = y_tube(kTube - kWall);
  if (!body || !cavity) {
    return;
  }
  const double before = volume(*body);
  ShapePtr wall;
  try {
    wall = shelled(*body);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "shell: %s\n", error.what());
  }
  CHECK(wall != nullptr);
  if (!wall) {
    return;
  }
  // The crotch's middle: where the arms meet (40 from both arcs' centres)
  // and where their insides do (38).
  check_shell(*wall, kLine + std::sqrt(40.0 * 40.0 - kBend * kBend), kLine + std::sqrt(38.0 * 38.0 - kBend * kBend));
  CHECK_NEAR_TOL(volume(*wall), before - volume(*cavity), 1e-5);
  CHECK_NEAR(volume(*body), before);
}

void test_shell_after_rounding(double r) {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    return;
  }
  ShapePtr filleted;
  try {
    filleted = rounded(*body, r);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fillet %g: %s\n", r, error.what());
  }
  CHECK(filleted != nullptr);
  if (!filleted) {
    return;
  }
  const double before = volume(*filleted);
  ShapePtr wall;
  try {
    wall = shelled(*filleted);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "shell after the fillet of %g: %s\n", r, error.what());
  }
  CHECK(wall != nullptr);
  if (!wall) {
    return;
  }
  // Under the crotch's middle the rounding is square to the z axis: the
  // inside of the wall 2 below it.
  const double low = rounding_low_point(r);
  check_shell(*wall, low, low - kWall);
  CHECK(volume(*wall) < before);
  CHECK_NEAR(volume(*filleted), before);
}

void test_shell_after_rounding_6() { test_shell_after_rounding(6.0); }

void test_shell_after_rounding_50() { test_shell_after_rounding(50.0); }

void set_environment(const char* name, const char* value) {
#ifdef _WIN32
  _putenv_s(name, value);
#else
  if (value[0] == '\0') {
    unsetenv(name);
  } else {
    setenv(name, value, 1);
  }
#endif
}

void test_refused_shell_says_why() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    return;
  }
  const std::vector<int> crotch = crotch_edges(*body);
  set_environment("MITCAD_TEST_OCCT_REFUSES", "shell");
  std::string text;
  try {
    shelled(*body);
  } catch (const std::exception& error) {
    text = error.what();
  }
  std::string closed;
  try {
    ShellSpec spec;
    spec.inside = kWall;
    shell("F6", *body, spec);
  } catch (const std::exception& error) {
    closed = error.what();
  }
  set_environment("MITCAD_TEST_OCCT_REFUSES", "");
  for (const std::string& said : {text, closed}) {
    const std::string message = error_message(said);
    if (message.find("fades out to a smooth join") == std::string::npos) {
      std::fprintf(stderr, "refused shell: %s\n", said.c_str());
    }
    CHECK(message.find("fades out to a smooth join") != std::string::npos);
    CHECK(message.find("raised an error") == std::string::npos);
    CHECK(message.find("E{") == std::string::npos);
    // The kernel's text and the edges' names are for agents, in the detail
    // and the places.
    CHECK(said.find("raised an error") != std::string::npos);
    for (const int e : crotch) {
      CHECK(said.find(json_text(body->edge_name(e))) != std::string::npos);
    }
  }
}

// --probe: what each radius and each shell gives, with the kernel's report.

void probe_kernel_fillet(const Shape& body, double r) {
  const std::vector<int> crotch = crotch_edges(body);
  if (crotch.empty()) {
    return;
  }
  BRepFilletAPI_MakeFillet maker(body.occt());
  maker.Add(r, body.edge(crotch.front()));
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("  kernel fillet %g raised %s: '%s'\n", r, typeid(failure).name(), failure.what());
  }
  std::printf("  kernel fillet %g: done %d, failure %d, failed contour %d, contours %d, faulty contours %d, faulty "
              "vertices %d\n",
              r, int(maker.IsDone()), int(maker.Failure()), maker.FailedContour(), maker.NbContours(),
              maker.NbFaultyContours(), maker.NbFaultyVertices());
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    std::printf("   contour %d: %d edges, status %d, %d computed surfaces\n", ic, maker.NbEdges(ic),
                int(maker.StripeStatus(ic)), maker.NbComputedSurfaces(ic));
    for (int k = 1; k <= maker.NbEdges(ic); ++k) {
      TopoDS_Vertex a;
      TopoDS_Vertex b;
      TopExp::Vertices(maker.Edge(ic, k), a, b);
      const gp_Pnt pa = BRep_Tool::Pnt(a);
      const gp_Pnt pb = BRep_Tool::Pnt(b);
      std::printf("    edge %d from (%.4f %.4f %.4f) to (%.4f %.4f %.4f)\n", k, pa.X(), pa.Y(), pa.Z(), pb.X(),
                  pb.Y(), pb.Z());
    }
    for (int is = 1; is <= maker.NbComputedSurfaces(ic); ++is) {
      const occ::handle<Geom_Surface> surface = maker.ComputedSurface(ic, is);
      if (surface.IsNull()) {
        continue;
      }
      double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
      surface->Bounds(u1, u2, v1, v2);
      std::printf("    surface %d (%s) u %.6g..%.6g v %.6g..%.6g, corners:", is, surface->DynamicType()->Name(), u1,
                  u2, v1, v2);
      for (const double u : {u1, u2}) {
        for (const double v : {v1, v2}) {
          if (std::isfinite(u) && std::isfinite(v) && std::abs(u) < 1e50 && std::abs(v) < 1e50) {
            const gp_Pnt p = surface->Value(u, v);
            std::printf(" (%.4f %.4f %.4f)", p.X(), p.Y(), p.Z());
          }
        }
      }
      std::printf("\n");
    }
  }
  for (int iv = 1; iv <= maker.NbFaultyVertices(); ++iv) {
    const gp_Pnt p = BRep_Tool::Pnt(maker.FaultyVertex(iv));
    std::printf("   faulty vertex (%.4f %.4f %.4f)\n", p.X(), p.Y(), p.Z());
  }
}

void probe_kernel_shell(const Shape& body, const char* what) {
  NCollection_List<TopoDS_Shape> open;
  for (const char* end : kOpenEnds) {
    for (const int f : body.find_faces(end)) {
      open.Append(body.face(f));
    }
  }
  for (const GeomAbs_JoinType join : {GeomAbs_Intersection, GeomAbs_Arc}) {
    BRepOffsetAPI_MakeThickSolid maker;
    try {
      OCC_CATCH_SIGNALS
      maker.MakeThickSolidByJoin(body.occt(), open, -kWall, 1e-6, BRepOffset_Skin, false, false, join, false);
      std::printf("  kernel shell of %s (%s): done %d, error %d", what,
                  join == GeomAbs_Arc ? "arc" : "intersection", int(maker.IsDone()),
                  int(maker.MakeOffset().Error()));
      if (maker.IsDone()) {
        std::printf(", valid %d, solids %d", int(BRepCheck_Analyzer(maker.Shape()).IsValid()),
                    solids(maker.Shape()));
      }
      std::printf("\n");
    } catch (const Standard_Failure& failure) {
      std::printf("  kernel shell of %s (%s) raised %s: '%s'\n", what, join == GeomAbs_Arc ? "arc" : "intersection",
                  typeid(failure).name(), failure.what());
    }
  }
}

// How near the rounding's faces come to having no normal: the smallest
// |du x dv| / (|du| |dv|) over 21 x 21 points of each face's parameter
// rectangle, and where.
void probe_rounding_faces(const Shape& filleted) {
  for (const int f : rounding_faces(filleted)) {
    const TopoDS_Face& face = filleted.face(f);
    double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
    BRepTools::UVBounds(face, u1, u2, v1, v2);
    const BRepAdaptor_Surface surface(face, false);
    double least = 1.0;
    gp_Pnt where;
    for (int i = 0; i <= 20; ++i) {
      for (int j = 0; j <= 20; ++j) {
        gp_Pnt p;
        gp_Vec du;
        gp_Vec dv;
        surface.D1(u1 + (u2 - u1) * i / 20.0, v1 + (v2 - v1) * j / 20.0, p, du, dv);
        const double size = du.Magnitude() * dv.Magnitude();
        const double ratio = size > 0.0 ? du.Crossed(dv).Magnitude() / size : 0.0;
        if (ratio < least) {
          least = ratio;
          where = p;
        }
      }
    }
    const char* name = filleted.face_names(f).empty() ? "" : filleted.face_names(f).front().c_str();
    std::printf("  rounding face %d (%s): type %d, u %.6g..%.6g v %.6g..%.6g, least normal ratio %.3g at (%.4f "
                "%.4f %.4f)\n",
                f, name, int(surface.GetType()), u1, u2, v1, v2, least, where.X(), where.Y(), where.Z());
  }
}

void probe_shell(const Shape& body, const char* what) {
  try {
    const ShapePtr wall = shelled(body);
    std::printf(" shell of %s: ok, valid %d, largest tolerance %g, solids %d, volume %.6f, notes %zu\n", what,
                int(valid(*wall)), max_tolerance(wall->occt()),
                solids(wall->occt()), volume(*wall), wall->notes().size());
  } catch (const std::exception& error) {
    std::printf(" shell of %s: %s\n", what, error.what());
  }
  probe_kernel_shell(body, what);
}

// The statuses of a check that are faults, as numbers.
std::string faults(const NCollection_List<BRepCheck_Status>& statuses) {
  std::string out;
  for (NCollection_List<BRepCheck_Status>::Iterator it(statuses); it.More(); it.Next()) {
    if (it.Value() != BRepCheck_NoError) {
      out += (out.empty() ? "" : " ") + std::to_string(int(it.Value()));
    }
  }
  return out;
}

// The faces of an invalid shape that the checker faults, with their wires'
// faults, surface types and boxes.
void probe_faulty_faces(const TopoDS_Shape& shape) {
  const BRepCheck_Analyzer analyzer(shape);
  int index = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next(), ++index) {
    const occ::handle<BRepCheck_Result>& result = analyzer.Result(f.Current());
    std::string said = result.IsNull() ? std::string() : faults(result->Status());
    for (TopExp_Explorer w(f.Current(), TopAbs_WIRE); w.More(); w.Next()) {
      const occ::handle<BRepCheck_Result>& wire = analyzer.Result(w.Current());
      if (!wire.IsNull()) {
        const std::string own = faults(wire->Status());
        const std::string on = wire->IsStatusOnShape(f.Current()) ? faults(wire->StatusOnShape(f.Current())) : "";
        if (!own.empty() || !on.empty()) {
          said += " wire [" + own + "] on face [" + on + "]";
        }
      }
    }
    if (said.empty()) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    double x1 = 0.0, y1 = 0.0, z1 = 0.0, x2 = 0.0, y2 = 0.0, z2 = 0.0;
    if (!box.IsVoid()) {
      box.Get(x1, y1, z1, x2, y2, z2);
    }
    std::printf("   faulty face %d (type %d, box %.3f %.3f %.3f to %.3f %.3f %.3f): %s\n", index,
                int(BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType()), x1, y1, z1, x2, y2, z2, said.c_str());
  }
}

// The offset's analysis of each edge of the body (as BRepOffset_MakeOffset
// makes it, angle 4 asin(1e-6)): its ends and the types of its intervals
// (ChFiDS_TypeOfConcavity: 0 concave, 1 convex, 2 tangential, ...).
void probe_offset_analysis(const Shape& body) {
  const BRepOffset_Analyse analyse(body.occt(), 4.0 * std::asin(1e-6));
  for (int e = 0; e < body.edge_count(); ++e) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(body.edge(e), a, b);
    std::string types;
    try {
      const NCollection_List<BRepOffset_Interval>& intervals = analyse.Type(body.edge(e));
      for (NCollection_List<BRepOffset_Interval>::Iterator it(intervals); it.More(); it.Next()) {
        types += (types.empty() ? "" : " ") + std::to_string(int(it.Value().Type()));
      }
    } catch (const Standard_Failure&) {
      types = "none";
    }
    const gp_Pnt pa = a.IsNull() ? gp_Pnt() : BRep_Tool::Pnt(a);
    const gp_Pnt pb = b.IsNull() ? gp_Pnt() : BRep_Tool::Pnt(b);
    std::printf("  edge %s (%.4f %.4f %.4f) to (%.4f %.4f %.4f): offset types %s\n", body.edge_name(e).c_str(),
                pa.X(), pa.Y(), pa.Z(), pb.X(), pb.Y(), pb.Z(), types.c_str());
  }
}

// The kernel's thick solid (Intersection) of the body with its ends open,
// the crease intersection traced (port patch 2140), the faulty faces of an
// invalid result.
void probe_traced_kernel_shell(const TopoDS_Shape& shape, const NCollection_List<TopoDS_Shape>& open,
                               const char* what) {
  std::printf(" traced kernel shell of %s:\n", what);
  std::fflush(stdout);
  set_environment("MITCAD_TRACE_OFFSET_CREASE", "1");
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(shape, open, -kWall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
    std::fflush(stderr);
    std::printf("  done %d, error %d", int(maker.IsDone()), int(maker.MakeOffset().Error()));
    if (maker.IsDone()) {
      const bool ok = BRepCheck_Analyzer(maker.Shape()).IsValid();
      std::printf(", valid %d, solids %d\n", int(ok), solids(maker.Shape()));
      if (!ok) {
        probe_faulty_faces(maker.Shape());
      }
    } else {
      std::printf("\n");
    }
  } catch (const Standard_Failure& failure) {
    std::fflush(stderr);
    std::printf("  raised %s: '%s'\n", typeid(failure).name(), failure.what());
  }
  set_environment("MITCAD_TRACE_OFFSET_CREASE", "");
  std::fflush(stdout);
}

NCollection_List<TopoDS_Shape> open_ends(const Shape& body) {
  NCollection_List<TopoDS_Shape> open;
  for (const char* end : kOpenEnds) {
    for (const int f : body.find_faces(end)) {
      open.Append(body.face(f));
    }
  }
  return open;
}

// The kernel's fillet with the walk traced (port patch 2141).
void probe_traced_kernel_fillet(const Shape& body, double r, const char* what) {
  std::printf(" traced kernel fillet %g of %s:\n", r, what);
  std::fflush(stdout);
  set_environment("MITCAD_TRACE_FILLET_WALK", "1");
  probe_kernel_fillet(body, r);
  std::fflush(stdout);
  std::fflush(stderr);
  set_environment("MITCAD_TRACE_FILLET_WALK", "");
}

// The body as the shell offsets it: where a face's surface is only C0, its
// faces split at their creases into C1 pieces with the face's names
// (faceops.cpp's split_at_creases); else the body itself. (Reading the
// divider's context where it split nothing crashed the probe.)
ShapePtr split_like_the_shell(const Shape& body) {
  bool creased = false;
  for (int f = 0; f < body.face_count() && !creased; ++f) {
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(body.face(f));
    creased = !surface.IsNull() && surface->Continuity() == GeomAbs_C0;
  }
  if (!creased) {
    return std::make_shared<Shape>(body);
  }
  ShapeUpgrade_ShapeDivideContinuity divide(body.occt());
  divide.SetSurfaceCriterion(GeomAbs_C1);
  divide.Perform();
  const occ::handle<ShapeBuild_ReShape> context = divide.GetContext();
  std::vector<Shape::NamedFace> named;
  for (int f = 0; f < body.face_count(); ++f) {
    for (TopExp_Explorer it(context->Apply(body.face(f)), TopAbs_FACE); it.More(); it.Next()) {
      named.push_back({it.Current(), body.face_names(f)});
    }
  }
  return std::make_shared<Shape>(divide.Result(), named);
}

// The thick solid as the shell builds it after a rounding: the faces of
// C0 surfaces split at their creases first (faceops.cpp's split_at_creases).
void probe_split_shell(const Shape& body, const char* what) {
  const ShapePtr split = split_like_the_shell(body);
  std::printf(" %s split at its creases: %d faces (%d before)\n", what, split->face_count(), body.face_count());
  probe_traced_kernel_shell(split->occt(), open_ends(*split), what);
}

// A step of the probe as one of the library's operations: a crash inside
// OCCT is an error that the probe prints, not the end of the run.
void probe_step(const char* what, const std::function<void()>& work) {
  try {
    run_operation(what, work);
  } catch (const std::exception& error) {
    std::fflush(stderr);
    std::printf(" %s failed: %s\n", what, error.what());
  }
  std::fflush(stdout);
  std::fflush(stderr);
}

// The ends of an edge, briefly.
std::string ends_of(const TopoDS_Shape& edge) {
  TopoDS_Vertex a;
  TopoDS_Vertex b;
  TopExp::Vertices(TopoDS::Edge(edge), a, b);
  if (a.IsNull() || b.IsNull()) {
    // Its vertices as they are (orientation 2 internal, 3 external) and its
    // curve's ends.
    std::string out = "(no first and last vertex; vertices:";
    for (TopoDS_Iterator v(edge, false, false); v.More(); v.Next()) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Value()));
      char one[96];
      std::snprintf(one, sizeof one, " (%.3f %.3f %.3f) o%d", p.X(), p.Y(), p.Z(), int(v.Value().Orientation()));
      out += one;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(TopoDS::Edge(edge), first, last);
    if (!curve.IsNull()) {
      const gp_Pnt p = curve->Value(first);
      const gp_Pnt q = curve->Value(last);
      char ends[128];
      std::snprintf(ends, sizeof ends, "; curve (%.3f %.3f %.3f)-(%.3f %.3f %.3f)", p.X(), p.Y(), p.Z(), q.X(), q.Y(),
                    q.Z());
      out += ends;
    } else {
      out += "; no 3d curve";
    }
    return out + ")";
  }
  char text[160];
  const gp_Pnt pa = BRep_Tool::Pnt(a);
  const gp_Pnt pb = BRep_Tool::Pnt(b);
  std::snprintf(text, sizeof text, "(%.3f %.3f %.3f)-(%.3f %.3f %.3f)%s%s", pa.X(), pa.Y(), pa.Z(), pb.X(), pb.Y(),
                pb.Z(), a.IsSame(b) ? " closed" : "", BRep_Tool::Degenerated(TopoDS::Edge(edge)) ? " degenerated" : "");
  return text;
}

// The kernel's thick solid of the shape (Intersection, the ends open) step
// by step as BRepOffset_MakeOffset keeps it: the images of the body's faces
// (each one's wires and edges, whether valid) and of its edges, so that a
// face it did not rebuild shows (the crease intersection traced too).
void probe_offset_images(const Shape& body, const char* what) {
  std::printf(" offset images of %s:\n", what);
  std::fflush(stdout);
  set_environment("MITCAD_TRACE_OFFSET_CREASE", "1");
  // (Through the API, as MitCad's shell and the third round's probe run it:
  // the offset driven directly crashed in the fourth round's probe.)
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body.occt(), open_ends(body), -kWall, 1e-6, BRepOffset_Skin, false, false,
                               GeomAbs_Intersection, false);
  } catch (const Standard_Failure& failure) {
    std::fflush(stderr);
    std::printf("  raised %s: '%s'\n", typeid(failure).name(), failure.what());
  }
  std::fflush(stderr);
  set_environment("MITCAD_TRACE_OFFSET_CREASE", "");
  const BRepOffset_MakeOffset& offset = maker.MakeOffset();
  std::printf("  done %d, error %d\n", int(offset.IsDone()), int(offset.Error()));
  std::fflush(stdout);
  const BRepAlgo_Image& faces = offset.OffsetFacesFromShapes();
  for (int f = 0; f < body.face_count(); ++f) {
    NCollection_List<TopoDS_Shape> images;
    if (faces.HasImage(body.face(f))) {
      faces.LastImage(body.face(f), images);
    }
    std::printf("  face %d %s: %d images\n", f, body.face_names(f).empty() ? "" : body.face_names(f).front().c_str(),
                images.Extent());
    for (NCollection_List<TopoDS_Shape>::Iterator image(images); image.More(); image.Next()) {
      if (image.Value().ShapeType() != TopAbs_FACE) {
        std::printf("   image of type %d\n", int(image.Value().ShapeType()));
        continue;
      }
      std::printf("   image face: valid %d, type %d\n", int(BRepCheck_Analyzer(image.Value()).IsValid()),
                  int(BRepAdaptor_Surface(TopoDS::Face(image.Value())).GetType()));
      for (TopExp_Explorer wire(image.Value(), TopAbs_WIRE); wire.More(); wire.Next()) {
        std::printf("    wire:\n");
        for (TopExp_Explorer edge(wire.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
          // With the ends of its curve on the face (u, v; which side of a
          // periodic face's seam) and its orientation.
          double first = 0.0;
          double last = 0.0;
          const occ::handle<Geom2d_Curve> pcurve =
              BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Current()), TopoDS::Face(image.Value()), first, last);
          if (pcurve.IsNull()) {
            std::printf("     %s, o%d, no curve on the face\n", ends_of(edge.Current()).c_str(),
                        int(edge.Current().Orientation()));
          } else {
            const gp_Pnt2d a = pcurve->Value(first);
            const gp_Pnt2d b = pcurve->Value(last);
            std::printf("     %s, o%d, uv (%.4f %.4f)-(%.4f %.4f)\n", ends_of(edge.Current()).c_str(),
                        int(edge.Current().Orientation()), a.X(), a.Y(), b.X(), b.Y());
          }
        }
      }
    }
  }
  const BRepAlgo_Image& edges = offset.OffsetEdgesFromShapes();
  for (int e = 0; e < body.edge_count(); ++e) {
    NCollection_List<TopoDS_Shape> images;
    if (edges.HasImage(body.edge(e))) {
      edges.LastImage(body.edge(e), images);
    }
    std::printf("  edge %s %s: %d images\n", body.edge_name(e).c_str(), ends_of(body.edge(e)).c_str(),
                images.Extent());
    for (NCollection_List<TopoDS_Shape>::Iterator image(images); image.More(); image.Next()) {
      if (image.Value().ShapeType() == TopAbs_EDGE) {
        std::printf("   %s\n", ends_of(image.Value()).c_str());
      }
    }
  }
  std::fflush(stdout);
}

// --probe: the fourth round's diagnosis (mitcad#2140): the offset's images
// of the tube's faces and edges where its thick solid fails, and MitCad's
// shells after the roundings of 6 and 50.
int probe() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    std::printf("no Y tube\n");
    return 1;
  }
  probe_step("offset images of the tube", [&] { probe_offset_images(*body, "the tube"); });
  for (const double r : {6.0, 50.0}) {
    probe_step("offset images after the rounding", [&] {
      const ShapePtr filleted = rounded(*body, r);
      // As the shell offsets it: its C0 faces split at their creases.
      const ShapePtr split_body = split_like_the_shell(*filleted);
      const Shape& split = *split_body;
      std::printf(" the tube rounded %g split at its creases: %d faces (%d before)\n", r, split.face_count(),
                  filleted->face_count());
      probe_offset_images(split, r == 6.0 ? "the tube rounded 6" : "the tube rounded 50");
    });
  }
  return 0;
}

// --probe-fillet-shell: only the kernel's thick solid of the tube rounded
// with 6, as the shell runs it (its faces split at their creases where they
// are C0), for a debugger to stop where it throws (the fifth round's probe:
// Standard_OutOfRange without a message).
int probe_fillet_shell() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    return 1;
  }
  const ShapePtr filleted = rounded(*body, 6.0);
  // MitCad's shell first (its working copy, its split where faces are C0),
  // as the ctests run it.
  try {
    const ShapePtr wall = shelled(*filleted);
    std::printf("shell: ok, valid %d, largest tolerance %g, volume %.6f\n", int(valid(*wall)),
                max_tolerance(wall->occt()), volume(*wall));
  } catch (const std::exception& error) {
    std::printf("shell: %s\n", error.what());
  }
  std::fflush(stdout);
  const ShapePtr split = split_like_the_shell(*filleted);
  const NCollection_List<TopoDS_Shape> open = open_ends(*split);
  std::printf("the tube rounded 6: %d faces, %d open\n", split->face_count(), open.Extent());
  std::fflush(stdout);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(split->occt(), open, -kWall, 1e-6, BRepOffset_Skin, false, false,
                               GeomAbs_Intersection, false);
    std::printf("done %d, error %d\n", int(maker.IsDone()), int(maker.MakeOffset().Error()));
  } catch (const Standard_Failure& failure) {
    std::printf("raised %s: '%s'\n", typeid(failure).name(), failure.what());
  }
  return 0;
}

// --probe-third: the third round's: the traced kernel shell of the tube,
// MitCad's shell, the traced kernel fillets at 8 and 50, MitCad's fillets
// and the shells after them, and with the seams turned.
int probe_third() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    std::printf("no Y tube\n");
    return 1;
  }
  std::printf("Y tube: valid %d, solids %d, volume %.6f, faces %d, edges %d\n", int(valid(*body)),
              solids(body->occt()), volume(*body), body->face_count(), body->edge_count());
  probe_step("traced kernel shell of the tube",
             [&] { probe_traced_kernel_shell(body->occt(), open_ends(*body), "the tube"); });
  probe_step("shell of the tube", [&] { probe_shell(*body, "the tube"); });
  probe_step("traced kernel fillet 8", [&] { probe_traced_kernel_fillet(*body, 8.0, "the tube"); });
  probe_step("traced kernel fillet 50", [&] { probe_traced_kernel_fillet(*body, 50.0, "the tube"); });
  for (const double r : {6.0, 8.0, 20.0, 50.0}) {
    probe_step("fillet and shell", [&] {
      ShapePtr filleted;
      try {
        filleted = rounded(*body, r);
      } catch (const std::exception& error) {
        std::printf("fillet %g: %s\n", r, error.what());
        return;
      }
      std::printf("fillet %g: ok, valid %d, volume %.6f, notes %zu, infos %zu, lowest point at the middle %s\n", r,
                  int(valid(*filleted)), volume(*filleted), filleted->notes().size(), filleted->infos().size(),
                  state(*filleted, gp_Pnt(0, 0, rounding_low_point(r) - 0.02)) == TopAbs_IN &&
                          state(*filleted, gp_Pnt(0, 0, rounding_low_point(r) + 0.02)) == TopAbs_OUT
                      ? "right"
                      : "wrong");
      std::fflush(stdout);
      if (r == 6.0 || r == 50.0) {
        probe_shell(*filleted, r == 6.0 ? "the tube rounded 6" : "the tube rounded 50");
      }
      if (r == 6.0) {
        probe_split_shell(*filleted, "the tube rounded 6");
      }
    });
  }
  const ShapePtr turned = y_tube(kTube, kPi);
  if (turned) {
    probe_step("traced kernel shell of the tube, seams turned",
               [&] { probe_traced_kernel_shell(turned->occt(), open_ends(*turned), "the tube, seams turned"); });
    for (const double r : {6.0, 50.0}) {
      probe_step("fillet, seams turned", [&] {
        try {
          const ShapePtr filleted = rounded(*turned, r);
          std::printf("seams turned, fillet %g: ok, valid %d, notes %zu, infos %zu\n", r, int(valid(*filleted)),
                      filleted->notes().size(), filleted->infos().size());
        } catch (const std::exception& error) {
          std::printf("seams turned, fillet %g: %s\n", r, error.what());
        }
      });
    }
  }
  return 0;
}

// --probe-second: the second round's: the offset's analysis of the tube's
// edges, the traced shells, the traced walks at 7 and 8.
int probe_second() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    std::printf("no Y tube\n");
    return 1;
  }
  probe_offset_analysis(*body);
  probe_step("traced kernel fillet 7", [&] { probe_traced_kernel_fillet(*body, 7.0, "the tube"); });
  probe_step("traced kernel fillet 8", [&] { probe_traced_kernel_fillet(*body, 8.0, "the tube"); });
  const ShapePtr turned = y_tube(kTube, kPi);
  if (turned) {
    probe_offset_analysis(*turned);
    probe_step("traced kernel fillet 6, seams turned",
               [&] { probe_traced_kernel_fillet(*turned, 6.0, "the tube, seams turned"); });
  }
  return 0;
}

// --probe-first: the first round's diagnosis: what each radius and each
// shell gives.
int probe_first() {
  const ShapePtr body = y_tube(kTube);
  if (!body) {
    std::printf("no Y tube\n");
    return 1;
  }
  probe_shell(*body, "the tube");
  double largest = 0.0;
  ShapePtr largest_round;
  for (const double r : {6.0, 6.5, 7.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0, 50.0}) {
    try {
      const ShapePtr filleted = rounded(*body, r);
      std::printf("fillet %g: ok, valid %d, volume %.6f, notes %zu, infos %zu\n", r, int(valid(*filleted)),
                  volume(*filleted), filleted->notes().size(), filleted->infos().size());
      if (r == 6.0 || r == 50.0) {
        probe_rounding_faces(*filleted);
        probe_shell(*filleted, r == 6.0 ? "the tube rounded 6" : "the tube rounded 50");
      }
      largest = r;
      largest_round = filleted;
    } catch (const std::exception& error) {
      std::printf("fillet %g: %s\n", r, error.what());
      probe_kernel_fillet(*body, r);
    }
  }
  if (largest_round && largest != 6.0 && largest != 50.0) {
    probe_rounding_faces(*largest_round);
    probe_shell(*largest_round, "the tube rounded with the largest radius that builds");
  }
  // The same tube with the circle's seam turned half round: the seams on
  // the arms' inner sides, the crotch one edge that no seam meets.
  const ShapePtr turned = y_tube(kTube, kPi);
  if (!turned) {
    std::printf("no Y tube with the seams turned\n");
    return 0;
  }
  std::printf("Y tube, seams turned: valid %d, volume %.6f, crotch edges %zu\n", int(valid(*turned)),
              volume(*turned), crotch_edges(*turned).size());
  probe_shell(*turned, "the tube, seams turned");
  for (const double r : {6.0, 8.0, 20.0, 50.0}) {
    try {
      const ShapePtr filleted = rounded(*turned, r);
      std::printf("seams turned, fillet %g: ok, valid %d, volume %.6f, notes %zu, infos %zu\n", r,
                  int(valid(*filleted)), volume(*filleted), filleted->notes().size(), filleted->infos().size());
    } catch (const std::exception& error) {
      std::printf("seams turned, fillet %g: %s\n", r, error.what());
      probe_kernel_fillet(*turned, r);
    }
  }
  return 0;
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  // A crash inside OCCT is an operation's error, not the end of the run.
  mitcad::geometry::catch_occt_crashes();
  if (argc > 1 && std::strcmp(argv[1], "--probe") == 0) {
    return test::probe();
  }
  if (argc > 1 && std::strcmp(argv[1], "--probe-fillet-shell") == 0) {
    return test::probe_fillet_shell();
  }
  if (argc > 1 && std::strcmp(argv[1], "--probe-third") == 0) {
    return test::probe_third();
  }
  if (argc > 1 && std::strcmp(argv[1], "--probe-second") == 0) {
    return test::probe_second();
  }
  if (argc > 1 && std::strcmp(argv[1], "--probe-first") == 0) {
    return test::probe_first();
  }
  test::guarded("test_body", test::test_body);
  test::guarded("test_crotch_rounded_6", test::test_crotch_rounded_6);
  test::guarded("test_crotch_rounded_50", test::test_crotch_rounded_50);
  test::guarded("test_shell_without_rounding", test::test_shell_without_rounding);
  test::guarded("test_shell_after_rounding_6", test::test_shell_after_rounding_6);
  test::guarded("test_shell_after_rounding_50", test::test_shell_after_rounding_50);
  test::guarded("test_refused_shell_says_why", test::test_refused_shell_says_why);
  if (test::failures == 0) {
    std::puts("test_y_tube_crotch: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
