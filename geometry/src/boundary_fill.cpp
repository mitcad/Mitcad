// SPDX-License-Identifier: MIT
#include "mitcad/geometry/boundary_fill.hpp"

#include <cstddef>
#include <cstdio>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

#include <BOPAlgo_CellsBuilder.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepTools_History.hxx>
#include <Message_ProgressRange.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/user_error.hpp"

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// A point as agents read it: "(10, 5, 2.5)" (mitcad#1612: the person reads
// "a point", the point painted).
std::string point_text(const gp_Pnt& p) {
  char text[128];
  std::snprintf(text, sizeof text, "(%g, %g, %g)", p.X(), p.Y(), p.Z());
  return text;
}

// Classifies points against the solids of one tool (each classifier set up
// once).
class ToolSolids {
public:
  explicit ToolSolids(const TopoDS_Shape& shape) {
    for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
      m_classifiers.push_back(std::make_unique<BRepClass3d_SolidClassifier>(solid.Current()));
    }
  }

  bool empty() const { return m_classifiers.empty(); }

  // IN inside a solid, else ON on the boundary of one, else OUT.
  TopAbs_State state(const gp_Pnt& point) {
    TopAbs_State found = TopAbs_OUT;
    for (const auto& classifier : m_classifiers) {
      classifier->Perform(point, Precision::Confusion());
      if (classifier->State() == TopAbs_IN) {
        return TopAbs_IN;
      }
      if (classifier->State() == TopAbs_ON) {
        found = TopAbs_ON;
      }
    }
    return found;
  }

private:
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> m_classifiers;
};

// The cell with the pieces of a face that it has side by side merged
// again (OCCT's ShapeUpgrade_UnifySameDomain), the names of `pieces`
// carried over; null when the merge fails or leaves an invalid shape.
// `pieces` is not finished: a merged face takes its pieces' names once.
ShapePtr merged(const detail::FaceNamer& pieces) {
  try {
    OCC_CATCH_SIGNALS
    ShapeUpgrade_UnifySameDomain unify(pieces.result(), true, true, false);
    unify.Build();
    const TopoDS_Shape result = unify.Shape();
    if (result.IsNull() || !detail::is_valid(result)) {
      return nullptr;
    }
    const occ::handle<BRepTools_History> history = unify.History();
    detail::FaceNamer namer(result);
    namer.adopt(pieces, [&](const TopoDS_Shape& face) -> TopoDS_Shape {
      if (history.IsNull()) {
        return TopoDS_Shape();
      }
      const NCollection_List<TopoDS_Shape>& images = history->Modified(face);
      return images.IsEmpty() ? TopoDS_Shape() : images.First();
    });
    namer.finish();
    namer.set_unified(true);
    namer.set_valid(true); // checked above
    return namer.shape();
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

} // namespace

std::vector<BoundaryCell> boundary_cells(const std::vector<const Shape*>& tools,
                                         const std::vector<gp_Pnt>& points) {
  return detail::run("boundary fill", [&] {
    if (tools.empty()) {
      throw std::runtime_error("no tool bodies");
    }
    std::vector<TopoDS_Shape> inputs;
    for (const Shape* tool : tools) {
      detail::require_surfaces(tool->occt(), "boundary fill");
      inputs.push_back(tool->occt());
    }
    // OCCT's general fuse writes into the sub-shapes of its arguments: it
    // works on copies of the tools (history.hpp).
    const detail::InputCopy copy(inputs);

    // The tools that hold each point.
    std::vector<ToolSolids> solids;
    for (std::size_t i = 0; i < tools.size(); ++i) {
      solids.emplace_back(copy.shape(i));
      if (solids.back().empty()) {
        throw std::runtime_error("tool " + std::to_string(i + 1) + " has no solid");
      }
    }
    std::vector<std::vector<std::size_t>> holders(points.size());
    for (std::size_t p = 0; p < points.size(); ++p) {
      for (std::size_t i = 0; i < tools.size(); ++i) {
        const TopAbs_State state = solids[i].state(points[p]);
        if (state == TopAbs_ON) {
          const std::string tool = " lies on the boundary of tool " + std::to_string(i + 1);
          throw std::runtime_error(
              placed_error("a point" + tool, "the point " + point_text(points[p]) + tool, {point_place(points[p])}));
        }
        if (state == TopAbs_IN) {
          holders[p].push_back(i);
        }
      }
      if (holders[p].empty()) {
        const std::string none = " is inside no tool body: no enclosed cell there";
        throw std::runtime_error(
            placed_error("a point" + none, "the point " + point_text(points[p]) + none, {point_place(points[p])}));
      }
    }

    std::vector<BoundaryCell> cells;
    cells.reserve(points.size());
    // One tool: its one cell is all of it.
    if (tools.size() == 1) {
      const ShapePtr whole = detail::working_copy(*tools.front());
      for (std::size_t p = 0; p < points.size(); ++p) {
        cells.push_back({whole, holders[p]});
      }
      return cells;
    }

    // The tools' faces divide space into parts once; a cell is the parts
    // inside the tools that hold its point and outside the others.
    BOPAlgo_CellsBuilder builder;
    for (std::size_t i = 0; i < tools.size(); ++i) {
      builder.AddArgument(copy.shape(i));
    }
    builder.SetNonDestructive(true);
    builder.SetRunParallel(true);
    detail::interruptible([&](const Message_ProgressRange& range) { builder.Perform(range); });
    if (builder.HasErrors()) {
      throw std::runtime_error("the tool bodies could not be divided into cells");
    }
    std::map<std::vector<std::size_t>, ShapePtr> made;
    for (std::size_t p = 0; p < points.size(); ++p) {
      const auto known = made.find(holders[p]);
      if (known != made.end()) {
        cells.push_back({known->second, holders[p]});
        continue;
      }
      NCollection_List<TopoDS_Shape> take;
      NCollection_List<TopoDS_Shape> avoid;
      std::size_t next = 0;
      for (std::size_t i = 0; i < tools.size(); ++i) {
        const bool holds = next < holders[p].size() && holders[p][next] == i;
        if (holds) {
          ++next;
          take.Append(copy.shape(i));
        } else {
          avoid.Append(copy.shape(i));
        }
      }
      builder.RemoveAllFromResult();
      builder.AddToResult(take, avoid);
      const TopoDS_Shape found = builder.Shape();
      if (found.IsNull() || !TopExp_Explorer(found, TopAbs_SOLID).More()) {
        throw std::runtime_error(placed_error("the cell at a point came out empty",
                                              "the cell at " + point_text(points[p]) + " came out empty",
                                              {point_place(points[p])}));
      }
      // Each face of the cell takes the names of the tool faces it is a
      // piece of, by the history of the parts the cell kept.
      const occ::handle<BRepTools_History> history = builder.History();
      if (history.IsNull()) {
        throw std::runtime_error("the division into cells has no history");
      }
      detail::FaceNamer pieces(found);
      for (const Shape* tool : tools) {
        for (int f = 0; f < tool->face_count(); ++f) {
          const TopoDS_Shape face = copy.of(tool->face(f));
          if (history->IsRemoved(face)) {
            continue;
          }
          const NCollection_List<TopoDS_Shape>& images = history->Modified(face);
          if (images.IsEmpty()) {
            pieces.add(face, tool->face_names(f), -1);
          }
          for (const TopoDS_Shape& image : images) {
            pieces.add(image, tool->face_names(f), -1);
          }
        }
      }
      ShapePtr cell = merged(pieces);
      if (!cell) {
        pieces.finish();
        cell = pieces.shape();
      }
      made.emplace(holders[p], cell);
      cells.push_back({cell, holders[p]});
    }
    return cells;
  });
}

} // namespace mitcad::geometry
