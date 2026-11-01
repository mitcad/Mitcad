// SPDX-License-Identifier: MIT
#include "history.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <stdexcept>
#include <set>
#include <string>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <Bnd_Box.hxx>
#include <Precision.hxx>
#include <gp_Pnt2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_CurveOnClosedSurface.hxx>
#include <BRep_CurveOnSurface.hxx>
#include <BRep_PointOnCurve.hxx>
#include <BRep_PointOnCurveOnSurface.hxx>
#include <BRep_PointOnSurface.hxx>
#include <BRep_TEdge.hxx>
#include <BRep_TVertex.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "util.hpp"

namespace mitcad::geometry::detail {

namespace {

// Whether a solid of a few faces is no thicker than 1e-6 mm anywhere (its
// volume under its area times that): a sliver an operation leaves between
// faces that coincide (a join of walls that continue the body's walls left
// strips of 1e-6 mm wide as solids of two faces), never a body.
bool is_sliver(const TopoDS_Shape& solid) {
  int faces = 0;
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    if (++faces > 6) {
      return false;
    }
  }
  GProp_GProps volume;
  BRepGProp::VolumeProperties(solid, volume);
  GProp_GProps area;
  BRepGProp::SurfaceProperties(solid, area);
  return std::abs(volume.Mass()) <= 1e-6 * area.Mass();
}

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

// Whether MITCAD_DEBUG_NAMING is set (FaceNamer::carry_by_surface).
bool debug_naming() {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_DEBUG_NAMING") != 0 || value == nullptr) {
      return false;
    }
    std::free(value);
    return true;
#else
    return std::getenv("MITCAD_DEBUG_NAMING") != nullptr;
#endif
  }();
  return on;
}

std::string number(double value) {
  char text[32];
  std::snprintf(text, sizeof text, "%.3g", value);
  return text;
}

// The direction a planar face faces.
gp_Dir facing(const TopoDS_Face& face, const gp_Pln& plane) {
  const gp_Dir normal = plane.Axis().Direction();
  return face.Orientation() == TopAbs_REVERSED ? normal.Reversed() : normal;
}

// The point of the face's surface at the middle of the face's parameters.
gp_Pnt parameter_middle(const TopoDS_Face& face) {
  double u1 = 0.0, u2 = 0.0, v1 = 0.0, v2 = 0.0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  return BRepAdaptor_Surface(face, false).Value(0.5 * (u1 + u2), 0.5 * (v1 + v2));
}

// Up to `count` points inside the face, spread over it: the middles of the
// cells of a coarse grid over its parameters that lie inside it, of a fine
// one for a narrow face.
std::vector<gp_Pnt> points_inside(const TopoDS_Face& face, std::size_t count) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
  std::vector<gp_Pnt2d> inside;
  for (const int cells : {5, 24}) {
    for (int a = 0; a < cells; ++a) {
      for (int b = 0; b < cells; ++b) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * (a + 0.5) / cells, v0 + (v1 - v0) * (b + 0.5) / cells);
        if (classifier.Perform(uv) == TopAbs_IN) {
          inside.push_back(uv);
        }
      }
    }
    if (!inside.empty()) {
      break;
    }
  }
  std::vector<gp_Pnt> points;
  const BRepAdaptor_Surface surface(face);
  const std::size_t wanted = std::min(inside.size(), count);
  for (std::size_t k = 0; k < wanted; ++k) {
    const gp_Pnt2d& uv = inside[k * inside.size() / wanted];
    points.push_back(surface.Value(uv.X(), uv.Y()));
  }
  return points;
}

// Whether faces a and b lie on one surface: the same plane facing the same
// way, or surfaces of one kind each through the middle of the other face
// (within the faces' tolerances, at least 1e-6 mm). `why` says why not.
bool on_one_surface(const TopoDS_Face& a, const TopoDS_Face& b, std::string& why) {
  const double tolerance = std::max({1.0e-6, BRep_Tool::Tolerance(a), BRep_Tool::Tolerance(b)});
  const BRepAdaptor_Surface sa(a, false);
  const BRepAdaptor_Surface sb(b, false);
  if (sa.GetType() != sb.GetType()) {
    why = "another kind of surface";
    return false;
  }
  if (sa.GetType() == GeomAbs_Plane) {
    const double angle = facing(a, sa.Plane()).Angle(facing(b, sb.Plane()));
    if (angle > 1.0e-7) {
      why = "a plane facing " + number(angle) + " rad another way";
      return false;
    }
    const double distance = sb.Plane().Distance(sa.Plane().Location());
    if (distance > tolerance) {
      why = "a parallel plane " + number(distance) + " mm away";
      return false;
    }
    return true;
  }
  const std::pair<const TopoDS_Face*, const TopoDS_Face*> orders[] = {{&a, &b}, {&b, &a}};
  for (const auto& [from, to] : orders) {
    GeomAPI_ProjectPointOnSurf projection(parameter_middle(*from), BRep_Tool::Surface(*to));
    if (projection.NbPoints() == 0 || projection.LowerDistance() > tolerance) {
      why = "a surface of its kind " +
            (projection.NbPoints() == 0 ? std::string("not reached") : number(projection.LowerDistance()) + " mm") +
            " from the other face's middle";
      return false;
    }
  }
  return true;
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
  result->copy_notes(shape);
  return result;
}

void own_pcurves(const TopoDS_Shape& copy) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(copy, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    auto* edge = static_cast<BRep_TEdge*>(edges(i).TShape().get());
    for (const occ::handle<BRep_CurveRepresentation>& curve : edge->ChangeCurves()) {
      if (!curve->IsCurveOnSurface()) {
        continue;
      }
      const auto on = occ::down_cast<BRep_CurveOnSurface>(curve);
      if (!on.IsNull() && !on->PCurve().IsNull()) {
        on->PCurve(occ::down_cast<Geom2d_Curve>(on->PCurve()->Copy()));
      }
      const auto closed = occ::down_cast<BRep_CurveOnClosedSurface>(curve);
      if (!closed.IsNull() && !closed->PCurve2().IsNull()) {
        closed->PCurve2(occ::down_cast<Geom2d_Curve>(closed->PCurve2()->Copy()));
      }
    }
  }
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

void FaceNamer::carry_by_surface(const Shape& input, const char* operation) {
  std::set<std::string> held;
  bool unnamed = false;
  for (const NameList& names : m_names) {
    held.insert(names.begin(), names.end());
    unnamed = unnamed || names.empty();
  }
  if (!unnamed) {
    return;
  }
  // The input's faces whose names reach no face of the result: deleted by
  // the history, or carried to images that are no faces of the result.
  std::vector<int> lost;
  for (int i = 0; i < input.face_count(); ++i) {
    const NameList& names = input.face_names(i);
    if (!names.empty() &&
        std::none_of(names.begin(), names.end(), [&held](const std::string& name) { return held.count(name) > 0; })) {
      lost.push_back(i);
    }
  }
  const bool debug = debug_naming();
  for (int f = 1; f <= m_faces.Extent(); ++f) {
    NameList& names = m_names[static_cast<std::size_t>(f - 1)];
    if (!names.empty()) {
      continue;
    }
    const TopoDS_Face& face = TopoDS::Face(m_faces(f));
    std::string report;
    for (int i = 0; i < input.face_count(); ++i) {
      const bool candidate = std::find(lost.begin(), lost.end(), i) != lost.end();
      if (input.face_names(i).empty() || (!candidate && !debug)) {
        continue;
      }
      std::string why;
      bool same = false;
      try {
        OCC_CATCH_SIGNALS
        same = on_one_surface(input.face(i), face, why);
      } catch (const Standard_Failure& failure) {
        why = std::string("the comparison failed: ") + failure.what();
      }
      if (same && candidate) {
        names.insert(names.end(), input.face_names(i).begin(), input.face_names(i).end());
        report += "\n  taken: " + input.face_names(i).front();
      } else if (same) {
        report += "\n  on its surface, but its names are on another face of the result: " +
                  input.face_names(i).front();
      } else if (candidate) {
        report += "\n  " + input.face_names(i).front() + ": " + why;
      }
    }
    if (debug) {
      std::fprintf(stderr, "%s: face %d of the result had no name; %zu input faces lost theirs%s\n", operation,
                   f - 1, lost.size(), report.c_str());
    }
  }
}

void FaceNamer::carry_by_containment(const Shape& input, double tolerance) {
  for (int f = 1; f <= m_faces.Extent(); ++f) {
    NameList& names = m_names[static_cast<std::size_t>(f - 1)];
    if (!names.empty()) {
      continue;
    }
    try {
      OCC_CATCH_SIGNALS
      const TopoDS_Face& face = TopoDS::Face(m_faces(f));
      const std::vector<gp_Pnt> points = points_inside(face, 3);
      if (points.empty()) {
        continue;
      }
      int best = -1;
      double nearest = 0.0;
      for (int i = 0; i < input.face_count(); ++i) {
        if (input.face_names(i).empty()) {
          continue;
        }
        const double within =
            std::max({tolerance, BRep_Tool::Tolerance(face), BRep_Tool::Tolerance(input.face(i))});
        Bnd_Box box;
        BRepBndLib::Add(input.face(i), box);
        box.Enlarge(within);
        if (std::any_of(points.begin(), points.end(), [&box](const gp_Pnt& p) { return box.IsOut(p); })) {
          continue;
        }
        double farthest = 0.0;
        for (const gp_Pnt& p : points) {
          BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), input.face(i));
          farthest = std::max(farthest, distance.IsDone() ? distance.Value() : 2.0 * within);
          if (farthest > within) {
            break;
          }
        }
        if (farthest <= within && (best < 0 || farthest < nearest)) {
          best = i;
          nearest = farthest;
        }
      }
      if (best >= 0) {
        names.insert(names.end(), input.face_names(best).begin(), input.face_names(best).end());
      }
    } catch (const Standard_Failure&) {
    }
  }
}

bool FaceNamer::holds(const std::string& name) const {
  return std::any_of(m_names.begin(), m_names.end(), [&name](const NameList& names) {
    return std::find(names.begin(), names.end(), name) != names.end();
  });
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
    // In piece_order, so that rounding noise does not swap pieces lying
    // symmetrically (mitcad#895).
    std::vector<TopoDS_Shape> holding;
    holding.reserve(faces.size());
    for (int face : faces) {
      holding.push_back(m_faces(face + 1));
    }
    const std::vector<std::size_t> order = piece_order(holding);
    for (std::size_t k = 0; k < order.size(); ++k) {
      NameList& names = m_names[static_cast<std::size_t>(faces[order[k]])];
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
  if (m_valid) {
    shape->set_checked_valid(true);
  }
  return shape;
}

std::vector<FaceNamer::Piece> FaceNamer::pieces() const {
  std::vector<std::pair<ShapeKey, Piece>> keyed;
  int solids = 0;
  for (TopExp_Explorer it(m_result, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  for (TopExp_Explorer it(m_result, TopAbs_SOLID); it.More(); it.Next()) {
    // Slivers beside other solids are no pieces.
    if (solids > 1 && is_sliver(it.Current())) {
      continue;
    }
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
    if (m_valid) {
      shape->set_checked_valid(true);
    }
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

namespace mitcad::geometry {

ShapePtr independent_copy(const Shape& shape) { return detail::working_copy(shape); }

} // namespace mitcad::geometry
