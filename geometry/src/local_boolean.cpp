// SPDX-License-Identifier: MIT
#include "local_boolean.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <functional>
#include <map>
#include <optional>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BOPAlgo_Builder.hxx>
#include <BOPTools_AlgoTools.hxx>
#include <BOPTools_AlgoTools3D.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <IntTools_Context.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using EdgeFaces =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// Below this many faces the ordinary boolean is as quick.
constexpr int kLocalFaces = 200;

TopoDS_Compound compound_of(const std::vector<TopoDS_Shape>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(compound, shape);
  }
  return compound;
}

std::vector<TopoDS_Shape> sub_shapes(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  std::vector<TopoDS_Shape> found;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    found.push_back(it.Current());
  }
  return found;
}

LocalResult ordinary(const Shape& body, const TopoDS_Shape& tool, LocalOperation operation,
                     const std::string& tool_name) {
  std::vector<Shape::NamedFace> names;
  for (TopExp_Explorer f(tool, TopAbs_FACE); f.More(); f.Next()) {
    names.push_back({f.Current(), {tool_name}});
  }
  const Shape tool_shape(tool, names);
  BRepAlgoAPI_Cut cut;
  BRepAlgoAPI_Fuse fuse;
  BRepAlgoAPI_BooleanOperation& boolean =
      operation == LocalOperation::Cut ? static_cast<BRepAlgoAPI_BooleanOperation&>(cut) : fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool);
  boolean.SetArguments(arguments);
  boolean.SetTools(tools);
  boolean.SetNonDestructive(true);
  build(boolean);
  if (!boolean.IsDone() || boolean.HasErrors()) {
    throw std::runtime_error("the boolean operation failed");
  }
  FaceNamer namer(boolean.Shape());
  namer.carry(boolean, body);
  namer.carry(boolean, tool_shape);
  return {namer.shape(), false};
}

// A piece of a face after the general fuse, oriented as the face it comes
// from is in its solid.
struct Piece {
  TopoDS_Face face;
  // The face of the body it comes from (an index into the body's faces),
  // or -1 for a face of the tool.
  int origin = -1;
  // The tool's piece that is the same face (a face of both), or -1.
  int same = -1;
  // Which group of pieces it belongs to (connected without crossing a
  // section edge).
  int group = -1;
};

// Pieces connected through edges that are not section edges have the same
// state: groups of them, by union-find.
struct Groups {
  std::vector<int> parent;
  int find(int i) {
    while (parent[static_cast<std::size_t>(i)] != i) {
      parent[static_cast<std::size_t>(i)] = parent[static_cast<std::size_t>(parent[static_cast<std::size_t>(i)])];
      i = parent[static_cast<std::size_t>(i)];
    }
    return i;
  }
  void join(int a, int b) {
    a = find(a);
    b = find(b);
    if (a != b) {
      parent[static_cast<std::size_t>(a)] = b;
    }
  }
};

enum class State { Unknown, In, Out };

std::optional<LocalResult> local(const Shape& body, const TopoDS_Shape& tool, LocalOperation operation,
                                 const std::string& tool_name) {
  const std::vector<TopoDS_Shape> solids = sub_shapes(body.occt(), TopAbs_SOLID);
  if (solids.size() != 1 || sub_shapes(solids.front(), TopAbs_SHELL).size() != 1) {
    return std::nullopt;
  }
  const TopoDS_Solid solid = TopoDS::Solid(solids.front());
  // One tool solid: several could touch or overlap, which the
  // classification below does not take.
  const std::vector<TopoDS_Shape> tool_solids = sub_shapes(tool, TopAbs_SOLID);
  if (tool_solids.size() != 1) {
    return std::nullopt;
  }
  ShapeMap body_faces;
  TopExp::MapShapes(body.occt(), TopAbs_FACE, body_faces);
  if (body_faces.Extent() < kLocalFaces) {
    return std::nullopt;
  }

  // The near faces: those whose boxes reach the tool's.
  Bnd_Box tool_box;
  BRepBndLib::Add(tool, tool_box, false);
  tool_box.Enlarge(1.0e-3 * std::max(1.0, std::sqrt(tool_box.SquareExtent())));
  std::vector<TopoDS_Shape> near;
  std::vector<TopoDS_Shape> far;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box, false);
    (box.IsOut(tool_box) ? far : near).push_back(f.Current());
  }
  if (near.empty() || 2 * near.size() > static_cast<std::size_t>(body_faces.Extent())) {
    return std::nullopt;
  }

  // Split the near faces and the tool's faces by each other.
  BOPAlgo_Builder fuse;
  fuse.AddArgument(compound_of(near));
  fuse.AddArgument(tool);
  fuse.SetNonDestructive(true);
  interruptible([&](const Message_ProgressRange& range) { fuse.Perform(range); });
  if (fuse.HasErrors()) {
    return std::nullopt;
  }
  const occ::handle<IntTools_Context> context = fuse.Context();

  // The pieces, oriented as their faces.
  std::vector<Piece> pieces;
  ShapeMap body_pieces;
  const auto images = [&](const TopoDS_Shape& face) {
    std::vector<TopoDS_Face> found;
    const NCollection_List<TopoDS_Shape>& modified = fuse.Modified(face);
    if (modified.IsEmpty()) {
      found.push_back(TopoDS::Face(face));
      return found;
    }
    for (const TopoDS_Shape& image : modified) {
      TopoDS_Face piece = TopoDS::Face(image);
      piece.Orientation(face.Orientation());
      if (BOPTools_AlgoTools::IsSplitToReverse(piece, TopoDS::Face(face), context)) {
        piece.Reverse();
      }
      found.push_back(piece);
    }
    return found;
  };
  for (const TopoDS_Shape& face : near) {
    for (const TopoDS_Face& piece : images(face)) {
      body_pieces.Add(piece);
      pieces.push_back({piece, body_faces.FindIndex(face) - 1, -1, -1});
    }
  }
  const std::size_t first_tool = pieces.size();
  for (TopExp_Explorer f(tool, TopAbs_FACE); f.More(); f.Next()) {
    for (const TopoDS_Face& piece : images(f.Current())) {
      const int index = body_pieces.FindIndex(piece);
      if (index > 0) {
        // A face of both: the body's piece stands for it.
        pieces[static_cast<std::size_t>(index - 1)].same = static_cast<int>(pieces.size());
      }
      pieces.push_back({piece, -1, index > 0 ? index - 1 : -1, -1});
    }
  }

  // Section edges: those of both a body piece and a tool piece.
  EdgeFaces body_edges;
  EdgeFaces tool_edges;
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    EdgeFaces& edges = i < first_tool ? body_edges : tool_edges;
    for (TopExp_Explorer e(pieces[i].face, TopAbs_EDGE); e.More(); e.Next()) {
      NCollection_List<TopoDS_Shape> none;
      const int index = edges.Contains(e.Current()) ? edges.FindIndex(e.Current()) : edges.Add(e.Current(), none);
      edges.ChangeFromIndex(index).Append(pieces[i].face);
    }
  }
  const auto section = [&](const TopoDS_Shape& edge) {
    return body_edges.Contains(edge) && tool_edges.Contains(edge);
  };

  // Group the pieces of each side: across edges that are not section
  // edges, within the body's pieces and within the tool's.
  Groups groups;
  groups.parent.resize(pieces.size());
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    groups.parent[i] = static_cast<int>(i);
  }
  // The piece of a face: the body's where it is a face of both sides.
  ShapeMap piece_faces;
  std::vector<int> piece_of;
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    if (piece_faces.Add(pieces[i].face) > static_cast<int>(piece_of.size())) {
      piece_of.push_back(static_cast<int>(i));
    }
  }
  const auto piece_at = [&](const TopoDS_Shape& face) {
    return piece_of[static_cast<std::size_t>(piece_faces.FindIndex(face) - 1)];
  };
  const auto index_of = [&](const TopoDS_Shape& face, bool tool_side) {
    // Pieces of both sides are the same face: pick the side's own.
    const int index = piece_at(face);
    if (tool_side && pieces[static_cast<std::size_t>(index)].same >= 0) {
      return pieces[static_cast<std::size_t>(index)].same;
    }
    return index;
  };
  for (const EdgeFaces* edges : {&body_edges, &tool_edges}) {
    const bool tool_side = edges == &tool_edges;
    for (int k = 1; k <= edges->Extent(); ++k) {
      if (section(edges->FindKey(k)) || BRep_Tool::Degenerated(TopoDS::Edge(edges->FindKey(k)))) {
        continue;
      }
      const NCollection_List<TopoDS_Shape>& faces = edges->FindFromIndex(k);
      const int a = index_of(faces.First(), tool_side);
      for (const TopoDS_Shape& face : faces) {
        groups.join(a, index_of(face, tool_side));
      }
    }
  }
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    pieces[i].group = groups.find(static_cast<int>(i));
  }

  // Body pieces joined to a far face through an edge that is not split
  // are outside the tool (the far faces lie away from it).
  std::map<int, State> states;
  {
    ShapeMap far_edges;
    for (const TopoDS_Shape& face : far) {
      TopExp::MapShapes(face, TopAbs_EDGE, far_edges);
    }
    for (std::size_t i = 0; i < first_tool; ++i) {
      if (pieces[i].same >= 0) {
        continue;
      }
      for (TopExp_Explorer e(pieces[i].face, TopAbs_EDGE); e.More(); e.Next()) {
        if (far_edges.Contains(e.Current()) && !section(e.Current())) {
          states[pieces[i].group] = State::Out;
          break;
        }
      }
    }
  }

  // Each other group by the faces of the other side at one of its section
  // edges (the method of angles), else by a point inside it.
  std::vector<TopoDS_Solid> tools;
  for (const TopoDS_Shape& s : tool_solids) {
    tools.push_back(TopoDS::Solid(s));
  }
  const auto state_of = [&](std::size_t i) -> State {
    const Piece& piece = pieces[i];
    const bool tool_side = i >= first_tool;
    EdgeFaces& other = tool_side ? body_edges : tool_edges;
    for (TopExp_Explorer e(piece.face, TopAbs_EDGE); e.More(); e.Next()) {
      if (!section(e.Current()) || BRep_Tool::Degenerated(TopoDS::Edge(e.Current()))) {
        continue;
      }
      NCollection_List<TopoDS_Shape> around = other.FindFromKey(e.Current());
      // Where a face of both sides meets the edge the faces around it can
      // lie in one surface, and the angles tell nothing.
      NCollection_List<TopoDS_Shape> faces;
      bool shared = false;
      for (const TopoDS_Shape& face : around) {
        shared = shared || pieces[static_cast<std::size_t>(piece_at(face))].same >= 0;
        faces.Append(face);
      }
      if (shared || piece.same >= 0 || faces.Extent() < 2) {
        continue;
      }
      const int in = BOPTools_AlgoTools::IsInternalFace(piece.face, TopoDS::Edge(e.Current()), faces, context);
      if (in != 2) {
        return in == 1 ? State::In : State::Out;
      }
    }
    // By a point inside the piece.
    gp_Pnt point;
    gp_Pnt2d uv;
    if (BOPTools_AlgoTools3D::PointInFace(piece.face, point, uv, context) != 0) {
      return State::Unknown;
    }
    const auto state_in = [&](const TopoDS_Solid& s) {
      const TopAbs_State state = BOPTools_AlgoTools::ComputeState(point, s, Precision::Confusion(), context);
      return state == TopAbs_IN ? State::In : state == TopAbs_OUT ? State::Out : State::Unknown;
    };
    if (tool_side) {
      return state_in(solid);
    }
    State result = State::Out;
    for (const TopoDS_Solid& t : tools) {
      const State state = state_in(t);
      if (state == State::In) {
        return State::In;
      }
      if (state == State::Unknown) {
        result = State::Unknown;
      }
    }
    return result;
  };
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    if (pieces[i].same >= 0 || states.count(pieces[i].group) > 0) {
      continue;
    }
    const State state = state_of(i);
    if (state == State::Unknown) {
      return std::nullopt;
    }
    states[pieces[i].group] = state;
  }

  // The pieces the result keeps.
  const bool cut = operation == LocalOperation::Cut;
  std::vector<TopoDS_Shape> kept;
  std::vector<std::pair<TopoDS_Shape, int>> kept_origins;
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    const Piece& piece = pieces[i];
    const bool tool_side = i >= first_tool;
    if (piece.same >= 0) {
      if (tool_side) {
        continue;
      }
      // A face of both: kept once where the solids lie on opposite sides
      // of it (a cut) or on the same side (a fuse).
      const bool same_side =
          piece.face.Orientation() == pieces[static_cast<std::size_t>(piece.same)].face.Orientation();
      if (same_side != cut) {
        kept.push_back(piece.face);
        kept_origins.emplace_back(piece.face, piece.origin);
      }
      continue;
    }
    const State state = states.at(piece.group);
    if (!tool_side) {
      if (state == State::Out) {
        kept.push_back(piece.face);
        kept_origins.emplace_back(piece.face, piece.origin);
      }
    } else if ((state == State::In) == cut) {
      kept.push_back(cut ? piece.face.Reversed() : TopoDS_Shape(piece.face));
      kept_origins.emplace_back(kept.back(), -1);
    }
  }

  // The shell: the far faces as they are and the kept pieces.
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (const TopoDS_Shape& face : far) {
    builder.Add(shell, face);
  }
  for (const TopoDS_Shape& face : kept) {
    builder.Add(shell, face);
  }
  // Closed, consistently oriented and in one piece: every edge has one
  // face on each side, and every face is reached from the first one.
  std::vector<TopoDS_Shape> faces = sub_shapes(shell, TopAbs_FACE);
  {
    EdgeFaces edges;
    for (std::size_t i = 0; i < faces.size(); ++i) {
      for (TopExp_Explorer e(faces[i], TopAbs_EDGE); e.More(); e.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        if (edge.Orientation() != TopAbs_FORWARD && edge.Orientation() != TopAbs_REVERSED) {
          return std::nullopt;
        }
        NCollection_List<TopoDS_Shape> none;
        const int index = edges.Contains(edge) ? edges.FindIndex(edge) : edges.Add(edge, none);
        TopoDS_Shape tagged = faces[i];
        // The face's number in the orientation of its use of the edge.
        tagged.Orientation(edge.Orientation());
        edges.ChangeFromIndex(index).Append(tagged);
      }
    }
    ShapeMap face_index;
    for (const TopoDS_Shape& face : faces) {
      face_index.Add(face);
    }
    Groups connected;
    connected.parent.resize(faces.size());
    for (std::size_t i = 0; i < faces.size(); ++i) {
      connected.parent[i] = static_cast<int>(i);
    }
    for (int k = 1; k <= edges.Extent(); ++k) {
      const NCollection_List<TopoDS_Shape>& around = edges.FindFromIndex(k);
      int forward = 0;
      int reversed = 0;
      for (const TopoDS_Shape& use : around) {
        (use.Orientation() == TopAbs_FORWARD ? forward : reversed) += 1;
      }
      if (forward != 1 || reversed != 1) {
        return std::nullopt;
      }
      connected.join(face_index.FindIndex(around.First()) - 1, face_index.FindIndex(around.Last()) - 1);
    }
    const int root = connected.find(0);
    for (std::size_t i = 0; i < faces.size(); ++i) {
      if (connected.find(static_cast<int>(i)) != root) {
        return std::nullopt;
      }
    }
  }
  shell.Closed(true);
  TopoDS_Solid result_solid;
  builder.MakeSolid(result_solid);
  builder.Add(result_solid, shell);
  const TopoDS_Compound result = compound_of({result_solid});

  // Names: the far faces and the body's pieces carry their faces' names,
  // the tool's pieces get the tool's.
  FaceNamer namer(result);
  for (const TopoDS_Shape& face : far) {
    const int index = body_faces.FindIndex(face) - 1;
    for (const std::string& name : body.face_names(index)) {
      namer.add(face, name);
    }
  }
  for (const auto& [face, origin] : kept_origins) {
    if (origin < 0) {
      namer.add(face, tool_name);
      continue;
    }
    for (const std::string& name : body.face_names(origin)) {
      namer.add(face, name);
    }
  }
  return LocalResult{namer.shape(), true};
}

} // namespace

LocalResult local_boolean(const Shape& body, const TopoDS_Shape& tool, LocalOperation operation,
                          const std::string& tool_name) {
  if (std::optional<LocalResult> done = local(body, tool, operation, tool_name)) {
    return *done;
  }
  return ordinary(body, tool, operation, tool_name);
}

void require_valid_near(const TopoDS_Shape& result, const TopoDS_Shape& changed, const char* operation) {
  // The changed faces themselves.
  require_valid(changed, operation);
  // Every edge of them has one face on each side in the result.
  ShapeMap edges;
  TopExp::MapShapes(changed, TopAbs_EDGE, edges);
  std::vector<int> forward(static_cast<std::size_t>(edges.Extent()), 0);
  std::vector<int> reversed(static_cast<std::size_t>(edges.Extent()), 0);
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const int index = edges.FindIndex(e.Current());
      if (index <= 0 || BRep_Tool::Degenerated(TopoDS::Edge(e.Current()))) {
        continue;
      }
      (e.Current().Orientation() == TopAbs_FORWARD ? forward : reversed)[static_cast<std::size_t>(index - 1)] += 1;
    }
  }
  for (std::size_t i = 0; i < forward.size(); ++i) {
    if (BRep_Tool::Degenerated(TopoDS::Edge(edges(static_cast<int>(i) + 1)))) {
      continue;
    }
    if (forward[i] != 1 || reversed[i] != 1) {
      throw std::runtime_error(std::string(operation) + " produced an invalid shape (an open or misoriented shell)");
    }
  }
}

} // namespace mitcad::geometry::detail
