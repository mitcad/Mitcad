// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surface_edits.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <map>
#include <memory>
#include <numeric>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeFix_Face.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTolerance = 1.0e-6;
// Faces sewn after an untrim in place.
constexpr double kSewing = 1.0e-4;

// The faces of the shape grouped into the pieces that hang together
// through shared edges (not through an edge on the cutting tool, which
// parts the faces at it), in the faces' order of TopExp.
std::vector<std::vector<TopoDS_Shape>> connected_faces(const TopoDS_Shape& shape, const TopoDS_Shape& cutter) {
  ShapeMap faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  std::vector<std::size_t> parent(static_cast<std::size_t>(faces.Extent()));
  std::iota(parent.begin(), parent.end(), std::size_t{0});
  const auto root = [&parent](std::size_t i) {
    while (parent[i] != i) {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    return i;
  };
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edges);
  for (int e = 1; e <= edges.Extent(); ++e) {
    // The cut: an edge on the cutting tool parts the faces at it.
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges.FindKey(e)));
    const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
    BRepExtrema_DistShapeShape gap(BRepBuilderAPI_MakeVertex(middle).Shape(), cutter);
    if (gap.IsDone() && gap.Value() <= 1.0e-5) {
      continue;
    }
    std::size_t first = std::numeric_limits<std::size_t>::max();
    for (const TopoDS_Shape& face : edges.FindFromIndex(e)) {
      const std::size_t index = static_cast<std::size_t>(faces.FindIndex(face) - 1);
      if (first == std::numeric_limits<std::size_t>::max()) {
        first = index;
      } else {
        parent[root(index)] = root(first);
      }
    }
  }
  std::map<std::size_t, std::vector<TopoDS_Shape>> groups;
  for (int f = 1; f <= faces.Extent(); ++f) {
    groups[root(static_cast<std::size_t>(f - 1))].push_back(faces.FindKey(f));
  }
  std::vector<std::vector<TopoDS_Shape>> pieces;
  for (auto& entry : groups) {
    pieces.push_back(std::move(entry.second));
  }
  return pieces;
}

// A shell of the faces as they are.
TopoDS_Shell shell_of(const std::vector<TopoDS_Shape>& faces) {
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (const TopoDS_Shape& face : faces) {
    builder.Add(shell, face);
  }
  shell.Closed(BRep_Tool::IsClosed(shell));
  return shell;
}

// The curves' wires swept both ways along the direction past the body.
TopoDS_Shape curve_cutter(const TrimInput& input, const Shape& body) {
  const Bnd_Box box = detail::box_of(body.occt());
  const double reach = std::sqrt(box.SquareExtent()) + 1.0;
  std::vector<TopoDS_Shape> walls;
  for (const Path& chain : input.chains) {
    gp_Trsf back;
    back.SetTranslation(gp_Vec(input.direction) * -reach);
    const TopoDS_Shape moved = BRepBuilderAPI_Transform(detail::path_wire(chain), back, true).Shape();
    BRepPrimAPI_MakePrism prism(moved, gp_Vec(input.direction) * (2.0 * reach));
    detail::build(prism);
    if (!prism.IsDone()) {
      throw std::runtime_error("trim: the curves could not be swept into a cutting tool");
    }
    walls.push_back(prism.Shape());
  }
  if (walls.empty()) {
    throw std::invalid_argument("trim: no curves to trim with");
  }
  return detail::compound_or_single(walls);
}

} // namespace

ShapePtr trim_surface(const Shape& body, const TrimInput& input, const std::vector<gp_Pnt>& keep) {
  detail::require_surfaces(body.occt(), "trim");
  if (TopExp_Explorer(body.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument("trim: the body is a solid; trim cuts surface bodies");
  }
  if (keep.empty()) {
    throw std::invalid_argument("trim: no point picks the pieces to keep");
  }
  return detail::run("trim", [&] {
    const TopoDS_Shape cutter = input.tool ? detail::make_cutter(*input.tool, detail::box_of(body.occt()), input.extend).shape
                                           : curve_cutter(input, body);
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(body.occt());
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(cutter);
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.SetNonDestructive(true);
    detail::build(splitter);
    if (!splitter.IsDone() || splitter.HasErrors()) {
      throw std::runtime_error("trim: the surface could not be cut");
    }
    detail::FaceNamer namer(splitter.Shape());
    namer.carry(splitter, body);
    namer.finish();
    const ShapePtr all = namer.shape();
    const std::vector<std::vector<TopoDS_Shape>> pieces = connected_faces(all->occt(), cutter);
    if (pieces.size() < 2) {
      throw std::invalid_argument("trim: the tool does not cut the surface apart");
    }
    // The piece nearest each point.
    std::set<std::size_t> kept;
    for (const gp_Pnt& point : keep) {
      const TopoDS_Shape vertex = BRepBuilderAPI_MakeVertex(point).Shape();
      double best = std::numeric_limits<double>::infinity();
      std::size_t nearest = 0;
      for (std::size_t i = 0; i < pieces.size(); ++i) {
        BRepExtrema_DistShapeShape distance(vertex, detail::compound_or_single(pieces[i]));
        if (distance.IsDone() && distance.Value() < best) {
          best = distance.Value();
          nearest = i;
        }
      }
      kept.insert(nearest);
    }
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    for (const std::size_t i : kept) {
      parts.push_back(shell_of(pieces[i]));
      for (const TopoDS_Shape& face : pieces[i]) {
        named.push_back({face, all->names_of_face(face)});
      }
    }
    auto trimmed = std::make_shared<Shape>(detail::compound_or_single(parts), named);
    trimmed->set_open_edge_names(all->named_open_edges());
    detail::require_valid(trimmed->occt(), "trim");
    return trimmed;
  });
}

ShapePtr untrim_surface(const std::string& feature, const Shape& body, const std::string& face, bool holes,
                        bool outer, bool in_place) {
  if (!holes && !outer) {
    throw std::invalid_argument("untrim: nothing to restore (neither holes nor the outer boundary)");
  }
  detail::require_surfaces(body.occt(), "untrim");
  return detail::run("untrim", [&] {
    const ShapePtr copy = detail::working_copy(body);
    const std::vector<int> found = detail::resolve_faces(*copy, {face}, "face");
    if (found.size() != 1) {
      const std::string pieces = " is in " + std::to_string(found.size()) + " pieces; name one piece";
      throw std::invalid_argument(face_error(face, "untrim: the face" + pieces, "untrim: face " + face + pieces));
    }
    const TopoDS_Face& original = copy->face(found.front());
    TopLoc_Location location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(original, location);
    if (surface.IsNull()) {
      throw std::invalid_argument("untrim: the face has no surface");
    }
    const TopoDS_Face forward = TopoDS::Face(original.Oriented(TopAbs_FORWARD));
    const TopoDS_Wire outer_wire = BRepTools::OuterWire(forward);
    std::vector<TopoDS_Wire> inner;
    for (TopExp_Explorer w(forward, TopAbs_WIRE); w.More(); w.Next()) {
      if (!w.Current().IsSame(outer_wire)) {
        inner.push_back(TopoDS::Wire(w.Current()));
      }
    }
    TopoDS_Face result;
    if (outer) {
      double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
      surface->Bounds(u1, u2, v1, v2);
      double fu1 = 0, fu2 = 0, fv1 = 0, fv2 = 0;
      BRepTools::UVBounds(forward, fu1, fu2, fv1, fv2);
      // A surface without bounds (a plane, a cylinder's length) stops at
      // the face's span; a periodic one goes once round.
      if (!std::isfinite(u1) || !std::isfinite(u2) || std::abs(u2 - u1) > 1.0e9) {
        u1 = fu1;
        u2 = fu2;
      }
      if (!std::isfinite(v1) || !std::isfinite(v2) || std::abs(v2 - v1) > 1.0e9) {
        v1 = fv1;
        v2 = fv2;
      }
      BRepBuilderAPI_MakeFace make(surface, u1, u2, v1, v2, kTolerance);
      if (!make.IsDone()) {
        throw std::runtime_error("untrim: the surface's bounds make no face");
      }
      if (!holes) {
        // The holes as the face had them (on the face's surface, the
        // face's location taken off), their orientation healed.
        for (const TopoDS_Wire& wire : inner) {
          make.Add(TopoDS::Wire(wire.Moved(location.Inverted())));
        }
      }
      ShapeFix_Face fix(make.Face());
      fix.Perform();
      result = fix.Face();
      result.Location(location);
    } else {
      // The face of its outer wire alone.
      BRepBuilderAPI_MakeFace make(surface, TopoDS::Wire(outer_wire.Moved(location.Inverted())), true);
      if (!make.IsDone()) {
        throw std::runtime_error("untrim: the outer boundary makes no face");
      }
      result = make.Face();
      result.Location(location);
    }
    if (original.Orientation() == TopAbs_REVERSED) {
      result.Reverse();
    }
    if (!in_place) {
      const ShapePtr untrimmed = std::make_shared<Shape>(
          shell_of({result}),
          std::vector<Shape::NamedFace>{{result, {face_name(feature, "untrim", detail::primary_name(*copy, found.front()))}}});
      detail::require_valid(untrimmed->occt(), "untrim");
      return untrimmed;
    }
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    for (int f = 0; f < body.face_count(); ++f) {
      const TopoDS_Shape part = f == found.front() ? TopoDS_Shape(result) : TopoDS_Shape(body.face(f));
      parts.push_back(part);
      named.push_back({part, body.face_names(f)});
    }
    const Shape faces(detail::compound_or_single(parts), named);
    return stitch({&faces}, kSewing);
  });
}

std::pair<ShapePtr, std::vector<ShapePtr>> unstitch(const Shape& body, const std::vector<std::string>& faces) {
  detail::require_surfaces(body.occt(), "unstitch");
  return detail::run("unstitch", [&] {
    std::vector<int> picked;
    if (faces.empty()) {
      for (int f = 0; f < body.face_count(); ++f) {
        picked.push_back(f);
      }
    } else {
      picked = detail::resolve_faces(body, faces);
    }
    std::vector<ShapePtr> apart;
    for (const int f : picked) {
      const detail::InputCopy copy(body.face(f));
      const TopoDS_Shape face = copy.shape();
      apart.push_back(std::make_shared<Shape>(shell_of({face}),
                                              std::vector<Shape::NamedFace>{{face, body.face_names(f)}}));
    }
    // What is left, shell by shell of the body, copied together.
    std::vector<TopoDS_Shape> shells;
    std::vector<int> left;
    ShapeMap placed;
    for (TopExp_Explorer s(body.occt(), TopAbs_SHELL); s.More(); s.Next()) {
      std::vector<TopoDS_Shape> kept;
      for (TopExp_Explorer f(s.Current(), TopAbs_FACE); f.More(); f.Next()) {
        placed.Add(f.Current());
        const int index = body.face_index(f.Current());
        if (std::find(picked.begin(), picked.end(), index) == picked.end()) {
          kept.push_back(f.Current());
          left.push_back(index);
        }
      }
      if (!kept.empty()) {
        shells.push_back(shell_of(kept));
      }
    }
    for (TopExp_Explorer f(body.occt(), TopAbs_FACE); f.More(); f.Next()) {
      const int index = body.face_index(f.Current());
      if (!placed.Contains(f.Current()) && std::find(picked.begin(), picked.end(), index) == picked.end()) {
        shells.push_back(f.Current());
        left.push_back(index);
      }
    }
    ShapePtr rest;
    if (!shells.empty()) {
      const detail::InputCopy copy(detail::compound_or_single(shells));
      std::vector<Shape::NamedFace> named;
      for (const int f : left) {
        named.push_back({copy.of(body.face(f)), body.face_names(f)});
      }
      rest = std::make_shared<Shape>(copy.shape(), named);
    }
    return std::make_pair(rest, apart);
  });
}

ShapePtr reverse_faces(const Shape& body, const std::vector<std::string>& faces) {
  detail::require_surfaces(body.occt(), "reverse normal");
  if (TopExp_Explorer(body.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument("reverse normal: the body is a solid; its faces point out of its material");
  }
  return detail::run("reverse normal", [&] {
    std::vector<int> picked;
    if (faces.empty()) {
      for (int f = 0; f < body.face_count(); ++f) {
        picked.push_back(f);
      }
    } else {
      picked = detail::resolve_faces(body, faces);
    }
    const detail::InputCopy copy(body.occt());
    const auto turned = [&](const TopoDS_Shape& face) {
      const int index = body.face_index(face);
      const TopoDS_Shape mine = copy.of(face);
      return std::find(picked.begin(), picked.end(), index) != picked.end() ? mine.Reversed() : mine;
    };
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    ShapeMap placed;
    for (TopExp_Explorer s(body.occt(), TopAbs_SHELL); s.More(); s.Next()) {
      std::vector<TopoDS_Shape> shell;
      for (TopExp_Explorer f(s.Current(), TopAbs_FACE); f.More(); f.Next()) {
        placed.Add(f.Current());
        const TopoDS_Shape face = turned(f.Current());
        shell.push_back(face);
        named.push_back({face, body.names_of_face(f.Current())});
      }
      parts.push_back(shell_of(shell));
    }
    for (TopExp_Explorer f(body.occt(), TopAbs_FACE); f.More(); f.Next()) {
      if (!placed.Contains(f.Current())) {
        const TopoDS_Shape face = turned(f.Current());
        parts.push_back(face);
        named.push_back({face, body.names_of_face(f.Current())});
      }
    }
    return std::make_shared<Shape>(detail::compound_or_single(parts), named);
  });
}

} // namespace mitcad::geometry
