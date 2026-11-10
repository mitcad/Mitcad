// SPDX-License-Identifier: MIT
#include "face_select.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d.hxx>
#include <ChFiDS_TypeOfConcavity.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/datum.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace mitcad::geometry::detail {
namespace {

// Faces meeting at less than this angle (radians) are tangent.
constexpr double kSmoothAngle = 1.0e-3;

void add_once(std::vector<int>& list, int value) {
  if (std::find(list.begin(), list.end(), value) == list.end()) {
    list.push_back(value);
  }
}

} // namespace

std::vector<int> resolve_faces(const Shape& body, const std::vector<std::string>& references,
                               const char* what) {
  if (references.empty()) {
    throw std::invalid_argument(std::string("no ") + what + " selected");
  }
  std::vector<int> faces;
  for (const std::string& reference : references) {
    const std::vector<int> found = body.find_faces(reference);
    if (found.empty()) {
      throw std::invalid_argument(no_face_error(reference));
    }
    for (int i : found) {
      add_once(faces, i);
    }
  }
  return faces;
}

std::vector<int> resolve_edges(const Shape& body, const std::vector<std::string>& edges,
                               const std::vector<std::string>& faces) {
  if (edges.empty() && faces.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  std::vector<int> result;
  for (const std::string& reference : edges) {
    const std::vector<int> found = body.find_edges(reference);
    if (found.empty()) {
      throw std::invalid_argument(no_edge_error(reference));
    }
    for (int i : found) {
      add_once(result, i);
    }
  }
  if (!faces.empty()) {
    for (int f : resolve_faces(body, faces)) {
      for (TopExp_Explorer it(body.face(f), TopAbs_EDGE); it.More(); it.Next()) {
        const int i = body.edge_index(it.Current());
        if (i >= 0 && !BRep_Tool::Degenerated(body.edge(i))) {
          add_once(result, i);
        }
      }
    }
  }
  return result;
}

gp_Dir normal_on_edge(const TopoDS_Face& face, const TopoDS_Edge& edge, double t) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    throw std::runtime_error("an edge has no curve on its face");
  }
  const gp_Pnt2d uv = pcurve->Value(first + (last - first) * t);
  const BRepAdaptor_Surface surface(face, false);
  BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1.0e-9);
  if (!props.IsNormalDefined()) {
    throw std::runtime_error("a face has no normal at an edge");
  }
  gp_Dir normal = props.Normal();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

double normal_angle(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b,
                    double t) {
  return normal_on_edge(a, edge, t).Angle(normal_on_edge(b, edge, t));
}

bool is_smooth(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b) {
  if (a.IsSame(b) || BRep_Tool::Degenerated(edge)) {
    return false;
  }
  for (const double t : {0.1, 0.5, 0.9}) {
    if (normal_angle(edge, a, b, t) > kSmoothAngle) {
      return false;
    }
  }
  return true;
}

EdgeFaces::EdgeFaces(const Shape& body) : m_body(body) {
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, m_ancestors);
}

std::vector<int> EdgeFaces::at(int edge) const {
  std::vector<int> faces;
  const TopoDS_Edge& e = m_body.edge(edge);
  if (m_ancestors.Contains(e)) {
    for (const TopoDS_Shape& face : m_ancestors.FindFromKey(e)) {
      const int f = m_body.face_index(face);
      if (f >= 0) {
        add_once(faces, f);
      }
    }
  }
  return faces;
}

std::vector<int> faces_at_edge(const Shape& body, int edge) {
  return EdgeFaces(body).at(edge);
}

namespace {

// The unit tangents of the edge at the vertex, pointing away from the edge
// (two for a closed edge, whose ends are both there; none where the curve
// has no direction there).
std::vector<gp_Vec> tangents_away(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  std::vector<gp_Vec> out;
  // (The curve's own parameters, whatever the edge's orientation: its
  // forward vertex, TopExp::FirstVertex, is at the first.)
  const BRepAdaptor_Curve curve(edge);
  for (const bool at_first : {true, false}) {
    const TopoDS_Vertex end = at_first ? TopExp::FirstVertex(edge) : TopExp::LastVertex(edge);
    if (!end.IsSame(vertex)) {
      continue;
    }
    gp_Pnt point;
    gp_Vec d;
    curve.D1(at_first ? curve.FirstParameter() : curve.LastParameter(), point, d);
    if (d.Magnitude() <= gp::Resolution()) {
      continue;
    }
    d.Normalize();
    out.push_back(at_first ? -d : d);
  }
  return out;
}

} // namespace

int follow_tangent_chains(const Shape& body, std::vector<int>& owner, const std::vector<char>& follows) {
  std::vector<int> queue;
  for (int e = 0; e < body.edge_count(); ++e) {
    const int s = owner[static_cast<std::size_t>(e)];
    if (s >= 0 && static_cast<std::size_t>(s) < follows.size() && follows[static_cast<std::size_t>(s)] != 0) {
      queue.push_back(e);
    }
  }
  if (queue.empty()) {
    return 0;
  }
  const EdgeContinuations continuations(body);
  int added = 0;
  while (!queue.empty()) {
    const int e = queue.back();
    queue.pop_back();
    const int s = owner[static_cast<std::size_t>(e)];
    for (const int o : continuations.next(e)) {
      if (owner[static_cast<std::size_t>(o)] < 0) {
        owner[static_cast<std::size_t>(o)] = s;
        queue.push_back(o);
        ++added;
      }
    }
  }
  return added;
}

EdgeContinuations::EdgeContinuations(const Shape& body)
    : m_body(body), m_edgeFaces(body), m_sharp(static_cast<std::size_t>(body.edge_count()), -1),
      m_convexity(static_cast<std::size_t>(body.edge_count()), -1) {
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, m_edgesOf);
}

bool EdgeContinuations::sharp(int edge) const {
  signed char& known = m_sharp[static_cast<std::size_t>(edge)];
  if (known < 0) {
    known = 0;
    const std::vector<int> faces = m_edgeFaces.at(edge);
    if (faces.size() == 2) {
      try {
        OCC_CATCH_SIGNALS
        known = is_smooth(m_body.edge(edge), m_body.face(faces[0]), m_body.face(faces[1])) ? 0 : 1;
      } catch (const Standard_Failure&) {
        known = 0;
      }
    }
  }
  return known == 1;
}

int EdgeContinuations::convexity(int edge) const {
  signed char& known = m_convexity[static_cast<std::size_t>(edge)];
  if (known == -1) {
    known = -2;
    const std::vector<int> faces = m_edgeFaces.at(edge);
    if (faces.size() == 2) {
      try {
        OCC_CATCH_SIGNALS
        // (At the edge's middle, as OCCT types its contours.)
        const ChFiDS_TypeOfConcavity type = ChFi3d::DefineConnectType(
            m_body.edge(edge), m_body.face(faces[0]), m_body.face(faces[1]), 1.0e-4, false);
        known = static_cast<signed char>(type == ChFiDS_Convex ? 1 : type == ChFiDS_Concave ? 0 : -2);
      } catch (const Standard_Failure&) {
        known = -2;
      }
    }
  }
  return known;
}

std::vector<int> EdgeContinuations::next(int e) const {
  std::vector<int> found;
  const TopoDS_Edge& edge = m_body.edge(e);
  if (BRep_Tool::Degenerated(edge)) {
    return found;
  }
  for (const TopoDS_Vertex& vertex : {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)}) {
    const int index = vertex.IsNull() ? 0 : m_edgesOf.FindIndex(vertex);
    if (index == 0) {
      continue;
    }
    try {
      OCC_CATCH_SIGNALS
      const std::vector<gp_Vec> away = tangents_away(edge, vertex);
      for (const TopoDS_Shape& next : m_edgesOf(index)) {
        const int o = m_body.edge_index(next);
        if (o < 0 || o == e || BRep_Tool::Degenerated(TopoDS::Edge(next))) {
          continue;
        }
        // The next edge runs on where its direction away from the
        // vertex is the opposite of this one's.
        bool continues = false;
        for (const gp_Vec& out : tangents_away(TopoDS::Edge(next), vertex)) {
          for (const gp_Vec& from : away) {
            continues = continues || from.Angle(-out) <= kTangentChainAngle;
          }
        }
        // A convex edge does not run on into a concave one or back: the
        // rounding vanishes between them.
        if (continues && sharp(o) &&
            (convexity(e) < 0 || convexity(o) < 0 || convexity(e) == convexity(o))) {
          add_once(found, o);
        } else if (!continues && sharp(o) && convexity(e) >= 0 && convexity(e) == convexity(o) &&
                   across_fading_crease(e, o, vertex)) {
          // Through a crease that fades out (mitcad#1435).
          add_once(found, o);
        }
      }
    } catch (const Standard_Failure&) {
    }
  }
  return found;
}

bool EdgeContinuations::across_fading_crease(int e, int o, const TopoDS_Vertex& vertex) const {
  const std::vector<int> fe = m_edgeFaces.at(e);
  const std::vector<int> fo = m_edgeFaces.at(o);
  if (fe.size() != 2 || fo.size() != 2) {
    return false;
  }
  // The face both lie on (one only) and each one's other face.
  int shared = -1;
  for (const int f : fe) {
    if (std::find(fo.begin(), fo.end(), f) != fo.end()) {
      if (shared >= 0) {
        return false;
      }
      shared = f;
    }
  }
  if (shared < 0) {
    return false;
  }
  const int x = fe[0] == shared ? fe[1] : fe[0];
  const int t = fo[0] == shared ? fo[1] : fo[0];
  if (x == t) {
    return false;
  }
  // The crease between the other faces from the vertex, tangent at its far
  // end.
  const int index = m_edgesOf.FindIndex(vertex);
  if (index == 0) {
    return false;
  }
  for (const TopoDS_Shape& shape : m_edgesOf(index)) {
    const int c = m_body.edge_index(shape);
    if (c < 0 || c == e || c == o || BRep_Tool::Degenerated(TopoDS::Edge(shape))) {
      continue;
    }
    const std::vector<int> fc = m_edgeFaces.at(c);
    if (fc.size() != 2 || !((fc[0] == x && fc[1] == t) || (fc[0] == t && fc[1] == x))) {
      continue;
    }
    const TopoDS_Edge& crease = m_body.edge(c);
    // (Relative parameter 0 at its forward vertex.)
    const double far = TopExp::FirstVertex(crease).IsSame(vertex) ? 1.0 : 0.0;
    if (TopExp::FirstVertex(crease).IsSame(TopExp::LastVertex(crease))) {
      continue;
    }
    if (normal_angle(crease, m_body.face(x), m_body.face(t), far) <= kSmoothAngle) {
      return true;
    }
  }
  return false;
}

std::vector<int> tangent_closure(const Shape& body, std::vector<int> faces) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  return tangent_closure(body, std::move(faces), ancestors);
}

std::vector<int> tangent_closure(
    const Shape& body, std::vector<int> faces,
    const NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>&
        ancestors) {
  for (std::size_t next = 0; next < faces.size(); ++next) {
    const TopoDS_Face& face = body.face(faces[next]);
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (!ancestors.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int f = body.face_index(other);
        if (f >= 0 && std::find(faces.begin(), faces.end(), f) == faces.end() &&
            is_smooth(edge, face, TopoDS::Face(other))) {
          faces.push_back(f);
        }
      }
    }
  }
  return faces;
}

gp_Dir planar_normal(const TopoDS_Face& face, const gp_Pln& plane) {
  gp_Dir normal = plane.Axis().Direction();
  if (plane.Direct() == (face.Orientation() == TopAbs_REVERSED)) {
    normal.Reverse();
  }
  return normal;
}

std::optional<gp_Pln> face_plane(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  const gp_Pln plane = surface.Plane();
  return gp_Pln(gp_Ax3(plane.Location(), planar_normal(face, plane), plane.XAxis().Direction()));
}

std::optional<gp_Pln> reference_plane(const TopoDS_Face& face) {
  if (std::optional<gp_Pln> plane = face_plane(face)) {
    return plane;
  }
  return geometry::face_plane(face);
}

Bnd_Box box_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box, false);
  return box;
}

TopoDS_Face plane_face(const gp_Pln& plane, const Bnd_Box& box) {
  double size = 100.0;
  gp_Pnt centre = plane.Location();
  if (!box.IsVoid()) {
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    const gp_Pnt low(xmin, ymin, zmin);
    const gp_Pnt high(xmax, ymax, zmax);
    size = std::max(1.0, low.Distance(high)) * 2.0;
    // The box's centre projected onto the plane.
    const gp_Pnt middle((xmin + xmax) / 2, (ymin + ymax) / 2, (zmin + zmax) / 2);
    const gp_Vec offset(plane.Location(), middle);
    const gp_Vec normal(plane.Axis().Direction());
    centre = middle.Translated(-normal * offset.Dot(normal));
  }
  const gp_Pln local(gp_Ax3(centre, plane.Axis().Direction(), plane.XAxis().Direction()));
  BRepBuilderAPI_MakeFace maker(local, -size, size, -size, size);
  if (!maker.IsDone()) {
    throw std::runtime_error("the plane could not be built");
  }
  return maker.Face();
}

std::string primary_name(const Shape& body, int face) {
  const NameList& names = body.face_names(face);
  return names.empty() ? std::string() : names.front();
}

} // namespace mitcad::geometry::detail
