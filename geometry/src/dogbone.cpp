// SPDX-License-Identifier: MIT
#include "mitcad/geometry/dogbone.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <initializer_list>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Precision.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

// How far from an edge the point that tells an inside corner from an
// outside one lies (mm): well beyond the faces' tolerances, well inside any
// wall a router bit cuts.
constexpr double kProbe = 1.0e-3;
// Directions whose cosine differs from 1 by less than this are parallel.
constexpr double kParallel = 1.0e-8;
// Outward normals nearer than this meet smoothly: no corner.
constexpr double kSmooth = 1.0e-6;

// A straight edge between two faces, seen across it: per face its outward
// normal and the direction along the face away from the edge, both square
// to the edge.
struct Corner {
  gp_Pnt start;
  gp_Dir along;
  double length = 0.0;
  std::array<int, 2> faces{-1, -1};
  std::array<gp_Dir, 2> normal;
  std::array<gp_Dir, 2> side;
  // Into the corner's opening, halfway between the faces' normals.
  gp_Dir bisector;
};

// The corner at an edge, or why there is none (an empty reason: the edge is
// not along `only_along`, when that is given).
struct Found {
  std::optional<Corner> corner;
  std::string why;
};

Found corner_at(const Shape& body, int index, BRepClass3d_SolidClassifier& classifier, const gp_Dir* only_along) {
  const TopoDS_Edge& edge = body.edge(index);
  if (BRep_Tool::Degenerated(edge)) {
    return {std::nullopt, "is degenerate"};
  }
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() != GeomAbs_Line) {
    return {std::nullopt, "is not straight"};
  }
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  Corner corner;
  corner.start = curve.Value(first);
  const gp_Pnt end = curve.Value(last);
  corner.length = corner.start.Distance(end);
  if (!(corner.length > Precision::Confusion())) {
    return {std::nullopt, "is degenerate"};
  }
  corner.along = gp_Dir(gp_Vec(corner.start, end));
  if (only_along != nullptr && std::abs(corner.along.Dot(*only_along)) < 1.0 - kParallel) {
    return {std::nullopt, std::string()};
  }
  corner.faces = body.edge_faces(index);
  if (corner.faces[0] < 0 || corner.faces[1] < 0 || corner.faces[0] == corner.faces[1]) {
    return {std::nullopt, "does not lie between two faces"};
  }
  const double middle = 0.5 * (first + last);
  const gp_Pnt at = curve.Value(middle);
  const gp_Vec along(corner.along);
  std::array<gp_Vec, 2> normals;
  for (std::size_t k = 0; k < 2; ++k) {
    const TopoDS_Face& face = body.face(corner.faces[k]);
    const BRepAdaptor_Curve2d trace(edge, face);
    const gp_Pnt2d uv = trace.Value(middle);
    const BRepAdaptor_Surface surface(face);
    gp_Pnt point;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), point, du, dv);
    gp_Vec normal = du.Crossed(dv);
    if (face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    // Across the edge only.
    normal -= along * normal.Dot(along);
    if (!(normal.Magnitude() > Precision::Confusion())) {
      return {std::nullopt, "has a face that does not turn across it"};
    }
    normals[k] = normal.Normalized();
  }
  const gp_Vec apart = normals[0] - normals[1];
  const gp_Vec opening = normals[0] + normals[1];
  if (apart.Magnitude() < kSmooth) {
    return {std::nullopt, "is not an inside corner: its faces meet smoothly"};
  }
  if (opening.Magnitude() < kSmooth) {
    return {std::nullopt, "is not an inside corner: its faces fold onto each other"};
  }
  // Square to the opening, off the corner: inside the material at an inside
  // corner, outside it at an outside one.
  classifier.Perform(at.Translated(apart.Normalized() * kProbe), Precision::Confusion());
  if (classifier.State() != TopAbs_IN) {
    return {std::nullopt, "is not an inside (concave) corner"};
  }
  corner.bisector = gp_Dir(opening);
  for (std::size_t k = 0; k < 2; ++k) {
    corner.normal[k] = gp_Dir(normals[k]);
    gp_Dir side = corner.along.Crossed(corner.normal[k]);
    if (side.Dot(corner.bisector) < 0.0) {
      side.Reverse();
    }
    corner.side[k] = side;
  }
  return {corner, std::string()};
}

// How far the face reaches from `from` along `side`.
double reach(const TopoDS_Face& face, const gp_Pnt& from, const gp_Dir& side) {
  double farthest = 0.0;
  const gp_Vec direction(side);
  for (TopExp_Explorer explorer(face, TopAbs_EDGE); explorer.More(); explorer.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(explorer.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    for (const double f : {0.0, 0.5, 1.0}) {
      farthest = std::max(farthest, gp_Vec(from, curve.Value(first + f * (last - first))).Dot(direction));
    }
  }
  return farthest;
}

struct Taken {
  int edge = -1;
  Corner corner;
  // The face of the corner (an index of the body's) a relief along a side
  // follows; -1: the longer.
  int side = -1;
};

} // namespace

ShapePtr dogbone_tool(const Shape& body, const DogboneSpec& spec) {
  detail::require_positive("the dogbone's tool diameter", spec.diameter);
  if (!(spec.extra >= 0.0) || !std::isfinite(spec.extra)) {
    throw std::invalid_argument("the dogbone's extra clearance must not be negative");
  }
  const gp_Vec axis_vector(spec.axis[0], spec.axis[1], spec.axis[2]);
  if (!(axis_vector.Magnitude() > Precision::Confusion())) {
    throw std::invalid_argument("the dogbone's tool axis must be a direction");
  }
  const gp_Dir axis(axis_vector);
  return detail::run("dogbone", [&] {
    const TopoDS_Shape& shape = body.occt();
    detail::require_surfaces(shape, "dogbone");
    if (!TopExp_Explorer(shape, TopAbs_SOLID).More()) {
      throw std::invalid_argument("a dogbone cuts a solid body, and this body has no solid");
    }
    BRepClass3d_SolidClassifier classifier(shape);
    std::vector<Taken> taken;
    std::map<int, std::size_t> where;
    // A picked edge (`picked` its reference) must be an inside corner; the
    // edges of picked faces and of the body are taken when they are
    // inside corners along the axis.
    const auto take = [&](int edge, int side, const std::string* picked) {
      const auto known = where.find(edge);
      if (known != where.end()) {
        if (side >= 0 && taken[known->second].side < 0) {
          taken[known->second].side = side;
        }
        return;
      }
      Found found = corner_at(body, edge, classifier, picked != nullptr ? nullptr : &axis);
      if (!found.corner) {
        if (picked != nullptr) {
          throw std::invalid_argument("edge " + *picked + " " + found.why);
        }
        return;
      }
      where.emplace(edge, taken.size());
      taken.push_back({edge, *found.corner, side});
    };
    for (const std::string& name : spec.edges) {
      const std::vector<int> edges = body.find_edges(name);
      if (edges.empty()) {
        throw std::invalid_argument("the body has no edge " + name);
      }
      for (const int edge : edges) {
        take(edge, -1, &name);
      }
    }
    for (const std::string& name : spec.faces) {
      const std::vector<int> faces = body.find_faces(name);
      if (faces.empty()) {
        throw std::invalid_argument("the body has no face " + name);
      }
      for (const int index : faces) {
        const TopoDS_Face& face = body.face(index);
        // The face's own corner edges: a wall of the corner.
        for (TopExp_Explorer explorer(face, TopAbs_EDGE); explorer.More(); explorer.Next()) {
          const int edge = body.edge_index(explorer.Current());
          if (edge >= 0) {
            take(edge, index, nullptr);
          }
        }
        // A face across the axis (a pocket's floor): the corner edges
        // that end on it.
        const BRepAdaptor_Surface surface(face);
        if (surface.GetType() != GeomAbs_Plane ||
            std::abs(surface.Plane().Axis().Direction().Dot(axis)) < 1.0 - kParallel) {
          continue;
        }
        std::set<int> vertices;
        for (TopExp_Explorer explorer(face, TopAbs_VERTEX); explorer.More(); explorer.Next()) {
          vertices.insert(body.vertex_index(explorer.Current()));
        }
        for (int edge = 0; edge < body.edge_count(); ++edge) {
          const TopoDS_Edge& candidate = body.edge(edge);
          if (vertices.count(body.vertex_index(TopExp::FirstVertex(candidate))) > 0 ||
              vertices.count(body.vertex_index(TopExp::LastVertex(candidate))) > 0) {
            take(edge, -1, nullptr);
          }
        }
      }
    }
    if (spec.edges.empty() && spec.faces.empty()) {
      for (int edge = 0; edge < body.edge_count(); ++edge) {
        take(edge, -1, nullptr);
      }
    }
    if (taken.empty()) {
      throw std::invalid_argument(spec.faces.empty()
                                      ? "the body has no inside corner along the tool axis"
                                      : "the picked faces have no inside corner along the tool axis");
    }
    const double radius = spec.diameter / 2.0;
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    std::vector<Shape::NamedFace> names;
    for (const Taken& relief : taken) {
      const Corner& corner = relief.corner;
      gp_Vec offset;
      if (spec.along_side) {
        std::size_t k = 0;
        if (relief.side >= 0 && (relief.side == corner.faces[0] || relief.side == corner.faces[1])) {
          k = relief.side == corner.faces[0] ? std::size_t{0} : std::size_t{1};
        } else {
          const double reach0 = reach(body.face(corner.faces[0]), corner.start, corner.side[0]);
          const double reach1 = reach(body.face(corner.faces[1]), corner.start, corner.side[1]);
          k = reach0 >= reach1 ? std::size_t{0} : std::size_t{1};
        }
        offset = gp_Vec(corner.normal[k]) * radius - gp_Vec(corner.side[k]) * spec.extra;
      } else {
        offset = gp_Vec(corner.bisector) * (radius - spec.extra);
      }
      BRepPrimAPI_MakeCylinder maker(gp_Ax2(corner.start.Translated(offset), corner.along), radius,
                                     corner.length);
      maker.Build();
      if (!maker.IsDone()) {
        throw std::runtime_error("a dogbone's relief could not be built");
      }
      const TopoDS_Shape cylinder = maker.Shape();
      const std::string& edge_name = body.edge_name(relief.edge);
      const std::string key = edge_name.empty() ? std::to_string(relief.edge) : edge_name;
      for (TopExp_Explorer explorer(cylinder, TopAbs_FACE); explorer.More(); explorer.Next()) {
        const TopoDS_Face& face = TopoDS::Face(explorer.Current());
        std::string role = "dogbone";
        if (BRepAdaptor_Surface(face).GetType() == GeomAbs_Plane) {
          const TopExp_Explorer vertex(face, TopAbs_VERTEX);
          const gp_Pnt point = BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
          role = gp_Vec(corner.start, point).Dot(gp_Vec(corner.along)) < 0.5 * corner.length ? "dogbone_start"
                                                                                           : "dogbone_end";
        }
        names.push_back({face, {face_name(spec.feature, role, key)}});
      }
      builder.Add(compound, cylinder);
    }
    return std::make_shared<Shape>(compound, names);
  });
}

} // namespace mitcad::geometry
