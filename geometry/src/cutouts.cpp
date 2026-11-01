// SPDX-License-Identifier: MIT
// Roundings that run over cut-outs of their faces (mitcad#311): the
// cut-outs filled back to the faces' surfaces and cut out again after the
// rounding (cutouts.hpp).
#include "cutouts.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <optional>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
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
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
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
// that are no vertices of `input` (none of the shape's other faces or the
// input's own edges are touched). Its history is added to `history`.
TopoDS_Shape merged_on(const TopoDS_Shape& made, const std::vector<occ::handle<Geom_Surface>>& surfaces,
                       const TopoDS_Shape& input, double tolerance, BRepTools_History& history) {
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
    corners.push_back(BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())));
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
