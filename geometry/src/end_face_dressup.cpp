// SPDX-License-Identifier: MIT
#include "end_face_dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <optional>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRep_Tool.hxx>
#include <ElCLib.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_State.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "mitcad/geometry/cancel.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
// The removed section is sampled on a grid of this many steps a side.
constexpr int kSteps = 12;

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The plane of a planar face with its normal pointing out of `solid` (the
// face's orientation in the solid's shells, not in a face list). None where
// the face is no plane or not in the solid.
std::optional<gp_Pln> outward_plane(const TopoDS_Face& face, const TopoDS_Shape& solid) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  for (TopExp_Explorer ex(solid, TopAbs_FACE); ex.More(); ex.Next()) {
    if (!ex.Current().IsSame(face)) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    gp_Dir normal = plane.Position().XDirection().Crossed(plane.Position().YDirection());
    if (ex.Current().Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    return gp_Pln(plane.Location(), normal);
  }
  return std::nullopt;
}

// How far a face reaches from `from` along `direction` (unit): the largest
// of (p - from) . direction over its boundary, which holds the largest of a
// planar face. Exact on lines and circles; other curves sampled and the
// best sample refined.
double extent_along(const TopoDS_Face& face, const gp_Pnt& from, const gp_Vec& direction) {
  double best = -std::numeric_limits<double>::infinity();
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    const auto value = [&](double u) { return gp_Vec(from, curve.Value(u)).Dot(direction); };
    best = std::max({best, value(first), value(last)});
    if (curve.GetType() == GeomAbs_Line) {
      continue;
    }
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Ax2 frame = curve.Circle().Position();
      const double x = gp_Vec(frame.XDirection()).Dot(direction);
      const double y = gp_Vec(frame.YDirection()).Dot(direction);
      if (std::hypot(x, y) > 0.0) {
        const double u = ElCLib::InPeriod(std::atan2(y, x), first, first + kTwoPi);
        if (u <= last) {
          best = std::max(best, value(u));
        }
      }
      continue;
    }
    constexpr int kSamples = 128;
    int at = 0;
    double most = value(first);
    for (int k = 1; k <= kSamples; ++k) {
      const double v = value(first + (last - first) * k / kSamples);
      if (v > most) {
        most = v;
        at = k;
      }
    }
    // (Golden section search between the best sample's neighbours.)
    double a = first + (last - first) * std::max(0, at - 1) / kSamples;
    double b = first + (last - first) * std::min(kSamples, at + 1) / kSamples;
    constexpr double kGolden = 0.61803398874989484820;
    for (int k = 0; k < 60; ++k) {
      const double c = b - kGolden * (b - a);
      const double d = a + kGolden * (b - a);
      if (value(c) > value(d)) {
        b = d;
      } else {
        a = c;
      }
    }
    best = std::max({best, most, value(0.5 * (a + b))});
  }
  return best;
}

// A chain's end at a vertex of a selected edge: the vertex, the edge's
// direction from it into the edge, and the planar face the edge ends on.
struct End {
  TopoDS_Vertex vertex;
  gp_Pnt at;
  gp_Vec into;
  int face = -1;
  gp_Pln plane;
};

// A selected edge's frame: its direction, the planes of its two faces
// (normals out of the body), the directions square to the edge in each
// face's plane, away from the edge into the face, how far the dressup
// reaches along each, the rounding's radius (0 for a bevel) and the ball's
// centre in the section (from the edge), and its two ends.
struct Frame {
  int edge = -1;
  gp_Vec along;
  int faces[2] = {-1, -1};
  gp_Vec normal[2];
  gp_Vec away[2];
  double contact[2] = {0.0, 0.0};
  double radius = 0.0;
  gp_Vec centre;
  std::array<End, 2> ends;
};

// The offsets from the edge, in its section, of points the dressup takes
// away: inside the bevel's triangle, or inside the triangle between the
// edge and the rounding's contacts and outside its ball.
std::vector<gp_Vec> removed_section(const Frame& frame) {
  std::vector<gp_Vec> out;
  const gp_Vec a = frame.away[0] * frame.contact[0];
  const gp_Vec b = frame.away[1] * frame.contact[1];
  for (int i = 1; i < kSteps; ++i) {
    for (int j = 1; i + j < kSteps; ++j) {
      const gp_Vec o = a * (static_cast<double>(i) / kSteps) + b * (static_cast<double>(j) / kSteps);
      if (frame.radius > 0.0 && (o - frame.centre).Magnitude() <= frame.radius * (1.0 + 1.0e-9)) {
        continue;
      }
      out.push_back(o);
    }
  }
  return out;
}

// Whether every edge of the face that does not run from the vertex is
// tangent to the face beyond it (the face ends in a rounding there).
bool ends_in_roundings(const Shape& body, const Ancestors& faces_of_edges, int face, const TopoDS_Vertex& vertex) {
  const TopoDS_Face& end = body.face(face);
  for (TopExp_Explorer ex(end, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    if (first.IsSame(vertex) || last.IsSame(vertex)) {
      continue;
    }
    if (!faces_of_edges.Contains(edge)) {
      return false;
    }
    bool tangent = false;
    for (const TopoDS_Shape& other : faces_of_edges.FindFromKey(edge)) {
      if (other.IsSame(end)) {
        continue;
      }
      tangent = is_smooth(edge, end, TopoDS::Face(other));
    }
    if (!tangent) {
      return false;
    }
  }
  return true;
}

// The frame of selected edge `e`, or none where it is not a straight convex
// edge between two planes ending at both vertices on one planar face that
// ends in roundings.
std::optional<Frame> frame_of(const Shape& body, int e, const std::vector<int>& owner, const SectionReach& reach,
                              bool round, const Ancestors& edges_at, const Ancestors& faces_at,
                              const Ancestors& faces_of_edges, const EdgeFaces& edge_faces, double tolerance) {
  const TopoDS_Shape& solid = body.occt();
  const TopoDS_Edge& edge = TopoDS::Edge(body.edge(e));
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() != GeomAbs_Line) {
    return std::nullopt;
  }
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(edge, first, last);
  if (first.IsNull() || last.IsNull() || first.IsSame(last)) {
    return std::nullopt;
  }
  const std::vector<int> own = edge_faces.at(e);
  if (own.size() != 2 || own[0] == own[1]) {
    return std::nullopt;
  }
  Frame frame;
  frame.edge = e;
  const gp_Pnt a = BRep_Tool::Pnt(first);
  const gp_Pnt b = BRep_Tool::Pnt(last);
  if (a.Distance(b) <= 10.0 * tolerance) {
    return std::nullopt;
  }
  frame.along = gp_Vec(a, b).Normalized();
  for (int k = 0; k < 2; ++k) {
    const std::optional<gp_Pln> plane = outward_plane(body.face(own[static_cast<std::size_t>(k)]), solid);
    if (!plane) {
      return std::nullopt;
    }
    frame.faces[k] = own[static_cast<std::size_t>(k)];
    frame.normal[k] = gp_Vec(plane->Axis().Direction());
  }
  // (Square to the edge in each face's plane, on the inner side of the
  // other face's plane: into the face where the edge is convex.)
  const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
  for (int k = 0; k < 2; ++k) {
    gp_Vec away = frame.along.Crossed(frame.normal[k]);
    if (away.Magnitude() < 1.0e-9) {
      return std::nullopt;
    }
    away.Normalize();
    const double side = away.Dot(frame.normal[1 - k]);
    if (std::abs(side) < 1.0e-6) {
      return std::nullopt; // the faces tangent or folded flat
    }
    if (side > 0.0) {
      away.Reverse();
    }
    frame.away[k] = away;
  }
  for (int k = 0; k < 2; ++k) {
    const double size = reach(e, frame.faces[k]);
    if (!(size > 0.0)) {
      return std::nullopt;
    }
    if (round) {
      // (r / tan(b / 2) from the edge, b the angle inside between the
      // faces.)
      const double inside = std::acos(std::clamp(frame.away[0].Dot(frame.away[1]), -1.0, 1.0));
      if (!(inside > 1.0e-6) || !(inside < kTwoPi / 2.0 - 1.0e-6)) {
        return std::nullopt;
      }
      frame.radius = size;
      frame.contact[k] = size / std::tan(inside / 2.0);
    } else {
      frame.contact[k] = size;
    }
  }
  // The edge convex: each face lies on the side the direction away points
  // to, next to the edge's middle.
  for (int k = 0; k < 2; ++k) {
    const gp_Pnt beside = middle.Translated(frame.away[k] * (1.0e-3 * frame.contact[k] + 10.0 * tolerance));
    const BRepClass_FaceClassifier classifier(body.face(frame.faces[k]), beside, tolerance);
    if (classifier.State() != TopAbs_IN) {
      return std::nullopt;
    }
  }
  if (round) {
    frame.centre = frame.away[0] * frame.contact[0] - frame.normal[0] * frame.radius;
  }
  // The ends: one selected edge at the vertex, one face there beside the
  // edge's own, a plane the edge leaves into the body, ending in roundings.
  for (int k = 0; k < 2; ++k) {
    End& end = frame.ends[static_cast<std::size_t>(k)];
    end.vertex = k == 0 ? first : last;
    end.at = k == 0 ? a : b;
    end.into = k == 0 ? frame.along : -frame.along;
    if (!edges_at.Contains(end.vertex) || !faces_at.Contains(end.vertex)) {
      return std::nullopt;
    }
    int chained = 0;
    for (const TopoDS_Shape& other : edges_at.FindFromKey(end.vertex)) {
      const int i = body.edge_index(other);
      chained += i >= 0 && i < static_cast<int>(owner.size()) && owner[static_cast<std::size_t>(i)] >= 0 ? 1 : 0;
    }
    if (chained != 1) {
      return std::nullopt;
    }
    for (const TopoDS_Shape& face : faces_at.FindFromKey(end.vertex)) {
      const int f = body.face_index(face);
      if (f < 0 || f == own[0] || f == own[1] || f == end.face) {
        continue;
      }
      if (end.face >= 0) {
        return std::nullopt; // more than one face at the end
      }
      end.face = f;
    }
    if (end.face < 0) {
      return std::nullopt;
    }
    const std::optional<gp_Pln> plane = outward_plane(body.face(end.face), solid);
    if (!plane || gp_Vec(plane->Axis().Direction()).Dot(end.into) > -0.1) {
      return std::nullopt;
    }
    end.plane = *plane;
    if (!ends_in_roundings(body, faces_of_edges, end.face, end.vertex)) {
      return std::nullopt;
    }
  }
  return frame;
}

// Whether the dressup's section at the end runs out of the end face (the
// section's points projected along the edge onto the face's plane), with no
// material of the body past the plane there; none where the body has
// material past the plane (another case: ended_on_end_planes in
// dressup.cpp).
std::optional<bool> runs_out(const Shape& body, const Frame& frame, const End& end, double tolerance) {
  const gp_Vec normal(end.plane.Axis().Direction());
  const double slope = normal.Dot(frame.along);
  const double past = 1.0e-3 * std::max(frame.contact[0], frame.contact[1]) + 10.0 * tolerance;
  bool out = false;
  for (const gp_Vec& offset : removed_section(frame)) {
    const gp_Pnt p = end.at.Translated(offset);
    const double t = -normal.Dot(offset) / slope;
    const gp_Pnt on_plane = p.Translated(frame.along * t);
    const BRepClass_FaceClassifier classifier(body.face(end.face), on_plane, tolerance);
    if (classifier.State() == TopAbs_OUT) {
      out = true;
    }
    const BRepClass3d_SolidClassifier beyond(body.occt(), on_plane.Translated(-end.into * past), tolerance);
    if (beyond.State() == TopAbs_IN) {
      return std::nullopt;
    }
  }
  return out;
}

} // namespace

std::optional<WiderThanBody> wider_than_the_body(const Shape& input, const std::vector<int>& owner,
                                                 const SectionReach& reach, bool round) {
  try {
    OCC_CATCH_SIGNALS
    const TopoDS_Shape& solid = input.occt();
    const double tolerance = std::max(1.0e-6, 2.0 * BRep_Tool::MaxTolerance(solid, TopAbs_VERTEX));
    Ancestors edges_at;
    Ancestors faces_at;
    Ancestors faces_of_edges;
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_EDGE, edges_at);
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_FACE, faces_at);
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_EDGE, TopAbs_FACE, faces_of_edges);
    const EdgeFaces edge_faces(input);
    std::vector<Frame> frames;
    for (int e = 0; e < input.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      std::optional<Frame> frame =
          frame_of(input, e, owner, reach, round, edges_at, faces_at, faces_of_edges, edge_faces, tolerance);
      if (!frame) {
        return std::nullopt;
      }
      frames.push_back(*frame);
    }
    if (frames.empty()) {
      return std::nullopt;
    }
    bool any = false;
    for (const Frame& frame : frames) {
      for (const End& end : frame.ends) {
        throw_if_cancelled();
        const std::optional<bool> out = runs_out(input, frame, end, tolerance);
        if (!out) {
          return std::nullopt;
        }
        any = any || *out;
      }
    }
    if (!any) {
      return std::nullopt;
    }
    // Where the dressup's contact lies past both faces beside an edge, it
    // touches neither: no bevel or rounding of that size exists there.
    for (const Frame& frame : frames) {
      double share[2] = {0.0, 0.0};
      for (int k = 0; k < 2; ++k) {
        share[k] = extent_along(input.face(frame.faces[k]), frame.ends[0].at, frame.away[k]) / frame.contact[k];
      }
      const double slack = 1.0e-6 + tolerance / std::max(frame.contact[0], frame.contact[1]);
      if (share[0] < 1.0 - slack && share[1] < 1.0 - slack) {
        WiderThanBody wider;
        wider.edge = frame.edge;
        wider.fits = std::max(0.0, std::min(share[0], share[1]));
        return wider;
      }
    }
    return std::nullopt;
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_Failure&) {
    return std::nullopt;
  } catch (const std::runtime_error&) {
    return std::nullopt;
  }
}

} // namespace mitcad::geometry::detail
