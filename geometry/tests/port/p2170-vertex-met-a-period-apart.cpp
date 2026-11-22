// SPDX-License-Identifier: MIT
// Port patch 2170: the boolean's wire splitter (BOPAlgo_WireSplitter, Path)
// tells apart the points of a vertex that a face's edges meet a period
// apart in the face's parameters when no edge there is closed on the face.
// The face, built from scratch on a cylinder of radius 1.5: one face
// without a seam whose boundary runs down the line u = 0, round the
// cylinder, and up beside a slit short of u = 2 pi that it leaves to
// touch u = 2 pi at one vertex, the same vertex it passes on the line
// u = 0, and at the top; a small triangular hole touches the boundary at that vertex on
// the line u = 0 (as a finned rod's side face in a reported design, where
// two fins meet the rod and a third one's knife edge grazes it). The
// boolean's face builder, given the face's edges as a boolean that splits
// the face gives them, alone or with a section line across the face (a
// plate cutting it), in every order, builds faces closed in the face's
// parameters and valid, holding the face's area. Before, the wire
// splitter closed loops at the vertex's other point, a period away: faces
// not closed in 2d that could not be oriented.

#include <cmath>
#include <cstddef>
#include <cstdio>
#include <vector>

#include <BOPAlgo_BuilderFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepLib.hxx>
#include <BRepLib_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <IntTools_Context.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 1.5;
// The slit's width in u.
constexpr double kSlit = 0.3;
constexpr double kTwoPi = 2.0 * test::kPi;
// The section line's height, where it cuts the face.
constexpr double kSection = 8.0;

struct Point {
  TopoDS_Vertex vertex;
  gp_Pnt2d uv;
};

// The face's edges as the boolean gives them to its face builder, its
// surface, and the area it bounds.
struct Edges {
  occ::handle<Geom_Surface> surface;
  TopoDS_Face face;
  std::vector<TopoDS_Shape> edges;
  double area = 0.0;
};

// An edge on the surface from a to b, straight in the face's parameters.
TopoDS_Edge edge_on(const occ::handle<Geom_Surface>& surface, const Point& a, const Point& b) {
  const gp_Vec2d along(a.uv, b.uv);
  const occ::handle<Geom2d_Curve> line(new Geom2d_Line(a.uv, gp_Dir2d(along)));
  BRepLib_MakeEdge maker(line, surface, a.vertex, b.vertex, 0.0, along.Magnitude());
  CHECK(maker.IsDone());
  return maker.IsDone() ? maker.Edge() : TopoDS_Edge();
}

// The area of a polygon in the face's parameters (counterclockwise
// positive).
double polygon_area(const std::vector<gp_Pnt2d>& points) {
  double area = 0.0;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const gp_Pnt2d& a = points[i];
    const gp_Pnt2d& b = points[(i + 1) % points.size()];
    area += a.X() * b.Y() - b.X() * a.Y();
  }
  return 0.5 * area;
}

// The face (its outer wire through A, C, D, P at u = 2 pi, E, F, T at
// u = 2 pi, M, T at u = 0, P at u = 0, counterclockwise in (u, v): the
// slit lies between u = 2 pi - kSlit and 2 pi from v = 0 to 4 and from 6
// to 9.5; its hole P, Q, R), and its edges,
// with the section line from S1 (u = 0) to S2 (beside the slit) at
// v = kSection when `cut`: the two lines it ends on split there, the
// section in both orientations.
Edges touching_face(bool cut) {
  Edges out;
  out.surface = new Geom_CylindricalSurface(gp_Ax3(), kRadius);
  const auto point = [&out](double u, double v) {
    const gp_Pnt2d uv(u, v);
    return Point{BRepBuilderAPI_MakeVertex(out.surface->Value(u, v)).Vertex(), uv};
  };
  const auto at = [](const Point& p, double u, double v) { return Point{p.vertex, gp_Pnt2d(u, v)}; };
  const Point a = point(0.0, 0.0);
  const Point c = point(kTwoPi - kSlit, 0.0);
  const Point d = point(kTwoPi - kSlit, 4.0);
  const Point p = point(0.0, 5.0);
  const Point e = point(kTwoPi - kSlit, 6.0);
  // Below the top: the face reaches u = 2 pi again only at the top, along
  // the edge from F up to T.
  const Point f = point(kTwoPi - kSlit, 9.5);
  const Point t = point(0.0, 10.0);
  const Point m = point(test::kPi, 10.0);
  const Point q = point(0.3, 5.2);
  const Point r = point(0.3, 4.8);
  const Point s1 = point(0.0, kSection);
  const Point s2 = point(kTwoPi - kSlit, kSection);
  const Point p_far = at(p, kTwoPi, 5.0);
  const Point t_far = at(t, kTwoPi, 10.0);

  std::vector<TopoDS_Edge> outer;
  outer.push_back(edge_on(out.surface, a, c));
  outer.push_back(edge_on(out.surface, c, d));
  outer.push_back(edge_on(out.surface, d, p_far));
  outer.push_back(edge_on(out.surface, p_far, e));
  if (cut) {
    outer.push_back(edge_on(out.surface, e, s2));
    outer.push_back(edge_on(out.surface, s2, f));
  } else {
    outer.push_back(edge_on(out.surface, e, f));
  }
  outer.push_back(edge_on(out.surface, f, t_far));
  outer.push_back(edge_on(out.surface, t_far, m));
  outer.push_back(edge_on(out.surface, m, t));
  if (cut) {
    outer.push_back(edge_on(out.surface, t, s1));
    outer.push_back(edge_on(out.surface, s1, p));
  } else {
    outer.push_back(edge_on(out.surface, t, p));
  }
  outer.push_back(edge_on(out.surface, p, a));
  // Clockwise: a hole.
  std::vector<TopoDS_Edge> hole;
  hole.push_back(edge_on(out.surface, p, q));
  hole.push_back(edge_on(out.surface, q, r));
  hole.push_back(edge_on(out.surface, r, p));

  BRep_Builder builder;
  TopoDS_Wire outer_wire;
  builder.MakeWire(outer_wire);
  for (const TopoDS_Edge& edge : outer) {
    builder.Add(outer_wire, edge);
  }
  outer_wire.Closed(true);
  TopoDS_Wire hole_wire;
  builder.MakeWire(hole_wire);
  for (const TopoDS_Edge& edge : hole) {
    builder.Add(hole_wire, edge);
  }
  hole_wire.Closed(true);
  builder.MakeFace(out.face, out.surface, 1e-7);
  builder.Add(out.face, outer_wire);
  builder.Add(out.face, hole_wire);
  BRepLib::BuildCurves3d(out.face);
  BRepLib::UpdateTolerances(out.face);

  for (const TopoDS_Edge& edge : outer) {
    out.edges.push_back(edge);
  }
  for (const TopoDS_Edge& edge : hole) {
    out.edges.push_back(edge);
  }
  if (cut) {
    const TopoDS_Edge section = edge_on(out.surface, s1, s2);
    BRepLib::BuildCurves3d(section);
    out.edges.push_back(section.Oriented(TopAbs_FORWARD));
    out.edges.push_back(section.Oriented(TopAbs_REVERSED));
  }
  const double outer_area = polygon_area({a.uv, c.uv, d.uv, p_far.uv, e.uv, f.uv, t_far.uv, t.uv, p.uv});
  const double hole_area = polygon_area({p.uv, q.uv, r.uv});
  out.area = kRadius * (outer_area + hole_area);
  return out;
}

int wires_of(const TopoDS_Shape& face) {
  int n = 0;
  for (TopoDS_Iterator it(face); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The face builder on the edges in every order (each edge first once):
// `faces` faces, each valid, holding the face's area, nothing left over.
void check_every_order(const Edges& given, int faces) {
  const std::size_t n = given.edges.size();
  int wrong = 0;
  for (std::size_t first = 0; first < n; ++first) {
    NCollection_List<TopoDS_Shape> edges;
    for (std::size_t k = 0; k < n; ++k) {
      edges.Append(given.edges[(first + k) % n]);
    }
    BOPAlgo_BuilderFace builder;
    builder.SetFace(given.face);
    builder.SetShapes(edges);
    const occ::handle<IntTools_Context> context = new IntTools_Context;
    builder.SetContext(context);
    builder.Perform();
    double area = 0.0;
    int invalid = 0;
    for (NCollection_List<TopoDS_Shape>::Iterator it(builder.Areas()); it.More(); it.Next()) {
      area += port::area_of(it.Value());
      invalid += port::valid(it.Value()) ? 0 : 1;
    }
    const bool right = !builder.HasErrors() && !builder.HasWarnings() && builder.Areas().Extent() == faces &&
                       invalid == 0 && std::abs(area - given.area) <= 1e-6 * given.area;
    if (!right) {
      std::fprintf(stderr, "  order from edge %zu: %d faces (%d invalid), area %.9f of %.9f%s%s\n", first,
                   builder.Areas().Extent(), invalid, area, given.area, builder.HasErrors() ? ", errors" : "",
                   builder.HasWarnings() ? ", warnings" : "");
      ++wrong;
    }
  }
  CHECK(wrong == 0);
}

// The face's own edges (a boolean that only touches the face, its vertex
// used more than twice): one face, its outer wire and the hole.
void test_face_touching_itself_a_period_apart_rebuilt() {
  const Edges given = touching_face(false);
  CHECK(port::valid(given.face));
  check_every_order(given, 1);
  // The face as it is: one face of two wires.
  BOPAlgo_BuilderFace builder;
  NCollection_List<TopoDS_Shape> edges;
  for (const TopoDS_Shape& edge : given.edges) {
    edges.Append(edge);
  }
  builder.SetFace(given.face);
  builder.SetShapes(edges);
  builder.Perform();
  CHECK(builder.Areas().Extent() == 1);
  if (builder.Areas().Extent() == 1) {
    CHECK(wires_of(builder.Areas().First()) == 2);
  }
}

// A section line across the face above the vertex (a plate cutting it):
// two faces, the lower one with the hole.
void test_face_touching_itself_a_period_apart_cut() {
  const Edges given = touching_face(true);
  check_every_order(given, 2);
}

} // namespace

MITCAD_PORT_TEST("2170", test_face_touching_itself_a_period_apart_rebuilt);
MITCAD_PORT_TEST("2170", test_face_touching_itself_a_period_apart_cut);
