// SPDX-License-Identifier: MIT
// Continuity Analysis (mitcad#2521): the gap, the angle between the normals
// and the difference of the normal curvatures across a body's edges,
// sampled along each edge between its two faces (edge_continuity.hpp).
#include "mitcad/geometry/edge_continuity.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <optional>
#include <set>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/input_check.hpp"

namespace mitcad::geometry {
namespace {

// The most samples along an edge.
constexpr int kMostSamples = 1000;
// A tangent or a direction shorter than this has no direction.
constexpr double kTiny = 1e-12;

// What a face says at the edge's parameter: the point, the normal out of
// the material and the normal curvature across the edge.
struct SidePoint {
  gp_Pnt point;
  gp_Dir normal;
};

// The normal curvature of the surface at the props' point along the unit
// `direction` (its part in the tangent plane), signed so that it is
// positive where the surface bends towards `normal` (the props' normal,
// turned with the face); none where the parametrisation is singular there
// or the direction lies mostly off the tangent plane.
std::optional<double> normal_curvature(BRepLProp_SLProps& props, const gp_Vec& direction, bool turned) {
  const gp_Vec su = props.D1U();
  const gp_Vec sv = props.D1V();
  const double e = su.Dot(su);
  const double f = su.Dot(sv);
  const double g = sv.Dot(sv);
  const double det = e * g - f * f;
  if (!(e > kTiny && g > kTiny && det > 1e-9 * e * g)) {
    return std::nullopt;
  }
  // The direction as a su + b sv (its part in the tangent plane).
  const double tu = direction.Dot(su);
  const double tv = direction.Dot(sv);
  const double a = (g * tu - f * tv) / det;
  const double b = (e * tv - f * tu) / det;
  // The squared length of that part: a unit direction in the plane gives
  // 1; one mostly along the normal says nothing of the curvature across.
  const double first = e * a * a + 2.0 * f * a * b + g * b * b;
  if (!(first > 0.25)) {
    return std::nullopt;
  }
  const gp_Vec n(props.Normal());
  const double l = props.D2U().Dot(n);
  const double m = props.DUV().Dot(n);
  const double nn = props.D2V().Dot(n);
  const double k = (l * a * a + 2.0 * m * a * b + nn * b * b) / first;
  return turned ? -k : k;
}

// One face's side of an edge: its surface and the edge's curve on it.
// `turn` turns the face's normal over (the faces of a shell oriented
// against each other at the edge).
class Side {
public:
  Side(const TopoDS_Face& face, const TopoDS_Edge& edge, bool turn)
      : m_surface(face, false), m_curve(edge, face), m_turned((face.Orientation() == TopAbs_REVERSED) != turn) {}

  // The face's point and normal at the edge's parameter t, and its normal
  // curvature along `across` when given; none where the normal is not
  // defined (a pole).
  std::optional<SidePoint> at(double t, const gp_Vec* across, std::optional<double>* curvature) const {
    const gp_Pnt2d uv = m_curve.Value(t);
    BRepLProp_SLProps props(m_surface, uv.X(), uv.Y(), 2, 1e-9);
    if (!props.IsNormalDefined()) {
      return std::nullopt;
    }
    SidePoint side;
    side.point = props.Value();
    side.normal = m_turned ? props.Normal().Reversed() : props.Normal();
    if (across != nullptr && curvature != nullptr) {
      *curvature = normal_curvature(props, *across, m_turned);
    }
    return side;
  }

private:
  BRepAdaptor_Surface m_surface;
  BRepAdaptor_Curve2d m_curve;
  bool m_turned;
};

// The orientation the edge has in the face's boundary (with the face's
// own), the first found; none when it is not on the face or is internal.
std::optional<TopAbs_Orientation> orientation_in(const TopoDS_Face& face, const TopoDS_Edge& edge) {
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    if (it.Current().IsSame(edge)) {
      const TopAbs_Orientation o = it.Current().Orientation();
      if (o == TopAbs_FORWARD || o == TopAbs_REVERSED) {
        return o;
      }
    }
  }
  return std::nullopt;
}

// The curvature a share is taken of where both faces are flatter: that of
// a circle as large as the body.
double flat_curvature(const Shape& body) {
  Bnd_Box box;
  BRepBndLib::Add(body.occt(), box, false);
  const double size = box.IsVoid() ? 0.0 : std::sqrt(box.SquareExtent());
  return size > kTiny ? 1.0 / size : 1.0;
}

// The edge measured: its sides, sampled.
EdgeContinuity measure(const Shape& body, int index, int samples, double flat,
                       const ContinuityTolerances& tolerances) {
  EdgeContinuity found;
  found.edge = index;
  found.name = body.edge_name(index);
  const TopoDS_Edge& edge = body.edge(index);
  const std::array<int, 2> faces = body.edge_faces(index);
  found.sides = faces[0] < 0 || faces[1] < 0 ? EdgeSides::Open
                : faces[0] == faces[1]       ? EdgeSides::Seam
                                             : EdgeSides::Shared;
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  for (int k = 0; k <= samples; ++k) {
    found.points.push_back(curve.Value(first + (last - first) * k / samples));
  }
  if (found.sides == EdgeSides::Open) {
    found.reached = ContinuityClass::Unmeasured;
    return found;
  }
  std::optional<Side> a;
  std::optional<Side> b;
  try {
    if (found.sides == EdgeSides::Seam) {
      // Both sides of a closed face: its two curves on the surface.
      const TopoDS_Face& face = body.face(faces[0]);
      TopoDS_Edge forward = edge;
      forward.Orientation(TopAbs_FORWARD);
      TopoDS_Edge reversed = edge;
      reversed.Orientation(TopAbs_REVERSED);
      a.emplace(face, forward, false);
      b.emplace(face, reversed, false);
    } else {
      const TopoDS_Face& face_a = body.face(faces[0]);
      const TopoDS_Face& face_b = body.face(faces[1]);
      // In a shell whose faces agree, the edge runs one way in one face's
      // boundary and the other way in the other's; where it runs the same
      // way in both, one face's normal is turned to compare them.
      const std::optional<TopAbs_Orientation> in_a = orientation_in(face_a, edge);
      const std::optional<TopAbs_Orientation> in_b = orientation_in(face_b, edge);
      const bool turn = in_a && in_b && *in_a == *in_b;
      a.emplace(face_a, edge, false);
      b.emplace(face_b, edge, turn);
    }
  } catch (const Standard_Failure&) {
    // A face without the edge's curve on it: nothing to compare.
    found.reached = ContinuityClass::Unmeasured;
    return found;
  }
  bool first_sample = true;
  bool first_curvature = true;
  for (int k = 0; k <= samples; ++k) {
    const double t = first + (last - first) * k / samples;
    gp_Pnt point;
    gp_Vec tangent;
    std::optional<SidePoint> on_a;
    std::optional<SidePoint> on_b;
    try {
      curve.D1(t, point, tangent);
      on_a = a->at(t, nullptr, nullptr);
      on_b = b->at(t, nullptr, nullptr);
    } catch (const Standard_Failure&) {
      continue; // not measured at this sample
    }
    if (!on_a || !on_b) {
      continue;
    }
    ++found.samples;
    const double gap = on_a->point.Distance(on_b->point);
    const double angle = on_a->normal.Angle(on_b->normal);
    // The curvatures across are compared where the faces are tangent
    // (within the angle's tolerance): elsewhere the direction across one
    // face is not across the other, and the edge is G0 at best anyway.
    std::optional<double> ka;
    std::optional<double> kb;
    gp_Vec across = gp_Vec(on_a->normal).Crossed(tangent);
    if (angle <= tolerances.angle && across.Magnitude() > kTiny) {
      across.Normalize();
      try {
        a->at(t, &across, &ka);
        b->at(t, &across, &kb);
      } catch (const Standard_Failure&) {
        ka.reset();
        kb.reset();
      }
    }
    if (first_sample || gap > found.gap) {
      found.gap = gap;
      found.gap_at = point;
    }
    if (first_sample || angle > found.angle) {
      found.angle = angle;
      found.angle_at = point;
    }
    first_sample = false;
    if (ka && kb) {
      const double difference = std::abs(*ka - *kb);
      const double share = difference / std::max({std::abs(*ka), std::abs(*kb), flat});
      if (first_curvature || share > found.curvature_share) {
        found.curvature = difference;
        found.curvature_share = share;
        found.curvature_at = point;
      }
      first_curvature = false;
    }
  }
  found.reached = continuity_class(found, tolerances);
  return found;
}

} // namespace

std::vector<int> edges_of_faces(const Shape& body, const std::vector<int>& faces) {
  std::set<int> edges;
  for (const int f : faces) {
    if (f < 0 || f >= body.face_count()) {
      continue;
    }
    for (TopExp_Explorer it(body.face(f), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      const int index = body.edge_index(edge);
      if (index >= 0 && !BRep_Tool::Degenerated(edge)) {
        edges.insert(index);
      }
    }
  }
  return std::vector<int>(edges.begin(), edges.end());
}

ContinuityClass continuity_class(const EdgeContinuity& measured, const ContinuityTolerances& tolerances) {
  if (measured.sides == EdgeSides::Open || measured.samples == 0) {
    return ContinuityClass::Unmeasured;
  }
  if (measured.gap > tolerances.gap) {
    return ContinuityClass::None;
  }
  if (measured.angle > tolerances.angle) {
    return ContinuityClass::G0;
  }
  if (measured.curvature_share > tolerances.curvature) {
    return ContinuityClass::G1;
  }
  return ContinuityClass::G2;
}

std::vector<EdgeContinuity> edge_continuity(const Shape& body, const std::vector<int>& edges, int samples,
                                            const ContinuityTolerances& tolerances) {
  const CheckedOperation check("edge continuity");
  const int count = std::clamp(samples, 1, kMostSamples);
  std::vector<int> wanted = edges;
  if (wanted.empty()) {
    for (int e = 0; e < body.edge_count(); ++e) {
      wanted.push_back(e);
    }
  }
  const double flat = flat_curvature(body);
  std::vector<EdgeContinuity> found;
  std::set<int> seen;
  for (const int e : wanted) {
    if (e < 0 || e >= body.edge_count() || !seen.insert(e).second || BRep_Tool::Degenerated(body.edge(e))) {
      continue;
    }
    try {
      found.push_back(measure(body, e, count, flat, tolerances));
    } catch (const Standard_Failure&) {
      // An edge whose curve cannot be read: listed, not measured.
      EdgeContinuity unread;
      unread.edge = e;
      unread.name = body.edge_name(e);
      found.push_back(unread);
    }
  }
  return found;
}

} // namespace mitcad::geometry
