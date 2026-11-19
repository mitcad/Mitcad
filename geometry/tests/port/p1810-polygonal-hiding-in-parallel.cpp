// SPDX-License-Identifier: MIT
// Port patch 1810: the hiding pass of OCCT's polygonal hidden-line removal
// (HLRBRep_PolyHLRToShape::Update) hides the segments in blocks beside each
// other on the threads of the default pool, and each segment is tested
// only against the hiding faces whose cells of a grid over the view meet
// its own (HLRAlgo_PolyAlgo::HideSegment), not against every shell and
// each face of every shell whose box meets it. The lines are those of
// stock OCCT, in the same order: on a view of many bodies, the visible and
// hidden lines of every kind, of all the bodies and of each body, are the
// lines that the walk over every shell, one segment after another, gives
// (the stock Hide, written out here with the algorithm's public parts),
// and the patched pass takes less than half that walk's time.

#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <vector>

#include <BRepBuilderAPI_Transform.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <HLRAlgo_BiPoint.hxx>
#include <HLRAlgo_EdgeIterator.hxx>
#include <HLRAlgo_EdgeStatus.hxx>
#include <HLRAlgo_PolyAlgo.hxx>
#include <HLRAlgo_PolyData.hxx>
#include <HLRAlgo_PolyShellData.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_PolyAlgo.hxx>
#include <HLRBRep_PolyHLRToShape.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

using Clock = std::chrono::steady_clock;

// A line of the view as the hiding pass gives it.
struct Line {
  gp_XY        p1, p2;
  TopoDS_Shape shape;
  bool         reg1, regn, outl, intl;
};

struct Lines {
  std::vector<Line> visible, hidden;
};

// A frame of beams, braces, tubes and balls, many of them in front of one
// another along the view: the posts of a grid with beams across them at two
// heights, a diagonal brace through each bay, a tube along each beam and a
// ball on each post. Each body is meshed on its own.
std::vector<TopoDS_Shape> frame(int columns, int rows) {
  std::vector<TopoDS_Shape> bodies;
  const double              bay = 600.0;
  for (int i = 0; i <= columns; ++i) {
    for (int j = 0; j <= rows; ++j) {
      const double x = i * bay, y = j * bay;
      bodies.push_back(BRepPrimAPI_MakeBox(gp_Pnt(x - 40.0, y - 40.0, 0.0), 80.0, 80.0, 2000.0).Shape());
      bodies.push_back(BRepPrimAPI_MakeSphere(gp_Pnt(x, y, 2060.0), 60.0).Shape());
      for (const double z : {900.0, 1900.0}) {
        if (i < columns) {
          bodies.push_back(BRepPrimAPI_MakeBox(gp_Pnt(x + 40.0, y - 30.0, z), bay - 80.0, 60.0, 100.0).Shape());
          bodies.push_back(
            BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x + 40.0, y - 60.0, z + 50.0), gp_Dir(1, 0, 0)), 18.0, bay - 80.0)
              .Shape());
        }
        if (j < rows) {
          bodies.push_back(BRepPrimAPI_MakeBox(gp_Pnt(x - 30.0, y + 40.0, z), 60.0, bay - 80.0, 100.0).Shape());
        }
      }
      if (i < columns && j < rows) {
        // A brace from the foot of the post to the top of the next one.
        const TopoDS_Shape brace =
          BRepPrimAPI_MakeBox(gp_Pnt(0.0, -20.0, -20.0), std::hypot(bay, 1800.0), 40.0, 40.0).Shape();
        gp_Trsf turn, move;
        turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, -1, 0)), std::atan2(1800.0, bay));
        move.SetTranslation(gp_Vec(x + 40.0, y + 0.5 * bay, 100.0));
        bodies.push_back(BRepBuilderAPI_Transform(brace, move * turn, true).Shape());
      }
    }
  }
  for (TopoDS_Shape& body : bodies) {
    BRepMesh_IncrementalMesh mesh(body, 0.1, false, 0.2, false);
  }
  return bodies;
}

// The view of the bodies from the side and above, as a drawing views them.
occ::handle<HLRBRep_PolyAlgo> loaded(const std::vector<TopoDS_Shape>& bodies) {
  occ::handle<HLRBRep_PolyAlgo> algo = new HLRBRep_PolyAlgo();
  for (const TopoDS_Shape& body : bodies) {
    algo->Load(body);
  }
  const gp_Dir eye(1.0, -0.05, 0.02);
  algo->Projector(HLRAlgo_Projector(gp_Ax2(gp_Pnt(0, 0, 0), eye, gp_Dir(eye.Crossed(gp_Dir(0, 0, 1))))));
  algo->Update();
  return algo;
}

// The visible and hidden parts of a segment as HLRBRep_PolyHLRToShape
// turns them into lines.
void add_lines(const HLRAlgo_BiPoint::PointsT& points,
               HLRAlgo_EdgeStatus&             status,
               const gp_Trsf&                  to_view,
               const Line&                     what,
               Lines&                          out) {
  gp_XYZ start = points.Pnt1, end = points.Pnt2;
  to_view.Transforms(start);
  to_view.Transforms(end);
  const gp_XY a(start.X(), start.Y()), d = gp_XY(end.X(), end.Y()) - a;
  if (d.Modulus() <= 1.e-10) {
    return;
  }
  double               sta = 0.0, stop = 0.0;
  float                tolsta = 0.0f, tolend = 0.0f;
  HLRAlgo_EdgeIterator it;
  for (it.InitVisible(status); it.MoreVisible(); it.NextVisible()) {
    it.Visible(sta, tolsta, stop, tolend);
    out.visible.push_back({a + sta * d, a + stop * d, what.shape, what.reg1, what.regn, what.outl, what.intl});
  }
  for (it.InitHidden(status); it.MoreHidden(); it.NextHidden()) {
    it.Hidden(sta, tolsta, stop, tolend);
    out.hidden.push_back({a + sta * d, a + stop * d, what.shape, what.reg1, what.regn, what.outl, what.intl});
  }
}

// Stock OCCT's hiding pass: one segment after another, each hidden by the
// faces of every shell whose box meets the segment's (HLRAlgo_PolyAlgo::Hide
// of OCCT 8.0.1, its tolerances as HLRAlgo_PolyAlgo::Update sets them).
Lines walked(const occ::handle<HLRBRep_PolyAlgo>& algo, double& seconds) {
  // The shapes of the segments, in their order, from the algorithm's own
  // walk (Hide names the edge or face of each).
  std::vector<Line>  named;
  HLRAlgo_EdgeStatus ignored;
  for (algo->InitHide(); algo->MoreHide(); algo->NextHide()) {
    Line what{};
    algo->Hide(ignored, what.shape, what.reg1, what.regn, what.outl, what.intl);
    named.push_back(what);
  }
  const auto                                                    started = Clock::now();
  const occ::handle<HLRAlgo_PolyAlgo>&                          poly    = algo->Algo();
  const NCollection_Array1<occ::handle<HLRAlgo_PolyShellData>>& shells  = poly->PolyShell();
  Bnd_Box                                                       box;
  for (int s = shells.Lower(); s <= shells.Upper(); ++s) {
    shells(s)->UpdateGlobalMinMax(box);
  }
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  HLRAlgo_PolyData::Triangle triangle;
  triangle.TolParam  = 0.00000001;
  triangle.TolAng    = 0.0001;
  triangle.Tolerance = std::max(std::max(xmax - xmin, ymax - ymin), zmax - zmin) * triangle.TolParam;
  const gp_Trsf& to_view = algo->Projector().Transformation();
  Lines          out;
  std::size_t    next = 0;
  for (int current = shells.Lower(); current <= shells.Upper(); ++current) {
    for (NCollection_List<HLRAlgo_BiPoint>::Iterator it(shells(current)->Edges()); it.More(); it.Next()) {
      HLRAlgo_BiPoint&           bp      = it.ChangeValue();
      HLRAlgo_BiPoint::PointsT&  points  = bp.Points();
      HLRAlgo_BiPoint::IndicesT& indices = bp.Indices();
      const Line& what = named[next++];
      HLRAlgo_EdgeStatus status(0.0, (float)triangle.TolParam, 1.0, (float)triangle.TolParam);
      if (bp.Hidden()) {
        status.HideAll();
      } else {
        for (int s = shells.Lower(); s <= shells.Upper(); ++s) {
          const occ::handle<HLRAlgo_PolyShellData>& shell = shells(s);
          if (!shell->Hiding()) {
            continue;
          }
          const HLRAlgo_PolyShellData::ShellIndices& box_of = shell->Indices();
          if (((box_of.Max - indices.MinSeg) & 0x80100200) == 0
              && ((indices.MaxSeg - box_of.Min) & 0x80100000) == 0) {
            NCollection_Array1<occ::handle<HLRAlgo_PolyData>>& faces = shell->HidingPolyData();
            for (int f = 1; f <= faces.Upper(); ++f) {
              faces(f)->HideByPolyData(points, triangle, indices, s == current, status);
            }
          }
        }
      }
      add_lines(points, status, to_view, what, out);
    }
  }
  seconds = std::chrono::duration<double>(Clock::now() - started).count();
  return out;
}

// The lines of a compound of 2D edges, in its order.
std::vector<std::pair<gp_XY, gp_XY>> edges_of(const TopoDS_Shape& compound) {
  std::vector<std::pair<gp_XY, gp_XY>> out;
  if (compound.IsNull()) {
    return out;
  }
  for (TopoDS_Iterator it(compound); it.More(); it.Next()) {
    TopoDS_Vertex first, last;
    TopExp::Vertices(TopoDS::Edge(it.Value()), first, last);
    const gp_Pnt a = BRep_Tool::Pnt(first), b = BRep_Tool::Pnt(last);
    out.push_back({gp_XY(a.X(), a.Y()), gp_XY(b.X(), b.Y())});
  }
  return out;
}

// The lines HLRBRep_PolyHLRToShape draws of a kind (1 the outlines of the
// curved faces, 2 smooth edges, 3 seam-like edges, 4 sharp edges), of one
// shape's sub-shapes or of all.
std::vector<std::pair<gp_XY, gp_XY>> drawn(const std::vector<Line>&                                       lines,
                                           int                                                            kind,
                                           const NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>* of) {
  std::vector<std::pair<gp_XY, gp_XY>> out;
  for (const Line& l : lines) {
    bool draw = kind == 1   ? l.intl
                : kind == 2 ? l.reg1 && !l.regn && !l.outl
                : kind == 3 ? l.regn && !l.outl
                            : !l.intl && (!l.reg1 || l.outl);
    if (draw && of != nullptr) {
      draw = of->Contains(l.shape);
    }
    if (draw && (l.p2 - l.p1).SquareModulus() > 1.e-20) {
      out.push_back({l.p1, l.p2});
    }
  }
  return out;
}

bool same(const std::vector<std::pair<gp_XY, gp_XY>>& a, const std::vector<std::pair<gp_XY, gp_XY>>& b) {
  if (a.size() != b.size()) {
    std::fprintf(stderr, "  %zu lines, %zu expected\n", a.size(), b.size());
    return false;
  }
  for (std::size_t i = 0; i < a.size(); ++i) {
    // A 2D edge's vertices are its line's ends, made without any rounding
    // of their own: equal to the last bit.
    if (a[i].first.X() != b[i].first.X() || a[i].first.Y() != b[i].first.Y()
        || a[i].second.X() != b[i].second.X() || a[i].second.Y() != b[i].second.Y()) {
      std::fprintf(stderr, "  line %zu differs\n", i);
      return false;
    }
  }
  return true;
}

// Each kind of line, visible and hidden, of all the bodies and of each of
// `bodies`, against the walk's.
void check_same_lines(HLRBRep_PolyHLRToShape& out, const Lines& expected, const std::vector<TopoDS_Shape>& bodies) {
  CHECK(same(edges_of(out.VCompound()), drawn(expected.visible, 4, nullptr)));
  CHECK(same(edges_of(out.OutLineVCompound()), drawn(expected.visible, 1, nullptr)));
  CHECK(same(edges_of(out.Rg1LineVCompound()), drawn(expected.visible, 2, nullptr)));
  CHECK(same(edges_of(out.RgNLineVCompound()), drawn(expected.visible, 3, nullptr)));
  CHECK(same(edges_of(out.HCompound()), drawn(expected.hidden, 4, nullptr)));
  CHECK(same(edges_of(out.OutLineHCompound()), drawn(expected.hidden, 1, nullptr)));
  CHECK(same(edges_of(out.Rg1LineHCompound()), drawn(expected.hidden, 2, nullptr)));
  for (const TopoDS_Shape& body : bodies) {
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> of;
    for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
      of.Add(e.Current());
    }
    for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
      of.Add(f.Current());
    }
    CHECK(same(edges_of(out.VCompound(body)), drawn(expected.visible, 4, &of)));
    CHECK(same(edges_of(out.HCompound(body)), drawn(expected.hidden, 4, &of)));
  }
}

// A frame of 12 x 8 bays (970 bodies) seen almost along its rows, so that
// many are seen through one another: the lines of the patched pass are the
// walk's, and the pass takes less than half the walk's time (stock OCCT's
// pass is the walk itself).
void test_frame_lines_are_the_walks() {
  const std::vector<TopoDS_Shape>     bodies = frame(12, 8);
  const occ::handle<HLRBRep_PolyAlgo> algo   = loaded(bodies);
  double                              walk   = 0.0;
  const Lines                         expected = walked(algo, walk);
  CHECK(expected.visible.size() > 10000);
  CHECK(expected.hidden.size() > 10000);
  HLRBRep_PolyHLRToShape out;
  const auto             started = Clock::now();
  out.Update(algo);
  const double pass = std::chrono::duration<double>(Clock::now() - started).count();
  std::fprintf(stderr, "frame of %zu bodies: %zu visible and %zu hidden lines; the walk %.3f s, the pass %.3f s\n",
               bodies.size(), expected.visible.size(), expected.hidden.size(), walk, pass);
  check_same_lines(out, expected, bodies);
  CHECK(pass < 0.5 * walk);
}

// One body (one block of segments, on the calling thread) and a view of
// none: the walk's lines, and no lines.
void test_one_body_and_none() {
  const std::vector<TopoDS_Shape>     one{frame(1, 1).back()};
  const occ::handle<HLRBRep_PolyAlgo> algo = loaded(one);
  double                              walk = 0.0;
  const Lines                         expected = walked(algo, walk);
  HLRBRep_PolyHLRToShape              out;
  out.Update(algo);
  CHECK(!expected.visible.empty());
  check_same_lines(out, expected, one);
  const occ::handle<HLRBRep_PolyAlgo> none = loaded({});
  HLRBRep_PolyHLRToShape              empty;
  empty.Update(none);
  CHECK(empty.VCompound().IsNull());
  CHECK(empty.HCompound().IsNull());
}

} // namespace

MITCAD_PORT_TEST("1810", test_frame_lines_are_the_walks);
MITCAD_PORT_TEST("1810", test_one_body_and_none);
