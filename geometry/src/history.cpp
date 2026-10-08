// SPDX-License-Identifier: MIT
#include "history.hpp"

#include <algorithm>
#include <map>
#include <stdexcept>
#include <utility>

#include <BRep_Builder.hxx>
#include <BRep_PointOnCurve.hxx>
#include <BRep_PointOnCurveOnSurface.hxx>
#include <BRep_PointOnSurface.hxx>
#include <BRep_TVertex.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_List.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>

#include "util.hpp"

namespace mitcad::geometry::detail {

namespace {

// A vertex's points on curves and surfaces, as new representations (OCCT
// updates their parameters in place).
void copy_points(const TopoDS_Shape& from, const TopoDS_Shape& to) {
  const auto* source = static_cast<const BRep_TVertex*>(from.TShape().get());
  auto* target = static_cast<BRep_TVertex*>(to.TShape().get());
  for (const occ::handle<BRep_PointRepresentation>& point : source->Points()) {
    occ::handle<BRep_PointRepresentation> copy;
    if (point->IsPointOnCurve()) {
      copy = new BRep_PointOnCurve(point->Parameter(), point->Curve(), point->Location());
    } else if (point->IsPointOnCurveOnSurface()) {
      copy = new BRep_PointOnCurveOnSurface(point->Parameter(), point->PCurve(), point->Surface(),
                                            point->Location());
    } else if (point->IsPointOnSurface()) {
      copy = new BRep_PointOnSurface(point->Parameter(), point->Parameter2(), point->Surface(),
                                     point->Location());
    }
    if (!copy.IsNull()) {
      target->ChangePoints().Append(copy);
    }
  }
}

} // namespace

InputCopy::InputCopy(const TopoDS_Shape& shape) { m_shapes.push_back(copied(shape)); }

InputCopy::InputCopy(const std::vector<TopoDS_Shape>& shapes) {
  for (const TopoDS_Shape& shape : shapes) {
    m_shapes.push_back(copied(shape));
  }
}

TopoDS_Shape InputCopy::of(const TopoDS_Shape& part) const {
  const auto found = m_copies.find(part.TShape().get());
  if (found == m_copies.end()) {
    throw std::logic_error("a shape that is not part of the copied input");
  }
  return found->second.Located(part.Location()).Oriented(part.Orientation());
}

TopoDS_Shape InputCopy::copied(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return shape;
  }
  const TopoDS_TShape* key = shape.TShape().get();
  auto found = m_copies.find(key);
  if (found == m_copies.end()) {
    const TopoDS_Shape plain = shape.Located(TopLoc_Location()).Oriented(TopAbs_FORWARD);
    // A face's surface and tolerance, an edge's curves, tolerance and
    // flags, a vertex's point and tolerance.
    TopoDS_Shape copy = plain.EmptyCopied();
    BRep_Builder builder;
    switch (plain.ShapeType()) {
    case TopAbs_VERTEX:
      copy_points(plain, copy);
      break;
    case TopAbs_FACE:
      builder.NaturalRestriction(TopoDS::Face(copy), BRep_Tool::NaturalRestriction(TopoDS::Face(plain)));
      break;
    default:
      break;
    }
    for (TopoDS_Iterator it(plain, false, false); it.More(); it.Next()) {
      builder.Add(copy, copied(it.Value()));
    }
    copy.Orientable(plain.Orientable());
    copy.Closed(plain.Closed());
    copy.Infinite(plain.Infinite());
    copy.Convex(plain.Convex());
    found = m_copies.emplace(key, copy).first;
  }
  return found->second.Located(shape.Location()).Oriented(shape.Orientation());
}

ShapePtr working_copy(const Shape& shape) {
  const InputCopy copy(shape.occt());
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < shape.face_count(); ++i) {
    if (!shape.face_names(i).empty()) {
      faces.push_back({copy.of(shape.face(i)), shape.face_names(i)});
    }
  }
  auto result = std::make_shared<Shape>(copy.shape(), faces);
  for (const std::string& note : shape.notes()) {
    result->add_note(note);
  }
  return result;
}

FaceNamer::FaceNamer(TopoDS_Shape result) : m_result(std::move(result)) {
  TopExp::MapShapes(m_result, TopAbs_FACE, m_faces);
  m_names.resize(static_cast<std::size_t>(m_faces.Extent()));
  m_sources.resize(static_cast<std::size_t>(m_faces.Extent()));
}

void FaceNamer::add(const TopoDS_Shape& face, const std::string& name) {
  const int index = m_faces.FindIndex(face);
  if (index > 0) {
    m_names[static_cast<std::size_t>(index - 1)].push_back(name);
  }
}

void FaceNamer::generated(BRepBuilderAPI_MakeShape& operation, const TopoDS_Shape& source,
                          const std::string& name) {
  for (const TopoDS_Shape& image : operation.Generated(source)) {
    if (image.ShapeType() == TopAbs_FACE) {
      add(image, name);
    }
  }
}

void FaceNamer::carry(BRepBuilderAPI_MakeShape& operation, const Shape& input, int source) {
  for (int i = 0; i < input.face_count(); ++i) {
    carry_face(operation, input.face(i), input.face_names(i), source);
  }
}

void FaceNamer::carry(BRepBuilderAPI_MakeShape& operation, const Shape& input,
                      const InputCopy& copy, int source) {
  for (int i = 0; i < input.face_count(); ++i) {
    carry_face(operation, copy.of(input.face(i)), input.face_names(i), source);
  }
}

void FaceNamer::carry(BRepBuilderAPI_MakeShape& operation, const Shape& input,
                      const std::function<TopoDS_Shape(const TopoDS_Shape&)>& stand_in, int source) {
  for (int i = 0; i < input.face_count(); ++i) {
    const TopoDS_Shape face = stand_in(input.face(i));
    if (!face.IsNull()) {
      carry_face(operation, face, input.face_names(i), source);
    }
  }
}

void FaceNamer::add(const TopoDS_Shape& face, const NameList& names, int source) {
  const int index = m_faces.FindIndex(face);
  if (index <= 0) {
    return;
  }
  NameList& own = m_names[static_cast<std::size_t>(index - 1)];
  own.insert(own.end(), names.begin(), names.end());
  if (source >= 0) {
    m_sources[static_cast<std::size_t>(index - 1)].insert(source);
  }
}

void FaceNamer::adopt(const FaceNamer& other, const std::function<TopoDS_Shape(const TopoDS_Shape&)>& image) {
  for (int i = 1; i <= other.m_faces.Extent(); ++i) {
    const TopoDS_Shape& face = other.m_faces(i);
    TopoDS_Shape target = image(face);
    if (target.IsNull()) {
      target = face;
    }
    const int index = m_faces.FindIndex(target);
    if (index <= 0) {
      continue;
    }
    const auto from = static_cast<std::size_t>(i - 1);
    const auto to = static_cast<std::size_t>(index - 1);
    m_names[to].insert(m_names[to].end(), other.m_names[from].begin(), other.m_names[from].end());
    m_sources[to].insert(other.m_sources[from].begin(), other.m_sources[from].end());
  }
}

void FaceNamer::carry_face(BRepBuilderAPI_MakeShape& operation, const TopoDS_Shape& face,
                           const NameList& carried, int source) {
  if (operation.IsDeleted(face)) {
    return;
  }
  const NCollection_List<TopoDS_Shape>& modified = operation.Modified(face);
  std::vector<TopoDS_Shape> images;
  if (modified.IsEmpty()) {
    images.push_back(face);
  } else {
    images.assign(modified.begin(), modified.end());
  }
  for (const TopoDS_Shape& image : images) {
    const int index = m_faces.FindIndex(image);
    if (index <= 0) {
      continue;
    }
    NameList& names = m_names[static_cast<std::size_t>(index - 1)];
    names.insert(names.end(), carried.begin(), carried.end());
    if (source >= 0) {
      m_sources[static_cast<std::size_t>(index - 1)].insert(source);
    }
  }
}

void FaceNamer::carry(const BRepTools_History& history, const Shape& input,
                      const std::vector<int>& except) {
  for (int i = 0; i < input.face_count(); ++i) {
    const TopoDS_Face& face = input.face(i);
    if (history.IsRemoved(face) || std::find(except.begin(), except.end(), i) != except.end()) {
      continue;
    }
    const NCollection_List<TopoDS_Shape>& modified = history.Modified(face);
    std::vector<TopoDS_Shape> images;
    if (modified.IsEmpty()) {
      images.push_back(face);
    } else {
      images.assign(modified.begin(), modified.end());
    }
    for (const TopoDS_Shape& image : images) {
      const int index = m_faces.FindIndex(image);
      if (index > 0) {
        NameList& names = m_names[static_cast<std::size_t>(index - 1)];
        const NameList& carried = input.face_names(i);
        names.insert(names.end(), carried.begin(), carried.end());
      }
    }
  }
}

void FaceNamer::generated(const BRepTools_History& history, const TopoDS_Shape& source,
                          const std::string& name) {
  for (const TopoDS_Shape& image : history.Generated(source)) {
    if (image.ShapeType() == TopAbs_FACE) {
      add(image, name);
    }
  }
}

bool FaceNamer::named(const TopoDS_Shape& face) const {
  const int index = m_faces.FindIndex(face);
  return index > 0 && !m_names[static_cast<std::size_t>(index - 1)].empty();
}

void FaceNamer::finish() {
  std::map<std::string, std::vector<int>> holders;
  for (std::size_t i = 0; i < m_names.size(); ++i) {
    NameList& names = m_names[i];
    std::sort(names.begin(), names.end());
    names.erase(std::unique(names.begin(), names.end()), names.end());
    for (const std::string& name : names) {
      holders[name].push_back(static_cast<int>(i));
    }
  }
  for (const auto& [name, faces] : holders) {
    if (faces.size() < 2) {
      continue;
    }
    std::vector<std::pair<ShapeKey, int>> keyed;
    for (int face : faces) {
      keyed.emplace_back(shape_key(m_faces(face + 1)), face);
    }
    std::sort(keyed.begin(), keyed.end());
    for (std::size_t k = 0; k < keyed.size(); ++k) {
      NameList& names = m_names[static_cast<std::size_t>(keyed[k].second)];
      std::replace(names.begin(), names.end(), name, name + '#' + std::to_string(k));
    }
  }
  for (NameList& names : m_names) {
    std::sort(names.begin(), names.end());
  }
}

std::vector<Shape::NamedFace> FaceNamer::named_faces() const {
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < m_faces.Extent(); ++i) {
    if (!m_names[static_cast<std::size_t>(i)].empty()) {
      faces.push_back({m_faces(i + 1), m_names[static_cast<std::size_t>(i)]});
    }
  }
  return faces;
}

ShapePtr FaceNamer::shape() const {
  auto shape = std::make_shared<Shape>(m_result, named_faces());
  shape->set_unified(m_unified);
  return shape;
}

std::vector<FaceNamer::Piece> FaceNamer::pieces() const {
  std::vector<std::pair<ShapeKey, Piece>> keyed;
  int solids = 0;
  for (TopExp_Explorer it(m_result, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  for (TopExp_Explorer it(m_result, TopAbs_SOLID); it.More(); it.Next()) {
    Piece piece;
    std::vector<Shape::NamedFace> own;
    for (TopExp_Explorer f(it.Current(), TopAbs_FACE); f.More(); f.Next()) {
      const int index = m_faces.FindIndex(f.Current());
      if (index <= 0) {
        continue;
      }
      const auto& sources = m_sources[static_cast<std::size_t>(index - 1)];
      piece.sources.insert(sources.begin(), sources.end());
      const NameList& names = m_names[static_cast<std::size_t>(index - 1)];
      if (!names.empty()) {
        own.push_back({f.Current(), names});
      }
    }
    auto shape = std::make_shared<Shape>(it.Current(), own);
    shape->set_unified(m_unified);
    piece.shape = std::move(shape);
    // One solid needs no order: its key (a volume integral) is the
    // expensive part on large bodies.
    keyed.emplace_back(solids > 1 ? shape_key(it.Current()) : ShapeKey{}, std::move(piece));
  }
  std::stable_sort(keyed.begin(), keyed.end(),
                   [](const auto& a, const auto& b) { return a.first < b.first; });
  std::vector<Piece> result;
  for (auto& entry : keyed) {
    result.push_back(std::move(entry.second));
  }
  return result;
}

} // namespace mitcad::geometry::detail
