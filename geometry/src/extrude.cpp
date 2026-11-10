// SPDX-License-Identifier: MIT
#include "mitcad/geometry/extrude.hpp"

#include <cmath>
#include <limits>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRep_Builder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Mat.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "canonical.hpp"
#include "face_join.hpp"
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
    throw std::runtime_error(placed_error("the extrusion of a profile failed", "the extrusion of " + region.name + " failed"));
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

// One side's distance is > 0; one of two sides may be negative (mitcad#765:
// it ends on the other side's side), never zero.
void check_side(const ExtrudeSide& side, bool two) {
  if (!side.target) {
    if (two) {
      detail::require_finite("extrude distance", side.distance);
      if (side.distance == 0.0) {
        throw std::invalid_argument("extrude distance must not be zero");
      }
    } else {
      detail::require_positive("extrude distance", side.distance);
    }
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
  return !a || (a->location == b->location && a->thickness == b->thickness &&
                 a->thickness2 == b->thickness2);
}


std::vector<detail::SweepFace> profile_faces(const ExtrudeFeatureSpec& spec, const ExtrudeSide& side,
                                             bool merge) {
  if (side.thin) {
    std::vector<detail::SweepFace> faces = detail::wall_faces(spec.feature, spec.frame, spec.regions, *side.thin);
    for (detail::SweepFace& face :
         detail::chain_wall_faces(spec.feature, spec.frame, spec.chains, spec.chain_keys, *side.thin)) {
      faces.push_back(std::move(face));
    }
    return faces;
  }
  std::vector<detail::SweepFace> faces = detail::region_faces(spec.feature, spec.frame, spec.regions);
  return merge ? detail::merged(faces) : faces;
}

// A tapered side along a direction off the face's normal (mitcad#539): the
// profile grows in its plane by t tan(taper) at distance t along `along`,
// each side face ruled from its edge to the edge's offset moved along the
// direction. Built as the taper along the normal to the same height (the
// distance times the cosine c of the direction's angle to the normal, the
// taper's tangent over c) sheared along the direction: the shear keeps the
// profile's plane and moves every plane parallel to it within itself, so
// the planes and cones of the straight taper stay planes and cones.
ShapePtr oblique_taper(const detail::SweepFace& face, double distance, double taper, const gp_Dir& along,
                       const NameList& near, const NameList& far) {
  const gp_Pln neutral = BRepAdaptor_Surface(face.face).Plane();
  gp_Dir normal = neutral.Axis().Direction();
  if (normal.Dot(along) < 0.0) {
    normal.Reverse();
  }
  const double c = normal.Dot(along);
  const ShapePtr straight = detail::named_prism(face, gp_Vec(normal) * (distance * c), near, far);
  NameList caps = near;
  caps.insert(caps.end(), far.begin(), far.end());
  const ShapePtr upright = detail::tapered(*straight, neutral, normal, std::atan(std::tan(taper) / c), caps);
  // x -> x + ((x - o) . n) (along / c - n), o on the profile's plane.
  const gp_XYZ shift = along.XYZ() / c - normal.XYZ();
  const gp_XYZ n = normal.XYZ();
  gp_GTrsf shear;
  shear.SetVectorialPart(gp_Mat(1.0 + shift.X() * n.X(), shift.X() * n.Y(), shift.X() * n.Z(),
                                shift.Y() * n.X(), 1.0 + shift.Y() * n.Y(), shift.Y() * n.Z(),
                                shift.Z() * n.X(), shift.Z() * n.Y(), 1.0 + shift.Z() * n.Z()));
  shear.SetTranslationPart(shift * -neutral.Location().XYZ().Dot(n));
  BRepBuilderAPI_GTransform operation(upright->occt(), shear, true);
  if (!operation.IsDone()) {
    throw std::runtime_error("the taper could not be turned along the extrusion direction");
  }
  detail::FaceNamer namer(operation.Shape());
  for (int i = 0; i < upright->face_count(); ++i) {
    const TopoDS_Shape image = operation.ModifiedShape(upright->face(i));
    for (const std::string& name : upright->face_names(i)) {
      namer.add(image, name);
    }
  }
  detail::require_valid(namer.result(), "the tapered extrusion");
  const ShapePtr sheared = namer.shape();
  // (The shear makes every face a B-spline; its planes and cones back.)
  if (ShapePtr analytic = detail::canonical_dressup(*upright, *sheared)) {
    return analytic;
  }
  return sheared;
}

// One side swept from a face on a start plane parallel to the sketch.
ShapePtr parallel_side(const std::string& feature, const detail::SweepFace& face,
                       const ExtrudeSide& side, const gp_Dir& along, const NameList& near,
                       const NameList& far) {
  if (!side.target) {
    return detail::distance_side(face, side.distance, side.taper, along, near, far);
  }
  const gp_Pln neutral = BRepAdaptor_Surface(face.face).Plane();
  // A taper along a direction off the sketch normal (mitcad#539).
  if (side.taper != 0.0 && std::abs(std::abs(neutral.Axis().Direction().Dot(along)) - 1.0) > 1.0e-12) {
    throw std::invalid_argument("unsupported: a taper up to an object along a direction off the sketch normal");
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
  return detail::trimmed(*solid, *side.target, tool, along, near.front(), far, unreached, true);
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
    return detail::trimmed(*past, *side.target, tool, along, near.front(), far, unreached, true);
  }
  // The far cap is the start plane moved by the distance.
  Target end;
  end.plane = gp_Pln(start.Location().Translated(gp_Vec(along) * side.distance), normal);
  return detail::trimmed(*past, end, detail::plane_tool(end.plane, around), along, near.front(), far,
                         unreached);
}

// The parts of an extrusion as one shape. Profiles that touch only at a
// point (a circle touching another profile's curve) make parts that touch
// along a line, which the fuse joins into a shell that cannot be oriented:
// then the parts stay apart, as those of profiles apart from each other do.
ShapePtr joined(const std::vector<ShapePtr>& parts) {
  try {
    // (The sides of touching profiles on one surface joined, which the
    // fuse's exact merge leaves apart when their curves lie a few 1e-7 mm
    // apart: arcs of one circle, mitcad#1261.)
    return detail::join_same_surfaces(detail::finished(*detail::fused(parts, "the extrusion"), "the extrusion"));
  } catch (const std::runtime_error& error) {
    if (parts.size() < 2 || std::string(error.what()).find("produced an invalid shape") == std::string::npos) {
      throw;
    }
  }
  BRep_Builder builder;
  TopoDS_Compound apart;
  builder.MakeCompound(apart);
  for (const ShapePtr& part : parts) {
    builder.Add(apart, part->occt());
  }
  detail::FaceNamer namer(apart);
  for (const ShapePtr& part : parts) {
    for (int i = 0; i < part->face_count(); ++i) {
      for (const std::string& name : part->face_names(i)) {
        namer.add(part->face(i), name);
      }
    }
  }
  return detail::finished(*namer.shape(), "the extrusion");
}

} // namespace

namespace detail {

ShapePtr distance_side(const SweepFace& face, double distance, double taper, const gp_Dir& along,
                       const NameList& near, const NameList& far) {
  const gp_Pln neutral = BRepAdaptor_Surface(face.face).Plane();
  // A taper along a direction off the face's normal (mitcad#539).
  if (taper != 0.0 && std::abs(std::abs(neutral.Axis().Direction().Dot(along)) - 1.0) > 1.0e-12) {
    return oblique_taper(face, distance, taper, along, near, far);
  }
  const ShapePtr solid = named_prism(face, gp_Vec(along) * distance, near, far);
  if (taper == 0.0) {
    return solid;
  }
  NameList caps = near;
  caps.insert(caps.end(), far.begin(), far.end());
  return tapered(*solid, neutral, along, taper, caps);
}

ShapePtr past_other_end(const Shape& solid, const gp_Pln& start, const gp_Dir& along, double distance,
                        const std::string& near, const NameList& cut) {
  const gp_Pln end_plane(start.Location().Translated(gp_Vec(along) * distance), start.Axis().Direction());
  return split_keep(
      solid, plane_tool(end_plane, bounds_of(solid.occt())), cut,
      [&near](const Shape& piece) { return !has_face_named(piece, near); },
      "the extrusion past the other side's end");
}

} // namespace detail

ShapePtr extrude_feature(const ExtrudeFeatureSpec& spec) {
  if (spec.regions.empty() && spec.chains.empty()) {
    throw std::invalid_argument("no profiles to extrude");
  }
  if (!spec.chains.empty() && !spec.side1.thin) {
    throw std::invalid_argument("only a thin extrusion follows curves");
  }
  if (spec.chain_keys.size() != spec.chains.size()) {
    throw std::invalid_argument("each chain of curves needs its key");
  }
  const bool two = spec.side2.has_value();
  check_side(spec.side1, two);
  // The side of two that ends on the other side's side (a negative
  // distance, mitcad#765): 0 or 1, -1 for none.
  int short_side = -1;
  if (two) {
    check_side(*spec.side2, two);
    if (spec.side1.thin.has_value() != spec.side2->thin.has_value()) {
      throw std::invalid_argument("both sides of a thin extrusion need a wall");
    }
    const bool short1 = !spec.side1.target && spec.side1.distance < 0.0;
    const bool short2 = !spec.side2->target && spec.side2->distance < 0.0;
    if (short1 && short2) {
      throw std::invalid_argument("both sides of the extrusion end on the other side");
    }
    if (!spec.side1.target && !spec.side2->target && !(spec.side1.distance + spec.side2->distance > 1.0e-7)) {
      throw std::invalid_argument("the two sides of the extrusion overlap: it has no length");
    }
    short_side = short1 ? 0 : short2 ? 1 : -1;
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
      // the start (`startFaces` in .f3d) and the far cap of side two the end
      // (a negative side's end lies on the other side's side: the prism
      // then does not reach the start, mitcad#765).
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
      // The plane the sides start from.
      const gp_Pln start_plane =
          oblique ? *oblique : gp_Pln(spec.frame.origin.Translated(gp_Vec(normal) * offset), normal);
      for (int k = 0; k < (two ? 2 : 1); ++k) {
        // A side ending on the other side's side has no solid of its own:
        // it cuts the other side's off where it ends (mitcad#765).
        if (k == short_side) {
          continue;
        }
        const ExtrudeSide& side = k == 0 ? spec.side1 : *spec.side2;
        const gp_Dir along = k == 0 ? spec.direction : spec.direction.Reversed();
        // Touching profiles taper as one.
        for (const detail::SweepFace& face : profile_faces(spec, side, taper)) {
          const NameList near = detail::cap_names(spec.feature, two ? "mid" : "start", face);
          const NameList far = detail::cap_names(spec.feature, k == 0 && two ? "start" : "end", face);
          ShapePtr part = oblique ? oblique_side(spec.feature, face, *oblique, side, along, near, far)
                                  : parallel_side(spec.feature, detail::transformed(face, to_start),
                                                  side, along, near, far);
          if (short_side >= 0) {
            // The short side's end, named as its far cap.
            const double cut_at = -(short_side == 0 ? spec.side1 : *spec.side2).distance;
            const NameList short_end = detail::cap_names(spec.feature, short_side == 0 ? "start" : "end", face);
            part = detail::past_other_end(*part, start_plane, along, cut_at, near.front(), short_end);
          }
          parts.push_back(part);
        }
      }
    }
    return joined(parts);
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
