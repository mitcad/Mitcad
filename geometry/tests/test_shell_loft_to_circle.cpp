// SPDX-License-Identifier: MIT
// Lofts from a polygon to a circle shelled with both ends open, and offset
// inwards closed (mitcad#1184), with OCCT alone and through the geometry
// facade. OCCT's loft through the two sections (ThruSections) is ruled: at
// the polygon's corners its faces meet at creases that flatten to tangency
// at the circle. Where the walls lean out towards the circle, the offsets of
// two faces at a corner become tangent just past the circle's end and cross
// there; the intersection of the enlarged offset faces was the branch along
// the corner and pieces about and past the tangency, all connected, and the
// inner walls could not be rebuilt from them: the thick solid failed or came
// out invalid, the closed offset null or invalid, at every thickness. With
// the port's patch 1130 only the branch along the corner is kept there.
// Where the walls lean in towards the circle (mitcad#1193), the offsets
// become tangent inside the solid, and past the tangency the walls extended
// to the removed end cross along the branch's continuation and two
// sideways branches; with the port's patch 1190 the branch and its
// continuation make one edge. The shells are valid single solids with both
// ends open (each end's rim an annulus), their inner walls the thickness
// away from the outer ones.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakeOffsetShape.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

// The octagon of the report: sides on x = 17 and 47, y = -15 and 15, and
// the 45 degree sides between, at z = 72; the circle about (32, 0) at
// z = 228.
const double kOctagon[8][2] = {{47.0, 6.213203435596426},  {47.0, -6.213203435596426},
                               {38.21320343559643, -15.0}, {25.786796564403573, -15.0},
                               {17.0, -6.213203435596426}, {17.0, 6.213203435596426},
                               {25.786796564403573, 15.0}, {38.21320343559643, 15.0}};
constexpr double kBottom = 72.0;
constexpr double kTop    = 228.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

TopoDS_Wire circle_wire(const gp_Pnt& centre, double radius, double seam) {
  const gp_Ax2 axes(centre, gp_Dir(0, 0, 1), gp_Dir(std::cos(seam), std::sin(seam), 0));
  return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(gp_Circ(axes, radius)).Edge()).Wire();
}

TopoDS_Shape thru(const TopoDS_Wire& bottom, const TopoDS_Wire& top) {
  BRepOffsetAPI_ThruSections maker(true, false, 1.0e-6);
  maker.CheckCompatibility(true);
  maker.AddWire(bottom);
  maker.AddWire(top);
  maker.Build();
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The octagon to the circle of `radius`, its seam at `seam` radians.
TopoDS_Shape octagon_loft(double radius, double seam) {
  BRepBuilderAPI_MakePolygon octagon;
  for (const auto& p : kOctagon) {
    octagon.Add(gp_Pnt(p[0], p[1], kBottom));
  }
  octagon.Close();
  return thru(octagon.Wire(), circle_wire(gp_Pnt(32, 0, kTop), radius, seam));
}

// A 20 mm square at z = 0 to a circle of `radius` at z = 40.
TopoDS_Shape square_loft(double radius) {
  BRepBuilderAPI_MakePolygon square(gp_Pnt(-10, -10, 0), gp_Pnt(10, -10, 0), gp_Pnt(10, 10, 0),
                                    gp_Pnt(-10, 10, 0), true);
  return thru(square.Wire(), circle_wire(gp_Pnt(0, 0, 40), radius, kPi / 4));
}

bool is_cap(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face);
  return surface.GetType() == GeomAbs_Plane &&
         std::abs(std::abs(surface.Plane().Axis().Direction().Z()) - 1.0) < 1e-9;
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The wall of a shell or an offset: `inner` faces `t` from the `outer` side
// faces (at each inner face's middle, within 1e-6 mm).
void check_wall(const char* name, const std::vector<TopoDS_Face>& outer, const std::vector<TopoDS_Face>& inner,
                double t) {
  BRep_Builder    builder;
  TopoDS_Compound sides;
  builder.MakeCompound(sides);
  for (const TopoDS_Face& face : outer) {
    builder.Add(sides, face);
  }
  CHECK(!inner.empty());
  for (const TopoDS_Face& face : inner) {
    double u0, u1, v0, v1;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const gp_Pnt p = BRepAdaptor_Surface(face).Value(0.5 * (u0 + u1), 0.5 * (v0 + v1));
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), sides);
    CHECK(distance.IsDone());
    if (distance.IsDone() && std::abs(distance.Value() - t) > 1e-6) {
      std::printf("%s: an inner face lies %.9f from the outer walls, not %g\n", name, distance.Value(), t);
      CHECK(false);
    }
  }
}

// Shells `body` inwards by t with its two ends removed, with OCCT alone.
void check_thick_solid(const char* name, const TopoDS_Shape& body, double t) {
  NCollection_List<TopoDS_Shape> closing;
  std::vector<TopoDS_Face>       sides;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    if (is_cap(TopoDS::Face(f.Current()))) {
      closing.Append(f.Current());
    } else {
      sides.push_back(TopoDS::Face(f.Current()));
    }
  }
  CHECK(closing.Extent() == 2);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, closing, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    std::printf("%s: not done\n", name);
    return;
  }
  const TopoDS_Shape result = maker.Shape();
  const bool         valid  = BRepCheck_Analyzer(result).IsValid();
  CHECK(valid);
  CHECK(count(result, TopAbs_SOLID) == 1);
  const double volume = volume_of(result);
  std::printf("%s: valid %d, volume %.6f of the body's %.6f\n", name, (int)valid, volume, volume_of(body));
  // (A hollow, well below the body: a 3 mm wall of the smaller lofts holds
  // more than half of it.)
  CHECK(volume > 0.0 && volume < 0.75 * volume_of(body));
  // Both ends open: each end is a rim, a plane face with a hole.
  std::vector<TopoDS_Face> inner;
  int                      rims = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    if (is_cap(face)) {
      CHECK(count(face, TopAbs_WIRE) == 2);
      ++rims;
      continue;
    }
    bool own = false;
    for (const TopoDS_Face& side : sides) {
      own = own || side.IsSame(face);
    }
    if (!own) {
      inner.push_back(face);
    }
  }
  CHECK(rims == 2);
  check_wall(name, sides, inner, t);
}

void test_occt_shells_of_lofts_to_a_circle() {
  for (const double radius : {24.5, 21.7}) {
    for (const double seam : {0.0, 22.5}) {
      for (const double t : {0.5, 1.0, 2.0, 3.0}) {
        char name[96];
        std::snprintf(name, sizeof name, "octagon to a circle of radius %.1f, seam at %.1f, shell %.1f", radius,
                      seam, t);
        check_thick_solid(name, octagon_loft(radius, seam * kPi / 180.0), t);
      }
    }
  }
  for (const double t : {0.5, 1.0, 2.0}) {
    char name[96];
    std::snprintf(name, sizeof name, "square to a circle of radius 20, shell %.1f", t);
    check_thick_solid(name, square_loft(20.0), t);
  }
  // Walls leaning in towards the circle (mitcad#1193): a circle inside the
  // square, one touching its sides, and one through (just inside) its
  // corners, where the offsets become tangent on the circle's end.
  for (const double radius : {8.0, 10.0, 14.1}) {
    for (const double t : {0.5, 1.0, 2.0, 3.0}) {
      char name[96];
      std::snprintf(name, sizeof name, "square to a circle of radius %.1f, shell %.1f", radius, t);
      check_thick_solid(name, square_loft(radius), t);
    }
  }
}

// An accurate volume (OCCT's default integration is off by tenths of a
// percent on these bodies).
double accurate_volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return props.Mass();
}

// The shell of `body` inwards by t with the caps at `ends` removed
// (0 the bottom, 1 the top, 2 both), or null.
TopoDS_Shape thick_solid_of(const TopoDS_Shape& body, double t, int ends) {
  NCollection_List<TopoDS_Shape> closing;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    if (is_cap(face)) {
      const bool top = BRepAdaptor_Surface(face).Plane().Location().Z() > 1.0;
      if (ends == 2 || (ends == 1) == top) {
        closing.Append(face);
      }
    }
  }
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(body, closing, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure&) {
    return TopoDS_Shape();
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// Walls leaning in: the hollow of the shell with both ends open is the
// closed offset's with the two end slabs that the shells with one end open
// add to it, so its volume is theirs: V(both) = V(top) + V(bottom) -
// V(body) + V(closed offset). The one-end shells and the closed offset are
// valid too.
void test_occt_lean_in_volumes() {
  const double cases[3][2] = {{8.0, 3.0}, {10.0, 1.0}, {14.1, 2.0}};
  for (const auto& c : cases) {
    const TopoDS_Shape body = square_loft(c[0]);
    const double       t    = c[1];
    const TopoDS_Shape both = thick_solid_of(body, t, 2), bottom = thick_solid_of(body, t, 0),
                       top = thick_solid_of(body, t, 1);
    BRepOffsetAPI_MakeOffsetShape closed;
    try {
      OCC_CATCH_SIGNALS
      closed.PerformByJoin(body, -t, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
    } catch (const Standard_Failure& error) {
      std::printf("the closed offset of radius %.1f: %s\n", c[0], error.what());
    }
    CHECK(!both.IsNull() && !bottom.IsNull() && !top.IsNull() && closed.IsDone() && !closed.Shape().IsNull());
    if (both.IsNull() || bottom.IsNull() || top.IsNull() || !closed.IsDone() || closed.Shape().IsNull()) {
      std::printf("radius %.1f, shell %.1f: a shell or the closed offset failed\n", c[0], t);
      continue;
    }
    for (const TopoDS_Shape* shape : {&both, &bottom, &top, &closed.Shape()}) {
      CHECK(BRepCheck_Analyzer(*shape).IsValid());
    }
    const double expected = accurate_volume_of(top) + accurate_volume_of(bottom) - accurate_volume_of(body) +
                            accurate_volume_of(closed.Shape());
    const double volume = accurate_volume_of(both);
    std::printf("radius %.1f, shell %.1f: volume %.6f, from the one-end shells and the closed offset %.6f\n", c[0],
                t, volume, expected);
    CHECK(std::abs(volume - expected) <= 0.1);
  }
}

void test_occt_closed_offset_of_a_loft_to_a_circle() {
  for (const double seam : {0.0, 22.5}) {
    const TopoDS_Shape body = octagon_loft(24.5, seam * kPi / 180.0);
    BRepOffsetAPI_MakeOffsetShape maker;
    try {
      OCC_CATCH_SIGNALS
      maker.PerformByJoin(body, -3.0, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
    } catch (const Standard_Failure& error) {
      std::printf("the closed offset: %s\n", error.what());
    }
    CHECK(maker.IsDone() && !maker.Shape().IsNull());
    if (!maker.IsDone() || maker.Shape().IsNull()) {
      continue;
    }
    const TopoDS_Shape result = maker.Shape();
    CHECK(BRepCheck_Analyzer(result).IsValid());
    std::printf("the closed offset, seam at %.1f: volume %.6f of the body's %.6f\n", seam, volume_of(result),
                volume_of(body));
    CHECK(volume_of(result) > 0.5 * volume_of(body) && volume_of(result) < volume_of(body));
    std::vector<TopoDS_Face> outer, inner;
    for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
      if (!is_cap(TopoDS::Face(f.Current()))) {
        outer.push_back(TopoDS::Face(f.Current()));
      }
    }
    for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
      if (!is_cap(TopoDS::Face(f.Current()))) {
        inner.push_back(TopoDS::Face(f.Current()));
      }
    }
    check_wall("the closed offset", outer, inner, 3.0);
  }
}

// Through the geometry facade, as the model builds it: the loft of the
// polygon's region at `bottom_z` to the circle's at `top_z`, shelled `t`
// inwards with both its ends removed. The input is left as it was.
void check_facade_shell(const char* what, const std::vector<gp_Pnt2d>& polygon, double bottom_z,
                        const gp_Pnt2d& centre, double radius, double top_z, double t) {
  using namespace mitcad::geometry;
  const int   n = static_cast<int>(polygon.size());
  Region      outline;
  Loop        loop;
  std::string key = "r{";
  for (int i = 0; i < n; ++i) {
    const auto curve = [n](int k) { return "c" + std::to_string((k + n) % n + 9); };
    Segment    segment;
    segment.kind  = SegmentKind::Line;
    segment.start = polygon[static_cast<std::size_t>(i)];
    segment.end   = polygon[static_cast<std::size_t>((i + 1) % n)];
    segment.name  = curve(i) + "[" + curve(i - 1) + "," + curve(i + 1) + "]";
    key += (i == 0 ? "" : ",") + segment.name;
    loop.segments.push_back(segment);
  }
  outline.loops.push_back(loop);
  outline.name                     = key + "}";
  Region circle                    = circle_region(centre.X(), centre.Y(), radius);
  circle.loops[0].segments[0].name = "c2";
  circle.name                      = "r{c2}";
  LoftSpec spec;
  spec.feature = "F19";
  LoftSection bottom, top;
  bottom.frame.origin = gp_Pnt(0, 0, bottom_z);
  bottom.region       = outline;
  top.frame.origin    = gp_Pnt(0, 0, top_z);
  top.region          = circle;
  spec.sections       = {bottom, top};
  const ShapePtr body   = loft(spec);
  const double   before = volume_of(body->occt());
  ShellSpec      shelled;
  shelled.faces  = {face_name("F19", "start", outline.name), face_name("F19", "end", circle.name)};
  shelled.inside = t;
  ShapePtr result;
  try {
    result = shell("F20", *body, shelled);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what, error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(count(result->occt(), TopAbs_SOLID) == 1);
  CHECK(std::abs(volume_of(body->occt()) - before) <= 1e-6 * before);
  std::vector<TopoDS_Face> outer, inner;
  int                      rims = 0;
  for (int i = 0; i < result->face_count(); ++i) {
    const TopoDS_Face& face   = result->face(i);
    bool               offset = false;
    for (const std::string& name : result->face_names(i)) {
      offset = offset || name.rfind("F20:offset(", 0) == 0;
    }
    if (is_cap(face)) {
      CHECK(count(face, TopAbs_WIRE) == 2);
      ++rims;
    } else if (offset) {
      inner.push_back(face);
    } else {
      outer.push_back(face);
    }
  }
  std::printf("%s: %d outer, %d inner faces, %d rims, volume %.6f\n", what, (int)outer.size(), (int)inner.size(),
              rims, volume_of(result->occt()));
  CHECK(rims == 2);
  CHECK(outer.size() == inner.size());
  check_wall(what, outer, inner, t);
}

// The reported loft: the octagon to the circle, shelled 3 mm.
void test_facade_shell_of_the_reported_loft() {
  std::vector<gp_Pnt2d> octagon;
  for (const auto& p : kOctagon) {
    octagon.emplace_back(p[0], p[1]);
  }
  check_facade_shell("the facade's shell", octagon, kBottom, gp_Pnt2d(32, 0), 24.5, kTop, 3.0);
}

// A loft whose walls lean in towards the circle (mitcad#1193): the 20 mm
// square to a circle of radius 10, shelled 1 mm.
void test_facade_shell_of_a_loft_leaning_in() {
  const std::vector<gp_Pnt2d> square = {gp_Pnt2d(-10, -10), gp_Pnt2d(10, -10), gp_Pnt2d(10, 10),
                                        gp_Pnt2d(-10, 10)};
  check_facade_shell("the facade's shell leaning in", square, 0.0, gp_Pnt2d(0, 0), 10.0, 40.0, 1.0);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_occt_shells_of_lofts_to_a_circle", test::test_occt_shells_of_lofts_to_a_circle);
  test::guarded("test_occt_closed_offset_of_a_loft_to_a_circle", test::test_occt_closed_offset_of_a_loft_to_a_circle);
  test::guarded("test_occt_lean_in_volumes", test::test_occt_lean_in_volumes);
  test::guarded("test_facade_shell_of_the_reported_loft", test::test_facade_shell_of_the_reported_loft);
  test::guarded("test_facade_shell_of_a_loft_leaning_in", test::test_facade_shell_of_a_loft_leaning_in);
  if (test::failures == 0) {
    std::puts("test_shell_loft_to_circle: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
