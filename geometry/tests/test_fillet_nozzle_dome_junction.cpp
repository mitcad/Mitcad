// SPDX-License-Identifier: MIT
// A nozzle's foot rounded where the nozzle meets a dome on a tube exactly
// at the line where the dome runs on tangentially into the tube (GitHub
// mitcad/Mitcad#582, mitcad#2272).
//
// The body as the reporter's (F2): a tube 40 to 50 about the z axis from
// z = 0 to 50 and on it a hollow hemisphere of the same radii about
// (0, 0, 50), turned a full turn about z from a profile in the xz plane
// (the dome's outer face F2:side(c17[c2+c3,c19]), the tube's outer face
// F2:side(c2[c1,c3+c17]), which meet tangentially along the circle at
// z = 50; their seam at +x). The nozzle (F5): a circle of radius 15 about
// z = 50 on a plane 80 from the axis, extruded towards the axis up to the
// dome's face continued past its edges, joined: its side face
// F5:side(c1) meets the dome above z = 50 and the tube below, so that its
// foot is a closed tangent chain of two edges whose two vertices lie on the
// dome's edge with the tube, where the faces under the rounding change.
// The nozzle is placed at four angles about z (the reporter's nozzle and
// its pattern's three copies; one of them lies across the faces' seam), and
// as a control 20 higher, wholly on the dome.
//
// Each foot is rounded with 1.5 (the reporter's radius) by the kernel's
// fillet alone and by MitCad's, the chain picked by its dome edge and,
// again, by its tube edge: one valid shape from the kernel, one valid
// closed solid at the exact size (no info of a smaller one, no note) from
// MitCad, and the material the rounding adds the same at every angle (the
// body is the same turned about z), within 1e-4 of it (the walked blend's
// approximation). On the reporter's design the original nozzle's foot
// failed ("the geometry kernel's computation of the rounding along edge
// E{F2:side(c17[c2+c3,c19])|F5:side(c1)} (and 1 more edges of its chain)
// fails") while the pattern's copies built, and a copy failed when picked
// by its tube edge. The cause (port patch 2272): the walk round the foot
// ends at the chain's other vertex on both faces' restrictions at once (the
// circle on the body, the nozzle's seam), where both vertices and the
// section lie in one plane normal to the closed guide, and took the
// guide's end parameter there, half a turn away; across the seams, a
// piece ending on the circle alone was moved to the circle's far end, the
// other vertex.
//
// Printed with `--probe` (to compare with the reporter's design): the
// foot's vertices against the exact points where the nozzle's side meets
// the circle at z = 50, their tolerances and the angle between the two
// edges' tangents there (MITCAD_TRACE_FILLET_WALK traces the walks).
// With MITCAD_NOZZLE_BODY naming a .brep of the reported body before the
// copies' roundings, the same is printed for each of its four nozzles'
// feet (found by their faces' surfaces), with both fillets, unchecked.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Sphere.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/revolve.hpp"
#include "mitcad/geometry/target.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kInner   = 40.0;  // the tube's and the dome's inner radius
constexpr double kOuter   = 50.0;  // their outer radius
constexpr double kHeight  = 50.0;  // the tube's top, the dome's centre
constexpr double kNozzle  = 15.0;  // the nozzle's radius
constexpr double kPlane   = 80.0;  // the nozzle's sketch plane from the axis
constexpr double kRadius  = 1.5;   // the rounding (the reporter's)
constexpr double kControl = 70.0;  // the control nozzle's centre: wholly on the dome

const std::string kDome = "F2:side(c17[c2+c3,c19])";
const std::string kTube = "F2:side(c2[c1,c3+c17])";
const std::string kSide = "F5:side(c1)";

bool probe = false;

Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  Segment s;
  s.name  = name;
  s.kind  = SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end   = gp_Pnt2d(x1, y1);
  return s;
}

Segment arc(const std::string& name, double radius, double from, double to) {
  Segment s;
  s.name        = name;
  s.kind        = SegmentKind::Arc;
  s.center      = gp_Pnt2d(0.0, kHeight);
  s.radius      = radius;
  s.start_angle = from;
  s.end_angle   = to;
  return s;
}

// The reporter's revolved body, the tube's and the dome's regions keyed as
// in the reporter's sketch.
ShapePtr dome_on_tube() {
  Region tube;
  tube.name = "r{c1[c4,c2],c2[c1,c3+c17],c3[c2+c17,c4+c12],c4[c3+c12,c1]}";
  Loop tube_loop;
  tube_loop.segments.push_back(line("c1[c4,c2]", kInner, 0.0, kOuter, 0.0));
  tube_loop.segments.push_back(line("c2[c1,c3+c17]", kOuter, 0.0, kOuter, kHeight));
  tube_loop.segments.push_back(line("c3[c2+c17,c4+c12]", kOuter, kHeight, kInner, kHeight));
  tube_loop.segments.push_back(line("c4[c3+c12,c1]", kInner, kHeight, kInner, 0.0));
  tube.loops.push_back(tube_loop);

  Region dome;
  dome.name = "r{c3[c2+c17,c4+c12],c12[c3+c4,c19],c17[c2+c3,c19],c19[c17,c12]}";
  Loop dome_loop;
  dome_loop.segments.push_back(line("c3[c2+c17,c4+c12]", kInner, kHeight, kOuter, kHeight));
  dome_loop.segments.push_back(arc("c17[c2+c3,c19]", kOuter, 0.0, kPi / 2.0));
  dome_loop.segments.push_back(line("c19[c17,c12]", 0.0, kHeight + kOuter, 0.0, kHeight + kInner));
  dome_loop.segments.push_back(arc("c12[c3+c4,c19]", kInner, 0.0, kPi / 2.0));
  dome.loops.push_back(dome_loop);

  RevolveSpec spec;
  spec.feature        = "F2";
  spec.frame.origin   = gp_Pnt(0.0, 0.0, 0.0);
  spec.frame.x_axis   = gp_Dir(1.0, 0.0, 0.0);
  spec.frame.y_axis   = gp_Dir(0.0, 0.0, 1.0);
  spec.regions        = {tube, dome};
  spec.axis           = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0));
  spec.angle1         = 2.0 * kPi;
  return revolve(spec);
}

gp_Trsf turn(double degrees) {
  gp_Trsf t;
  t.SetRotation(gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), degrees * kPi / 180.0);
  return t;
}

// The nozzle about height `centre` at `degrees` about z from +y, extruded
// towards the axis up to the dome's face continued, joined to the body.
ShapePtr with_nozzle(const ShapePtr& body, double degrees, double centre) {
  const gp_Trsf t = turn(degrees);
  ExtrudeFeatureSpec spec;
  spec.feature      = "F5";
  spec.frame.origin = gp_Pnt(0.0, kPlane, 0.0).Transformed(t);
  spec.frame.x_axis = gp_Dir(1.0, 0.0, 0.0).Transformed(t);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.direction    = spec.frame.normal();
  Region region     = circle_region(0.0, centre, kNozzle);
  region.loops[0].segments[0].name = "c1";
  region.name                      = "r{c1}";
  spec.regions                     = {region};
  Target target;
  target.kind        = Target::Kind::Face;
  target.body        = body;
  target.face        = kDome;
  target.extend      = true;
  spec.side1.target  = target;
  const ShapePtr nozzle = extrude_feature(spec);
  const BooleanResult joined = boolean(BooleanOp::Join, {body.get()}, *nozzle);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.size() == 1 ? joined.pieces.front().shape : nullptr;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-12, true);
  return props.Mass();
}

int solids_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

bool between(const std::string& name, const std::string& a, const std::string& b) {
  return name.find(a) != std::string::npos && name.find(b) != std::string::npos;
}

// The foot's edges: between the nozzle's side and the dome, and the tube.
std::vector<int> foot_edges(const Shape& body, const std::string& face) {
  std::vector<int> out;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (between(body.edge_name(e), kSide, face)) {
      out.push_back(e);
    }
  }
  return out;
}

// The edge's unit tangent at its end at the vertex.
gp_Vec tangent_at(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  const BRepAdaptor_Curve curve(edge);
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(edge, first, last);
  const double u = first.IsSame(vertex) ? curve.FirstParameter() : curve.LastParameter();
  gp_Pnt p;
  gp_Vec d;
  curve.D1(u, p, d);
  return d.Magnitude() > 0.0 ? d.Normalized() : d;
}

// Prints where the foot's dome and tube edges meet: each vertex they share
// against the nearest exact point where the nozzle's side crosses the circle
// at z = 50, its tolerance, and the angle between the edges' tangents.
// `out`: the nozzle's axis, horizontal, away from the body's axis.
void print_foot(const Shape& body, const gp_Dir& out, double degrees, const std::vector<int>& dome,
                const std::vector<int>& tube) {
  const double y = std::sqrt(kOuter * kOuter - kNozzle * kNozzle);
  const gp_Vec along(out);
  const gp_Vec across = along.Crossed(gp_Vec(0.0, 0.0, 1.0));
  const gp_Pnt centre(0.0, 0.0, kHeight);
  const std::array<gp_Pnt, 2> exact = {centre.Translated(along * y + across * kNozzle),
                                       centre.Translated(along * y - across * kNozzle)};
  for (const int d : dome) {
    for (const int u : tube) {
      for (TopExp_Explorer a(body.edge(d), TopAbs_VERTEX); a.More(); a.Next()) {
        for (TopExp_Explorer b(body.edge(u), TopAbs_VERTEX); b.More(); b.Next()) {
          if (!a.Current().IsSame(b.Current())) {
            continue;
          }
          const TopoDS_Vertex v = TopoDS::Vertex(a.Current());
          const gp_Pnt        p = BRep_Tool::Pnt(v);
          const double off = std::min(p.Distance(exact[0]), p.Distance(exact[1]));
          const double sine = tangent_at(body.edge(d), v).Crossed(tangent_at(body.edge(u), v)).Magnitude();
          std::printf("  %3.0f deg: vertex (%.6f, %.6f, %.6f) %.3g mm from the exact point, tolerance %.3g, "
                      "edge tolerances %.3g and %.3g, the edges' tangents %.4g deg apart\n",
                      degrees, p.X(), p.Y(), p.Z(), off, BRep_Tool::Tolerance(v),
                      BRep_Tool::Tolerance(body.edge(d)), BRep_Tool::Tolerance(body.edge(u)),
                      std::asin(std::min(1.0, sine)) * 180.0 / kPi);
        }
      }
    }
  }
}

// OCCT's fillet alone of the chain picked by the edge, printed: whether it
// built one valid shape.
bool print_occt_alone(const Shape& body, int edge, const char* by) {
  try {
    BRepFilletAPI_MakeFillet maker(body.occt());
    maker.Add(kRadius, body.edge(edge));
    maker.Build();
    if (!maker.IsDone()) {
      std::printf("    OCCT alone, by its %s edge: not done (failure %d, contour %d)\n", by,
                  static_cast<int>(maker.Failure()), maker.FailedContour());
      return false;
    }
    const bool valid = BRepCheck_Analyzer(maker.Shape()).IsValid();
    std::printf("    OCCT alone, by its %s edge: done, %s, adds %.6f\n", by, valid ? "valid" : "invalid",
                volume_of(maker.Shape()) - volume_of(body.occt()));
    return valid;
  } catch (const Standard_Failure& failure) {
    std::printf("    OCCT alone, by its %s edge: %s\n", by, failure.what());
    return false;
  }
}

// MitCad's fillet of the chain picked by the edge: one valid closed solid
// at the exact size without a note; the material it adds, or -1.
double rounded_adds(const Shape& body, int edge, const char* by, double degrees, double centre) {
  try {
    const ShapePtr result = fillet("F6", body, {body.edge_name(edge)}, kRadius);
    const bool valid = BRepCheck_Analyzer(result->occt()).IsValid();
    const double adds = volume_of(result->occt()) - volume_of(body.occt());
    std::printf("    MitCad, by its %s edge: %s, %d solid(s), %zu note(s), %zu info(s), adds %.6f\n", by,
                valid ? "valid" : "invalid", solids_of(result->occt()), result->notes().size(),
                result->infos().size(), adds);
    CHECK(valid);
    CHECK(solids_of(result->occt()) == 1);
    CHECK(result->notes().empty());
    CHECK(result->infos().empty()); // the exact size, not 0.1 % smaller
    CHECK(adds > 0.0);
    return valid ? adds : -1.0;
  } catch (const std::exception& failure) {
    std::fprintf(stderr, "nozzle at %.0f deg, z %.0f, by its %s edge: %s\n", degrees, centre, by, failure.what());
    ++failures;
    return -1.0;
  }
}

// The nozzle about `centre` at the four angles, its foot rounded picked by
// each of its edges; the material added the same everywhere.
void round_feet(double centre) {
  const ShapePtr body = dome_on_tube();
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  std::vector<double> added;
  for (const double degrees : {0.0, 90.0, 180.0, 270.0}) {
    const ShapePtr joined = with_nozzle(body, degrees, centre);
    if (!joined) {
      continue;
    }
    const std::vector<int> dome = foot_edges(*joined, kDome);
    const std::vector<int> tube = foot_edges(*joined, kTube);
    std::printf("  nozzle at %.0f deg, z %.0f: %zu dome edge(s), %zu tube edge(s) at its foot\n", degrees, centre,
                dome.size(), tube.size());
    CHECK(!dome.empty());
    if (centre == kHeight) {
      CHECK(!tube.empty());
      if (probe) {
        print_foot(*joined, gp_Dir(0.0, 1.0, 0.0).Transformed(turn(degrees)), degrees, dome, tube);
      }
    } else {
      CHECK(tube.empty());
    }
    if (dome.empty()) {
      continue;
    }
    // The kernel's fillet alone (port patch 2272): its walk round the foot
    // ends at the chain's other vertex on both faces' restrictions.
    CHECK(print_occt_alone(*joined, dome.front(), "dome"));
    if (!tube.empty()) {
      CHECK(print_occt_alone(*joined, tube.front(), "tube"));
    }
    added.push_back(rounded_adds(*joined, dome.front(), "dome", degrees, centre));
    if (!tube.empty()) {
      added.push_back(rounded_adds(*joined, tube.front(), "tube", degrees, centre));
    }
  }
  // The walked blend is approximated within 1e-4 mm, its face a little
  // differently at each angle: the material added differs by up to 2e-5 of
  // it (the probe run before the patch).
  for (const double adds : added) {
    if (adds > 0.0 && added.front() > 0.0) {
      CHECK_NEAR_TOL(adds, added.front(), 1.0e-4);
    }
  }
}

// The reporter's case: the nozzle's foot across the dome's edge with the tube.
void test_foot_across_the_dome_edge() {
  round_feet(kHeight);
}

// The control: the nozzle wholly on the dome, its foot one edge on it.
void test_foot_on_the_dome() {
  round_feet(kControl);
}

// ---- The reported body (MITCAD_NOZZLE_BODY, a probe) ----

std::string env(const char* name) {
#ifdef _MSC_VER
  char*       value  = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

enum class Kind { Other, Nozzle, Dome, Tube };

// A face of the reported body by its surface: a nozzle's side (radius 15,
// its axis horizontal; a flange's groove too, which meets no dome or tube),
// the dome's outer face (50 from its centre) or the tube's.
Kind kind_of(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face);
  const gp_Dir z(0.0, 0.0, 1.0);
  switch (surface.GetType()) {
  case GeomAbs_Cylinder: {
    const gp_Cylinder cylinder = surface.Cylinder();
    if (std::abs(cylinder.Radius() - kOuter) < 1.0e-6 && cylinder.Axis().Direction().IsParallel(z, 1.0e-9)) {
      return Kind::Tube;
    }
    if (std::abs(cylinder.Radius() - kNozzle) < 1.0e-6 && std::abs(cylinder.Axis().Direction().Dot(z)) < 1.0e-9) {
      return Kind::Nozzle;
    }
    return Kind::Other;
  }
  case GeomAbs_Sphere:
    return std::abs(surface.Sphere().Radius() - kOuter) < 1.0e-6 ? Kind::Dome : Kind::Other;
  case GeomAbs_SurfaceOfRevolution: {
    const double u = 0.5 * (surface.FirstUParameter() + surface.LastUParameter());
    const double v = 0.5 * (surface.FirstVParameter() + surface.LastVParameter());
    return std::abs(surface.Value(u, v).Distance(gp_Pnt(0.0, 0.0, kHeight)) - kOuter) < 1.0e-6 ? Kind::Dome
                                                                                                : Kind::Other;
  }
  default:
    return Kind::Other;
  }
}

const char* surface_type(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face).GetType()) {
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_SurfaceOfRevolution:
    return "surface of revolution";
  case GeomAbs_BSplineSurface:
    return "B-spline";
  default:
    return "other";
  }
}

// MitCad's fillet of the chain picked by the edge, printed only.
void print_mitcad(const Shape& body, int edge, const char* by) {
  try {
    const ShapePtr result = fillet("F6", body, {body.edge_name(edge)}, kRadius);
    std::printf("    MitCad, by its %s edge: %s, %d solid(s), %zu note(s), %zu info(s), adds %.6f\n", by,
                BRepCheck_Analyzer(result->occt()).IsValid() ? "valid" : "invalid", solids_of(result->occt()),
                result->notes().size(), result->infos().size(),
                volume_of(result->occt()) - volume_of(body.occt()));
  } catch (const std::exception& failure) {
    std::printf("    MitCad, by its %s edge: %s\n", by, failure.what());
  }
}

struct Foot {
  gp_Dir out;
  std::vector<int> dome;
  std::vector<int> tube;
};

// The reported body before the copies' roundings (a .brep the probe run
// exports): each nozzle's foot found by its faces' surfaces, its vertices
// printed as the synthetic ones' and rounded with 1.5 by OCCT alone and by
// MitCad, picked by either edge. Nothing is checked: it shows how the
// original nozzle's foot differs from the copies'.
void test_reported_body() {
  const std::string path = env("MITCAD_NOZZLE_BODY");
  TopoDS_Shape      shape;
  BRep_Builder      builder;
  if (!BRepTools::Read(shape, path.c_str(), builder)) {
    std::fprintf(stderr, "cannot read %s\n", path.c_str());
    ++failures;
    return;
  }
  TopExp_Explorer solid(shape, TopAbs_SOLID);
  const ShapePtr  body = import_body(solid.More() ? solid.Current() : shape, "F1", 0);
  std::printf("reported body: %d faces, %d edges, %s\n", body->face_count(), body->edge_count(),
              BRepCheck_Analyzer(body->occt()).IsValid() ? "valid" : "invalid");
  std::vector<Foot> feet;
  for (int e = 0; e < body->edge_count(); ++e) {
    const std::array<int, 2> faces = body->edge_faces(e);
    if (faces[0] < 0 || faces[1] < 0) {
      continue;
    }
    const Kind a = kind_of(body->face(faces[0]));
    const Kind b = kind_of(body->face(faces[1]));
    const int  nozzle = a == Kind::Nozzle ? faces[0] : b == Kind::Nozzle ? faces[1] : -1;
    const Kind other  = a == Kind::Nozzle ? b : a;
    if (nozzle < 0 || (other != Kind::Dome && other != Kind::Tube)) {
      continue;
    }
    gp_Dir out = BRepAdaptor_Surface(body->face(nozzle)).Cylinder().Axis().Direction();
    const BRepAdaptor_Curve curve(body->edge(e));
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (gp_Vec(out).Dot(gp_Vec(middle.X(), middle.Y(), 0.0)) < 0.0) {
      out.Reverse();
    }
    auto found = std::find_if(feet.begin(), feet.end(), [&](const Foot& f) { return f.out.IsEqual(out, 1.0e-6); });
    if (found == feet.end()) {
      feet.push_back(Foot{out, {}, {}});
      found = feet.end() - 1;
    }
    (other == Kind::Dome ? found->dome : found->tube).push_back(e);
  }
  for (const Foot& foot : feet) {
    const double degrees = std::atan2(foot.out.Y(), foot.out.X()) * 180.0 / kPi;
    std::printf("  foot towards %.1f deg: %zu dome edge(s), %zu tube edge(s)\n", degrees, foot.dome.size(),
                foot.tube.size());
    for (const int e : foot.dome) {
      const std::array<int, 2> faces = body->edge_faces(e);
      std::printf("    dome edge %d (%s, faces %s and %s)\n", e, body->edge_name(e).c_str(),
                  surface_type(body->face(faces[0])), surface_type(body->face(faces[1])));
    }
    for (const int e : foot.tube) {
      std::printf("    tube edge %d (%s)\n", e, body->edge_name(e).c_str());
    }
    print_foot(*body, foot.out, degrees, foot.dome, foot.tube);
    if (!foot.dome.empty()) {
      print_occt_alone(*body, foot.dome.front(), "dome");
      print_mitcad(*body, foot.dome.front(), "dome");
    }
    if (!foot.tube.empty()) {
      print_occt_alone(*body, foot.tube.front(), "tube");
      print_mitcad(*body, foot.tube.front(), "tube");
    }
  }
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  for (int i = 1; i < argc; ++i) {
    if (std::strcmp(argv[i], "--probe") == 0) {
      test::probe = true;
    }
  }
  test::guarded("test_foot_across_the_dome_edge", test::test_foot_across_the_dome_edge);
  test::guarded("test_foot_on_the_dome", test::test_foot_on_the_dome);
  if (!test::env("MITCAD_NOZZLE_BODY").empty()) {
    test::guarded("test_reported_body", test::test_reported_body);
  }
  if (test::failures == 0) {
    std::puts("test_fillet_nozzle_dome_junction: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
