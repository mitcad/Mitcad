// SPDX-License-Identifier: MIT
// Shells of bodies with parts no thicker than the wall (mitcad#1233): where
// the geometry kernel's offset fails, the shell is the body less a cavity,
// and the parts the wall does not fit in stay solid with their own faces.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Curve.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/revolve.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// A regular hexagon of circumradius r about the origin, curves c<first>..
// c<first+5>.
Region hexagon(int first, double r) {
  Region region;
  Loop loop;
  const auto curve = [first](int k) { return "c" + std::to_string(first + (k + 6) % 6); };
  region.name = "r{";
  for (int i = 0; i < 6; ++i) {
    Segment segment;
    segment.kind = SegmentKind::Line;
    const double a = kPi / 3 * i, b = kPi / 3 * (i + 1);
    segment.start = gp_Pnt2d(r * std::cos(a), r * std::sin(a));
    segment.end = gp_Pnt2d(r * std::cos(b), r * std::sin(b));
    segment.name = curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
    region.name += (i == 0 ? "" : ",") + segment.name;
    loop.segments.push_back(segment);
  }
  region.name += "}";
  region.loops.push_back(loop);
  return region;
}

std::string hexagon_side(const std::string& feature, int first, int i) {
  const auto curve = [first](int k) { return "c" + std::to_string(first + (k + 6) % 6); };
  return face_name(feature, "side", curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]");
}

std::string hexagon_end(const std::string& feature, int first) {
  std::string region = "r{";
  for (int i = 0; i < 6; ++i) {
    const auto curve = [first](int k) { return "c" + std::to_string(first + (k + 6) % 6); };
    region += (i == 0 ? "" : ",") + curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
  }
  return face_name(feature, "end", region + "}");
}

// The names of the shape's edges whose middle passes the test.
std::vector<std::string> edges_where(const Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<std::string> names;
  for (int i = 0; i < shape.edge_count(); ++i) {
    double first = 0, last = 0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(shape.edge(i), first, last);
    if (!curve.IsNull() && keep(curve->Value((first + last) / 2)) && !shape.edge_name(i).empty()) {
      names.push_back(shape.edge_name(i));
    }
  }
  return names;
}

ShapePtr joined(const ShapePtr& a, const ShapePtr& b) {
  const BooleanResult result = boolean(BooleanOp::Join, {a.get()}, *b);
  CHECK(result.pieces.size() == 1);
  return result.pieces.front().shape;
}

bool inside(const Shape& shape, double x, double y, double z) {
  BRepClass3d_SolidClassifier classifier(shape.occt(), gp_Pnt(x, y, z), 1e-7);
  return classifier.State() == TopAbs_IN;
}

// The plane of the one face holding the name, its normal out of the material.
gp_Pln plane_of(const Shape& shape, const std::string& name) {
  const std::vector<int> faces = shape.find_faces(name);
  CHECK(faces.size() == 1);
  if (faces.empty()) {
    return gp_Pln();
  }
  const TopoDS_Face& face = shape.face(faces.front());
  const BRepAdaptor_Surface surface(face, false);
  CHECK(surface.GetType() == GeomAbs_Plane);
  gp_Pln plane = surface.Plane();
  if (face.Orientation() == TopAbs_REVERSED) {
    plane.SetAxis(plane.Axis().Reversed());
  }
  return plane;
}

// The distance from the inside of the wall at the face to the face, along
// the face's normal into the material.
double wall_at(const Shape& before, const Shape& after, const std::string& feature,
               const std::string& face) {
  const gp_Pln outside = plane_of(before, face);
  const gp_Pln inner = plane_of(after, face_name(feature, "offset", face));
  CHECK(outside.Axis().Direction().IsOpposite(inner.Axis().Direction(), 1e-9));
  return outside.Distance(inner.Location());
}

bool has_note(const Shape& shape, const std::string& text) {
  return std::any_of(shape.notes().begin(), shape.notes().end(),
                     [&](const std::string& note) { return note.find(text) != std::string::npos; });
}

// The cap: a flange of radius 40 mm, 2 mm thick (z 205-207), and on it a
// hexagonal prism 10 mm high (circumradius 18 mm) whose vertical corners
// are rounded 2 mm and whose foot is chamfered 1.2 mm all round.
const char* kFlangeBottom = "F2:start(r{c1})";
const char* kFlangeTop = "F2:end(r{c1})";
const char* kFlangeSide = "F2:side(c1)";

ShapePtr cap() {
  const ShapePtr flange = extrude("F2", {circle(1, 0, 0, 40)}, 205, 207);
  const ShapePtr prism = extrude("F3", {hexagon(2, 18)}, 207, 217);
  const ShapePtr body = joined(flange, prism);
  const std::vector<std::string> corners = edges_where(*body, [](const gp_Pnt& p) {
    return std::abs(std::hypot(p.X(), p.Y()) - 18) < 1e-6 && p.Z() > 208;
  });
  CHECK(corners.size() == 6);
  const ShapePtr rounded = fillet("F4", *body, corners, 2.0);
  const std::vector<std::string> foot = edges_where(*rounded, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 207) < 1e-6 && std::hypot(p.X(), p.Y()) < 20;
  });
  CHECK(foot.size() == 12);
  ChamferSpec bevel;
  bevel.distance = 1.2;
  return chamfer("F5", *rounded, foot, bevel);
}

// The cap shelled from its open bottom at 2 and 3 mm: a hexagonal cup with
// walls and a top of the wall's thickness, open through the flange, which
// stays solid with its own faces.
void test_cap() {
  const ShapePtr body = cap();
  for (const double t : {2.0, 3.0}) {
    ShellSpec spec;
    spec.faces = {kFlangeBottom};
    spec.inside = t;
    ShapePtr hollow;
    try {
      hollow = shell("F6", *body, spec);
    } catch (const std::exception& error) {
      std::fprintf(stderr, "cap at %g mm: %s\n", t, error.what());
      CHECK(false);
      continue;
    }
    CHECK(volume(*hollow) < volume(*body));
    CHECK(edge_names_unique(*hollow));
    // The walls and the top.
    for (int i = 0; i < 6; ++i) {
      CHECK_NEAR(wall_at(*body, *hollow, "F6", hexagon_side("F3", 2, i)), t);
    }
    CHECK_NEAR(wall_at(*body, *hollow, "F6", hexagon_end("F3", 2)), t);
    CHECK(inside(*hollow, 0, 0, 217 - t / 2));
    CHECK(!inside(*hollow, 0, 0, 217 - t - 0.5));
    const double mid = 18 * std::cos(kPi / 6) - t / 2; // in the wall of side c2
    CHECK(inside(*hollow, mid * std::cos(kPi / 6), mid * std::sin(kPi / 6), 212));
    // Open through the flange.
    CHECK(!inside(*hollow, 0, 0, 206));
    CHECK(!inside(*hollow, 0, 0, 205.5));
    // The flange solid, with its own faces.
    CHECK(inside(*hollow, 30, 0, 206));
    CHECK(inside(*hollow, 0, -35, 205.1));
    CHECK(faces_named(*hollow, kFlangeTop) == 1);
    CHECK(faces_named(*hollow, kFlangeSide) == 1);
    CHECK(faces_named(*hollow, kFlangeBottom) == 0);
    CHECK(faces_named(*hollow, "F6:offset_cap(" + std::string(kFlangeBottom) + ")") == 1);
    CHECK(faces_named(*hollow, "F6:offset(" + std::string(kFlangeTop) + ")") == 0);
    CHECK(has_note(*hollow, "the wall does not fit under faces " + std::string(kFlangeTop)));
    // The roundings and the chamfers round them: run past.
    CHECK(has_note(*hollow, "the inside of the wall runs past faces F4:fillet("));
  }
  // A wall to both sides: the outside wall joins the inside one, which
  // keeps its notes.
  ShellSpec both;
  both.faces = {kFlangeBottom};
  both.inside = 3;
  both.outside = 1;
  const ShapePtr wall = shell("F6", *body, both);
  CHECK(!inside(*wall, 0, 0, 206));
  CHECK(inside(*wall, 30, 0, 206));
  CHECK(inside(*wall, 0, 0, 217.5));
  CHECK(has_note(*wall, "the wall does not fit under faces " + std::string(kFlangeTop)));
}

// A block 30 x 30 x 20 mm open at its top, a plate 1.5 mm thick standing
// out of its side: the plate is thinner than the wall (2 mm) and stays
// solid; the block is hollowed.
void test_plate_beside_block() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 30, 30)}, 0, 20);
  const ShapePtr plate = extrude("F3", {rectangle(5, 30, 10, 20, 10)}, 9, 10.5);
  const ShapePtr body = joined(block, plate);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {end_cap("F2", 1)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "plate: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR(volume(*hollow), volume(*body) - (30 - 2 * t) * (30 - 2 * t) * (20 - t));
  CHECK(edge_names_unique(*hollow));
  for (int i = 0; i < 4; ++i) {
    CHECK_NEAR(wall_at(*body, *hollow, "F4", side("F2", 1, i)), t);
  }
  CHECK_NEAR(wall_at(*body, *hollow, "F4", start_cap("F2", 1)), t);
  CHECK(inside(*hollow, 40, 15, 9.75));
  CHECK(!inside(*hollow, 15, 15, 10));
  CHECK(faces_named(*hollow, start_cap("F3", 5)) == 1);
  CHECK(faces_named(*hollow, end_cap("F3", 5)) == 1);
  CHECK(faces_named(*hollow, "F4:offset(" + end_cap("F3", 5) + ")") == 0);
  CHECK(faces_named(*hollow, "F4:offset_cap(" + end_cap("F2", 1) + ")") == 1);
  CHECK(has_note(*hollow, "the wall does not fit under faces F3:"));
}

// A closed void in a block 40 x 30 x 20 mm with a fin 2 mm thick and 10 mm
// high on its top, the wall 1.5 mm: the fin is thinner than twice the wall
// and stays solid, the void is the block's.
void test_closed_void_with_fin() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 40, 30)}, 0, 20);
  const ShapePtr fin = extrude("F3", {rectangle(5, 10, 14, 20, 2)}, 20, 30);
  const ShapePtr body = joined(block, fin);
  const double t = 1.5;
  ShellSpec spec;
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fin: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR(volume(*hollow), volume(*body) - (40 - 2 * t) * (30 - 2 * t) * (20 - 2 * t));
  CHECK(!inside(*hollow, 20, 15, 10));
  CHECK(inside(*hollow, 20, 15, 25));
  CHECK(inside(*hollow, 20, 15, 19));
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*hollow, side("F3", 5, i)) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + side("F3", 5, i) + ")") == 0);
  }
  CHECK(faces_named(*hollow, "F4:offset(" + start_cap("F2", 1) + ")") == 1);
  CHECK(faces_named(*hollow, "F4:offset(" + end_cap("F2", 1) + ")") == 1);
  CHECK(has_note(*hollow, "the wall does not fit under faces F3:"));
}

// A plate 2 mm thick shelled 3 mm from its removed bottom: the wall fits
// nowhere, and the shell fails saying so.
void test_nothing_to_hollow() {
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 40, 40)}, 0, 2);
  ShellSpec spec;
  spec.faces = {start_cap("F2", 1)};
  spec.inside = 3;
  CHECK(throws_with([&] { shell("F3", *plate, spec); },
                    "the wall does not fit under faces F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"));
  CHECK(throws_with([&] { shell("F3", *plate, spec); }, "nothing can be hollowed"));
}

// Two blocks 20 mm square joined by a web 1.5 mm thick, closed voids with a
// 2 mm wall: the kernel's offset fails on the web, which is thinner than
// twice the wall; it is cut off before the offset and stays solid, and each
// block gets its void.
void test_closed_voids_beside_a_web() {
  const ShapePtr a = extrude("F2", {rectangle(1, 0, 0, 20, 20)}, 0, 20);
  const ShapePtr b = extrude("F3", {rectangle(5, 40, 0, 20, 20)}, 0, 20);
  const ShapePtr web = extrude("F4", {rectangle(9, 20, 5, 20, 10)}, 9, 10.5);
  const ShapePtr body = joined(joined(a, web), b);
  const double t = 2;
  ShellSpec spec;
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F6", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "web: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR(volume(*hollow), volume(*body) - 2 * std::pow(20 - 2 * t, 3));
  CHECK(edge_names_unique(*hollow));
  CHECK(!inside(*hollow, 10, 10, 10));
  CHECK(!inside(*hollow, 50, 10, 10));
  CHECK(inside(*hollow, 30, 10, 9.75));
  CHECK(inside(*hollow, 21, 10, 9.75));
  for (int i = 0; i < 4; ++i) {
    CHECK_NEAR(wall_at(*body, *hollow, "F6", side("F2", 1, i)), t);
    CHECK_NEAR(wall_at(*body, *hollow, "F6", side("F3", 5, i)), t);
  }
  CHECK_NEAR(wall_at(*body, *hollow, "F6", start_cap("F2", 1)), t);
  CHECK_NEAR(wall_at(*body, *hollow, "F6", end_cap("F3", 5)), t);
  CHECK(faces_named(*hollow, end_cap("F4", 9)) == 1);
  CHECK(faces_named(*hollow, start_cap("F4", 9)) == 1);
  CHECK(faces_named(*hollow, "F6:offset(" + end_cap("F4", 9) + ")") == 0);
  CHECK(has_note(*hollow, "the wall does not fit under faces F4:end"));
  // With an outside wall too, the voids are cut out of the body's outside
  // offset.
  spec.outside = 1;
  const ShapePtr thick = shell("F6", *body, spec);
  CHECK(volume(*thick) > volume(*hollow));
  CHECK(!inside(*thick, 10, 10, 10));
  CHECK(!inside(*thick, 50, 10, 10));
  CHECK(inside(*thick, 30, 10, 9.75));
  CHECK(inside(*thick, 10, 10, 20.5));
  CHECK(has_note(*thick, "the wall does not fit under faces F4:end"));
}

// A closed polygon through the points, lines c<first>.. in order, each named
// after the lines before and after it.
Region polygon(int first, const std::vector<gp_Pnt2d>& points) {
  const int n = static_cast<int>(points.size());
  const auto curve = [first, n](int k) { return "c" + std::to_string(first + (k + n) % n); };
  Region region;
  Loop loop;
  region.name = "r{";
  for (int i = 0; i < n; ++i) {
    Segment segment;
    segment.kind = SegmentKind::Line;
    segment.start = points[static_cast<std::size_t>(i)];
    segment.end = points[static_cast<std::size_t>((i + 1) % n)];
    segment.name = curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
    region.name += (i == 0 ? "" : ",") + segment.name;
    loop.segments.push_back(segment);
  }
  region.name += "}";
  region.loops.push_back(loop);
  return region;
}

// The side face of line i of polygon(first, ...) with n lines.
std::string polygon_side(const std::string& feature, int first, int n, int i) {
  const auto curve = [first, n](int k) { return "c" + std::to_string(first + (k + n) % n); };
  return face_name(feature, "side", curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]");
}

std::string all_notes(const Shape& shape) {
  std::string out;
  for (const std::string& note : shape.notes()) {
    out += note + "\n";
  }
  return out;
}

// The faces of the shape whose names pass the test, as a compound.
TopoDS_Compound faces_where(const Shape& shape, const std::function<bool(const NameList&)>& keep) {
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (int i = 0; i < shape.face_count(); ++i) {
    if (keep(shape.face_names(i))) {
      builder.Add(faces, shape.face(i));
    }
  }
  return faces;
}

// The wall's least thickness: the distance from the inside of the wall
// (<feature>:offset(..)) to the faces the body had (named without the
// feature).
double thinnest_wall(const Shape& hollow, const std::string& feature) {
  const std::string inner = feature + ":offset(";
  const std::string own = feature + ":";
  const TopoDS_Compound inside_faces = faces_where(hollow, [&](const NameList& names) {
    return !names.empty() && names.front().rfind(inner, 0) == 0;
  });
  const TopoDS_Compound outside_faces = faces_where(hollow, [&](const NameList& names) {
    return !names.empty() && std::none_of(names.begin(), names.end(), [&](const std::string& name) {
      return name.rfind(own, 0) == 0;
    });
  });
  BRepExtrema_DistShapeShape distance(inside_faces, outside_faces);
  CHECK(distance.IsDone());
  return distance.IsDone() ? distance.Value() : 0.0;
}

// The cap's wall at 2 and 3 mm is nowhere thinner than given, also beside
// the chamfers and roundings the inside of the wall runs past; or the note
// says by how much it is.
void test_cap_wall_thickness() {
  const ShapePtr body = cap();
  for (const double t : {2.0, 3.0}) {
    ShellSpec spec;
    spec.faces = {kFlangeBottom};
    spec.inside = t;
    const ShapePtr hollow = shell("F6", *body, spec);
    const double thinnest = thinnest_wall(*hollow, "F6");
    std::fprintf(stderr, "cap at %g mm: the wall at least %.9g mm\n%s", t, thinnest,
                 all_notes(*hollow).c_str());
    if (thinnest < t - 1e-6) {
      char text[64];
      std::snprintf(text, sizeof text, "down to %.2f mm", thinnest);
      CHECK(has_note(*hollow, text));
    }
  }
}

// A cup 40 mm across and 30 mm high, open at its top, with a skirt 1.25 mm
// thick sloping down and out from its side round it: the skirt is thinner
// than twice the wall (2 mm), between two cones, and stays solid; the cup is
// hollowed, its wall running past the skirt's root.
void test_cup_with_conical_skirt() {
  RevolveSpec revolution;
  revolution.feature = "F2";
  revolution.regions = {polygon(1, {gp_Pnt2d(0, 0), gp_Pnt2d(20, 0), gp_Pnt2d(20, 20), gp_Pnt2d(32, 12),
                                    gp_Pnt2d(32, 13.5), gp_Pnt2d(20, 21.5), gp_Pnt2d(20, 30), gp_Pnt2d(0, 30)})};
  revolution.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  revolution.angle1 = 2 * kPi;
  const ShapePtr body = revolve(revolution);
  const std::string top = polygon_side("F2", 1, 8, 6);
  const std::string lower = polygon_side("F2", 1, 8, 2);
  const std::string rim = polygon_side("F2", 1, 8, 3);
  const std::string upper = polygon_side("F2", 1, 8, 4);
  CHECK(faces_named(*body, top) == 1);
  CHECK(faces_named(*body, lower) == 1);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {top};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F3", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "skirt: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR_TOL(volume(*hollow), volume(*body) - kPi * (20 - t) * (20 - t) * (30 - t), 1e-5);
  CHECK(edge_names_unique(*hollow));
  CHECK(!inside(*hollow, 0, 15, 0));
  CHECK(!inside(*hollow, 17, 10, 0));
  CHECK(inside(*hollow, 19, 10, 0));
  CHECK(inside(*hollow, 0, 1, 0));
  // The wall under the skirt's root, and the skirt solid.
  CHECK(inside(*hollow, 19, 20.75, 0));
  CHECK(!inside(*hollow, 17.5, 20.75, 0));
  CHECK(inside(*hollow, 26, 16.75, 0));
  CHECK(inside(*hollow, 0, 16.75, -26));
  for (const std::string& face : {lower, rim, upper}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F3:offset(" + face + ")") == 0);
    CHECK(has_note(*hollow, face));
  }
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  CHECK(faces_named(*hollow, "F3:offset(" + polygon_side("F2", 1, 8, 1) + ")") >= 1);
  CHECK(faces_named(*hollow, "F3:offset(" + polygon_side("F2", 1, 8, 0) + ")") == 1);
  CHECK(thinnest_wall(*hollow, "F3") >= t - 1e-6);
}

// A plate 60 x 40 x 6 mm open at its bottom with a boss 20 mm across and a
// sleeve 1 mm thick (radii 5 and 6 mm) on its top: the sleeve is thinner
// than twice the wall (2 mm), between two cylinders, and stays solid; the
// plate and the boss are hollowed.
void test_sleeve_beside_boss() {
  const ShapePtr plate = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 6);
  const ShapePtr boss = extrude("F3", {circle(5, 15, 20, 10)}, 6, 21);
  Region ring = circle(6, 42, 20, 6);
  ring.loops.push_back(circle(7, 42, 20, 5).loops.front());
  ring.name = "r{c6;c7}";
  const ShapePtr sleeve = extrude("F4", {ring}, 6, 18);
  const ShapePtr body = joined(joined(plate, boss), sleeve);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {start_cap("F2", 1)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F5", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "sleeve: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR_TOL(volume(*hollow),
                 volume(*body) - (60 - 2 * t) * (40 - 2 * t) * (6 - t) - kPi * (10 - t) * (10 - t) * 15,
                 1e-5);
  CHECK(edge_names_unique(*hollow));
  // The plate and the boss hollowed.
  CHECK(!inside(*hollow, 30, 20, 2));
  CHECK(inside(*hollow, 30, 20, 5));
  CHECK(!inside(*hollow, 15, 20, 10));
  CHECK(inside(*hollow, 24, 20, 10));
  CHECK(inside(*hollow, 15, 20, 20));
  // The sleeve solid, the plate's wall under it.
  CHECK(inside(*hollow, 47.5, 20, 12));
  CHECK(inside(*hollow, 36.5, 20, 17));
  CHECK(!inside(*hollow, 42, 20, 8));
  CHECK(inside(*hollow, 42, 20, 5));
  CHECK(!inside(*hollow, 42, 20, 3));
  for (const std::string& face : {std::string("F4:side(c6)"), std::string("F4:side(c7)")}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F5:offset(" + face + ")") == 0);
    CHECK(has_note(*hollow, face));
  }
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  CHECK(faces_named(*hollow, "F5:offset(F3:side(c5))") == 1);
  CHECK(thinnest_wall(*hollow, "F5") >= t - 1e-6);
}

// A block 30 x 30 x 20 mm open at its top with a fin standing out of its
// side, 3 mm thick at its root and 1 mm at its tip 15 mm out: the fin is
// thinner than twice the wall (2 mm) between faces that are not parallel,
// and stays solid; the block is hollowed.
void test_tapered_fin() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 30, 30)}, 0, 20);
  const ShapePtr fin = extrude(
      "F3", {polygon(5, {gp_Pnt2d(30, 13.5), gp_Pnt2d(45, 14.5), gp_Pnt2d(45, 15.5), gp_Pnt2d(30, 16.5)})}, 2,
      18);
  const ShapePtr body = joined(block, fin);
  const std::string taper_a = polygon_side("F3", 5, 4, 0);
  const std::string tip = polygon_side("F3", 5, 4, 1);
  const std::string taper_b = polygon_side("F3", 5, 4, 2);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {end_cap("F2", 1)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "tapered fin: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR(volume(*hollow), volume(*body) - (30 - 2 * t) * (30 - 2 * t) * (20 - t));
  CHECK(edge_names_unique(*hollow));
  CHECK(!inside(*hollow, 15, 15, 10));
  CHECK(inside(*hollow, 1, 15, 10));
  CHECK(inside(*hollow, 29, 15, 10));
  CHECK(!inside(*hollow, 27, 15, 10));
  CHECK(inside(*hollow, 37, 15, 10));
  CHECK(inside(*hollow, 44, 15, 3));
  for (const std::string& face : {taper_a, tip, taper_b}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + face + ")") == 0);
  }
  CHECK(has_note(*hollow, taper_a));
  CHECK(has_note(*hollow, taper_b));
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  for (int i = 0; i < 4; ++i) {
    CHECK_NEAR(wall_at(*body, *hollow, "F4", side("F2", 1, i)), t);
  }
  CHECK_NEAR(wall_at(*body, *hollow, "F4", start_cap("F2", 1)), t);
  CHECK(thinnest_wall(*hollow, "F4") >= t - 1e-6);
}

// A closed void in a block 40 x 30 x 20 mm with a fin on its top 12 mm
// high, 3 mm thick at its root and 1 mm at its top, the wall 2 mm: the fin
// stays solid, the void is the block's.
void test_closed_void_with_tapered_fin() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 40, 30)}, 0, 20);
  ExtrudeSpec upright;
  upright.feature = "F3";
  upright.frame.x_axis = gp_Dir(0, 1, 0);
  upright.frame.y_axis = gp_Dir(0, 0, 1);
  upright.direction = gp_Dir(1, 0, 0);
  upright.regions = {
      polygon(5, {gp_Pnt2d(13.5, 20), gp_Pnt2d(16.5, 20), gp_Pnt2d(15.5, 32), gp_Pnt2d(14.5, 32)})};
  upright.start = 10;
  upright.end = 30;
  const ShapePtr body = joined(block, mitcad::geometry::extrude(upright));
  const double t = 2;
  ShellSpec spec;
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "closed void, tapered fin: %s\n", error.what());
    CHECK(false);
    return;
  }
  CHECK_NEAR(volume(*hollow), volume(*body) - (40 - 2 * t) * (30 - 2 * t) * (20 - 2 * t));
  CHECK(!inside(*hollow, 20, 15, 10));
  CHECK(inside(*hollow, 20, 15, 19));
  CHECK(inside(*hollow, 20, 15, 25));
  CHECK(inside(*hollow, 20, 15, 31.5));
  for (const int i : {1, 3}) {
    const std::string taper = polygon_side("F3", 5, 4, i);
    CHECK(faces_named(*hollow, taper) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + taper + ")") == 0);
    CHECK(has_note(*hollow, taper));
  }
  CHECK(has_note(*hollow, "the wall does not fit under faces F3:"));
  CHECK(faces_named(*hollow, "F4:offset(" + end_cap("F2", 1) + ")") == 1);
  CHECK(thinnest_wall(*hollow, "F4") >= t - 1e-6);
}

// A block 30 x 30 x 20 mm open at its top with a keel under it, 15 mm deep,
// 3 mm thick at its root and 1 mm at its foot, running the block's whole
// length: its ends continue the block's sides, so no face round it can be
// extended over its root (mitcad#1240). It stays solid, cut off at its
// root's plane; the block is hollowed.
void test_keel_flush_with_sides() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 30, 30)}, 0, 20);
  ExtrudeSpec along;
  along.feature = "F3";
  along.frame.x_axis = gp_Dir(0, 1, 0);
  along.frame.y_axis = gp_Dir(0, 0, 1);
  along.direction = gp_Dir(1, 0, 0);
  along.regions = {
      polygon(5, {gp_Pnt2d(14.5, -15), gp_Pnt2d(15.5, -15), gp_Pnt2d(16.5, 0), gp_Pnt2d(13.5, 0)})};
  along.start = 0;
  along.end = 30;
  const ShapePtr body = joined(block, mitcad::geometry::extrude(along));
  const std::string taper_a = polygon_side("F3", 5, 4, 1);
  const std::string foot = polygon_side("F3", 5, 4, 0);
  const std::string taper_b = polygon_side("F3", 5, 4, 3);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {end_cap("F2", 1)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "keel: %s\n", error.what());
    CHECK(false);
    return;
  }
  std::fprintf(stderr, "keel:\n%s", all_notes(*hollow).c_str());
  CHECK_NEAR(volume(*hollow), volume(*body) - (30 - 2 * t) * (30 - 2 * t) * (20 - t));
  CHECK(edge_names_unique(*hollow));
  // The block hollowed, its bottom wall over the keel.
  CHECK(!inside(*hollow, 15, 15, 10));
  CHECK(!inside(*hollow, 15, 15, 2.5));
  CHECK(inside(*hollow, 15, 15, 1));
  CHECK(inside(*hollow, 1, 15, 10));
  CHECK(inside(*hollow, 29, 15, 10));
  // The keel solid all along.
  for (const double x : {0.5, 15.0, 29.5}) {
    CHECK(inside(*hollow, x, 15, -1));
    CHECK(inside(*hollow, x, 15, -7.5));
    CHECK(inside(*hollow, x, 15, -14.5));
  }
  for (const std::string& face : {taper_a, foot, taper_b}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + face + ")") == 0);
  }
  CHECK(has_note(*hollow, taper_a));
  CHECK(has_note(*hollow, taper_b));
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  for (int i = 0; i < 4; ++i) {
    CHECK_NEAR(wall_at(*body, *hollow, "F4", side("F2", 1, i)), t);
  }
  CHECK(thinnest_wall(*hollow, "F4") >= t - 1e-6);
}

// A block 30 x 30 x 20 mm open at its top with a fin standing out of its
// side, 3 mm thick at its root and 1 mm at its tip 15 mm out, as high as
// the block: its top and bottom continue the block's, the top open with
// it (mitcad#1240). The fin stays solid, cut off at its root's plane; the
// block is hollowed.
void test_fin_flush_with_top_and_bottom() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 30, 30)}, 0, 20);
  const ShapePtr fin = extrude(
      "F3", {polygon(5, {gp_Pnt2d(30, 13.5), gp_Pnt2d(45, 14.5), gp_Pnt2d(45, 15.5), gp_Pnt2d(30, 16.5)})}, 0,
      20);
  const ShapePtr body = joined(block, fin);
  const std::string taper_a = polygon_side("F3", 5, 4, 0);
  const std::string tip = polygon_side("F3", 5, 4, 1);
  const std::string taper_b = polygon_side("F3", 5, 4, 2);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {end_cap("F2", 1)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fin flush with top and bottom: %s\n", error.what());
    CHECK(false);
    return;
  }
  std::fprintf(stderr, "fin flush with top and bottom:\n%s", all_notes(*hollow).c_str());
  CHECK_NEAR(volume(*hollow), volume(*body) - (30 - 2 * t) * (30 - 2 * t) * (20 - t));
  CHECK(edge_names_unique(*hollow));
  CHECK(!inside(*hollow, 15, 15, 10));
  CHECK(inside(*hollow, 29, 15, 10));
  CHECK(!inside(*hollow, 27, 15, 10));
  CHECK(inside(*hollow, 15, 15, 1));
  for (const double z : {0.5, 10.0, 19.5}) {
    CHECK(inside(*hollow, 31, 15, z));
    CHECK(inside(*hollow, 37, 15, z));
    CHECK(inside(*hollow, 44.5, 15, z));
  }
  for (const std::string& face : {taper_a, tip, taper_b}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + face + ")") == 0);
  }
  CHECK(has_note(*hollow, taper_a));
  CHECK(has_note(*hollow, taper_b));
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  // The side the fin stands on is two faces beside it (#0 and #1), the
  // inside of its wall one face named after both.
  for (int i = 0; i < 4; ++i) {
    const std::string face = side("F2", 1, i);
    if (faces_named(*body, face + "#0") == 1) {
      CHECK(faces_named(*body, face + "#1") == 1);
      const std::vector<int> a = hollow->find_faces("F4:offset(" + face + "#0)");
      const std::vector<int> b = hollow->find_faces("F4:offset(" + face + "#1)");
      CHECK(a.size() == 1);
      CHECK(a == b);
    } else {
      CHECK_NEAR(wall_at(*body, *hollow, "F4", face), t);
    }
  }
  CHECK(inside(*hollow, 29, 5, 10));
  CHECK(!inside(*hollow, 27, 5, 10));
  CHECK(inside(*hollow, 29, 25, 10));
  CHECK(!inside(*hollow, 27, 25, 10));
  CHECK_NEAR(wall_at(*body, *hollow, "F4", start_cap("F2", 1)), t);
  CHECK(thinnest_wall(*hollow, "F4") >= t - 1e-6);
}

// An L-shaped body (a base 40 x 30 x 10 mm and a tower 15 mm wide and 30 mm
// high at its one end) open at the tower's top, with a fin in the inside
// corner between the base's top (the floor) and the tower's side (the
// wall): 15 mm high, 3 mm thick at the wall and 1 mm at its tip, which
// continues the base's end (mitcad#1241). Its root lies on two planes; it
// stays solid, cut off at them; the L is hollowed.
void test_fin_in_inside_corner() {
  const ShapePtr base = extrude("F2", {rectangle(1, 0, 0, 40, 30)}, 0, 10);
  const ShapePtr tower = extrude("F3", {rectangle(5, 0, 0, 15, 30)}, 0, 30);
  const ShapePtr fin = extrude(
      "F4", {polygon(9, {gp_Pnt2d(15, 13.5), gp_Pnt2d(40, 14.5), gp_Pnt2d(40, 15.5), gp_Pnt2d(15, 16.5)})}, 10,
      25);
  const ShapePtr body = joined(joined(base, tower), fin);
  const std::string taper_a = polygon_side("F4", 9, 4, 0);
  const std::string tip = polygon_side("F4", 9, 4, 1);
  const std::string taper_b = polygon_side("F4", 9, 4, 2);
  CHECK(faces_named(*body, taper_a) == 1);
  CHECK(faces_named(*body, taper_b) == 1);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {end_cap("F3", 5)};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F5", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fin in an inside corner: %s\n", error.what());
    CHECK(false);
    return;
  }
  std::fprintf(stderr, "fin in an inside corner:\n%s", all_notes(*hollow).c_str());
  // The cavity: the L less the wall, open at the tower's top.
  const double cavity = ((40 - 2 * t) * (10 - 2 * t) + (15 - 2 * t) * (30 - (10 - t))) * (30 - 2 * t);
  CHECK_NEAR(volume(*hollow), volume(*body) - cavity);
  CHECK(edge_names_unique(*hollow));
  // The L hollowed, its walls under and beside the fin.
  CHECK(!inside(*hollow, 20, 15, 5));
  CHECK(!inside(*hollow, 8, 15, 20));
  CHECK(inside(*hollow, 20, 15, 9));
  CHECK(inside(*hollow, 14, 15, 20));
  CHECK(inside(*hollow, 39, 15, 5));
  // The fin solid, from its root on both planes to its tip.
  for (const double z : {10.5, 17.5, 24.5}) {
    CHECK(inside(*hollow, 15.5, 15, z));
    CHECK(inside(*hollow, 27, 15, z));
    CHECK(inside(*hollow, 39.5, 15, z));
  }
  // The tip is one face with the base's end, whose wall is offset.
  CHECK(faces_named(*hollow, tip) == 1);
  for (const std::string& face : {taper_a, taper_b}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F5:offset(" + face + ")") == 0);
  }
  CHECK(has_note(*hollow, taper_a));
  CHECK(has_note(*hollow, taper_b));
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  CHECK(thinnest_wall(*hollow, "F5") >= t - 1e-6);
}

// A cylinder 30 mm across and 20 mm high open at its top with a fin standing
// out of its curved side, 2.75 mm thick at its root and 1 mm at its tip 15
// mm out, as high as the cylinder (mitcad#1241): its root lies on the
// cylinder, which no face round it can be extended over. It stays solid,
// cut off at the cylinder; the cylinder is hollowed.
void test_fin_on_cylinder() {
  const ShapePtr drum = extrude("F2", {circle(1, 0, 0, 15)}, 0, 20);
  const ShapePtr fin = extrude(
      "F3", {polygon(5, {gp_Pnt2d(10, -1.5), gp_Pnt2d(30, -0.5), gp_Pnt2d(30, 0.5), gp_Pnt2d(10, 1.5)})}, 0, 20);
  const ShapePtr body = joined(drum, fin);
  const std::string taper_a = polygon_side("F3", 5, 4, 0);
  const std::string tip = polygon_side("F3", 5, 4, 1);
  const std::string taper_b = polygon_side("F3", 5, 4, 2);
  CHECK(faces_named(*body, taper_a) == 1);
  CHECK(faces_named(*body, taper_b) == 1);
  const double t = 2;
  ShellSpec spec;
  spec.faces = {face_name("F2", "end", "r{c1}")};
  spec.inside = t;
  ShapePtr hollow;
  try {
    hollow = shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "fin on a cylinder: %s\n", error.what());
    CHECK(false);
    return;
  }
  std::fprintf(stderr, "fin on a cylinder:\n%s", all_notes(*hollow).c_str());
  CHECK_NEAR_TOL(volume(*hollow), volume(*body) - kPi * (15 - t) * (15 - t) * (20 - t), 1e-5);
  CHECK(edge_names_unique(*hollow));
  // The cylinder hollowed, its wall behind the fin.
  CHECK(!inside(*hollow, 0, 0, 10));
  CHECK(!inside(*hollow, 12.5, 0, 10));
  CHECK(inside(*hollow, 14, 0, 10));
  CHECK(inside(*hollow, -14, 0, 10));
  CHECK(inside(*hollow, 0, 0, 1));
  // The fin solid from its root to its tip, top to bottom.
  for (const double z : {0.5, 10.0, 19.5}) {
    CHECK(inside(*hollow, 15.5, 0, z));
    CHECK(inside(*hollow, 22, 0, z));
    CHECK(inside(*hollow, 29.5, 0, z));
  }
  for (const std::string& face : {taper_a, tip, taper_b}) {
    CHECK(faces_named(*hollow, face) == 1);
    CHECK(faces_named(*hollow, "F4:offset(" + face + ")") == 0);
  }
  CHECK(has_note(*hollow, taper_a));
  CHECK(has_note(*hollow, taper_b));
  CHECK(has_note(*hollow, "the wall does not fit under faces "));
  CHECK(thinnest_wall(*hollow, "F4") >= t - 1e-6);
}

} // namespace

void shell_thin_parts_tests() {
  guarded("test_cap", test_cap);
  guarded("test_plate_beside_block", test_plate_beside_block);
  guarded("test_closed_void_with_fin", test_closed_void_with_fin);
  guarded("test_closed_voids_beside_a_web", test_closed_voids_beside_a_web);
  guarded("test_nothing_to_hollow", test_nothing_to_hollow);
  guarded("test_cap_wall_thickness", test_cap_wall_thickness);
  guarded("test_cup_with_conical_skirt", test_cup_with_conical_skirt);
  guarded("test_sleeve_beside_boss", test_sleeve_beside_boss);
  guarded("test_tapered_fin", test_tapered_fin);
  guarded("test_closed_void_with_tapered_fin", test_closed_void_with_tapered_fin);
  guarded("test_keel_flush_with_sides", test_keel_flush_with_sides);
  guarded("test_fin_flush_with_top_and_bottom", test_fin_flush_with_top_and_bottom);
  guarded("test_fin_in_inside_corner", test_fin_in_inside_corner);
  guarded("test_fin_on_cylinder", test_fin_on_cylinder);
}

} // namespace test
