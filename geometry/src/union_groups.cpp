// SPDX-License-Identifier: MIT
#include "union_groups.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <initializer_list>
#include <memory>
#include <numeric>

#include <BOPAlgo_Alerts.hxx>
#include <BOPTools_AlgoTools3D.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <IntTools_Context.hxx>
#include <NCollection_List.hxx>
#include <OSD_Parallel.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <TopoDS_Compound.hxx>
// OSD_Parallel.hxx brings in windows.h on Windows, whose empty near and far
// macros would remove those names.
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif

#include "history.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// The root of i's set, its path halved on the way.
std::size_t root(std::vector<std::size_t>& parent, std::size_t i) {
  while (parent[i] != i) {
    parent[i] = parent[parent[i]];
    i = parent[i];
  }
  return i;
}

// The pairs of shapes whose boxes, enlarged by `gap`, meet (the lower index
// first); false when a shape has no box.
bool meeting_pairs(const std::vector<const Shape*>& shapes, double gap,
                   std::vector<std::pair<std::size_t, std::size_t>>& pairs) {
  const std::size_t count = shapes.size();
  std::vector<Bnd_Box> boxes(count);
  for (std::size_t i = 0; i < count; ++i) {
    BRepBndLib::Add(shapes[i]->occt(), boxes[i]);
    if (boxes[i].IsVoid()) {
      return false;
    }
    boxes[i].Enlarge(gap);
  }
  // Boxes sorted by their low x, each compared with those that start
  // before it ends (all pairs: 12 million for a pattern's 4900 copies).
  std::vector<std::size_t> order(count);
  std::iota(order.begin(), order.end(), std::size_t{0});
  std::sort(order.begin(), order.end(), [&boxes](std::size_t a, std::size_t b) {
    return boxes[a].CornerMin().X() < boxes[b].CornerMin().X();
  });
  for (std::size_t k = 0; k < count; ++k) {
    const Bnd_Box& box = boxes[order[k]];
    const double end = box.CornerMax().X();
    for (std::size_t m = k + 1; m < count && boxes[order[m]].CornerMin().X() <= end; ++m) {
      if (!box.IsOut(boxes[order[m]])) {
        pairs.emplace_back(std::min(order[k], order[m]), std::max(order[k], order[m]));
      }
    }
  }
  return true;
}

} // namespace

bool leaves_pieces_unused(const occ::handle<Message_Alert>& alert) {
  return alert->IsKind(STANDARD_TYPE(BOPAlgo_AlertFaceBuilderUnusedEdges)) ||
         alert->IsKind(STANDARD_TYPE(BOPAlgo_AlertSolidBuilderUnusedFaces)) ||
         alert->IsKind(STANDARD_TYPE(BOPAlgo_AlertAcquiredSelfIntersection));
}

std::vector<std::vector<std::size_t>> linked_groups(std::size_t count,
                                                    const std::vector<std::pair<std::size_t, std::size_t>>& links) {
  std::vector<std::size_t> parent(count);
  std::iota(parent.begin(), parent.end(), std::size_t{0});
  for (const auto& [first, second] : links) {
    const std::size_t a = root(parent, first);
    const std::size_t b = root(parent, second);
    // The smaller index as the root: groups in the order of their first
    // shape.
    parent[std::max(a, b)] = std::min(a, b);
  }
  std::vector<std::vector<std::size_t>> groups;
  std::vector<std::size_t> group_of(count, count);
  for (std::size_t i = 0; i < count; ++i) {
    const std::size_t r = root(parent, i);
    if (group_of[r] == count) {
      group_of[r] = groups.size();
      groups.emplace_back();
    }
    groups[group_of[r]].push_back(i);
  }
  return groups;
}

std::vector<std::vector<std::size_t>> box_groups(const std::vector<const Shape*>& shapes, double gap) {
  std::vector<std::pair<std::size_t, std::size_t>> pairs;
  if (!meeting_pairs(shapes, gap, pairs)) {
    std::vector<std::size_t> all(shapes.size());
    std::iota(all.begin(), all.end(), std::size_t{0});
    return {all};
  }
  return linked_groups(shapes.size(), pairs);
}

std::vector<std::pair<std::size_t, std::size_t>> box_pairs(const std::vector<const Shape*>& shapes, double gap) {
  std::vector<std::pair<std::size_t, std::size_t>> pairs;
  if (!meeting_pairs(shapes, gap, pairs)) {
    return {};
  }
  std::sort(pairs.begin(), pairs.end());
  return pairs;
}

namespace {

// A shape's vertices and faces with their boxes, for near_misses.
struct Parts {
  Bnd_Box box;
  std::vector<TopoDS_Vertex> vertices;
  std::vector<gp_Pnt> points;
  std::vector<TopoDS_Face> faces;
  std::vector<Bnd_Box> face_boxes;
};

Parts parts_of(const Shape& shape, double reach) {
  Parts parts;
  BRepBndLib::Add(shape.occt(), parts.box);
  parts.box.Enlarge(reach);
  ShapeMap vertices;
  TopExp::MapShapes(shape.occt(), TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    parts.vertices.push_back(TopoDS::Vertex(vertices(i)));
    parts.points.push_back(BRep_Tool::Pnt(parts.vertices.back()));
  }
  for (int i = 0; i < shape.face_count(); ++i) {
    parts.faces.push_back(shape.face(i));
    Bnd_Box box;
    BRepBndLib::Add(parts.faces.back(), box, false);
    box.Enlarge(reach);
    parts.face_boxes.push_back(box);
  }
  return parts;
}

// Whether a vertex of `a` lies near a face of `b` but not on it.
bool vertex_near_face(const Parts& a, const Parts& b, double reach) {
  for (std::size_t v = 0; v < a.points.size(); ++v) {
    const gp_Pnt& point = a.points[v];
    if (b.box.IsOut(point)) {
      continue;
    }
    const double vertex_tolerance = BRep_Tool::Tolerance(a.vertices[v]);
    for (std::size_t f = 0; f < b.faces.size(); ++f) {
      if (b.face_boxes[f].IsOut(point)) {
        continue;
      }
      // (A distance OCCT fails to measure is no near miss: the union of
      // all meets what lies there.)
      double d = 0.0;
      try {
        BRepExtrema_DistShapeShape distance(a.vertices[v], b.faces[f], Extrema_ExtFlag_MIN);
        if (!distance.IsDone()) {
          continue;
        }
        d = distance.Value();
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (const Standard_Failure&) {
        continue;
      }
      if (d > vertex_tolerance + BRep_Tool::Tolerance(b.faces[f]) && d <= reach) {
        return true;
      }
    }
  }
  return false;
}

} // namespace

std::vector<char> near_misses(const std::vector<const Shape*>& shapes,
                              const std::vector<std::pair<std::size_t, std::size_t>>& pairs, double reach) {
  std::vector<Parts> parts(shapes.size());
  parallel_jobs(shapes.size(), [&](std::size_t i) { parts[i] = parts_of(*shapes[i], reach); });
  std::vector<char> missed(pairs.size(), 0);
  parallel_jobs(pairs.size(), [&](std::size_t k) {
    const Parts& a = parts[pairs[k].first];
    const Parts& b = parts[pairs[k].second];
    missed[k] = static_cast<char>(vertex_near_face(a, b, reach) || vertex_near_face(b, a, reach) ? 1 : 0);
  });
  return missed;
}

void parallel_jobs(std::size_t count, const std::function<void(std::size_t)>& job) {
  std::vector<std::exception_ptr> errors(count);
  const CancelSource* source = cancel_source();
  OSD_Parallel::For(
      0, static_cast<int>(count),
      [&](int j) {
        const std::size_t k = static_cast<std::size_t>(j);
        // (As an operation of its own on the thread, so that OCCT's faults
        // there are its errors.)
        const auto work = [&] { job(k); };
        try {
          if (source != nullptr) {
            const CancelScope scope(*source);
            run("union", work);
          } else {
            run("union", work);
          }
        } catch (...) {
          errors[k] = std::current_exception();
        }
      },
      count < 2);
  for (const std::exception_ptr& error : errors) {
    if (error) {
      std::rethrow_exception(error);
    }
  }
}

ShapePtr solids_compound(const std::vector<const Shape*>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const Shape* shape : shapes) {
    for (TopExp_Explorer it(shape->occt(), TopAbs_SOLID); it.More(); it.Next()) {
      builder.Add(compound, it.Current());
    }
  }
  FaceNamer namer(compound);
  for (const Shape* shape : shapes) {
    for (int i = 0; i < shape->face_count(); ++i) {
      for (const std::string& name : shape->face_names(i)) {
        namer.add(shape->face(i), name);
      }
    }
  }
  namer.finish();
  return namer.shape();
}

ShapePtr grouped_union(const std::vector<const Shape*>& shapes,
                       const std::vector<std::vector<std::size_t>>& groups,
                       const std::function<ShapePtr(const std::vector<const Shape*>&)>& unite_group) {
  // The groups of several, the largest first so that it does not start
  // last on a thread of its own.
  std::vector<std::size_t> jobs;
  for (std::size_t g = 0; g < groups.size(); ++g) {
    if (groups[g].size() > 1) {
      jobs.push_back(g);
    }
  }
  std::stable_sort(jobs.begin(), jobs.end(),
                   [&groups](std::size_t a, std::size_t b) { return groups[a].size() > groups[b].size(); });
  std::vector<ShapePtr> united(groups.size());
  parallel_jobs(jobs.size(), [&](std::size_t j) {
    const std::size_t g = jobs[j];
    std::vector<const Shape*> members;
    for (const std::size_t i : groups[g]) {
      members.push_back(shapes[i]);
    }
    united[g] = unite_group(members);
  });
  std::vector<const Shape*> parts;
  bool valid = true;
  for (std::size_t g = 0; g < groups.size(); ++g) {
    parts.push_back(groups[g].size() > 1 ? united[g].get() : shapes[groups[g].front()]);
    valid = valid && parts.back()->checked_valid().value_or(false);
  }
  ShapePtr compound = solids_compound(parts);
  // Parts apart, each found valid by OCCT's checker: so is their compound.
  if (valid) {
    compound->set_checked_valid(true);
  }
  return compound;
}

namespace {

// How many of a solid's faces points_inside starts from, and how many of
// the points it keeps.
constexpr std::size_t kInsideFaces = 8;
constexpr std::size_t kInsideKept = 2;

// The classifier's tolerance for a point inside an input (mm).
constexpr double kInsideTolerance = 1.0e-7;


std::vector<gp_Pnt> solid_points(const TopoDS_Solid& solid, const occ::handle<IntTools_Context>& context) {
  Bnd_Box box;
  BRepBndLib::Add(solid, box);
  if (box.IsVoid()) {
    return {};
  }
  // Where the line meets the face it starts from, or a face at its start's
  // edge, is not where it leaves the solid.
  const double start = std::max(1.0e-6, 1.0e-9 * std::sqrt(box.SquareExtent()));
  BRepClass3d_SolidClassifier classifier(solid);
  IntCurvesFace_ShapeIntersector lines;
  lines.Load(solid, kInsideTolerance);
  // Each point with its depth (its distance from the face it started from).
  std::vector<std::pair<double, gp_Pnt>> found;
  std::size_t tried = 0;
  for (TopExp_Explorer it(solid, TopAbs_FACE); it.More() && tried < kInsideFaces; it.Next(), ++tried) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    // (A face where OCCT fails gives no point.)
    try {
      gp_Pnt on;
      gp_Pnt2d uv;
      if (BOPTools_AlgoTools3D::PointInFace(face, on, uv, context) != 0) {
        continue;
      }
      // The normal as the face is oriented in the solid: outward.
      gp_Pnt at;
      gp_Vec normal;
      BRepGProp_Face(face).Normal(uv.X(), uv.Y(), at, normal);
      if (normal.Magnitude() <= gp::Resolution()) {
        continue;
      }
      // Inward, and the other way where the classifier finds that is not
      // (a face oriented the wrong way).
      for (const double sense : {-1.0, 1.0}) {
        const gp_Dir direction(normal * sense);
        lines.Perform(gp_Lin(on, direction), start, Precision::Infinite());
        if (!lines.IsDone()) {
          continue;
        }
        double nearest = Precision::Infinite();
        for (int k = 1; k <= lines.NbPnt(); ++k) {
          nearest = std::min(nearest, lines.WParameter(k));
        }
        if (nearest >= Precision::Infinite()) {
          continue;
        }
        const double depth = 0.5 * nearest;
        const gp_Pnt point = on.Translated(gp_Vec(direction) * depth);
        classifier.Perform(point, kInsideTolerance);
        if (classifier.State() == TopAbs_IN) {
          found.emplace_back(depth, point);
          break;
        }
      }
    } catch (const Standard_OutOfMemory&) {
      throw;
    } catch (const Standard_Failure&) {
      continue;
    }
  }
  std::stable_sort(found.begin(), found.end(),
                   [](const std::pair<double, gp_Pnt>& a, const std::pair<double, gp_Pnt>& b) {
                     return a.first > b.first;
                   });
  std::vector<gp_Pnt> points;
  for (std::size_t k = 0; k < found.size() && k < kInsideKept; ++k) {
    points.push_back(found[k].second);
  }
  return points;
}

} // namespace

std::vector<InsidePoints> points_inside(const std::vector<const Shape*>& shapes) {
  std::vector<std::vector<InsidePoints>> of_shape(shapes.size());
  parallel_jobs(shapes.size(), [&](std::size_t i) {
    const occ::handle<IntTools_Context> context = new IntTools_Context();
    for (TopExp_Explorer it(shapes[i]->occt(), TopAbs_SOLID); it.More(); it.Next()) {
      // (A solid OCCT fails on has no points.)
      std::vector<gp_Pnt> points;
      try {
        points = solid_points(TopoDS::Solid(it.Current()), context);
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (const Standard_Failure&) {
        points.clear();
      }
      of_shape[i].push_back(InsidePoints{i, std::move(points)});
    }
  });
  std::vector<InsidePoints> points;
  for (std::vector<InsidePoints>& solids : of_shape) {
    for (InsidePoints& solid : solids) {
      points.push_back(std::move(solid));
    }
  }
  return points;
}

std::vector<std::size_t> missing_inputs(const TopoDS_Shape& result, const std::vector<InsidePoints>& points,
                                        double tolerance) {
  // The result's solids with their boxes; a classifier made for a solid
  // when a point first falls in its box.
  struct Part {
    TopoDS_Shape solid;
    Bnd_Box box;
    std::unique_ptr<BRepClass3d_SolidClassifier> classifier;
  };
  std::vector<Part> parts;
  for (TopExp_Explorer it(result, TopAbs_SOLID); it.More(); it.Next()) {
    Part part;
    part.solid = it.Current();
    BRepBndLib::Add(part.solid, part.box);
    part.box.Enlarge(tolerance);
    parts.push_back(std::move(part));
  }
  const auto held = [&parts, tolerance](const gp_Pnt& point) {
    for (Part& part : parts) {
      if (part.box.IsVoid() || part.box.IsOut(point)) {
        continue;
      }
      // (A point OCCT fails to classify counts as held: the check does not
      // make a union wrong where it cannot tell.)
      try {
        if (!part.classifier) {
          part.classifier = std::make_unique<BRepClass3d_SolidClassifier>(part.solid);
        }
        part.classifier->Perform(point, tolerance);
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (const Standard_Failure&) {
        return true;
      }
      const TopAbs_State state = part.classifier->State();
      if (state == TopAbs_IN || state == TopAbs_ON || state == TopAbs_UNKNOWN) {
        return true;
      }
    }
    return false;
  };
  std::vector<std::size_t> missing;
  for (const InsidePoints& solid : points) {
    if (solid.points.empty() || (!missing.empty() && missing.back() == solid.shape)) {
      continue;
    }
    bool kept = false;
    for (const gp_Pnt& point : solid.points) {
      kept = kept || held(point);
    }
    if (!kept) {
      missing.push_back(solid.shape);
    }
  }
  return missing;
}

namespace {

// The volume of the shape's solids.
double solid_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return std::abs(props.Mass());
}

// How far a box may stand out of another's for the shape in it to count as
// possibly inside the other (mm).
constexpr double kContainedReach = 1.0e-3;

// Whether `inner` lies in `outer` enlarged by kContainedReach.
bool box_within(const Bnd_Box& inner, const Bnd_Box& outer) {
  if (inner.IsVoid() || outer.IsVoid()) {
    return false;
  }
  Bnd_Box wide = outer;
  wide.Enlarge(kContainedReach);
  return !wide.IsOut(inner.CornerMin()) && !wide.IsOut(inner.CornerMax());
}

} // namespace

namespace {

// How far a point of a shape on the other's boundary may lie from it
// (mm), for lies_inside.
constexpr double kOnTolerance = 1.0e-6;

// How many points along each edge, and across each face's parameter
// range in each direction, lies_inside samples.
constexpr int kEdgeSamples = 5;
constexpr int kFaceSamples = 5;

// The solids of a shape, each with its box and a classifier made when a
// point first falls in its box.
class Holder {
public:
  explicit Holder(const TopoDS_Shape& shape) {
    for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
      Part part;
      part.solid = it.Current();
      BRepBndLib::Add(part.solid, part.box);
      part.box.Enlarge(kOnTolerance);
      m_parts.push_back(std::move(part));
    }
  }

  // Whether the point lies in or on one of the solids; false where OCCT
  // cannot tell.
  bool holds(const gp_Pnt& point) {
    for (Part& part : m_parts) {
      if (part.box.IsVoid() || part.box.IsOut(point)) {
        continue;
      }
      try {
        if (!part.classifier) {
          part.classifier = std::make_unique<BRepClass3d_SolidClassifier>(part.solid);
        }
        part.classifier->Perform(point, kOnTolerance);
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (const Standard_Failure&) {
        continue;
      }
      const TopAbs_State state = part.classifier->State();
      if (state == TopAbs_IN || state == TopAbs_ON) {
        return true;
      }
    }
    return false;
  }

private:
  struct Part {
    TopoDS_Shape solid;
    Bnd_Box box;
    std::unique_ptr<BRepClass3d_SolidClassifier> classifier;
  };
  std::vector<Part> m_parts;
};

} // namespace

bool lies_inside(const Shape& shape, const Shape& other) {
  Holder holder(other.occt());
  // Points inside its solids.
  const std::vector<InsidePoints> inside = points_inside({&shape});
  if (inside.empty()) {
    return false;
  }
  for (const InsidePoints& solid : inside) {
    if (solid.points.empty()) {
      return false;
    }
    for (const gp_Pnt& point : solid.points) {
      if (!holder.holds(point)) {
        return false;
      }
    }
  }
  // Its vertices.
  ShapeMap vertices;
  TopExp::MapShapes(shape.occt(), TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    if (!holder.holds(BRep_Tool::Pnt(TopoDS::Vertex(vertices(i))))) {
      return false;
    }
  }
  // Points along its edges.
  ShapeMap edges;
  TopExp::MapShapes(shape.occt(), TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    for (int k = 1; k <= kEdgeSamples; ++k) {
      if (!holder.holds(curve.Value(first + (last - first) * k / (kEdgeSamples + 1)))) {
        return false;
      }
    }
  }
  // Points across its faces, those inside each face.
  for (int i = 0; i < shape.face_count(); ++i) {
    const TopoDS_Face face = shape.face(i);
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    BRepTopAdaptor_FClass2d within(face, Precision::PConfusion());
    for (int a = 1; a <= kFaceSamples; ++a) {
      for (int b = 1; b <= kFaceSamples; ++b) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * a / (kFaceSamples + 1), v0 + (v1 - v0) * b / (kFaceSamples + 1));
        if (within.Perform(uv) != TopAbs_IN) {
          continue;
        }
        if (!holder.holds(surface.Value(uv.X(), uv.Y()))) {
          return false;
        }
      }
    }
  }
  return true;
}

std::vector<std::size_t> contained_inputs(const std::vector<const Shape*>& shapes) {
  const std::size_t count = shapes.size();
  std::vector<Bnd_Box> boxes(count);
  std::vector<char> solids(count, 0);
  for (std::size_t i = 0; i < count; ++i) {
    // Tight boxes: a box from the faces' surfaces may stand out of the
    // shape by more than the reach, and differently for two shapes with
    // the same faces trimmed differently.
    BRepBndLib::AddOptimal(shapes[i]->occt(), boxes[i], false, false);
    solids[i] = static_cast<char>(TopExp_Explorer(shapes[i]->occt(), TopAbs_SOLID).More() ? 1 : 0);
  }
  std::vector<char> left_out(count, 0);
  std::vector<std::size_t> contained;
  for (std::size_t i = 0; i < count; ++i) {
    if (solids[i] == 0) {
      continue;
    }
    double own = -1.0;
    for (std::size_t j = 0; j < count; ++j) {
      if (j == i || left_out[j] != 0 || solids[j] == 0 || !box_within(boxes[i], boxes[j])) {
        continue;
      }
      // (A shape OCCT fails on stays in the union.)
      try {
        if (own < 0.0) {
          own = solid_volume(shapes[i]->occt());
        }
        if (own <= 0.0) {
          break;
        }
        if (lies_inside(*shapes[i], *shapes[j])) {
          left_out[i] = 1;
          contained.push_back(i);
          break;
        }
      } catch (const Standard_OutOfMemory&) {
        throw;
      } catch (const Standard_Failure&) {
        continue;
      }
    }
  }
  return contained;
}

} // namespace mitcad::geometry::detail
