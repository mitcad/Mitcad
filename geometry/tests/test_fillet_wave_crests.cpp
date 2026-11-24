// SPDX-License-Identifier: MIT
// A wave-shaped band rounded along its top edge (mitcad#2228): the band
// between y = 12 + 8 sin(2 pi (x + 30) / 30) and the same curve 24 lower,
// x from -30 to 30 (two waves), 10 high. Its walls bend at the crests with
// a radius of 2.85 mm: a rounding of the top edge wider than that folds its
// contact line on the top face over itself at each of the edge's two
// crests. The kernel walks the twisted stretch across each fold as a piece
// of its own between untwisted ones and fills it between them, cut where
// their contact lines cross (OCCT port patch 2228).
//
// Through MitCad's fillet: at 2.5 (below the crests' radius) the volume
// the corner's section (1 - pi / 4) r^2 swept along the edge takes, exact
// for a curve that turns as much one way as the other. At 3, 5 and 8.5
// (the largest radius another program builds on the reported model, below
// the 10 mm wall) one valid closed solid, the rounding's faces named after
// the edge, the volume falling as the radius grows, by more than half the
// corner's section along the edge and less than its square r^2. Both top
// edges together at 3 and 5 (the walls lie 14.2 mm apart at their
// nearest): one valid solid each.
//
// `test_fillet_wave_crests --probe` prints, for each radius, what OCCT's
// own fillet of the upper edge gives (done, failure, each contour's status
// and computed surfaces) with its walk traced on the standard error
// (MITCAD_TRACE_FILLET_WALK, port patch 2141; the guides split at the
// folds, patch 2228), and for a result its faces and the checker's report;
// `--probe-reported` the same, and MitCad's fillet too, on the reported
// model (the STEP file in wavy-fillet/ under MITCAD_MITCAD_REPORTED).

#include <cctype>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <sstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Wire.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <TopoDS_Wire.hxx>
#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_HArray1.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/io/step.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kHeight = 10.0;
constexpr double kBand = 14400.0; // 24 x 60 x 10

// The volume as MitCad reports it (the analysis library's integral): OCCT's
// default rule with an accuracy gave the band 15382.3 instead of 14400,
// 7 % off on these B-spline walls.
double volume_of(const TopoDS_Shape& shape) { return mitcad::geometry::volume(Shape(shape)); }

// OCCT's default rule with an accuracy, for the probe's comparison.
double occt_volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

// The wave y = offset + 8 sin(2 pi (x + 30) / 30) from x = -30 to 30 as a
// sketch's B-spline segment (interpolated through 121 points), from x = -30
// to the other end (from x = 30 where `reversed`).
Segment wave(const std::string& name, double offset, bool reversed) {
  constexpr int kPoints = 121;
  occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, kPoints);
  for (int i = 0; i < kPoints; ++i) {
    const double x = -30.0 + 60.0 * i / (kPoints - 1);
    const double y = offset + 8.0 * std::sin(2.0 * kPi * (x + 30.0) / 30.0);
    points->SetValue(i + 1, reversed ? gp_Pnt2d(-x, offset + 8.0 * std::sin(2.0 * kPi * (-x + 30.0) / 30.0))
                                     : gp_Pnt2d(x, y));
  }
  Geom2dAPI_Interpolate interpolate(points, false, 1e-9);
  interpolate.Perform();
  const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
  Segment s;
  s.name = name;
  s.kind = SegmentKind::BSpline;
  s.degree = curve->Degree();
  for (int i = 1; i <= curve->NbPoles(); ++i) {
    s.poles.push_back(curve->Pole(i));
  }
  for (int i = 1; i <= curve->NbKnots(); ++i) {
    s.knots.push_back(curve->Knot(i));
    s.multiplicities.push_back(curve->Multiplicity(i));
  }
  s.start = s.poles.front();
  s.end = s.poles.back();
  return s;
}

Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

// The band, extruded 10 high (Extrude1 F2).
ShapePtr band() {
  Region region;
  region.name = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
  region.loops.push_back(Loop{{wave("c1[c4,c2]", 12.0, false), line("c2[c1,c3]", 30, 12, 30, -12),
                               wave("c3[c2,c4]", -12.0, true), line("c4[c3,c1]", -30, -12, -30, 12)}});
  ExtrudeSpec spec;
  spec.feature = "F2";
  spec.regions = {region};
  spec.start = 0.0;
  spec.end = kHeight;
  return extrude(spec);
}

// The top edge along the wave at y around `y` (12 or -12), its name and
// length; empty where there is none.
std::string top_wave_edge(const Shape& body, double y, double& length) {
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (curve.GetType() == GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    if (std::abs(a.Z() - kHeight) < 1e-6 && std::abs(b.Z() - kHeight) < 1e-6 && std::abs(a.Y() - y) < 1e-6 &&
        std::abs(b.Y() - y) < 1e-6) {
      length = GCPnts_AbscissaPoint::Length(curve);
      return body.edge_name(e);
    }
  }
  return {};
}

struct Rounded {
  double volume = 0.0;
  bool ok = false;
};

Rounded rounded(const Shape& body, const std::vector<std::string>& edges, double radius) {
  FilletSet set;
  set.edges = edges;
  set.radius = radius;
  Rounded out;
  ShapePtr result;
  try {
    result = fillet("F3", body, {set});
  } catch (const std::exception& failure) {
    std::printf("  r %g, %zu edges: failed: %s\n", radius, edges.size(), failure.what());
    CHECK(result != nullptr);
    return out;
  }
  CHECK(result != nullptr);
  if (!result) {
    return out;
  }
  out.volume = volume_of(result->occt());
  std::printf("  r %g, %zu edges: volume %.6f, %d faces, %zu notes\n", radius, edges.size(), out.volume,
              count_of(result->occt(), TopAbs_FACE), result->notes().size());
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(count_of(result->occt(), TopAbs_SOLID) == 1);
  int named = 0;
  for (int f = 0; f < result->face_count(); ++f) {
    for (const std::string& name : result->face_names(f)) {
      named += name.rfind("F3:fillet(", 0) == 0 ? 1 : 0;
    }
  }
  CHECK(named >= static_cast<int>(edges.size()));
  out.ok = true;
  return out;
}

void test_one_edge() {
  const ShapePtr body = band();
  const double before = volume_of(body->occt());
  CHECK_NEAR_TOL(before, kBand, 1e-6);
  double length = 0.0;
  const std::string edge = top_wave_edge(*body, 12.0, length);
  CHECK(!edge.empty());
  if (edge.empty()) {
    return;
  }
  const double corner = 1.0 - kPi / 4.0;
  double previous = before;
  for (const double radius : {2.5, 3.0, 5.0, 8.5}) {
    const Rounded r = rounded(*body, {edge}, radius);
    if (!r.ok) {
      continue;
    }
    const double removed = before - r.volume;
    if (radius == 2.5) {
      CHECK_NEAR_TOL(removed, corner * radius * radius * length, 2e-3);
    } else {
      CHECK(removed > corner * radius * radius * length * 0.5);
      CHECK(removed < radius * radius * length);
    }
    CHECK(r.volume < previous);
    previous = r.volume;
  }
}

void test_both_edges() {
  const ShapePtr body = band();
  double length = 0.0;
  const std::string a = top_wave_edge(*body, 12.0, length);
  const std::string b = top_wave_edge(*body, -12.0, length);
  CHECK(!a.empty() && !b.empty());
  if (a.empty() || b.empty()) {
    return;
  }
  for (const double radius : {3.0, 5.0}) {
    rounded(*body, {a, b}, radius);
  }
}

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

// The statuses of a checker's result other than "no error", on the shape
// itself and in each context (an edge in a face), as text; empty for none.
std::string statuses_of(const occ::handle<BRepCheck_Result>& result,
                        const NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>& faces) {
  std::ostringstream text;
  for (const BRepCheck_Status status : result->Status()) {
    if (status != BRepCheck_NoError) {
      BRepCheck::Print(status, text);
      text << ' ';
    }
  }
  for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
    for (const BRepCheck_Status status : result->StatusOnShape()) {
      if (status != BRepCheck_NoError) {
        text << "[in " << TopAbs::ShapeTypeToString(result->ContextualShape().ShapeType());
        const int face = faces.FindIndex(result->ContextualShape());
        if (face > 0) {
          text << ' ' << face;
        }
        text << "] ";
        BRepCheck::Print(status, text);
        text << ' ';
      }
    }
  }
  std::string out = text.str();
  // (BRepCheck::Print ends each status with a new line.)
  for (char& c : out) {
    if (c == '\n') {
      c = ' ';
    }
  }
  return out;
}

// The result's faces (surface type, orientation, area, centre, largest
// edge tolerance), then the checker's report: each shell, face, wire,
// edge and vertex with a status other than "no error".
void probe_faces_and_checker(const TopoDS_Shape& made) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(made, TopAbs_FACE, faces);
  for (int f = 1; f <= faces.Extent(); ++f) {
    const TopoDS_Face& face = TopoDS::Face(faces(f));
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    double tolerance = 0.0;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
    }
    const gp_Pnt c = props.CentreOfMass();
    std::printf("   face %d: %s, %s, area %.6f, centre (%.4f %.4f %.4f), edge tolerance up to %.3g\n", f,
                BRep_Tool::Surface(face).IsNull() ? "no surface" : BRep_Tool::Surface(face)->DynamicType()->Name(),
                face.Orientation() == TopAbs_FORWARD ? "forward" : "reversed", props.Mass(), c.X(), c.Y(), c.Z(),
                tolerance);
  }
  BRepCheck_Analyzer checker(made);
  if (checker.IsValid()) {
    return;
  }
  // A face's wire that crosses itself: its edges in order, then the two
  // edges the checker finds crossing and where their curves on the face
  // cross (in the face's parameters and in space).
  const auto describe_edge = [&](const TopoDS_Edge& edge, const TopoDS_Face& face) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(edge, a, b);
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    const BRepAdaptor_Curve curve(edge);
    std::printf("%s %s, tolerance %.3g, length %.6f",
                pcurve.IsNull() ? "no p-curve" : pcurve->DynamicType()->Name(),
                edge.Orientation() == TopAbs_FORWARD ? "forward" : "reversed", BRep_Tool::Tolerance(edge),
                GCPnts_AbscissaPoint::Length(curve));
    if (!a.IsNull() && !b.IsNull()) {
      const gp_Pnt pa = BRep_Tool::Pnt(a);
      const gp_Pnt pb = BRep_Tool::Pnt(b);
      std::printf(", from (%.4f %.4f %.4f) to (%.4f %.4f %.4f)", pa.X(), pa.Y(), pa.Z(), pb.X(), pb.Y(), pb.Z());
    }
  };
  for (int f = 1; f <= faces.Extent(); ++f) {
    const TopoDS_Face& face = TopoDS::Face(faces(f));
    for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
      BRepCheck_Wire check(TopoDS::Wire(w.Current()));
      TopoDS_Edge e1;
      TopoDS_Edge e2;
      if (check.SelfIntersect(face, e1, e2, false) != BRepCheck_SelfIntersectingWire) {
        continue;
      }
      std::printf("   face %d's wire crosses itself; its edges:\n", f);
      int k = 0;
      for (TopExp_Explorer e(w.Current(), TopAbs_EDGE); e.More(); e.Next()) {
        std::printf("    edge %d: ", ++k);
        describe_edge(TopoDS::Edge(e.Current()), face);
        const char* mark = e.Current().IsSame(e1) ? " <- crossing (1)"
                           : e.Current().IsSame(e2) ? " <- crossing (2)"
                                                    : "";
        std::printf("%s\n", mark);
      }
      if (e1.IsNull() || e2.IsNull()) {
        continue;
      }
      double f1 = 0.0;
      double l1 = 0.0;
      double f2 = 0.0;
      double l2 = 0.0;
      const occ::handle<Geom2d_Curve> c1 = BRep_Tool::CurveOnSurface(e1, face, f1, l1);
      const occ::handle<Geom2d_Curve> c2 = BRep_Tool::CurveOnSurface(e2, face, f2, l2);
      if (c1.IsNull() || c2.IsNull()) {
        continue;
      }
      const occ::handle<Geom2d_Curve> t1 = new Geom2d_TrimmedCurve(c1, f1, l1);
      const occ::handle<Geom2d_Curve> t2 = new Geom2d_TrimmedCurve(c2, f2, l2);
      Geom2dAPI_InterCurveCurve crossing(t1, t2, 1.0e-9);
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
      for (int i = 1; i <= crossing.NbPoints(); ++i) {
        const gp_Pnt2d uv = crossing.Point(i);
        const gp_Pnt p = surface->Value(uv.X(), uv.Y());
        std::printf("    the crossing edges meet at uv (%.6f %.6f), (%.6f %.6f %.6f)\n", uv.X(), uv.Y(), p.X(), p.Y(),
                    p.Z());
      }
      std::printf("    (%d crossing points, %d overlapping stretches)\n", crossing.NbPoints(), crossing.NbSegments());
    }
  }
  for (const TopAbs_ShapeEnum kind : {TopAbs_SHELL, TopAbs_FACE, TopAbs_WIRE, TopAbs_EDGE, TopAbs_VERTEX}) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
    TopExp::MapShapes(made, kind, all);
    for (int i = 1; i <= all.Extent(); ++i) {
      const occ::handle<BRepCheck_Result>& result = checker.Result(all(i));
      if (result.IsNull()) {
        continue;
      }
      const std::string said = statuses_of(result, faces);
      if (said.empty()) {
        continue;
      }
      std::printf("   checker: %s %d: %s", TopAbs::ShapeTypeToString(kind), i, said.c_str());
      if (kind == TopAbs_EDGE) {
        const TopoDS_Edge& edge = TopoDS::Edge(all(i));
        TopoDS_Vertex a;
        TopoDS_Vertex b;
        TopExp::Vertices(edge, a, b);
        if (!a.IsNull() && !b.IsNull()) {
          const gp_Pnt pa = BRep_Tool::Pnt(a);
          const gp_Pnt pb = BRep_Tool::Pnt(b);
          std::printf("from (%.4f %.4f %.4f) to (%.4f %.4f %.4f), ", pa.X(), pa.Y(), pa.Z(), pb.X(), pb.Y(),
                      pb.Z());
        }
        std::printf("tolerance %.3g, faces", BRep_Tool::Tolerance(edge));
        for (int f = 1; f <= faces.Extent(); ++f) {
          for (TopExp_Explorer e(faces(f), TopAbs_EDGE); e.More(); e.Next()) {
            if (e.Current().IsSame(edge)) {
              std::printf(" %d", f);
              break;
            }
          }
        }
      } else if (kind == TopAbs_VERTEX) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(all(i)));
        std::printf("at (%.4f %.4f %.4f), tolerance %.3g", p.X(), p.Y(), p.Z(),
                    BRep_Tool::Tolerance(TopoDS::Vertex(all(i))));
      } else if (kind == TopAbs_FACE) {
        std::printf("(face %d above)", faces.FindIndex(all(i)));
      }
      std::printf("\n");
    }
  }
}

// OCCT's own fillet of the body's upper top wave edge at each radius, its
// walk traced, the result's faces and the checker's report; then, with
// `mitcad`, MitCad's fillet of the same edge (its result or its error).
int probe_body(const ShapePtr& body, const std::vector<double>& radii, bool mitcad) {
  double length = 0.0;
  const std::string name = top_wave_edge(*body, 12.0, length);
  const std::vector<int> found = name.empty() ? std::vector<int>() : body->find_edges(name);
  if (found.size() != 1) {
    std::printf("probe: no upper top edge\n");
    return 1;
  }
  const TopoDS_Edge& edge = body->edge(found.front());
  const double before = volume_of(body->occt());
  std::printf("probe: upper top edge %s, %.6f long; the body's volume %.6f (OCCT's default rule with an "
              "accuracy: %.6f), %d faces\n",
              name.c_str(), length, before, occt_volume_of(body->occt()), body->face_count());
  for (const double r : radii) {
    std::printf(" kernel fillet %g:\n", r);
    std::fflush(stdout);
    set_environment("MITCAD_TRACE_FILLET_WALK", "1");
    BRepFilletAPI_MakeFillet maker(body->occt());
    maker.Add(r, edge);
    try {
      OCC_CATCH_SIGNALS
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::printf("  raised '%s'\n", failure.what());
    }
    std::fflush(stderr);
    set_environment("MITCAD_TRACE_FILLET_WALK", "");
    std::printf("  done %d, failure %d, failed contour %d, contours %d, faulty contours %d\n", int(maker.IsDone()),
                int(maker.Failure()), maker.FailedContour(), maker.NbContours(), maker.NbFaultyContours());
    for (int ic = 1; ic <= maker.NbContours(); ++ic) {
      std::printf("   contour %d: status %d, %d computed surfaces\n", ic, int(maker.StripeStatus(ic)),
                  maker.NbComputedSurfaces(ic));
      for (int is = 1; is <= maker.NbComputedSurfaces(ic); ++is) {
        const occ::handle<Geom_Surface> surface = maker.ComputedSurface(ic, is);
        std::printf("    surface %d: %s\n", is, surface.IsNull() ? "none" : surface->DynamicType()->Name());
      }
    }
    if (maker.IsDone()) {
      const TopoDS_Shape made = maker.Shape();
      std::printf("  volume %.6f (removed %.6f, the corner's section along the edge %.6f), %d faces, valid %d\n",
                  volume_of(made), before - volume_of(made), (1.0 - kPi / 4.0) * r * r * length,
                  count_of(made, TopAbs_FACE), int(BRepCheck_Analyzer(made).IsValid()));
      probe_faces_and_checker(made);
    }
    std::fflush(stdout);
    if (mitcad) {
      std::printf(" MitCad's fillet %g:\n", r);
      std::fflush(stdout);
      set_environment("MITCAD_TRACE_FILLET_WALK", "1");
      rounded(*body, {name}, r);
      std::fflush(stderr);
      set_environment("MITCAD_TRACE_FILLET_WALK", "");
      std::fflush(stdout);
    }
  }
  return 0;
}

// --probe: the band.
int probe() { return probe_body(band(), {2.5, 2.9, 3.0, 5.0, 8.5}, false); }

// --probe-reported: the reported model, the STEP file in `wavy-fillet/`
// under MITCAD_MITCAD_REPORTED (the only one there), imported as MitCad
// imports a STEP file's body; OCCT's and MitCad's fillet of its upper top
// wave edge at 2.5, 3, 5, 8.5 and 8.7 (the .f3d fillet's radius).
int probe_reported() {
#ifdef _MSC_VER
  char* reported_value = nullptr;
  std::size_t reported_length = 0;
  std::string reported_text;
  if (_dupenv_s(&reported_value, &reported_length, "MITCAD_MITCAD_REPORTED") == 0 && reported_value != nullptr) {
    reported_text = reported_value;
    std::free(reported_value);
  }
#else
  const char* reported_env = std::getenv("MITCAD_MITCAD_REPORTED");
  const std::string reported_text = reported_env != nullptr ? std::string(reported_env) : std::string();
#endif
  const char* reported = reported_text.c_str();
  if (reported[0] == '\0') {
    std::printf("probe: MITCAD_MITCAD_REPORTED is not set\n");
    return 1;
  }
  const std::filesystem::path folder = std::filesystem::path(reported) / "wavy-fillet";
  std::filesystem::path step;
  if (std::filesystem::is_directory(folder)) {
    for (const auto& entry : std::filesystem::directory_iterator(folder)) {
      std::string extension = entry.path().extension().string();
      for (char& c : extension) {
        c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
      }
      if (extension == ".step" || extension == ".stp") {
        step = entry.path();
      }
    }
  }
  if (step.empty()) {
    std::printf("probe: no STEP file in MITCAD_MITCAD_REPORTED/wavy-fillet\n");
    return 1;
  }
  const std::vector<mitcad::io::Body> read = mitcad::io::read_step(step.string());
  if (read.size() != 1) {
    std::printf("probe: the STEP file holds %zu bodies, not 1\n", read.size());
    return 1;
  }
  return probe_body(mitcad::geometry::import_body(read.front().shape, "F1", 0), {2.5, 3.0, 5.0, 8.5, 8.7}, true);
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  if (argc > 1 && std::strcmp(argv[1], "--probe") == 0) {
    return test::probe();
  }
  if (argc > 1 && std::strcmp(argv[1], "--probe-reported") == 0) {
    return test::probe_reported();
  }
  test::guarded("test_one_edge", test::test_one_edge);
  test::guarded("test_both_edges", test::test_both_edges);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("all tests passed\n");
  return 0;
}
