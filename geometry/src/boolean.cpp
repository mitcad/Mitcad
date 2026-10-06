// SPDX-License-Identifier: MIT
#include "mitcad/geometry/boolean.hpp"

#include <algorithm>
#include <cmath>
#include <memory>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "history.hpp"
#include "mitcad/analysis/common.hpp"
#include "mitcad/geometry/query.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Shapes closer than this touch.
constexpr double kContact = 1.0e-6;

const char* operation_name(BooleanOp op) {
  switch (op) {
  case BooleanOp::Join:
    return "join";
  case BooleanOp::Cut:
    return "cut";
  case BooleanOp::Intersect:
    return "intersect";
  }
  return "boolean";
}

Bnd_Box box_of(const Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape.occt(), box);
  return box;
}

// A shape's faces with their boxes (enlarged by kContact).
struct FaceBoxes {
  std::vector<std::pair<TopoDS_Shape, Bnd_Box>> faces;

  explicit FaceBoxes(const TopoDS_Shape& shape) {
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      Bnd_Box box;
      BRepBndLib::Add(it.Current(), box);
      box.Enlarge(kContact);
      faces.emplace_back(it.Current(), box);
    }
  }

  // The faces (of those marked in `among`, else of all) that come within
  // kContact of a box.
  std::vector<bool> near(const Bnd_Box& box, const std::vector<bool>& among = {}) const {
    std::vector<bool> marks(faces.size(), false);
    for (std::size_t i = 0; i < faces.size(); ++i) {
      marks[i] = (among.empty() || among[i]) && !faces[i].second.IsOut(box);
    }
    return marks;
  }

  // The marked faces as a compound.
  TopoDS_Shape compound(const std::vector<bool>& marks) const {
    BRep_Builder builder;
    TopoDS_Compound found;
    builder.MakeCompound(found);
    for (std::size_t i = 0; i < faces.size(); ++i) {
      if (marks[i]) {
        builder.Add(found, faces[i].first);
      }
    }
    return found;
  }
};

bool any_marked(const std::vector<bool>& marks) {
  return std::find(marks.begin(), marks.end(), true) != marks.end();
}

// Whether two shapes come within `limit` of each other (true when that
// cannot be measured). They are measured together with two vertices a
// little more than `limit` apart, far from both: OCCT's distance measures
// only the pairs of vertices, edges and faces whose boxes are nearer than
// the nearest pair found so far, and its first pairs are vertices, so the
// edges and faces whose boxes are farther apart than `limit` are left out
// (they were measured curve to curve while the shapes' vertices were far
// apart, seconds for the faces of a large body near a tool).
bool within(const TopoDS_Shape& a, const TopoDS_Shape& b, double limit) {
  Bnd_Box box;
  BRepBndLib::Add(a, box);
  BRepBndLib::Add(b, box);
  if (box.IsVoid()) {
    return true;
  }
  const gp_Pnt high = box.CornerMax();
  const double away = box.CornerMin().Distance(high) + limit + 1.0;
  const gp_Pnt start(high.X() + away, high.Y() + away, high.Z() + away);
  const gp_Pnt end(start.X() + limit + kContact, start.Y(), start.Z());
  BRep_Builder builder;
  TopoDS_Compound with_a;
  builder.MakeCompound(with_a);
  builder.Add(with_a, a);
  builder.Add(with_a, BRepBuilderAPI_MakeVertex(start).Vertex());
  TopoDS_Compound with_b;
  builder.MakeCompound(with_b);
  builder.Add(with_b, b);
  builder.Add(with_b, BRepBuilderAPI_MakeVertex(end).Vertex());
  detail::Interrupt interrupt;
  BRepExtrema_DistShapeShape measure(with_a, with_b, Extrema_ExtFlag_MINMAX, Extrema_ExtAlgo_Grad,
                                     interrupt.range());
  throw_if_cancelled();
  return !measure.IsDone() || measure.Value() <= limit;
}

// Whether something inside `box` may come within kContact of `faces`: not
// when the faces are farther from the box's centre than half its diagonal
// and kContact. The distance of a point is quick to measure, that of two
// sets of faces is not (curve-to-curve distances between all their
// edges): measuring every copy of a 90-copy pattern against the faces
// near it took over a minute, though most were millimetres away.
bool may_reach(const Bnd_Box& box, const TopoDS_Shape& faces) {
  const gp_Pnt low = box.CornerMin();
  const gp_Pnt high = box.CornerMax();
  const gp_Pnt centre((low.XYZ() + high.XYZ()) / 2.0);
  return within(BRepBuilderAPI_MakeVertex(centre).Vertex(), faces,
                low.Distance(high) / 2.0 + kContact);
}

// The faces of `from` (of those marked in `among`) that may come within
// kContact of faces of `to` (of those marked in `to_among`): faces of `to`
// are near their box, and may_reach does not rule them out.
std::vector<bool> reaching(const FaceBoxes& from, const std::vector<bool>& among,
                           const FaceBoxes& to, const std::vector<bool>& to_among) {
  std::vector<bool> kept(from.faces.size(), false);
  for (std::size_t i = 0; i < from.faces.size(); ++i) {
    if (!among[i]) {
      continue;
    }
    const Bnd_Box& box = from.faces[i].second;
    const std::vector<bool> near = to.near(box, to_among);
    kept[i] = any_marked(near) && may_reach(box, to.compound(near));
  }
  return kept;
}

// Whether faces of `part` come within kContact of faces of `a`. Only the
// faces of each that may reach the other's are measured (reaching: the
// faces of a large body are mostly small, those of a tool often not).
bool faces_touch(const FaceBoxes& a, const TopoDS_Shape& part, const Bnd_Box& part_box,
                 const Bnd_Box& box_a) {
  const std::vector<bool> near_a = a.near(part_box);
  if (!any_marked(near_a) || !may_reach(part_box, a.compound(near_a))) {
    return false;
  }
  const FaceBoxes faces(part);
  const std::vector<bool> kept = reaching(faces, faces.near(box_a), a, near_a);
  if (!any_marked(kept)) {
    return false;
  }
  const std::vector<bool> kept_a = reaching(a, near_a, faces, kept);
  if (!any_marked(kept_a)) {
    return false;
  }
  return within(a.compound(kept_a), faces.compound(kept), kContact);
}

// A point of a shape (its first vertex), if it has one.
std::optional<gp_Pnt> some_point(const TopoDS_Shape& shape) {
  TopExp_Explorer vertex(shape, TopAbs_VERTEX);
  if (!vertex.More()) {
    return std::nullopt;
  }
  return BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
}

// Classifies points against the solids of a shape (each classifier set up
// once: on a large body that is the costly part).
class Inside {
public:
  explicit Inside(const TopoDS_Shape& shape) : m_shape(shape) {}

  bool operator()(const gp_Pnt& point) {
    if (m_classifiers.empty()) {
      for (TopExp_Explorer solid(m_shape, TopAbs_SOLID); solid.More(); solid.Next()) {
        m_classifiers.push_back(std::make_unique<BRepClass3d_SolidClassifier>(solid.Current()));
      }
    }
    for (const auto& classifier : m_classifiers) {
      classifier->Perform(point, kContact);
      if (classifier->State() == TopAbs_IN) {
        return true;
      }
    }
    return false;
  }

private:
  TopoDS_Shape m_shape;
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> m_classifiers;
};

// Whether the shapes touch or overlap: a solid inside the other counts.
// Each solid of `b` (a tool may be many copies) is measured against only
// the faces of `a` near it (faces_touch): the distance of a large body to
// a pattern's tool as a whole took over a minute.
bool touches(const Shape& a, const Shape& b) {
  Bnd_Box box_a = box_of(a);
  box_a.Enlarge(kContact);
  std::optional<FaceBoxes> faces_a;
  Inside inside_a(a.occt());
  std::vector<TopoDS_Shape> parts;
  for (TopExp_Explorer solid(b.occt(), TopAbs_SOLID); solid.More(); solid.Next()) {
    parts.push_back(solid.Current());
  }
  if (parts.empty()) {
    parts.push_back(b.occt());
  }
  for (const TopoDS_Shape& part : parts) {
    Bnd_Box box;
    BRepBndLib::Add(part, box);
    box.Enlarge(kContact);
    if (box.IsOut(box_a)) {
      continue;
    }
    if (!faces_a) {
      faces_a.emplace(a.occt());
    }
    if (faces_touch(*faces_a, part, box, box_a)) {
      return true;
    }
    // Apart at their boundaries: one inside the other, or apart.
    const std::optional<gp_Pnt> in_part = some_point(part);
    if (in_part && !box_a.IsOut(*in_part) && inside_a(*in_part)) {
      return true;
    }
    const std::optional<gp_Pnt> in_a = some_point(a.occt());
    if (in_a && !box.IsOut(*in_a) && Inside(part)(*in_a)) {
      return true;
    }
  }
  return false;
}

// Whether the one piece a cut or an intersection left of a target is the
// target changed: their volumes differ by more than 1e-9 relative. The
// difference is first measured on the faces the two do not share, the
// piece's and the target's reversed, in one integral (together they bound
// what changed): on a large body most faces stay the target's, and
// measuring both bodies whole took a fifth of the time of the features
// that cut them. `seen` is the copy of the target the operation worked on.
bool volume_changed(const Shape& target, const TopoDS_Shape& seen, const Shape& piece) {
  const auto whole = [&] {
    const double before = volume(target);
    return std::abs(volume(piece) - before) > 1.0e-9 * std::max(1.0, before);
  };
  if (target.measured() && piece.measured()) {
    return whole();
  }
  ShapeMap own;
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer it(seen, TopAbs_FACE); it.More(); it.Next()) {
    if (own.Add(it.Current()) <= static_cast<int>(faces.size())) {
      return whole(); // a face used twice (by two shells)
    }
    faces.push_back(it.Current());
  }
  std::vector<bool> kept(faces.size(), false);
  BRep_Builder builder;
  TopoDS_Compound change;
  builder.MakeCompound(change);
  for (TopExp_Explorer it(piece.occt(), TopAbs_FACE); it.More(); it.Next()) {
    const int index = own.FindIndex(it.Current());
    const auto at = static_cast<std::size_t>(index - 1);
    if (index > 0 && !kept[at] && faces[at].Orientation() == it.Current().Orientation()) {
      kept[at] = true;
    } else {
      builder.Add(change, it.Current());
    }
  }
  for (std::size_t i = 0; i < faces.size(); ++i) {
    if (!kept[i]) {
      builder.Add(change, faces[i].Reversed());
    }
  }
  const double difference = std::abs(analysis::volume_properties(change).Mass());
  // The limit is 1e-9 of the larger of 1 and the target's volume, which
  // its box bounds; in between, both volumes decide.
  if (difference <= 1.0e-9) {
    return false;
  }
  const Bnd_Box box = box_of(target);
  if (box.IsVoid()) {
    return whole();
  }
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  const double bound = (xmax - xmin) * (ymax - ymin) * (zmax - zmin);
  if (difference > 1.0e-9 * std::max(1.0, bound)) {
    return true;
  }
  return whole();
}

std::unique_ptr<BRepAlgoAPI_BooleanOperation> make_operation(BooleanOp op) {
  switch (op) {
  case BooleanOp::Join:
    return std::make_unique<BRepAlgoAPI_Fuse>();
  case BooleanOp::Cut:
    return std::make_unique<BRepAlgoAPI_Cut>();
  case BooleanOp::Intersect:
    return std::make_unique<BRepAlgoAPI_Common>();
  }
  throw std::invalid_argument("unknown boolean operation");
}

// Runs the operation of the tagged targets with the tool and names the
// result. It works on a copy of them (InputCopy, all copied together): the
// inputs are cached results, the result shares the sub-shapes the boolean
// kept with them, and the face merge after it (SimplifyResult, OCCT's
// ShapeUpgrade_UnifySameDomain, also in the safe input mode the
// non-destructive boolean asks for) gives the edges of the faces it merges
// p-curves on the merged face's surface in place (T0e, input_check.hpp).
// `seen` (if given) gets the copies of the targets, whose faces the result
// shares where it kept them.
detail::FaceNamer run_operation(BooleanOp op, const std::vector<std::pair<const Shape*, int>>& targets,
                                const Shape& tool, int tool_tag, bool simplify = true,
                                std::vector<TopoDS_Shape>* seen = nullptr) {
  std::vector<TopoDS_Shape> inputs;
  for (const auto& target : targets) {
    inputs.push_back(target.first->occt());
  }
  inputs.push_back(tool.occt());
  const detail::InputCopy copy(inputs);
  const auto operation = make_operation(op);
  NCollection_List<TopoDS_Shape> arguments;
  for (std::size_t i = 0; i < targets.size(); ++i) {
    arguments.Append(copy.shape(i));
  }
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(copy.shape(targets.size()));
  operation->SetArguments(arguments);
  operation->SetTools(tools);
  // As before T0e: OCCT copies the sub-shapes whose tolerances it widens.
  operation->SetNonDestructive(true);
  detail::build(*operation);
  if (!operation->IsDone() || operation->HasErrors()) {
    throw std::runtime_error("the boolean operation failed");
  }
  // Merge the coplanar faces and collinear edges the operation leaves behind.
  // The history then includes the merge.
  if (simplify) {
    operation->SimplifyResult();
  }
  detail::FaceNamer namer(operation->Shape());
  for (const auto& [target, tag] : targets) {
    namer.carry(*operation, *target, copy, tag);
  }
  namer.carry(*operation, tool, copy, tool_tag);
  namer.finish();
  if (simplify && !detail::is_valid(namer.result())) {
    // The merge can break a result the operation made valid (a join of a
    // corpus design: its merged faces made a shell that cannot be
    // oriented); then the result keeps the faces as the operation left
    // them.
    return run_operation(op, targets, tool, tool_tag, false, seen);
  }
  if (seen != nullptr) {
    seen->clear();
    for (std::size_t i = 0; i < targets.size(); ++i) {
      seen->push_back(copy.shape(i));
    }
  }
  if (!simplify) {
    detail::require_valid(namer.result(), "the boolean operation");
  }
  return namer;
}

} // namespace

BooleanResult boolean(BooleanOp op, const std::vector<const Shape*>& targets, const Shape& tool) {
  return detail::run(operation_name(op), [&] {
    BooleanResult result;
    result.touched.assign(targets.size(), false);
    const Bnd_Box tool_box = box_of(tool);
    const int tool_tag = static_cast<int>(targets.size());
    const auto pieces_of = [tool_tag](const detail::FaceNamer& namer) {
      std::vector<BooleanPiece> pieces;
      for (const detail::FaceNamer::Piece& piece : namer.pieces()) {
        BooleanPiece out{piece.shape, {}};
        for (int source : piece.sources) {
          if (source != tool_tag) {
            out.sources.push_back(static_cast<std::size_t>(source));
          }
        }
        pieces.push_back(std::move(out));
      }
      return pieces;
    };

    if (op == BooleanOp::Join) {
      // Only the targets the tool touches: fusing all of them would also
      // merge targets that overlap each other but not the tool.
      std::vector<std::pair<const Shape*, int>> near;
      for (std::size_t i = 0; i < targets.size(); ++i) {
        if (!tool_box.IsOut(box_of(*targets[i]))) {
          near.emplace_back(targets[i], static_cast<int>(i));
        }
      }
      if (near.empty()) {
        return result;
      }
      // One target near the tool is fused with it first, and the result
      // tells whether they touch: when a solid of it holds the target with
      // the tool, that union is the join and nothing needs measuring.
      // Several targets near the tool are measured first (touches) and only
      // those touched are fused: fusing all of them took many times as long
      // as the join itself (a large body and four small ones near the tool:
      // 9 s instead of 0.5 s), and when the tool touched two of them the
      // result could not tell, so they were fused again.
      if (near.size() == 1) {
        std::vector<detail::FaceNamer::Piece> solids;
        try {
          solids = run_operation(op, near, tool, tool_tag).pieces();
        } catch (const Cancelled&) {
          throw;
        } catch (...) {
          // Measured below; the union then fails again if it must.
          solids.clear();
        }
        const int target = near.front().second;
        // A target the tool swallows has no face left: measured below.
        const bool merged = std::any_of(solids.begin(), solids.end(), [&](const auto& piece) {
          return piece.sources.count(tool_tag) > 0 && piece.sources.count(target) > 0;
        });
        if (merged) {
          for (const detail::FaceNamer::Piece& piece : solids) {
            if (piece.sources.count(tool_tag) == 0) {
              continue; // a solid the tool does not reach, as it was
            }
            BooleanPiece out{piece.shape, {}};
            for (int source : piece.sources) {
              if (source != tool_tag) {
                out.sources.push_back(static_cast<std::size_t>(source));
                result.touched[static_cast<std::size_t>(source)] = true;
              }
            }
            result.pieces.push_back(std::move(out));
          }
          return result;
        }
      }
      std::vector<std::pair<const Shape*, int>> joined;
      for (const auto& [target, i] : near) {
        if (touches(*target, tool)) {
          joined.emplace_back(target, i);
          result.touched[static_cast<std::size_t>(i)] = true;
        }
      }
      if (joined.empty()) {
        return result;
      }
      result.pieces = pieces_of(run_operation(op, joined, tool, tool_tag));
      return result;
    }

    for (std::size_t i = 0; i < targets.size(); ++i) {
      if (tool_box.IsOut(box_of(*targets[i]))) {
        continue;
      }
      std::vector<TopoDS_Shape> seen;
      const detail::FaceNamer namer =
          run_operation(op, {{targets[i], static_cast<int>(i)}}, tool, tool_tag, true, &seen);
      const std::vector<detail::FaceNamer::Piece> solids = namer.pieces();
      // A single piece of the same volume is the target unchanged (a tool
      // touching a face shares the face but removes nothing); no piece at
      // all is a target cut away completely, or one the tool of an
      // intersection does not reach.
      bool changed = solids.empty() ? op == BooleanOp::Cut : solids.size() > 1;
      if (solids.size() == 1) {
        changed = volume_changed(*targets[i], seen.front(), *solids.front().shape);
      }
      if (!changed) {
        continue;
      }
      result.touched[i] = true;
      for (const detail::FaceNamer::Piece& piece : solids) {
        result.pieces.push_back({piece.shape, {i}});
      }
    }
    return result;
  });
}

} // namespace mitcad::geometry
