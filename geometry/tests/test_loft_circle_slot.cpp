// SPDX-License-Identifier: MIT
// Shell and Thicken of a loft from a circle to a slot (mitcad#2141,
// mitcad#2143), through the geometry facade as the model builds them.
//
// The loft through two sections rules a face across each pair of matched
// edges: the circle is cut where rays from the middle through the slot's
// vertices meet it, and its long pieces go to the slot's straight sides,
// its short ones to the half circles. Both sections run on smoothly through
// those vertices, so neighbouring faces share their tangent planes at the
// two ends of the line between them; as OCCT's through-sections made them
// (each edge at its own parameter) the faces met at a crease of some ten
// degrees between those ends, because a long piece of the circle against a
// straight side and a short one against a half circle advance in ratios
// about twice apart. Every offset of the faces ran into it: Shell failed
// ("the offset faces do not close up", earlier "the shell produced an
// invalid shape"), Sharp and Round alike and with either end removed, and
// Thicken of the surface loft failed ("the thickened surface does not
// close"). The loft now makes the faces pieces of one surface, continuous
// in its first derivative across those lines and periodic round the
// section (match_seams in geometry/src/loft_skin.cpp); faces tangent on
// surfaces of their own were not enough (the kernel's offset extended
// them differently and the shared edges' curves came out short). The
// symmetric Thicken offsets the faces twice; their first offset is C0 as a
// whole and is put on each face's own range for the second
// (geometry/src/surfaces.cpp, on_own_ranges).
//
// Checked here: the side faces of the lofts are tangent along every line
// between them; the shells are valid single solids whose inner faces lie
// the thickness from the outer ones, Sharp and Round the same volume within
// 1e-4 (no edge is concave, so Round adds no rounding; the kernel's Round
// wall comes out 1e-5 apart); Thicken to either side and symmetric gives
// valid single solids whose volumes are the surface's area times the
// thickness as the offset's volume formula says (the area terms of the two
// sides cancel in their sum, and symmetric in itself). The ruled loft of
// the same sections (mitcad#2191; every loft of the `.solidcraft` import
// is ruled) gets the same eased faces, each straight between the sections
// within 1e-6, and shells as the smooth one does.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

using EdgeFaces = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// An accurate volume (OCCT's default integration is off by tenths of a
// percent on lofted bodies).
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1.0e-9);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

bool planar(const TopoDS_Face& face) { return BRepAdaptor_Surface(face).GetType() == GeomAbs_Plane; }

// A circle of `radius` about the sketch origin: region r{c1}.
g::Region circle(double radius) {
  g::Region region = g::circle_region(0, 0, radius);
  region.loops[0].segments[0].name = "c1";
  region.name = "r{c1}";
  return region;
}

// A centre-point slot along the sketch's x axis about its origin, its arcs'
// centres `half` either side of it, of radius `radius`: region
// r{c2,c3,c4,c5}, counter-clockwise from the lower straight side.
g::Region slot(double half, double radius) {
  const auto line = [](const char* name, double x0, double y0, double x1, double y1) {
    g::Segment segment;
    segment.name = name;
    segment.kind = g::SegmentKind::Line;
    segment.start = gp_Pnt2d(x0, y0);
    segment.end = gp_Pnt2d(x1, y1);
    return segment;
  };
  const auto arc = [radius](const char* name, double cx, double from) {
    g::Segment segment;
    segment.name = name;
    segment.kind = g::SegmentKind::Arc;
    segment.center = gp_Pnt2d(cx, 0);
    segment.radius = radius;
    segment.start_angle = from;
    segment.end_angle = from + kPi;
    return segment;
  };
  g::Loop loop;
  loop.segments = {line("c2", -half, -radius, half, -radius), arc("c3", half, -kPi / 2),
                   line("c4", half, radius, -half, radius), arc("c5", -half, kPi / 2)};
  g::Region region;
  region.name = "r{c2,c3,c4,c5}";
  region.loops.push_back(loop);
  return region;
}

g::LoftSection section(const g::Frame& frame, const g::Region& region) {
  g::LoftSection s;
  s.frame = frame;
  s.region = region;
  return s;
}

// The ticket's model: a circle of radius 30 on XY, a slot (centres 60 either
// side, radius 20) 50 above it.
g::LoftSpec ticket_loft() {
  g::Frame bottom;
  g::Frame top;
  top.origin = gp_Pnt(0, 0, 50);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(bottom, circle(30)), section(top, slot(60, 20))};
  return spec;
}

// The owner's model: on XZ a circle of radius 50, on a plane 100 along Y a
// slot (centres 73 either side, radius 21); `slot_first` lofts from the
// slot.
g::LoftSpec owner_loft(bool slot_first) {
  g::Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  g::Frame plane = xz;
  plane.origin = gp_Pnt(0, 100, 0);
  g::LoftSpec spec;
  spec.feature = "F8";
  const g::LoftSection circular = section(xz, circle(50));
  const g::LoftSection slotted = section(plane, slot(73, 21));
  spec.sections =
      slot_first ? std::vector<g::LoftSection>{slotted, circular} : std::vector<g::LoftSection>{circular, slotted};
  return spec;
}

// The side faces of the loft meet tangentially along every edge between
// two of them: their normals at 11 points along it within 1e-6 radians.
void check_tangent_sides(const char* what, const TopoDS_Shape& loft) {
  EdgeFaces faces_of;
  TopExp::MapShapesAndAncestors(loft, TopAbs_EDGE, TopAbs_FACE, faces_of);
  int lines = 0;
  double worst = 0.0;
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces_of.FindKey(i));
    const NCollection_List<TopoDS_Shape>& faces = faces_of.FindFromIndex(i);
    if (BRep_Tool::Degenerated(edge) || faces.Extent() != 2) {
      continue;
    }
    const TopoDS_Face& a = TopoDS::Face(faces.First());
    const TopoDS_Face& b = TopoDS::Face(faces.Last());
    if (a.IsSame(b) || planar(a) || planar(b)) {
      continue;
    }
    ++lines;
    for (int k = 0; k <= 10; ++k) {
      gp_Vec normals[2];
      int side = 0;
      for (const TopoDS_Face* face : {&a, &b}) {
        double first = 0.0;
        double last = 0.0;
        const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, *face, first, last);
        CHECK(!pcurve.IsNull());
        if (pcurve.IsNull()) {
          return;
        }
        const gp_Pnt2d uv = pcurve->Value(first + (last - first) * k / 10.0);
        gp_Pnt p;
        gp_Vec du;
        gp_Vec dv;
        BRepAdaptor_Surface(*face, false).D1(uv.X(), uv.Y(), p, du, dv);
        normals[side++] = du.Crossed(dv);
      }
      const double angle = normals[0].Angle(normals[1]);
      worst = std::max(worst, std::min(angle, kPi - angle));
    }
  }
  std::printf("%s: %d lines between side faces, the largest crease %.3g rad\n", what, lines, worst);
  CHECK(lines > 0);
  CHECK(worst <= 1.0e-6);
}

// The inner faces lie `t` from the outer ones: at up to five points of
// each (its middle and four about it, those inside the face), within 1e-6.
void check_wall(const char* what, const std::vector<TopoDS_Face>& outer, const std::vector<TopoDS_Face>& inner,
                double t) {
  BRep_Builder builder;
  TopoDS_Compound outside;
  builder.MakeCompound(outside);
  for (const TopoDS_Face& face : outer) {
    builder.Add(outside, face);
  }
  CHECK(!inner.empty());
  int measured = 0;
  for (const TopoDS_Face& face : inner) {
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    for (const auto& [su, sv] : {std::pair{0.5, 0.5}, {0.3, 0.3}, {0.7, 0.3}, {0.3, 0.7}, {0.7, 0.7}}) {
      const gp_Pnt2d uv(u0 + (u1 - u0) * su, v0 + (v1 - v0) * sv);
      if (BRepClass_FaceClassifier(face, uv, 1.0e-7).State() != TopAbs_IN) {
        continue;
      }
      const gp_Pnt p = surface.Value(uv.X(), uv.Y());
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), outside);
      CHECK(distance.IsDone());
      ++measured;
      if (distance.IsDone() && std::abs(distance.Value() - t) > 1.0e-6) {
        std::printf("%s: an inner face lies %.9f from the outer faces, not %g\n", what, distance.Value(), t);
        CHECK(false);
      }
    }
  }
  CHECK(measured >= static_cast<int>(inner.size()));
}

// The body shelled `t` inside with the face `removed` open, Sharp or
// Round: a valid single solid whose inner faces lie `t` from its outer
// ones, with no note, the body left as it was. Returns its volume, or -1.
double check_shell(const char* what, const g::ShapePtr& body, const std::string& removed, double t, bool rounded) {
  const double before = volume_of(body->occt());
  g::ShellSpec spec;
  spec.faces = {removed};
  spec.inside = t;
  spec.rounded = rounded;
  g::ShapePtr result;
  try {
    result = g::shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what, error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return -1.0;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(count(result->occt(), TopAbs_SOLID) == 1);
  CHECK(result->notes().empty());
  CHECK(near(volume_of(body->occt()), before, 1e-9));
  std::vector<TopoDS_Face> outer;
  std::vector<TopoDS_Face> inner;
  int rims = 0;
  double kept_area = 0.0;
  for (int i = 0; i < result->face_count(); ++i) {
    bool offset = false;
    bool cap = false;
    for (const std::string& name : result->face_names(i)) {
      offset = offset || name.rfind("F4:offset(", 0) == 0;
      cap = cap || name.rfind("F4:offset_cap(", 0) == 0;
    }
    if (cap) {
      ++rims;
    } else if (offset) {
      inner.push_back(result->face(i));
    } else {
      outer.push_back(result->face(i));
      kept_area += area_of(result->face(i));
    }
  }
  const double volume = volume_of(result->occt());
  std::printf("%s: %d outer, %d inner faces, %d rim faces, volume %.6f of the body's %.6f\n", what,
              static_cast<int>(outer.size()), static_cast<int>(inner.size()), rims, volume, before);
  CHECK(rims >= 1);
  CHECK(outer.size() == inner.size());
  check_wall(what, outer, inner, t);
  // About the kept faces' area times the wall: the curvature of the faces
  // and the corners at the kept end take some percent off (the slot's end
  // kept, its corners with the sides leaning in at the half circles, most).
  CHECK(volume > 0.8 * t * kept_area && volume < 1.02 * t * kept_area);
  return volume;
}

// mitcad#2141: the ticket's loft shelled 2 mm inside, the slot's end and
// then the circle's removed, Sharp and Round.
void test_shells_of_the_ticket_loft() {
  const g::ShapePtr body = g::loft(ticket_loft());
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(count(body->occt(), TopAbs_SOLID) == 1);
  check_tangent_sides("the ticket's loft", body->occt());
  for (const std::string& removed :
       {g::face_name("F3", "end", "r{c2,c3,c4,c5}"), g::face_name("F3", "start", "r{c1}")}) {
    CHECK(body->find_faces(removed).size() == 1);
    const std::string sharp_name = "the ticket's loft, " + removed + " removed, Sharp";
    const std::string round_name = "the ticket's loft, " + removed + " removed, Round";
    const double sharp_volume = check_shell(sharp_name.c_str(), body, removed, 2.0, false);
    const double round_volume = check_shell(round_name.c_str(), body, removed, 2.0, true);
    CHECK(sharp_volume > 0.0 && round_volume > 0.0);
    // (The kernel builds Round's wall otherwise: 1.1e-5 and 0.8e-5 apart
    // with the slot's and the circle's end open.)
    CHECK_NEAR_TOL(round_volume, sharp_volume, 1e-4);
  }
}

// Every side face of the loft between two section planes `height` apart
// is straight between them: the iso-curves of its surface that run from
// one plane to the other (at five places across the face, its ends on the
// planes within 1e-6) deviate from their chords by at most 1e-6 at 11
// points each.
void check_straight_rulings(const char* what, const TopoDS_Shape& loft, double height) {
  int faces = 0;
  double worst = 0.0;
  for (TopExp_Explorer it(loft, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (planar(face)) {
      continue;
    }
    ++faces;
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face, false);
    // The ruling runs along u or along v: the direction whose middle
    // iso-curve goes from one section plane to the other.
    const auto point = [&](bool by_v, double across, double along) {
      const double u = by_v ? u0 + (u1 - u0) * across : u0 + (u1 - u0) * along;
      const double v = by_v ? v0 + (v1 - v0) * along : v0 + (v1 - v0) * across;
      return surface.Value(u, v);
    };
    const double rise_v = std::abs(point(true, 0.5, 1.0).Z() - point(true, 0.5, 0.0).Z());
    const double rise_u = std::abs(point(false, 0.5, 1.0).Z() - point(false, 0.5, 0.0).Z());
    const bool along_v = rise_v >= rise_u;
    for (const double across : {0.0, 0.25, 0.5, 0.75, 1.0}) {
      const gp_Pnt a = point(along_v, across, 0.0);
      const gp_Pnt b = point(along_v, across, 1.0);
      CHECK_NEAR_TOL(std::abs(b.Z() - a.Z()), height, 1e-6 / height);
      if (a.Distance(b) < 1.0e-9) {
        CHECK(false);
        continue;
      }
      const gp_Lin chord(a, gp_Dir(gp_Vec(a, b)));
      for (int k = 1; k < 10; ++k) {
        worst = std::max(worst, chord.Distance(point(along_v, across, k / 10.0)));
      }
    }
  }
  std::printf("%s: %d side faces, their rulings at most %.3g from straight\n", what, faces, worst);
  CHECK(faces > 0);
  CHECK(worst <= 1.0e-6);
}

// mitcad#2191: the ticket's loft ruled (as the `.solidcraft` import makes
// every loft) gets the eased faces of the smooth one, straight between the
// sections, and shells 2 mm inside with either end removed, Sharp and
// Round, as the smooth one does.
void test_shells_of_the_ruled_ticket_loft() {
  g::LoftSpec spec = ticket_loft();
  spec.ruled = true;
  const g::ShapePtr body = g::loft(spec);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(count(body->occt(), TopAbs_SOLID) == 1);
  check_tangent_sides("the ruled ticket's loft", body->occt());
  check_straight_rulings("the ruled ticket's loft", body->occt(), 50.0);
  CHECK_NEAR_TOL(volume_of(body->occt()), volume_of(g::loft(ticket_loft())->occt()), 1e-9);
  for (const std::string& removed :
       {g::face_name("F3", "end", "r{c2,c3,c4,c5}"), g::face_name("F3", "start", "r{c1}")}) {
    CHECK(body->find_faces(removed).size() == 1);
    const std::string sharp_name = "the ruled ticket's loft, " + removed + " removed, Sharp";
    const std::string round_name = "the ruled ticket's loft, " + removed + " removed, Round";
    const double sharp_volume = check_shell(sharp_name.c_str(), body, removed, 2.0, false);
    const double round_volume = check_shell(round_name.c_str(), body, removed, 2.0, true);
    CHECK(sharp_volume > 0.0 && round_volume > 0.0);
    CHECK_NEAR_TOL(round_volume, sharp_volume, 1e-4);
  }
}

// mitcad#2141, the owner's case: the slot first, its end removed, 2 mm
// inside, Sharp.
void test_shell_of_the_owner_loft() {
  const g::ShapePtr body = g::loft(owner_loft(true));
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  check_tangent_sides("the owner's loft", body->occt());
  const std::string removed = g::face_name("F8", "start", "r{c2,c3,c4,c5}");
  CHECK(body->find_faces(removed).size() == 1);
  CHECK(check_shell("the owner's loft, the slot's end removed, Sharp", body, removed, 2.0, false) > 0.0);
}

// The surface thickened by `thickness` (to the other side when negative,
// half each side with `symmetric`): a valid single solid. Returns its
// volume, or -1.
double check_thicken(const char* what, const g::ShapePtr& sheet, double thickness, bool symmetric) {
  const double area = area_of(sheet->occt());
  g::ShapePtr thick;
  try {
    thick = g::thicken("F9", *sheet, thickness, symmetric);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what, error.what());
  }
  CHECK(thick != nullptr);
  if (!thick) {
    return -1.0;
  }
  CHECK(BRepCheck_Analyzer(thick->occt()).IsValid());
  CHECK(count(thick->occt(), TopAbs_SOLID) == 1);
  CHECK(near(area_of(sheet->occt()), area, 1e-9));
  const double volume = volume_of(thick->occt());
  std::printf("%s: volume %.6f, the area %.6f times the thickness\n", what, volume, area);
  return volume;
}

// mitcad#2143: the owner's surface loft (the circle first) thickened 1 mm
// to each side and symmetric. The volume between a surface of area A and
// its offset by t is t A + t^2 Int H + t^3 / 3 Int K (H the mean, K the
// Gaussian curvature): the two sides' volumes sum to 2 t A + 2/3 t^3 Int K,
// symmetric gives t A + t^3 / 12 Int K, the t^3 terms a few mm^3 here (a
// tube's Int K is a few times pi). One side alone differs from t A by
// t^2 Int H, under 1 % here.
void test_thickened_surface_loft() {
  g::LoftSpec spec = owner_loft(false);
  spec.solid = false;
  const g::ShapePtr sheet = g::loft(spec);
  CHECK(count(sheet->occt(), TopAbs_SOLID) == 0);
  check_tangent_sides("the owner's surface loft", sheet->occt());
  const double area = area_of(sheet->occt());
  const double front = check_thicken("thickened 1 mm along the normals", sheet, 1.0, false);
  const double back = check_thicken("thickened 1 mm against the normals", sheet, -1.0, false);
  const double both = check_thicken("thickened 1 mm symmetric", sheet, 1.0, true);
  CHECK(front > 0.0 && back > 0.0 && both > 0.0);
  CHECK_NEAR_TOL(front + back, 2.0 * area, 1e-3);
  CHECK_NEAR_TOL(both, area, 1e-3);
  CHECK_NEAR_TOL(front, area, 0.015);
  CHECK_NEAR_TOL(back, area, 0.015);
}

// Lofts the change leaves as they were: a square to a circle (the square's
// corners meet the faces at angles; the circle's seam halves a side, where
// both run on smoothly but the two halves' faces are already tangent) is
// OCCT's through-sections loft of the same wires, to the volume.
void test_lofts_with_corners_unchanged() {
  g::Frame bottom;
  g::Frame top;
  top.origin = gp_Pnt(0, 0, 40);
  g::Region square;
  square.name = "r{c2,c3,c4,c5}";
  g::Loop loop;
  const double corners[4][2] = {{-10, -10}, {10, -10}, {10, 10}, {-10, 10}};
  const char* names[4] = {"c2", "c3", "c4", "c5"};
  for (int i = 0; i < 4; ++i) {
    g::Segment segment;
    segment.name = names[i];
    segment.kind = g::SegmentKind::Line;
    segment.start = gp_Pnt2d(corners[i][0], corners[i][1]);
    segment.end = gp_Pnt2d(corners[(i + 1) % 4][0], corners[(i + 1) % 4][1]);
    loop.segments.push_back(segment);
  }
  square.loops.push_back(loop);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(bottom, square), section(top, circle(10))};
  const g::ShapePtr body = g::loft(spec);
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  BRepOffsetAPI_ThruSections occt(true, false, 1.0e-6);
  occt.CheckCompatibility(true);
  occt.AddWire(BRepTools::OuterWire(g::make_profile(bottom, square).face));
  occt.AddWire(BRepTools::OuterWire(g::make_profile(top, circle(10)).face));
  occt.Build();
  CHECK(occt.IsDone());
  if (occt.IsDone()) {
    CHECK_NEAR_TOL(volume_of(body->occt()), volume_of(occt.Shape()), 1e-9);
  }
}

// The ticket's loft against OCCT's through-sections loft of the same wires
// (faces meeting at creases): printed, not checked; the eased faces run
// between the same edges by other rulings.
void test_ticket_loft_against_occt() {
  const g::LoftSpec spec = ticket_loft();
  BRepOffsetAPI_ThruSections occt(true, false, 1.0e-6);
  occt.CheckCompatibility(true);
  for (const g::LoftSection& s : spec.sections) {
    occt.AddWire(BRepTools::OuterWire(g::make_profile(s.frame, s.region).face));
  }
  occt.Build();
  CHECK(occt.IsDone());
  if (occt.IsDone()) {
    std::printf("the ticket's loft: volume %.6f, OCCT's through the same sections %.6f\n",
                volume_of(g::loft(spec)->occt()), volume_of(occt.Shape()));
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(loft_circle_slot) {
  test::guarded("test_shells_of_the_ticket_loft", test::test_shells_of_the_ticket_loft);
  test::guarded("test_shell_of_the_owner_loft", test::test_shell_of_the_owner_loft);
  test::guarded("test_shells_of_the_ruled_ticket_loft", test::test_shells_of_the_ruled_ticket_loft);
  test::guarded("test_thickened_surface_loft", test::test_thickened_surface_loft);
  test::guarded("test_lofts_with_corners_unchanged", test::test_lofts_with_corners_unchanged);
  test::guarded("test_ticket_loft_against_occt", test::test_ticket_loft_against_occt);
  if (test::failures == 0) {
    std::puts("test_loft_circle_slot: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
