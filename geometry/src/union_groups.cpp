// SPDX-License-Identifier: MIT
#include "union_groups.hpp"

#include <algorithm>
#include <exception>
#include <numeric>

#include <BOPAlgo_Alerts.hxx>
#include <BRepBndLib.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <OSD_Parallel.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
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

} // namespace mitcad::geometry::detail
