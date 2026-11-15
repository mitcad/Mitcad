// SPDX-License-Identifier: MIT
#include "mitcad/geometry/split.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <set>
#include <stdexcept>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
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
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// The role key a cut face is named with: the tool face's name, a sketch
// segment, or none for a plane.
std::string split_name(const std::string& feature, const std::string& key) {
  return face_name(feature, "split", key);
}

double along(const TopoDS_Shape& solid, const gp_Dir& normal) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(solid, props);
  return std::round(gp_Vec(props.CentreOfMass().XYZ()).Dot(gp_Vec(normal)) * 1.0e6);
}

using ShapeList = NCollection_List<TopoDS_Shape>;
using ShapeSet = NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>;

double distance_to(const gp_Pnt& point, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
  return distance.IsDone() && distance.NbSolution() > 0 ? distance.Value() : Precision::Infinite();
}

// A split's result with the pieces joined across their faces' seams, and
// the history from the split's result to it.
struct Joined {
  TopoDS_Shape shape;
  occ::handle<BRepTools_History> history;
};

// The pieces of the split faces that lie side by side across the seam of
// their face joined into one face each (mitcad#1853): a region of the tool
// that crosses the seam of a cylinder's or a cone's side (a square of a
// sketch on the face drawn over it) comes out of OCCT's split as two
// pieces, one on either side of the seam, which are one region on the
// face. Only the pieces of the seam that lie between two pieces of one
// split face are merged across (OCCT's ShapeUpgrade_UnifySameDomain with
// every other edge kept), so the tool's own cuts stay, and a cut of the
// tool that runs along the seam (`cuts`) keeps its pieces apart. None when
// no region crosses a seam, or when the merge fails or leaves an invalid
// shape: the split's pieces then stay as they are.
std::optional<Joined> joined_across_seams(const TopoDS_Shape& split, BRepFeat_SplitShape& splitter,
                                          const Shape& body, const std::vector<int>& chosen,
                                          const TopoDS_Shape& cuts) {
  try {
    OCC_CATCH_SIGNALS
    // On a copy: the merge writes p-curves into the edges it keeps, which
    // the split shares with the body (a cached result).
    BRepBuilderAPI_Copy copier(split);
    const TopoDS_Shape shape = copier.Shape();
    NCollection_IndexedDataMap<TopoDS_Shape, ShapeList, TopTools_ShapeMapHasher> faces_of;
    TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
    ShapeSet across;
    for (const int f : chosen) {
      const TopoDS_Face& face = body.face(f);
      // The face's seam: its edges that it holds twice.
      BRep_Builder builder;
      TopoDS_Compound seams;
      builder.MakeCompound(seams);
      bool seamed = false;
      double tolerance = 1.0e-6;
      for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
        if (BRep_Tool::IsClosed(edge, face)) {
          builder.Add(seams, edge);
          seamed = true;
          tolerance = std::max(tolerance, 10.0 * BRep_Tool::Tolerance(edge));
        }
      }
      if (!seamed) {
        continue;
      }
      // The face's pieces in the copy.
      ShapeSet pieces;
      for (const TopoDS_Shape& piece : splitter.Modified(face)) {
        for (const TopoDS_Shape& copy : copier.Modified(piece)) {
          pieces.Add(copy);
        }
      }
      // An edge between two of them on the seam, not along a cut.
      for (int i = 1; i <= faces_of.Extent(); ++i) {
        const ShapeList& faces = faces_of(i);
        if (faces.Extent() != 2 || faces.First().IsSame(faces.Last()) || !pieces.Contains(faces.First()) ||
            !pieces.Contains(faces.Last())) {
          continue;
        }
        const TopoDS_Edge& edge = TopoDS::Edge(faces_of.FindKey(i));
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
        if (distance_to(middle, seams) <= tolerance && (cuts.IsNull() || distance_to(middle, cuts) > tolerance)) {
          across.Add(edge);
        }
      }
    }
    if (across.IsEmpty()) {
      return std::nullopt;
    }
    ShapeSet keep;
    for (int i = 1; i <= faces_of.Extent(); ++i) {
      if (!across.Contains(faces_of.FindKey(i))) {
        keep.Add(faces_of.FindKey(i));
      }
    }
    ShapeUpgrade_UnifySameDomain unify(shape, false, true, false);
    unify.KeepShapes(keep);
    unify.SetSafeInputMode(true);
    unify.AllowInternalEdges(false);
    unify.Build();
    const TopoDS_Shape result = unify.Shape();
    if (result.IsNull() || unify.History().IsNull() || !detail::is_valid(result)) {
      return std::nullopt;
    }
    ShapeList arguments;
    arguments.Append(split);
    const occ::handle<BRepTools_History> history = new BRepTools_History(arguments, copier);
    history->Merge(unify.History());
    return Joined{result, history};
  } catch (const Standard_Failure&) {
    return std::nullopt;
  }
}

} // namespace

std::vector<ShapePtr> split_body(const std::string& feature, const Shape& body, const Tool& tool,
                                 bool extend) {
  detail::require_surfaces(body.occt(), "split body");
  return detail::run("split body", [&] {
    const detail::Cutter cutter = detail::make_cutter(tool, detail::box_of(body.occt()), extend);
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(body.occt());
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(cutter.shape);
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.SetNonDestructive(true);
    detail::build(splitter);
    if (!splitter.IsDone() || splitter.HasErrors()) {
      throw std::runtime_error("the body could not be split");
    }
    detail::FaceNamer namer(splitter.Shape());
    namer.carry(splitter, body);
    for (std::size_t i = 0; i < cutter.faces.size(); ++i) {
      const std::string name = split_name(feature, cutter.keys[i]);
      for (const TopoDS_Shape& image : splitter.Modified(cutter.faces[i])) {
        if (image.ShapeType() == TopAbs_FACE && !namer.named(image)) {
          namer.add(image, name);
        }
      }
      if (namer.contains(cutter.faces[i]) && !namer.named(cutter.faces[i])) {
        namer.add(cutter.faces[i], name);
      }
    }
    namer.finish();
    std::vector<detail::FaceNamer::Piece> pieces = namer.pieces();
    if (pieces.size() < 2) {
      throw std::invalid_argument("the splitting tool does not divide the body");
    }
    if (cutter.normal) {
      std::stable_sort(pieces.begin(), pieces.end(), [&](const auto& a, const auto& b) {
        return along(a.shape->occt(), *cutter.normal) < along(b.shape->occt(), *cutter.normal);
      });
    }
    std::vector<ShapePtr> result;
    for (const detail::FaceNamer::Piece& piece : pieces) {
      detail::require_valid(piece.shape->occt(), "the split");
      result.push_back(piece.shape);
    }
    return result;
  });
}

ShapePtr split_faces(const std::string& feature, const Shape& body,
                     const std::vector<std::string>& faces, const Tool& tool, bool extend) {
  (void)feature;
  detail::require_surfaces(body.occt(), "split faces");
  const std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to split");
  return detail::run("split faces", [&] {
    const detail::Cutter cutter = detail::make_cutter(tool, detail::box_of(body.occt()), extend);
    BRepAlgoAPI_Section section(body.occt(), cutter.shape, false);
    section.ComputePCurveOn1(true);
    section.Approximation(true);
    // Destructive, it gave the body's edges p-curves on their planes in
    // place: the body is a cached result (T0e, input_check.hpp).
    section.SetNonDestructive(true);
    detail::build(section);
    if (!section.IsDone() || section.HasErrors()) {
      throw std::runtime_error("the faces could not be intersected with the tool");
    }
    BRepFeat_SplitShape splitter(body.occt());
    std::set<int> crossed;
    // The cuts made, for the pieces joined across a seam below.
    BRep_Builder builder;
    TopoDS_Compound cuts;
    builder.MakeCompound(cuts);
    for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
      TopoDS_Shape face;
      if (!section.HasAncestorFaceOn1(it.Current(), face)) {
        continue;
      }
      const int index = body.face_index(face);
      if (std::find(chosen.begin(), chosen.end(), index) == chosen.end()) {
        continue;
      }
      splitter.Add(TopoDS::Edge(it.Current()), TopoDS::Face(body.face(index)));
      builder.Add(cuts, it.Current());
      crossed.insert(index);
    }
    for (int f : chosen) {
      if (crossed.count(f) == 0) {
        const std::string name = detail::primary_name(body, f);
        throw std::invalid_argument(
            face_error(name, "the tool does not cross one of the faces to split", "the tool does not cross face " + name));
      }
    }
    detail::build(splitter);
    if (!splitter.IsDone()) {
      throw std::runtime_error("the faces could not be split");
    }
    for (int f : chosen) {
      if (splitter.Modified(body.face(f)).Extent() < 2) {
        const std::string name = detail::primary_name(body, f);
        throw std::invalid_argument(
            face_error(name, "the tool does not split one of the faces", "the tool does not split face " + name));
      }
    }
    detail::FaceNamer pieces(splitter.Shape());
    pieces.carry(splitter, body);
    // A region of the tool across a face's seam is one face (mitcad#1853).
    if (const std::optional<Joined> joined = joined_across_seams(splitter.Shape(), splitter, body, chosen, cuts)) {
      detail::FaceNamer namer(joined->shape);
      namer.adopt(pieces, [&joined](const TopoDS_Shape& face) -> TopoDS_Shape {
        const NCollection_List<TopoDS_Shape>& images = joined->history->Modified(face);
        return images.IsEmpty() ? TopoDS_Shape() : images.First();
      });
      namer.finish();
      detail::require_valid(namer.result(), "the split");
      return namer.shape();
    }
    pieces.finish();
    detail::require_valid(pieces.result(), "the split");
    return pieces.shape();
  });
}

ShapePtr extended_tool(const Tool& tool, const gp_Pnt& low, const gp_Pnt& high, bool extend) {
  if (!(low.X() < high.X() && low.Y() < high.Y() && low.Z() < high.Z())) {
    throw std::invalid_argument("extended tool: the box is empty");
  }
  return detail::run("extended tool", [&] {
    Bnd_Box box;
    box.Update(low.X(), low.Y(), low.Z(), high.X(), high.Y(), high.Z());
    // The same cutter the split makes around its body's box.
    const detail::Cutter cutter = detail::make_cutter(tool, box, extend);
    const TopoDS_Shape clip = BRepPrimAPI_MakeBox(low, high).Shape();
    BRepAlgoAPI_Common common;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(cutter.shape);
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(clip);
    common.SetArguments(arguments);
    common.SetTools(tools);
    common.SetNonDestructive(true);
    detail::build(common);
    if (!common.IsDone() || common.HasErrors()) {
      throw std::runtime_error("the tool could not be clipped to the bodies' box");
    }
    if (!TopExp_Explorer(common.Shape(), TopAbs_FACE).More()) {
      throw std::invalid_argument("the tool does not reach the bodies");
    }
    return std::make_shared<Shape>(common.Shape());
  });
}

} // namespace mitcad::geometry
