// SPDX-License-Identifier: MIT
#include "node_dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <ElCLib.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ChFi3d.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry::detail {
namespace {

// Relative tolerance of equal radii, crossing axes and points on surfaces.
constexpr double kSame = 1.0e-7;
// Axes closer than this to parallel (radians) do not cross.
constexpr double kAngle = 1.0e-3;
// How far past a node's facet a rounding runs before it is cut, in radii:
// past where it meets the other roundings at the node, its end inside them
// and the third tube.
constexpr double kPastNode = 3.0;

using AncestorMap = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// Where the rounding is cut at one end of its edge: by `plane`, keeping the
// side of the edge; on the body's plane `face` at a far end, -1 past a
// node.
struct EndCut {
  gp_Pln plane;
  int face = -1;
  // At a node, the other tubes round its facet: the rounding keeps out of
  // them (inside a tube it adds nothing, and past the node a tube's bore
  // may lie there).
  std::vector<gp_Cylinder> others;
  gp_Pnt at; // the facet's vertex the edge ends at
};

// A valley: its edges (a chain where the booleans split its curve) and
// its rounding's radius.
struct Valley {
  int edge = -1;            // its first edge
  std::vector<int> pieces;  // all of them
  double radius = 0.0;
  std::array<gp_Cylinder, 2> cylinders;
  gp_Pnt crossing; // of the two axes
  gp_Pnt middle;   // of the edge
  std::array<EndCut, 2> ends;
};

// A face's cylinder whose material lies inside it (a tube's outside).
std::optional<gp_Cylinder> convex_cylinder(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Cylinder) {
    return std::nullopt;
  }
  const gp_Cylinder cylinder = surface.Cylinder();
  // (The surface's normal points away from the axis for a direct frame.)
  const bool outward = cylinder.Position().Direct() == (face.Orientation() == TopAbs_FORWARD);
  if (!outward) {
    return std::nullopt;
  }
  return cylinder;
}

double axis_distance(const gp_Cylinder& cylinder, const gp_Pnt& p) {
  return gp_Lin(cylinder.Axis()).Distance(p);
}

// The faces of the body at a vertex, each once.
std::vector<TopoDS_Shape> faces_at(const AncestorMap& vertex_faces, const TopoDS_Shape& vertex) {
  std::vector<TopoDS_Shape> faces;
  if (!vertex_faces.Contains(vertex)) {
    return faces;
  }
  for (const TopoDS_Shape& f : vertex_faces.FindFromKey(vertex)) {
    if (std::none_of(faces.begin(), faces.end(), [&](const TopoDS_Shape& g) { return g.IsSame(f); })) {
      faces.push_back(f);
    }
  }
  return faces;
}

// The edge's valley: two convex cylinders of one radius whose axes cross,
// the edge concave between them; its ends not yet set.
std::optional<Valley> valley_of(const Shape& body, const EdgeFaces& edge_faces, const NodeEdge& item) {
  const std::vector<int> faces = edge_faces.at(item.edge);
  if (faces.size() != 2 || faces[0] == faces[1] || !(item.radius > 0.0)) {
    return std::nullopt;
  }
  Valley valley;
  valley.edge = item.edge;
  valley.pieces = {item.edge};
  valley.radius = item.radius;
  for (std::size_t i = 0; i < 2; ++i) {
    const std::optional<gp_Cylinder> cylinder = convex_cylinder(body.face(faces[i]));
    if (!cylinder) {
      return std::nullopt;
    }
    valley.cylinders[i] = *cylinder;
  }
  const gp_Cylinder& c0 = valley.cylinders[0];
  const gp_Cylinder& c1 = valley.cylinders[1];
  const double r = c0.Radius();
  const double tolerance = kSame * std::max(1.0, r);
  if (std::abs(c1.Radius() - r) > tolerance) {
    return std::nullopt;
  }
  const gp_Dir d0 = c0.Axis().Direction();
  const gp_Dir d1 = c1.Axis().Direction();
  const gp_Vec normal = gp_Vec(d0).Crossed(gp_Vec(d1));
  if (normal.Magnitude() < std::sin(kAngle)) {
    return std::nullopt;
  }
  const gp_Pnt p0 = c0.Axis().Location();
  const gp_Pnt p1 = c1.Axis().Location();
  const gp_Vec between(p0, p1);
  if (std::abs(between.Dot(normal)) / normal.Magnitude() > tolerance) {
    return std::nullopt;
  }
  // (Where the axes cross: on axis 0 at the parameter that meets axis 1.)
  const gp_Vec n1 = gp_Vec(d1).Crossed(normal);
  const double t0 = between.Dot(n1) / gp_Vec(d0).Dot(n1);
  valley.crossing = p0.Translated(gp_Vec(d0) * t0);
  const BRepAdaptor_Curve curve(body.edge(item.edge));
  valley.middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
  for (const gp_Cylinder& c : valley.cylinders) {
    if (std::abs(axis_distance(c, valley.middle) - r) > 10.0 * tolerance) {
      return std::nullopt;
    }
  }
  // Concave: the two tubes joined there.
  if (ChFi3d::DefineConnectType(body.edge(item.edge), body.face(faces[0]), body.face(faces[1]), 1.0e-4, false) !=
      ChFiDS_Concave) {
    return std::nullopt;
  }
  return valley;
}

// The direction of the edge at its end `e` (0 first, 1 last, as the edge
// is oriented), pointing out of the edge.
gp_Dir leaving(const TopoDS_Edge& edge, int e) {
  const BRepAdaptor_Curve curve(edge);
  gp_Pnt p;
  gp_Vec d;
  const bool first = (e == 0) == (edge.Orientation() != TopAbs_REVERSED);
  curve.D1(first ? curve.FirstParameter() : curve.LastParameter(), p, d);
  return first ? gp_Dir(d.Reversed()) : gp_Dir(d);
}

// The material between the ball and the valley's two cylinders, along the
// valley between its end cuts: OCCT's rounding of the valley edge of the
// two tubes (as solid rods, cut where the rounding ends), less the rods.
// Its rounding is named `name` (its edges' roundings), its face on a far
// end's plane that plane's names, its other faces `other`. Null where it
// cannot be built.
ShapePtr fill_of(const Shape& body, const Valley& valley, double reach, const NameList& name,
                 const std::string& other) {
  TopoDS_Shape rods;
  {
    std::array<TopoDS_Shape, 2> rod;
    for (std::size_t i = 0; i < 2; ++i) {
      const gp_Cylinder& c = valley.cylinders[i];
      const gp_Dir d = c.Axis().Direction();
      const gp_Pnt start = valley.crossing.Translated(gp_Vec(d) * -reach);
      // (Its seam on the far side from the valley, off the rounding.)
      const gp_Lin axis(c.Axis());
      const gp_Pnt foot = ElCLib::Value(ElCLib::Parameter(axis, valley.middle), axis);
      rod[i] = BRepPrimAPI_MakeCylinder(gp_Ax2(start, d, gp_Dir(gp_Vec(valley.middle, foot))), c.Radius(), 2.0 * reach)
                   .Shape();
    }
    BRepAlgoAPI_Fuse fuse(rod[0], rod[1]);
    if (!fuse.IsDone() || fuse.HasErrors()) {
      return nullptr;
    }
    rods = fuse.Shape();
  }
  for (const EndCut& end : valley.ends) {
    // A box past the cut, away from the edge.
    gp_Dir outward = end.plane.Axis().Direction();
    if (gp_Vec(end.plane.Location(), valley.middle).Dot(gp_Vec(outward)) > 0.0) {
      outward.Reverse();
    }
    const gp_Ax2 frame(end.plane.Location(), outward);
    const double size = 4.0 * reach;
    const gp_Pnt corner = end.plane.Location().Translated(gp_Vec(frame.XDirection()) * -size +
                                                          gp_Vec(frame.YDirection()) * -size);
    const TopoDS_Shape box =
        BRepPrimAPI_MakeBox(gp_Ax2(corner, outward, frame.XDirection()), 2.0 * size, 2.0 * size, size).Shape();
    BRepAlgoAPI_Cut cut(rods, box);
    if (!cut.IsDone() || cut.HasErrors()) {
      return nullptr;
    }
    rods = cut.Shape();
  }
  // The rods' edge along the valley, through the body's edge.
  const double tolerance = 1.0e-6 * std::max(1.0, valley.cylinders[0].Radius());
  TopoDS_Edge along;
  for (TopExp_Explorer it(rods, TopAbs_EDGE); it.More() && along.IsNull(); it.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(it.Current());
    double f = 0.0;
    double l = 0.0;
    const occ::handle<Geom_Curve> c = BRep_Tool::Curve(e, f, l);
    if (c.IsNull()) {
      continue;
    }
    GeomAPI_ProjectPointOnCurve projection(valley.middle, c, f, l);
    if (projection.NbPoints() > 0 && projection.LowerDistance() <= tolerance) {
      along = e;
    }
  }
  if (along.IsNull()) {
    return nullptr;
  }
  BRepFilletAPI_MakeFillet rounding(rods);
  rounding.Add(valley.radius, along);
  rounding.Build();
  if (!rounding.IsDone()) {
    return nullptr;
  }
  BRepAlgoAPI_Cut fill(rounding.Shape(), rods);
  if (!fill.IsDone() || fill.HasErrors()) {
    return nullptr;
  }
  TopoDS_Shape kept = fill.Shape();
  for (const EndCut& end : valley.ends) {
    for (const gp_Cylinder& c : end.others) {
      const gp_Lin axis(c.Axis());
      const gp_Pnt nearest = ElCLib::Value(ElCLib::Parameter(axis, valley.crossing), axis);
      const gp_Dir d = c.Axis().Direction();
      const TopoDS_Shape rod =
          BRepPrimAPI_MakeCylinder(gp_Ax2(nearest.Translated(gp_Vec(d) * -reach), d), c.Radius(), 2.0 * reach).Shape();
      // (Only about the node: the tube may not run on along its whole axis.)
      const TopoDS_Shape ball = BRepPrimAPI_MakeSphere(end.at, (kPastNode + 2.0) * valley.radius).Shape();
      BRepAlgoAPI_Common local(rod, ball);
      if (!local.IsDone() || local.HasErrors()) {
        return nullptr;
      }
      BRepAlgoAPI_Cut out(kept, local.Shape());
      if (!out.IsDone() || out.HasErrors()) {
        return nullptr;
      }
      kept = out.Shape();
    }
  }
  int solids = 0;
  for (TopExp_Explorer it(kept, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  if (solids != 1) {
    return nullptr;
  }
  std::vector<Shape::NamedFace> faces;
  int rounded = 0;
  for (TopExp_Explorer it(kept, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const BRepAdaptor_Surface surface(face, false);
    NameList names{other};
    if (surface.GetType() == GeomAbs_Plane) {
      const gp_Pln plane = surface.Plane();
      for (const EndCut& end : valley.ends) {
        if (end.face >= 0 && plane.Axis().Direction().IsParallel(end.plane.Axis().Direction(), kAngle) &&
            end.plane.Distance(plane.Location()) <= tolerance) {
          names = body.face_names(end.face);
        }
      }
    } else if (surface.GetType() != GeomAbs_Cylinder) {
      names = name;
      ++rounded;
    }
    faces.push_back({face, names});
  }
  if (rounded == 0) {
    return nullptr;
  }
  return std::make_shared<Shape>(kept, faces);
}

} // namespace

ShapePtr node_fillet(const std::string& feature, const Shape& body, const std::vector<NodeEdge>& edges) {
  if (edges.size() < 3) {
    return nullptr;
  }
  const EdgeFaces edge_faces(body);
  AncestorMap vertex_faces;
  TopExp::MapShapesAndAncestors(body.occt(), TopAbs_VERTEX, TopAbs_FACE, vertex_faces);
  std::vector<Valley> valleys;
  // The edges ending at each vertex.
  AncestorMap vertex_edges;
  for (const NodeEdge& item : edges) {
    std::optional<Valley> valley = valley_of(body, edge_faces, item);
    if (!valley || body.edge_name(item.edge).empty()) {
      return nullptr;
    }
    valleys.push_back(std::move(*valley));
    TopoDS_Vertex v0;
    TopoDS_Vertex v1;
    TopExp::Vertices(body.edge(item.edge), v0, v1);
    if (v0.IsNull() || v1.IsNull() || v0.IsSame(v1)) {
      return nullptr;
    }
    for (const TopoDS_Vertex& v : {v0, v1}) {
      if (!vertex_edges.Contains(v)) {
        vertex_edges.Add(v, NCollection_List<TopoDS_Shape>());
      }
      vertex_edges.ChangeFromKey(v).Append(body.edge(item.edge));
    }
  }
  // A node's facet: a face all of whose vertices end exactly one edge of
  // the set, none of its own edges in it.
  const auto facet = [&](const TopoDS_Shape& face) {
    for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
      if (!vertex_edges.Contains(it.Current()) || vertex_edges.FindFromKey(it.Current()).Extent() != 1) {
        return false;
      }
    }
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      const int index = body.edge_index(it.Current());
      if (std::any_of(edges.begin(), edges.end(), [&](const NodeEdge& item) { return item.edge == index; })) {
        return false;
      }
    }
    return true;
  };
  Bnd_Box box;
  BRepBndLib::Add(body.occt(), box);
  if (box.IsVoid()) {
    return nullptr;
  }
  const double reach = 2.0 * std::sqrt(box.SquareExtent());
  // Edges of one valley that the booleans split: two edges of the set
  // between the same two faces, meeting at a vertex of those faces alone,
  // are one valley.
  for (bool joined = true; joined;) {
    joined = false;
    for (std::size_t i = 0; i < valleys.size() && !joined; ++i) {
      for (std::size_t j = i + 1; j < valleys.size() && !joined; ++j) {
        std::vector<int> fi = edge_faces.at(valleys[i].edge);
        std::vector<int> fj = edge_faces.at(valleys[j].edge);
        std::sort(fi.begin(), fi.end());
        std::sort(fj.begin(), fj.end());
        if (fi != fj) {
          continue;
        }
        for (const int a : valleys[i].pieces) {
          for (const int b : valleys[j].pieces) {
            TopoDS_Vertex a0;
            TopoDS_Vertex a1;
            TopoDS_Vertex b0;
            TopoDS_Vertex b1;
            TopExp::Vertices(body.edge(a), a0, a1);
            TopExp::Vertices(body.edge(b), b0, b1);
            for (const TopoDS_Vertex& v : {a0, a1}) {
              if (joined || !(v.IsSame(b0) || v.IsSame(b1)) || vertex_edges.FindFromKey(v).Extent() != 2) {
                continue;
              }
              const std::vector<TopoDS_Shape> round = faces_at(vertex_faces, v);
              joined = round.size() == 2;
            }
          }
        }
        if (joined) {
          valleys[i].pieces.insert(valleys[i].pieces.end(), valleys[j].pieces.begin(), valleys[j].pieces.end());
          valleys.erase(valleys.begin() + static_cast<std::ptrdiff_t>(j));
        }
      }
    }
  }
  int nodes = 0;
  for (Valley& valley : valleys) {
    // The chain's ends: its edges' vertices that end one of them only.
    std::vector<std::pair<TopoDS_Vertex, gp_Dir>> chain_ends;
    for (const int piece : valley.pieces) {
      const TopoDS_Edge& edge = body.edge(piece);
      for (int e = 0; e < 2; ++e) {
        const TopoDS_Vertex vertex = e == 0 ? TopExp::FirstVertex(edge, true) : TopExp::LastVertex(edge, true);
        int count = 0;
        for (const int other_piece : valley.pieces) {
          TopoDS_Vertex v0;
          TopoDS_Vertex v1;
          TopExp::Vertices(body.edge(other_piece), v0, v1);
          count += (vertex.IsSame(v0) ? 1 : 0) + (vertex.IsSame(v1) ? 1 : 0);
        }
        if (count == 1) {
          chain_ends.emplace_back(vertex, leaving(edge, e));
        }
      }
    }
    if (chain_ends.size() != 2) {
      return nullptr;
    }
    for (int e = 0; e < 2; ++e) {
      const TopoDS_Vertex& vertex = chain_ends[static_cast<std::size_t>(e)].first;
      std::vector<TopoDS_Shape> third;
      for (const TopoDS_Shape& f : faces_at(vertex_faces, vertex)) {
        const int index = body.face_index(f);
        const std::vector<int> own = edge_faces.at(valley.edge);
        if (std::find(own.begin(), own.end(), index) == own.end()) {
          third.push_back(f);
        }
      }
      if (third.size() != 1) {
        return nullptr;
      }
      EndCut& end = valley.ends[static_cast<std::size_t>(e)];
      const gp_Pnt at = BRep_Tool::Pnt(vertex);
      if (facet(third.front())) {
        // Past the node: square to the edge, the rounding's reach on.
        const gp_Dir& out = chain_ends[static_cast<std::size_t>(e)].second;
        end.plane = gp_Pln(at.Translated(gp_Vec(out) * (kPastNode * valley.radius)), out);
        end.face = -1;
        end.at = at;
        const std::vector<int> own = edge_faces.at(valley.edge);
        for (TopExp_Explorer it(third.front(), TopAbs_EDGE); it.More(); it.Next()) {
          for (const int f : edge_faces.at(body.edge_index(it.Current()))) {
            if (std::find(own.begin(), own.end(), f) != own.end() || body.face(f).IsSame(third.front())) {
              continue;
            }
            const std::optional<gp_Cylinder> c = convex_cylinder(body.face(f));
            if (!c) {
              return nullptr;
            }
            end.others.push_back(*c);
          }
        }
        if (end.others.empty()) {
          return nullptr;
        }
        ++nodes;
        continue;
      }
      if (vertex_edges.FindFromKey(vertex).Extent() != 1) {
        return nullptr;
      }
      const BRepAdaptor_Surface surface(TopoDS::Face(third.front()), false);
      if (surface.GetType() != GeomAbs_Plane) {
        return nullptr;
      }
      end.plane = surface.Plane();
      end.face = body.face_index(third.front());
    }
  }
  if (nodes == 0) {
    return nullptr;
  }
  // The fills joined first, then joined to the body at once.
  const std::string other = face_name(feature, "node");
  std::vector<ShapePtr> fills;
  NCollection_List<TopoDS_Shape> first;
  NCollection_List<TopoDS_Shape> rest;
  for (const Valley& valley : valleys) {
    ShapePtr fill;
    try {
      NameList names;
      for (const int piece : valley.pieces) {
        names.push_back(face_name(feature, "fillet", body.edge_name(piece)));
      }
      fill = fill_of(body, valley, reach, names, other);
    } catch (const Standard_Failure&) {
      fill = nullptr;
    }
    if (!fill) {
      return nullptr;
    }
    (fills.empty() ? first : rest).Append(fill->occt());
    fills.push_back(std::move(fill));
  }
  BRepAlgoAPI_Fuse fuse;
  fuse.SetArguments(first);
  fuse.SetTools(rest);
  fuse.SetRunParallel(false);
  fuse.Build();
  if (!fuse.IsDone() || fuse.HasErrors()) {
    return nullptr;
  }
  FaceNamer namer(fuse.Shape());
  for (const ShapePtr& fill : fills) {
    namer.carry(fuse, *fill);
  }
  namer.finish();
  const ShapePtr tool = namer.shape();
  const BooleanResult made = boolean(BooleanOp::Join, {&body}, *tool);
  if (made.pieces.size() != 1 || made.touched.empty() || !made.touched.front()) {
    return nullptr;
  }
  const ShapePtr result = made.pieces.front().shape;
  // The fills' faces on the rods and past the nodes lie inside the body or
  // the other fills: none is left; every rounding is.
  for (int f = 0; f < result->face_count(); ++f) {
    if (face_matches(result->face_names(f), other)) {
      return nullptr;
    }
  }
  for (const NodeEdge& item : edges) {
    const std::string name = face_name(feature, "fillet", body.edge_name(item.edge));
    bool held = false;
    for (int f = 0; f < result->face_count() && !held; ++f) {
      held = face_matches(result->face_names(f), name);
    }
    if (!held) {
      return nullptr;
    }
  }
  if (!BRepCheck_Analyzer(result->occt(), true, true).IsValid()) {
    return nullptr;
  }
  return result;
}

} // namespace mitcad::geometry::detail
