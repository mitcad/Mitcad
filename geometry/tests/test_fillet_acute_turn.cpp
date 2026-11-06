// SPDX-License-Identifier: MIT
// A concave rounding along an acute corner whose edge turns towards the ball
// (mitcad#1396), with OCCT's fillet alone. A plate and a rail standing on it,
// the rail's foot rounded with 2, run into an end wall standing at 45 degrees
// to the rail; the wall's foot along the plate, the rail's foot rounding and
// the rail's side is rounded with R. The rounding's edge on the wall turns
// round the foot rounding (an ellipse whose radius of curvature is 1.41 at
// the rail's side), while the ball, in the acute corner, lies 2.6 R from it:
// the walk's sections, square to the edge, turned back past the ball, and the
// walk over the foot rounding failed at once (the port's patch 1440 walks it
// along the balls' centres, third_party/vcpkg-ports).
//
// The blend over the foot rounding is the rolling ball's: every point of it
// lies R from the path of the ball's centre (the wall's offset plane cut by
// the foot rounding's axis cylinder of radius 2 - R), at radii 1, 1.5 and
// 1.9.
//
// With MITCAD_GEOMETRY_CASES set, the case `acute-concave-turn`: `body.brep`,
// the reported design's stored body before its rounding (its end wall's foot
// along a plate and two rails, both sides of a mirror, rounded with 1.5), and
// `after.brep`, the stored body after it: the result lies on the stored body
// and the stored body on the result within 0.01 mm.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

using EdgeFaces = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The plate (0 <= x <= 26.5, 0 <= z <= 3) and the rail on it (23.5 <= x <=
// 26.5, up to z = 43), the rail's foot rounded with 2 (its axis x = 21.5, z
// = 5), run along y into the end wall y >= x + kWall.
constexpr double kPlate = 3.0;
constexpr double kRailIn = 23.5;
constexpr double kRailOut = 26.5;
constexpr double kFoot = 2.0;
constexpr double kTop = 43.0;
constexpr double kWall = 117.272078;
constexpr double kAxisX = kRailIn - kFoot;
constexpr double kAxisZ = kPlate + kFoot;

TopoDS_Edge segment(const gp_Pnt& a, const gp_Pnt& b) {
  return BRepBuilderAPI_MakeEdge(GC_MakeSegment(a, b).Value()).Edge();
}

TopoDS_Shape body() {
  // The cross-section in the plane y = 0, extruded along y.
  const double            s = std::sqrt(0.5);
  const gp_Pnt            p0(0., 0., 0.), p1(kRailOut, 0., 0.), p2(kRailOut, 0., kTop), p3(kRailIn, 0., kTop);
  const gp_Pnt            p4(kRailIn, 0., kAxisZ), p5(kAxisX, 0., kPlate), p6(0., 0., kPlate);
  const gp_Pnt            pm(kAxisX + kFoot * s, 0., kAxisZ - kFoot * s);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(segment(p0, p1));
  wire.Add(segment(p1, p2));
  wire.Add(segment(p2, p3));
  wire.Add(segment(p3, p4));
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(p4, pm, p5).Value()).Edge());
  wire.Add(segment(p5, p6));
  wire.Add(segment(p6, p0));
  const TopoDS_Face  section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape rail    = BRepPrimAPI_MakePrism(section, gp_Vec(0., 160., 0.)).Shape();
  // The end wall: above the plane y = x + kWall over the plate and the rail.
  BRepBuilderAPI_MakePolygon plan;
  plan.Add(gp_Pnt(0., kWall, 0.));
  plan.Add(gp_Pnt(kRailOut, kWall + kRailOut, 0.));
  plan.Add(gp_Pnt(kRailOut, kWall + 60., 0.));
  plan.Add(gp_Pnt(0., kWall + 60., 0.));
  plan.Close();
  const TopoDS_Face  foot = BRepBuilderAPI_MakeFace(plan.Wire(), true).Face();
  const TopoDS_Shape wall = BRepPrimAPI_MakePrism(foot, gp_Vec(0., 0., kTop)).Shape();
  BRepAlgoAPI_Fuse   fuse(rail, wall);
  if (!fuse.IsDone()) {
    return TopoDS_Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The edges of the wall (the plane x - y + kWall = 0) with the plate's top,
// the foot rounding and the rail's inner side.
std::vector<TopoDS_Edge> chain_edges(const TopoDS_Shape& shape) {
  EdgeFaces map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  std::vector<TopoDS_Edge> out;
  for (int i = 1; i <= map.Extent(); ++i) {
    if (map(i).Extent() != 2) {
      continue;
    }
    bool wall = false, other = false;
    for (NCollection_List<TopoDS_Shape>::Iterator it(map(i)); it.More(); it.Next()) {
      const BRepAdaptor_Surface s(TopoDS::Face(it.Value()), false);
      if (s.GetType() == GeomAbs_Plane) {
        const gp_Pln p = s.Plane();
        const gp_Dir n = p.Axis().Direction();
        if (std::abs(std::abs(n.X()) - std::sqrt(0.5)) < 1e-9 && std::abs(n.Z()) < 1e-9) {
          wall = true;
        } else if (std::abs(std::abs(n.Z()) - 1.) < 1e-9 && std::abs(p.Location().Z() - kPlate) < 1e-9) {
          other = true;
        } else if (std::abs(std::abs(n.X()) - 1.) < 1e-9 && std::abs(p.Location().X() - kRailIn) < 1e-9) {
          other = true;
        }
      } else if (s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - kFoot) < 1e-9) {
        other = true;
      }
    }
    if (wall && other) {
      out.push_back(TopoDS::Edge(map.FindKey(i)));
    }
  }
  return out;
}

// The fillet of the edges with one radius, as MitCad's dressup calls it.
TopoDS_Shape round_edges(const TopoDS_Shape& shape, const std::vector<TopoDS_Edge>& edges, double radius) {
  BRepFilletAPI_MakeFillet                               fillet(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : edges) {
    selected.Add(e);
  }
  fillet.StopChainsAtTurns(selected);
  for (const TopoDS_Edge& e : edges) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(radius, e);
    }
  }
  fillet.CarryChainsAcrossNotches();
  try {
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours\n", fillet.NbFaultyContours());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// Whether every edge bounds two faces (a seam its face twice).
bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const TopAbs_Orientation o = e.Current().Orientation();
      if (BRep_Tool::Degenerated(TopoDS::Edge(e.Current())) || o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL) {
        continue;
      }
      const int i = uses.FindIndex(e.Current());
      if (i == 0) {
        uses.Add(e.Current(), 1);
      } else {
        ++uses(i);
      }
    }
  }
  for (int i = 1; i <= uses.Extent(); ++i) {
    if (uses(i) != 2) {
      return false;
    }
  }
  return true;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The ball's centre over the foot rounding at angle a (0 at the plate, pi/2
// at the rail's side): 2 - R from the rounding's axis, R off the wall.
gp_Pnt centre(double radius, double a) {
  const double x = kAxisX + (kFoot - radius) * std::sin(a);
  const double z = kAxisZ - (kFoot - radius) * std::cos(a);
  // (R along the wall's normal (1, -1, 0) / sqrt 2 off the wall y = x + kWall.)
  return gp_Pnt(x, x + kWall - radius * std::sqrt(2.), z);
}

// The distance from a point to the path of the ball's centre over the foot
// rounding (and a little past its ends).
double off_centre_path(double radius, const gp_Pnt& p) {
  double best = 1e100;
  int    k    = 0;
  for (int i = 0; i <= 400; ++i) {
    const double d = p.Distance(centre(radius, (-0.05 + 1.1 * i / 400.) * kPi / 2.));
    if (d < best) {
      best = d;
      k    = i;
    }
  }
  double a = (-0.05 + 1.1 * (k - 1) / 400.) * kPi / 2., b = (-0.05 + 1.1 * (k + 1) / 400.) * kPi / 2.;
  for (int it = 0; it < 100; ++it) {
    const double c1 = b - 0.618034 * (b - a), c2 = a + 0.618034 * (b - a);
    if (p.Distance(centre(radius, c1)) < p.Distance(centre(radius, c2))) {
      b = c2;
    } else {
      a = c1;
    }
  }
  return p.Distance(centre(radius, 0.5 * (a + b)));
}

void check_radius(const TopoDS_Shape& shape, double radius) {
  const std::vector<TopoDS_Edge> edges = chain_edges(shape);
  CHECK(edges.size() == 3);
  const TopoDS_Shape result = round_edges(shape, edges, radius);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  CHECK(volume_of(result) > volume_of(shape));
  // The blend over the foot rounding: its points R from the centre's path.
  int    walked = 0;
  double worst  = 0.;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face         face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface s(face);
    if (s.GetType() != GeomAbs_BSplineSurface) {
      continue;
    }
    ++walked;
    double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const gp_Pnt p = s.Value(u0 + (u1 - u0) * i / 10., v0 + (v1 - v0) * j / 10.);
        worst          = std::max(worst, std::abs(off_centre_path(radius, p) - radius));
      }
    }
  }
  std::printf("radius %.2f: %d walked faces, their points off the ball by %.2e mm at most; material added %.6f mm3\n",
              radius,
              walked,
              worst,
              volume_of(result) - volume_of(shape));
  CHECK(walked >= 1);
  CHECK(worst <= 1e-4);
}

void test_rounding_round_the_foot() {
  const TopoDS_Shape shape = body();
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  for (const double radius : {1.0, 1.5, 1.9}) {
    check_radius(shape, radius);
  }
}

std::string geometry_cases() {
#ifdef _MSC_VER
  char*       value  = nullptr;
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

// The faces of a shape whose box meets the region.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder    builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (!box.IsOut(region)) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` within the region from
// the faces of `to` about it.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.);
  const TopoDS_Compound target = faces_in(to, wider);
  double                worst  = 0.;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        if (region.IsOut(p)) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

// The edges of the stored body's end walls (the planes through the line x =
// 0, y = kWall at 45 degrees) with the plate's top, the rails' foot
// roundings, sides and top roundings, and the rails' tops: both chains.
std::vector<TopoDS_Edge> reported_chain(const TopoDS_Shape& shape) {
  EdgeFaces map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  std::vector<TopoDS_Edge> out;
  for (int i = 1; i <= map.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(map.FindKey(i));
    if (map(i).Extent() != 2 || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    bool wall = false;
    for (NCollection_List<TopoDS_Shape>::Iterator it(map(i)); it.More(); it.Next()) {
      const BRepAdaptor_Surface s(TopoDS::Face(it.Value()), false);
      if (s.GetType() == GeomAbs_Plane) {
        const gp_Dir n = s.Plane().Axis().Direction();
        const gp_Pnt o = s.Plane().Location();
        wall = wall
               || (std::abs(std::abs(n.X()) - std::sqrt(0.5)) < 1e-9 && std::abs(n.Z()) < 1e-9
                   && std::abs(std::abs(o.Y() - kWall) - std::abs(o.X())) < 1e-4);
      }
    }
    // (Below the rails' tops and inside the rails' outer sides, but for the
    // outer sides' own edges down to the foot: the chain's seven edges a side.)
    Bnd_Box box;
    BRepBndLib::Add(edge, box);
    double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (wall && z1 < 23.1 && std::max(std::abs(x0), std::abs(x1)) < 26.6 && z1 > 2.9) {
      out.push_back(edge);
    }
  }
  return out;
}

void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/acute-concave-turn/";
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> edges = reported_chain(shape);
  std::printf("the chains: %d edges\n", static_cast<int>(edges.size()));
  CHECK(edges.size() == 14);
  const TopoDS_Shape result = round_edges(shape, edges, 1.5);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  for (const double side : {1., -1.}) {
    // About the turn round the foot rounding.
    Bnd_Box region;
    region.Update(side > 0. ? 19.5 : -24.5, 135.5, 2.5, side > 0. ? 24.5 : -19.5, 141.5, 6.5);
    const double out  = farthest(result, after, region);
    const double back = farthest(after, result, region);
    std::printf("about the foot at x %s 0: the result on the stored body within %.6f mm, the stored body on it "
                "within %.6f mm\n",
                side > 0. ? ">" : "<",
                out,
                back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
  // Along the whole chains.
  Bnd_Box region;
  region.Update(-28.5, 114., -0.5, 28.5, 146., 25.);
  const double out  = farthest(result, after, region);
  const double back = farthest(after, result, region);
  std::printf("along the chains: %.6f mm and %.6f mm\n", out, back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  const double added = volume_of(result) - volume_of(shape);
  const double file  = volume_of(after) - volume_of(shape);
  std::printf("the material added %.6f mm3, the stored body's %.6f\n", added, file);
  CHECK(std::abs(added - file) <= 0.01 * std::abs(file));
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_round_the_foot", test::test_rounding_round_the_foot);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_acute_turn: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
