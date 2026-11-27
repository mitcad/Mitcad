// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet_develop.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom2d_Line.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2d.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Mat.hxx>
#include <gp_Pln.hxx>
#include <math_Jacobi.hxx>
#include <math_Matrix.hxx>
#include <math_Vector.hxx>

#include "mitcad/geometry/datum.hpp"
#include "sheet_curve.hpp"
#include "sheet_oblique_cone.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
// Edges that are no generator and no circle about the axis: cubic
// B-splines through their exact development within this (mm), as an
// emboss wraps a profile onto a cone (mitcad#473).
constexpr double kFit = 1.0e-8;
// Faces lie on one surface within this (mm), as face_geometry recognises
// surfaces stored as other types.
constexpr double kSame = 1.0e-5;
// An edge runs along a generator or round the axis when its development
// keeps one chart coordinate within this (mm).
constexpr double kOnLine = 1.0e-7;
// Points of each edge read for its angle round the axis and its kind.
constexpr int kSamples = 256;
// A wall's plane meets the generators (mitcad#2491) when the sine of its
// angle to them is at least this; a plane nearly along them cuts no end.
constexpr double kAcross = 1.0e-3;

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The cylinder or cone a face lies on: a cylinder's axis point, axis and
// radius; a cone's apex, its axis where the radius grows and half angle;
// an oblique cone's apex and circle (mitcad#2491).
struct Support {
  bool cone = false;
  bool oblique = false;
  gp_Pnt origin;
  gp_Dir axis{0, 0, 1};
  double radius = 0;
  double half_angle = 0;
  detail::ObliqueCone skew;
};

// The solution of the 3 x 3 system m x = r, if m is not singular (relative
// to its size).
std::optional<gp_XYZ> solved(const gp_Mat& m, const gp_XYZ& r) {
  double size = 0;
  for (int i = 1; i <= 3; ++i) {
    for (int j = 1; j <= 3; ++j) size = std::max(size, std::abs(m.Value(i, j)));
  }
  if (!(size > 0) || std::abs(m.Determinant()) <= 1e-12 * size * size * size) return std::nullopt;
  return r.Multiplied(m.Inverted());
}

// A face whose straight lines along one parameter all run through one
// point (its apex) over a circle: an oblique (eccentric) cone, stored as a
// ruled spline or a loft between circles (mitcad#2491). The lines'
// directions from the apex lie on a quadric cone, which has circular
// sections (its cyclic planes); the circle is the one halfway along the
// face. Every point of a grid of the face within kSame of the cone.
std::optional<detail::ObliqueCone> oblique_of(const TopoDS_Face& face) {
  try {
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    constexpr int kLines = 32;
    for (int along = 0; along < 2; ++along) {
      // s across the lines, t along one, both 0..1.
      const auto at = [&](double s, double t) {
        return along == 0 ? surface.Value(u0 + (u1 - u0) * s, v0 + (v1 - v0) * t)
                          : surface.Value(u0 + (u1 - u0) * t, v0 + (v1 - v0) * s);
      };
      std::vector<std::pair<gp_Pnt, gp_Pnt>> lines;
      bool straight = true;
      for (int i = 0; i <= kLines && straight; ++i) {
        const double s = static_cast<double>(i) / kLines;
        const gp_Pnt a = at(s, 0), b = at(s, 1);
        const double length = a.Distance(b);
        if (length <= 1e-6) {
          straight = false;
          break;
        }
        const gp_Vec d = gp_Vec(a, b) / length;
        for (int j = 1; j < 8 && straight; ++j) {
          const gp_Vec m(a, at(s, j / 8.0));
          if ((m - m.Dot(d) * d).Magnitude() > kSame) straight = false;
        }
        lines.emplace_back(a, b);
      }
      if (!straight) continue;
      // The point nearest every line.
      gp_Mat m(0, 0, 0, 0, 0, 0, 0, 0, 0);
      gp_XYZ r(0, 0, 0);
      for (const auto& [a, b] : lines) {
        const gp_Vec d = gp_Vec(a, b).Normalized();
        const gp_Mat across(1 - d.X() * d.X(), -d.X() * d.Y(), -d.X() * d.Z(), -d.Y() * d.X(), 1 - d.Y() * d.Y(),
                            -d.Y() * d.Z(), -d.Z() * d.X(), -d.Z() * d.Y(), 1 - d.Z() * d.Z());
        m += across;
        r += a.XYZ().Multiplied(across);
      }
      const std::optional<gp_XYZ> found = solved(m, r);
      if (!found) continue;
      const gp_Pnt apex(*found);
      // The lines' directions from the apex, read at their far ends.
      std::vector<gp_Vec> directions;
      gp_Vec mean(0, 0, 0);
      bool through = true;
      for (const auto& [a, b] : lines) {
        const gp_Vec d = gp_Vec(a, b).Normalized();
        const gp_Vec off(apex, a);
        const double reach = std::max(1.0, std::max(apex.Distance(a), apex.Distance(b)) / a.Distance(b));
        if ((off - off.Dot(d) * d).Magnitude() > kSame * reach) {
          through = false;
          break;
        }
        const gp_Pnt end = apex.Distance(a) > apex.Distance(b) ? a : b;
        if (apex.Distance(end) <= 1e-6) {
          through = false;
          break;
        }
        directions.push_back(gp_Vec(apex, end).Normalized());
        mean += directions.back();
      }
      if (!through || mean.Magnitude() <= 1e-9) continue;
      mean.Normalize();
      // The quadric cone g^T Q g = 0 of the directions: the least
      // eigenvector of their moments.
      math_Matrix moments(1, 6, 1, 6, 0.0);
      for (const gp_Vec& g : directions) {
        const double row[6] = {g.X() * g.X(), g.Y() * g.Y(), g.Z() * g.Z(),
                               2 * g.X() * g.Y(), 2 * g.X() * g.Z(), 2 * g.Y() * g.Z()};
        for (int i = 0; i < 6; ++i) {
          for (int j = 0; j < 6; ++j) moments(i + 1, j + 1) += row[i] * row[j];
        }
      }
      math_Jacobi fit(moments);
      if (!fit.IsDone()) continue;
      int least = 1;
      for (int i = 2; i <= 6; ++i) {
        if (fit.Value(i) < fit.Value(least)) least = i;
      }
      math_Vector q(1, 6);
      fit.Vector(least, q);
      math_Matrix quadric(1, 3, 1, 3, 0.0);
      quadric(1, 1) = q(1); quadric(2, 2) = q(2); quadric(3, 3) = q(3);
      quadric(1, 2) = quadric(2, 1) = q(4);
      quadric(1, 3) = quadric(3, 1) = q(5);
      quadric(2, 3) = quadric(3, 2) = q(6);
      math_Jacobi axes(quadric);
      if (!axes.IsDone()) continue;
      // Two eigenvalues of one sign and one of the other: those two
      // positive, l1 >= l2 > 0 > l3.
      std::vector<std::pair<double, gp_Vec>> eigen;
      double largest = 0;
      for (int i = 1; i <= 3; ++i) {
        math_Vector e(1, 3);
        axes.Vector(i, e);
        eigen.emplace_back(axes.Value(i), gp_Vec(e(1), e(2), e(3)).Normalized());
        largest = std::max(largest, std::abs(axes.Value(i)));
      }
      int positive = 0, negative = 0;
      for (const auto& value : eigen) {
        if (value.first > 1e-9 * largest) ++positive;
        if (value.first < -1e-9 * largest) ++negative;
      }
      if (positive + negative != 3 || positive == 0 || negative == 0) continue;
      if (positive == 1) {
        for (auto& value : eigen) value.first = -value.first;
      }
      std::sort(eigen.begin(), eigen.end(), [](const auto& a, const auto& b) { return a.first > b.first; });
      const double a1 = std::sqrt(std::max(0.0, eigen[0].first - eigen[1].first));
      const double a3 = std::sqrt(eigen[1].first - eigen[2].first);
      // The cyclic plane across the face's generators most squarely.
      gp_Vec normal;
      double best = -1;
      for (const double sign : {1.0, -1.0}) {
        gp_Vec p = sign * a1 * eigen[0].second + a3 * eigen[2].second;
        if (p.Magnitude() <= 1e-12) continue;
        p.Normalize();
        if (p.Dot(mean) < 0) p.Reverse();
        double squarest = 1;
        for (const gp_Vec& g : directions) squarest = std::min(squarest, p.Dot(g));
        if (squarest > best) {
          best = squarest;
          normal = p;
        }
      }
      if (!(best > 1e-6)) continue;
      // The section halfway along the face: a circle.
      double level = 0;
      for (const auto& [a, b] : lines) {
        level += (normal.Dot(gp_Vec(apex, a)) + normal.Dot(gp_Vec(apex, b))) / 2;
      }
      level /= static_cast<double>(lines.size());
      gp_Vec ex = std::abs(normal.X()) < 0.9 ? gp_Vec(1, 0, 0) : gp_Vec(0, 1, 0);
      ex = (ex - ex.Dot(normal) * normal).Normalized();
      const gp_Vec ey = normal.Crossed(ex);
      const gp_Pnt middle = apex.Translated(level * normal);
      gp_Mat sums(0, 0, 0, 0, 0, 0, 0, 0, 0);
      gp_XYZ rhs(0, 0, 0);
      for (const gp_Vec& g : directions) {
        const gp_Vec on = gp_Vec(middle, apex.Translated(g * (level / normal.Dot(g))));
        const double x = on.Dot(ex), y = on.Dot(ey), w = x * x + y * y;
        sums += gp_Mat(x * x, x * y, x, x * y, y * y, y, x, y, 1);
        rhs -= gp_XYZ(x * w, y * w, w);
      }
      const std::optional<gp_XYZ> circle = solved(sums, rhs);
      if (!circle) continue;
      const double cx = -circle->X() / 2, cy = -circle->Y() / 2;
      const double radius2 = cx * cx + cy * cy - circle->Z();
      if (!(radius2 > 0)) continue;
      detail::ObliqueCone cone;
      cone.apex = apex;
      cone.centre = middle.Translated(cx * ex + cy * ey);
      cone.radius = std::sqrt(radius2);
      cone.x = ex;
      cone.y = ey;
      // Every point of the face on it.
      bool on = true;
      for (int i = 0; i <= 8 && on; ++i) {
        for (int j = 0; j <= 8 && on; ++j) {
          const gp_Pnt p = surface.Value(u0 + (u1 - u0) * i / 8, v0 + (v1 - v0) * j / 8);
          const detail::ObliqueCone::Generator g = cone.generator(cone.angle(p));
          if (gp_Vec(apex, p).Crossed(g.g).Magnitude() > kSame) on = false;
        }
      }
      if (on) return cone;
    }
  } catch (const Standard_Failure&) {
  } catch (const std::invalid_argument&) {
  }
  return std::nullopt;
}

Support support_of(const TopoDS_Face& face) {
  const occ::handle<Geom_Surface> analytic = analytic_surface(face);
  Support result;
  if (!analytic.IsNull()) {
    const GeomAdaptor_Surface surface(analytic);
    if (surface.GetType() == GeomAbs_Cylinder) {
      const gp_Cylinder cylinder = surface.Cylinder();
      result.origin = cylinder.Location();
      result.axis = cylinder.Axis().Direction();
      result.radius = cylinder.Radius();
      return result;
    }
    if (surface.GetType() == GeomAbs_Cone) {
      const gp_Cone cone = surface.Cone();
      result.cone = true;
      result.origin = cone.Apex();
      result.axis = cone.SemiAngle() > 0 ? cone.Axis().Direction() : cone.Axis().Direction().Reversed();
      result.half_angle = std::abs(cone.SemiAngle());
      return result;
    }
  }
  // An oblique cone (mitcad#2491), which no analytic surface is.
  if (const std::optional<detail::ObliqueCone> skew = oblique_of(face)) {
    result.oblique = true;
    result.skew = *skew;
    result.origin = skew->apex;
    result.axis = gp_Dir(skew->x.Crossed(skew->y));
    return result;
  }
  throw std::invalid_argument("the face is not a cylinder or a cone");
}

// The face lies on the support: points of its own surface over its
// parameter box within kSame of it (faces stored as splines are each a
// cylinder or a cone within 1e-6 mm, recognised one by one, so their
// recognised axes and radii differ by more than that over the panel).
bool on_support(const TopoDS_Face& face, const Support& support) {
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(face);
  const gp_Vec axis(support.axis);
  for (int i = 0; i <= 4; ++i) {
    for (int j = 0; j <= 4; ++j) {
      const gp_Pnt p = surface.Value(u0 + (u1 - u0) * i / 4, v0 + (v1 - v0) * j / 4);
      const gp_Vec d(support.origin, p);
      if (support.oblique) {
        const detail::ObliqueCone::Generator g = support.skew.generator(support.skew.angle(p));
        if (d.Crossed(g.g).Magnitude() > kSame) return false;
        continue;
      }
      const double h = d.Dot(axis);
      const double r = (d - h * axis).Magnitude();
      const double off = support.cone ? r * std::cos(support.half_angle) - h * std::sin(support.half_angle)
                                      : r - support.radius;
      if (std::abs(off) > kSame) return false;
    }
  }
  return true;
}

double wrapped(double angle) {
  while (angle > kPi) angle -= 2 * kPi;
  while (angle <= -kPi) angle += 2 * kPi;
  return angle;
}

// An edge as a face's wire runs it: its points at parameters increasing
// over its range, their angles round the axis as read (`raw`) and as the
// wire runs on round it (`angle`, continuous along the wire).
struct Track {
  TopoDS_Edge edge;
  double first = 0, last = 0;
  std::vector<double> params;
  std::vector<gp_Pnt> points;
  std::vector<double> raw, angle;

  double middle() const { return angle[angle.size() / 2]; }
};

struct FaceTracks {
  std::vector<Track> tracks;
  double shift = 0;
  bool placed = false;
};

// A cubic B-spline through `exact` at evenly spaced parameters of
// [first, last], more of them until it is within kFit of it between them.
occ::handle<Geom2d_BSplineCurve> fitted(const std::function<gp_Pnt2d(double)>& exact, double first, double last) {
  for (int n = 16; n <= 4096; n *= 2) {
    throw_if_cancelled();
    const occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, n + 1);
    const occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      points->SetValue(i + 1, exact(t));
      params->SetValue(i + 1, t);
    }
    Geom2dAPI_Interpolate interpolate(points, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) break;
    const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
    double worst = 0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      worst = std::max(worst, curve->Value(t).Distance(exact(t)));
    }
    if (worst <= kFit) return curve;
  }
  throw std::runtime_error("an edge of the face could not be developed within 1e-8 mm");
}

BoundedCurve2 chart_line(const gp_Pnt2d& a, const gp_Pnt2d& b) {
  return detail::export_sheet_curve(new Geom2d_Line(a, gp_Dir2d(gp_Vec2d(a, b))), {0.0, a.Distance(b)});
}

// How an edge develops: along a generator (a line), round a cylinder's or
// a cone's axis (a line or an arc), or otherwise (a B-spline).
enum class EdgeKind { Generator, Circle, Other };

} // namespace

SheetFaceDevelopment sheet_face_development(const Shape& shape, const std::vector<std::string>& names,
                                            double neutral_depth, double thickness) {
  return detail::run("sheet face development", [&] {
    detail::require_finite("the neutral surface's depth", neutral_depth);
    detail::require_positive("the sheet's thickness", thickness);
    std::vector<TopoDS_Face> faces;
    for (const std::string& name : names) {
      const std::vector<int> found = shape.find_faces(name);
      if (found.empty()) throw std::invalid_argument("the body has no such face");
      for (const int f : found) {
        const TopoDS_Face face = shape.face(f);
        const bool listed = std::any_of(faces.begin(), faces.end(), [&](const TopoDS_Face& g) { return g.IsSame(face); });
        if (!listed) faces.push_back(face);
      }
    }
    if (faces.empty()) throw std::invalid_argument("no face to develop");
    const Support support = support_of(faces.front());
    for (const TopoDS_Face& face : faces) {
      if (!on_support(face, support)) {
        throw std::invalid_argument("the faces do not lie on one cylinder or cone");
      }
    }
    SheetFaceDevelopment result;
    result.cone = support.cone;
    result.oblique = support.oblique;
    // The way the first face's material lies: its outward normal towards
    // the axis (an oblique cone's inside) makes it the sheet's inner
    // surface.
    {
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(faces.front(), u0, u1, v0, v1);
      const BRepAdaptor_Surface surface(faces.front());
      BRepLProp_SLProps props(surface, (u0 + u1) / 2, (v0 + v1) / 2, 1, 1.0e-9);
      if (!props.IsNormalDefined()) throw std::runtime_error("the face has no normal at its middle");
      gp_Dir normal = props.Normal();
      if (faces.front().Orientation() == TopAbs_REVERSED) normal.Reverse();
      gp_Vec outward;
      if (support.oblique) {
        outward = support.skew.generator(support.skew.angle(props.Value())).n;
      } else {
        outward = gp_Vec(support.origin, props.Value());
        outward -= outward.Dot(gp_Vec(support.axis)) * gp_Vec(support.axis);
      }
      result.inner_is_front = gp_Vec(normal).Dot(outward) < 0;
    }
    // The neutral surface from the faces', away from the axis.
    const double offset = result.inner_is_front ? neutral_depth : -(thickness - neutral_depth);
    const gp_Vec axis(support.axis);
    const double sin_angle = std::sin(support.half_angle), cos_angle = std::cos(support.half_angle);
    if (support.oblique) {
      result.origin = support.skew.apex;
      result.centre = support.skew.centre;
      result.circle_radius = support.skew.radius;
    } else if (support.cone) {
      result.half_angle = support.half_angle;
      result.origin = support.origin.Translated(axis * (-offset / sin_angle));
    } else {
      result.origin = support.origin;
      result.neutral_radius = support.radius + offset;
      detail::require_positive("the neutral surface's radius", result.neutral_radius);
    }
    result.axis = support.axis;
    // The faces of the body across each edge (mitcad#2491: an oblique cut's
    // plane, an oblique cone's walls).
    Ancestors faces_of_edge;
    TopExp::MapShapesAndAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
    const auto is_front = [&](const TopoDS_Shape& face) {
      return std::any_of(faces.begin(), faces.end(), [&](const TopoDS_Face& g) { return g.IsSame(face); });
    };
    if (support.oblique) {
      // The plate behind an oblique cone: its back the cone moved the
      // thickness along its normals, every other face a wall meeting the
      // front and the back.
      const double back = result.inner_is_front ? thickness : -thickness;
      const auto off_cone = [&](const gp_Pnt& p) -> std::optional<double> {
        // The generator whose normal plane holds p: (p - apex) . g' = 0.
        const gp_Vec d(support.skew.apex, p);
        double theta = support.skew.angle(p);
        for (int i = 0; i < 50; ++i) {
          const detail::ObliqueCone::Generator g = support.skew.generator(theta);
          const double h = d.Dot(g.dg), slope = d.Dot(g.ddg);
          if (std::abs(h) <= 1e-12 * std::max(1.0, d.Magnitude())) return d.Dot(g.n);
          if (!(std::abs(slope) > 0)) return std::nullopt;
          theta -= h / slope;
        }
        return std::nullopt;
      };
      std::vector<TopoDS_Face> backs, walls;
      for (int f = 0; f < shape.face_count(); ++f) {
        const TopoDS_Face face = shape.face(f);
        if (is_front(face)) continue;
        double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
        BRepTools::UVBounds(face, u0, u1, v0, v1);
        const BRepAdaptor_Surface surface(face);
        bool on_back = true;
        for (int i = 0; i <= 4 && on_back; ++i) {
          for (int j = 0; j <= 4 && on_back; ++j) {
            const std::optional<double> off = off_cone(surface.Value(u0 + (u1 - u0) * i / 4, v0 + (v1 - v0) * j / 4));
            on_back = off && std::abs(*off - back) <= 1e-4;
          }
        }
        (on_back ? backs : walls).push_back(face);
      }
      const auto meets = [&](const TopoDS_Face& wall, const std::vector<TopoDS_Face>& others, bool fronts) {
        for (TopExp_Explorer e(wall, TopAbs_EDGE); e.More(); e.Next()) {
          if (!faces_of_edge.Contains(e.Current())) continue;
          for (const TopoDS_Shape& other : faces_of_edge.FindFromKey(e.Current())) {
            if (other.IsSame(wall)) continue;
            if (fronts ? is_front(other)
                       : std::any_of(others.begin(), others.end(), [&](const TopoDS_Face& g) { return g.IsSame(other); })) {
              return true;
            }
          }
        }
        return false;
      };
      result.plate = !backs.empty() && std::all_of(walls.begin(), walls.end(), [&](const TopoDS_Face& wall) {
        return meets(wall, backs, true) && meets(wall, backs, false);
      });
    }
    // A frame across the axis to read angles round it (an oblique cone's
    // own, in its circle's plane).
    gp_Vec x(1, 0, 0);
    if (std::abs(x.Dot(axis)) > 0.9) x = gp_Vec(0, 1, 0);
    x = (x - x.Dot(axis) * axis).Normalized();
    gp_Vec y = axis.Crossed(x);
    if (support.oblique) {
      x = support.skew.x;
      y = support.skew.y;
    }
    const auto raw_angle = [&](const gp_Pnt& p) {
      if (support.oblique) return support.skew.angle(p);
      const gp_Vec d(support.origin, p);
      return std::atan2(d.Dot(y), d.Dot(x));
    };
    // Every face's edges, the angles continuous along each wire; a hole's
    // wire turned by whole turns to lie with its face's outline.
    std::vector<FaceTracks> tracked(faces.size());
    for (std::size_t f = 0; f < faces.size(); ++f) {
      const TopoDS_Wire outer = BRepTools::OuterWire(faces[f]);
      std::vector<TopoDS_Wire> wires{outer};
      for (TopExp_Explorer w(faces[f], TopAbs_WIRE); w.More(); w.Next()) {
        if (!w.Current().IsSame(outer)) wires.push_back(TopoDS::Wire(w.Current()));
      }
      double outer_mean = 0;
      for (std::size_t w = 0; w < wires.size(); ++w) {
        const std::size_t begin = tracked[f].tracks.size();
        bool started = false;
        double previous_raw = 0, previous_angle = 0;
        for (BRepTools_WireExplorer e(wires[w], faces[f]); e.More(); e.Next()) {
          throw_if_cancelled();
          const TopoDS_Edge edge = e.Current();
          if (BRep_Tool::Degenerated(edge)) {
            if (support.cone || support.oblique) {
              throw std::invalid_argument("the face runs to its cone's apex, which no sheet can be formed to");
            }
            continue;
          }
          const BRepAdaptor_Curve curve(edge);
          Track track;
          track.edge = edge;
          track.first = curve.FirstParameter();
          track.last = curve.LastParameter();
          for (int i = 0; i <= kSamples; ++i) {
            const double t = track.first + (track.last - track.first) * i / kSamples;
            track.params.push_back(t);
            track.points.push_back(curve.Value(t));
            track.raw.push_back(raw_angle(track.points.back()));
          }
          track.angle.assign(track.raw.size(), 0);
          const bool reversed = edge.Orientation() == TopAbs_REVERSED;
          for (int k = 0; k <= kSamples; ++k) {
            const int i = reversed ? kSamples - k : k;
            if (!started) {
              previous_raw = previous_angle = track.raw[i];
              started = true;
            }
            track.angle[i] = previous_angle + wrapped(track.raw[i] - previous_raw);
            previous_raw = track.raw[i];
            previous_angle = track.angle[i];
          }
          tracked[f].tracks.push_back(std::move(track));
        }
        double sum = 0;
        std::size_t count = 0;
        for (std::size_t t = begin; t < tracked[f].tracks.size(); ++t) {
          for (const double a : tracked[f].tracks[t].angle) {
            sum += a;
            ++count;
          }
        }
        const double mean = count > 0 ? sum / count : 0;
        if (w == 0) {
          outer_mean = mean;
        } else {
          const double turn = 2 * kPi * std::round((outer_mean - mean) / (2 * kPi));
          for (std::size_t t = begin; t < tracked[f].tracks.size(); ++t) {
            for (double& a : tracked[f].tracks[t].angle) a += turn;
          }
        }
      }
    }
    // The faces turned by whole turns to meet at their shared edges, from
    // the first; where they meet again further round, they stay apart (the
    // seam of faces closing round the axis).
    tracked.front().placed = true;
    for (bool progress = true; progress;) {
      progress = false;
      for (auto& face : tracked) {
        if (face.placed) continue;
        for (const auto& other : tracked) {
          if (!other.placed || face.placed) continue;
          for (const auto& mine : face.tracks) {
            const auto shared = std::find_if(other.tracks.begin(), other.tracks.end(),
                                             [&](const Track& t) { return t.edge.IsSame(mine.edge); });
            if (shared == other.tracks.end()) continue;
            face.shift = 2 * kPi * std::round((shared->middle() + other.shift - mine.middle()) / (2 * kPi));
            face.placed = true;
            progress = true;
            break;
          }
        }
      }
    }
    for (const auto& face : tracked) {
      if (!face.placed) throw std::invalid_argument("the faces on the cylinder or cone are not joined");
    }
    // A vertex's angle round the axis (the faces turned to meet): where a
    // generator ends at it, the generator's own. A vertex a boolean made
    // lies up to its tolerance off the generator (the seam of a tube cut at
    // an angle, mitcad#2491), which would put the outline that much past
    // the generator, beyond the panel's turn.
    struct Snap {
      TopoDS_Vertex vertex;
      double angle = 0;
    };
    std::vector<Snap> snaps;
    const auto along_generator = [](const Track& track) {
      return std::all_of(track.angle.begin(), track.angle.end(),
                         [&](double a) { return std::abs(a - track.angle.front()) <= 1.0e-9; });
    };
    for (const auto& face : tracked) {
      for (const auto& track : face.tracks) {
        if (!along_generator(track)) continue;
        TopoDS_Vertex first_vertex, last_vertex;
        TopExp::Vertices(track.edge, first_vertex, last_vertex);
        for (const TopoDS_Vertex& v : {first_vertex, last_vertex}) {
          if (!v.IsNull()) snaps.push_back({v, track.angle.front() + face.shift});
        }
      }
    }
    const auto vertex_angle = [&](const FaceTracks& face, const Track& track, const TopoDS_Vertex& v, std::size_t i) {
      const gp_Pnt p = v.IsNull() ? track.points[i] : BRep_Tool::Pnt(v);
      const double angle = track.angle[i] + wrapped(raw_angle(p) - track.raw[i]) + face.shift;
      if (v.IsNull()) return angle;
      for (const Snap& snap : snaps) {
        if (snap.vertex.IsSame(v) && std::abs(snap.angle - angle) < 1.0e-3) return snap.angle;
      }
      return angle;
    };
    // The faces' angles: generators', the inside of other edges, and the
    // vertices', as the outline is developed.
    double low = 1e300, high = -1e300;
    for (const auto& face : tracked) {
      for (const auto& track : face.tracks) {
        const bool generator = along_generator(track);
        for (std::size_t i = 0; i < track.angle.size(); ++i) {
          if (!generator && (i == 0 || i + 1 == track.angle.size())) continue;
          low = std::min(low, track.angle[i] + face.shift);
          high = std::max(high, track.angle[i] + face.shift);
        }
        if (generator) continue;
        TopoDS_Vertex first_vertex, last_vertex;
        TopExp::Vertices(track.edge, first_vertex, last_vertex);
        for (const double a : {vertex_angle(face, track, first_vertex, 0),
                               vertex_angle(face, track, last_vertex, static_cast<std::size_t>(kSamples))}) {
          low = std::min(low, a);
          high = std::max(high, a);
        }
      }
    }
    if (!(high >= low)) throw std::invalid_argument("the faces have no edges to develop");
    result.turn = high - low;
    if (result.turn > 2 * kPi + 1.0e-7) throw std::invalid_argument("the faces run round their axis more than once");
    result.closed = result.turn >= 2 * kPi - 1.0e-7;
    if (result.closed) result.turn = 2 * kPi;
    result.radial_start = gp_Dir(std::cos(low) * x + std::sin(low) * y);
    result.angular_direction = gp_Dir(-std::sin(low) * x + std::cos(low) * y);
    // An oblique cone's development from where the faces start
    // (mitcad#2491): its angle 0 there.
    const detail::ObliqueCone skew = support.skew.turned(support.oblique ? low : 0.0);
    std::optional<detail::ObliqueDevelopment> development;
    if (support.oblique) development.emplace(skew, offset, -0.5, 2 * kPi + 0.5);
    // A point at the angle `angle` from the start into the chart.
    const auto chart = [&](const gp_Pnt& p, double angle) {
      if (support.oblique) {
        const detail::ObliqueCone::Generator g = skew.generator(angle);
        return development->chart(angle, gp_Vec(skew.apex, p).Dot(g.g));
      }
      if (!support.cone) {
        return gp_Pnt2d(result.neutral_radius * angle, gp_Vec(result.origin, p).Dot(axis));
      }
      const gp_Vec d(result.origin, p);
      const double h = d.Dot(axis);
      const double r = (d - h * axis).Magnitude();
      const double rho = r * sin_angle + h * cos_angle;
      const double phi = sin_angle * angle;
      return gp_Pnt2d(rho * std::cos(phi), rho * std::sin(phi));
    };
    // The point at the angle `angle` where the plane `wall` meets the
    // neutral surface on the generator through `p` (mitcad#2491); none
    // where the plane runs along the generators.
    const auto on_wall = [&](const gp_Pnt& p, double angle, const gp_Pln& wall) -> std::optional<gp_Pnt2d> {
      const gp_Vec n(wall.Axis().Direction());
      const gp_Pnt on = wall.Location();
      if (support.oblique) {
        const detail::ObliqueCone::Generator g = skew.generator(angle);
        const double across = n.Dot(g.g);
        if (std::abs(across) < kAcross) return std::nullopt;
        const gp_Pnt start = skew.apex.Translated(offset * g.n);
        return development->chart(angle, n.Dot(gp_Vec(start, on)) / across);
      }
      gp_Vec radial(support.origin, p);
      radial -= radial.Dot(axis) * axis;
      if (radial.Magnitude() <= 1e-12) return std::nullopt;
      radial.Normalize();
      if (!support.cone) {
        const double across = n.Dot(axis);
        if (std::abs(across) < kAcross) return std::nullopt;
        const gp_Pnt start = result.origin.Translated(result.neutral_radius * radial);
        return gp_Pnt2d(result.neutral_radius * angle, n.Dot(gp_Vec(start, on)) / across);
      }
      const gp_Vec generator = sin_angle * radial + cos_angle * axis;
      const double across = n.Dot(generator);
      if (std::abs(across) < kAcross) return std::nullopt;
      const double rho = n.Dot(gp_Vec(result.origin, on)) / across;
      const double phi = sin_angle * angle;
      return gp_Pnt2d(rho * std::cos(phi), rho * std::sin(phi));
    };
    // The plane of the face across an edge, if it is one.
    const auto wall_plane = [&](const TopoDS_Edge& edge) -> std::optional<gp_Pln> {
      if (!faces_of_edge.Contains(edge)) return std::nullopt;
      for (const TopoDS_Shape& other : faces_of_edge.FindFromKey(edge)) {
        if (!is_front(other)) return face_plane(TopoDS::Face(other));
      }
      return std::nullopt;
    };
    // Each edge's development and kind; an edge of kind Other whose wall is
    // a plane across the generators is developed on that plane, and its
    // vertices with it.
    struct Read {
      std::vector<gp_Pnt2d> q;
      EdgeKind kind = EdgeKind::Other;
      std::optional<gp_Pln> wall;
      bool empty = false;
    };
    std::vector<std::vector<Read>> reads(tracked.size());
    std::vector<std::pair<TopoDS_Vertex, gp_Pln>> walled;
    for (std::size_t f = 0; f < tracked.size(); ++f) {
      const auto& face = tracked[f];
      const double turned = face.shift - low;
      for (const auto& track : face.tracks) {
        throw_if_cancelled();
        Read read;
        for (std::size_t i = 0; i < track.points.size(); ++i) {
          read.q.push_back(chart(track.points[i], track.angle[i] + turned));
        }
        const auto& q = read.q;
        double spread_angle = 0, spread_along = 0, length = 0;
        for (std::size_t i = 1; i < q.size(); ++i) length += q[i].Distance(q[i - 1]);
        read.empty = length <= 1.0e-9;
        if (support.oblique) {
          // Across the generator at the edge's start; no circles.
          const double phi = development->phi(track.angle[0] + turned);
          const gp_Vec2d g(std::cos(phi), std::sin(phi));
          for (std::size_t i = 0; i < q.size(); ++i) {
            spread_angle = std::max(spread_angle, std::abs(g.Crossed(gp_Vec2d(q[0], q[i]))));
          }
          spread_along = std::numeric_limits<double>::infinity();
        } else {
          for (std::size_t i = 0; i < q.size(); ++i) {
            if (!support.cone) {
              spread_angle = std::max(spread_angle, std::abs(q[i].X() - q[0].X()));
              spread_along = std::max(spread_along, std::abs(q[i].Y() - q[0].Y()));
            } else {
              const double rho = std::hypot(q[i].X(), q[i].Y()), rho0 = std::hypot(q[0].X(), q[0].Y());
              const double turn = sin_angle * (track.angle[i] - track.angle[0]);
              spread_angle = std::max(spread_angle, std::abs(turn) * rho);
              spread_along = std::max(spread_along, std::abs(rho - rho0));
            }
          }
        }
        if (spread_angle <= kOnLine) {
          read.kind = EdgeKind::Generator;
        } else if (spread_along <= kOnLine) {
          read.kind = EdgeKind::Circle;
        }
        if (read.kind == EdgeKind::Other && !read.empty) {
          read.wall = wall_plane(track.edge);
          // Only a plane across the generators all along the edge.
          for (std::size_t i = 0; read.wall && i < track.points.size(); i += 16) {
            if (!on_wall(track.points[i], track.angle[i] + turned, *read.wall)) read.wall.reset();
          }
          if (read.wall) {
            TopoDS_Vertex first_vertex, last_vertex;
            TopExp::Vertices(track.edge, first_vertex, last_vertex);
            for (const TopoDS_Vertex& v : {first_vertex, last_vertex}) {
              if (!v.IsNull()) walled.emplace_back(v, *read.wall);
            }
          }
        }
        reads[f].push_back(std::move(read));
      }
    }
    for (std::size_t f = 0; f < tracked.size(); ++f) {
      const auto& face = tracked[f];
      const double turned = face.shift - low;
      std::vector<BoundedCurve2> edges;
      for (std::size_t e = 0; e < face.tracks.size(); ++e) {
        throw_if_cancelled();
        const Track& track = face.tracks[e];
        const Read& read = reads[f][e];
        if (read.empty) continue;
        // The developed point of `p` at `angle`: on the edge's own wall, or
        // on a wall its vertex `v` ends at (a generator ending at a cut),
        // else projected along the normals.
        const auto developed = [&](const gp_Pnt& p, double angle, const TopoDS_Vertex* v) {
          if (read.wall) {
            if (const auto at = on_wall(p, angle, *read.wall)) return *at;
          } else if (v && !v->IsNull()) {
            for (const auto& [vertex, wall] : walled) {
              if (!vertex.IsSame(*v)) continue;
              if (const auto at = on_wall(p, angle, wall)) return *at;
            }
          }
          return chart(p, angle);
        };
        // The edge's ends at its vertices, where the edges of a loop meet
        // exactly (an edge's curve may end up to its tolerance from them).
        TopoDS_Vertex first_vertex, last_vertex;
        TopExp::Vertices(track.edge, first_vertex, last_vertex);
        // At a generator's end, the generator's angle.
        const auto at_vertex = [&](const TopoDS_Vertex& v, std::size_t i) {
          const gp_Pnt p = v.IsNull() ? track.points[i] : BRep_Tool::Pnt(v);
          const double angle = vertex_angle(face, track, v, i) - low;
          return std::make_pair(angle, developed(p, angle, &v));
        };
        const auto [start_angle, start] = at_vertex(first_vertex, 0);
        const auto [end_angle, end] = at_vertex(last_vertex, kSamples);
        if (read.kind == EdgeKind::Generator || (read.kind == EdgeKind::Circle && !support.cone)) {
          // A generator, or a circle round a cylinder's axis: a line.
          edges.push_back(chart_line(start, end));
          continue;
        }
        if (read.kind == EdgeKind::Circle) {
          // A circle round a cone's axis: an arc about the apex.
          const double rho = (std::hypot(start.X(), start.Y()) + std::hypot(end.X(), end.Y())) / 2;
          detail::require_positive("a developed circle's radius", rho);
          const double a = sin_angle * start_angle;
          const double b = sin_angle * end_angle;
          const occ::handle<Geom2d_Circle> circle = new Geom2d_Circle(gp_Ax2d(gp_Pnt2d(0, 0), gp_Dir2d(1, 0)), rho);
          edges.push_back(detail::export_sheet_curve(circle, {std::min(a, b), std::max(a, b)}));
          continue;
        }
        const BRepAdaptor_Curve curve(track.edge);
        const auto exact = [&](double t) {
          const double at = (t - track.first) / (track.last - track.first) * kSamples;
          const std::size_t i = static_cast<std::size_t>(std::clamp(std::round(at), 0.0, double(kSamples)));
          const gp_Pnt p = curve.Value(t);
          const double angle = track.angle[i] + wrapped(raw_angle(p) - track.raw[i]);
          return developed(p, angle + turned, nullptr);
        };
        // Each span where the edge's curve is twice continuous on its own
        // (a spline's knots of lower continuity change its speed, which no
        // cubic through evenly spaced points follows).
        const int spans = curve.NbIntervals(GeomAbs_C2);
        NCollection_Array1<double> bounds(1, spans + 1);
        curve.Intervals(bounds, GeomAbs_C2);
        for (int k = 1; k <= spans; ++k) {
          const double a = std::max(track.first, bounds(k)), b = std::min(track.last, bounds(k + 1));
          if (!(b > a)) continue;
          const occ::handle<Geom2d_BSplineCurve> developed_curve = fitted(exact, a, b);
          if (a == track.first) developed_curve->SetPole(1, start);
          if (b == track.last) developed_curve->SetPole(developed_curve->NbPoles(), end);
          edges.push_back(detail::export_sheet_curve(developed_curve,
                                                     {developed_curve->FirstParameter(), developed_curve->LastParameter()}));
        }
      }
      result.faces.push_back(std::move(edges));
    }
    return result;
  });
}

FaceBending face_bending(const Shape& shape, const std::string& name) {
  return detail::run("face bending", [&] {
    const std::vector<int> found = shape.find_faces(name);
    if (found.empty()) throw std::invalid_argument("the body has no such face");
    FaceBending result;
    Bnd_Box box;
    constexpr int kGrid = 12;
    for (const int f : found) {
      throw_if_cancelled();
      const TopoDS_Face face = shape.face(f);
      BRepBndLib::Add(face, box);
      const BRepAdaptor_Surface surface(face);
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      const double tolerance = BRep_Tool::Tolerance(face);
      // Where the face is (a trimmed face's points inside its loops); a
      // face too narrow for the grid takes every point of its bounds.
      double inside = 0, anywhere = 0;
      bool any_inside = false;
      for (int i = 0; i < kGrid; ++i) {
        for (int j = 0; j < kGrid; ++j) {
          const double u = u0 + (u1 - u0) * (i + 0.5) / kGrid;
          const double v = v0 + (v1 - v0) * (j + 0.5) / kGrid;
          BRepLProp_SLProps props(surface, u, v, 2, 1.0e-9);
          if (!props.IsCurvatureDefined()) continue;
          const double k = std::abs(props.GaussianCurvature());
          anywhere = std::max(anywhere, k);
          BRepClass_FaceClassifier classifier;
          classifier.Perform(face, gp_Pnt2d(u, v), tolerance);
          if (classifier.State() == TopAbs_IN) {
            inside = std::max(inside, k);
            any_inside = true;
          }
        }
      }
      result.gaussian = std::max(result.gaussian, any_inside ? inside : anywhere);
    }
    if (!box.IsVoid()) result.size = std::sqrt(box.SquareExtent());
    return result;
  });
}

} // namespace mitcad::geometry
