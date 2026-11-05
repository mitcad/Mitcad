// SPDX-License-Identifier: MIT
// Roundings that run over cut-outs of their faces (mitcad#311): the
// cut-outs filled back to the faces' surfaces and cut out again after the
// rounding; chamfers wider than a narrow face beside their edges
// (mitcad#1232), the space past its far edge filled likewise (cutouts.hpp).
#include "cutouts.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <optional>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ChFi3d.hxx>
#include <ElSLib.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLib.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt2d.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kHalfPi = 1.5707963267948966;

using ShapeList = NCollection_List<TopoDS_Shape>;
using ShapeSet = NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>;

// An axis-aligned box: min x, y, z, max x, y, z.
using Box = std::array<double, 6>;

Box bounds_of(const TopoDS_Shape& shape) {
  Bnd_Box b;
  BRepBndLib::Add(shape, b, false);
  Box out{};
  b.Get(out[0], out[1], out[2], out[3], out[4], out[5]);
  return out;
}

Box grown(Box b, double d) {
  for (int i = 0; i < 3; ++i) {
    b[static_cast<std::size_t>(i)] -= d;
    b[static_cast<std::size_t>(i + 3)] += d;
  }
  return b;
}

Box meet(const Box& a, const Box& b) {
  Box out{};
  for (std::size_t i = 0; i < 3; ++i) {
    out[i] = std::max(a[i], b[i]);
    out[i + 3] = std::min(a[i + 3], b[i + 3]);
  }
  return out;
}

bool empty(const Box& b) { return !(b[0] < b[3] && b[1] < b[4] && b[2] < b[5]); }

double diagonal(const Box& b) { return std::hypot(b[3] - b[0], b[4] - b[1], b[5] - b[2]); }

TopoDS_Shape solid_of(const Box& b) {
  return BRepPrimAPI_MakeBox(gp_Pnt(b[0], b[1], b[2]), gp_Pnt(b[3], b[4], b[5])).Shape();
}

// The size of a shape's volume, whichever way a solid is turned.
double absolute_volume_of(const TopoDS_Shape& shape) { return std::abs(volume_of(shape)); }

// Runs `make`; what the geometry kernel fails at gives nothing (the
// fallback then does not apply).
template <class Make>
auto occt_guarded(Make&& make) -> decltype(make()) {
  try {
    OCC_CATCH_SIGNALS
    return make();
  } catch (const Standard_Failure&) {
    return {};
  }
}

// A boolean of solids that leaves its inputs as they are.
template <class Operation>
TopoDS_Shape boolean(const TopoDS_Shape& object, const ShapeList& tools, Operation& operation) {
  ShapeList objects;
  objects.Append(object);
  operation.SetArguments(objects);
  operation.SetTools(tools);
  operation.SetNonDestructive(true);
  operation.SetRunParallel(true);
  operation.Build();
  if (!operation.IsDone() || operation.HasErrors()) {
    throw Standard_Failure("a boolean of the cut-outs failed");
  }
  return operation.Shape();
}

template <class Operation>
TopoDS_Shape boolean(const TopoDS_Shape& object, const TopoDS_Shape& tool) {
  Operation operation;
  ShapeList tools;
  tools.Append(tool);
  return boolean(object, tools, operation);
}

// A point of the face's surface and the surface's normal there, pointing
// out of the face's material (the face's orientation applied).
std::pair<gp_Pnt, gp_Dir> point_and_normal(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  gp_Pnt p;
  gp_Vec du, dv;
  surface.D1(0.5 * (u0 + u1), 0.5 * (v0 + v1), p, du, dv);
  gp_Vec n = du.Crossed(dv);
  if (n.Magnitude() < gp::Resolution()) {
    throw Standard_Failure("a cut-out's face has no normal");
  }
  if (face.Orientation() == TopAbs_REVERSED) {
    n.Reverse();
  }
  return {p, gp_Dir(n)};
}

// The region within `box` on one side of the face's surface: out of the
// face's material (`outer`) or into it. None for surfaces other than
// planes, cylinders, cones, spheres and tori.
std::optional<TopoDS_Shape> side_of(const TopoDS_Face& face, bool outer, const Box& box) {
  const BRepAdaptor_Surface surface(face, false);
  const std::pair<gp_Pnt, gp_Dir> at = point_and_normal(face);
  const gp_Pnt p = at.first;
  const gp_Dir n = at.second;
  const double d = diagonal(box);
  const TopoDS_Shape region = solid_of(box);
  const gp_Pnt centre(0.5 * (box[0] + box[3]), 0.5 * (box[1] + box[4]), 0.5 * (box[2] + box[5]));
  const auto side_point = [&](double step) { return p.Translated(gp_Vec(n) * (outer ? step : -step)); };
  TopoDS_Shape primitive;
  double step = 1.0e-4 * std::max(1.0, d);
  switch (surface.GetType()) {
  case GeomAbs_Plane: {
    const TopoDS_Face plane = BRepBuilderAPI_MakeFace(surface.Plane()).Face();
    const TopoDS_Shape half = BRepPrimAPI_MakeHalfSpace(plane, side_point(1.0)).Solid();
    return boolean<BRepAlgoAPI_Common>(region, half);
  }
  case GeomAbs_Cylinder: {
    const gp_Cylinder c = surface.Cylinder();
    const gp_Ax1 axis = c.Axis();
    const double t = gp_Vec(axis.Location(), centre).Dot(gp_Vec(axis.Direction()));
    const gp_Pnt start = axis.Location().Translated(gp_Vec(axis.Direction()) * (t - d));
    primitive = BRepPrimAPI_MakeCylinder(gp_Ax2(start, axis.Direction()), c.Radius(), 2.0 * d).Shape();
    step = std::min(step, 1.0e-3 * c.Radius());
    break;
  }
  case GeomAbs_Cone: {
    const gp_Cone c = surface.Cone();
    const gp_Pnt apex = c.Apex();
    gp_Dir dir = c.Axis().Direction();
    // The nappe of the face's point.
    if (gp_Vec(apex, p).Dot(gp_Vec(dir)) < 0.0) {
      dir.Reverse();
    }
    const double height = std::abs(gp_Vec(apex, centre).Dot(gp_Vec(dir))) + d;
    const double angle = std::abs(c.SemiAngle());
    if (!(angle > 1.0e-6 && angle < kHalfPi - 1.0e-6)) {
      return std::nullopt;
    }
    primitive = BRepPrimAPI_MakeCone(gp_Ax2(apex, dir), 0.0, height * std::tan(angle), height).Shape();
    step = std::min(step, 1.0e-3 * std::max(Precision::Confusion(), apex.Distance(p) * std::sin(angle)));
    break;
  }
  case GeomAbs_Sphere: {
    const gp_Sphere s = surface.Sphere();
    primitive = BRepPrimAPI_MakeSphere(s.Location(), s.Radius()).Shape();
    step = std::min(step, 1.0e-3 * s.Radius());
    break;
  }
  case GeomAbs_Torus: {
    const gp_Torus t = surface.Torus();
    if (!(t.MajorRadius() > t.MinorRadius())) {
      return std::nullopt;
    }
    primitive = BRepPrimAPI_MakeTorus(gp_Ax2(t.Location(), t.Axis().Direction()), t.MajorRadius(),
                                      t.MinorRadius())
                    .Shape();
    step = std::min(step, 1.0e-3 * t.MinorRadius());
    break;
  }
  default:
    return std::nullopt;
  }
  BRepClass3d_SolidClassifier classifier(primitive, side_point(step), Precision::Confusion());
  if (classifier.State() == TopAbs_IN) {
    return boolean<BRepAlgoAPI_Common>(region, primitive);
  }
  if (classifier.State() == TopAbs_OUT) {
    return boolean<BRepAlgoAPI_Cut>(region, primitive);
  }
  return std::nullopt;
}

// Points of a face to test where it lies: its vertices, its edges'
// middles and the middle of its parameters where that is inside it.
std::vector<gp_Pnt> samples(const TopoDS_Face& face) {
  std::vector<gp_Pnt> points;
  for (TopExp_Explorer ex(face, TopAbs_VERTEX); ex.More(); ex.Next()) {
    points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())));
  }
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    double f = 0.0, l = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, f, l);
    if (!curve.IsNull()) {
      points.push_back(curve->Value(0.5 * (f + l)));
    }
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const gp_Pnt2d middle(0.5 * (u0 + u1), 0.5 * (v0 + v1));
  if (BRepTopAdaptor_FClass2d(face, Precision::PConfusion()).Perform(middle) == TopAbs_IN) {
    points.push_back(BRepAdaptor_Surface(face).Value(middle.X(), middle.Y()));
  }
  return points;
}

// Whether every point lies on the surface, within `tolerance`.
bool on_surface(const std::vector<gp_Pnt>& points, const occ::handle<Geom_Surface>& surface, double tolerance) {
  if (points.empty() || surface.IsNull()) {
    return false;
  }
  for (const gp_Pnt& p : points) {
    GeomAPI_ProjectPointOnSurf projection(p, surface);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > tolerance) {
      return false;
    }
  }
  return true;
}

// The distance between two shapes.
double distance(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepExtrema_DistShapeShape d(a, b);
  return d.IsDone() && d.NbSolution() > 0 ? d.Value() : Precision::Infinite();
}

// The shape with the faces merged that lie on one of `surfaces` on both
// sides of an edge between them, and the edges merged only at vertices
// that are no vertices of `input` but those at `loose` (none of the
// shape's other faces or the input's own edges are touched). Its history is
// added to `history`.
TopoDS_Shape merged_on(const TopoDS_Shape& made, const std::vector<occ::handle<Geom_Surface>>& surfaces,
                       const TopoDS_Shape& input, double tolerance, BRepTools_History& history,
                       const std::vector<gp_Pnt>& loose = {}) {
  // (On a copy: the face merge writes p-curves into the edges it keeps,
  // which the shape shares with the operation's inputs.)
  BRepBuilderAPI_Copy copier(made);
  ShapeList made_list;
  made_list.Append(made);
  history.Merge(made_list, copier);
  const TopoDS_Shape shape = copier.Shape();
  NCollection_IndexedDataMap<TopoDS_Shape, ShapeList, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
  // The surfaces each face lies on.
  NCollection_DataMap<TopoDS_Shape, std::vector<int>, TopTools_ShapeMapHasher> lies_on;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    if (lies_on.IsBound(ex.Current())) {
      continue;
    }
    const std::vector<gp_Pnt> points = samples(TopoDS::Face(ex.Current()));
    std::vector<int> on;
    for (std::size_t s = 0; s < surfaces.size(); ++s) {
      if (on_surface(points, surfaces[s], tolerance)) {
        on.push_back(static_cast<int>(s));
      }
    }
    lies_on.Bind(ex.Current(), on);
  }
  ShapeSet keep;
  bool any = false;
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const ShapeList& faces = faces_of(i);
    bool merge = false;
    bool seam = false;
    if (faces.Extent() == 2 && !faces.First().IsSame(faces.Last())) {
      const std::vector<int>& a = lies_on.Find(faces.First());
      const std::vector<int>& b = lies_on.Find(faces.Last());
      for (const int s : a) {
        merge = merge || std::find(b.begin(), b.end(), s) != b.end();
      }
    } else if (faces.Extent() == 1) {
      // (A seam of a face that may be merged: the merged face makes its
      // own.)
      seam = !lies_on.Find(faces.First()).empty();
    }
    if (merge) {
      any = true;
    } else if (!seam) {
      keep.Add(faces_of.FindKey(i));
    }
  }
  if (!any) {
    return shape;
  }
  std::vector<gp_Pnt> corners;
  for (TopExp_Explorer ex(input, TopAbs_VERTEX); ex.More(); ex.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
    if (std::none_of(loose.begin(), loose.end(), [&](const gp_Pnt& q) { return q.Distance(p) <= tolerance; })) {
      corners.push_back(p);
    }
  }
  for (TopExp_Explorer ex(shape, TopAbs_VERTEX); ex.More(); ex.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
    const double near = BRep_Tool::Tolerance(TopoDS::Vertex(ex.Current())) + tolerance;
    for (const gp_Pnt& c : corners) {
      if (c.Distance(p) <= near) {
        keep.Add(ex.Current());
        break;
      }
    }
  }
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.KeepShapes(keep);
  unify.Build();
  history.Merge(unify.History());
  return unify.Shape();
}

// A region next to a face of the selected edges, on one side of the
// face's surface, that a rounding runs over.
struct Region {
  // The cut-out (outside the body, on the face's material side) or the
  // protrusion (of the body, on the face's outer side).
  TopoDS_Shape solid;
  bool protrusion = false;
  // The region on the side of every wall the region is on: cut out again,
  // or (of the body) joined again.
  TopoDS_Shape tool;
  std::vector<int> walls;
};

// The regions next to face `n` of the body within `within` that a rounding
// along the `selected` edges, touching its faces within `contact` of them,
// runs over (`body_volume` and `face_boxes`: the body's volume and its
// faces' boxes).
std::vector<Region> regions_at(const Shape& body, int n, const ShapeSet& selected,
                               const std::vector<int>& spine, const Box& within, double contact,
                               double tolerance, double body_volume, const std::vector<Box>& face_boxes) {
  std::vector<Region> found;
  const TopoDS_Shape& solid = body.occt();
  const TopoDS_Face& face = body.face(n);
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  // The selected edges of the face: the rounding along them touches it
  // (one along the other edges does not).
  BRep_Builder builder;
  TopoDS_Compound edges;
  builder.MakeCompound(edges);
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    if (selected.Contains(ex.Current())) {
      builder.Add(edges, ex.Current());
    }
  }
  // The face's own edges but the selected ones and those that meet them:
  // a region next to the face meets one of them, within the rounding's
  // contact distance of the selected edges (the region's edges along the
  // face). Where no such edge comes that near, no region is looked for (a
  // test of edges, without the booleans below on the whole body).
  ShapeSet corners;
  for (ShapeSet::Iterator it(selected); it.More(); it.Next()) {
    for (TopExp_Explorer vx(it.Value(), TopAbs_VERTEX); vx.More(); vx.Next()) {
      corners.Add(vx.Current());
    }
  }
  TopoDS_Compound rims;
  builder.MakeCompound(rims);
  bool any_rim = false;
  std::vector<Box> rim_boxes;
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    if (selected.Contains(ex.Current())) {
      continue;
    }
    bool meets = false;
    for (TopExp_Explorer vx(ex.Current(), TopAbs_VERTEX); vx.More() && !meets; vx.Next()) {
      meets = corners.Contains(vx.Current());
    }
    if (!meets) {
      builder.Add(rims, ex.Current());
      any_rim = true;
      rim_boxes.push_back(grown(bounds_of(ex.Current()), tolerance));
    }
  }
  if (!any_rim || distance(rims, edges) > contact + tolerance) {
    return found;
  }
  // The regions are taken as far from the edges as the rounding touches
  // the faces (with a margin); their tools as far as it changes the body.
  const Box near = grown(bounds_of(edges), 1.25 * contact + tolerance);
  const Box reached = grown(bounds_of(edges), contact + tolerance);
  for (const bool protrusion : {false, true}) {
    const std::optional<TopoDS_Shape> side = side_of(face, protrusion, near);
    if (!side) {
      continue;
    }
    const TopoDS_Shape candidates =
        protrusion ? boolean<BRepAlgoAPI_Common>(*side, solid) : boolean<BRepAlgoAPI_Cut>(*side, solid);
    for (TopExp_Explorer cx(candidates, TopAbs_SOLID); cx.More(); cx.Next()) {
      try {
        OCC_CATCH_SIGNALS
        // (Tests of boxes and distances first, which reject most candidates
        // cheaply: one farther from the edges than the rounding touches, or
        // not meeting the face along a rim.)
        const Box bounds = bounds_of(cx.Current());
        if (empty(meet(bounds, reached)) ||
            std::none_of(rim_boxes.begin(), rim_boxes.end(), [&](const Box& b) { return !empty(meet(bounds, b)); }) ||
            distance(cx.Current(), rims) > tolerance || distance(cx.Current(), edges) > contact + tolerance) {
          continue;
        }
        // (A copy: the region's faces are the body's, which the booleans
        // below would write into.)
        const TopoDS_Shape region = BRepBuilderAPI_Copy(cx.Current()).Shape();
        const double region_volume = absolute_volume_of(region);
        if (region_volume <= 1.0e-9 * std::max(1.0, body_volume)) {
          continue;
        }
        // Its faces: on the face's surface (where it meets it), on the box's
        // sides, or on a face of the body (its walls).
        bool meets = false;
        bool known = true;
        std::vector<int> walls;
        for (TopExp_Explorer fx(region, TopAbs_FACE); fx.More() && known; fx.Next()) {
          const std::vector<gp_Pnt> points = samples(TopoDS::Face(fx.Current()));
          if (on_surface(points, surface, tolerance)) {
            meets = true;
            continue;
          }
          bool on_side = false;
          for (std::size_t k = 0; k < 6 && !on_side; ++k) {
            on_side = std::all_of(points.begin(), points.end(), [&](const gp_Pnt& p) {
              return std::abs(p.Coord(static_cast<int>(k % 3) + 1) - near[k]) <= tolerance;
            });
          }
          if (on_side) {
            continue;
          }
          int wall = -1;
          const TopoDS_Shape at = BRepBuilderAPI_MakeVertex(points.front()).Vertex();
          const gp_Pnt& q = points.front();
          for (int f = 0; f < body.face_count() && wall < 0; ++f) {
            const Box& b = face_boxes[static_cast<std::size_t>(f)];
            if (q.X() < b[0] || q.Y() < b[1] || q.Z() < b[2] || q.X() > b[3] || q.Y() > b[4] || q.Z() > b[5]) {
              continue;
            }
            if (f != n && on_surface(points, BRep_Tool::Surface(body.face(f)), tolerance) &&
                distance(at, body.face(f)) <= tolerance) {
              wall = f;
            }
          }
          if (wall < 0) {
            known = false;
          } else if (std::find(walls.begin(), walls.end(), wall) == walls.end()) {
            walls.push_back(wall);
          }
        }
        // (A region bounded by the other face of an edge lies past the edge.)
        const bool past = std::any_of(walls.begin(), walls.end(), [&](int w) {
          return std::find(spine.begin(), spine.end(), w) != spine.end();
        });
        if (!meets || !known || walls.empty() || past) {
          continue;
        }
        // Near the region, as far as the rounding there can change the
        // body past the walls (twice its contact distance).
        const Box around = meet(grown(bounds_of(region), 2.0 * contact + tolerance), within);
        if (empty(around)) {
          continue;
        }
        // The region's corners, which lie on the side of every wall the
        // region is on (a test of points before the booleans below).
        std::vector<gp_Pnt> corners_of_region;
        for (TopExp_Explorer vx(region, TopAbs_VERTEX); vx.More(); vx.Next()) {
          corners_of_region.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vx.Current())));
        }
        // (First the plane walls, by the corners' sides of their planes.)
        const bool past_a_plane = std::any_of(walls.begin(), walls.end(), [&](int w) {
          const BRepAdaptor_Surface wall(body.face(w), false);
          if (wall.GetType() != GeomAbs_Plane) {
            return false;
          }
          const std::pair<gp_Pnt, gp_Dir> at = point_and_normal(body.face(w));
          return std::any_of(corners_of_region.begin(), corners_of_region.end(), [&](const gp_Pnt& q) {
            const double side = gp_Vec(at.first, q).Dot(gp_Vec(at.second));
            return protrusion ? side > tolerance : side < -tolerance;
          });
        });
        if (past_a_plane) {
          continue;
        }
        std::optional<TopoDS_Shape> tool = solid_of(around);
        for (const int w : walls) {
          // Out of a cut-out's walls' material, into a protrusion's.
          const std::optional<TopoDS_Shape> beyond = side_of(body.face(w), !protrusion, around);
          const bool outside = beyond && std::any_of(corners_of_region.begin(), corners_of_region.end(),
                                                      [&](const gp_Pnt& p) {
                                                        BRepClass3d_SolidClassifier classifier(*beyond, p, tolerance);
                                                        return classifier.State() == TopAbs_OUT;
                                                      });
          if (!beyond || outside) {
            tool.reset();
            break;
          }
          tool = boolean<BRepAlgoAPI_Common>(*tool, *beyond);
        }
        if (!tool) {
          continue;
        }
        // The tool holds all of the region; a cut-out's none of the body, a
        // protrusion's no other part of the body past the face's surface.
        const double eps = 1.0e-6 * region_volume + 1.0e-12;
        if (absolute_volume_of(boolean<BRepAlgoAPI_Cut>(region, *tool)) > eps) {
          continue;
        }
        if (!protrusion && absolute_volume_of(boolean<BRepAlgoAPI_Common>(*tool, solid)) > eps) {
          continue;
        }
        if (protrusion) {
          const TopoDS_Shape kept = boolean<BRepAlgoAPI_Common>(*tool, solid);
          if (absolute_volume_of(boolean<BRepAlgoAPI_Common>(kept, *side)) > region_volume + eps) {
            continue;
          }
          tool = kept;
        }
        found.push_back({region, protrusion, *tool, walls});
      } catch (const Standard_Failure&) {
        // (A candidate the geometry kernel fails on is not taken.)
      }
    }
  }
  return found;
}

} // namespace

std::optional<Cutouts> fill_cutouts(const Shape& body, const std::vector<int>& owner, double contact,
                                    double reach) {
  if (!(contact > 0.0) || !(reach > 0.0)) {
    return std::nullopt;
  }
  throw_if_cancelled();
  return occt_guarded([&]() -> std::optional<Cutouts> {
    const TopoDS_Shape& solid = body.occt();
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(solid, TopAbs_VERTEX));
    // The selected edges and their faces.
    BRep_Builder builder;
    TopoDS_Compound edges;
    builder.MakeCompound(edges);
    ShapeSet selected;
    std::vector<int> faces;
    const EdgeFaces edge_faces(body);
    for (int e = 0; e < body.edge_count(); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      builder.Add(edges, body.edge(e));
      selected.Add(body.edge(e));
      for (const int f : edge_faces.at(e)) {
        if (std::find(faces.begin(), faces.end(), f) == faces.end()) {
          faces.push_back(f);
        }
      }
    }
    if (faces.empty()) {
      return std::nullopt;
    }
    // (Past the regions, twice the contact distance further, and as far as
    // the rounding changes the body.)
    const Box within = grown(bounds_of(edges), std::max(reach, 3.25 * contact) + tolerance);
    ShapeList fills, removes;
    TopoDS_Compound recut, rejoin;
    builder.MakeCompound(recut);
    builder.MakeCompound(rejoin);
    std::vector<int> walls;
    const std::vector<int> spine = faces;
    const double body_volume = absolute_volume_of(solid);
    // The body's faces' boxes, to look for a region's walls among the faces
    // near it only.
    std::vector<Box> face_boxes;
    for (int f = 0; f < body.face_count(); ++f) {
      face_boxes.push_back(grown(bounds_of(body.face(f)), tolerance));
    }
    for (const int n : spine) {
      for (const Region& r : regions_at(body, n, selected, spine, within, contact, tolerance, body_volume, face_boxes)) {
        (r.protrusion ? removes : fills).Append(r.solid);
        builder.Add(r.protrusion ? rejoin : recut, r.tool);
        for (const int w : r.walls) {
          if (std::find(walls.begin(), walls.end(), w) == walls.end()) {
            walls.push_back(w);
          }
        }
      }
    }
    if (fills.IsEmpty() && removes.IsEmpty()) {
      return std::nullopt;
    }
    for (const int w : walls) {
      if (std::find(faces.begin(), faces.end(), w) == faces.end()) {
        faces.push_back(w);
      }
    }
    std::vector<occ::handle<Geom_Surface>> surfaces;
    for (const int f : faces) {
      surfaces.push_back(BRep_Tool::Surface(body.face(f)));
    }
    // The body with the cut-outs filled and the protrusions taken away, the
    // pieces of the faces on one surface merged.
    occ::handle<BRepTools_History> history;
    TopoDS_Shape cleared = solid;
    if (!fills.IsEmpty()) {
      BRepAlgoAPI_Fuse fuse;
      cleared = boolean(cleared, fills, fuse);
      history = fuse.History();
    }
    if (!removes.IsEmpty()) {
      BRepAlgoAPI_Cut cut;
      cleared = boolean(cleared, removes, cut);
      if (history.IsNull()) {
        history = cut.History();
      } else {
        history->Merge(cut.History());
      }
    }
    const TopoDS_Shape filled = merged_on(cleared, surfaces, solid, tolerance, *history);
    if (!BRepCheck_Analyzer(filled).IsValid()) {
      return std::nullopt;
    }
    FaceNamer namer(filled);
    namer.carry(*history, body);
    namer.finish();
    Cutouts out;
    out.body = namer.shape();
    out.faces = faces;
    out.recut = recut;
    out.rejoin = rejoin;
    const Shape& m = *out.body;
    out.owner.assign(static_cast<std::size_t>(m.edge_count()), -1);
    for (int e = 0; e < body.edge_count(); ++e) {
      const int s = owner[static_cast<std::size_t>(e)];
      if (s < 0) {
        continue;
      }
      const TopoDS_Shape& edge = body.edge(e);
      const NCollection_List<TopoDS_Shape>& modified = history->Modified(edge);
      const int i = history->IsRemoved(edge) || modified.Extent() > 1
                        ? -1
                        : m.edge_index(modified.IsEmpty() ? edge : modified.First());
      if (i < 0) {
        return std::nullopt; // a selected edge split or gone
      }
      out.owner[static_cast<std::size_t>(i)] = s;
    }
    return out;
  });
}

namespace {

// The region within `box` on the side of the face's surface that holds
// `p`; none where `p` lies on the surface or the surface is not one
// side_of takes.
std::optional<TopoDS_Shape> side_holding(const TopoDS_Face& face, const gp_Pnt& p, const Box& box,
                                         double tolerance) {
  GeomAPI_ProjectPointOnSurf on(p, BRep_Tool::Surface(face));
  if (on.NbPoints() > 0 && on.LowerDistance() <= 10.0 * tolerance) {
    return std::nullopt;
  }
  for (const bool outer : {false, true}) {
    const std::optional<TopoDS_Shape> side = side_of(face, outer, box);
    if (!side) {
      return std::nullopt;
    }
    if (BRepClass3d_SolidClassifier(*side, p, tolerance).State() == TopAbs_IN) {
      return side;
    }
  }
  return std::nullopt;
}

// The face's surface continued past the face along itself (mitcad#1238):
// a plane over the box, a cylinder or cone along its axis `reach` past
// the face (a cone up to its apex), a sphere or torus whole, a surface of
// extrusion or revolution `reach` past the face where it is unbounded,
// another bounded surface (a B-spline) to its own bounds and extended
// `reach` further with matching tangents (GeomLib::ExtendSurfByLength). Directions in which the surface closes
// stay as they are. None where it cannot be made.
std::optional<TopoDS_Face> continued_face(const TopoDS_Face& face, const Box& box, double reach) {
  const occ::handle<Geom_Surface> located = BRep_Tool::Surface(face);
  if (located.IsNull()) {
    return std::nullopt;
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface adaptor(face, false);
  const auto made = [](const BRepBuilderAPI_MakeFace& maker) -> std::optional<TopoDS_Face> {
    return maker.IsDone() ? std::optional<TopoDS_Face>(maker.Face()) : std::nullopt;
  };
  switch (adaptor.GetType()) {
  case GeomAbs_Plane: {
    const gp_Pln plane = adaptor.Plane();
    double a0 = Precision::Infinite(), a1 = -Precision::Infinite();
    double b0 = Precision::Infinite(), b1 = -Precision::Infinite();
    for (int i = 0; i < 8; ++i) {
      const gp_Pnt corner(box[(i & 1) ? 3 : 0], box[(i & 2) ? 4 : 1], box[(i & 4) ? 5 : 2]);
      double u = 0.0, v = 0.0;
      ElSLib::Parameters(plane, corner, u, v);
      a0 = std::min(a0, u);
      a1 = std::max(a1, u);
      b0 = std::min(b0, v);
      b1 = std::max(b1, v);
    }
    const double margin = 0.1 * diagonal(box) + reach;
    return made(BRepBuilderAPI_MakeFace(plane, a0 - margin, a1 + margin, b0 - margin, b1 + margin));
  }
  case GeomAbs_Cylinder:
    return made(BRepBuilderAPI_MakeFace(adaptor.Cylinder(), 0.0, 4.0 * kHalfPi, v0 - reach, v1 + reach));
  case GeomAbs_Cone: {
    const gp_Cone cone = adaptor.Cone();
    const double apex = -cone.RefRadius() / std::sin(cone.SemiAngle());
    const double gap = 1.0e-6 * std::max(1.0, reach);
    const bool after = 0.5 * (v0 + v1) > apex;
    const double low = after ? std::max(apex + gap, v0 - reach) : v0 - reach;
    const double high = after ? v1 + reach : std::min(apex - gap, v1 + reach);
    return made(BRepBuilderAPI_MakeFace(cone, 0.0, 4.0 * kHalfPi, low, high));
  }
  case GeomAbs_Sphere:
    return made(BRepBuilderAPI_MakeFace(adaptor.Sphere()));
  case GeomAbs_Torus:
    return made(BRepBuilderAPI_MakeFace(adaptor.Torus()));
  default:
    break;
  }
  occ::handle<Geom_Surface> geometry = located;
  // A trimmed surface continues as its basis.
  while (const auto trimmed = occ::down_cast<Geom_RectangularTrimmedSurface>(geometry)) {
    geometry = trimmed->BasisSurface();
  }
  double a0 = 0.0, a1 = 0.0, b0 = 0.0, b1 = 0.0;
  geometry->Bounds(a0, a1, b0, b1);
  if (geometry->IsKind(STANDARD_TYPE(Geom_BoundedSurface))) {
    // Extended on a copy: the body's surface stays as it is.
    occ::handle<Geom_BoundedSurface> bounded = occ::down_cast<Geom_BoundedSurface>(geometry->Copy());
    const bool u_closed = geometry->IsUClosed() || geometry->IsUPeriodic();
    const bool v_closed = geometry->IsVClosed() || geometry->IsVPeriodic();
    for (int s = 0; s < 4; ++s) {
      if (!(s < 2 ? u_closed : v_closed)) {
        GeomLib::ExtendSurfByLength(bounded, reach, 1, s < 2, s == 1 || s == 3);
      }
    }
    geometry = bounded;
    geometry->Bounds(a0, a1, b0, b1);
  } else {
    // Surfaces of extrusion and revolution, offsets: infinite bounds
    // become the face's ones and `reach` more.
    a0 = Precision::IsInfinite(a0) ? u0 - reach : a0;
    a1 = Precision::IsInfinite(a1) ? u1 + reach : a1;
    b0 = Precision::IsInfinite(b0) ? v0 - reach : b0;
    b1 = Precision::IsInfinite(b1) ? v1 + reach : b1;
  }
  return made(BRepBuilderAPI_MakeFace(geometry, a0, a1, b0, b1, Precision::Confusion()));
}

// A point inside the face (on its surface, off its boundary), or none.
std::optional<gp_Pnt> inner_point(const TopoDS_Face& face) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
  const BRepAdaptor_Surface surface(face);
  for (const int n : {2, 4, 8}) {
    for (int i = 1; i < n; ++i) {
      for (int j = 1; j < n; ++j) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * i / n, v0 + (v1 - v0) * j / n);
        if (classifier.Perform(uv) == TopAbs_IN) {
          return surface.Value(uv.X(), uv.Y());
        }
      }
    }
  }
  return std::nullopt;
}

// A wall rising from face `face` (a region of the body on the face's
// outer side) continued into the material along its own faces
// (mitcad#1238): the body's material within `depth` under the wall's
// foot (its faces on the face's surface) on the material side of the
// face's surface, split by the wall's faces meeting the foot continued
// along their surfaces (continued_face), the pieces whose faces on the
// surface lie on the foot joined to the wall. The wall alone where it has
// no foot; throws where the pieces do not cover the foot (the continued
// faces not parting the material under it from that under the face).
TopoDS_Shape continued_into(const TopoDS_Shape& wall, const TopoDS_Face& face, const TopoDS_Shape& solid,
                            double depth, double tolerance) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  BRep_Builder builder;
  TopoDS_Compound foot;
  builder.MakeCompound(foot);
  bool any_foot = false;
  std::vector<TopoDS_Face> sides;
  for (TopExp_Explorer fx(wall, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face& f = TopoDS::Face(fx.Current());
    if (on_surface(samples(f), surface, tolerance)) {
      builder.Add(foot, f);
      any_foot = true;
    } else {
      sides.push_back(f);
    }
  }
  if (!any_foot) {
    return wall;
  }
  const Box box = grown(bounds_of(foot), depth + tolerance);
  const std::optional<TopoDS_Shape> under = side_of(face, false, box);
  if (!under) {
    throw Standard_Failure("no side of a narrow face to continue a wall into");
  }
  const TopoDS_Shape slab = boolean<BRepAlgoAPI_Common>(*under, solid);
  // (A surface extrapolated past its own bounds, a B-spline's, goes on
  // farther than the depth: a thread's flank, nearly level along the
  // thread, comes down that depth only far along it.)
  const double reach = std::max(depth, 0.1 * diagonal(box)) + tolerance;
  ShapeList tools;
  for (const TopoDS_Face& f : sides) {
    if (distance(f, foot) > tolerance) {
      continue;
    }
    if (const std::optional<TopoDS_Face> continued = continued_face(f, box, reach)) {
      tools.Append(*continued);
    }
  }
  TopoDS_Shape pieces = slab;
  if (!tools.IsEmpty()) {
    BRepAlgoAPI_Splitter splitter;
    ShapeList arguments;
    arguments.Append(slab);
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.SetNonDestructive(true);
    splitter.SetRunParallel(true);
    splitter.Build();
    if (!splitter.IsDone() || splitter.HasErrors()) {
      throw Standard_Failure("splitting a wall's continuation failed");
    }
    pieces = splitter.Shape();
  }
  // The pieces under the foot: each of their faces on the surface lies on
  // the wall, inside and on its boundary.
  const auto on_wall = [&](const gp_Pnt& p) {
    return distance(BRepBuilderAPI_MakeVertex(p).Vertex(), wall) <= 10.0 * tolerance;
  };
  const auto area_of = [](const TopoDS_Shape& shape) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape, props, 1.0e-9);
    return props.Mass();
  };
  ShapeList under_foot;
  double covered = 0.0;
  for (TopExp_Explorer px(pieces, TopAbs_SOLID); px.More(); px.Next()) {
    bool any = false;
    bool all = true;
    double top = 0.0;
    for (TopExp_Explorer fx(px.Current(), TopAbs_FACE); fx.More() && all; fx.Next()) {
      const TopoDS_Face& f = TopoDS::Face(fx.Current());
      std::vector<gp_Pnt> points = samples(f);
      if (!on_surface(points, surface, tolerance)) {
        continue;
      }
      if (const std::optional<gp_Pnt> inside = inner_point(f)) {
        points.push_back(*inside);
      }
      any = true;
      all = std::all_of(points.begin(), points.end(), on_wall);
      top += area_of(f);
    }
    if (any && all) {
      under_foot.Append(px.Current());
      covered += top;
    }
  }
  // The whole foot, or the continued faces did not part the material under
  // it from that under the face (a continuation not long enough).
  const double whole = area_of(foot);
  if (under_foot.IsEmpty() || std::abs(covered - whole) > 1.0e-6 * std::max(1.0, whole)) {
    throw Standard_Failure("a wall past a narrow face's far edge cannot be continued along its faces");
  }
  BRepAlgoAPI_Fuse fuse;
  return boolean(wall, under_foot, fuse);
}

// The space an overflowing chamfer of concave edges would hang out into
// past face `n`'s far edges (mitcad#1260), added to `recut`: next to the
// space filled past them (`filled`), for each far edge among `rims` that
// it meets, where the narrow face and the face beyond meet convexly, the
// region outside the body within `box` on the outer side of the face
// beyond (its surface continued) that meets that edge and not the selected
// edges (`spine`). The chamfer of the filled body is cut by it as by the
// face beyond, which grows out to meet the chamfer. `done` holds the faces
// beyond taken for `n` already.
void add_overhang(const Shape& body, int n, const TopoDS_Shape& filled, const TopoDS_Shape& rims,
                  const TopoDS_Shape& spine, const Box& box, double tolerance, double body_volume,
                  std::vector<int>& done, TopoDS_Compound& recut) {
  const EdgeFaces edge_faces(body);
  BRep_Builder builder;
  for (TopExp_Explorer rx(rims, TopAbs_EDGE); rx.More(); rx.Next()) {
    const TopoDS_Edge& rim = TopoDS::Edge(rx.Current());
    const int r = body.edge_index(rim);
    if (r < 0 || distance(filled, rim) > tolerance) {
      continue;
    }
    for (const int b : edge_faces.at(r)) {
      if (b == n || std::find(done.begin(), done.end(), b) != done.end() ||
          ChFi3d::DefineConnectType(rim, body.face(n), body.face(b), 1.0e-4, false) != ChFiDS_Convex) {
        continue;
      }
      const std::optional<TopoDS_Shape> side = side_of(body.face(b), true, box);
      if (!side) {
        continue;
      }
      done.push_back(b);
      const TopoDS_Shape beyond = boolean<BRepAlgoAPI_Cut>(*side, body.occt());
      for (TopExp_Explorer px(beyond, TopAbs_SOLID); px.More(); px.Next()) {
        if (distance(px.Current(), rim) > tolerance || distance(px.Current(), spine) <= tolerance ||
            absolute_volume_of(px.Current()) <= 1.0e-9 * std::max(1.0, body_volume)) {
          continue;
        }
        builder.Add(recut, BRepBuilderAPI_Copy(px.Current()).Shape());
      }
    }
  }
}

} // namespace

std::optional<Cutouts> fill_overflow(const Shape& body, const std::vector<int>& owner, double contact,
                                     double steepness, bool walls_too, bool at_corners) {
  if (!(contact > 0.0) || !(steepness > 0.0)) {
    return std::nullopt;
  }
  throw_if_cancelled();
  return occt_guarded([&]() -> std::optional<Cutouts> {
    const TopoDS_Shape& solid = body.occt();
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(solid, TopAbs_VERTEX));
    // The selected edges, their vertices and their faces.
    BRep_Builder builder;
    TopoDS_Compound edges;
    builder.MakeCompound(edges);
    ShapeSet selected;
    ShapeSet corners;
    std::vector<int> spine;
    const EdgeFaces edge_faces(body);
    for (int e = 0; e < body.edge_count(); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      builder.Add(edges, body.edge(e));
      selected.Add(body.edge(e));
      for (TopExp_Explorer vx(body.edge(e), TopAbs_VERTEX); vx.More(); vx.Next()) {
        corners.Add(vx.Current());
      }
      for (const int f : edge_faces.at(e)) {
        if (std::find(spine.begin(), spine.end(), f) == spine.end()) {
          spine.push_back(f);
        }
      }
    }
    if (spine.empty()) {
      return std::nullopt;
    }
    // The faces at the ends of the chains (at a vertex of one selected edge,
    // beside that edge's own faces), each with a point of that edge: the
    // regions are looked for on that point's side of them.
    NCollection_IndexedDataMap<TopoDS_Shape, ShapeList, TopTools_ShapeMapHasher> edges_at, faces_at;
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_FACE, faces_at);
    std::vector<std::pair<int, gp_Pnt>> ends;
    std::vector<int> end_faces;
    for (int e = 0; e < body.edge_count(); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      const TopoDS_Edge& edge = TopoDS::Edge(body.edge(e));
      const TopoDS_Vertex first = TopExp::FirstVertex(edge);
      const TopoDS_Vertex last = TopExp::LastVertex(edge);
      if (first.IsNull() || last.IsNull() || first.IsSame(last)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
      const std::vector<int> own = edge_faces.at(e);
      for (const TopoDS_Vertex& vertex : {first, last}) {
        if (!edges_at.Contains(vertex) || !faces_at.Contains(vertex)) {
          continue;
        }
        int chained = 0;
        for (const TopoDS_Shape& other : edges_at.FindFromKey(vertex)) {
          chained += selected.Contains(other) ? 1 : 0;
        }
        if (chained != 1) {
          continue;
        }
        for (const TopoDS_Shape& face : faces_at.FindFromKey(vertex)) {
          const int f = body.face_index(face);
          if (f < 0 || std::find(own.begin(), own.end(), f) != own.end()) {
            continue;
          }
          ends.emplace_back(f, middle);
          if (std::find(end_faces.begin(), end_faces.end(), f) == end_faces.end()) {
            end_faces.push_back(f);
          }
        }
      }
    }
    const double body_volume = absolute_volume_of(solid);
    std::vector<Box> face_boxes;
    for (int f = 0; f < body.face_count(); ++f) {
      face_boxes.push_back(grown(bounds_of(body.face(f)), tolerance));
    }
    ShapeList fills, removes;
    TopoDS_Compound recut, rejoin;
    builder.MakeCompound(recut);
    builder.MakeCompound(rejoin);
    std::vector<int> walls;
    std::vector<Cutouts::Wall> rising;
    // (At corners, each selected edge of a face on its own, with the points
    // where the regions met other selected edges' ends.)
    std::vector<gp_Pnt> loose;
    std::vector<std::pair<int, TopoDS_Shape>> groups;
    for (const int n : spine) {
      if (!at_corners) {
        groups.emplace_back(n, TopoDS_Shape());
        continue;
      }
      for (TopExp_Explorer ex(body.face(n), TopAbs_EDGE); ex.More(); ex.Next()) {
        if (selected.Contains(ex.Current())) {
          groups.emplace_back(n, ex.Current());
        }
      }
    }
    for (const auto& [n, single] : groups) {
      const TopoDS_Face& face = body.face(n);
      // The face's selected edges, and its rims: its other edges but those
      // that meet them (at corners: the edge, and the face's other edges but
      // the selected ones and those meeting the edge).
      ShapeSet ends_of_single;
      if (!single.IsNull()) {
        for (TopExp_Explorer vx(single, TopAbs_VERTEX); vx.More(); vx.Next()) {
          ends_of_single.Add(vx.Current());
        }
      }
      TopoDS_Compound own, rims;
      builder.MakeCompound(own);
      builder.MakeCompound(rims);
      bool any_rim = false;
      for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
        if (selected.Contains(ex.Current())) {
          if (single.IsNull() || single.IsSame(ex.Current())) {
            builder.Add(own, ex.Current());
          }
          continue;
        }
        bool meets = false;
        for (TopExp_Explorer vx(ex.Current(), TopAbs_VERTEX); vx.More() && !meets; vx.Next()) {
          meets = single.IsNull() ? corners.Contains(vx.Current()) : ends_of_single.Contains(vx.Current());
        }
        if (!meets && !BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
          builder.Add(rims, ex.Current());
          any_rim = true;
        }
      }
      if (!any_rim || distance(rims, own) > contact + tolerance) {
        continue;
      }
      // A chamfer of concave edges adds material (mitcad#1260): past a
      // convex far edge of the narrow face it would hang out over the face
      // beyond, which cuts it instead.
      bool concave = true;
      for (TopExp_Explorer ex(own, TopAbs_EDGE); ex.More() && concave; ex.Next()) {
        const int e = body.edge_index(ex.Current());
        const std::vector<int> pair = e < 0 ? std::vector<int>() : edge_faces.at(e);
        concave = pair.size() == 2 && ChFi3d::DefineConnectType(TopoDS::Edge(ex.Current()), body.face(pair[0]),
                                                                 body.face(pair[1]), 1.0e-4, false) == ChFiDS_Concave;
      }
      std::vector<int> trimmed_by;
      for (const bool protrusion : {false, true}) {
        if (protrusion && !walls_too) {
          continue;
        }
        // (A wall is looked for farther out: whether the chamfer comes out
        // of it again is told within that box.)
        // (At corners about all the selected edges, so that the regions of
        // the edges meeting there line up.)
        const Box near = grown(bounds_of(single.IsNull() ? TopoDS_Shape(own) : TopoDS_Shape(edges)),
                               (protrusion ? 3.0 : 1.25) * contact + tolerance);
        const std::optional<TopoDS_Shape> side = side_of(face, protrusion, near);
        if (!side) {
          continue;
        }
        // On the edges' side of the faces at the chains' ends too, where
        // nothing is found without that (the space past the far edge
        // running round an end to the edges).
        std::optional<TopoDS_Shape> clip;
        std::vector<std::pair<int, gp_Pnt>> clips = ends;
        if (!single.IsNull()) {
          // At corners: on the edge's side of every face at its ends but its
          // own two.
          clips.clear();
          const BRepAdaptor_Curve curve(TopoDS::Edge(single));
          const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
          const int e = body.edge_index(single);
          const std::vector<int> own_faces = e < 0 ? std::vector<int>() : edge_faces.at(e);
          for (ShapeSet::Iterator it(ends_of_single); it.More(); it.Next()) {
            if (!faces_at.Contains(it.Value())) {
              continue;
            }
            for (const TopoDS_Shape& at : faces_at.FindFromKey(it.Value())) {
              const int f = body.face_index(at);
              if (f >= 0 && std::find(own_faces.begin(), own_faces.end(), f) == own_faces.end()) {
                clips.emplace_back(f, middle);
              }
            }
          }
        }
        for (const auto& [f, p] : clips) {
          if (f == n) {
            continue;
          }
          if (const std::optional<TopoDS_Shape> at_end = side_holding(body.face(f), p, near, tolerance)) {
            clip = clip ? boolean<BRepAlgoAPI_Common>(*clip, *at_end) : *at_end;
          }
        }
        bool any = false;
        for (const bool clipped : {false, true}) {
          if (any || (clipped && !clip) || (!clipped && !single.IsNull() && clip)) {
            continue;
          }
          const TopoDS_Shape region = clipped ? boolean<BRepAlgoAPI_Common>(*side, *clip) : *side;
          const TopoDS_Shape candidates =
              protrusion ? boolean<BRepAlgoAPI_Common>(region, solid) : boolean<BRepAlgoAPI_Cut>(region, solid);
          for (TopExp_Explorer cx(candidates, TopAbs_SOLID); cx.More(); cx.Next()) {
            try {
              OCC_CATCH_SIGNALS
              // Past a far edge of the face, near the edges, not at them.
              // (At corners it may touch the other selected edges' ends.)
              if (distance(cx.Current(), rims) > tolerance || distance(cx.Current(), own) > contact + tolerance ||
                  distance(cx.Current(), single.IsNull() ? TopoDS_Shape(edges) : single) <= tolerance) {
                continue;
              }
              if (!single.IsNull()) {
                for (TopExp_Explorer vx(edges, TopAbs_VERTEX); vx.More(); vx.Next()) {
                  if (distance(cx.Current(), vx.Current()) <= tolerance) {
                    loose.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vx.Current())));
                  }
                }
              }
              const TopoDS_Shape found = BRepBuilderAPI_Copy(cx.Current()).Shape();
              if (absolute_volume_of(found) <= 1.0e-9 * std::max(1.0, body_volume)) {
                continue;
              }
              if (protrusion) {
                // (How deep the chamfer can go under the face's surface past
                // its far edge: its legs' ratio times how far its contact
                // reaches past the wall, and half again.)
                const double past = std::max(0.0, contact - distance(found, own));
                const double depth = 1.5 * steepness * past + 10.0 * tolerance;
                Cutouts::Wall wall;
                wall.solid = found;
                wall.face = n;
                wall.within = near;
                wall.continued = boolean<BRepAlgoAPI_Common>(continued_into(found, face, solid, depth, tolerance), solid);
                removes.Append(found);
                rising.push_back(wall);
              } else {
                fills.Append(found);
                builder.Add(recut, found);
                if (concave) {
                  add_overhang(body, n, found, rims, single.IsNull() ? TopoDS_Shape(edges) : single, near,
                               tolerance, body_volume, trimmed_by, recut);
                }
              }
              any = true;
              // Its walls: the body's faces it meets.
              const Box bounds = grown(bounds_of(found), tolerance);
              for (int f = 0; f < body.face_count(); ++f) {
                if (f == n || empty(meet(bounds, face_boxes[static_cast<std::size_t>(f)])) ||
                    std::find(walls.begin(), walls.end(), f) != walls.end()) {
                  continue;
                }
                if (distance(found, body.face(f)) <= tolerance) {
                  walls.push_back(f);
                }
              }
            } catch (const Standard_Failure&) {
              // (A candidate the geometry kernel fails on is not taken.)
            }
          }
        }
      }
    }
    if (fills.IsEmpty() && removes.IsEmpty()) {
      return std::nullopt;
    }
    std::vector<int> faces = spine;
    for (const std::vector<int>* more : {&end_faces, &walls}) {
      for (const int f : *more) {
        if (std::find(faces.begin(), faces.end(), f) == faces.end()) {
          faces.push_back(f);
        }
      }
    }
    std::vector<occ::handle<Geom_Surface>> surfaces;
    for (const int f : faces) {
      surfaces.push_back(BRep_Tool::Surface(body.face(f)));
    }
    // The body with the regions past the far edges filled and the walls
    // there taken away, the pieces of the faces on one surface merged.
    occ::handle<BRepTools_History> history;
    TopoDS_Shape cleared = solid;
    if (!fills.IsEmpty()) {
      BRepAlgoAPI_Fuse fuse;
      cleared = boolean(cleared, fills, fuse);
      history = fuse.History();
    }
    if (!removes.IsEmpty()) {
      BRepAlgoAPI_Cut cut;
      cleared = boolean(cleared, removes, cut);
      if (history.IsNull()) {
        history = cut.History();
      } else {
        history->Merge(cut.History());
      }
    }
    const TopoDS_Shape filled = merged_on(cleared, surfaces, solid, tolerance, *history, loose);
    int solids = 0;
    for (TopExp_Explorer ex(filled, TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1 || !BRepCheck_Analyzer(filled).IsValid()) {
      return std::nullopt;
    }
    FaceNamer namer(filled);
    namer.carry(*history, body);
    namer.finish();
    Cutouts out;
    out.body = namer.shape();
    out.faces = faces;
    out.recut = recut;
    out.rejoin = rejoin;
    out.rising = rising;
    out.nearest_names = true;
    out.spine = spine;
    const Shape& m = *out.body;
    out.owner.assign(static_cast<std::size_t>(m.edge_count()), -1);
    for (int e = 0; e < body.edge_count(); ++e) {
      const int s = owner[static_cast<std::size_t>(e)];
      if (s < 0) {
        continue;
      }
      const TopoDS_Shape& edge = body.edge(e);
      const NCollection_List<TopoDS_Shape>& modified = history->Modified(edge);
      const int i = history->IsRemoved(edge) || modified.Extent() > 1
                        ? -1
                        : m.edge_index(modified.IsEmpty() ? edge : modified.First());
      if (i < 0) {
        return std::nullopt; // a selected edge split or gone
      }
      out.owner[static_cast<std::size_t>(i)] = s;
    }
    return out;
  });
}

std::string overflow_failure(const Shape& body, const std::vector<int>& owner, double contact) {
  if (!(contact > 0.0)) {
    return {};
  }
  try {
    OCC_CATCH_SIGNALS
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(body.occt(), TopAbs_VERTEX));
    ShapeSet selected;
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] >= 0) {
        selected.Add(body.edge(e));
      }
    }
    NCollection_IndexedDataMap<TopoDS_Shape, ShapeList, TopTools_ShapeMapHasher> edges_at;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    const EdgeFaces edge_faces(body);
    std::string first;
    int more = 0;
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      ShapeSet ends;
      for (TopExp_Explorer vx(body.edge(e), TopAbs_VERTEX); vx.More(); vx.Next()) {
        ends.Add(vx.Current());
      }
      for (const int n : edge_faces.at(e)) {
        // The face's far edges nearer the edge than the chamfer reaches
        // (not those at the edge's own ends), and the faces beyond them.
        std::vector<std::string> beyond;
        for (TopExp_Explorer ex(body.face(n), TopAbs_EDGE); ex.More(); ex.Next()) {
          const int rim = body.edge_index(ex.Current());
          if (rim < 0 || selected.Contains(ex.Current()) || BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
            continue;
          }
          bool meets = false;
          for (TopExp_Explorer vx(ex.Current(), TopAbs_VERTEX); vx.More() && !meets; vx.Next()) {
            meets = ends.Contains(vx.Current());
          }
          if (meets || distance(ex.Current(), body.edge(e)) >= contact - tolerance) {
            continue;
          }
          for (const int f : edge_faces.at(rim)) {
            if (f != n && !body.face_names(f).empty() &&
                std::find(beyond.begin(), beyond.end(), body.face_names(f).front()) == beyond.end()) {
              beyond.push_back(body.face_names(f).front());
            }
          }
        }
        if (beyond.empty() || body.face_names(n).empty()) {
          continue;
        }
        if (!first.empty()) {
          ++more;
          continue;
        }
        const std::string& edge = body.edge_name(e);
        first = "the chamfer of " + (edge.empty() ? std::string("an unnamed edge") : "edge " + edge) +
                " is wider than face " + body.face_names(n).front() + " and cannot meet " +
                (beyond.size() == 1 ? "face " : "faces ") + beyond.front();
        for (std::size_t k = 1; k < beyond.size() && k < 3; ++k) {
          first += (k + 1 == beyond.size() || k == 2 ? " and " : ", ") + beyond[k];
        }
        first += " beyond it";
        // The corner where it meets other bevels, if any.
        for (ShapeSet::Iterator it(ends); it.More(); it.Next()) {
          if (!edges_at.Contains(it.Value())) {
            continue;
          }
          int others = 0;
          for (const TopoDS_Shape& other : edges_at.FindFromKey(it.Value())) {
            others += selected.Contains(other) && !other.IsSame(body.edge(e)) ? 1 : 0;
          }
          const std::optional<std::string> vertex = body.name_of_vertex(it.Value());
          if (others >= 1 && vertex) {
            const std::string bevels = others == 1 ? std::string("bevel of 1 more edge")
                                                   : "bevels of " + std::to_string(others) + " more edges";
            first += ", at its corner with the " + bevels + " at vertex " + *vertex;
            break;
          }
        }
      }
    }
    if (more > 0) {
      first += " (and " + std::to_string(more) + " more such " + (more == 1 ? "face" : "faces") + ")";
    }
    return first;
  } catch (const Standard_Failure&) {
    return {};
  }
}

std::string overflow_tangent(const Shape& body, const std::vector<int>& owner,
                             const std::function<double(int, int)>& reach) {
  try {
    OCC_CATCH_SIGNALS
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(body.occt(), TopAbs_VERTEX));
    // How near the contact may come to the far edge's extreme and count as
    // touching it: a tenth of a micrometre at least (a narrower strip of the
    // face would be left, mitcad#1106).
    const double touching = std::max(1.0e-4, 10.0 * tolerance);
    ShapeSet selected;
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] >= 0) {
        selected.Add(body.edge(e));
      }
    }
    const EdgeFaces edge_faces(body);
    std::vector<int> spine;
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      for (const int f : edge_faces.at(e)) {
        if (std::find(spine.begin(), spine.end(), f) == spine.end()) {
          spine.push_back(f);
        }
      }
    }
    for (const int n : spine) {
      // The face's selected edges with their reach along it, their
      // vertices, and its far edges: its other edges but those meeting them.
      std::vector<std::pair<int, double>> own;
      ShapeSet ends;
      double farthest = 0.0;
      for (TopExp_Explorer ex(body.face(n), TopAbs_EDGE); ex.More(); ex.Next()) {
        const int e = body.edge_index(ex.Current());
        if (e < 0 || !selected.Contains(ex.Current()) ||
            std::any_of(own.begin(), own.end(), [&](const std::pair<int, double>& o) { return o.first == e; })) {
          continue;
        }
        const double along = reach(e, n);
        if (!(along > 0.0)) {
          continue;
        }
        own.emplace_back(e, along);
        farthest = std::max(farthest, along);
        for (TopExp_Explorer vx(ex.Current(), TopAbs_VERTEX); vx.More(); vx.Next()) {
          ends.Add(vx.Current());
        }
      }
      if (own.empty()) {
        continue;
      }
      // How far past the contact a point lies (negative: short of it): the
      // least of its distances from the edges less their reach, and the
      // edge that gives it.
      const auto past = [&](const gp_Pnt& p, int* edge) {
        const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(p).Vertex();
        double least = Precision::Infinite();
        for (const auto& [e, along] : own) {
          const double gap = distance(vertex, body.edge(e)) - along;
          if (gap < least) {
            least = gap;
            if (edge != nullptr) {
              *edge = e;
            }
          }
        }
        return least;
      };
      for (TopExp_Explorer ex(body.face(n), TopAbs_EDGE); ex.More(); ex.Next()) {
        const TopoDS_Edge& rim = TopoDS::Edge(ex.Current());
        const int r = body.edge_index(rim);
        if (r < 0 || selected.Contains(rim) || BRep_Tool::Degenerated(rim)) {
          continue;
        }
        bool meets = false;
        for (TopExp_Explorer vx(rim, TopAbs_VERTEX); vx.More() && !meets; vx.Next()) {
          meets = ends.Contains(vx.Current());
        }
        if (meets || distance(rim, body.face(n)) > tolerance) {
          continue;
        }
        double nearest = Precision::Infinite();
        for (const auto& [e, along] : own) {
          nearest = std::min(nearest, distance(rim, body.edge(e)) - along);
        }
        // (Wholly past the contact: no tangent.)
        if (nearest > touching) {
          continue;
        }
        // The gap along the far edge, sampled; at each of its extremes (a
        // closed edge's ends joined), the extreme found by golden sections
        // and compared with nothing.
        const BRepAdaptor_Curve curve(rim);
        const double t0 = curve.FirstParameter();
        const double t1 = curve.LastParameter();
        const bool closed = TopExp::FirstVertex(rim).IsSame(TopExp::LastVertex(rim));
        constexpr int kSamples = 64;
        std::vector<double> gaps(kSamples + 1);
        for (int k = 0; k <= kSamples; ++k) {
          gaps[static_cast<std::size_t>(k)] = past(curve.Value(t0 + (t1 - t0) * k / kSamples), nullptr);
        }
        const double step = (t1 - t0) / kSamples;
        for (int k = closed ? 0 : 1; k < kSamples; ++k) {
          const double before = gaps[static_cast<std::size_t>(k == 0 ? kSamples - 1 : k - 1)];
          const double here = gaps[static_cast<std::size_t>(k)];
          const double after = gaps[static_cast<std::size_t>(k + 1)];
          const bool highest = here >= before && here >= after;
          const bool lowest = here <= before && here <= after;
          // (Neither, or level along the edge: an edge parallel to the
          // contact, which a contact meets along its length or not at all.)
          if ((!highest && !lowest) || (here == before && here == after) || std::abs(here) > 0.25 * farthest) {
            continue;
          }
          // (A parameter of a closed edge's span before its start taken
          // round.)
          const auto at = [&](double t) {
            if (closed && t < t0) {
              t += t1 - t0;
            }
            return curve.Value(t);
          };
          const double sign = highest ? 1.0 : -1.0;
          double a = t0 + (k - 1) * step;
          double b = t0 + (k + 1) * step;
          constexpr double kGolden = 0.6180339887498949;
          double c = b - kGolden * (b - a);
          double d = a + kGolden * (b - a);
          double fc = sign * past(at(c), nullptr);
          double fd = sign * past(at(d), nullptr);
          for (int i = 0; i < 60 && b - a > 1.0e-12 * std::max(1.0, std::abs(t1 - t0)); ++i) {
            if (fc > fd) {
              b = d;
              d = c;
              fd = fc;
              c = b - kGolden * (b - a);
              fc = sign * past(at(c), nullptr);
            } else {
              a = c;
              c = d;
              fc = fd;
              d = a + kGolden * (b - a);
              fd = sign * past(at(d), nullptr);
            }
          }
          const gp_Pnt p = at(0.5 * (a + b));
          int edge = -1;
          const double extreme = past(p, &edge);
          if (std::abs(extreme) > touching || edge < 0) {
            continue;
          }
          std::vector<std::string> beyond;
          for (const int f : edge_faces.at(r)) {
            if (f != n && !body.face_names(f).empty()) {
              beyond.push_back(body.face_names(f).front());
            }
          }
          const std::string& name = body.edge_name(edge);
          char point[96];
          std::snprintf(point, sizeof point, "(%.4g, %.4g, %.4g)", std::abs(p.X()) < 5.0e-5 ? 0.0 : p.X(),
                        std::abs(p.Y()) < 5.0e-5 ? 0.0 : p.Y(), std::abs(p.Z()) < 5.0e-5 ? 0.0 : p.Z());
          return "the chamfer of " + (name.empty() ? std::string("an unnamed edge") : "edge " + name) +
                 " reaches exactly to the far edge of face " +
                 (body.face_names(n).empty() ? std::string("?") : body.face_names(n).front()) +
                 (beyond.empty() ? std::string() : " where face " + beyond.front() + " lies beyond it") + ", at " +
                 point + ": its contact touches that edge there, so that the face would end in a point; it "
                 "cannot be built at this size";
        }
      }
    }
    return {};
  } catch (const Standard_Failure&) {
    return {};
  }
}

bool put_walls_back(Cutouts& cutouts, const Shape& chamfered, const std::string& generated, const Shape& input) {
  if (cutouts.rising.empty()) {
    return true;
  }
  throw_if_cancelled();
  return occt_guarded([&]() -> bool {
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(input.occt(), TopAbs_VERTEX));
    std::vector<TopoDS_Face> made;
    for (int f = 0; f < chamfered.face_count(); ++f) {
      const NameList& names = chamfered.face_names(f);
      if (std::any_of(names.begin(), names.end(),
                      [&](const std::string& name) { return name.compare(0, generated.size(), generated) == 0; })) {
        made.push_back(chamfered.face(f));
      }
    }
    BRep_Builder builder;
    TopoDS_Compound rejoin;
    builder.MakeCompound(rejoin);
    for (const Cutouts::Wall& wall : cutouts.rising) {
      TopoDS_Shape back = wall.continued;
      // The chamfer's face nearest the wall, and the wall's parts on its
      // open side at the narrow face's far edge.
      int best = -1;
      double nearest = Precision::Infinite();
      for (std::size_t k = 0; k < made.size(); ++k) {
        const double d = distance(made[k], wall.solid);
        if (d < nearest) {
          nearest = d;
          best = static_cast<int>(k);
        }
      }
      const std::optional<TopoDS_Shape> open =
          best >= 0 ? side_of(made[static_cast<std::size_t>(best)], true, wall.within) : std::nullopt;
      if (open) {
        ShapeList cut;
        bool closed = true;
        const TopoDS_Shape parts = boolean<BRepAlgoAPI_Common>(wall.solid, *open);
        for (TopExp_Explorer px(parts, TopAbs_SOLID); px.More(); px.Next()) {
          if (distance(px.Current(), input.face(wall.face)) > tolerance) {
            continue;
          }
          // (The chamfer comes out of the wall again only where the part
          // stays inside the box the wall was looked for in.)
          const Box b = bounds_of(px.Current());
          for (std::size_t i = 0; i < 3; ++i) {
            closed = closed && b[i] > wall.within[i] + 10.0 * tolerance &&
                     b[i + 3] < wall.within[i + 3] - 10.0 * tolerance;
          }
          cut.Append(px.Current());
        }
        if (!cut.IsEmpty() && closed) {
          BRepAlgoAPI_Cut operation;
          back = boolean(wall.solid, cut, operation);
        }
      }
      builder.Add(rejoin, back);
    }
    cutouts.rejoin = rejoin;
    return true;
  });
}

ShapePtr cut_out_again(const ShapePtr& rounded, const Cutouts& cutouts, const Shape& input) {
  if (!rounded) {
    return nullptr;
  }
  throw_if_cancelled();
  return occt_guarded([&]() -> ShapePtr {
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(input.occt(), TopAbs_VERTEX));
    const auto solids_of = [](const TopoDS_Shape& compound) {
      ShapeList list;
      for (TopExp_Explorer ex(compound, TopAbs_SOLID); ex.More(); ex.Next()) {
        list.Append(ex.Current());
      }
      return list;
    };
    occ::handle<BRepTools_History> history;
    TopoDS_Shape shape = rounded->occt();
    const ShapeList recut = solids_of(cutouts.recut);
    if (!recut.IsEmpty()) {
      BRepAlgoAPI_Cut cut;
      shape = boolean(shape, recut, cut);
      history = cut.History();
    }
    const ShapeList rejoin = solids_of(cutouts.rejoin);
    if (!rejoin.IsEmpty()) {
      BRepAlgoAPI_Fuse fuse;
      shape = boolean(shape, rejoin, fuse);
      if (history.IsNull()) {
        history = fuse.History();
      } else {
        history->Merge(fuse.History());
      }
    }
    if (history.IsNull()) {
      return nullptr;
    }
    std::vector<occ::handle<Geom_Surface>> surfaces;
    for (const int f : cutouts.faces) {
      surfaces.push_back(BRep_Tool::Surface(input.face(f)));
    }
    const TopoDS_Shape result = merged_on(shape, surfaces, input.occt(), tolerance, *history);
    int solids = 0;
    for (TopExp_Explorer ex(result, TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1 || !BRepCheck_Analyzer(result).IsValid()) {
      return nullptr;
    }
    FaceNamer namer(result);
    namer.carry(*history, *rounded);
    // A face the cut or the join made lies on a wall of a region: it is
    // that face of the input (grown out or back to the rounding), named
    // after it.
    for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
      if (namer.named(ex.Current())) {
        continue;
      }
      const std::vector<gp_Pnt> points = samples(TopoDS::Face(ex.Current()));
      int best = -1;
      double nearest = Precision::Infinite();
      for (const int f : cutouts.faces) {
        if (!on_surface(points, BRep_Tool::Surface(input.face(f)), tolerance)) {
          continue;
        }
        const double d = distance(ex.Current(), input.face(f));
        if (d < nearest) {
          nearest = d;
          best = f;
        }
      }
      // (A wall continued straight down: the nearest of the faces.)
      // (Of those it meets, the one holding most of its points: the wall's
      // foot is its top edge.)
      if (best < 0 && cutouts.nearest_names) {
        int most = -1;
        for (const int f : cutouts.faces) {
          if (std::find(cutouts.spine.begin(), cutouts.spine.end(), f) != cutouts.spine.end()) {
            continue;
          }
          const double d = distance(ex.Current(), input.face(f));
          if (d > tolerance && best >= 0) {
            continue;
          }
          int held = 0;
          for (const gp_Pnt& p : points) {
            held += distance(BRepBuilderAPI_MakeVertex(p).Vertex(), input.face(f)) <= 10.0 * tolerance ? 1 : 0;
          }
          if (held > most || (held == most && d < nearest)) {
            most = held;
            nearest = d;
            best = f;
          }
        }
      }
      if (best >= 0) {
        namer.add(ex.Current(), input.face_names(best), -1);
      }
    }
    namer.finish();
    ShapePtr named = namer.shape();
    named->copy_notes(*rounded);
    return named;
  });
}

} // namespace mitcad::geometry::detail
