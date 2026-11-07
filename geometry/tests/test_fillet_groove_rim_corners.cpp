// SPDX-License-Identifier: MIT
// The rim of a concave groove rounded with r past an earlier rounding of r
// and the corners where the rim meets a wall, with OCCT's fillet alone
// (mitcad#1428, the port's patches 1610 and 1611).
//
// A leg (a profile extruded along z): a flange on the left whose top y = 5
// meets a slanted side through a convex corner rounded with r = 0.5, the
// slanted side meeting the right side x = 3 through a convex corner rounded
// with 2, and above the flange a wall x = 0 up to the top. A groove, a
// cylinder of 12.5 along x, is cut across it. Its rim is rounded with r:
// the arc on the wall x = 0 is one chain; the rest (the flange's top, the
// rounding of r, the slanted side, the rounding of 2, the side x = 3 and
// back) is one tangent chain.
//
// Where the chain runs over the corner rounded with r, the ball's centre
// stays on that rounding's axis while its contact sweeps round it: the
// rounding is a piece of the ball's sphere (port patch 0670), and the walk
// over the slanted side goes on from the section where its rule (the
// contacts' mid-point in the guide's normal plane) puts that ball. The
// guide turns round the rounding of 2 further on, and its normal plane
// swings back through the ball there: that second section was taken (the
// last one in the gap), the walk started from it far down the guide and
// came out twisted (port patch 1610 takes only sections where the normal
// planes pass the ball forwards). Where the chain meets the wall's arc, the
// two roundings cross each other at a vertex whose third edge (the wall
// with the flange's top) is concave: the flange's rounding runs on into the
// wall's and both are cut along their intersection; the cut on the wall
// failed where the contact's circle on the wall had parameters before its
// first period, and the corner was filled with a patch 0.05 mm off the
// roundings (port patch 1611).
//
// Checks: valid and closed; every new face lies on the ball's envelope (the
// ball's centre seen from it 12.5 + r from the groove's axis within 1e-5
// mm); each corner rounded with r is a piece of the sphere about the
// point where its axis meets that envelope; the flange's rounding runs on
// past the wall to r beyond it.
//
// With MITCAD_GEOMETRY_CASES set, the case `groove-rim-corners`: `body.brep`,
// a reported design's stored body before the rounding (two such legs),
// and `after.brep`, the stored body after it. Its rims rounded as the design
// selects them: the result has the stored body's faces, lies on it and it on
// the result within 0.01 mm, and changes the volume as the stored body does
// within 5e-3 of it (the stored body's roundings over the earlier roundings
// of 2 are approximations whose edges lie up to 1.3e-3 mm off their faces,
// which puts the stored body's volume integrals off by about 0.01 mm3).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kRadius = 0.5;  // the rounding's radius and the flange corner's
constexpr double kGroove = 12.5; // the groove's radius, its axis along x at y = -5, z = 15
constexpr double kAxisY  = -5.;
constexpr double kAxisZ  = 15.;

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

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++n;
  }
  return n;
}

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

// The leg: the profile in the plane z = 0.5, extruded up to z = 29.5, the
// groove cut across it.
struct Leg {
  TopoDS_Shape body;
  double       cornerX = 0.; // the axis of the flange's corner rounding: x, y = 4.5
};

Leg leg() {
  Leg          out;
  const double z  = 0.5;
  const double s  = std::sqrt(34.); // the slanted side 5 x + 3 y = 0, its normal (5, 3) / s
  const gp_Vec n(5. / s, 3. / s, 0.);
  // The corner rounded with 2 between the slanted side and x = 3.
  const double c2 = (2. * s - 5.) / 3.;
  const gp_Pnt o2(1., c2, z);
  // The corner rounded with kRadius between the slanted side and y = 5.
  const double a5 = (kRadius * s - 13.5) / 5.;
  const gp_Pnt o5(a5, 5. - kRadius, z);
  out.cornerX = a5;

  const gp_Pnt A(0., 10., z), B(3., 10., z), C(3., c2, z);
  const gp_Pnt D = o2.Translated(-2. * n);
  const gp_Pnt E = o5.Translated(-kRadius * n);
  const gp_Pnt F(a5, 5., z), G(0., 5., z);
  const gp_Vec m2 = (gp_Vec(1., 0., 0.) - n).Normalized();
  const gp_Vec m5 = (gp_Vec(0., 1., 0.) - n).Normalized();

  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(A, B).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(B, C).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(C, o2.Translated(2. * m2), D).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(D, E).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(E, o5.Translated(kRadius * m5), F).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(F, G).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(G, A).Edge());
  const TopoDS_Face  profile = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape prism   = BRepPrimAPI_MakePrism(profile, gp_Vec(0., 0., 29.)).Shape();
  const TopoDS_Shape groove =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-10., kAxisY, kAxisZ), gp_Dir(1., 0., 0.)), kGroove, 30.).Shape();
  out.body = BRepAlgoAPI_Cut(prism, groove).Shape();
  return out;
}

bool is_groove(const TopoDS_Face& face) {
  const BRepAdaptor_Surface s(face);
  return s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - kGroove) < 1e-6
         && s.Cylinder().Axis().Direction().IsParallel(gp_Dir(1., 0., 0.), 1e-9);
}

// The edges of the groove's faces whose other face `select` takes.
template <class Select>
std::vector<TopoDS_Edge> rim_edges(const TopoDS_Shape& shape, Select select) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ef;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, ef);
  std::vector<TopoDS_Edge>                               out;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    if (!is_groove(face)) {
      continue;
    }
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(e.Current());
      if (BRep_Tool::Degenerated(edge) || seen.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ef.FindFromKey(edge)) {
        if (!other.IsSame(face) && select(TopoDS::Face(other))) {
          seen.Add(edge);
          out.push_back(edge);
          break;
        }
      }
    }
  }
  return out;
}

TopoDS_Shape round_edges(const TopoDS_Shape& shape, const std::vector<TopoDS_Edge>& edges) {
  BRepFilletAPI_MakeFillet                               fillet(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : edges) {
    selected.Add(e);
  }
  fillet.StopChainsAtTurns(selected);
  for (const TopoDS_Edge& e : edges) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(kRadius, e);
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
    std::printf("the rounding failed: %d faulty contours, %d faulty vertices\n",
                fillet.NbFaultyContours(),
                fillet.NbFaultyVertices());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// A point inside the face (the middle of its parameters, or the first of a
// grid that the face holds).
bool inner_point(const TopoDS_Face& face, gp_Pnt& p) {
  double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface s(face);
  for (int i = 0; i <= 8; ++i) {
    for (int j = 0; j <= 8; ++j) {
      const double u = u0 + (u1 - u0) * (0.5 + (i % 2 == 0 ? 1. : -1.) * 0.05 * i);
      const double v = v0 + (v1 - v0) * (0.5 + (j % 2 == 0 ? 1. : -1.) * 0.05 * j);
      if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() == TopAbs_IN) {
        p = s.Value(u, v);
        return true;
      }
    }
  }
  return false;
}

// The faces of the result that are not on the body before (a point inside
// each more than 1e-6 from it).
std::vector<TopoDS_Face> new_faces(const TopoDS_Shape& result, const TopoDS_Shape& body) {
  std::vector<TopoDS_Face> out;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    gp_Pnt            p;
    if (!inner_point(face, p)) {
      continue;
    }
    BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), body);
    if (d.IsDone() && d.Value() > 1e-6) {
      out.push_back(face);
    }
  }
  return out;
}

// How far the centres of the balls the face touches (each point less
// kRadius along its outward normal) lie off the groove's offset by kRadius,
// at most.
double off_the_envelope(const TopoDS_Face& face) {
  double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface s(face);
  double                    worst = 0.;
  for (int i = 0; i <= 8; ++i) {
    for (int j = 0; j <= 8; ++j) {
      const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 8.);
      const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 8.);
      if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
        continue;
      }
      gp_Pnt p;
      gp_Vec du, dv;
      s.D1(u, v, p, du, dv);
      gp_Vec normal = du.Crossed(dv);
      if (normal.Magnitude() < 1e-12) {
        continue;
      }
      normal.Normalize();
      if (face.Orientation() == TopAbs_REVERSED) {
        normal.Reverse();
      }
      const gp_Pnt c = p.Translated(-kRadius * normal);
      const double r = std::hypot(c.Y() - kAxisY, c.Z() - kAxisZ);
      worst          = std::max(worst, std::abs(r - (kGroove + kRadius)));
    }
  }
  return worst;
}

void test_synthetic_leg() {
  const Leg l = leg();
  CHECK(!l.body.IsNull() && BRepCheck_Analyzer(l.body).IsValid());
  // Every edge of the groove's face: the wall's arc, and the chain from the
  // flange's top round to the side x = 3 and back.
  const std::vector<TopoDS_Edge> edges = rim_edges(l.body, [](const TopoDS_Face&) { return true; });
  std::printf("%zu edges of the groove's rim\n", edges.size());
  CHECK(edges.size() >= 10u);
  const TopoDS_Shape result = round_edges(l.body, edges);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  const std::vector<TopoDS_Face> added = new_faces(result, l.body);
  std::printf("%zu new faces\n", added.size());
  CHECK(added.size() >= 10u);
  // Every new face on the ball's envelope.
  double worst = 0.;
  for (const TopoDS_Face& f : added) {
    worst = std::max(worst, off_the_envelope(f));
  }
  std::printf("the new faces' ball centres off the envelope by %.3g mm at most\n", worst);
  CHECK(worst <= 1e-5);
  // Each corner rounded with kRadius: a piece of the sphere about the point
  // of its axis 12.5 + r from the groove's axis.
  const double dz = std::sqrt((kGroove + kRadius) * (kGroove + kRadius)
                              - (5. - kRadius - kAxisY) * (5. - kRadius - kAxisY));
  for (const double zc : {kAxisZ - dz, kAxisZ + dz}) {
    const gp_Pnt centre(l.cornerX, 5. - kRadius, zc);
    bool         found = false;
    for (const TopoDS_Face& f : added) {
      const BRepAdaptor_Surface s(f);
      if (s.GetType() == GeomAbs_Sphere && s.Sphere().Location().Distance(centre) < 1e-6
          && std::abs(s.Sphere().Radius() - kRadius) < 1e-9) {
        found = true;
      }
    }
    std::printf("the corner's sphere about (%.4f, %.4f, %.4f): %s\n",
                centre.X(),
                centre.Y(),
                centre.Z(),
                found ? "found" : "missing");
    CHECK(found);
  }
  // The flange's roundings (cylinders of r along x under y = 5) run on past
  // the wall x = 0 up to x = r, where they meet the wall's rounding.
  int along = 0;
  for (const TopoDS_Face& f : added) {
    const BRepAdaptor_Surface s(f);
    if (s.GetType() != GeomAbs_Cylinder || std::abs(s.Cylinder().Radius() - kRadius) > 1e-9
        || !s.Cylinder().Axis().Direction().IsParallel(gp_Dir(1., 0., 0.), 1e-9)) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(f, box);
    double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
    box.Get(x0, y0, z0, x1, y1, z1);
    std::printf("the flange's rounding runs to x = %.4f\n", x1);
    CHECK(std::abs(x1 - kRadius) < 1e-3);
    ++along;
  }
  CHECK(along == 2);
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

void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/groove-rim-corners/";
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  if (shape.IsNull() && after.IsNull()) {
    std::printf("test_reported_body: skipped (no case groove-rim-corners in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  // As the design selects them: the groove's edges with the walls x = const,
  // the slanted sides and the roundings of 2 (the flange's top and its
  // corner rounding join the chain as tangent edges; the edge with the
  // flange's chamfer at the far end is not rounded).
  const std::vector<TopoDS_Edge> edges = rim_edges(shape, [](const TopoDS_Face& f) {
    const BRepAdaptor_Surface s(f);
    if (s.GetType() == GeomAbs_Cylinder) {
      return std::abs(s.Cylinder().Radius() - 2.) < 1e-6;
    }
    if (s.GetType() != GeomAbs_Plane) {
      return false;
    }
    const gp_Dir d = s.Plane().Axis().Direction();
    return std::abs(std::abs(d.X()) - 1.) < 1e-9 || std::abs(std::abs(d.X()) - 5. / std::sqrt(34.)) < 1e-5;
  });
  std::printf("%zu edges selected\n", edges.size());
  CHECK(edges.size() == 14u);
  const TopoDS_Shape result = round_edges(shape, edges);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  std::printf("faces %d, the stored body's %d\n", face_count(result), face_count(after));
  CHECK(face_count(result) == face_count(after));
  for (const double x : {0., 20.}) {
    Bnd_Box region;
    region.Update(x - 3.5, -1., 2., x + 3.5, 10., 28.);
    const double out  = farthest(result, after, region);
    const double back = farthest(after, result, region);
    std::printf("about the leg at x = %.0f: the result on the stored body within %.6f mm, the stored body on it "
                "within %.6f mm\n",
                x,
                out,
                back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
  const double change = volume_of(result) - volume_of(shape);
  const double file   = volume_of(after) - volume_of(shape);
  std::printf("the change of volume %.6f mm3, the stored body's %.6f\n", change, file);
  CHECK(std::abs(change - file) <= 5e-3 * std::abs(file));
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_synthetic_leg", test::test_synthetic_leg);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_groove_rim_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
