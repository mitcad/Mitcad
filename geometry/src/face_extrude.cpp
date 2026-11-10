// SPDX-License-Identifier: MIT
#include "mitcad/geometry/face_extrude.hpp"

#include <cmath>
#include <initializer_list>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/naming.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {

ShapePtr extrude_face(const Shape& body, const FaceExtrudeSpec& spec) {
  if (spec.distance2) {
    // One of two sides may end on the other's side (mitcad#765).
    detail::require_finite("extrude distance", spec.distance1);
    detail::require_finite("extrude distance 2", *spec.distance2);
    if (spec.distance1 == 0.0 || *spec.distance2 == 0.0) {
      throw std::invalid_argument("extrude distance must not be zero");
    }
    if (!(spec.distance1 + *spec.distance2 > 1.0e-7)) {
      throw std::invalid_argument("the two sides of the extrusion overlap: it has no length");
    }
  } else {
    detail::require_positive("extrude distance", spec.distance1);
  }
  detail::require_finite("start offset", spec.start_offset);
  // As an extrusion's tapers: below 90 degrees by a margin (85 degrees
  // itself allowed, with rounding).
  constexpr double kMaxTaper = 85.0 * 3.14159265358979323846 / 180.0 + 1.0e-9;
  for (const double taper : {spec.taper1, spec.taper2}) {
    detail::require_finite("taper angle", taper);
    if (std::abs(taper) > kMaxTaper) {
      throw std::invalid_argument("the taper angle must be between -85 and 85 degrees");
    }
  }
  return detail::run("extrude face", [&] {
    const std::vector<int> found = body.find_faces(spec.face);
    if (found.empty()) {
      throw std::invalid_argument(no_face_error(spec.face));
    }
    if (found.size() > 1) {
      throw std::invalid_argument(face_error(
          spec.face, "the face is in " + std::to_string(found.size()) + " pieces; an extrusion takes one face",
          spec.face + " names " + std::to_string(found.size()) + " faces; an extrusion takes one face"));
    }
    const TopoDS_Face& face = body.face(found.front());
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      const std::string why = " has no surface (a mesh body's triangles): an extrusion takes a planar face";
      throw std::invalid_argument(face_error(spec.face, "the face" + why, "face " + spec.face + why));
    }
    const BRepAdaptor_Surface surface(face);
    const bool tapered = spec.taper1 != 0.0 || (spec.distance2 && spec.taper2 != 0.0);
    const bool planar = surface.GetType() == GeomAbs_Plane;
    if (!planar && (!spec.direction || tapered || spec.start_offset != 0.0)) {
      const std::string why = " is not planar: an extrusion takes a planar face (a curved one only along a "
                              "direction, without a taper or a start offset)";
      throw std::invalid_argument(face_error(spec.face, "the face" + why, "face " + spec.face + why));
    }
    // Out of the material: the plane's normal as the face is oriented (a
    // curved face goes along its direction).
    const gp_Dir normal = planar ? detail::planar_normal(face, surface.Plane()) : *spec.direction;
    if (planar && spec.direction && std::abs(spec.direction->Dot(normal)) < 1.0e-6) {
      throw std::invalid_argument(face_error(spec.face, "the extrusion direction lies in the plane of the face",
                                             "the extrusion direction lies in the plane of face " + spec.face));
    }
    const gp_Dir sweep_along = spec.direction ? *spec.direction : normal;
    const gp_Dir along = spec.reversed ? sweep_along.Reversed() : sweep_along;

    // The face copied, so that the prism shares nothing with the body, its
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

    // With two sides the far cap of side one is the start and that of side
    // two the end, as an extrusion of sketch profiles names them.
    const bool two = spec.distance2.has_value();
    const NameList start{face_name(spec.feature, "start", spec.key)};
    const NameList end{face_name(spec.feature, "end", spec.key)};
    if (tapered) {
      // Each side tapered on its own from the face (mitcad#541), as an
      // extrusion of sketch profiles tapers them; with two sides the face
      // between them is `mid(<key>)`.
      gp_Trsf to_start;
      to_start.SetTranslation(gp_Vec(normal) * spec.start_offset);
      const detail::SweepFace at_start = detail::transformed(sweep, to_start);
      const NameList mid{face_name(spec.feature, "mid", spec.key)};
      // A side ending on the other side's side (mitcad#765) has no solid of
      // its own: the other side's is cut off where it ends, the cut face
      // named as the short side's far cap.
      const bool short1 = two && spec.distance1 < 0.0;
      const bool short2 = two && *spec.distance2 < 0.0;
      if (short1 || short2) {
        // A tapered face is planar: the plane it starts on.
        const gp_Pln start_plane(BRepAdaptor_Surface(at_start.face).Plane().Location(), normal);
        const ShapePtr side = short2 ? detail::distance_side(at_start, spec.distance1, spec.taper1, along, mid, start)
                                     : detail::distance_side(at_start, *spec.distance2, spec.taper2,
                                                             along.Reversed(), mid, end);
        const ShapePtr past = detail::past_other_end(*side, start_plane, short2 ? along : along.Reversed(),
                                                     short2 ? -*spec.distance2 : -spec.distance1, mid.front(),
                                                     short2 ? end : start);
        return detail::finished(*past, "the extrusion of the face");
      }
      std::vector<ShapePtr> parts;
      parts.push_back(detail::distance_side(at_start, spec.distance1, spec.taper1, along, two ? mid : start,
                                            two ? start : end));
      if (two) {
        parts.push_back(detail::distance_side(at_start, *spec.distance2, spec.taper2, along.Reversed(), mid, end));
      }
      const ShapePtr solid = parts.size() == 1 ? parts.front() : detail::fused(parts, "the extrusion of the face");
      return detail::finished(*solid, "the extrusion of the face");
    }
    // Untapered: both sides in one prism, from the far end of side two.
    const double low = two ? -*spec.distance2 : 0.0;
    const double length = spec.distance1 - low;
    gp_Trsf place;
    place.SetTranslation(gp_Vec(normal) * spec.start_offset + gp_Vec(along) * low);
    const ShapePtr prism = detail::named_prism(detail::transformed(sweep, place), gp_Vec(along) * length,
                                               two ? end : start, two ? start : end);
    return detail::finished(*prism, "the extrusion of the face");
  });
}

} // namespace mitcad::geometry
