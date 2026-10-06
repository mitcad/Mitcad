// SPDX-License-Identifier: MIT
#include "mitcad/geometry/revolve.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
// A target sweep turns this much short of a full turn, so that it still has
// a far cap to check the target against.
constexpr double kAlmostFull = kTwoPi - 1.0e-3;

// Throws when a profile crosses the axis (points on both sides of it), or
// the axis passes through a profile.
void check_axis(const std::vector<detail::SweepFace>& faces, const Frame& frame, const gp_Ax1& axis) {
  const gp_Dir normal = frame.normal();
  const gp_Dir direction = axis.Direction();
  const double tolerance = 1.0e-6;
  if (std::abs(direction.Dot(normal)) < 1.0e-9) {
    // Parallel to the sketch plane: every point on one side of the axis.
    const gp_Vec across = gp_Vec(direction).Crossed(gp_Vec(normal));
    for (const detail::SweepFace& face : faces) {
      double low = std::numeric_limits<double>::infinity();
      double high = -std::numeric_limits<double>::infinity();
      for (TopExp_Explorer e(face.face, TopAbs_EDGE); e.More(); e.Next()) {
        const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
        for (int i = 0; i <= 16; ++i) {
          const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 16.0;
          const double side = gp_Vec(axis.Location(), curve.Value(t)).Dot(across);
          low = std::min(low, side);
          high = std::max(high, side);
        }
      }
      if (low < -tolerance && high > tolerance) {
        throw std::invalid_argument("profile " + face.regions.front() + " crosses the revolve axis");
      }
      if (high - low <= tolerance) {
        throw std::invalid_argument("profile " + face.regions.front() + " lies on the revolve axis");
      }
    }
    return;
  }
  // The axis pierces the sketch plane: not inside a profile.
  const gp_Vec to_plane(axis.Location(), frame.origin);
  const double t = to_plane.Dot(gp_Vec(normal)) / direction.Dot(normal);
  const gp_Pnt pierce = axis.Location().Translated(gp_Vec(direction) * t);
  for (const detail::SweepFace& face : faces) {
    BRepClass_FaceClassifier classifier(face.face, pierce, tolerance);
    if (classifier.State() == TopAbs_IN) {
      throw std::invalid_argument("the revolve axis passes through profile " + face.regions.front());
    }
  }
}

ShapePtr named_revolution(const detail::SweepFace& face, const gp_Ax1& axis, double angle,
                          const NameList& near, const NameList& far) {
  BRepPrimAPI_MakeRevol maker(face.face, axis, angle);
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error("the revolution of " + face.regions.front() + " failed");
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

} // namespace

ShapePtr revolve(const RevolveSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to revolve");
  }
  if (spec.target) {
    if (spec.angle2 || spec.target->offset != 0.0 || spec.target->through) {
      throw std::invalid_argument(
          "a revolution up to an object has one side, no offset and stops at the first face");
    }
  } else {
    detail::require_positive("revolve angle", spec.angle1);
  }
  if (spec.angle2) {
    detail::require_finite("second revolve angle", *spec.angle2);
    if (*spec.angle2 < 0.0) {
      throw std::invalid_argument("the second revolve angle must not be negative");
    }
  }
  const double total = spec.target ? kAlmostFull : spec.angle1 + spec.angle2.value_or(0.0);
  if (total > kTwoPi + 1.0e-9) {
    throw std::invalid_argument("the revolve angles add up to more than a full turn");
  }
  return detail::run("revolve", [&] {
    const std::vector<detail::SweepFace> faces = detail::region_faces(spec.feature, spec.frame, spec.regions);
    check_axis(faces, spec.frame, spec.axis);
    const bool two = spec.angle2.has_value();
    const bool full = !spec.target && total >= kTwoPi - 1.0e-9;
    gp_Trsf back;
    back.SetRotation(spec.axis, -spec.angle2.value_or(0.0));
    std::vector<ShapePtr> parts;
    for (const detail::SweepFace& face : faces) {
      const NameList start = detail::cap_names(spec.feature, "start", face);
      const NameList end = detail::cap_names(spec.feature, "end", face);
      if (spec.target) {
        const std::string unreached = face_name(spec.feature, "unreached", face.regions.front());
        const ShapePtr sweep = named_revolution(face, spec.axis, total, start, {unreached});
        const TopoDS_Shape tool =
            detail::target_tool(*spec.target, spec.axis.Direction(), detail::bounds_of(sweep->occt()));
        parts.push_back(detail::trimmed(*sweep, *spec.target, tool, spec.axis.Direction(),
                                        start.front(), end, unreached));
      } else if (full) {
        parts.push_back(named_revolution(face, spec.axis, kTwoPi, start, end));
      } else {
        // From the end of side two round to the end of side one.
        parts.push_back(named_revolution(two ? detail::transformed(face, back) : face, spec.axis,
                                         total, two ? end : start, two ? start : end));
      }
    }
    return detail::finished(*detail::fused(parts, "the revolution"), "the revolution");
  });
}

} // namespace mitcad::geometry
