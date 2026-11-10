// SPDX-License-Identifier: MIT
#include "mitcad/geometry/face_revolve.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "history.hpp"
#include "mitcad/geometry/naming.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

// Throws when the face crosses the axis (points on both sides of it in the
// face's plane), lies on it, or the axis passes through the face.
void check_axis(const TopoDS_Face& face, const gp_Pln& plane, const gp_Ax1& axis, const std::string& name) {
  const gp_Dir normal = plane.Axis().Direction();
  const gp_Dir direction = axis.Direction();
  const double tolerance = 1.0e-6;
  if (std::abs(direction.Dot(normal)) < 1.0e-9) {
    // Parallel to the face's plane: every point on one side of the axis.
    const gp_Vec across = gp_Vec(direction).Crossed(gp_Vec(normal));
    double low = std::numeric_limits<double>::infinity();
    double high = -std::numeric_limits<double>::infinity();
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      for (int i = 0; i <= 16; ++i) {
        const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 16.0;
        const double side = gp_Vec(axis.Location(), curve.Value(t)).Dot(across);
        low = std::min(low, side);
        high = std::max(high, side);
      }
    }
    if (low < -tolerance && high > tolerance) {
      throw std::invalid_argument(
          face_error(name, "the face crosses the revolve axis", "face " + name + " crosses the revolve axis"));
    }
    if (high - low <= tolerance) {
      throw std::invalid_argument(
          face_error(name, "the face lies on the revolve axis", "face " + name + " lies on the revolve axis"));
    }
    return;
  }
  // The axis pierces the face's plane: not inside the face.
  const gp_Vec to_plane(axis.Location(), plane.Location());
  const double t = to_plane.Dot(gp_Vec(normal)) / direction.Dot(normal);
  const gp_Pnt pierce = axis.Location().Translated(gp_Vec(direction) * t);
  BRepClass_FaceClassifier classifier(face, pierce, tolerance);
  if (classifier.State() == TopAbs_IN) {
    throw std::invalid_argument(face_error(name, "the revolve axis passes through the face",
                                           "the revolve axis passes through face " + name));
  }
}

// The face turned by `angle` about the axis, its side faces named after the
// edges, the cap at the face `near` and the far one `far`.
ShapePtr named_revolution(const detail::SweepFace& face, const gp_Ax1& axis, double angle, const NameList& near,
                          const NameList& far) {
  BRepPrimAPI_MakeRevol maker(face.face, axis, angle);
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error(face_error(face.regions.front(), "the revolution of the face failed",
                                        "the revolution of face " + face.regions.front() + " failed"));
  }
  detail::FaceNamer namer(maker.Shape());
  for (const auto& [edge, name] : face.edges) {
    namer.generated(maker, edge, name);
  }
  for (const std::string& name : near) {
    namer.add(maker.FirstShape(), name);
  }
  for (const std::string& name : far) {
    namer.add(maker.LastShape(), name);
  }
  return detail::with_swept_names(*namer.shape(), face.edges, axis, angle);
}

double signed_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

} // namespace

ShapePtr revolve_face(const Shape& body, const FaceRevolveSpec& spec) {
  detail::require_positive("revolve angle", spec.angle1);
  if (spec.angle2) {
    detail::require_finite("second revolve angle", *spec.angle2);
    if (*spec.angle2 < 0.0) {
      throw std::invalid_argument("the second revolve angle must not be negative");
    }
  }
  const double total = spec.angle1 + spec.angle2.value_or(0.0);
  if (total > kTwoPi + 1.0e-9) {
    throw std::invalid_argument("the revolve angles add up to more than a full turn");
  }
  return detail::run("revolve face", [&] {
    const std::vector<int> found = body.find_faces(spec.face);
    if (found.empty()) {
      throw std::invalid_argument(no_face_error(spec.face));
    }
    if (found.size() > 1) {
      throw std::invalid_argument(face_error(
          spec.face, "the face is in " + std::to_string(found.size()) + " pieces; a revolution takes one face",
          spec.face + " names " + std::to_string(found.size()) + " faces; a revolution takes one face"));
    }
    const TopoDS_Face& face = body.face(found.front());
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      const std::string why = " has no surface (a mesh body's triangles): a revolution takes a planar face";
      throw std::invalid_argument(face_error(spec.face, "the face" + why, "face " + spec.face + why));
    }
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      throw std::invalid_argument(face_error(spec.face, "the face is not planar: a revolution takes a planar face",
                                             "face " + spec.face + " is not planar: a revolution takes a planar face"));
    }
    check_axis(face, surface.Plane(), spec.axis, spec.face);

    // The face copied, so that the solid shares nothing with the body, its
    // edges named after the body's.
    const detail::InputCopy copy(face);
    detail::SweepFace sweep{TopoDS::Face(copy.shape()), {}, {spec.key}};
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    TopExp::MapShapes(face, TopAbs_EDGE, edges);
    for (int i = 1; i <= edges.Extent(); ++i) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
      const std::optional<std::string> name = body.name_of_edge(edge);
      const std::string key = name ? *name : std::to_string(i - 1);
      sweep.edges.emplace_back(TopoDS::Edge(copy.of(edge)), face_name(spec.feature, "side", key));
    }

    // From the end of side two round to the end of side one, as a
    // revolution of sketch profiles turns.
    const bool two = spec.angle2.has_value();
    const bool full = total >= kTwoPi - 1.0e-9;
    const NameList start{face_name(spec.feature, "start", spec.key)};
    const NameList end{face_name(spec.feature, "end", spec.key)};
    const auto turned = [&](const detail::SweepFace& profile) {
      if (full) {
        return named_revolution(profile, spec.axis, kTwoPi, start, end);
      }
      gp_Trsf back;
      back.SetRotation(spec.axis, -spec.angle2.value_or(0.0));
      return named_revolution(two ? detail::transformed(profile, back) : profile, spec.axis, total,
                              two ? end : start, two ? start : end);
    };
    ShapePtr solid = turned(sweep);
    // A face looking against the turn sweeps an inside-out solid: the
    // other side of the face then.
    if (signed_volume(solid->occt()) < 0.0) {
      sweep.face = TopoDS::Face(sweep.face.Reversed());
      solid = turned(sweep);
    }
    return detail::finished(*solid, "the revolution of the face");
  });
}

} // namespace mitcad::geometry
