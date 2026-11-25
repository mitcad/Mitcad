// SPDX-License-Identifier: MIT
// The top edges of a block rounded where an oval pocket in the top lies
// closer to each edge than the rounding is wide (GitHub #635, mitcad#2365):
// the rounding's contact on the top runs past the pocket's rim, so the
// rounding goes through the pocket's wall beside the edge, with a small bump
// where the pocket's round ends turn away. The reporter's two edges rounded
// differently: on one the rounding dipped into the pocket's rim as
// expected, on the other it ran straight past and the bump appeared on the
// neighbouring edge, with no error, whether both edges were in one set or
// in two. The reporter's design is not here; this is a block made the same
// way: 60 x 30 x 12, a pocket 20 x 6 (round ends of radius 3) and 6 deep
// beside each long top edge, 1.5 from it, off the edge's middle, and the
// edges rounded with 2.5 and 4. The pockets lie
//   mirrored: mirror images about the block's middle plane y = 15, so that
//             the two edges run in opposite directions round the top face
//             with each pocket at the same end;
//   turned:   the block turned half a turn about its upright middle line,
//             so that each pocket lies at its edge's start.
// MitCad's fillet must round each edge the way it rounds the other (the
// material taken the same, point by point, on the other side's image), or
// fail in words for both; never one way here and another there. Rounded
// together, in one set and in two, the result is each edge's rounding, the
// two sides each other's images.
//
// This block does not show the reported fault: both edges round alike in
// both layouts and at both radii, and the kernel's fillet alone builds the
// same bodies. What in the reporter's design makes the far edge differ is
// not known without it.
//
// Printed always: each result's validity, volume, notes and infos and its
// faces by surface type. With `--probe` also each result's faces (surface,
// area, centre) and the kernel's fillet alone of each edge (its volume and
// faces), to tell whether a difference is the kernel's or MitCad's path past
// the narrow strip of the top.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <exception>
#include <map>
#include <string>
#include <tuple>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

using mitcad::geometry::FilletSet;
using mitcad::geometry::import_body;
using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;

bool probe = false;

constexpr double kLength = 60.0; // the block along x
constexpr double kWidth  = 30.0; // along y; the rounded edges at y = 0 and y = kWidth
constexpr double kHeight = 12.0; // along z; the top at z = kHeight
constexpr double kGap    = 1.5;  // from the edge to the pocket's straight side
constexpr double kEnd    = 3.0;  // the pocket's half width, the radius of its ends
constexpr double kDepth  = 6.0;
constexpr double kStart  = 12.0; // the centres of the pocket's round ends along the edge
constexpr double kStop   = 26.0;

enum class Layout { Mirrored, Turned };

const char* layout_name(Layout layout) { return layout == Layout::Mirrored ? "mirrored" : "turned"; }

// The point on the other side that a point on the side of the edge y = 0
// maps to.
gp_Pnt image(Layout layout, const gp_Pnt& p) {
  if (layout == Layout::Mirrored) {
    return gp_Pnt(p.X(), kWidth - p.Y(), p.Z());
  }
  return gp_Pnt(kLength - p.X(), kWidth - p.Y(), p.Z());
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int solids_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_SOLID); e.More(); e.Next()) {
    ++n;
  }
  return n;
}

bool shells_closed(const TopoDS_Shape& shape) {
  int shells = 0;
  bool closed = true;
  for (TopExp_Explorer e(shape, TopAbs_SHELL); e.More(); e.Next()) {
    ++shells;
    closed = closed && BRep_Tool::IsClosed(e.Current());
  }
  return shells == 1 && closed;
}

// A pocket of the oval outline about the line y = yc from x = a to x = b
// (round ends of radius kEnd), from the depth up past the top.
TopoDS_Shape pocket(double a, double b, double yc) {
  const double z = kHeight - kDepth;
  const gp_Pnt p1(a, yc - kEnd, z), p2(b, yc - kEnd, z), p3(b, yc + kEnd, z), p4(a, yc + kEnd, z);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeSegment(p1, p2).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p2, gp_Pnt(b + kEnd, yc, z), p3).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeSegment(p3, p4).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p4, gp_Pnt(a - kEnd, yc, z), p1).Value()).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kDepth + 1.0)).Shape();
}

// The block with its two pockets, one beside each long top edge.
ShapePtr block(Layout layout) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(kLength, kWidth, kHeight)).Shape();
  const double near_y = kGap + kEnd;
  const TopoDS_Shape first = pocket(kStart, kStop, near_y);
  const TopoDS_Shape second = layout == Layout::Mirrored ? pocket(kStart, kStop, kWidth - near_y)
                                                         : pocket(kLength - kStop, kLength - kStart, kWidth - near_y);
  const TopoDS_Shape once = BRepAlgoAPI_Cut(box, first).Shape();
  return import_body(BRepAlgoAPI_Cut(once, second).Shape(), "F1", 0);
}

// The index of the body's top edge along x at y, from x = 0 to kLength; -1
// when there is none.
int top_edge(const Shape& body, double y) {
  for (int i = 0; i < body.edge_count(); ++i) {
    BRepAdaptor_Curve curve(body.edge(i));
    if (curve.GetType() != GeomAbs_Line) {
      continue;
    }
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    if (std::abs(a.Y() - y) < 1e-7 && std::abs(b.Y() - y) < 1e-7 && std::abs(a.Z() - kHeight) < 1e-7 &&
        std::abs(b.Z() - kHeight) < 1e-7 && std::abs(std::abs(a.X() - b.X()) - kLength) < 1e-7) {
      return i;
    }
  }
  return -1;
}

const char* surface_name(GeomAbs_SurfaceType type) {
  switch (type) {
  case GeomAbs_Plane: return "plane";
  case GeomAbs_Cylinder: return "cylinder";
  case GeomAbs_Cone: return "cone";
  case GeomAbs_Sphere: return "sphere";
  case GeomAbs_Torus: return "torus";
  case GeomAbs_BSplineSurface: return "bspline";
  default: return "other";
  }
}

// The faces' area by surface type.
std::map<std::string, double> areas_by_type(const TopoDS_Shape& shape) {
  std::map<std::string, double> areas;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(e.Current(), props);
    areas[surface_name(BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType())] += props.Mass();
  }
  return areas;
}

void print_faces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(e.Current(), props);
    const gp_Pnt c = props.CentreOfMass();
    std::printf("      %-8s area %10.6f centre (%.4f, %.4f, %.4f)\n",
                surface_name(BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType()), props.Mass(), c.X(), c.Y(),
                c.Z());
  }
}

// A rounding's outcome: the body, or the error's words.
struct Outcome {
  ShapePtr body;
  std::string error;
};

Outcome round_edges(const Shape& body, const std::vector<std::vector<std::string>>& sets, double radius,
                    const char* what) {
  Outcome out;
  try {
    std::vector<FilletSet> fillet_sets;
    for (const std::vector<std::string>& edges : sets) {
      FilletSet set;
      set.edges = edges;
      set.radius = radius;
      fillet_sets.push_back(set);
    }
    out.body = mitcad::geometry::fillet("F2", body, fillet_sets);
  } catch (const std::exception& failure) {
    out.error = failure.what();
  }
  if (!out.body) {
    std::printf("    %s: failed: %s\n", what, out.error.c_str());
    return out;
  }
  const TopoDS_Shape& shape = out.body->occt();
  std::printf("    %s: %s, %d solid(s), %s, volume %.6f, %zu note(s), %zu info(s)\n", what,
              BRepCheck_Analyzer(shape).IsValid() ? "valid" : "invalid", solids_of(shape),
              shells_closed(shape) ? "closed" : "open", volume_of(shape), out.body->notes().size(),
              out.body->infos().size());
  for (const std::string& note : out.body->notes()) {
    std::printf("      note: %s\n", note.c_str());
  }
  for (const auto& [type, area] : areas_by_type(shape)) {
    std::printf("      %-8s %.6f\n", type.c_str(), area);
  }
  if (probe) {
    print_faces(shape);
  }
  return out;
}

// One valid closed solid with no note.
void check_sound(const Outcome& outcome) {
  if (!outcome.body) {
    return;
  }
  const TopoDS_Shape& shape = outcome.body->occt();
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  CHECK(solids_of(shape) == 1);
  CHECK(shells_closed(shape));
  CHECK(outcome.body->notes().empty());
}

// The points of the box from `low` to `high` on a grid of about `step`,
// shifted off round numbers so that none lies on a face.
std::vector<gp_Pnt> grid(const gp_Pnt& low, const gp_Pnt& high, double step) {
  std::vector<gp_Pnt> points;
  const int nx = static_cast<int>(std::ceil((high.X() - low.X()) / step));
  const int ny = static_cast<int>(std::ceil((high.Y() - low.Y()) / step));
  const int nz = static_cast<int>(std::ceil((high.Z() - low.Z()) / step));
  for (int i = 0; i < nx; ++i) {
    for (int j = 0; j < ny; ++j) {
      for (int k = 0; k < nz; ++k) {
        points.emplace_back(low.X() + (i + 0.4137) * (high.X() - low.X()) / nx,
                            low.Y() + (j + 0.3719) * (high.Y() - low.Y()) / ny,
                            low.Z() + (k + 0.5531) * (high.Z() - low.Z()) / nz);
      }
    }
  }
  return points;
}

// The points near the edge y = 0: the whole edge's length, the pocket and a
// band past it, 0.5 apart (a misplaced bump of half a cubic millimetre
// holds about four of them).
const std::vector<gp_Pnt>& near_points() {
  static const std::vector<gp_Pnt> points = grid(gp_Pnt(0, 0, kHeight - 9.0), gp_Pnt(kLength, 10.0, kHeight), 0.5);
  return points;
}

bool inside(BRepClass3d_SolidClassifier& classifier, const gp_Pnt& p) {
  classifier.Perform(p, 1e-7);
  return classifier.State() == TopAbs_IN;
}

// The point's state, ON within 1e-3 of the boundary: walked blends on the
// two sides may be approximated a little differently.
TopAbs_State state(BRepClass3d_SolidClassifier& classifier, const gp_Pnt& p) {
  classifier.Perform(p, 1e-3);
  return classifier.State();
}

const char* state_name(TopAbs_State s) { return s == TopAbs_IN ? "in" : s == TopAbs_OUT ? "out" : "on"; }

// Every point near the edge y = 0 in `a` is in `b` at its image and the
// other way round (those on either boundary aside); the first few that
// differ are printed. The number that differ.
int differing_points(Layout layout, const TopoDS_Shape& a, const TopoDS_Shape& b, const char* what) {
  BRepClass3d_SolidClassifier in_a(a);
  BRepClass3d_SolidClassifier in_b(b);
  int differ = 0;
  for (const gp_Pnt& p : near_points()) {
    const TopAbs_State at_a = state(in_a, p);
    if (at_a == TopAbs_ON) {
      continue;
    }
    const gp_Pnt q = image(layout, p);
    const TopAbs_State at_b = state(in_b, q);
    if (at_b != TopAbs_ON && at_a != at_b) {
      if (differ < 12) {
        std::printf("      %s: (%.3f, %.3f, %.3f) %s, its image (%.3f, %.3f, %.3f) %s\n", what, p.X(), p.Y(), p.Z(),
                    state_name(at_a), q.X(), q.Y(), q.Z(), state_name(at_b));
      }
      ++differ;
    }
  }
  if (differ > 0) {
    std::printf("      %s: %d of %zu points differ\n", what, differ, near_points().size());
  }
  return differ;
}

// What both sides of every rounding share: the corner far from the pocket
// is rounded off, the pocket stays open, the material well below the edge
// stays.
void check_rounded_near(Layout layout, const TopoDS_Shape& shape, bool first_side, double radius) {
  BRepClass3d_SolidClassifier classifier(shape);
  const auto at = [&](double x, double y, double z) {
    const gp_Pnt p(x, y, z);
    return first_side ? p : image(layout, p);
  };
  const double corner = 0.15 * radius; // off the edge along both faces: outside the rounding
  // The corner beyond the pocket's far end and across the pocket's span.
  CHECK(!inside(classifier, at(45.0, corner, kHeight - corner)));
  CHECK(!inside(classifier, at(0.5 * (kStart + kStop), corner, kHeight - corner)));
  // The pocket's middle.
  CHECK(!inside(classifier, at(0.5 * (kStart + kStop), kGap + kEnd, kHeight - 0.5 * kDepth)));
  // Below the rounding's contact on the side, and past its contact on the
  // top beyond the pocket.
  CHECK(inside(classifier, at(0.5 * (kStart + kStop), 0.5 * kGap, kHeight - radius - 1.0)));
  CHECK(inside(classifier, at(45.0, radius + 1.0, kHeight - 0.5)));
}

// The kernel's own fillet of one edge alone (`--probe`).
void probe_kernel(const Shape& body, int edge, double radius, const char* what) {
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body.occt());
    fillet.Add(radius, body.edge(edge));
    fillet.Build();
    if (!fillet.IsDone()) {
      std::printf("    the kernel's fillet alone, %s: not done\n", what);
      return;
    }
    const TopoDS_Shape& shape = fillet.Shape();
    std::printf("    the kernel's fillet alone, %s: %s, %d solid(s), volume %.6f\n", what,
                BRepCheck_Analyzer(shape).IsValid() ? "valid" : "invalid", solids_of(shape), volume_of(shape));
    print_faces(shape);
  } catch (const Standard_Failure& failure) {
    std::printf("    the kernel's fillet alone, %s: threw %s\n", what, failure.what());
  }
}

void round_pocket_rims(Layout layout, double radius) {
  std::printf("%s, radius %.2f\n", layout_name(layout), radius);
  const ShapePtr body = block(layout);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  const double before = volume_of(body->occt());
  const int first_index = top_edge(*body, 0.0);
  const int second_index = top_edge(*body, kWidth);
  CHECK(first_index >= 0 && second_index >= 0);
  if (first_index < 0 || second_index < 0) {
    return;
  }
  const std::string first = body->edge_name(first_index);
  const std::string second = body->edge_name(second_index);
  if (probe) {
    probe_kernel(*body, first_index, radius, "the edge y = 0");
    probe_kernel(*body, second_index, radius, "the edge y = 30");
  }

  // Each edge alone: both build or both fail in words.
  const Outcome near_one = round_edges(*body, {{first}}, radius, "the edge y = 0 alone");
  const Outcome far_one = round_edges(*body, {{second}}, radius, "the edge y = 30 alone");
  CHECK(static_cast<bool>(near_one.body) == static_cast<bool>(far_one.body));
  CHECK(near_one.body || !near_one.error.empty());
  CHECK(far_one.body || !far_one.error.empty());
  check_sound(near_one);
  check_sound(far_one);
  // The body is the input as it was.
  CHECK(std::abs(volume_of(body->occt()) - before) <= 1e-9 * before);
  if (near_one.body && far_one.body) {
    const double near_taken = before - volume_of(near_one.body->occt());
    const double far_taken = before - volume_of(far_one.body->occt());
    std::printf("    taken: by the edge y = 0 %.6f, by the edge y = 30 %.6f\n", near_taken, far_taken);
    CHECK(near_taken > 0.0);
    CHECK_NEAR_TOL(far_taken, near_taken, 1e-5);
    // The rounding of each edge goes past its pocket's rim the same way, and
    // leaves the other edge sharp.
    CHECK(differing_points(layout, near_one.body->occt(), far_one.body->occt(), "alone") == 0);
    check_rounded_near(layout, near_one.body->occt(), true, radius);
    check_rounded_near(layout, far_one.body->occt(), false, radius);
    const std::map<std::string, double> near_areas = areas_by_type(near_one.body->occt());
    const std::map<std::string, double> far_areas = areas_by_type(far_one.body->occt());
    CHECK(near_areas.size() == far_areas.size());
    for (const auto& [type, area] : near_areas) {
      const auto it = far_areas.find(type);
      CHECK(it != far_areas.end());
      if (it != far_areas.end()) {
        CHECK_NEAR_TOL(it->second, area, 1e-5);
      }
    }
    // The rounding's faces are named after their edge and lie on its side.
    for (const auto& [outcome, edge, low_side] :
         {std::make_tuple(&near_one, first, true), std::make_tuple(&far_one, second, false)}) {
      const Shape& rounded = *outcome->body;
      int named = 0;
      for (int i = 0; i < rounded.face_count(); ++i) {
        const auto& names = rounded.face_names(i);
        if (std::find(names.begin(), names.end(), "F2:fillet(" + edge + ")") == names.end()) {
          continue;
        }
        ++named;
        Bnd_Box box;
        BRepBndLib::Add(rounded.face(i), box);
        double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
        box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
        CHECK(low_side ? ymax < 0.5 * kWidth : ymin > 0.5 * kWidth);
        CHECK(zmin > kHeight - kDepth - 1e-6);
      }
      CHECK(named >= 1);
    }
  }

  // Both edges together, in one set and in two: the same body, each side
  // the image of the other, and the material taken that of each edge alone.
  const Outcome one_set = round_edges(*body, {{first, second}}, radius, "both edges in one set");
  const Outcome two_sets = round_edges(*body, {{first}, {second}}, radius, "both edges in two sets");
  CHECK(static_cast<bool>(one_set.body) == static_cast<bool>(two_sets.body));
  CHECK(one_set.body || !one_set.error.empty());
  CHECK(two_sets.body || !two_sets.error.empty());
  check_sound(one_set);
  check_sound(two_sets);
  if (one_set.body && two_sets.body) {
    CHECK_NEAR_TOL(volume_of(two_sets.body->occt()), volume_of(one_set.body->occt()), 1e-7);
  }
  for (const Outcome* both : {&one_set, &two_sets}) {
    if (!both->body) {
      continue;
    }
    CHECK(differing_points(layout, both->body->occt(), both->body->occt(), "both") == 0);
    check_rounded_near(layout, both->body->occt(), true, radius);
    check_rounded_near(layout, both->body->occt(), false, radius);
    if (near_one.body && far_one.body) {
      const double taken = before - volume_of(both->body->occt());
      const double each = (before - volume_of(near_one.body->occt())) + (before - volume_of(far_one.body->occt()));
      CHECK_NEAR_TOL(taken, each, 1e-5);
    }
  }
}

void test_mirrored_pockets_at_2_5() { round_pocket_rims(Layout::Mirrored, 2.5); }
void test_mirrored_pockets_at_4() { round_pocket_rims(Layout::Mirrored, 4.0); }
void test_turned_pockets_at_2_5() { round_pocket_rims(Layout::Turned, 2.5); }
void test_turned_pockets_at_4() { round_pocket_rims(Layout::Turned, 4.0); }

} // namespace
} // namespace test

int main(int argc, char** argv) {
  for (int i = 1; i < argc; ++i) {
    if (std::strcmp(argv[i], "--probe") == 0) {
      test::probe = true;
    }
  }
  test::guarded("test_mirrored_pockets_at_2_5", test::test_mirrored_pockets_at_2_5);
  test::guarded("test_mirrored_pockets_at_4", test::test_mirrored_pockets_at_4);
  test::guarded("test_turned_pockets_at_2_5", test::test_turned_pockets_at_2_5);
  test::guarded("test_turned_pockets_at_4", test::test_turned_pockets_at_4);
  if (test::failures == 0) {
    std::puts("test_fillet_pocket_rims: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
