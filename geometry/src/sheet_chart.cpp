// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet.hpp"

#include <algorithm>
#include <array>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <limits>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>

#include <Adaptor3d_CurveOnSurface.hxx>
#include <Approx_Curve3d.hxx>
#include <BOPAlgo_ArgumentAnalyzer.hxx>
#include <BRepAlgoAPI_Check.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BndLib_Add2dCurve.hxx>
#include <Bnd_Box.hxx>
#include <Bnd_Box2d.hxx>
#include <GProp_GProps.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Circ2d.hxx>

#include "history.hpp"
#include "mitcad/geometry/query.hpp"
#include "sheet_curve.hpp"
#include "sheet_oblique_cone.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kAngularTolerance = 1.0e-9;

using Curve2Ptr = occ::handle<Geom2d_BSplineCurve>;
using Curve3Ptr = occ::handle<Geom_BSplineCurve>;
using SurfacePtr = occ::handle<Geom_Surface>;

double parity(MaterialParity p) {
  switch (p) {
  case MaterialParity::Positive: return 1;
  case MaterialParity::Negative: return -1;
  }
  throw std::invalid_argument("unknown sheet material parity");
}

void finite(const gp_Pnt& p) {
  for (int i = 1; i <= 3; ++i) detail::require_finite("sheet chart coordinate", p.Coord(i));
}

std::string domain_key(const SheetDomainRef& d) {
  if (d.feature.empty()) throw std::invalid_argument("sheet chart has no feature identity");
  switch (d.kind) {
  case SheetDomainKind::Panel: return d.feature + ".p" + std::to_string(d.slot);
  case SheetDomainKind::Bend: return d.feature + ".b" + std::to_string(d.slot);
  }
  throw std::invalid_argument("unknown sheet domain kind");
}

std::string cap_name(const SheetDomainRef& d, bool back) {
  const std::string role = d.kind == SheetDomainKind::Panel ? "sheet_" : "sheet_bend_";
  return face_name(d.feature, role + (back ? "back" : "front"), std::to_string(d.slot));
}

std::string side_name(const SheetDomainRef& d, const CurvePieceRef& p) {
  const std::string role = d.kind == SheetDomainKind::Panel ? "sheet_side" : "sheet_bend_side";
  return face_name(d.feature, role + std::to_string(d.slot), p.curve);
}

struct Embedding {
  const SheetPatch* patch;
  double thickness;
  std::array<SurfacePtr, 2> surfaces;
  // Chart orientation relative to the cap's native UV plane.
  double uv_sign = 1;
  // Orientation of (u,v,front-to-back), relative to a positive volume.
  double material_sign = 1;
  // Orientation of the chart's loops in the caps' UV plane: a cone's chart
  // is polar there, (angle, distance) turned against (x, y).
  double wire_sign = 1;
  // A cone (mitcad#2435): the surfaces' offsets from the neutral cone
  // (outwards; front, back), the generator distance of their reference
  // circle and the sine and cosine of the half angle.
  std::array<double, 2> cone_offsets{0, 0};
  double rho_ref = 0;
  double sin_angle = 0;
  double cos_angle = 1;
  // An oblique cone (mitcad#2491): its front surface's cone, the caps'
  // offsets from it along its normals (front, back), the development of
  // its neutral surface and the sign of its caps' own normals against the
  // outward one (their parameters are the angle and the distance along the
  // generator).
  detail::ObliqueCone skew{};
  std::array<double, 2> skew_offsets{0, 0};
  std::shared_ptr<const detail::ObliqueDevelopment> skew_development{};
  double skew_natural = 1;

  const ConeEmbedding* cone() const { return std::get_if<ConeEmbedding>(&patch->embedding); }
  const ObliqueConeEmbedding* oblique() const { return std::get_if<ObliqueConeEmbedding>(&patch->embedding); }
  // A conical or an oblique conical panel: its boundaries are fitted onto
  // its caps (cone_rails), and no port joins it.
  bool fanned() const { return cone() || oblique(); }

  // An oblique cone's chart point as its angle and its distance along the
  // generator.
  gp_Pnt2d skew_coordinates(const gp_Pnt2d& p) const {
    double theta = 0, rho = 0;
    if (!skew_development->inverse(p, theta, rho)) {
      throw std::invalid_argument("sheet material lies outside the oblique cone's angle interval");
    }
    return {theta, rho};
  }

  // A cone chart point's developed angle in the chart's window.
  double developed_angle(const gp_Pnt2d& p) const {
    const double free = kPi - cone()->angle_extent / 2;
    double phi = std::atan2(p.Y(), p.X());
    if (phi < -free) phi += 2 * kPi;
    return phi;
  }

  gp_Pnt point(const gp_Pnt2d& p, bool back) const {
    if (const auto* plane = std::get_if<PlaneEmbedding>(&patch->embedding)) {
      gp_Pnt result = plane->front_frame.point(p);
      if (back) result.Translate(thickness * parity(plane->front_to_back_sign) *
                                 gp_Vec(plane->front_frame.normal()));
      return result;
    }
    if (const auto* k = cone()) {
      const double rho = std::hypot(p.X(), p.Y());
      const double theta = developed_angle(p) / sin_angle;
      const gp_Vec radial = std::cos(theta) * gp_Vec(k->radial_start) + std::sin(theta) * gp_Vec(k->angular_direction);
      const double d = cone_offsets[back ? 1 : 0];
      return k->apex.Translated((rho * sin_angle + d * cos_angle) * radial +
                                (rho * cos_angle - d * sin_angle) * gp_Vec(k->axis));
    }
    if (oblique()) {
      const gp_Pnt2d at = skew_coordinates(p);
      return skew.point(at.X(), at.Y(), skew_offsets[back ? 1 : 0]);
    }
    const auto& c = std::get<CylinderEmbedding>(patch->embedding);
    const double r = c.inside_radius + ((back == c.inner_is_front) ? thickness : 0.0);
    const double angle = p.X() / c.neutral_radius;
    return c.centre.Translated(p.Y() * gp_Vec(c.axis) + r * std::cos(angle) * gp_Vec(c.radial_start) +
                              r * std::sin(angle) * gp_Vec(c.angular_direction));
  }

  gp_Pnt2d uv(const gp_Pnt2d& p) const {
    if (const auto* c = std::get_if<CylinderEmbedding>(&patch->embedding)) {
      return {uv_sign * p.X() / c->neutral_radius, p.Y()};
    }
    if (cone()) {
      return {uv_sign * developed_angle(p) / sin_angle, std::hypot(p.X(), p.Y()) - rho_ref};
    }
    if (oblique()) return skew_coordinates(p);
    return p;
  }

  bool reverse_cap(bool back) const {
    if (std::holds_alternative<PlaneEmbedding>(patch->embedding)) {
      return back ? material_sign < 0 : material_sign > 0;
    }
    // An oblique cone's caps: the cap's own normal against the solid's
    // outward one there (the front's points against front-to-back).
    if (const auto* o = oblique()) return skew_natural * (o->inner_is_front ? 1 : -1) * (back ? 1 : -1) < 0;
    // A cylinder's and a cone's surfaces face away from the axis.
    if (const auto* k = cone()) return back != k->inner_is_front;
    return back != std::get<CylinderEmbedding>(patch->embedding).inner_is_front;
  }
};

// An oblique cone's cap (mitcad#2491): the surface `offset` from the cone
// along its normals over the angles [first, last] and the distances [inner_rho,
// outer_rho] along the generators, its parameters the angle and the distance: a
// B-spline of degree 3 in the angle through its exact points at evenly
// spaced angles (more until within `tolerance` of it between them) and of
// degree 1 along the generators, where the surface is straight.
occ::handle<Geom_BSplineSurface> oblique_cap(const detail::ObliqueCone& cone, double offset, double first,
                                             double last, double inner_rho, double outer_rho, double tolerance) {
  for (int n = 32; n <= 16384; n *= 2) {
    throw_if_cancelled();
    const occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    std::array<occ::handle<NCollection_HArray1<gp_Pnt>>, 2> rows{new NCollection_HArray1<gp_Pnt>(1, n + 1),
                                                                 new NCollection_HArray1<gp_Pnt>(1, n + 1)};
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      params->SetValue(i + 1, t);
      rows[0]->SetValue(i + 1, cone.point(t, inner_rho, offset));
      rows[1]->SetValue(i + 1, cone.point(t, outer_rho, offset));
    }
    std::array<Curve3Ptr, 2> curves;
    for (int k = 0; k < 2; ++k) {
      GeomAPI_Interpolate fit(rows[k], params, false, 1.0e-12);
      fit.Perform();
      if (!fit.IsDone()) throw std::runtime_error("sheet oblique conical panel could not be laid on its cone");
      curves[k] = fit.Curve();
    }
    const int poles = curves[0]->NbPoles();
    if (curves[1]->NbPoles() != poles || curves[0]->Degree() != curves[1]->Degree()) {
      throw std::runtime_error("sheet oblique conical panel could not be laid on its cone");
    }
    NCollection_Array2<gp_Pnt> net(1, poles, 1, 2);
    for (int i = 1; i <= poles; ++i) {
      net(i, 1) = curves[0]->Pole(i);
      net(i, 2) = curves[1]->Pole(i);
    }
    NCollection_Array1<double> along(1, 2);
    along(1) = inner_rho;
    along(2) = outer_rho;
    NCollection_Array1<int> ends(1, 2);
    ends(1) = ends(2) = 2;
    const occ::handle<Geom_BSplineSurface> surface = new Geom_BSplineSurface(
        net, curves[0]->Knots(), along, curves[0]->Multiplicities(), ends, curves[0]->Degree(), 1);
    double worst = 0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      for (const double rho : {inner_rho, (inner_rho + outer_rho) / 2, outer_rho}) {
        worst = std::max(worst, surface->Value(t, rho).Distance(cone.point(t, rho, offset)));
      }
    }
    if (worst <= tolerance) return surface;
  }
  throw std::runtime_error("sheet oblique conical panel exceeded the surface approximation tolerance");
}

Embedding make_embedding(const SheetPatch& patch, double thickness, double tolerance) {
  Embedding result{&patch, thickness, {}};
  if (const auto* o = std::get_if<ObliqueConeEmbedding>(&patch.embedding)) {
    finite(o->apex);
    finite(o->centre);
    detail::require_positive("sheet oblique cone radius", o->radius);
    detail::require_positive("sheet oblique cone angle", o->angle_extent);
    if (o->angle_extent >= 2 * kPi - kAngularTolerance ||
        std::abs(o->radial_start.Dot(o->angular_direction)) > kAngularTolerance) {
      throw std::invalid_argument("invalid sheet oblique cone angle or orthogonal basis");
    }
    result.skew.apex = o->apex;
    result.skew.centre = o->centre;
    result.skew.radius = o->radius;
    result.skew.x = gp_Vec(o->radial_start);
    result.skew.y = gp_Vec(o->angular_direction);
    // The neutral surface and the back from the front, away from the
    // cone's inside.
    const double neutral = o->inner_is_front ? o->neutral_depth : o->neutral_depth - thickness;
    result.skew_offsets = {0.0, o->inner_is_front ? thickness : -thickness};
    const double free = kPi - o->angle_extent / 2;
    result.skew_development =
        std::make_shared<const detail::ObliqueDevelopment>(result.skew, neutral, -free, o->angle_extent + free);
    // The chart's (x, y) turns against (angle, distance): along the
    // generator and across it, as on a cone of revolution.
    const detail::ObliqueCone::Generator start = result.skew.generator(0);
    result.skew_natural = start.dg.Crossed(start.g).Dot(start.n) > 0 ? 1 : -1;
    result.wire_sign = -1;
    result.material_sign = -result.skew_natural * (o->inner_is_front ? 1 : -1);
    // The caps over the material's angles and distances along the
    // generators, a millimetre wider.
    double inner_rho = std::numeric_limits<double>::infinity(), outer_rho = -inner_rho;
    double first = inner_rho, last = outer_rho;
    for (const auto& region : patch.regions) {
      for (const auto& loop : region.loops) {
        for (const auto& edge : loop.edges) {
          const auto curve = detail::make_sheet_curve(edge.curve);
          for (int i = 0; i <= 64; ++i) {
            const double t = edge.curve.range.first + (edge.curve.range.last - edge.curve.range.first) * i / 64;
            const gp_Pnt2d at = result.skew_coordinates(curve.curve->Value(curve.native_parameter(t)));
            first = std::min(first, at.X());
            last = std::max(last, at.X());
            inner_rho = std::min(inner_rho, at.Y());
            outer_rho = std::max(outer_rho, at.Y());
          }
        }
      }
    }
    if (!(outer_rho > inner_rho) || !(last > first)) throw std::invalid_argument("sheet oblique conical panel has no material");
    inner_rho = inner_rho > 1 ? inner_rho - 1 : inner_rho / 2;
    outer_rho += 1;
    first = std::max(-free, first - 0.01);
    last = std::min(o->angle_extent + free, last + 0.01);
    for (int back = 0; back < 2; ++back) {
      result.surfaces[back] =
          oblique_cap(result.skew, result.skew_offsets[back], first, last, inner_rho, outer_rho, tolerance / 16);
    }
    return result;
  }
  if (const auto* p = std::get_if<PlaneEmbedding>(&patch.embedding)) {
    const Frame& f = p->front_frame;
    finite(f.origin);
    if (std::abs(f.x_axis.Dot(f.y_axis)) > kAngularTolerance) {
      throw std::invalid_argument("sheet plane axes must be orthogonal");
    }
    result.material_sign = parity(p->front_to_back_sign);
    for (int back = 0; back < 2; ++back) {
      result.surfaces[back] = new Geom_Plane(gp_Ax3(result.point({0, 0}, back != 0), f.normal(), f.x_axis));
    }
  } else if (const auto* k = std::get_if<ConeEmbedding>(&patch.embedding)) {
    finite(k->apex);
    if (!(k->half_angle > kAngularTolerance && k->half_angle < kPi / 2 - kAngularTolerance)) {
      throw std::invalid_argument("invalid sheet cone half angle");
    }
    result.sin_angle = std::sin(k->half_angle);
    result.cos_angle = std::cos(k->half_angle);
    detail::require_positive("sheet cone angle", k->angle_extent);
    if (k->angle_extent >= 2 * kPi * result.sin_angle - kAngularTolerance ||
        std::abs(k->axis.Dot(k->radial_start)) > kAngularTolerance ||
        std::abs(k->axis.Dot(k->angular_direction)) > kAngularTolerance ||
        std::abs(k->radial_start.Dot(k->angular_direction)) > kAngularTolerance) {
      throw std::invalid_argument("invalid sheet cone angle or orthogonal basis");
    }
    result.uv_sign = k->axis.Crossed(k->radial_start).Dot(k->angular_direction) > 0 ? 1 : -1;
    // The chart's (x, y) turns against the caps' (angle, distance): the
    // polar frame (along the generator, round the axis) maps to (g, e),
    // and (g, e, outward normal) is left-handed.
    result.wire_sign = -result.uv_sign;
    result.material_sign = -result.uv_sign * (k->inner_is_front ? 1 : -1);
    const double inner = -k->neutral_depth, outer = thickness - k->neutral_depth;
    result.cone_offsets = k->inner_is_front ? std::array<double, 2>{inner, outer} : std::array<double, 2>{outer, inner};
    // A reference circle where both surfaces have a positive radius.
    result.rho_ref = std::max(1.0, (std::abs(inner) + std::abs(outer)) * result.cos_angle / result.sin_angle + 1.0);
    for (int back = 0; back < 2; ++back) {
      const double d = result.cone_offsets[back];
      // The cone moved along its normals by d: the same angle and, at the
      // same parameters, the point moved along the normal.
      const gp_Ax3 axes(k->apex.Translated(gp_Vec(k->axis) * (result.rho_ref * result.cos_angle - d * result.sin_angle)),
                        k->axis, k->radial_start);
      result.surfaces[back] =
          new Geom_ConicalSurface(axes, k->half_angle, result.rho_ref * result.sin_angle + d * result.cos_angle);
    }
  } else {
    const auto& c = std::get<CylinderEmbedding>(patch.embedding);
    finite(c.centre);
    detail::require_positive("sheet inside radius", c.inside_radius);
    detail::require_positive("sheet neutral radius", c.neutral_radius);
    detail::require_positive("sheet bend angle", c.angle_extent);
    // A flange's bend may turn past 180 degrees, short of a full turn (a hem, mitcad#1378).
    // The neutral radius only scales the chart's u to the angle (u / neutral radius): a
    // measured allowance or deduction of a fold (mitcad#1939: a nearly sharp crease in board)
    // may put it outside the material, beyond K = 1 or below K = 0, so any positive radius is
    // taken.
    if (c.angle_extent >= 2 * kPi - kAngularTolerance || std::abs(c.axis.Dot(c.radial_start)) > kAngularTolerance ||
        std::abs(c.axis.Dot(c.angular_direction)) > kAngularTolerance ||
        std::abs(c.radial_start.Dot(c.angular_direction)) > kAngularTolerance) {
      throw std::invalid_argument("invalid sheet cylinder radius, angle or orthogonal basis");
    }
    result.uv_sign = c.axis.Crossed(c.radial_start).Dot(c.angular_direction) > 0 ? 1 : -1;
    result.wire_sign = result.uv_sign;
    result.material_sign = result.uv_sign * (c.inner_is_front ? 1 : -1);
    const gp_Ax3 axes(c.centre, c.axis, c.radial_start);
    for (int back = 0; back < 2; ++back) {
      const double radius = c.inside_radius + ((static_cast<bool>(back) == c.inner_is_front) ? thickness : 0.0);
      result.surfaces[back] = new Geom_CylindricalSurface(axes, radius);
    }
  }
  return result;
}

// The bounded support is converted exactly ONCE. An affine map of rational
// poles changes geometry without a second parameterization of either rail.
Curve2Ptr bounded_spline(const detail::Curve2& curve, ParamSpan span) {
  const double first = std::min(span.first, span.last);
  const double last = std::max(span.first, span.last);
  if (!(last > first)) throw std::invalid_argument("sheet boundary has an empty parameter interval");
  const occ::handle<Geom2d_TrimmedCurve> trimmed = new Geom2d_TrimmedCurve(curve.curve, first, last);
  Curve2Ptr result = Geom2dConvert::CurveToBSplineCurve(trimmed);
  if (result.IsNull()) throw std::runtime_error("sheet curve cannot be represented exactly");
  return result;
}

Curve2Ptr map_uv(const Curve2Ptr& curve, const Embedding& embedding) {
  Curve2Ptr result = Curve2Ptr::DownCast(curve->Copy());
  for (int i = 1; i <= result->NbPoles(); ++i) result->SetPole(i, embedding.uv(result->Pole(i)));
  return result;
}

Curve3Ptr plane_rail(const Curve2Ptr& curve, const Embedding& embedding, bool back) {
  NCollection_Array1<gp_Pnt> poles(1, curve->NbPoles());
  NCollection_Array1<double> weights(1, curve->NbPoles());
  for (int i = 1; i <= curve->NbPoles(); ++i) {
    poles(i) = embedding.point(curve->Pole(i), back);
    weights(i) = curve->Weight(i);
  }
  return new Geom_BSplineCurve(poles, weights, curve->Knots(), curve->Multiplicities(), curve->Degree());
}

struct Rails {
  Curve2Ptr uv;
  std::array<Curve3Ptr, 2> rail;
};

// A conical panel's boundary (mitcad#2435): the chart is the cone's
// development, which the caps' parameters are not an affine image of. The
// parameter curve and both rails are cubic B-splines through the exact
// images of the chart curve's points at the same parameters (so the rails
// pair pole by pole for the wall between them), more points until every
// one is within a quarter of the tolerance of its exact image between the
// points, on its surface too. The chart curve is read at its own parameter
// (a line's length, an arc's angle), never a conversion's: an arc converted
// to a rational spline changes speed at its knots, which no cubic through
// evenly spaced points follows.
Rails cone_rails(const occ::handle<Geom2d_Curve>& chart_curve, double first, double last, const Embedding& embedding,
                 double tolerance) {
  for (int n = 16; n <= 4096; n *= 2) {
    throw_if_cancelled();
    const occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    const occ::handle<NCollection_HArray1<gp_Pnt2d>> uv = new NCollection_HArray1<gp_Pnt2d>(1, n + 1);
    std::array<occ::handle<NCollection_HArray1<gp_Pnt>>, 2> rail{new NCollection_HArray1<gp_Pnt>(1, n + 1),
                                                                 new NCollection_HArray1<gp_Pnt>(1, n + 1)};
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      const gp_Pnt2d p = chart_curve->Value(t);
      params->SetValue(i + 1, t);
      uv->SetValue(i + 1, embedding.uv(p));
      for (int back = 0; back < 2; ++back) rail[back]->SetValue(i + 1, embedding.point(p, back != 0));
    }
    Geom2dAPI_Interpolate uv_fit(uv, params, false, 1.0e-12);
    uv_fit.Perform();
    if (!uv_fit.IsDone()) throw std::runtime_error("sheet conical boundary could not be laid on its cone");
    Rails result{uv_fit.Curve(), {}};
    for (int back = 0; back < 2; ++back) {
      GeomAPI_Interpolate fit(rail[back], params, false, 1.0e-12);
      fit.Perform();
      if (!fit.IsDone()) throw std::runtime_error("sheet conical boundary could not be laid on its cone");
      result.rail[back] = fit.Curve();
    }
    double worst = 0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      const gp_Pnt2d p = chart_curve->Value(t);
      const gp_Pnt2d at = result.uv->Value(t);
      for (int back = 0; back < 2; ++back) {
        const gp_Pnt exact = embedding.point(p, back != 0);
        worst = std::max(worst, result.rail[back]->Value(t).Distance(exact));
        worst = std::max(worst, embedding.surfaces[back]->Value(at.X(), at.Y()).Distance(exact));
      }
    }
    if (worst <= tolerance / 4) return result;
  }
  throw std::runtime_error("sheet conical boundary exceeded the rail approximation tolerance");
}

Rails make_rails(const Curve2Ptr& chart_curve, const Embedding& embedding, double tolerance) {
  if (embedding.fanned()) {
    return cone_rails(chart_curve, chart_curve->FirstParameter(), chart_curve->LastParameter(), embedding, tolerance);
  }
  Rails result{map_uv(chart_curve, embedding), {}};
  if (std::holds_alternative<PlaneEmbedding>(embedding.patch->embedding)) {
    for (int back = 0; back < 2; ++back) result.rail[back] = plane_rail(chart_curve, embedding, back != 0);
    return result;
  }
  const auto& c = std::get<CylinderEmbedding>(embedding.patch->embedding);
  const int inner = c.inner_is_front ? 0 : 1;
  const double scale = (c.inside_radius + embedding.thickness) / c.inside_radius;
  const double fit_tolerance = tolerance / (16 * scale);
  const occ::handle<Geom2dAdaptor_Curve> curve = new Geom2dAdaptor_Curve(result.uv);
  const occ::handle<GeomAdaptor_Surface> surface = new GeomAdaptor_Surface(embedding.surfaces[inner]);
  const occ::handle<Adaptor3d_CurveOnSurface> on_surface = new Adaptor3d_CurveOnSurface(curve, surface);
  throw_if_cancelled();
  Approx_Curve3d fit(on_surface, fit_tolerance, GeomAbs_C1, 256, 12);
  throw_if_cancelled();
  if (!fit.IsDone() || fit.Curve().IsNull() || fit.MaxError() > fit_tolerance) {
    throw std::runtime_error("sheet cylindrical boundary exceeded the rail approximation tolerance");
  }
  result.rail[inner] = fit.Curve();
  result.rail[1 - inner] = Curve3Ptr::DownCast(fit.Curve()->Copy());
  for (int i = 1; i <= result.rail[1 - inner]->NbPoles(); ++i) {
    const gp_Vec relative(c.centre, result.rail[inner]->Pole(i));
    const gp_Vec axial = relative.Dot(gp_Vec(c.axis)) * gp_Vec(c.axis);
    result.rail[1 - inner]->SetPole(i, c.centre.Translated(axial + scale * (relative - axial)));
  }
  // Approx_Curve3d keeps the adaptor parameter. Verify that contract, including
  // each knot span, against BOTH exact supports. The approximation certificate
  // above bounds the whole inner curve; radial scaling bounds the outer one.
  for (int k = 1; k < result.rail[inner]->NbKnots(); ++k) {
    for (int j = 0; j <= 16; ++j) {
      const double a = result.rail[inner]->Knot(k), b = result.rail[inner]->Knot(k + 1);
      const double t = a + (b - a) * j / 16;
      const gp_Pnt2d uv = result.uv->Value(t);
      for (int back = 0; back < 2; ++back) {
        if (result.rail[back]->Value(t).Distance(embedding.surfaces[back]->Value(uv.X(), uv.Y())) > tolerance / 4) {
          throw std::runtime_error("sheet paired rails lost their common material parameter");
        }
      }
    }
  }
  return result;
}

class Vertices {
public:
  explicit Vertices(double tolerance) : m_tolerance(tolerance) {}

  void add(const TopoDS_Vertex& vertex) {
    if (vertex.IsNull()) return;
    for (const auto& existing : m_vertices) if (existing.IsSame(vertex)) return;
    m_vertices.push_back(vertex);
  }

  TopoDS_Vertex at(const gp_Pnt& point) {
    for (const auto& vertex : m_vertices) {
      if (BRep_Tool::Pnt(vertex).Distance(point) <= m_tolerance) return vertex;
    }
    const auto vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
    m_vertices.push_back(vertex);
    return vertex;
  }

private:
  double m_tolerance;
  std::vector<TopoDS_Vertex> m_vertices;
};

TopoDS_Edge cap_edge(const Curve3Ptr& rail, const Curve2Ptr& uv, const SurfacePtr& surface,
                    double tolerance, Vertices& vertices) {
  const double first = rail->FirstParameter(), last = rail->LastParameter();
  BRepBuilderAPI_MakeEdge maker(rail, vertices.at(rail->Value(first)), vertices.at(rail->Value(last)), first, last);
  if (!maker.IsDone()) throw std::runtime_error("sheet cap edge construction failed");
  TopoDS_Edge edge = maker.Edge();
  BRep_Builder builder;
  builder.UpdateEdge(edge, uv, surface, TopLoc_Location(), tolerance);
  builder.Range(edge, surface, TopLoc_Location(), rail->FirstParameter(), rail->LastParameter());
  builder.SameRange(edge, true);
  builder.SameParameter(edge, true);
  return edge;
}

TopoDS_Face wall(const Rails& rails, bool reverse, double tolerance) {
  const Curve3Ptr& front = rails.rail[0];
  const Curve3Ptr& back = rails.rail[1];
  NCollection_Array2<gp_Pnt> poles(1, front->NbPoles(), 1, 2);
  NCollection_Array2<double> weights(1, front->NbPoles(), 1, 2);
  for (int i = 1; i <= front->NbPoles(); ++i) {
    poles(i, 1) = front->Pole(i);
    poles(i, 2) = back->Pole(i);
    weights(i, 1) = weights(i, 2) = front->Weight(i);
  }
  NCollection_Array1<double> qknots(1, 2);
  NCollection_Array1<int> qmultiplicities(1, 2);
  qknots(1) = 0; qknots(2) = 1;
  qmultiplicities(1) = qmultiplicities(2) = 2;
  const occ::handle<Geom_BSplineSurface> surface = new Geom_BSplineSurface(
      poles, weights, front->Knots(), qknots, front->Multiplicities(), qmultiplicities, front->Degree(), 1);
  BRepBuilderAPI_MakeFace maker(surface, front->FirstParameter(), front->LastParameter(), 0, 1, tolerance);
  if (!maker.IsDone()) throw std::runtime_error("sheet paired wall construction failed");
  TopoDS_Face result = maker.Face();
  if (reverse) result.Reverse();
  return result;
}

double loop_area(const MaterialLoop& loop, double tolerance) {
  if (loop.edges.empty()) throw std::invalid_argument("sheet material loop is empty");
  double twice_area = 0;
  gp_Pnt2d first, previous;
  bool started = false;
  for (const auto& edge : loop.edges) {
    if (edge.piece.curve.empty()) throw std::invalid_argument("sheet boundary has no curve identity");
    const auto curve = detail::make_sheet_curve(edge.curve);
    const double a = curve.native_range.first, b = curve.native_range.last;
    const gp_Pnt2d start = curve.curve->Value(a), end = curve.curve->Value(b);
    if (!started) { first = start; started = true; }
    else if (previous.Distance(start) > tolerance) throw std::invalid_argument("sheet material loop has an open gap");
    previous = end;
    // This quadrature decides orientation only. It never replaces the exact
    // material boundary or supplies geometric data to the constructor.
    for (int i = 0; i <= 128; ++i) {
      const double t = a + (b - a) * i / 128;
      gp_Pnt2d p; gp_Vec2d d;
      curve.curve->D1(t, p, d);
      const int weight = (i == 0 || i == 128) ? 1 : (i % 2 == 0 ? 2 : 4);
      twice_area += weight * (p.X() * d.Y() - p.Y() * d.X()) * (b - a) / 384;
    }
  }
  if (previous.Distance(first) > tolerance) throw std::invalid_argument("sheet material loop has an open gap");
  if (std::abs(twice_area) <= tolerance * tolerance) throw std::invalid_argument("sheet material loop has zero area");
  return twice_area / 2;
}

MaterialRegion normalized(MaterialRegion region, double tolerance) {
  if (region.loops.empty()) throw std::invalid_argument("sheet material region has no exterior");
  for (std::size_t i = 0; i < region.loops.size(); ++i) {
    auto& loop = region.loops[i];
    if ((loop_area(loop, tolerance) > 0) != (i == 0)) {
      std::reverse(loop.edges.begin(), loop.edges.end());
      for (auto& edge : loop.edges) std::swap(edge.curve.range.first, edge.curve.range.last);
    }
  }
  return region;
}

const MaterialEdge& boundary(const SheetChartShapeSpec& spec, const BoundaryPieceRef& ref, std::size_t& patch_index) {
  const MaterialEdge* found = nullptr;
  for (std::size_t p = 0; p < spec.patches.size(); ++p) {
    if (spec.patches[p].domain != ref.domain) continue;
    for (const auto& region : spec.patches[p].regions) for (const auto& loop : region.loops) for (const auto& edge : loop.edges) {
      if (edge.piece != ref.piece) continue;
      if (found) throw std::invalid_argument("sheet port boundary has ambiguous source pieces");
      found = &edge; patch_index = p;
    }
  }
  if (!found) throw std::invalid_argument("sheet port boundary is missing");
  return *found;
}

struct Port {
  const ShapePort* input;
  std::array<TopoDS_Edge, 2> edges;
  std::array<std::array<gp_Pnt, 2>, 2> points;
  std::array<int, 2> uses{0, 0};
  double length;
};

std::vector<Port> make_ports(const SheetChartShapeSpec& spec, const std::vector<Embedding>& embeddings) {
  std::vector<Port> result;
  std::set<std::uint32_t> tokens;
  std::array<Vertices, 2> vertices{Vertices(spec.tolerance), Vertices(spec.tolerance)};
  for (const auto& input : spec.ports) {
    if (!tokens.insert(input.token).second || input.first.boundary.domain == input.second.boundary.domain) {
      throw std::invalid_argument("sheet port has duplicate identity or joins its own domain");
    }
    const double low = std::min(input.axial_interval.first, input.axial_interval.last);
    const double high = std::max(input.axial_interval.first, input.axial_interval.last);
    detail::require_finite("sheet port axial start", low);
    detail::require_finite("sheet port axial end", high);
    if (high - low <= spec.tolerance) throw std::invalid_argument("sheet port has zero width");
    Port port{&input, {}, {}, {0, 0}, high - low};
    const std::array<const ShapePortSide*, 2> sides{&input.first, &input.second};
    for (int side = 0; side < 2; ++side) {
      std::size_t p = 0;
      const auto& ref = *sides[side];
      const auto& edge = boundary(spec, ref.boundary, p);
      if (embeddings[p].fanned()) throw std::invalid_argument("sheet ports cannot join a conical panel");
      const auto curve = detail::make_sheet_curve(edge.curve);
      if (edge.curve.geometry.kind != SegmentKind::Line) throw std::invalid_argument("sheet tangent ports must be straight");
      detail::require_finite("sheet port axial scale", ref.axial.scale);
      detail::require_finite("sheet port axial offset", ref.axial.offset);
      if (std::abs(ref.axial.scale) <= Precision::PConfusion()) throw std::invalid_argument("sheet port has a singular axial map");
      const double t0 = (low - ref.axial.offset) / ref.axial.scale;
      const double t1 = (high - ref.axial.offset) / ref.axial.scale;
      if (std::holds_alternative<CylinderEmbedding>(embeddings[p].patch->embedding)) {
        const auto a = curve.curve->Value(curve.native_parameter(t0));
        const auto b = curve.curve->Value(curve.native_parameter(t1));
        if (std::abs(a.X() - b.X()) > spec.tolerance) {
          throw std::invalid_argument("sheet cylindrical tangent ports must follow the bend axis");
        }
      }
      detail::require_finite("sheet port range start", ref.span.first);
      detail::require_finite("sheet port range end", ref.span.last);
      const double span_low = std::min(ref.span.first, ref.span.last), span_high = std::max(ref.span.first, ref.span.last);
      if (std::abs(std::min(t0, t1) - span_low) > Precision::PConfusion() ||
          std::abs(std::max(t0, t1) - span_high) > Precision::PConfusion() ||
          span_low < std::min(edge.curve.range.first, edge.curve.range.last) - Precision::PConfusion() ||
          span_high > std::max(edge.curve.range.first, edge.curve.range.last) + Precision::PConfusion()) {
        throw std::invalid_argument("sheet port range does not match its common axial interval");
      }
      for (int back = 0; back < 2; ++back) for (int end = 0; end < 2; ++end) {
        const double parameter = curve.native_parameter(end == 0 ? t0 : t1);
        const gp_Pnt point = embeddings[p].point(curve.curve->Value(parameter), back != 0);
        if (side == 0) port.points[back][end] = point;
        else if (point.Distance(port.points[back][end]) > spec.tolerance) {
          throw std::invalid_argument("sheet port material sides do not meet at their shared coordinates");
        }
      }
      for (int back = 0; back < 2; ++back) {
        if (std::abs(port.points[back][0].Distance(port.points[back][1]) - port.length) > spec.tolerance) {
          throw std::invalid_argument("sheet port axial map is not an isometry");
        }
      }
    }
    for (int back = 0; back < 2; ++back) {
      BRepBuilderAPI_MakeEdge maker(vertices[back].at(port.points[back][0]), vertices[back].at(port.points[back][1]));
      if (!maker.IsDone()) throw std::runtime_error("sheet shared port edge construction failed");
      port.edges[back] = maker.Edge();
    }
    result.push_back(std::move(port));
  }
  return result;
}

Curve2Ptr line_uv(const gp_Pnt2d& first, const gp_Pnt2d& last, double length) {
  NCollection_Array1<gp_Pnt2d> poles(1, 2); poles(1) = first; poles(2) = last;
  NCollection_Array1<double> knots(1, 2); knots(1) = 0; knots(2) = length;
  NCollection_Array1<int> mults(1, 2); mults(1) = mults(2) = 2;
  return new Geom2d_BSplineCurve(poles, knots, mults, 1);
}

struct NamedFace {
  TopoDS_Face face;
  std::string name;
  std::size_t member;
};

void region_faces(const MaterialRegion& input, const Embedding& embedding, std::size_t member,
                  std::vector<Port>& ports, double tolerance, std::vector<NamedFace>& faces) {
  const auto region = normalized(input, tolerance);
  std::array<std::vector<TopoDS_Wire>, 2> wires;
  std::array<Vertices, 2> vertices{Vertices(tolerance), Vertices(tolerance)};
  // An exact tangency may connect an exterior and a hole at one vertex.
  // Preserve that topology across all loops, including prebuilt port edges.
  for (const auto& port : ports) {
    if (port.input->first.boundary.domain != embedding.patch->domain &&
        port.input->second.boundary.domain != embedding.patch->domain) continue;
    for (int back = 0; back < 2; ++back) {
      TopoDS_Vertex first, last;
      TopExp::Vertices(port.edges[back], first, last);
      vertices[back].add(first); vertices[back].add(last);
    }
  }
  for (const auto& loop : region.loops) {
    std::array<BRepBuilderAPI_MakeWire, 2> wire;
    for (const auto& edge : loop.edges) {
      throw_if_cancelled();
      const auto curve = detail::make_sheet_curve(edge.curve);
      if (const auto* cylinder = std::get_if<CylinderEmbedding>(&embedding.patch->embedding)) {
        Bnd_Box2d bounds;
        BndLib_Add2dCurve::AddOptimal(curve.curve, std::min(curve.native_range.first, curve.native_range.last),
                                     std::max(curve.native_range.first, curve.native_range.last), 0, bounds);
        double u0, v0, u1, v1;
        bounds.Get(u0, v0, u1, v1);
        if (u0 < -tolerance || u1 > cylinder->neutral_radius * cylinder->angle_extent + tolerance) {
          throw std::invalid_argument("sheet material curve lies outside the cylinder angle interval");
        }
      }
      const double low = std::min(edge.curve.range.first, edge.curve.range.last);
      const double high = std::max(edge.curve.range.first, edge.curve.range.last);
      std::vector<double> splits{low, high};
      std::vector<std::pair<Port*, const ShapePortSide*>> uses;
      for (auto& port : ports) for (const auto* side : {&port.input->first, &port.input->second}) {
        if (side->boundary.domain == embedding.patch->domain && side->boundary.piece == edge.piece) {
          uses.emplace_back(&port, side);
          splits.push_back(std::min(side->span.first, side->span.last));
          splits.push_back(std::max(side->span.first, side->span.last));
        }
      }
      std::sort(splits.begin(), splits.end());
      splits.erase(std::unique(splits.begin(), splits.end(), [](double a, double b) {
        return std::abs(a - b) <= Precision::PConfusion();
      }), splits.end());
      const bool reverse = edge.curve.range.last < edge.curve.range.first;
      if (reverse) std::reverse(splits.begin(), splits.end());
      for (std::size_t k = 1; k < splits.size(); ++k) {
        const double first = splits[k - 1], last = splits[k], middle = (first + last) / 2;
        Port* shared = nullptr;
        const ShapePortSide* side = nullptr;
        for (const auto& use : uses) {
          if (middle > std::min(use.second->span.first, use.second->span.last) &&
              middle < std::max(use.second->span.first, use.second->span.last)) {
            if (shared) throw std::invalid_argument("sheet port intervals overlap");
            shared = use.first; side = use.second;
          }
        }
        if (shared) {
          const double axial_first = side->axial.offset + side->axial.scale * first;
          const double axial_last = side->axial.offset + side->axial.scale * last;
          const bool reversed = axial_last < axial_first;
          const gp_Pnt2d a = embedding.uv(curve.curve->Value(curve.native_parameter(reversed ? last : first)));
          const gp_Pnt2d b = embedding.uv(curve.curve->Value(curve.native_parameter(reversed ? first : last)));
          for (int back = 0; back < 2; ++back) {
            BRep_Builder builder;
            builder.UpdateEdge(shared->edges[back], line_uv(a, b, shared->length), embedding.surfaces[back],
                               TopLoc_Location(), tolerance);
            builder.Range(shared->edges[back], embedding.surfaces[back], TopLoc_Location(), 0, shared->length);
            TopoDS_Edge current = shared->edges[back];
            if (reversed) current.Reverse();
            wire[back].Add(current);
            ++shared->uses[back];
          }
        } else {
          const double native_first = curve.native_parameter(first), native_last = curve.native_parameter(last);
          const Rails rails = embedding.fanned()
                                  ? cone_rails(curve.curve, std::min(native_first, native_last),
                                               std::max(native_first, native_last), embedding, tolerance)
                                  : make_rails(bounded_spline(curve, {native_first, native_last}), embedding, tolerance);
          for (int back = 0; back < 2; ++back) {
            TopoDS_Edge current = cap_edge(rails.rail[back], rails.uv, embedding.surfaces[back], tolerance, vertices[back]);
            if (reverse) current.Reverse();
            wire[back].Add(current);
          }
          faces.push_back({wall(rails, (reverse ? -1 : 1) * embedding.material_sign < 0, tolerance),
                           side_name(embedding.patch->domain, edge.piece), member});
        }
      }
    }
    for (int back = 0; back < 2; ++back) {
      if (!wire[back].IsDone()) throw std::runtime_error("sheet cap wire is disconnected");
      TopoDS_Wire current = wire[back].Wire();
      if (embedding.wire_sign < 0) current.Reverse();
      wires[back].push_back(current);
    }
  }
  for (int back = 0; back < 2; ++back) {
    BRepBuilderAPI_MakeFace maker(embedding.surfaces[back], wires[back].front(), true);
    for (std::size_t i = 1; i < wires[back].size(); ++i) maker.Add(wires[back][i]);
    if (!maker.IsDone()) throw std::runtime_error("sheet cap construction failed");
    TopoDS_Face cap = maker.Face();
    if (embedding.reverse_cap(back != 0)) cap.Reverse();
    faces.push_back({cap, cap_name(embedding.patch->domain, back != 0), member});
  }
}

// Panels of one plane and material direction whose regions touch along a
// boundary that is not a registered port (a curved seam between panels of
// one Face, which ports, being straight, cannot pair) form one material, as
// the base Face's union makes them (mitcad#242): solids that touch only
// through such panels are fused, their memberships joined.
void fuse_coplanar(SheetChartGeometry& result, const SheetChartShapeSpec& spec,
                   const std::vector<Embedding>& embeddings) {
  const auto plane_of = [&](const RegionMembership& m) -> const PlaneEmbedding* {
    for (std::size_t p = 0; p < spec.patches.size(); ++p) {
      if (domain_key(spec.patches[p].domain) != domain_key(m.domain)) continue;
      if (spec.patches[p].domain.kind != SheetDomainKind::Panel) return nullptr;
      return std::get_if<PlaneEmbedding>(&embeddings[p].patch->embedding);
    }
    return nullptr;
  };
  const auto coplanar = [&](const PlaneEmbedding* a, const PlaneEmbedding* b) {
    if (!a || !b) return false;
    const gp_Vec fa = parity(a->front_to_back_sign) * gp_Vec(a->front_frame.normal());
    const gp_Vec fb = parity(b->front_to_back_sign) * gp_Vec(b->front_frame.normal());
    return fa.IsEqual(fb, spec.tolerance, kAngularTolerance) &&
           std::abs(gp_Vec(a->front_frame.origin, b->front_frame.origin).Dot(fa)) <= spec.tolerance;
  };
  bool merged = true;
  while (merged) {
    merged = false;
    for (std::size_t i = 0; i < result.solids.size() && !merged; ++i) {
      for (std::size_t j = i + 1; j < result.solids.size() && !merged; ++j) {
        bool shares_plane = false;
        for (const auto& a : result.solids[i].membership) {
          for (const auto& b : result.solids[j].membership) {
            shares_plane = shares_plane || coplanar(plane_of(a), plane_of(b));
          }
        }
        if (!shares_plane) continue;
        detail::Interrupt interrupt;
        BRepExtrema_DistShapeShape distance(result.solids[i].shape->occt(), result.solids[j].shape->occt(),
                                            Extrema_ExtFlag_MINMAX, Extrema_ExtAlgo_Grad, interrupt.range());
        throw_if_cancelled();
        if (!distance.IsDone() || distance.Value() > spec.tolerance) continue;
        const double parts = volume(*result.solids[i].shape) + volume(*result.solids[j].shape);
        const ShapePtr joined = detail::finished(
            *detail::fused({result.solids[i].shape, result.solids[j].shape}, "sheet chart shape"),
            "sheet chart shape");
        if (solids(*joined).size() != 1) continue;
        if (std::abs(volume(*joined) - parts) > 1.0e-9 * std::max(parts, 1.0)) {
          throw std::invalid_argument("sheet material regions overlap");
        }
        result.solids[i].shape = joined;
        result.solids[i].membership.insert(result.solids[i].membership.end(),
                                           result.solids[j].membership.begin(),
                                           result.solids[j].membership.end());
        result.solids.erase(result.solids.begin() + static_cast<std::ptrdiff_t>(j));
        merged = true;
      }
    }
  }
}

bool legacy_planar(const SheetChartShapeSpec& spec) {
  if (!spec.ports.empty()) return false;
  const PlaneEmbedding* first = nullptr;
  for (const auto& patch : spec.patches) {
    const auto* plane = std::get_if<PlaneEmbedding>(&patch.embedding);
    if (!plane || plane->front_to_back_sign != MaterialParity::Positive || patch.regions.size() != 1 ||
        patch.domain.kind != SheetDomainKind::Panel) return false;
    if (!first) first = plane;
    else if (!plane->front_frame.normal().IsEqual(first->front_frame.normal(), kAngularTolerance) ||
             std::abs(gp_Vec(first->front_frame.origin, plane->front_frame.origin).Dot(gp_Vec(first->front_frame.normal()))) > spec.tolerance) return false;
  }
  return first != nullptr;
}

SheetChartGeometry planar_adapter(const SheetChartShapeSpec& spec) {
  SheetShapeSpec legacy;
  legacy.thickness = spec.thickness;
  legacy.reference_side = SheetReferenceSide::Front;
  legacy.tolerance = spec.tolerance;
  for (const auto& patch : spec.patches) {
    Region region;
    for (const auto& boundary : normalized(patch.regions.front(), spec.tolerance).loops) {
      Loop loop;
      for (const auto& edge : boundary.edges) {
        const auto source = detail::make_sheet_curve(edge.curve);
        const ParamSpan range{std::min(source.native_range.first, source.native_range.last),
                              std::max(source.native_range.first, source.native_range.last)};
        auto support = source.curve;
        if (const auto spline = Curve2Ptr::DownCast(support)) {
          const auto copy = Curve2Ptr::DownCast(spline->Copy());
          copy->Segment(range.first, range.last);
          support = copy;
        }
        Segment segment = detail::export_sheet_curve(support, range).geometry;
        segment.name = edge.piece.curve;
        loop.segments.push_back(std::move(segment));
      }
      region.loops.push_back(std::move(loop));
    }
    legacy.planar_patches.push_back({patch.domain.feature, patch.domain.slot,
                                    std::get<PlaneEmbedding>(patch.embedding).front_frame, std::move(region)});
  }
  const auto constructed = sheet_shape(legacy);
  SheetChartGeometry result;
  result.diagnostics = constructed.diagnostics;
  for (const auto& solid : constructed.solids) {
    SheetChartSolid output{solid.shape, {}};
    for (const auto index : solid.domains) output.membership.push_back({spec.patches.at(index).domain, 0});
    result.solids.push_back(std::move(output));
  }
  return result;
}

// The spec with the holes of its regions filled: the interior loops none of
// whose edges carries a port (mitcad#288). Its material holds the spec's,
// so where its charts do not intersect, the spec's do not either. Empty
// when no region has such a hole.
std::optional<SheetChartShapeSpec> holes_filled(const SheetChartShapeSpec& spec) {
  SheetChartShapeSpec filled = spec;
  bool any = false;
  for (auto& patch : filled.patches) {
    for (auto& region : patch.regions) {
      if (region.loops.size() < 2) continue;
      std::vector<MaterialLoop> kept{region.loops.front()};
      for (std::size_t l = 1; l < region.loops.size(); ++l) {
        const auto& loop = region.loops[l];
        const bool ported = std::any_of(loop.edges.begin(), loop.edges.end(), [&](const MaterialEdge& edge) {
          return std::any_of(spec.ports.begin(), spec.ports.end(), [&](const ShapePort& port) {
            return (port.first.boundary.domain == patch.domain && port.first.boundary.piece == edge.piece) ||
                   (port.second.boundary.domain == patch.domain && port.second.boundary.piece == edge.piece);
          });
        });
        if (ported) kept.push_back(loop);
        else any = true;
      }
      region.loops = std::move(kept);
    }
  }
  if (!any) return std::nullopt;
  return filled;
}

// How a construction checks the sheet's walls for collisions: Checked
// (walls that meet refused, naming them), Unchecked (a construction with
// its holes filled found none), Clearance (that construction with its
// holes filled: it builds and checks the shells only, and any collision or
// invalid shell throws without naming walls, since only whether there is
// one decides, mitcad#2536).
enum class Pass { Checked, Unchecked, Clearance };

SheetChartGeometry construct(const SheetChartShapeSpec& spec, Pass pass);

// Whether MITCAD_LOG_TIMING (not 0) asks for a line per sheet construction
// on standard error (mitcad#2444): where a sheet feature's time goes.
bool log_timing() {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_LOG_TIMING") != 0 || value == nullptr) {
      return false;
    }
    const bool set = std::string(value) != "0";
    std::free(value);
    return set;
#else
    const char* value = std::getenv("MITCAD_LOG_TIMING");
    return value != nullptr && std::string(value) != "0";
#endif
  }();
  return on;
}

// The phases of one construction, each the time since the one before
// (log_timing()), printed as one line when it ends:
// `Timing sheet chart shape: 9 charts, 58 faces, faces 2.1 ms, sew 14.0 ms, ...`,
// with how the shells' walls were found not to meet: kept apart by planes or
// by OCCT's self-interference check.
class ShapeTiming {
public:
  enum Phase { Faces, Sew, Valid, Collisions, Names, Fuse, Apart, PhaseCount };

  ShapeTiming() : m_start(Clock::now()), m_last(m_start) {}

  void mark(Phase phase) {
    if (!log_timing()) return;
    const auto now = Clock::now();
    m_ms[static_cast<std::size_t>(phase)] += std::chrono::duration<double, std::milli>(now - m_last).count();
    m_last = now;
  }

  // A shell of `regions` material regions, `suspects` of them not kept
  // apart by planes from the rest and checked (none: the shell's walls kept
  // apart; `unshown`: the first pair of regions not shown apart).
  void shell(std::size_t suspects, std::size_t regions, const std::string& unshown) {
    const bool apart = suspects == 0;
    ++(apart ? m_apart : m_checked);
    m_suspects += suspects;
    m_regions += regions;
    if (!apart && m_unshown.empty()) m_unshown = unshown;
  }

  // `filled`: the construction with the holes filled, which checks the
  // shells only (mitcad#2536).
  void print(std::size_t charts, std::size_t faces, bool filled = false) const {
    if (!log_timing()) return;
    static const char* const names[PhaseCount] = {"faces", "sew", "valid", "collisions", "names", "fuse", "apart"};
    std::string line = std::string("Timing sheet chart shape") + (filled ? " (holes filled)" : "") + ": " +
                       std::to_string(charts) + " charts, " + std::to_string(faces) + " faces";
    char buffer[64];
    for (std::size_t i = 0; i < m_ms.size(); ++i) {
      std::snprintf(buffer, sizeof buffer, ", %s %.2f ms", names[i], m_ms[i]);
      line += buffer;
    }
    std::snprintf(buffer, sizeof buffer, ", total %.2f ms",
                  std::chrono::duration<double, std::milli>(Clock::now() - m_start).count());
    line += buffer;
    if (m_apart + m_checked == 0) {
      line += " (collisions not checked)\n";
    } else {
      line += " (shells kept apart by planes " + std::to_string(m_apart) + ", checked for collisions " +
              std::to_string(m_checked) +
              (m_checked == 0 ? std::string()
                              : ", " + std::to_string(m_suspects) + " of " + std::to_string(m_regions) +
                                    " regions checked") +
              (m_unshown.empty() ? std::string() : ": not shown apart " + m_unshown) + ")\n";
    }
    std::fputs(line.c_str(), stderr);
  }

private:
  using Clock = std::chrono::steady_clock;
  Clock::time_point m_start;
  Clock::time_point m_last;
  std::array<double, PhaseCount> m_ms{};
  int m_apart = 0;
  int m_checked = 0;
  std::size_t m_suspects = 0;
  std::size_t m_regions = 0;
  std::string m_unshown;
};

// A domain as the model names it in messages: `panel F4 slot 0`.
std::string domain_label(const SheetDomainRef& d) {
  return std::string(d.kind == SheetDomainKind::Panel ? "panel " : "bend ") + d.feature + " slot " +
         std::to_string(d.slot);
}

// One material region on its own: its caps, and walls all round it, also
// where it shares a port with a neighbour. Empty when it does not close.
std::optional<TopoDS_Solid> region_solid(const MaterialRegion& region, const Embedding& embedding, double tolerance) {
  std::vector<Port> none;
  std::vector<NamedFace> faces;
  region_faces(region, embedding, 0, none, tolerance, faces);
  BRepBuilderAPI_Sewing sewing(tolerance);
  sewing.SetMaxTolerance(tolerance);
  for (const auto& face : faces) sewing.Add(face.face);
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  if (sewing.NbFreeEdges() != 0 || sewing.NbMultipleEdges() != 0) return std::nullopt;
  TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL);
  if (!it.More()) return std::nullopt;
  const auto shell = TopoDS::Shell(it.Current());
  it.Next();
  if (it.More() || !BRep_Tool::IsClosed(shell)) return std::nullopt;
  BRepBuilderAPI_MakeSolid maker(shell);
  if (!maker.IsDone()) return std::nullopt;
  TopoDS_Solid solid = maker.Solid();
  if (!BRepLib::OrientClosedSolid(solid)) return std::nullopt;
  return solid;
}

TopoDS_Shape common_of(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> objects, tools;
  objects.Append(a);
  tools.Append(b);
  common.SetArguments(objects);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  detail::build(common);
  if (!common.IsDone() || common.HasErrors()) {
    throw std::invalid_argument("sheet material charts intersect in the current configuration");
  }
  return common.Shape();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps properties;
  BRepGProp::SurfaceProperties(shape, properties);
  return properties.Mass();
}

// Two walls of one sheet that meet where no port joins them.
struct WallMeeting {
  std::size_t first, second;
  bool shares_volume;  // else they touch over a face
};

// Where the shell of one sheet meets itself (the self-interference check
// found faces that meet, or the checker refused the solid): the two
// material regions that meet in a way one solid cannot hold. First a pair
// whose material shares volume, more than the tolerance deep over a square
// of the thickness (a common volume above `tolerance t^2`); else a pair
// not joined by a port whose surfaces touch over a face (a common area
// above `tolerance t`), such as a half open corner, whose solid the
// checker refuses (mitcad#1409). Walls that touch only along a line or at
// a point (a full open corner closed by welding at the exact inside
// dimension) give none: the sheet keeps them as they are, neither fused
// nor their faces merged, the corner line an edge of each. Throws the
// collision refusal when a region does not close on its own, meets itself
// or a common fails, as nothing then tells contact from penetration. Pairs
// are searched from the latest region back (the walls a feature added come
// last), so a refusal names the new wall and the wall it meets.
std::optional<WallMeeting> meeting_walls(const SheetChartShapeSpec& spec, const std::vector<Embedding>& embeddings,
                                         const std::vector<RegionMembership>& membership,
                                         const std::vector<std::size_t>& member_patch,
                                         const std::vector<std::size_t>& members) {
  const auto refused = [] { return std::invalid_argument("sheet material charts intersect in the current configuration"); };
  std::vector<TopoDS_Solid> solids;
  std::vector<Bnd_Box> boxes;
  for (const std::size_t m : members) {
    const auto& patch = spec.patches.at(member_patch[m]);
    const auto solid = region_solid(patch.regions.at(membership[m].region), embeddings[member_patch[m]], spec.tolerance);
    if (!solid) throw refused();
    BRepAlgoAPI_Check itself;
    itself.SetData(*solid, false, true);
    detail::interruptible([&](const Message_ProgressRange& range) { itself.Perform(range); });
    if (itself.HasErrors() || !itself.IsValid()) throw refused();
    Bnd_Box box;
    BRepBndLib::Add(*solid, box);
    box.Enlarge(spec.tolerance);
    solids.push_back(*solid);
    boxes.push_back(box);
  }
  const auto joined = [&](std::size_t a, std::size_t b) {
    const auto key_a = domain_key(membership[a].domain), key_b = domain_key(membership[b].domain);
    return std::any_of(spec.ports.begin(), spec.ports.end(), [&](const ShapePort& port) {
      const auto first = domain_key(port.first.boundary.domain), second = domain_key(port.second.boundary.domain);
      return (first == key_a && second == key_b) || (first == key_b && second == key_a);
    });
  };
  // The latest region first (mitcad#2445): a feature appends its new walls
  // to the charts, and the sheet before it held, so the pair found first
  // is the new wall and the one it meets, not two walls that were there.
  std::optional<WallMeeting> touching;
  for (std::size_t j = solids.size(); j-- > 1;) for (std::size_t i = 0; i < j; ++i) {
    if (boxes[i].IsOut(boxes[j])) continue;
    if (detail::volume_of(common_of(solids[i], solids[j])) > spec.tolerance * spec.thickness * spec.thickness) {
      return WallMeeting{members[i], members[j], true};
    }
    if (touching || joined(members[i], members[j])) continue;
    TopExp_Explorer a(solids[i], TopAbs_SHELL), b(solids[j], TopAbs_SHELL);
    if (area_of(common_of(a.Current(), b.Current())) > spec.tolerance * spec.thickness) {
      touching = WallMeeting{members[i], members[j], false};
    }
  }
  return touching;
}

// The refusal of two walls `meeting_walls` found, in the domains' terms.
std::invalid_argument meeting_refusal(const WallMeeting& meeting, const std::vector<RegionMembership>& membership) {
  const auto first = domain_label(membership[meeting.first].domain);
  const auto second = domain_label(membership[meeting.second].domain);
  if (meeting.shares_volume) {
    return std::invalid_argument("sheet walls " + first + " and " + second + " pass through each other");
  }
  return std::invalid_argument("sheet walls " + first + " and " + second +
                               " touch over a face, which a sheet body cannot hold: leave a seam gap between them");
}

// Walls that cannot meet (mitcad#2444). OCCT's self-interference check of a
// folded sheet intersects every pair of its faces whose boxes meet, and the
// sides of the charts are ruled B-spline faces: it took 90 to 95 percent of
// a Flange's time on a plain plate (0.12 s for one flange, 0.55 s for a box
// of four in a debug build), more with every wall. Most pairs of material
// regions are kept apart by a plane that is easy to find: a cap's plane, the
// plane of a straight side, a bend's end plane, the plane through a port or
// the bisecting plane of a corner between two bends. When such a plane separates every pair of a shell's
// regions, the check could find nothing and is not run; otherwise it runs
// on the faces of the regions such planes do not keep apart from the rest
// (mitcad#2536). Each region is bounded by its exact support function (the
// largest of m . x over its material): exact for straight and circular
// sides, over the poles of the exact B-spline form (which hold the curve)
// for other curves. A region on its own was checked by the validity check.

// The largest of A cos a + B sin a for a in [lo, hi].
double turn_max(double a_coefficient, double b_coefficient, double lo, double hi) {
  double best = std::max(a_coefficient * std::cos(lo) + b_coefficient * std::sin(lo),
                         a_coefficient * std::cos(hi) + b_coefficient * std::sin(hi));
  const double peak = std::atan2(b_coefficient, a_coefficient);
  const double turns = std::ceil((lo - peak) / (2 * kPi));
  if (peak + turns * 2 * kPi <= hi) best = std::max(best, std::hypot(a_coefficient, b_coefficient));
  return best;
}

// One side of a region's outline in its chart, as the support function
// reads it: a straight side by its two ends, a circular arc exactly, another
// curve by the poles of its exact B-spline form (positive weights: they hold
// the curve).
struct ChartEdge {
  bool line = false;
  std::vector<gp_Pnt2d> points;
  bool circular = false;
  gp_Pnt2d centre;
  gp_Vec2d x, y;
  double radius = 0, lo = 0, hi = 0;
  // A curved side of a bend's chart in short pieces, each by its chart box
  // (u from, u to, y from, y to): a relief's arc ends a bend where its box
  // as a whole would reach the corner the relief took away.
  std::vector<std::array<double, 4>> boxes;
};

// How many pieces a curved side of a bend's chart is bounded in.
constexpr int kBendSidePieces = 32;

// The largest of w . p over a side.
double edge_support(const ChartEdge& edge, const gp_Vec2d& w) {
  double best = -std::numeric_limits<double>::infinity();
  for (const auto& p : edge.points) best = std::max(best, gp_Vec2d(p.XY()).Dot(w));
  if (edge.circular) {
    best = std::max(best, gp_Vec2d(edge.centre.XY()).Dot(w) +
                              edge.radius * turn_max(edge.x.Dot(w), edge.y.Dot(w), edge.lo, edge.hi));
  }
  return best;
}

// A region's outline (its exterior loop), with what may separate it from
// another region.
struct RegionHull {
  const Embedding* embedding = nullptr;
  std::vector<ChartEdge> edges;
  // A bend's chart box: u along the turn (at the neutral radius), y along the axis.
  double u0 = 0, u1 = 0, y0 = 0, y1 = 0;
  // Unit normals of the planes that may separate it from another region.
  std::vector<gp_Vec> directions;
};

// The largest of w . p over a region's outline in its chart.
double chart_support(const RegionHull& hull, const gp_Vec2d& w) {
  double best = -std::numeric_limits<double>::infinity();
  for (const auto& edge : hull.edges) best = std::max(best, edge_support(edge, w));
  return best;
}

// The largest of along * y + d * (a cos(u / neutral) + b sin(u / neutral))
// over a side (u, y) of a bend's chart, at the radius d: exact along a
// straight side (its ends and the points where the derivative is zero),
// over the side's own chart box for another curve.
double bend_edge_max(const ChartEdge& edge, double along, double a, double b, double neutral, double d) {
  const auto at = [&](double u, double y) { return along * y + d * (a * std::cos(u / neutral) + b * std::sin(u / neutral)); };
  if (!edge.line) {
    double best = -std::numeric_limits<double>::infinity();
    for (const auto& [ua, ub, ya, yb] : edge.boxes) {
      best = std::max(best, std::max(along * ya, along * yb) + d * turn_max(a, b, ua / neutral, ub / neutral));
    }
    return best;
  }
  const gp_Pnt2d p = edge.points[0], q = edge.points[1];
  double best = std::max(at(p.X(), p.Y()), at(q.X(), q.Y()));
  const double du = q.X() - p.X();
  const double rho = std::hypot(a, b);
  // Along the axis (u fixed) or with no turning part, linear: its ends.
  if (std::abs(du) <= Precision::PConfusion() || rho <= 0) return best;
  const double slope = (q.Y() - p.Y()) / du;
  const double lo = std::min(p.X(), q.X()), hi = std::max(p.X(), q.X());
  // F(u) = along (p.y + slope (u - p.x)) + d rho cos(u / neutral - phi):
  // F'(u) = 0 where sin(u / neutral - phi) = along slope neutral / (d rho).
  const double s = along * slope * neutral / (d * rho);
  if (std::abs(s) <= 1) {
    const double phi = std::atan2(b, a);
    const double period = 2 * kPi * neutral;
    for (const double theta : {std::asin(s), kPi - std::asin(s)}) {
      const double base = neutral * (theta + phi);
      for (double u = base + std::ceil((lo - base) / period) * period; u <= hi; u += period) {
        best = std::max(best, at(u, p.Y() + slope * (u - p.X())));
      }
    }
  }
  return best;
}

// The largest of m . x over a region's material. A bend's: for a fixed u
// the material's slice is linear in y and the radius, so the most is on the
// outline, at the inside or the outside radius.
double support(const RegionHull& hull, const gp_Vec& m) {
  const auto& patch = *hull.embedding->patch;
  const double thickness = hull.embedding->thickness;
  if (const auto* plane = std::get_if<PlaneEmbedding>(&patch.embedding)) {
    const Frame& f = plane->front_frame;
    const gp_Vec2d w(m.Dot(gp_Vec(f.x_axis)), m.Dot(gp_Vec(f.y_axis)));
    const double across = thickness * parity(plane->front_to_back_sign) * m.Dot(gp_Vec(f.normal()));
    return m.Dot(gp_Vec(f.origin.XYZ())) + chart_support(hull, w) + std::max(0.0, across);
  }
  const auto& c = std::get<CylinderEmbedding>(patch.embedding);
  const double along = m.Dot(gp_Vec(c.axis));
  const double a = m.Dot(gp_Vec(c.radial_start)), b = m.Dot(gp_Vec(c.angular_direction));
  double best = -std::numeric_limits<double>::infinity();
  for (const double d : {c.inside_radius, c.inside_radius + thickness}) {
    for (const auto& edge : hull.edges) best = std::max(best, bend_edge_max(edge, along, a, b, c.neutral_radius, d));
  }
  return m.Dot(gp_Vec(c.centre.XYZ())) + best;
}

RegionHull region_hull(const MaterialRegion& region, const Embedding& embedding) {
  if (region.loops.empty() || region.loops.front().edges.empty()) {
    throw std::invalid_argument("sheet material region has no outline");
  }
  RegionHull hull;
  hull.embedding = &embedding;
  std::vector<gp_Vec2d> normals;
  const bool bend = std::holds_alternative<CylinderEmbedding>(embedding.patch->embedding);
  for (const auto& edge : region.loops.front().edges) {
    const auto curve = detail::make_sheet_curve(edge.curve);
    const double lo = std::min(curve.native_range.first, curve.native_range.last);
    const double hi = std::max(curve.native_range.first, curve.native_range.last);
    occ::handle<Geom2d_Curve> basis = curve.curve;
    if (const auto trimmed = occ::handle<Geom2d_TrimmedCurve>::DownCast(basis)) basis = trimmed->BasisCurve();
    ChartEdge side;
    if (!occ::handle<Geom2d_Line>::DownCast(basis).IsNull()) {
      const gp_Pnt2d a = curve.curve->Value(lo), b = curve.curve->Value(hi);
      side.line = true;
      side.points = {a, b};
      const gp_Vec2d along(a, b);
      if (along.Magnitude() > Precision::Confusion()) normals.push_back(gp_Vec2d(along.Y(), -along.X()).Normalized());
    } else if (const auto circle = occ::handle<Geom2d_Circle>::DownCast(basis); !circle.IsNull()) {
      const gp_Circ2d c = circle->Circ2d();
      side.circular = true;
      side.centre = c.Location();
      side.x = gp_Vec2d(c.XAxis().Direction());
      side.y = gp_Vec2d(c.YAxis().Direction());
      side.radius = c.Radius();
      side.lo = lo;
      side.hi = hi;
    } else {
      const auto spline = bounded_spline(curve, {lo, hi});
      for (int i = 1; i <= spline->NbPoles(); ++i) {
        if (spline->Weight(i) <= 0) throw std::invalid_argument("sheet curve has a weight that is not positive");
        side.points.push_back(spline->Pole(i));
      }
    }
    if (bend && !side.line) {
      for (int k = 0; k < kBendSidePieces; ++k) {
        const double from = lo + (hi - lo) * k / kBendSidePieces;
        const double to = lo + (hi - lo) * (k + 1) / kBendSidePieces;
        ChartEdge piece;
        if (side.circular) {
          piece.circular = true;
          piece.centre = side.centre;
          piece.x = side.x;
          piece.y = side.y;
          piece.radius = side.radius;
          piece.lo = from;
          piece.hi = to;
        } else {
          const auto spline = bounded_spline(curve, {from, to});
          for (int i = 1; i <= spline->NbPoles(); ++i) {
            if (spline->Weight(i) <= 0) throw std::invalid_argument("sheet curve has a weight that is not positive");
            piece.points.push_back(spline->Pole(i));
          }
        }
        side.boxes.push_back({-edge_support(piece, gp_Vec2d(-1, 0)), edge_support(piece, gp_Vec2d(1, 0)),
                              -edge_support(piece, gp_Vec2d(0, -1)), edge_support(piece, gp_Vec2d(0, 1))});
      }
    }
    hull.edges.push_back(std::move(side));
  }
  const auto& patch = *embedding.patch;
  if (const auto* plane = std::get_if<PlaneEmbedding>(&patch.embedding)) {
    const Frame& f = plane->front_frame;
    hull.directions.push_back(gp_Vec(f.normal()));
    for (const auto& n : normals) hull.directions.push_back(n.X() * gp_Vec(f.x_axis) + n.Y() * gp_Vec(f.y_axis));
  } else {
    const auto& c = std::get<CylinderEmbedding>(patch.embedding);
    hull.u0 = -chart_support(hull, gp_Vec2d(-1, 0));
    hull.u1 = chart_support(hull, gp_Vec2d(1, 0));
    hull.y0 = -chart_support(hull, gp_Vec2d(0, -1));
    hull.y1 = chart_support(hull, gp_Vec2d(0, 1));
    hull.directions.push_back(gp_Vec(c.axis));
    for (const double u : {hull.u0, hull.u1}) {
      const double a = u / c.neutral_radius;
      hull.directions.push_back(-std::sin(a) * gp_Vec(c.radial_start) + std::cos(a) * gp_Vec(c.angular_direction));
    }
  }
  for (const gp_Vec axis : {gp_Vec(1, 0, 0), gp_Vec(0, 1, 0), gp_Vec(0, 0, 1)}) hull.directions.push_back(axis);
  return hull;
}

// Whether a plane keeps two regions apart: their material on its two sides
// at least `gap` apart (a negative gap: overlapping at most by its size).
// The planes tried are the two regions' own; the planes halfway between
// one of each (the sum or difference of their normals: the bisecting plane
// of a corner between two bends' axes, which no side of the sheet lies in,
// as walls are cut normal to their surfaces); then those of every region of
// the shell (`shell`).
bool kept_apart(const RegionHull& a, const RegionHull& b, const std::vector<gp_Vec>& shell, double gap) {
  const auto separates = [&](const gp_Vec& direction) {
    for (const gp_Vec m : {direction, direction.Reversed()}) {
      if (support(a, m) + support(b, m.Reversed()) <= -gap) return true;
    }
    return false;
  };
  const auto apart = [&](const std::vector<gp_Vec>& directions) {
    return std::any_of(directions.begin(), directions.end(), separates);
  };
  if (apart(a.directions) || apart(b.directions)) return true;
  for (const auto& first : a.directions) {
    for (const auto& second : b.directions) {
      for (const gp_Vec halfway : {first + second, first - second}) {
        if (halfway.Magnitude() > 1.0e-9 && separates(halfway.Normalized())) return true;
      }
    }
  }
  return apart(shell);
}

// Whether a port joins two domains.
bool joined_domains(const SheetChartShapeSpec& spec, const SheetDomainRef& a, const SheetDomainRef& b) {
  const auto key_a = domain_key(a), key_b = domain_key(b);
  return std::any_of(spec.ports.begin(), spec.ports.end(), [&](const ShapePort& port) {
    const auto first = domain_key(port.first.boundary.domain), second = domain_key(port.second.boundary.domain);
    return (first == key_a && second == key_b) || (first == key_b && second == key_a);
  });
}

// The material regions among `members` (one shell's) that planes do not
// keep apart from every other region of the shell, in the order of
// `members`; empty when every pair is kept apart, so that the shell cannot
// meet itself. Two regions no port joins are kept apart at least four
// tolerances apart (the self-interference check finds faces within their
// tolerances of each other, and walls a few tolerances apart are then
// refused as touching, mitcad#1409), two a port joins touching at most
// within the tolerance on the plane between them (where the port is). All
// of `members` for an outline this cannot read. `unshown` names the first
// pair not shown apart (for MITCAD_LOG_TIMING).
//
// Only these regions' faces can meet a face of another region: each other
// region's material is on its own side of a plane from each region of the
// shell (mitcad#2536). A folded box's tab through a slot of its base
// leaves the tab and the base here, and its other walls out.
std::vector<std::size_t> unshown_regions(const SheetChartShapeSpec& spec, const std::vector<Embedding>& embeddings,
                                         const std::vector<RegionMembership>& membership,
                                         const std::vector<std::size_t>& member_patch,
                                         const std::vector<std::size_t>& members, std::string& unshown) {
  try {
    std::vector<RegionHull> hulls;
    std::vector<gp_Vec> shell;
    for (const std::size_t m : members) {
      hulls.push_back(region_hull(spec.patches.at(member_patch[m]).regions.at(membership[m].region),
                                  embeddings[member_patch[m]]));
      shell.insert(shell.end(), hulls.back().directions.begin(), hulls.back().directions.end());
    }
    std::vector<bool> suspect(hulls.size(), false);
    for (std::size_t i = 0; i < hulls.size(); ++i) {
      for (std::size_t j = i + 1; j < hulls.size(); ++j) {
        // A pair whose two regions are both suspects already adds nothing.
        if (suspect[i] && suspect[j]) continue;
        const auto& first = membership[members[i]].domain;
        const auto& second = membership[members[j]].domain;
        const double gap = joined_domains(spec, first, second) ? -spec.tolerance : 4 * spec.tolerance;
        if (!kept_apart(hulls[i], hulls[j], shell, gap)) {
          if (unshown.empty()) unshown = domain_label(first) + " and " + domain_label(second);
          suspect[i] = suspect[j] = true;
        }
      }
    }
    std::vector<std::size_t> result;
    for (std::size_t i = 0; i < members.size(); ++i) {
      if (suspect[i]) result.push_back(members[i]);
    }
    return result;
  } catch (const std::exception& error) {
    unshown = std::string("an outline not read: ") + error.what();
    return members;
  } catch (...) {
    unshown = "an outline not read";
    return members;
  }
}

// Whether OCCT's self-interference check finds faces of `shape` that meet
// (or fails). The shape is a solid the validity check has passed, or a
// compound of some of its faces: the checker's own validity check of it
// (BRepAlgoAPI_Check runs one after the interferences) would only repeat
// that one (mitcad#2536).
bool self_interferes(const TopoDS_Shape& shape) {
  BOPAlgo_ArgumentAnalyzer analyzer;
  analyzer.SetShape1(shape);
  analyzer.ArgumentTypeMode() = true;
  analyzer.SelfInterMode() = true;
  analyzer.SmallEdgeMode() = false;
  analyzer.SetRunParallel(true);
  detail::interruptible([&](const Message_ProgressRange& range) { analyzer.Perform(range); });
  return analyzer.HasErrors() || analyzer.HasFaulty();
}

// How many holes of the regions a collision check would look at make
// building the sheet once more with its holes filled pay off (mitcad#288,
// mitcad#2536). Each hole adds its walls to the check, which compares every
// pair of faces whose boxes meet: hundreds (a vent pattern) took most of a
// minute. A few slots (a box's base, its tabs through them) cost the check
// less than building every face of the sheet and sewing it a second time,
// which a tab through a slot then refuses anyway (the filled base meets it).
constexpr std::size_t kHolesFilledFirst = 16;

// Whether the spec is first built with its holes filled (`filled`, from
// holes_filled): when the regions planes do not keep apart from the rest
// of their shell (unshown_regions) hold at least kHolesFilledFirst of the
// holes filled there. Not when every region is kept apart: then nothing is
// checked for collisions either way. False for a spec it cannot read; the
// construction says why.
bool holes_filled_first(const SheetChartShapeSpec& spec, const SheetChartShapeSpec& filled) {
  try {
    std::vector<Embedding> embeddings;
    std::vector<RegionMembership> membership;
    std::vector<std::size_t> member_patch;
    std::vector<std::size_t> holes;
    for (std::size_t p = 0; p < spec.patches.size(); ++p) {
      embeddings.push_back(make_embedding(spec.patches[p], spec.thickness, spec.tolerance));
      for (std::size_t r = 0; r < spec.patches[p].regions.size(); ++r) {
        membership.push_back({spec.patches[p].domain, r});
        member_patch.push_back(p);
        holes.push_back(spec.patches[p].regions[r].loops.size() - filled.patches.at(p).regions.at(r).loops.size());
      }
    }
    // The shells as the construction sews them: patches joined by ports.
    std::vector<std::size_t> group(spec.patches.size());
    for (std::size_t p = 0; p < group.size(); ++p) group[p] = p;
    const auto patch_of = [&](const SheetDomainRef& d) {
      for (std::size_t p = 0; p < spec.patches.size(); ++p) {
        if (domain_key(spec.patches[p].domain) == domain_key(d)) return p;
      }
      throw std::invalid_argument("sheet port refers to an unknown domain");
    };
    const std::function<std::size_t(std::size_t)> root = [&](std::size_t p) {
      return group[p] == p ? p : group[p] = root(group[p]);
    };
    for (const auto& port : spec.ports) {
      group[root(patch_of(port.first.boundary.domain))] = root(patch_of(port.second.boundary.domain));
    }
    std::map<std::size_t, std::vector<std::size_t>> shells;
    for (std::size_t m = 0; m < membership.size(); ++m) shells[root(member_patch[m])].push_back(m);
    std::size_t checked = 0;
    for (const auto& entry : shells) {
      std::string unshown;
      for (const std::size_t m : unshown_regions(spec, embeddings, membership, member_patch, entry.second, unshown)) {
        checked += holes[m];
      }
    }
    return checked >= kHolesFilledFirst;
  } catch (...) {
    return false;
  }
}

} // namespace

SheetChartGeometry sheet_shape(const SheetChartShapeSpec& spec) {
  return detail::run("sheet chart shape", [&] {
    detail::require_positive("sheet thickness", spec.thickness);
    detail::require_positive("sheet tolerance", spec.tolerance);
    if (spec.thickness <= std::max(spec.tolerance, Precision::Confusion())) {
      throw std::invalid_argument("sheet thickness must exceed the modelling tolerance");
    }
    if (spec.patches.empty()) throw std::invalid_argument("sheet shape has no material charts");
    // The existing base-face constructor deliberately unions adjacent labelled
    // planar regions. They do not acquire artificial bend ports or two walls
    // at their shared boundary merely because the model now uses chart types.
    if (legacy_planar(spec)) return planar_adapter(spec);
    // The check for intersecting charts compares every pair of faces whose
    // boxes meet: the walls of hundreds of holes (a vent pattern) each meet
    // their panel's caps, which took most of a minute (mitcad#288). The
    // charts with their holes filled are checked instead; only where those
    // intersect (material through a window) is the sheet itself checked.
    // That costs a second construction, which pays only for many holes
    // where the check looks (holes_filled_first, mitcad#2536).
    if (const auto filled = holes_filled(spec); filled && holes_filled_first(spec, *filled)) {
      bool clear = false;
      try {
        construct(*filled, Pass::Clearance);
        clear = true;
      } catch (const std::invalid_argument&) {
      } catch (const std::runtime_error&) {
      }
      return construct(spec, clear ? Pass::Unchecked : Pass::Checked);
    }
    return construct(spec, Pass::Checked);
  });
}

namespace {

SheetChartGeometry construct(const SheetChartShapeSpec& spec, Pass pass) {
  const bool check_collisions = pass != Pass::Unchecked;
  const bool clearance = pass == Pass::Clearance;
  {
    ShapeTiming timing;
    std::set<std::string> identities;
    std::vector<Embedding> embeddings;
    std::vector<RegionMembership> membership;
    for (const auto& patch : spec.patches) {
      if (!identities.insert(domain_key(patch.domain)).second) throw std::invalid_argument("sheet chart identity is duplicated");
      embeddings.push_back(make_embedding(patch, spec.thickness, spec.tolerance));
      if (patch.regions.empty()) throw std::invalid_argument("sheet chart has no material regions");
    }
    auto ports = make_ports(spec, embeddings);
    std::vector<NamedFace> faces;
    for (std::size_t p = 0; p < spec.patches.size(); ++p) {
      for (std::size_t r = 0; r < spec.patches[p].regions.size(); ++r) {
        throw_if_cancelled();
        const std::size_t member = membership.size();
        membership.push_back({spec.patches[p].domain, r});
        region_faces(spec.patches[p].regions[r], embeddings[p], member, ports, spec.tolerance, faces);
      }
    }
    for (const auto& port : ports) {
      if (port.uses[0] != 2 || port.uses[1] != 2) throw std::invalid_argument("sheet port must join exactly two material regions");
    }
    timing.mark(ShapeTiming::Faces);
    // Domains joined by ports form one shell; each such group is sewn on its
    // own, so that walls of separate groups that coincide (coplanar panels
    // meeting along a boundary no port pairs, mitcad#242) are not sewn into
    // one shell. Their material is joined afterwards (fuse_coplanar).
    std::vector<std::size_t> group(spec.patches.size());
    for (std::size_t p = 0; p < group.size(); ++p) group[p] = p;
    const auto patch_of = [&](const SheetDomainRef& d) {
      for (std::size_t p = 0; p < spec.patches.size(); ++p) {
        if (domain_key(spec.patches[p].domain) == domain_key(d)) return p;
      }
      throw std::invalid_argument("sheet port refers to an unknown domain");
    };
    const std::function<std::size_t(std::size_t)> root = [&](std::size_t p) {
      return group[p] == p ? p : group[p] = root(group[p]);
    };
    for (const auto& port : ports) {
      group[root(patch_of(port.input->first.boundary.domain))] = root(patch_of(port.input->second.boundary.domain));
    }
    std::vector<std::size_t> member_patch;
    for (const auto& m : membership) member_patch.push_back(patch_of(m.domain));
    SheetChartGeometry result;
    std::vector<bool> seen(membership.size(), false);
    std::set<std::size_t> roots;
    for (std::size_t p = 0; p < group.size(); ++p) roots.insert(root(p));
    const auto sew = [&](std::size_t current) {
      // The port edges above are shared topology, with both incident pcurves.
      // Sewing closes the remaining exterior-wall vertices, within the
      // explicit modelling tolerance; it does not discover or guess material
      // adjacency.
      //
      // Each material region's faces are sewn on their own first, then the
      // regions together. Walls that touch only along a line (an open
      // corner, mitcad#1409; the corner of a flange on several edges with no
      // gap, mitcad#1872) each have an edge on that line, so it carries four
      // coincident free edges; sewn all at once, the sewing may pair one
      // wall's edge with the other wall's and leave both walls open there.
      // A region closes on its own edges first, so only the edges where
      // regions meet are left to pair.
      std::vector<const NamedFace*> mine;
      std::vector<std::vector<std::size_t>> by_member;
      std::vector<std::size_t> member_index(membership.size(), membership.size());
      for (const auto& face : faces) {
        if (root(member_patch[face.member]) != current) continue;
        if (member_index[face.member] == membership.size()) {
          member_index[face.member] = by_member.size();
          by_member.emplace_back();
        }
        by_member[member_index[face.member]].push_back(mine.size());
        mine.push_back(&face);
      }
      // Each face's image after its region's own sewing.
      std::vector<TopoDS_Shape> images(mine.size());
      BRepBuilderAPI_Sewing sewing(spec.tolerance);
      sewing.SetMaxTolerance(spec.tolerance);
      for (const auto& indices : by_member) {
        BRepBuilderAPI_Sewing region(spec.tolerance);
        region.SetMaxTolerance(spec.tolerance);
        for (const std::size_t i : indices) region.Add(mine[i]->face);
        detail::interruptible([&](const Message_ProgressRange& range) { region.Perform(range); });
        if (region.NbMultipleEdges() != 0) throw std::runtime_error("sheet charts do not close into manifold shells");
        for (const std::size_t i : indices) {
          images[i] = region.IsModified(mine[i]->face) ? region.Modified(mine[i]->face) : mine[i]->face;
        }
        sewing.Add(region.SewedShape());
      }
      detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
      if (sewing.NbFreeEdges() != 0 || sewing.NbMultipleEdges() != 0) {
        throw std::runtime_error("sheet charts do not close into manifold shells");
      }
      for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next()) {
        const auto shell = TopoDS::Shell(it.Current());
        if (!BRep_Tool::IsClosed(shell)) throw std::runtime_error("sheet chart shell is open");
        BRepBuilderAPI_MakeSolid maker(shell);
        TopoDS_Solid solid = maker.Solid();
        if (!BRepLib::OrientClosedSolid(solid)) throw std::runtime_error("sheet chart shell has no consistent material side");
        timing.mark(ShapeTiming::Sew);
        // Walls of this shell that meet where no port joins them: the
        // checker refuses a solid whose walls touch over a face or pass
        // through each other; the self-interference check below finds walls
        // that meet at all (mitcad#1409).
        std::vector<std::size_t> group_members;
        for (const NamedFace* face : mine) group_members.push_back(face->member);
        std::sort(group_members.begin(), group_members.end());
        group_members.erase(std::unique(group_members.begin(), group_members.end()), group_members.end());
        const auto meeting = [&](const std::vector<std::size_t>& regions) {
          return meeting_walls(spec, embeddings, membership, member_patch, regions);
        };
        try {
          detail::require_valid(solid, "sheet chart solid");
        } catch (const std::runtime_error&) {
          // With the holes filled only whether the shell holds counts.
          if (clearance) throw;
          std::optional<WallMeeting> found;
          try {
            found = meeting(group_members);
          } catch (const std::invalid_argument&) {
          }
          if (!found) throw;
          throw meeting_refusal(*found, membership);
        }
        timing.mark(ShapeTiming::Valid);
        // Topological validity alone permits intersecting nonadjacent walls
        // in a connected folded sheet. Check geometric self-interference as
        // well; where faces meet, walls that touch only along a line or at a
        // point are kept, the others refused naming both. Regions that
        // planes keep apart pair by pair cannot meet: the check could find
        // nothing then and is not run (mitcad#2444). Otherwise it looks at
        // the faces of the regions not kept apart only (mitcad#2536): a
        // face of any other region is kept apart from every region of the
        // shell, so it meets none, and walls that meet are two of those
        // regions, which is all meeting_walls then needs to compare.
        if (check_collisions) {
          std::string unshown;
          const auto suspects = unshown_regions(spec, embeddings, membership, member_patch, group_members, unshown);
          timing.shell(suspects.size(), group_members.size(), unshown);
          if (!suspects.empty()) {
            TopoDS_Shape checked = solid;
            if (suspects.size() < group_members.size()) {
              NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> in_solid;
              TopExp::MapShapes(solid, TopAbs_FACE, in_solid);
              TopoDS_Compound faces_of;
              BRep_Builder builder;
              builder.MakeCompound(faces_of);
              bool all_found = true;
              for (std::size_t i = 0; i < mine.size(); ++i) {
                if (!std::binary_search(suspects.begin(), suspects.end(), mine[i]->member)) continue;
                const auto& sewn = images[i];
                const auto image = sewing.IsModifiedSubShape(sewn) ? sewing.ModifiedSubShape(sewn) : sewn;
                if (in_solid.Contains(image)) builder.Add(faces_of, image);
                else all_found = false;
              }
              // A face not found in the solid as such: the solid is checked
              // whole, as before.
              if (all_found) checked = faces_of;
            }
            if (self_interferes(checked)) {
              if (clearance) throw std::invalid_argument("sheet material charts intersect in the current configuration");
              if (const auto found = meeting(suspects)) {
                throw meeting_refusal(*found, membership);
              }
            }
          }
        }
        timing.mark(ShapeTiming::Collisions);
        // With the holes filled the shells are what was asked: the sheet
        // itself is built next, named and fused there.
        if (clearance) continue;
        detail::FaceNamer namer(solid);
        std::set<std::size_t> members;
        for (std::size_t i = 0; i < mine.size(); ++i) {
          const auto& sewn = images[i];
          const auto image = sewing.IsModifiedSubShape(sewn) ? sewing.ModifiedSubShape(sewn) : sewn;
          if (namer.contains(image)) { namer.add(image, mine[i]->name); members.insert(mine[i]->member); }
        }
        namer.finish();
        SheetChartSolid output{namer.shape(), {}};
        if (volume(*output.shape) <= 0 || members.empty()) throw std::runtime_error("sheet chart produced an empty material solid");
        for (std::size_t member : members) {
          if (seen[member]) throw std::runtime_error("sheet material region belongs to multiple solids");
          seen[member] = true;
          output.membership.push_back(membership[member]);
        }
        result.solids.push_back(std::move(output));
        timing.mark(ShapeTiming::Names);
      }
    };
    for (const std::size_t current : roots) sew(current);
    if (clearance) {
      timing.print(spec.patches.size(), faces.size(), true);
      return result;
    }
    if (std::find(seen.begin(), seen.end(), false) != seen.end()) throw std::runtime_error("sheet chart construction lost a material region");
    fuse_coplanar(result, spec, embeddings);
    timing.mark(ShapeTiming::Fuse);
    for (std::size_t i = 0; i < result.solids.size(); ++i) for (std::size_t j = i + 1; j < result.solids.size(); ++j) {
      detail::Interrupt interrupt;
      BRepExtrema_DistShapeShape distance(result.solids[i].shape->occt(), result.solids[j].shape->occt(),
                                         Extrema_ExtFlag_MINMAX, Extrema_ExtAlgo_Grad, interrupt.range());
      throw_if_cancelled();
      if (!distance.IsDone() || distance.Value() <= spec.tolerance) {
        throw std::invalid_argument("sheet material regions overlap or touch without a paired port");
      }
    }
    timing.mark(ShapeTiming::Apart);
    timing.print(spec.patches.size(), faces.size());
    return result;
  }
}

} // namespace

} // namespace mitcad::geometry
