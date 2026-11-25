// SPDX-License-Identifier: MIT
// A probe of roundings wider than the face beside their edge whose chain
// ends at sharp corners (GitHub mitcad/Mitcad#620, mitcad#2352), for the
// diagnosis: no ctest, run by hand (the command in the ticket). Prints,
// never fails.
//
// Bodies (mm):
// - ridge: a block 30 (x) by 20 (y) by 10 (z) with a ridge along x on top,
//   its foot 1 in from the front and back (y 1 to 19), its slopes up to a
//   flat crest 1 wide at z 22; one extrusion along x, so that the 1 wide
//   strips of the top are faces of their own ending on the end faces. The
//   front top edge (y 0, z 10) rounded: past 1 the ball takes the strip
//   away and touches the ridge's slope (continued down past its foot); the
//   exact removed section (the strip's corner less the ball) is known.
// - spike: the reporter's simpler case: the block with a spike on top, its
//   foot 1 in from all four sides, tapered to a crest 12 above; the top a
//   frame 1 wide round the spike, one face. The front top edge rounded with
//   4 (and smaller), alone and after the block's two front upright edges
//   are rounded with 0.1 (as the other user found to help on the
//   reporter's design).
// - notch: a block whose left side slopes at 45 degrees (x = z - 10), with
//   a U-shaped pocket from the top (z 4 to 10) open to the front face (y 0):
//   its left wall at x 1, its round end (radius 4 about (5, 15)), its right
//   wall at x 9. The rim at z 10 (line, arc, line, tangent) rounded with
//   1.5: beside the left wall the top is 1 wide, its far edge the slope's
//   top. Alone and after the front's two edges of the thin wall (the
//   pocket's left wall's upright edge and the slope's front edge) are
//   rounded with 0.1.
// - cylinder, tilted: the notch with the slope a cylinder of radius 20
//   instead (through the top's left edge at 45 degrees), its axis along y
//   or turned by about 3 degrees so that the top beside the pocket's left
//   wall widens from about 1.2 at the front to about 2 at the pocket's end.
//
// For each: the input; MitCad's fillet (validity, closed, solids, volume,
// notes and infos, or its error) and, for the spike and the notches, the
// section of the input and of the result across the edge near it; the
// narrow-face check (overflow_failure) at the contact; whether
// faces_beyond_grown finds the faces beyond, and its body's validity;
// OCCT's own fillet of the edges on a copy.
//
// test_probe_fillet_rim_narrow_corner [ridge|spike|notch|cylinder|tilted]...

#include <cmath>
#include <cstdio>
#include <exception>
#include <functional>
#include <optional>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Pnt.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "../src/cutouts.hpp"

namespace {

using namespace mitcad::geometry;

constexpr double kPi = 3.14159265358979323846;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return std::abs(props.Mass());
}

int solids_of(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_SOLID); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

bool closed(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1;
}

Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

// A closed polygon's region, its segments c1, c2, ... named in a ring.
Region polygon(const std::vector<gp_Pnt2d>& points) {
  const int n = static_cast<int>(points.size());
  const auto key = [n](int i) { return "c" + std::to_string(((i % n) + n) % n + 1); };
  Region region;
  Loop loop;
  region.name = "r{";
  for (int i = 0; i < n; ++i) {
    const std::string name = key(i) + "[" + key(i - 1) + "," + key(i + 1) + "]";
    const gp_Pnt2d& a = points[static_cast<std::size_t>(i)];
    const gp_Pnt2d& b = points[static_cast<std::size_t>((i + 1) % n)];
    loop.segments.push_back(line(name, a.X(), a.Y(), b.X(), b.Y()));
    region.name += (i == 0 ? "" : ",") + name;
  }
  region.name += "}";
  region.loops.push_back(loop);
  return region;
}

ShapePtr extruded(const std::string& feature, const Frame& frame, const gp_Dir& direction, Region region,
                  double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  spec.frame = frame;
  spec.direction = direction;
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = end;
  return extrude(spec);
}

// The edges of the body whose midpoint `pick` accepts, by name.
std::vector<std::string> edges_where(const Shape& body, const std::function<bool(const gp_Pnt&)>& pick) {
  std::vector<std::string> names;
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (pick(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())))) {
      names.push_back(body.edge_name(e));
    }
  }
  return names;
}

bool near(double a, double b) { return std::abs(a - b) < 1e-6; }

// The ridge: the section in (y, z) along x 0 to 30.
ShapePtr ridge() {
  Frame frame;
  frame.x_axis = gp_Dir(0, 1, 0);
  frame.y_axis = gp_Dir(0, 0, 1);
  return extruded("F2", frame, gp_Dir(1, 0, 0),
                  polygon({gp_Pnt2d(0, 0), gp_Pnt2d(20, 0), gp_Pnt2d(20, 10), gp_Pnt2d(19, 10), gp_Pnt2d(10.5, 22),
                           gp_Pnt2d(9.5, 22), gp_Pnt2d(1, 10), gp_Pnt2d(0, 10)}),
                  30.0);
}

// The section the rounding of the ridge's front top edge by r takes away:
// the strip's corner between the ball's contacts on the front (y 0) and on
// the slope continued down past its foot (1, 10), less the circular segment
// between them.
double ridge_removed(double r) {
  const double dy = 8.5;
  const double dz = 12.0;
  const double len = std::sqrt(dy * dy + dz * dz);
  const double uy = dy / len;
  const double uz = dz / len;
  // The corner V where the slope, continued, meets y = 0.
  const double vz = 10.0 - dz / dy;
  // The angle at V inside the material, between down the front (0, -1) and
  // up the slope (uy, uz).
  const double alpha = std::acos(-uz);
  const double reach = r / std::tan(0.5 * alpha);
  const double c1y = 0.0;
  const double c1z = vz - reach;
  const double c2y = reach * uy;
  const double c2z = vz + reach * uz;
  // The quadrilateral c1, (0, 10), (1, 10), c2 (shoelace).
  const double ys[4] = {c1y, 0.0, 1.0, c2y};
  const double zs[4] = {c1z, 10.0, 10.0, c2z};
  double twice = 0.0;
  for (int i = 0; i < 4; ++i) {
    twice += ys[i] * zs[(i + 1) % 4] - ys[(i + 1) % 4] * zs[i];
  }
  const double theta = kPi - alpha;
  return 0.5 * std::abs(twice) - 0.5 * r * r * (theta - std::sin(theta));
}

// The spike: the block and the tapered spike joined.
ShapePtr spike() {
  const ShapePtr block = extruded("F1", Frame(), gp_Dir(0, 0, 1), rectangle_region(0, 0, 30, 20), 10.0);
  ExtrudeFeatureSpec spec;
  spec.feature = "F2";
  spec.frame.origin = gp_Pnt(0, 0, 10);
  spec.regions = {rectangle_region(1, 1, 28, 18)};
  spec.direction = gp_Dir(0, 0, 1);
  spec.side1.distance = 12.0;
  spec.side1.taper = -std::atan(8.5 / 12.0);
  const ShapePtr tip = extrude_feature(spec);
  return boolean(BooleanOp::Join, {block.get()}, *tip).pieces.at(0).shape;
}

// The notch: the sloped block less the U-shaped pocket.
ShapePtr notch() {
  Frame side;
  side.origin = gp_Pnt(0, 30, 0);
  side.x_axis = gp_Dir(1, 0, 0);
  side.y_axis = gp_Dir(0, 0, 1);
  const ShapePtr block = extruded(
      "F1", side, gp_Dir(0, -1, 0), polygon({gp_Pnt2d(-10, 0), gp_Pnt2d(30, 0), gp_Pnt2d(30, 10), gp_Pnt2d(0, 10)}),
      30.0);
  Region u;
  u.name = "r{k1[k4,k2],k2[k1,k3],k3[k2,k4],k4[k3,k1]}";
  Loop loop;
  loop.segments.push_back(line("k1[k4,k2]", 9, -1, 9, 15));
  Segment arc;
  arc.name = "k2[k1,k3]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(5, 15);
  arc.radius = 4.0;
  arc.start_angle = 0.0;
  arc.end_angle = kPi;
  arc.start = gp_Pnt2d(9, 15);
  arc.end = gp_Pnt2d(1, 15);
  loop.segments.push_back(arc);
  loop.segments.push_back(line("k3[k2,k4]", 1, 15, 1, -1));
  loop.segments.push_back(line("k4[k3,k1]", 1, -1, 9, -1));
  u.loops.push_back(loop);
  Frame floor;
  floor.origin = gp_Pnt(0, 0, 4);
  const ShapePtr pocket = extruded("F3", floor, gp_Dir(0, 0, 1), u, 7.0);
  return boolean(BooleanOp::Cut, {block.get()}, *pocket).pieces.at(0).shape;
}

void print_input(const char* label, const Shape& body) {
  std::printf("%s: volume %.9f, %d faces, %d edges, valid %d\n", label, volume_of(body.occt()), body.face_count(),
              body.edge_count(), BRepCheck_Analyzer(body.occt()).IsValid() ? 1 : 0);
}

// Where a plane cuts a shape: the cut's edges with a point inside `keep`,
// each as its kind (line, circle with its centre and radius, other) and
// its end points.
using Window = std::function<bool(const gp_Pnt&)>;

void print_section(const TopoDS_Shape& shape, const gp_Pln& plane, const Window& keep) {
  try {
    OCC_CATCH_SIGNALS
    BRepAlgoAPI_Section section(shape, plane, false);
    section.ComputePCurveOn1(false);
    section.Approximation(false);
    section.Build();
    if (!section.IsDone()) {
      std::printf("    section: not done\n");
      return;
    }
    for (TopExp_Explorer ex(section.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
      const gp_Pnt a = curve.Value(curve.FirstParameter());
      const gp_Pnt b = curve.Value(curve.LastParameter());
      const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
      if (!keep(a) && !keep(b) && !keep(m)) {
        continue;
      }
      std::printf("    section: ");
      switch (curve.GetType()) {
      case GeomAbs_Line:
        std::printf("line");
        break;
      case GeomAbs_Circle: {
        const gp_Circ circle = curve.Circle();
        std::printf("circle about (%.4f, %.4f, %.4f) radius %.4f", circle.Location().X(), circle.Location().Y(),
                    circle.Location().Z(), circle.Radius());
        break;
      }
      case GeomAbs_Ellipse:
        std::printf("ellipse");
        break;
      default:
        std::printf("curve of kind %d", static_cast<int>(curve.GetType()));
        break;
      }
      std::printf(" from (%.4f, %.4f, %.4f) to (%.4f, %.4f, %.4f)\n", a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z());
    }
  } catch (const Standard_Failure& failure) {
    std::printf("    section: throws %s\n", failure.what());
  }
}

// The plane and window of the section printed after a rounding (none
// where `window` is empty).
struct Cut {
  gp_Pln plane;
  Window window;
};

// MitCad's fillet of `edges` by r as F9 on `body`, and the steps of the
// path past narrow faces; `expected` the exact volume where known (< 0
// where not); the section `cut` of the input and the result where given.
void probe(const char* label, const Shape& body, const std::vector<std::string>& edges, double r, double expected,
           const Cut& cut = Cut()) {
  std::printf("\n%s: fillet %g of %zu edge(s)\n", label, r, edges.size());
  const double before = volume_of(body.occt());
  if (cut.window) {
    std::printf("  input's section:\n");
    print_section(body.occt(), cut.plane, cut.window);
  }
  try {
    FilletSet set;
    set.edges = edges;
    set.radius = r;
    const ShapePtr made = fillet("F9", body, {set});
    if (cut.window) {
      std::printf("  result's section:\n");
      print_section(made->occt(), cut.plane, cut.window);
    }
    const double after = volume_of(made->occt());
    std::printf("  MitCad: valid %d, closed %d, solids %d, volume %.9f (removed %.9f",
                BRepCheck_Analyzer(made->occt()).IsValid() ? 1 : 0, closed(made->occt()) ? 1 : 0,
                solids_of(made->occt()), after, before - after);
    if (expected >= 0.0) {
      std::printf(", exact %.9f, off %.3g", expected, after - expected);
    }
    std::printf("), %d faces, %zu notes, %zu infos\n", made->face_count(), made->notes().size(),
                made->infos().size());
    for (const std::string& note : made->notes()) {
      std::printf("    note: %s\n", note.c_str());
    }
    for (const std::string& info : made->infos()) {
      std::printf("    info: %s\n", info.c_str());
    }
  } catch (const std::exception& error) {
    std::printf("  MitCad: fails: %s\n", error.what());
  }
  std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
  std::vector<TopoDS_Edge> picked;
  for (int e = 0; e < body.edge_count(); ++e) {
    for (const std::string& name : edges) {
      if (body.edge_name(e) == name) {
        owner[static_cast<std::size_t>(e)] = 0;
        picked.push_back(TopoDS::Edge(body.edge(e)));
      }
    }
  }
  for (const double contact : {r, r * (1.0 + 1.0e-3)}) {
    try {
      const std::string where = detail::overflow_failure(body, owner, contact, "fillet");
      std::printf("  narrow at contact %.6f: %s\n", contact, where.empty() ? "(none)" : where.c_str());
    } catch (const std::exception& error) {
      std::printf("  narrow at contact %.6f: throws %s\n", contact, error.what());
    }
  }
  try {
    const std::optional<detail::FacesBeyondGrown> grown = detail::faces_beyond_grown(body, owner, r);
    if (!grown) {
      std::printf("  faces beyond grown: none\n");
    } else {
      int kept = 0;
      for (const int o : grown->owner) {
        kept += o >= 0 ? 1 : 0;
      }
      std::printf("  faces beyond grown: valid %d, volume %.9f, %d faces, %d picked edges, %zu replaced\n",
                  BRepCheck_Analyzer(grown->body->occt()).IsValid() ? 1 : 0, volume_of(grown->body->occt()),
                  grown->body->face_count(), kept, grown->replaced.size());
    }
  } catch (const std::exception& error) {
    std::printf("  faces beyond grown: throws %s\n", error.what());
  }
  // OCCT's own fillet of the edges (the whole tangent chain of each), on a
  // copy.
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copier(body.occt());
    const TopoDS_Shape copy = copier.Shape();
    BRepFilletAPI_MakeFillet maker(copy);
    for (const TopoDS_Edge& edge : picked) {
      maker.Add(r, TopoDS::Edge(copier.Modified(edge).First()));
    }
    maker.Build();
    std::printf("  OCCT's fillet: done %d", maker.IsDone() ? 1 : 0);
    for (int c = 1; c <= maker.NbContours(); ++c) {
      std::printf(", contour %d status %d", c, static_cast<int>(maker.StripeStatus(c)));
    }
    if (maker.IsDone()) {
      std::printf(", valid %d, closed %d, volume %.9f", BRepCheck_Analyzer(maker.Shape()).IsValid() ? 1 : 0,
                  closed(maker.Shape()) ? 1 : 0, volume_of(maker.Shape()));
    }
    std::printf("\n");
  } catch (const Standard_Failure& failure) {
    std::printf("  OCCT's fillet: throws %s\n", failure.what());
  }
}

// `body` with `edges` rounded by 0.1 first (F8); null where that fails.
ShapePtr corners_rounded(const Shape& body, const std::vector<std::string>& edges) {
  try {
    FilletSet set;
    set.edges = edges;
    set.radius = 0.1;
    set.tangent_chain = false;
    return fillet("F8", body, {set});
  } catch (const std::exception& error) {
    std::printf("the 0.1 corners fail: %s\n", error.what());
    return nullptr;
  }
}

void run_ridge() {
  const ShapePtr body = ridge();
  print_input("ridge", *body);
  const std::vector<std::string> front =
      edges_where(*body, [](const gp_Pnt& p) { return near(p.Y(), 0) && near(p.Z(), 10) && near(p.X(), 15); });
  for (const double r : {0.8, 1.5, 4.0}) {
    probe("ridge", *body, front, r, volume_of(body->occt()) - 30.0 * ridge_removed(r));
  }
}

void run_spike() {
  const ShapePtr body = spike();
  print_input("spike", *body);
  const auto front = [](const Shape& b) {
    return edges_where(b, [](const gp_Pnt& p) { return near(p.Y(), 0) && near(p.Z(), 10) && near(p.X(), 15); });
  };
  // The section across the edge at its middle, near the edge.
  const Cut middle{gp_Pln(gp_Pnt(15, 0, 0), gp_Dir(1, 0, 0)),
                   [](const gp_Pnt& p) { return p.Y() < 3.0 && p.Z() > 5.0; }};
  for (const double r : {0.8, 1.5, 2.0, 4.0}) {
    probe("spike", *body, front(*body), r, -1.0, middle);
    // The rounding as the narrow-face rule has it (the ledge taken away,
    // the spike's front continued down to meet the block's front, the
    // corner between them rounded): its contacts in that section.
    const double vz = 10.0 - 12.0 / 8.5;
    const double len = std::sqrt(8.5 * 8.5 + 12.0 * 12.0);
    const double reach = r / std::tan(0.5 * std::acos(-12.0 / len));
    std::printf("  by the rule: contacts (15, 0, %.4f) and (15, %.4f, %.4f)\n", vz - reach, reach * 8.5 / len,
                vz + reach * 12.0 / len);
  }
  const std::vector<std::string> uprights = edges_where(*body, [](const gp_Pnt& p) {
    return near(p.Y(), 0) && near(p.Z(), 5) && (near(p.X(), 0) || near(p.X(), 30));
  });
  std::printf("\nspike: its %zu front upright edges rounded with 0.1 first\n", uprights.size());
  if (const ShapePtr rounded = corners_rounded(*body, uprights)) {
    print_input("spike, corners rounded", *rounded);
    for (const double r : {1.5, 4.0}) {
      probe("spike, corners rounded", *rounded, front(*rounded), r, -1.0, middle);
    }
  }
}

// A block x -10 to 25, y 0 to 30, z 0 to 10 within a cylinder of radius
// 20 whose surface passes through (0, 0, 10) at 45 degrees (its axis
// through (14.142, 0, -4.142)), along y or, where `tilted`, turned by
// about 3 degrees in the plane z = const so that the top beside the
// pocket's left wall widens towards the back; less the notch's pocket.
ShapePtr cylinder_notch(bool tilted) {
  const ShapePtr block = extruded("F1", Frame(), gp_Dir(0, 0, 1), rectangle_region(-10, 0, 35, 30), 10.0);
  const gp_Dir axis = tilted ? gp_Dir(-0.05, 1, 0) : gp_Dir(0, 1, 0);
  // Sketched across the axis (u along z, v across), the circle about
  // (u, v) = (-4.142, 14.142), extruded along the axis from y = -5.
  Frame frame;
  frame.origin = gp_Pnt(0, -5, 0);
  frame.x_axis = gp_Dir(0, 0, 1);
  frame.y_axis = gp_Dir(axis.Y(), -axis.X(), 0);
  const double c = 20.0 / std::sqrt(2.0);
  Region disc = circle_region(10.0 - c, c, 20.0);
  const ShapePtr round = extruded("F2", frame, axis, disc, 45.0);
  const ShapePtr body = boolean(BooleanOp::Intersect, {block.get()}, *round).pieces.at(0).shape;
  Region u;
  u.name = "r{k1[k4,k2],k2[k1,k3],k3[k2,k4],k4[k3,k1]}";
  Loop loop;
  loop.segments.push_back(line("k1[k4,k2]", 9, -1, 9, 15));
  Segment arc;
  arc.name = "k2[k1,k3]";
  arc.kind = SegmentKind::Arc;
  arc.center = gp_Pnt2d(5, 15);
  arc.radius = 4.0;
  arc.start_angle = 0.0;
  arc.end_angle = kPi;
  arc.start = gp_Pnt2d(9, 15);
  arc.end = gp_Pnt2d(1, 15);
  loop.segments.push_back(arc);
  loop.segments.push_back(line("k3[k2,k4]", 1, 15, 1, -1));
  loop.segments.push_back(line("k4[k3,k1]", 1, -1, 9, -1));
  u.loops.push_back(loop);
  Frame floor;
  floor.origin = gp_Pnt(0, 0, 4);
  const ShapePtr pocket = extruded("F3", floor, gp_Dir(0, 0, 1), u, 7.0);
  return boolean(BooleanOp::Cut, {body.get()}, *pocket).pieces.at(0).shape;
}

// The rim of a notch body rounded from its left wall's edge (the tangent
// chain takes the rest) at 1.5 (and `radii`), alone and after the thin
// wall's front edges are rounded with 0.1, with the section at y 7.5 and
// at y 2.
void notch_rims(const char* label, const Shape& body, const std::vector<double>& radii) {
  print_input(label, body);
  const auto left = [](const Shape& b) {
    return edges_where(b, [](const gp_Pnt& p) { return near(p.X(), 1) && near(p.Z(), 10) && p.Y() > 5 && p.Y() < 10; });
  };
  const Window near_wall = [](const gp_Pnt& p) { return p.X() > -4.0 && p.X() < 3.0 && p.Z() > 4.0; };
  const Cut middle{gp_Pln(gp_Pnt(0, 7.5, 0), gp_Dir(0, 1, 0)), near_wall};
  const Cut front_cut{gp_Pln(gp_Pnt(0, 2, 0), gp_Dir(0, 1, 0)), near_wall};
  for (const double r : radii) {
    probe(label, body, left(body), r, -1.0, middle);
  }
  // The thin wall's front edges: those on the front face left of the
  // pocket's right wall, neither at the bottom nor at the top.
  const std::vector<std::string> front = edges_where(body, [](const gp_Pnt& p) {
    return near(p.Y(), 0) && p.X() < 1.0 + 1e-6 && p.Z() > 1e-6 && p.Z() < 10.0 - 1e-6;
  });
  std::printf("\n%s: the thin wall's %zu front edges rounded with 0.1 first\n", label, front.size());
  if (const ShapePtr rounded = corners_rounded(body, front)) {
    const std::string with = std::string(label) + ", corners rounded";
    print_input(with.c_str(), *rounded);
    probe(with.c_str(), *rounded, left(*rounded), 1.5, -1.0, front_cut);
  }
}

void run_cylinder(bool tilted) {
  notch_rims(tilted ? "tilted cylinder notch" : "cylinder notch", *cylinder_notch(tilted), {1.0, 1.5, 2.0});
}

void run_notch() { notch_rims("notch", *notch(), {1.5, 2.0}); }

} // namespace

int main(int argc, char** argv) {
  std::vector<std::string> which;
  for (int i = 1; i < argc; ++i) {
    which.emplace_back(argv[i]);
  }
  if (which.empty()) {
    which = {"ridge", "spike", "notch", "cylinder", "tilted"};
  }
  for (const std::string& name : which) {
    try {
      if (name == "ridge") {
        run_ridge();
      } else if (name == "spike") {
        run_spike();
      } else if (name == "notch") {
        run_notch();
      } else if (name == "cylinder") {
        run_cylinder(false);
      } else if (name == "tilted") {
        run_cylinder(true);
      } else {
        std::printf("unknown body %s\n", name.c_str());
      }
    } catch (const std::exception& error) {
      std::printf("%s: throws %s\n", name.c_str(), error.what());
    }
    std::fflush(stdout);
  }
  return 0;
}
