// SPDX-License-Identifier: MIT
#include "mitcad/geometry/extrude.hpp"

#include <cmath>
#include <limits>
#include <stdexcept>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <gp_Vec.hxx>

#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

ShapePtr prism(const ExtrudeSpec& spec, const Frame& frame, const Region& region) {
  const ProfileFace profile = make_profile(frame, region);
  BRepPrimAPI_MakePrism maker(profile.face, gp_Vec(spec.direction) * (spec.end - spec.start));
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error("the extrusion of " + region.name + " failed");
  }
  detail::FaceNamer namer(maker.Shape());
  for (const auto& [edge, segment] : profile.edges) {
    namer.generated(maker, edge, face_name(spec.feature, "side", segment));
  }
  namer.add(maker.FirstShape(), face_name(spec.feature, "start", region.name));
  namer.add(maker.LastShape(), face_name(spec.feature, "end", region.name));
  namer.finish();
  return namer.shape();
}

} // namespace

namespace {

// Below 90 degrees by a margin; steeper tapers fold the sides over. 85
// degrees itself is allowed (radians, with rounding).
constexpr double kMaxTaper = 85.0 * 3.14159265358979323846 / 180.0 + 1.0e-9;

void check_side(const ExtrudeSide& side) {
  if (!side.target) {
    detail::require_positive("extrude distance", side.distance);
  }
  detail::require_finite("taper angle", side.taper);
  if (std::abs(side.taper) > kMaxTaper) {
    throw std::invalid_argument("the taper angle must be between -85 and 85 degrees");
  }
}

bool same_wall(const std::optional<ThinWall>& a, const std::optional<ThinWall>& b) {
  if (a.has_value() != b.has_value()) {
    return false;
  }
  return !a || (a->location == b->location && a->thickness == b->thickness);
}


std::vector<detail::SweepFace> profile_faces(const ExtrudeFeatureSpec& spec, const ExtrudeSide& side,
                                             bool merge) {
  if (side.thin) {
    return detail::wall_faces(spec.feature, spec.frame, spec.regions, *side.thin);
  }
  std::vector<detail::SweepFace> faces = detail::region_faces(spec.feature, spec.frame, spec.regions);
  return merge ? detail::merged(faces) : faces;
}

// One side swept from a face on a start plane parallel to the sketch.
ShapePtr parallel_side(const std::string& feature, const detail::SweepFace& face,
                       const ExtrudeSide& side, const gp_Dir& along, const NameList& near,
                       const NameList& far) {
  const gp_Pln neutral = BRepAdaptor_Surface(face.face).Plane();
  if (!side.target) {
    const ShapePtr solid = detail::named_prism(face, gp_Vec(along) * side.distance, near, far);
    if (side.taper == 0.0) {
      return solid;
    }
    NameList caps = near;
    caps.insert(caps.end(), far.begin(), far.end());
    return detail::tapered(*solid, neutral, along, side.taper, caps);
  }
  // Past the target, then back to where it is reached.
  const std::string unreached = face_name(feature, "unreached", face.regions.front());
  const double length = detail::reach(*side.target, along, detail::bounds_of(face.face));
  ShapePtr solid = detail::named_prism(face, gp_Vec(along) * length, near, {unreached});
  if (side.taper != 0.0) {
    NameList caps = near;
    caps.push_back(unreached);
    solid = detail::tapered(*solid, neutral, along, side.taper, caps);
  }
  const TopoDS_Shape tool = detail::target_tool(*side.target, along, detail::bounds_of(solid->occt()));
  return detail::trimmed(*solid, *side.target, tool, along, near.front(), far, unreached);
}

// One side swept from a start plane that is not parallel to the sketch: the
// profile's prism through that plane, cut off behind it.
ShapePtr oblique_side(const std::string& feature, const detail::SweepFace& face, const gp_Pln& start,
                      const ExtrudeSide& side, const gp_Dir& along, const NameList& near,
                      const NameList& far) {
  const gp_Dir normal = start.Axis().Direction();
  const double slope = normal.Dot(along);
  if (std::abs(slope) < 1.0e-6) {
    throw std::invalid_argument("the start plane is parallel to the extrusion direction");
  }
  // Where the plane lies along the sweep over the profile.
  double first = std::numeric_limits<double>::infinity();
  double last = -std::numeric_limits<double>::infinity();
  const Bnd_Box box = detail::bounds_of(face.face);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  for (int i = 0; i < 8; ++i) {
    const gp_Pnt corner(i & 1 ? x1 : x0, i & 2 ? y1 : y0, i & 4 ? z1 : z0);
    const double t = gp_Vec(corner, start.Location()).Dot(gp_Vec(normal)) / slope;
    first = std::min(first, t);
    last = std::max(last, t);
  }
  Bnd_Box at_start = box;
  at_start.Enlarge(last - first);
  const double extent =
      side.target ? detail::reach(*side.target, along, at_start) + (last - first) : side.distance;
  const double back = first - 1.0;
  const double length = (last + extent + 1.0) - back;

  const std::string behind = face_name(feature, "behind", face.regions.front());
  const std::string unreached = face_name(feature, "unreached", face.regions.front());
  gp_Trsf shift;
  shift.SetTranslation(gp_Vec(along) * back);
  const ShapePtr prism = detail::named_prism(detail::transformed(face, shift), gp_Vec(along) * length,
                                             {behind}, {unreached});
  const Bnd_Box around = detail::bounds_of(prism->occt());
  const ShapePtr past = detail::split_keep(
      *prism, detail::plane_tool(start, around), near,
      [&behind](const Shape& piece) { return !detail::has_face_named(piece, behind); },
      "the extrusion from its start plane");
  if (side.target) {
    const TopoDS_Shape tool = detail::target_tool(*side.target, along, around);
    return detail::trimmed(*past, *side.target, tool, along, near.front(), far, unreached);
  }
  // The far cap is the start plane moved by the distance.
  Target end;
  end.plane = gp_Pln(start.Location().Translated(gp_Vec(along) * side.distance), normal);
  return detail::trimmed(*past, end, detail::plane_tool(end.plane, around), along, near.front(), far,
                         unreached);
}

} // namespace

ShapePtr extrude_feature(const ExtrudeFeatureSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to extrude");
  }
  check_side(spec.side1);
  const bool two = spec.side2.has_value();
  if (two) {
    check_side(*spec.side2);
    if (spec.side1.thin.has_value() != spec.side2->thin.has_value()) {
      throw std::invalid_argument("both sides of a thin extrusion need a wall");
    }
  }
  detail::require_finite("start offset", spec.start_offset);
  const gp_Dir normal = spec.frame.normal();
  if (std::abs(spec.direction.Dot(normal)) < 1.0e-6) {
    throw std::invalid_argument("the extrusion direction lies in the sketch plane");
  }
  return detail::run("extrude", [&] {
    // The start: a plane parallel to the sketch, or an oblique plane.
    double offset = spec.start_offset;
    std::optional<gp_Pln> oblique;
    if (spec.start) {
      if (spec.start->kind == Target::Kind::Body) {
        throw std::invalid_argument("an extrusion cannot start from a body");
      }
      const gp_Pln plane = detail::target_plane(*spec.start, normal);
      if (plane.Axis().Direction().IsParallel(normal, 1.0e-9)) {
        offset = gp_Vec(spec.frame.origin, plane.Location()).Dot(gp_Vec(normal));
      } else {
        oblique = plane;
      }
    }
    const bool taper = spec.side1.taper != 0.0 || (two && spec.side2->taper != 0.0);
    if (oblique && taper) {
      throw std::invalid_argument("a tapered extrusion needs a start plane parallel to the sketch");
    }
    gp_Trsf to_start;
    to_start.SetTranslation(gp_Vec(normal) * offset);

    std::vector<ShapePtr> parts;
    const bool one_prism = !oblique && !taper && !spec.side1.target &&
                           (!two || (!spec.side2->target && same_wall(spec.side1.thin, spec.side2->thin)));
    if (one_prism) {
      // Both sides in one prism; with two sides the far cap of side one is
      // the start (`startFaces` in .f3d) and the far cap of side two the end.
      const double low = two ? -spec.side2->distance : 0.0;
      const double length = spec.side1.distance - low;
      if (!(length > 1.0e-7)) {
        throw std::invalid_argument("the extrusion has no length");
      }
      gp_Trsf place;
      place.SetTranslation(gp_Vec(normal) * offset + gp_Vec(spec.direction) * low);
      for (const detail::SweepFace& face : profile_faces(spec, spec.side1, false)) {
        const NameList start = detail::cap_names(spec.feature, "start", face);
        const NameList end = detail::cap_names(spec.feature, "end", face);
        parts.push_back(detail::named_prism(detail::transformed(face, place),
                                            gp_Vec(spec.direction) * length, two ? end : start,
                                            two ? start : end));
      }
    } else {
      for (int k = 0; k < (two ? 2 : 1); ++k) {
        const ExtrudeSide& side = k == 0 ? spec.side1 : *spec.side2;
        const gp_Dir along = k == 0 ? spec.direction : spec.direction.Reversed();
        // Touching profiles taper as one.
        for (const detail::SweepFace& face : profile_faces(spec, side, taper)) {
          const NameList near = detail::cap_names(spec.feature, two ? "mid" : "start", face);
          const NameList far = detail::cap_names(spec.feature, k == 0 && two ? "start" : "end", face);
          parts.push_back(oblique ? oblique_side(spec.feature, face, *oblique, side, along, near, far)
                                  : parallel_side(spec.feature, detail::transformed(face, to_start),
                                                  side, along, near, far));
        }
      }
    }
    return detail::finished(*detail::fused(parts, "the extrusion"), "the extrusion");
  });
}

ShapePtr extrude(const ExtrudeSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to extrude");
  }
  detail::require_finite("extrude start", spec.start);
  detail::require_finite("extrude end", spec.end);
  if (!(spec.end - spec.start > 1.0e-7)) {
    throw std::invalid_argument("the extrusion has no length");
  }
  if (std::abs(spec.direction.Dot(spec.frame.normal())) < 1.0e-6) {
    throw std::invalid_argument("the extrusion direction lies in the sketch plane");
  }
  return detail::run("extrude", [&] {
    Frame frame = spec.frame;
    frame.origin.Translate(gp_Vec(spec.direction) * spec.start);
    std::vector<ShapePtr> prisms;
    for (const Region& region : spec.regions) {
      prisms.push_back(prism(spec, frame, region));
    }
    if (prisms.size() == 1) {
      return prisms.front();
    }

    BRepAlgoAPI_Fuse fuse;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(prisms.front()->occt());
    NCollection_List<TopoDS_Shape> tools;
    for (std::size_t i = 1; i < prisms.size(); ++i) {
      tools.Append(prisms[i]->occt());
    }
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    detail::build(fuse);
    if (!fuse.IsDone() || fuse.HasErrors()) {
      throw std::runtime_error("the profiles could not be fused");
    }
    // Merge the coplanar caps of touching profiles.
    fuse.SimplifyResult();
    detail::FaceNamer namer(fuse.Shape());
    for (const ShapePtr& part : prisms) {
      namer.carry(fuse, *part);
    }
    namer.finish();
    detail::require_valid(namer.result(), "the extrusion");
    return namer.shape();
  });
}

} // namespace mitcad::geometry
