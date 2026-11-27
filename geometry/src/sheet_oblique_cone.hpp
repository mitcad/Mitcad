// SPDX-License-Identifier: MIT
#pragma once

// Oblique (eccentric) cones as sheet panels (mitcad#2491): the cone of the
// lines from an apex through a circle whose centre need not lie below it,
// and the development of a surface a constant distance from it along its
// normals (the sheet's neutral surface), which is developable but no cone
// itself: its generators keep their directions and lengths, and the curve
// they start from (the apex moved along the normals) develops into a small
// curve near the chart's origin.
//
// The development is integrated along the angle `theta` round the circle
// from 0 with the classical Runge-Kutta rule at steps of a whole turn over
// kObliqueSteps; the Rust side (`ObliqueConeEmbedding` in
// core/model-base/src/sheet_charts.rs) integrates the same way, so both
// read the same chart.

#include <vector>

#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_XY.hxx>

namespace mitcad::geometry::detail {

constexpr int kObliqueSteps = 4096;

// The cone of the lines from `apex` through the circle about `centre` of
// `radius` in the plane of the unit vectors `x` and `y` (at right angles):
// its generator at `theta` runs through centre + radius (cos theta x +
// sin theta y).
struct ObliqueCone {
  gp_Pnt apex;
  gp_Pnt centre;
  gp_Vec x{1, 0, 0};
  gp_Vec y{0, 1, 0};
  double radius = 0;

  // The unit generator at `theta` and its first two derivatives by theta,
  // and the unit normal away from the cone's inside (the same all along
  // the generator).
  struct Generator {
    gp_Vec g, dg, ddg, n;
  };
  Generator generator(double theta) const;
  // The angle of the generator through `p` (the line from the apex through
  // it meets the circle's plane there), in (-pi, pi].
  double angle(const gp_Pnt& p) const;
  // apex + rho g(theta) + offset n(theta).
  gp_Pnt point(double theta, double rho, double offset) const;
  // The cone turned about the circle's centre: its angle 0 where `theta`
  // was.
  ObliqueCone turned(double theta) const;
};

// The development of the surface `offset` from the cone along its normals
// (outwards, away from its inside) over the angles [first, last] (which
// hold 0): chart(theta, rho) = base(theta) + rho (cos phi, sin phi), phi
// the developed angle (phi(0) = 0, phi' = |g'|), base the development of
// the apex moved along the normals (base' = -offset (n . g'') / |g'| times
// the chart direction across the generator, base(0) = offset c(0) (1, 0),
// c = -(n . g'') / |g'|^2), rho the distance along the generator from the
// cone's apex. On a cone of revolution base is offset cot(half angle)
// along the generator: the neutral cone's apex.
class ObliqueDevelopment {
public:
  ObliqueDevelopment(const ObliqueCone& cone, double offset, double first, double last);

  double phi(double theta) const;
  gp_Pnt2d base(double theta) const;
  gp_Pnt2d chart(double theta, double rho) const;
  // The point where the generator at theta develops from the chart's
  // origin as on a cone of revolution: base - offset c (cos phi, sin phi).
  gp_Pnt2d frame_origin(double theta) const;
  // The angle and the distance along the generator of a chart point,
  // theta within [first, last]; false where no generator of the range
  // runs through it on its material side.
  bool inverse(const gp_Pnt2d& p, double& theta, double& rho) const;
  double first() const { return m_first; }
  double last() const { return m_last; }

private:
  struct State {
    double phi = 0;
    gp_XY base;
  };
  State step(double theta, const State& from, double h) const;
  State at(double theta) const;

  ObliqueCone m_cone;
  double m_offset = 0;
  double m_first = 0, m_last = 0;
  double m_h = 0;
  int m_low = 0; // node index of the first node
  std::vector<State> m_nodes;
};

} // namespace mitcad::geometry::detail
