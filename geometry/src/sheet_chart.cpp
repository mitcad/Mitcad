// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <limits>
#include <optional>
#include <set>
#include <stdexcept>
#include <tuple>
#include <utility>

#include <Adaptor3d_CurveOnSurface.hxx>
#include <Approx_Curve3d.hxx>
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
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>

#include "history.hpp"
#include "mitcad/geometry/query.hpp"
#include "sheet_curve.hpp"
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

  gp_Pnt point(const gp_Pnt2d& p, bool back) const {
    if (const auto* plane = std::get_if<PlaneEmbedding>(&patch->embedding)) {
      gp_Pnt result = plane->front_frame.point(p);
      if (back) result.Translate(thickness * parity(plane->front_to_back_sign) *
                                 gp_Vec(plane->front_frame.normal()));
      return result;
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
    return p;
  }

  bool reverse_cap(bool back) const {
    if (std::holds_alternative<PlaneEmbedding>(patch->embedding)) {
      return back ? material_sign < 0 : material_sign > 0;
    }
    return back != std::get<CylinderEmbedding>(patch->embedding).inner_is_front;
  }
};

Embedding make_embedding(const SheetPatch& patch, double thickness) {
  Embedding result{&patch, thickness, {}};
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
  } else {
    const auto& c = std::get<CylinderEmbedding>(patch.embedding);
    finite(c.centre);
    detail::require_positive("sheet inside radius", c.inside_radius);
    detail::require_positive("sheet neutral radius", c.neutral_radius);
    detail::require_positive("sheet bend angle", c.angle_extent);
    // A flange's bend may turn past 180 degrees, short of a full turn (a hem, mitcad#1378).
    if (c.neutral_radius < c.inside_radius || c.neutral_radius > c.inside_radius + thickness ||
        c.angle_extent >= 2 * kPi - kAngularTolerance || std::abs(c.axis.Dot(c.radial_start)) > kAngularTolerance ||
        std::abs(c.axis.Dot(c.angular_direction)) > kAngularTolerance ||
        std::abs(c.radial_start.Dot(c.angular_direction)) > kAngularTolerance) {
      throw std::invalid_argument("invalid sheet cylinder radius, angle or orthogonal basis");
    }
    result.uv_sign = c.axis.Crossed(c.radial_start).Dot(c.angular_direction) > 0 ? 1 : -1;
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

Rails make_rails(const Curve2Ptr& chart_curve, const Embedding& embedding, double tolerance) {
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
          const auto spline = bounded_spline(curve, {curve.native_parameter(first), curve.native_parameter(last)});
          const Rails rails = make_rails(spline, embedding, tolerance);
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
      if (embedding.uv_sign < 0) current.Reverse();
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

SheetChartGeometry construct(const SheetChartShapeSpec& spec, bool check_collisions);

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
// or a common fails, as nothing then tells contact from penetration.
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
  std::optional<WallMeeting> touching;
  for (std::size_t i = 0; i < solids.size(); ++i) for (std::size_t j = i + 1; j < solids.size(); ++j) {
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
    if (const auto filled = holes_filled(spec)) {
      bool clear = false;
      try {
        construct(*filled, true);
        clear = true;
      } catch (const std::invalid_argument&) {
      } catch (const std::runtime_error&) {
      }
      return construct(spec, !clear);
    }
    return construct(spec, true);
  });
}

namespace {

SheetChartGeometry construct(const SheetChartShapeSpec& spec, bool check_collisions) {
  {
    std::set<std::string> identities;
    std::vector<Embedding> embeddings;
    std::vector<RegionMembership> membership;
    for (const auto& patch : spec.patches) {
      if (!identities.insert(domain_key(patch.domain)).second) throw std::invalid_argument("sheet chart identity is duplicated");
      embeddings.push_back(make_embedding(patch, spec.thickness));
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
      BRepBuilderAPI_Sewing sewing(spec.tolerance);
      sewing.SetMaxTolerance(spec.tolerance);
      std::vector<const NamedFace*> mine;
      for (const auto& face : faces) {
        if (root(member_patch[face.member]) == current) {
          sewing.Add(face.face);
          mine.push_back(&face);
        }
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
        // Walls of this shell that meet where no port joins them: the
        // checker refuses a solid whose walls touch over a face or pass
        // through each other; the self-interference check below finds walls
        // that meet at all (mitcad#1409).
        const auto meeting = [&] {
          std::vector<std::size_t> group_members;
          for (const NamedFace* face : mine) group_members.push_back(face->member);
          std::sort(group_members.begin(), group_members.end());
          group_members.erase(std::unique(group_members.begin(), group_members.end()), group_members.end());
          return meeting_walls(spec, embeddings, membership, member_patch, group_members);
        };
        try {
          detail::require_valid(solid, "sheet chart solid");
        } catch (const std::runtime_error&) {
          std::optional<WallMeeting> found;
          try {
            found = meeting();
          } catch (const std::invalid_argument&) {
          }
          if (!found) throw;
          throw meeting_refusal(*found, membership);
        }
        // Topological validity alone permits intersecting nonadjacent walls
        // in a connected folded sheet. Check geometric self-interference as
        // well; where faces meet, walls that touch only along a line or at a
        // point are kept, the others refused naming both.
        if (check_collisions) {
          BRepAlgoAPI_Check collision;
          collision.SetData(solid, false, true);
          detail::interruptible([&](const Message_ProgressRange& range) { collision.Perform(range); });
          if (collision.HasErrors() || !collision.IsValid()) {
            if (const auto found = meeting()) {
              throw meeting_refusal(*found, membership);
            }
          }
        }
        detail::FaceNamer namer(solid);
        std::set<std::size_t> members;
        for (const NamedFace* face : mine) {
          const auto image = sewing.IsModified(face->face) ? sewing.Modified(face->face) : face->face;
          if (namer.contains(image)) { namer.add(image, face->name); members.insert(face->member); }
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
      }
    };
    for (const std::size_t current : roots) sew(current);
    if (std::find(seen.begin(), seen.end(), false) != seen.end()) throw std::runtime_error("sheet chart construction lost a material region");
    fuse_coplanar(result, spec, embeddings);
    for (std::size_t i = 0; i < result.solids.size(); ++i) for (std::size_t j = i + 1; j < result.solids.size(); ++j) {
      detail::Interrupt interrupt;
      BRepExtrema_DistShapeShape distance(result.solids[i].shape->occt(), result.solids[j].shape->occt(),
                                         Extrema_ExtFlag_MINMAX, Extrema_ExtAlgo_Grad, interrupt.range());
      throw_if_cancelled();
      if (!distance.IsDone() || distance.Value() <= spec.tolerance) {
        throw std::invalid_argument("sheet material regions overlap or touch without a paired port");
      }
    }
    return result;
  }
}

} // namespace

} // namespace mitcad::geometry
