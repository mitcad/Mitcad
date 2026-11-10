// SPDX-License-Identifier: MIT
#include "mitcad/geometry/revolve.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Lin.hxx>
#include <TopoDS.hxx>

#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
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
        throw std::invalid_argument(placed_error("a profile crosses the revolve axis",
                                                 "profile " + face.regions.front() + " crosses the revolve axis"));
      }
      if (high - low <= tolerance) {
        throw std::invalid_argument(placed_error("a profile lies on the revolve axis",
                                                 "profile " + face.regions.front() + " lies on the revolve axis"));
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
      throw std::invalid_argument(placed_error("the revolve axis passes through a profile",
                                               "the revolve axis passes through profile " + face.regions.front()));
    }
  }
}

// The direction a planar face looks in (its normal with the face's orientation).
gp_Dir looking(const TopoDS_Face& face) {
  gp_Dir normal = BRepAdaptor_Surface(face).Plane().Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

// A profile with points on both sides of the axis line (a region that straddles
// it: imported revolutions have them) in the parts on each side, which are
// swept apart: a face that crosses the axis would sweep through itself. Only for
// a turn of less than half (`total`): the sides then sweep different halves of
// the space; for more they overlap and the profile stays an error. The edges
// of the parts keep the names of the edges they come from.
std::vector<detail::SweepFace> split_across_axis(const std::vector<detail::SweepFace>& input,
                                                 const Frame& frame, const gp_Ax1& axis,
                                                 double total) {
  const gp_Dir normal = frame.normal();
  if (total >= kPi - 1.0e-9 || std::abs(axis.Direction().Dot(normal)) >= 1.0e-9) {
    return input;
  }
  const gp_Vec across = gp_Vec(axis.Direction()).Crossed(gp_Vec(normal));
  const double tolerance = 1.0e-6;
  const auto straddles = [&](const detail::SweepFace& face) {
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
    return low < -tolerance && high > tolerance;
  };
  if (!std::any_of(input.begin(), input.end(), straddles)) {
    return input;
  }
  // (Faces that touch are one face already: their solids, fused where they
  // share a surface, can lose each other.)
  std::vector<detail::SweepFace> result;
  for (const detail::SweepFace& face : input) {
    if (!straddles(face)) {
      result.push_back(face);
      continue;
    }
    // The axis line in the plane of the profile, past its ends.
    Bnd_Box box;
    BRepBndLib::Add(face.face, box);
    const double size = std::sqrt(box.SquareExtent()) + 1.0;
    const gp_Pnt on_plane = axis.Location().Translated(
        gp_Vec(normal) * -gp_Vec(frame.origin, axis.Location()).Dot(gp_Vec(normal)));
    const gp_Pnt centre = box.IsVoid() ? on_plane : gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
    const double at = gp_Vec(on_plane, centre).Dot(gp_Vec(axis.Direction()));
    const gp_Pnt from = on_plane.Translated(gp_Vec(axis.Direction()) * (at - 2.0 * size));
    const TopoDS_Edge line = BRepBuilderAPI_MakeEdge(gp_Lin(from, axis.Direction()), 0.0, 4.0 * size).Edge();
    BRepAlgoAPI_Splitter splitter;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(face.face);
    NCollection_List<TopoDS_Shape> tools;
    tools.Append(line);
    splitter.SetArguments(arguments);
    splitter.SetTools(tools);
    splitter.SetNonDestructive(true);
    detail::build(splitter);
    if (!splitter.IsDone() || splitter.HasErrors()) {
      throw std::runtime_error(placed_error("a profile could not be parted at the revolve axis",
                                            "profile " + face.regions.front() +
                                                " could not be parted at the revolve axis"));
    }
    std::vector<std::pair<TopoDS_Edge, std::string>> named;
    for (const auto& [edge, name] : face.edges) {
      NCollection_List<TopoDS_Shape> images = splitter.Modified(edge);
      if (images.IsEmpty() && !splitter.IsDeleted(edge)) {
        images.Append(edge);
      }
      for (const TopoDS_Shape& image : images) {
        named.emplace_back(TopoDS::Edge(image), name);
      }
    }
    for (TopExp_Explorer f(splitter.Shape(), TopAbs_FACE); f.More(); f.Next()) {
      detail::SweepFace part{TopoDS::Face(f.Current()), {}, face.regions};
      // A part looks the way the profile does (a swept face looking the other way
      // is an inside-out solid, which fuses with its neighbours wrongly).
      if (looking(part.face).Dot(looking(face.face)) < 0.0) {
        part.face = TopoDS::Face(part.face.Reversed());
      }
      for (TopExp_Explorer e(part.face, TopAbs_EDGE); e.More(); e.Next()) {
        for (const auto& [image, name] : named) {
          if (image.IsSame(e.Current())) {
            part.edges.emplace_back(TopoDS::Edge(e.Current()), name);
          }
        }
      }
      result.push_back(std::move(part));
    }
  }
  return result;
}

ShapePtr named_revolution(const detail::SweepFace& face, const gp_Ax1& axis, double angle,
                          const NameList& near, const NameList& far) {
  BRepPrimAPI_MakeRevol maker(face.face, axis, angle);
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error(
        placed_error("the revolution of a profile failed", "the revolution of " + face.regions.front() + " failed"));
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
    if (spec.angle2 || spec.target->offset != 0.0) {
      throw std::invalid_argument("a revolution up to an object has one side and no offset");
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
    // Regions that touch are one face first. Revolved apart and fused, two
    // parts of one surface (a cone a shared edge cut in two) are merged
    // back into one face by the fuse's simplification, which built that
    // face wrong: the solid came out 2.6 % too big. One face revolves
    // exactly, and no shared face is left for the fuse to merge.
    std::vector<detail::SweepFace> faces = detail::region_faces(spec.feature, spec.frame, spec.regions);
    if (faces.size() > 1) {
      faces = detail::merged(faces);
    }
    faces = split_across_axis(faces, spec.frame, spec.axis, total);
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
        // (Through a body to its last face, mitcad#539: its pieces in the
        // order of the turn.)
        parts.push_back(detail::trimmed(*sweep, *spec.target, tool, spec.axis.Direction(),
                                        start.front(), end, unreached, false, &spec.axis));
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
