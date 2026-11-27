// SPDX-License-Identifier: MIT
// Booleans on stored bodies whose faces run many turns: a threaded hole's
// minor and major cylinders kept as one face each, their helix edges'
// pcurves spanning tens of periods, and the thread flanks one ruled
// B-spline face each (mitcad#349). The port's patches 0510 to 0512 and 0560 let
// OCCT's booleans cut them; 0591 to 0593 bound their work on such faces
// (mitcad#235); a major cylinder a file keeps as a cone of a tiny half
// angle is read as the cylinder (mitcad#389).

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>

#include <Adaptor3d_HSurfaceTool.hxx>
#include <Adaptor3d_Surface.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <Extrema_ExtFlag.hxx>
#include <Extrema_ExtPS.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <IntTools_CommonPrt.hxx>
#include <IntTools_EdgeFace.hxx>
#include <IntTools_Range.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/brep_import.hpp"
#include "mitcad/geometry/query.hpp"
#include "bundled_test.hpp"

namespace {

using test::kPi;
namespace brep = mitcad::brep;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    ++n;
  }
  return n;
}

// A square thread in a hole along z through a block, as a stored body
// keeps it: the hole's minor cylinder (radius r1) between the thread's
// turns and its major cylinder (r2) in the groove are one face each, as are
// the two flanks (radial helicoids z = k theta and z = k theta + w, ruled
// surfaces between helices at r1 and r2). The helices are interpolated at
// 64 points a turn; their parameter is the angle, so an edge's range spans
// `turns` periods of the cylinders.
struct ThreadedBlock {
  double r1 = 3.4;
  double r2 = 4.0;
  double pitch = 1.25;
  double groove = 0.6;
  int turns = 32;
  double half = 8.0; // half the block's width
  // The half angle of the cone the major cylinder is stored as (0: a
  // cylinder); a file may keep a cylinder as a cone of a half angle of
  // 1e-13 rad, its apex 1e14 mm away (mitcad#389).
  double major_angle = 0.0;

  double height() const { return pitch * turns; }

  // The block's volume: the square prism less the core and the groove,
  // which the helicoids sweep uniformly (groove / pitch of the ring).
  double expected_volume() const {
    const double h = height();
    return 4.0 * half * half * h - kPi * r1 * r1 * h - kPi * (r2 * r2 - r1 * r1) * h * groove / pitch;
  }

  // Whether a point inside the block's square lies in its material: outside
  // the major cylinder, or between the cylinders above the upper flank and
  // below the next turn's lower flank.
  bool material(const gp_Pnt& p) const {
    const double r = std::hypot(p.X(), p.Y());
    if (r < r1) {
      return false;
    }
    if (r > r2) {
      return true;
    }
    const double k = pitch / (2.0 * kPi);
    const double along = p.Z() - k * std::atan2(p.Y(), p.X());
    const double phase = along - pitch * std::floor(along / pitch);
    return phase > groove;
  }

  // The volume of the material in a box where `inside` holds, sampled at
  // the centres of 60^3 cells.
  double material_volume(const gp_Pnt& low, const gp_Pnt& high,
                         const std::function<bool(const gp_Pnt&)>& inside) const {
    constexpr int n = 60;
    const gp_Vec size(low, high);
    int in = 0;
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        for (int l = 0; l < n; ++l) {
          const gp_Pnt p(low.X() + size.X() * (i + 0.5) / n, low.Y() + size.Y() * (j + 0.5) / n,
                         low.Z() + size.Z() * (l + 0.5) / n);
          in += material(p) && inside(p) ? 1 : 0;
        }
      }
    }
    return size.X() * size.Y() * size.Z() * in / (static_cast<double>(n) * n * n);
  }

  brep::Body body() const {
    brep::Body b;
    const double k = pitch / (2.0 * kPi);
    const double h = height();
    const double tb = -2.0 * kPi * groove / pitch; // where the upper flank meets z = 0
    const double span = 2.0 * kPi * turns;
    const auto geometry = [&](int kind, const std::vector<std::int32_t>& ints, const std::vector<double>& reals) {
      brep::Geometry g;
      g.kind = kind;
      g.int_offset = static_cast<std::uint32_t>(b.ints.size());
      g.int_count = static_cast<std::uint32_t>(ints.size());
      g.real_offset = static_cast<std::uint32_t>(b.reals.size());
      g.real_count = static_cast<std::uint32_t>(reals.size());
      b.ints.insert(b.ints.end(), ints.begin(), ints.end());
      b.reals.insert(b.reals.end(), reals.begin(), reals.end());
      return g;
    };
    // An interpolated helix's ints and reals: radius r, z = k t + dz for t
    // in [t0, t0 + span].
    const int samples = 64 * turns;
    const auto helix = [&](double r, double t0, double dz, std::vector<std::int32_t>& ints,
                           std::vector<double>& reals) {
      ints.push_back(samples + 1);
      std::vector<double> points;
      for (int i = 0; i <= samples; ++i) {
        const double t = t0 + span * i / samples;
        reals.push_back(t);
        points.insert(points.end(), {r * std::cos(t), r * std::sin(t), k * t + dz});
      }
      reals.insert(reals.end(), points.begin(), points.end());
    };
    const auto add_helix = [&](double r, double t0, double dz) {
      std::vector<std::int32_t> ints;
      std::vector<double> reals;
      helix(r, t0, dz, ints, reals);
      b.curves.push_back(geometry(3, ints, reals));
      return static_cast<std::uint32_t>(b.curves.size() - 1);
    };
    const auto add_circle = [&](double r, double z) {
      b.curves.push_back(geometry(1, {}, {0, 0, z, 0, 0, 1, 1, 0, 0, r, r}));
      return static_cast<std::uint32_t>(b.curves.size() - 1);
    };
    const auto add_line = [&](gp_Pnt from, gp_Pnt to) {
      const double l = from.Distance(to);
      b.curves.push_back(geometry(0, {},
                                  {from.X(), from.Y(), from.Z(), (to.X() - from.X()) / l, (to.Y() - from.Y()) / l,
                                   (to.Z() - from.Z()) / l}));
      return static_cast<std::uint32_t>(b.curves.size() - 1);
    };
    const auto add_vertex = [&](gp_Pnt p) {
      b.vertices.insert(b.vertices.end(), {p.X(), p.Y(), p.Z(), 1e-5});
      return static_cast<std::uint32_t>(b.vertices.size() / 4 - 1);
    };
    const auto add_edge = [&](std::uint32_t curve, std::uint32_t v0, std::uint32_t v1, double t0, double t1) {
      b.edges.push_back({curve, v0, v1, t0, t1, 1e-5});
      return static_cast<std::uint32_t>(b.edges.size() - 1);
    };
    const auto at = [](double r, double t, double z) { return gp_Pnt(r * std::cos(t), r * std::sin(t), z); };
    const auto line_edge = [&](std::uint32_t v0, std::uint32_t v1) {
      const gp_Pnt p0(b.vertices[4 * v0], b.vertices[4 * v0 + 1], b.vertices[4 * v0 + 2]);
      const gp_Pnt p1(b.vertices[4 * v1], b.vertices[4 * v1 + 1], b.vertices[4 * v1 + 2]);
      return add_edge(add_line(p0, p1), v0, v1, 0.0, p0.Distance(p1));
    };

    // Vertices where the flanks meet the end faces.
    const std::uint32_t a1s = add_vertex(at(r1, 0, 0));
    const std::uint32_t a2s = add_vertex(at(r2, 0, 0));
    const std::uint32_t b1s = add_vertex(at(r1, tb, 0));
    const std::uint32_t b2s = add_vertex(at(r2, tb, 0));
    const std::uint32_t a1e = add_vertex(at(r1, span, h));
    const std::uint32_t a2e = add_vertex(at(r2, span, h));
    const std::uint32_t b1e = add_vertex(at(r1, tb + span, h));
    const std::uint32_t b2e = add_vertex(at(r2, tb + span, h));
    // The helices: the lower flank's (A) and the upper flank's (B).
    const std::uint32_t ca1 = add_helix(r1, 0, 0);
    const std::uint32_t ca2 = add_helix(r2, 0, 0);
    const std::uint32_t cb1 = add_helix(r1, tb, groove);
    const std::uint32_t cb2 = add_helix(r2, tb, groove);
    const std::uint32_t a1 = add_edge(ca1, a1s, a1e, 0, span);
    const std::uint32_t a2 = add_edge(ca2, a2s, a2e, 0, span);
    const std::uint32_t b1 = add_edge(cb1, b1s, b1e, tb, tb + span);
    const std::uint32_t b2 = add_edge(cb2, b2s, b2e, tb, tb + span);
    // The hole's outline in the end faces: the minor arc, the flanks'
    // radial lines and the major arc.
    const double tb_turned = tb + 2.0 * kPi;
    const std::uint32_t bottom1 = add_edge(add_circle(r1, 0), a1s, b1s, 0, tb_turned);
    const std::uint32_t bottom2 = add_edge(add_circle(r2, 0), b2s, a2s, tb_turned, 2.0 * kPi);
    const std::uint32_t top1 = add_edge(add_circle(r1, h), a1e, b1e, 0, tb_turned);
    const std::uint32_t top2 = add_edge(add_circle(r2, h), b2e, a2e, tb_turned, 2.0 * kPi);
    const std::uint32_t ra0 = line_edge(a1s, a2s);
    const std::uint32_t rb0 = line_edge(b1s, b2s);
    const std::uint32_t ran = line_edge(a1e, a2e);
    const std::uint32_t rbn = line_edge(b1e, b2e);
    // The block.
    std::uint32_t corner[8];
    for (int i = 0; i < 8; ++i) {
      const double x = (i % 4 == 1 || i % 4 == 2) ? half : -half;
      const double y = (i % 4 >= 2) ? half : -half;
      corner[i] = add_vertex(gp_Pnt(x, y, i < 4 ? 0.0 : h));
    }
    std::uint32_t q[12];
    for (int i = 0; i < 4; ++i) {
      q[i] = line_edge(corner[i], corner[(i + 1) % 4]);
      q[4 + i] = line_edge(corner[4 + i], corner[4 + (i + 1) % 4]);
      q[8 + i] = line_edge(corner[i], corner[4 + i]);
    }

    // Surfaces.
    const auto add_surface = [&](int kind, const std::vector<std::int32_t>& ints, const std::vector<double>& reals) {
      b.surfaces.push_back(geometry(kind, ints, reals));
      return static_cast<std::uint32_t>(b.surfaces.size() - 1);
    };
    const auto add_ruled = [&](double t0, double dz) {
      std::vector<std::int32_t> ints{3};
      std::vector<double> reals;
      helix(r1, t0, dz, ints, reals);
      ints.push_back(3);
      helix(r2, t0, dz, ints, reals);
      return add_surface(7, ints, reals);
    };
    const std::uint32_t minor = add_surface(1, {}, {0, 0, 0, 0, 0, 1, 1, 0, 0, r1, 0});
    const std::uint32_t major = add_surface(1, {}, {0, 0, 0, 0, 0, 1, 1, 0, 0, r2, major_angle});
    const std::uint32_t flank_a = add_ruled(0, 0);
    const std::uint32_t flank_b = add_ruled(tb, groove);
    const std::uint32_t bottom = add_surface(0, {}, {0, 0, 0, 0, 0, 1, 1, 0, 0});
    const std::uint32_t top = add_surface(0, {}, {0, 0, h, 0, 0, 1, 1, 0, 0});
    const std::uint32_t side[4] = {
        add_surface(0, {}, {0, -half, 0, 0, -1, 0, 1, 0, 0}), add_surface(0, {}, {half, 0, 0, 1, 0, 0, 0, 1, 0}),
        add_surface(0, {}, {0, half, 0, 0, 1, 0, -1, 0, 0}), add_surface(0, {}, {-half, 0, 0, -1, 0, 0, 0, -1, 0})};

    // Faces: loops listed counterclockwise about the surface's own normal
    // (the uv plane); `reversed` faces get their coedges turned, as the
    // data gives them about the material's normal.
    const auto add_face = [&](std::uint32_t surface, bool reversed,
                              const std::vector<std::vector<brep::Coedge>>& loops) {
      brep::Face f;
      f.surface = surface;
      f.reversed = reversed;
      f.first_loop = static_cast<std::uint32_t>(b.loops.size());
      f.loop_count = static_cast<std::uint32_t>(loops.size());
      for (const auto& loop : loops) {
        b.loops.push_back({static_cast<std::uint32_t>(b.coedges.size()), static_cast<std::uint32_t>(loop.size())});
        for (brep::Coedge c : loop) {
          c.forward = c.forward != reversed;
          b.coedges.push_back(c);
        }
      }
      b.shell_faces.push_back(static_cast<std::uint32_t>(b.faces.size()));
      b.faces.push_back(f);
    };
    add_face(minor, true, {{{bottom1, true}, {b1, true}, {top1, false}, {a1, false}}});
    add_face(major, true, {{{bottom2, true}, {a2, true}, {top2, false}, {b2, false}}});
    add_face(flank_a, false, {{{ra0, true}, {a2, true}, {ran, false}, {a1, false}}});
    add_face(flank_b, true, {{{rb0, true}, {b2, true}, {rbn, false}, {b1, false}}});
    add_face(bottom, true,
             {{{q[0], true}, {q[1], true}, {q[2], true}, {q[3], true}},
              {{ra0, true}, {bottom2, false}, {rb0, false}, {bottom1, false}}});
    add_face(top, false,
             {{{q[4], true}, {q[5], true}, {q[6], true}, {q[7], true}},
              {{ran, true}, {top2, false}, {rbn, false}, {top1, false}}});
    for (int i = 0; i < 4; ++i) {
      add_face(side[i], false, {{{q[i], true}, {q[8 + (i + 1) % 4], true}, {q[4 + i], false}, {q[8 + i], false}}});
    }
    b.shells.push_back({0, 0, static_cast<std::uint32_t>(b.faces.size()), true});
    b.lump_count = 1;
    return b;
  }
};

// The threaded block builds as a valid solid of the expected volume, with
// its hole's cylinders and flanks one face each.
TopoDS_Shape threaded_block(const ThreadedBlock& spec) {
  const brep::BuildResult built = brep::build_body(spec.body());
  CHECK(built.report.built && built.report.solid && built.report.valid);
  CHECK_NEAR_TOL(volume(built.shape), spec.expected_volume(), 1e-6);
  CHECK(count(built.shape, TopAbs_FACE) == 10);
  return built.shape;
}

// A cut that crosses the thread: done, valid, one solid; what it has in
// common with the block is the material inside the tool (`inside`, sampled
// within `low`..`high`); and what it removed is that, as the faces the cut
// changed bound it.
TopoDS_Shape check_cut(const ThreadedBlock& spec, const TopoDS_Shape& block, const TopoDS_Shape& tool, const gp_Pnt& low,
                       const gp_Pnt& high, const std::function<bool(const gp_Pnt&)>& inside) {
  BRepAlgoAPI_Common common(block, tool);
  CHECK(common.IsDone());
  const double removed = common.IsDone() ? volume(common.Shape()) : 0.0;
  CHECK_NEAR_TOL(removed, spec.material_volume(low, high, inside), 0.01);
  BRepAlgoAPI_Cut cut(block, tool);
  CHECK(cut.IsDone());
  if (!cut.IsDone()) {
    return TopoDS_Shape();
  }
  CHECK(count(cut.Shape(), TopAbs_SOLID) == 1);
  CHECK(BRepCheck_Analyzer(cut.Shape()).IsValid());
  // What it removed, integrated over the faces the cut changed: the
  // integration over the whole body is not exact enough on faces running
  // 32 turns.
  const mitcad::geometry::ChangedVolume changed = mitcad::geometry::changed_volume({block}, {cut.Shape()});
  CHECK_NEAR_TOL(-changed.volume, removed, 1e-5);
  return cut.Shape();
}

TopoDS_Shape check_box_cut(const ThreadedBlock& spec, const TopoDS_Shape& block, const gp_Pnt& low, const gp_Pnt& high) {
  return check_cut(spec, block, BRepPrimAPI_MakeBox(low, high).Shape(), low, high,
                   [](const gp_Pnt&) { return true; });
}

void test_cut_across_a_thread_whose_faces_run_many_turns() {
  // Boxes of 1 mm across the thread's minor and major cylinders and its
  // flanks: near the bottom, in the middle and on another side. Without
  // 0510 the section curves on the cylinders lay a period or more outside
  // the faces and the results were invalid; without 0511 the parts of them
  // inside the faces were classified as outside and the first cut removed
  // nothing.
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  check_box_cut(spec, block, gp_Pnt(3.0, -0.5, 2.0), gp_Pnt(4.5, 0.5, 3.0));
  check_box_cut(spec, block, gp_Pnt(3.0, -0.5, 20.2), gp_Pnt(4.5, 0.5, 21.2));
  check_box_cut(spec, block, gp_Pnt(-0.5, -4.5, 30.0), gp_Pnt(0.5, -3.0, 31.0));
}

// The crossings of an edge with a face's surface found by sampling the
// signed distance to a cylinder about the x axis densely.
int sampled_crossings(const TopoDS_Edge& edge, double radius, double z) {
  BRepAdaptor_Curve curve(edge);
  const int n = 100000;
  int crossings = 0;
  double previous = 0.0;
  for (int k = 0; k <= n; ++k) {
    const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / n;
    const gp_Pnt p = curve.Value(t);
    const double d = std::hypot(p.Y(), p.Z() - z) - radius;
    if (k > 0 && (d > 0) != (previous > 0)) {
      ++crossings;
    }
    previous = d;
  }
  return crossings;
}

void test_cross_hole_through_a_thread_whose_edges_run_many_turns() {
  // A hole of radius 1 across the threaded hole at mid-height: each helix
  // edge of 32 turns crosses it twice where it passes the hole, several
  // times within a few turns. OCCT's intersection of a curve with a
  // quadric sampled the whole edge 50 times, every two thirds of a turn,
  // and found one crossing of each such pair or none (0512).
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  const double z = 20.0;
  const double radius = 1.0;
  const TopoDS_Shape drill =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-9, 0, z), gp_Dir(1, 0, 0)), radius, 18).Shape();
  TopoDS_Face side;
  for (TopExp_Explorer ex(drill, TopAbs_FACE); ex.More(); ex.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_Cylinder) {
      side = TopoDS::Face(ex.Current());
    }
  }
  int long_edges = 0;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(block, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    double first = 0.0;
    double last = 0.0;
    BRep_Tool::Range(edge, first, last);
    if (last - first < 100.0) {
      continue;
    }
    ++long_edges;
    IntTools_EdgeFace intersection;
    intersection.SetEdge(edge);
    intersection.SetFace(side);
    intersection.SetRange(IntTools_Range(first, last));
    intersection.Perform();
    CHECK(intersection.IsDone());
    const int expected = sampled_crossings(edge, radius, z);
    CHECK(expected >= 4);
    CHECK(intersection.IsDone() && intersection.CommonParts().Length() == expected);
  }
  CHECK(long_edges == 4);
  const auto in_drill = [&](const gp_Pnt& p) { return std::hypot(p.Y(), p.Z() - z) < radius; };
  check_cut(spec, block, drill, gp_Pnt(-8, -radius, z - radius), gp_Pnt(8, radius, z + radius), in_drill);
}

void test_second_cut_across_a_thread_whose_faces_run_many_turns() {
  // Box cuts on the result of a cross hole through the threaded hole at
  // mid-height (mitcad#380). The hole splits the thread's cylinders into
  // pieces below and above it, each on the one surface: the piece below
  // ends well short of the surface's first period. OCCT moved a box's
  // section curves from that period into such a piece's range, but checked
  // the copy at twice the shift, found it outside the piece and moved the
  // curves on from there, out of the piece (0560): the box cut removed
  // nothing and its result was invalid.
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  const double z = 20.0;
  const double radius = 1.0;
  const TopoDS_Shape drill =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-9, 0, z), gp_Dir(1, 0, 0)), radius, 18).Shape();
  const auto in_drill = [&](const gp_Pnt& p) { return std::hypot(p.Y(), p.Z() - z) < radius; };
  const TopoDS_Shape drilled =
      check_cut(spec, block, drill, gp_Pnt(-8, -radius, z - radius), gp_Pnt(8, radius, z + radius), in_drill);
  CHECK(!drilled.IsNull());
  if (drilled.IsNull()) {
    return;
  }
  // Below the cross hole and above it, on two sides of the hole.
  check_box_cut(spec, drilled, gp_Pnt(3.0, -0.5, 2.0), gp_Pnt(4.5, 0.5, 3.0));
  check_box_cut(spec, drilled, gp_Pnt(-0.5, -4.5, 30.0), gp_Pnt(0.5, -3.0, 31.0));
}

void test_cut_across_a_thread_whose_major_cylinder_is_stored_as_a_cone() {
  // A file may store a thread's major cylinder as a cone of a half angle
  // of 1e-13 rad, its apex 1e14 mm away, one face running all the turns
  // (mitcad#389). OCCT's intersections of such a cone with other faces of
  // revolution about its axis lost their curves: the cut by a cone round
  // the axis removed nothing (here it ran for minutes, past 6 GB). Within
  // the angular precision of zero it is read as the cylinder it is, and
  // cuts across it go as across the cylinder.
  ThreadedBlock spec;
  spec.major_angle = -1e-13;
  const TopoDS_Shape block = threaded_block(spec);
  int cones = 0;
  for (TopExp_Explorer ex(block, TopAbs_FACE); ex.More(); ex.Next()) {
    cones += BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_Cone ? 1 : 0;
  }
  CHECK(cones == 0);
  check_box_cut(spec, block, gp_Pnt(3.0, -0.5, 2.0), gp_Pnt(4.5, 0.5, 3.0));
  check_box_cut(spec, block, gp_Pnt(-0.5, -4.5, 30.0), gp_Pnt(0.5, -3.0, 31.0));
  // A cone round the hole's axis from inside the major cylinder out past
  // it: the circle where it crosses the major face.
  const TopoDS_Shape tool = BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 19.0), gp_Dir(0, 0, 1)), 3.7, 4.4, 0.8).Shape();
  check_cut(spec, block, tool, gp_Pnt(-4.4, -4.4, 19.0), gp_Pnt(4.4, 4.4, 19.8), [](const gp_Pnt& p) {
    return std::hypot(p.X(), p.Y()) < 3.7 + 0.7 * (p.Z() - 19.0) / 0.8;
  });
}

// The thread's flank faces: B-spline surfaces whose knots run along the
// helices of all 32 turns (64 a turn).
std::vector<TopoDS_Face> flank_faces(const TopoDS_Shape& block) {
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer ex(block, TopAbs_FACE); ex.More(); ex.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(ex.Current())).GetType() == GeomAbs_BSplineSurface) {
      faces.push_back(TopoDS::Face(ex.Current()));
    }
  }
  return faces;
}

// The knot spans of the knots within [from, to], counted as OCCT's
// adaptors count the intervals of a range.
int spans_within(const NCollection_Array1<double>& knots, double from, double to) {
  int inside = 0;
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    inside += knots(i) > from + 1e-9 && knots(i) < to - 1e-9 ? 1 : 0;
  }
  return inside + 1;
}

void test_face_on_a_part_of_a_multi_turn_surface_sampled_within_it() {
  // An edge crossing a face is intersected with a polyhedron of the face's
  // surface (IntCurveSurface) of NbSamplesU x NbSamplesV points; for a
  // B-spline they were its knots times its degree over the whole surface,
  // also for a face on a third of a turn of a flank running 32 turns, as a
  // thread built in pieces sharing its surfaces has them (mitcad#235, port
  // 0591). They are now the knot spans within the face's range: the same
  // for a face of the whole surface.
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  const std::vector<TopoDS_Face> flanks = flank_faces(block);
  CHECK(flanks.size() == 2);
  if (flanks.empty()) {
    return;
  }
  const occ::handle<Geom_BSplineSurface> surface =
      occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(flanks.front()));
  CHECK(!surface.IsNull());
  if (surface.IsNull()) {
    return;
  }
  // The direction along the helices.
  const bool along_u = surface->NbUKnots() > surface->NbVKnots();
  const int knots = along_u ? surface->NbUKnots() : surface->NbVKnots();
  const int degree = along_u ? surface->UDegree() : surface->VDegree();
  CHECK(knots > 32 * 60);
  const occ::handle<BRepAdaptor_Surface> whole = new BRepAdaptor_Surface(flanks.front());
  const int whole_samples =
      along_u ? Adaptor3d_HSurfaceTool::NbSamplesU(whole) : Adaptor3d_HSurfaceTool::NbSamplesV(whole);
  CHECK(whole_samples == knots * degree);

  // A face on a third of a turn of the surface, halfway up.
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  surface->Bounds(u0, u1, v0, v1);
  const double from = along_u ? (u0 + u1) / 2.0 : (v0 + v1) / 2.0;
  const double to = from + (along_u ? u1 - u0 : v1 - v0) / spec.turns / 3.0;
  const TopoDS_Face piece = along_u ? BRepBuilderAPI_MakeFace(surface, from, to, v0, v1, 1e-7).Face()
                                    : BRepBuilderAPI_MakeFace(surface, u0, u1, from, to, 1e-7).Face();
  const occ::handle<BRepAdaptor_Surface> part = new BRepAdaptor_Surface(piece);
  const int part_samples =
      along_u ? Adaptor3d_HSurfaceTool::NbSamplesU(part) : Adaptor3d_HSurfaceTool::NbSamplesV(part);
  const int spans = spans_within(along_u ? surface->UKnots() : surface->VKnots(), from, to);
  std::printf("  samples along the helices: whole face %d, a third of a turn %d (%d spans)\n", whole_samples,
              part_samples, spans);
  CHECK(part_samples <= (spans + 1) * degree);
  CHECK(part_samples * 50 < whole_samples);

  // A line through the piece's middle along its normal crosses it there,
  // once.
  const double um = along_u ? (from + to) / 2.0 : (u0 + u1) / 2.0;
  const double vm = along_u ? (v0 + v1) / 2.0 : (from + to) / 2.0;
  gp_Pnt middle;
  gp_Vec du;
  gp_Vec dv;
  surface->D1(um, vm, middle, du, dv);
  const gp_Vec normal = du.Crossed(dv).Normalized();
  const TopoDS_Edge line =
      BRepBuilderAPI_MakeEdge(middle.Translated(-normal * 0.2), middle.Translated(normal * 0.2)).Edge();
  IntTools_EdgeFace intersection;
  intersection.SetEdge(line);
  intersection.SetFace(piece);
  intersection.SetRange(IntTools_Range(0.0, 0.4));
  intersection.Perform();
  CHECK(intersection.IsDone());
  CHECK(intersection.IsDone() && intersection.CommonParts().Length() == 1);
  if (intersection.IsDone() && intersection.CommonParts().Length() == 1) {
    const IntTools_CommonPrt& part0 = intersection.CommonParts().First();
    CHECK(part0.Type() == TopAbs_VERTEX);
    CHECK_NEAR_TOL(part0.VertexParameter1(), 0.2, 1e-6);
  }
}

// An adaptor of a surface that counts its evaluations: the work of
// algorithms that evaluate a surface through its adaptor.
class CountingSurface : public Adaptor3d_Surface {
public:
  explicit CountingSurface(const occ::handle<Geom_Surface>& surface) : m_inner(surface) {}

  long evaluations() const { return m_evaluations; }

  double FirstUParameter() const override { return m_inner.FirstUParameter(); }
  double LastUParameter() const override { return m_inner.LastUParameter(); }
  double FirstVParameter() const override { return m_inner.FirstVParameter(); }
  double LastVParameter() const override { return m_inner.LastVParameter(); }
  GeomAbs_Shape UContinuity() const override { return m_inner.UContinuity(); }
  GeomAbs_Shape VContinuity() const override { return m_inner.VContinuity(); }
  int NbUIntervals(const GeomAbs_Shape s) const override { return m_inner.NbUIntervals(s); }
  int NbVIntervals(const GeomAbs_Shape s) const override { return m_inner.NbVIntervals(s); }
  void UIntervals(NCollection_Array1<double>& t, const GeomAbs_Shape s) const override { m_inner.UIntervals(t, s); }
  void VIntervals(NCollection_Array1<double>& t, const GeomAbs_Shape s) const override { m_inner.VIntervals(t, s); }
  bool IsUClosed() const override { return m_inner.IsUClosed(); }
  bool IsVClosed() const override { return m_inner.IsVClosed(); }
  bool IsUPeriodic() const override { return m_inner.IsUPeriodic(); }
  double UPeriod() const override { return m_inner.UPeriod(); }
  bool IsVPeriodic() const override { return m_inner.IsVPeriodic(); }
  double VPeriod() const override { return m_inner.VPeriod(); }
  double UResolution(const double r) const override { return m_inner.UResolution(r); }
  double VResolution(const double r) const override { return m_inner.VResolution(r); }
  GeomAbs_SurfaceType GetType() const override { return m_inner.GetType(); }
  int UDegree() const override { return m_inner.UDegree(); }
  int VDegree() const override { return m_inner.VDegree(); }
  int NbUPoles() const override { return m_inner.NbUPoles(); }
  int NbVPoles() const override { return m_inner.NbVPoles(); }
  int NbUKnots() const override { return m_inner.NbUKnots(); }
  int NbVKnots() const override { return m_inner.NbVKnots(); }
  bool IsURational() const override { return m_inner.IsURational(); }
  bool IsVRational() const override { return m_inner.IsVRational(); }
  occ::handle<Geom_BSplineSurface> BSpline() const override { return m_inner.BSpline(); }
  gp_Pnt EvalD0(const double u, const double v) const override {
    ++m_evaluations;
    return m_inner.EvalD0(u, v);
  }
  Geom_Surface::ResD1 EvalD1(const double u, const double v) const override {
    ++m_evaluations;
    return m_inner.EvalD1(u, v);
  }
  Geom_Surface::ResD2 EvalD2(const double u, const double v) const override {
    ++m_evaluations;
    return m_inner.EvalD2(u, v);
  }
  Geom_Surface::ResD3 EvalD3(const double u, const double v) const override {
    ++m_evaluations;
    return m_inner.EvalD3(u, v);
  }
  gp_Vec EvalDN(const double u, const double v, const int nu, const int nv) const override {
    ++m_evaluations;
    return m_inner.EvalDN(u, v, nu, nv);
  }

private:
  GeomAdaptor_Surface m_inner;
  mutable long m_evaluations = 0;
};

void test_projection_onto_a_multi_turn_surface_works_on_a_bounded_grid() {
  // The projection of a point onto a B-spline surface finds its candidates
  // on a grid, the degree's number of parameters a knot span, and computes
  // the distance to every node, edge and cell of it for each point: on a
  // flank running 32 turns (2000 knot spans along the helices) the grid had
  // 44 x 6000 nodes, and the booleans project onto each such face many times
  // (mitcad#235, port 0593). A range of more than 1001 such parameters is
  // now sampled at 1001; the projections are as before. The first
  // projection evaluates the grid's nodes through the adaptor.
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  const std::vector<TopoDS_Face> flanks = flank_faces(block);
  CHECK(!flanks.empty());
  if (flanks.empty()) {
    return;
  }
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(flanks.front());
  const CountingSurface counted(surface);
  Extrema_ExtPS projection;
  projection.Initialize(counted, counted.FirstUParameter(), counted.LastUParameter(), counted.FirstVParameter(),
                        counted.LastVParameter(), 1e-9, 1e-9);
  projection.SetFlag(Extrema_ExtFlag_MIN);
  // Points 0.05 off the surface along its normal, along its height: each
  // projects back onto its foot.
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  surface->Bounds(u0, u1, v0, v1);
  long grid = 0;
  long most = 0;
  for (int k = 0; k < 8; ++k) {
    const double u = u0 + (u1 - u0) * (k + 0.37) / 8.0;
    const double v = v0 + (v1 - v0) * (k + 0.61) / 8.0;
    gp_Pnt foot;
    gp_Vec du;
    gp_Vec dv;
    surface->D1(u, v, foot, du, dv);
    const gp_Pnt point = foot.Translated(du.Crossed(dv).Normalized() * 0.05);
    const long before = counted.evaluations();
    projection.Perform(point);
    // The first projection builds the grid.
    if (k == 0) {
      grid = counted.evaluations() - before;
    } else {
      most = std::max(most, counted.evaluations() - before);
    }
    CHECK(projection.IsDone() && projection.NbExt() > 0);
    if (!projection.IsDone() || projection.NbExt() == 0) {
      continue;
    }
    double best = projection.SquareDistance(1);
    for (int i = 2; i <= projection.NbExt(); ++i) {
      best = std::min(best, projection.SquareDistance(i));
    }
    CHECK_NEAR_TOL(std::sqrt(best), 0.05, 1e-7);
  }
  std::printf("  surface evaluations: the first projection (its grid) %ld, the others at most %ld\n", grid, most);
  CHECK(grid < 100000);
}

void test_merge_after_a_cut_across_a_multi_turn_thread() {
  // The face merge after a boolean (ShapeUpgrade_UnifySameDomain) tests
  // neighbouring faces of other surfaces for planarity, over the whole
  // surface; it now tests each surface once (mitcad#235, port 0592), with
  // the test's own results. The merge after a box cut across the thread,
  // whose faces lie on no common surface, keeps the cut's result as it is:
  // valid, of the same volume (integrated over the faces the merge
  // changed) and faces.
  const ThreadedBlock spec;
  const TopoDS_Shape block = threaded_block(spec);
  BRepAlgoAPI_Cut cut(block, BRepPrimAPI_MakeBox(gp_Pnt(3.0, -0.5, 20.2), gp_Pnt(4.5, 0.5, 21.2)).Shape());
  CHECK(cut.IsDone());
  if (!cut.IsDone()) {
    return;
  }
  const TopoDS_Shape before = cut.Shape();
  cut.SimplifyResult();
  const TopoDS_Shape after = cut.Shape();
  CHECK(BRepCheck_Analyzer(after).IsValid());
  const mitcad::geometry::ChangedVolume changed = mitcad::geometry::changed_volume({before}, {after});
  CHECK_NEAR_TOL(changed.volume, 0.0, 1e-6);
  std::printf("  faces before the merge %d, after %d (flanks %zu)\n", count(before, TopAbs_FACE),
              count(after, TopAbs_FACE), flank_faces(after).size());
  CHECK(count(after, TopAbs_FACE) == count(before, TopAbs_FACE));
}

} // namespace

MITCAD_BUNDLED_TEST(multi_turn_faces) {
  test::guarded("cut across a thread whose faces run many turns", test_cut_across_a_thread_whose_faces_run_many_turns);
  test::guarded("cross hole through a thread whose edges run many turns",
                test_cross_hole_through_a_thread_whose_edges_run_many_turns);
  test::guarded("second cut across a thread whose faces run many turns",
                test_second_cut_across_a_thread_whose_faces_run_many_turns);
  test::guarded("cut across a thread whose major cylinder is stored as a cone",
                test_cut_across_a_thread_whose_major_cylinder_is_stored_as_a_cone);
  test::guarded("face on a part of a multi-turn surface sampled within it",
                test_face_on_a_part_of_a_multi_turn_surface_sampled_within_it);
  test::guarded("projection onto a multi-turn surface works on a bounded grid",
                test_projection_onto_a_multi_turn_surface_works_on_a_bounded_grid);
  test::guarded("merge after a cut across a multi-turn thread", test_merge_after_a_cut_across_a_multi_turn_thread);
  if (test::failures == 0) {
    std::puts("test_multi_turn_faces: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
