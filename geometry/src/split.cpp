// SPDX-License-Identifier: MIT
#include "mitcad/geometry/split.hpp"

#include <algorithm>
#include <cmath>
#include <set>
#include <stdexcept>
#include <utility>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

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
    detail::FaceNamer namer(splitter.Shape());
    namer.carry(splitter, body);
    namer.finish();
    detail::require_valid(namer.result(), "the split");
    return namer.shape();
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
