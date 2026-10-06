// SPDX-License-Identifier: MIT
#include "corner.hpp"

#include <algorithm>
#include <cmath>
#include <map>
#include <set>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Pln.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// How closely the patch must meet the faces around it: position (mm) and
// the angle between the normals (radians).
constexpr double kPatchGap = 1.0e-4;
constexpr double kPatchAngle = 1.0e-2;
constexpr double kHalfTurn = 3.14159265358979323846;

// The images of a face of the input in the split shape: its pieces, or
// itself when the split left it.
std::vector<TopoDS_Shape> images(BRepFeat_SplitShape& split, const TopoDS_Shape& face) {
  std::vector<TopoDS_Shape> found;
  if (split.IsDeleted(face)) {
    return found;
  }
  const NCollection_List<TopoDS_Shape>& modified = split.Modified(face);
  if (modified.IsEmpty()) {
    found.push_back(face);
  } else {
    found.assign(modified.begin(), modified.end());
  }
  return found;
}

// The face of a dressup the name of an edge it rounds (<prefix><edge>)).
std::string dressed_edge(const NameList& names, const std::string& prefix) {
  for (const std::string& name : names) {
    if (name.rfind(prefix, 0) == 0 && name.size() > prefix.size() && name.back() == ')') {
      return name.substr(prefix.size(), name.size() - prefix.size() - 1);
    }
  }
  return std::string();
}

} // namespace

ShapePtr setback_corners(const std::string& feature, const char* role, const Shape& body, const Shape& dressed,
                         const std::vector<Setback>& corners) {
  const std::string prefix = feature + ":" + role + "(";
  // Per corner: its corner faces and the dressup faces around them (of
  // `dressed`), and where each of those is cut.
  struct Corner {
    std::vector<int> faces;
    std::vector<int> around;
  };
  std::vector<Corner> found(corners.size());
  std::vector<std::pair<int, TopoDS_Edge>> cuts;
  for (std::size_t c = 0; c < corners.size(); ++c) {
    const Setback& corner = corners[c];
    found[c].faces = dressed.find_faces(face_name(feature, "corner", corner.vertex));
    const std::vector<int> apexes = body.find_vertices(corner.vertex);
    if (found[c].faces.empty() || apexes.size() != 1) {
      throw std::runtime_error("no corner face at vertex " + corner.vertex);
    }
    const gp_Pnt apex = BRep_Tool::Pnt(body.vertex(apexes.front()));
    std::set<int> around;
    for (int f : found[c].faces) {
      for (TopExp_Explorer it(dressed.face(f), TopAbs_EDGE); it.More(); it.Next()) {
        for (int g : dressed.edge_faces(dressed.edge_index(it.Current()))) {
          if (g >= 0 && std::find(found[c].faces.begin(), found[c].faces.end(), g) == found[c].faces.end() &&
              !dressed_edge(dressed.face_names(g), prefix).empty()) {
            around.insert(g);
          }
        }
      }
    }
    for (int g : around) {
      const std::string edge_name = dressed_edge(dressed.face_names(g), prefix);
      const std::vector<int> edges = body.find_edges(edge_name);
      if (edges.size() != 1) {
        throw std::runtime_error("no edge " + edge_name + " for a setback corner");
      }
      // The point `distance` along the edge from the vertex, and the plane
      // across the edge there.
      const BRepAdaptor_Curve curve(body.edge(edges.front()));
      const double first = curve.FirstParameter();
      const double last = curve.LastParameter();
      const bool from_first = curve.Value(first).Distance(apex) <= curve.Value(last).Distance(apex);
      const GCPnts_AbscissaPoint at(curve, from_first ? corner.distance : -corner.distance,
                                    from_first ? first : last);
      if (!at.IsDone() || at.Parameter() <= first || at.Parameter() >= last) {
        throw std::invalid_argument("unsupported: a setback corner at vertex " + corner.vertex +
                                    ", where an edge is shorter than the setback");
      }
      gp_Pnt p;
      gp_Vec tangent;
      curve.D1(at.Parameter(), p, tangent);
      BRepAlgoAPI_Section section(dressed.face(g), gp_Pln(p, gp_Dir(tangent)), false);
      section.ComputePCurveOn1(true);
      section.Approximation(true);
      // The rounding's edges are the dressed body's (input_check.hpp).
      section.SetNonDestructive(true);
      detail::build(section);
      int count = 0;
      for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next(), ++count) {
        cuts.emplace_back(g, TopoDS::Edge(it.Current()));
      }
      if (!section.IsDone() || count == 0) {
        throw std::invalid_argument("unsupported: a setback corner at vertex " + corner.vertex +
                                    ", where a rounding is shorter than the setback");
      }
      found[c].around.push_back(g);
    }
  }

  // The roundings cut across.
  BRepFeat_SplitShape split(dressed.occt());
  for (const auto& [face, edge] : cuts) {
    split.Add(edge, dressed.face(face));
  }
  detail::build(split);
  if (!split.IsDone()) {
    throw std::runtime_error("the roundings could not be cut for a setback corner");
  }
  const TopoDS_Shape cut = split.Shape();
  Ancestors faces_of_edge;
  TopExp::MapShapesAndAncestors(cut, TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  const auto touches = [&](const TopoDS_Shape& face, const ShapeMap& set) {
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      for (const TopoDS_Shape& other : faces_of_edge.FindFromKey(it.Current())) {
        if (!other.IsSame(face) && set.Contains(other)) {
          return true;
        }
      }
    }
    return false;
  };

  // Each corner's hole: its corner faces and the pieces of the roundings
  // next to them; the patch through the hole's boundary, tangent to the
  // faces outside it.
  ShapeMap removed;
  std::vector<std::pair<TopoDS_Face, std::string>> patches;
  for (std::size_t c = 0; c < corners.size(); ++c) {
    throw_if_cancelled();
    ShapeMap hole;
    for (int f : found[c].faces) {
      for (const TopoDS_Shape& piece : images(split, dressed.face(f))) {
        hole.Add(piece);
      }
    }
    ShapeMap corner_faces = hole;
    for (int g : found[c].around) {
      const std::vector<TopoDS_Shape> pieces = images(split, dressed.face(g));
      int next_to_corner = 0;
      for (const TopoDS_Shape& piece : pieces) {
        if (touches(piece, corner_faces)) {
          hole.Add(piece);
          ++next_to_corner;
        }
      }
      if (pieces.size() < 2 || next_to_corner != 1) {
        throw std::invalid_argument("unsupported: a setback corner at vertex " + corners[c].vertex +
                                    ", where a rounding is shorter than the setback");
      }
    }
    // Tangent to the faces outside where what it replaces was (the
    // roundings and their corner), through the edges elsewhere (a bevel
    // meets its faces at an angle).
    BRepOffsetAPI_MakeFilling filling(3, 15, 2, false, 1.0e-5, kPatchGap, kPatchAngle, 0.1, 8, 9);
    int bounds = 0;
    for (int h = 1; h <= hole.Extent(); ++h) {
      for (TopExp_Explorer it(hole(h), TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
        for (const TopoDS_Shape& other : faces_of_edge.FindFromKey(edge)) {
          if (!hole.Contains(other)) {
            // (The pieces come with either orientation: a kink of a half
            // turn is none.)
            const double kink = normal_angle(edge, TopoDS::Face(hole(h)), TopoDS::Face(other));
            const bool smooth = std::min(kink, kHalfTurn - kink) < kPatchAngle;
            filling.Add(edge, TopoDS::Face(other), smooth ? GeomAbs_G1 : GeomAbs_C0, true);
            ++bounds;
          }
        }
      }
    }
    detail::build(filling);
    if (!filling.IsDone() || bounds < 3) {
      throw std::runtime_error("the patch of a setback corner at vertex " + corners[c].vertex +
                               " could not be built");
    }
    if (filling.G0Error() > 10 * kPatchGap || filling.G1Error() > 10 * kPatchAngle) {
      throw std::runtime_error("the patch of a setback corner at vertex " + corners[c].vertex +
                               " does not meet the faces around it");
    }
    patches.emplace_back(TopoDS::Face(filling.Shape()), face_name(feature, "corner", corners[c].vertex));
    for (int h = 1; h <= hole.Extent(); ++h) {
      removed.Add(hole(h));
    }
  }

  // The faces left and the patches sewn into the solid.
  BRepBuilderAPI_Sewing sewing(10 * kPatchGap);
  ShapeMap kept;
  for (TopExp_Explorer it(cut, TopAbs_FACE); it.More(); it.Next()) {
    if (!removed.Contains(it.Current()) && kept.Add(it.Current()) > 0) {
      sewing.Add(it.Current());
    }
  }
  for (const auto& patch : patches) {
    sewing.Add(patch.first);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
    shell = TopoDS::Shell(it.Current());
  }
  if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
    throw std::runtime_error("the setback corners do not close the body");
  }
  BRepBuilderAPI_MakeSolid make(shell);
  TopoDS_Solid solid = make.Solid();
  BRepLib::OrientClosedSolid(solid);

  // Names: the kept faces' through the split and the sewing, the patches'.
  FaceNamer namer(solid);
  const auto image = [&](const TopoDS_Shape& face) {
    return sewing.IsModified(face) ? sewing.Modified(face) : face;
  };
  for (int f = 0; f < dressed.face_count(); ++f) {
    for (const TopoDS_Shape& piece : images(split, dressed.face(f))) {
      if (!removed.Contains(piece)) {
        for (const std::string& name : dressed.face_names(f)) {
          namer.add(image(piece), name);
        }
      }
    }
  }
  for (const auto& [patch, name] : patches) {
    namer.add(image(patch), name);
  }
  namer.finish();
  require_valid(namer.result(), "setback corner");
  return namer.shape();
}

} // namespace mitcad::geometry::detail
