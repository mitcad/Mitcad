// SPDX-License-Identifier: MIT
// How far a lofted body's side stands out beyond its rails (mitcad#2374):
// the body's section at a height, its outermost x on either side against
// its x where it crosses the plane y = 0, the plane its two mirrored rails
// lie in (the rails lie on the body, so there the section runs through
// them). Used by the skin suite's lofts with mirrored rails and by
// test_probe_loft_outermost on an exported body (the CLI tests of the
// reported designs).
//
// The section is found on the side faces' own surfaces, not by a boolean
// section of the body: across each face (u) the height z is reached along
// the loft (v) by regula falsi, the outermost x refined over u by golden
// section and the crossing of y = 0 by bisection over u. A section of the
// whole body (BRepAlgoAPI_Section, its curves approximated) took about 3 s
// per height on these lofts of high degree, which put the CLI test of the
// four-section bottle past its time limit; this evaluates the surfaces a
// few thousand times per face and height, to the same values. The faces
// are whole surfaces (a loft's sides; a face whose box misses the height
// is passed over); planar faces at right angles to z (the caps) are left
// out.

#pragma once

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepTools.hxx>
#include <Bnd_Box.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

namespace loft_outermost {

// The section at one height: its largest and smallest x, and its largest
// and smallest x where it crosses y = 0 (the rails' points on the right
// and on the left); `right_beyond` and `left_beyond` how far the side
// stands out beyond them (0 where the rail is the outermost line).
struct Level {
  double z = 0.0;
  double right = -std::numeric_limits<double>::max();
  double left = std::numeric_limits<double>::max();
  double right_rail = -std::numeric_limits<double>::max();
  double left_rail = std::numeric_limits<double>::max();
  double right_beyond = 0.0;
  double left_beyond = 0.0;
};

// A side face's surface over its parameter ranges, and the heights it
// spans (its box, from its geometry).
struct Side {
  BRepAdaptor_Surface surface;
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  double low = 0.0;
  double high = 0.0;
};

// The body's faces but the planar ones at right angles to z (the caps).
inline std::vector<Side> sides_of(const TopoDS_Shape& body) {
  std::vector<Side> sides;
  for (TopExp_Explorer e(body, TopAbs_FACE); e.More(); e.Next()) {
    const TopoDS_Face face = TopoDS::Face(e.Current());
    Side side{BRepAdaptor_Surface(face), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0};
    if (side.surface.GetType() == GeomAbs_Plane &&
        std::abs(side.surface.Plane().Axis().Direction().Z()) > 1.0 - 1.0e-9) {
      continue;
    }
    BRepTools::UVBounds(face, side.u0, side.u1, side.v0, side.v1);
    Bnd_Box box;
    BRepBndLib::Add(face, box, false);
    if (box.IsVoid()) {
      continue;
    }
    double x0 = 0.0;
    double y0 = 0.0;
    double x1 = 0.0;
    double y1 = 0.0;
    box.Get(x0, y0, side.low, x1, y1, side.high);
    sides.push_back(side);
  }
  return sides;
}

// The points where the surface at u reaches height z along v, in order:
// between samples on either side of it, regula falsi (the Illinois
// variant, a few steps to the last digit).
inline std::vector<gp_Pnt> crossings_at(const Side& side, double u, double z) {
  constexpr int kAlong = 16;
  std::vector<gp_Pnt> found;
  const auto height = [&](double v) { return side.surface.Value(u, v).Z() - z; };
  double a = side.v0;
  double za = height(a);
  for (int k = 1; k <= kAlong; ++k) {
    const double b = side.v0 + (side.v1 - side.v0) * k / kAlong;
    const double zb = height(b);
    if (za == 0.0) {
      found.push_back(side.surface.Value(u, a));
    } else if (zb != 0.0 && (za < 0.0) != (zb < 0.0)) {
      double lo = a;
      double hi = b;
      double flo = za;
      double fhi = zb;
      double m = lo;
      int kept = 0; // which end stayed last time: -1 lo, 1 hi
      for (int i = 0; i < 100; ++i) {
        m = (lo * fhi - hi * flo) / (fhi - flo);
        const double fm = height(m);
        if (fm == 0.0 || hi - lo <= 1.0e-15 * std::max(1.0, std::abs(hi))) {
          break;
        }
        if ((fm < 0.0) == (flo < 0.0)) {
          lo = m;
          flo = fm;
          if (kept == 1) {
            fhi /= 2;
          }
          kept = 1;
        } else {
          hi = m;
          fhi = fm;
          if (kept == -1) {
            flo /= 2;
          }
          kept = -1;
        }
        if (std::abs(fm) <= 1.0e-13 * std::max(1.0, std::abs(z))) {
          break;
        }
      }
      found.push_back(side.surface.Value(u, m));
    }
    a = b;
    za = zb;
  }
  if (za == 0.0) {
    found.push_back(side.surface.Value(u, a));
  }
  return found;
}

// The largest x times `sign` of the points where the surface at u reaches
// height z, or nothing.
inline std::optional<double> extreme_at(const Side& side, double u, double z, double sign) {
  std::optional<double> best;
  for (const gp_Pnt& p : crossings_at(side, u, z)) {
    if (!best || sign * p.X() > *best) {
      best = sign * p.X();
    }
  }
  return best;
}

// The largest of extreme_at over u between a and b around a sample's
// maximum (golden section; unimodal that near its largest sample).
inline double refined(const Side& side, double a, double b, double z, double sign, double start) {
  const double ratio = (std::sqrt(5.0) - 1.0) / 2.0;
  const auto at = [&](double u) {
    const std::optional<double> value = extreme_at(side, u, z, sign);
    return value ? *value : -std::numeric_limits<double>::max();
  };
  double best = start;
  double c = b - ratio * (b - a);
  double d = a + ratio * (b - a);
  double fc = at(c);
  double fd = at(d);
  for (int k = 0; k < 100 && b - a > 1.0e-13 * std::max(1.0, std::abs(b)); ++k) {
    if (fc > fd) {
      b = d;
      d = c;
      fd = fc;
      c = b - ratio * (b - a);
      fc = at(c);
    } else {
      a = c;
      c = d;
      fc = fd;
      d = a + ratio * (b - a);
      fd = at(d);
    }
  }
  return std::max({best, fc, fd});
}

// The section at height z of the body whose side faces are `sides`, or
// nothing where none reaches it.
inline std::optional<Level> level_at(const std::vector<Side>& sides, double z) {
  constexpr int kAcross = 128;
  constexpr double kOnPlane = 1.0e-7;
  Level level;
  level.z = z;
  bool any = false;
  for (const Side& side : sides) {
    if (z < side.low || z > side.high) {
      continue;
    }
    std::vector<double> u(kAcross + 1);
    std::vector<std::vector<gp_Pnt>> points(kAcross + 1);
    for (int k = 0; k <= kAcross; ++k) {
      const std::size_t i = static_cast<std::size_t>(k);
      u[i] = side.u0 + (side.u1 - side.u0) * k / kAcross;
      points[i] = crossings_at(side, u[i], z);
      any = any || !points[i].empty();
    }
    // The outermost x on either side, refined round the extreme sample.
    for (const double sign : {1.0, -1.0}) {
      std::optional<std::size_t> best;
      double value = -std::numeric_limits<double>::max();
      for (std::size_t i = 0; i < points.size(); ++i) {
        for (const gp_Pnt& p : points[i]) {
          if (sign * p.X() > value) {
            value = sign * p.X();
            best = i;
          }
        }
      }
      if (!best) {
        continue;
      }
      const double a = u[*best == 0 ? 0 : *best - 1];
      const double b = u[std::min(*best + 1, u.size() - 1)];
      const double x = sign * refined(side, a, b, z, sign, value);
      if (sign > 0) {
        level.right = std::max(level.right, x);
      } else {
        level.left = std::min(level.left, x);
      }
    }
    // Where the section crosses y = 0: a point within 1e-7 mm of it as it
    // is (a face ending at a rail ends there within about 1e-12, on either
    // side), else bisection over u between samples of one crossing each on
    // either side of it.
    const auto crossing = [&](double x) {
      level.right_rail = std::max(level.right_rail, x);
      level.left_rail = std::min(level.left_rail, x);
    };
    for (std::size_t i = 0; i < points.size(); ++i) {
      for (const gp_Pnt& p : points[i]) {
        if (std::abs(p.Y()) <= kOnPlane) {
          crossing(p.X());
        }
      }
      if (i + 1 == points.size() || points[i].size() != 1 || points[i + 1].size() != 1) {
        continue;
      }
      const double ya = points[i].front().Y();
      const double yb = points[i + 1].front().Y();
      if (std::abs(ya) <= kOnPlane || std::abs(yb) <= kOnPlane || (ya < 0.0) == (yb < 0.0)) {
        continue;
      }
      double lo = u[i];
      double hi = u[i + 1];
      gp_Pnt at = points[i].front();
      for (int k = 0; k < 100 && hi - lo > 1.0e-15 * std::max(1.0, std::abs(hi)); ++k) {
        const double m = (lo + hi) / 2;
        const std::vector<gp_Pnt> middle = crossings_at(side, m, z);
        if (middle.size() != 1) {
          break;
        }
        at = middle.front();
        if ((at.Y() < 0.0) == (ya < 0.0)) {
          lo = m;
        } else {
          hi = m;
        }
      }
      crossing(at.X());
    }
  }
  if (!any || level.right_rail < level.left_rail) {
    return std::nullopt;
  }
  level.right_beyond = std::max(0.0, level.right - level.right_rail);
  level.left_beyond = std::max(0.0, level.left_rail - level.left);
  return level;
}

// The levels from z0 to z1 in `steps` equal steps, the ends left out (a
// cap lies in the plane there).
inline std::vector<Level> levels(const TopoDS_Shape& body, double z0, double z1, int steps) {
  const std::vector<Side> sides = sides_of(body);
  std::vector<Level> found;
  for (int k = 1; k < steps; ++k) {
    if (const std::optional<Level> level = level_at(sides, z0 + (z1 - z0) * k / steps)) {
      found.push_back(*level);
    }
  }
  return found;
}

} // namespace loft_outermost
