// SPDX-License-Identifier: MIT
#include "sheet_oblique_cone.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>

namespace mitcad::geometry::detail {
namespace {

constexpr double kPi = 3.14159265358979323846;

} // namespace

ObliqueCone::Generator ObliqueCone::generator(double theta) const {
  const double c = std::cos(theta), s = std::sin(theta);
  const gp_Vec radial = c * x + s * y;
  const gp_Vec w = gp_Vec(apex, centre) + radius * radial;
  const gp_Vec w1 = radius * (-s * x + c * y);
  const gp_Vec w2 = -radius * radial;
  const double length = w.Magnitude();
  if (!(length > 0)) throw std::invalid_argument("the oblique cone's apex lies on its circle");
  Generator result;
  result.g = w / length;
  const double l1 = result.g.Dot(w1);
  const double l2 = (w1.Dot(w1) + w.Dot(w2) - l1 * l1) / length;
  result.dg = (w1 - l1 * result.g) / length;
  result.ddg = (w2 - 2 * l1 * result.dg - l2 * result.g) / length;
  gp_Vec n = result.g.Crossed(w1);
  const double size = n.Magnitude();
  if (!(size > 0)) throw std::invalid_argument("the oblique cone has no normal");
  n /= size;
  // Away from the inside: the inside lies towards the circle's centre.
  if (n.Dot(radial) < 0) n.Reverse();
  result.n = n;
  return result;
}

double ObliqueCone::angle(const gp_Pnt& p) const {
  const gp_Vec normal = x.Crossed(y);
  const gp_Vec d(apex, p);
  const double across = normal.Dot(d);
  if (std::abs(across) <= 1e-300) throw std::invalid_argument("the point lies on no generator of the oblique cone");
  const double s = normal.Dot(gp_Vec(apex, centre)) / across;
  const gp_Vec in_plane = gp_Vec(centre, apex) + s * d;
  return std::atan2(in_plane.Dot(y), in_plane.Dot(x));
}

gp_Pnt ObliqueCone::point(double theta, double rho, double offset) const {
  const Generator at = generator(theta);
  return apex.Translated(rho * at.g + offset * at.n);
}

ObliqueCone ObliqueCone::turned(double theta) const {
  ObliqueCone result = *this;
  const double c = std::cos(theta), s = std::sin(theta);
  result.x = c * x + s * y;
  result.y = -s * x + c * y;
  return result;
}

ObliqueDevelopment::ObliqueDevelopment(const ObliqueCone& cone, double offset, double first, double last)
    : m_cone(cone), m_offset(offset), m_first(std::min(first, 0.0)), m_last(std::max(last, 0.0)),
      m_h(2 * kPi / kObliqueSteps) {
  if (!std::isfinite(m_first) || !std::isfinite(m_last) || m_last - m_first > 4 * kPi) {
    throw std::invalid_argument("invalid oblique cone angle range");
  }
  m_low = static_cast<int>(std::floor(m_first / m_h));
  const int high = static_cast<int>(std::ceil(m_last / m_h));
  m_nodes.assign(static_cast<std::size_t>(high - m_low + 1), State{});
  const ObliqueCone::Generator start = m_cone.generator(0);
  const double speed = start.dg.Magnitude();
  if (!(speed > 0)) throw std::invalid_argument("the oblique cone's generators do not turn");
  const std::size_t zero = static_cast<std::size_t>(-m_low);
  m_nodes[zero].phi = 0;
  m_nodes[zero].base = gp_XY(-m_offset * start.n.Dot(start.ddg) / (speed * speed), 0);
  for (int i = 1; i <= high; ++i) {
    const std::size_t k = zero + static_cast<std::size_t>(i);
    m_nodes[k] = step((i - 1) * m_h, m_nodes[k - 1], m_h);
  }
  for (int i = -1; i >= m_low; --i) {
    const std::size_t k = zero - static_cast<std::size_t>(-i);
    m_nodes[k] = step((i + 1) * m_h, m_nodes[k + 1], -m_h);
  }
}

ObliqueDevelopment::State ObliqueDevelopment::step(double theta, const State& from, double h) const {
  const auto slope = [&](double t, const State& s) {
    const ObliqueCone::Generator at = m_cone.generator(t);
    const double speed = at.dg.Magnitude();
    const double k = at.n.Dot(at.ddg) / speed;
    State d;
    d.phi = speed;
    d.base = gp_XY(m_offset * k * std::sin(s.phi), -m_offset * k * std::cos(s.phi));
    return d;
  };
  const auto moved = [](const State& s, const State& d, double by) {
    State r;
    r.phi = s.phi + by * d.phi;
    r.base = s.base + by * d.base;
    return r;
  };
  const State k1 = slope(theta, from);
  const State k2 = slope(theta + h / 2, moved(from, k1, h / 2));
  const State k3 = slope(theta + h / 2, moved(from, k2, h / 2));
  const State k4 = slope(theta + h, moved(from, k3, h));
  State result;
  result.phi = from.phi + h / 6 * (k1.phi + 2 * k2.phi + 2 * k3.phi + k4.phi);
  result.base = from.base + (h / 6) * (k1.base + 2 * k2.base + 2 * k3.base + k4.base);
  return result;
}

ObliqueDevelopment::State ObliqueDevelopment::at(double theta) const {
  // The node next to theta towards 0, then one step to it: as the steps
  // run from 0.
  const int high = m_low + static_cast<int>(m_nodes.size()) - 1;
  int j = theta >= 0 ? static_cast<int>(std::floor(theta / m_h)) : static_cast<int>(std::ceil(theta / m_h));
  j = std::clamp(j, m_low, high);
  const State& node = m_nodes[static_cast<std::size_t>(j - m_low)];
  const double h = theta - j * m_h;
  if (h == 0) return node;
  return step(j * m_h, node, h);
}

double ObliqueDevelopment::phi(double theta) const { return at(theta).phi; }

gp_Pnt2d ObliqueDevelopment::base(double theta) const { return gp_Pnt2d(at(theta).base); }

gp_Pnt2d ObliqueDevelopment::chart(double theta, double rho) const {
  const State s = at(theta);
  return gp_Pnt2d(s.base + rho * gp_XY(std::cos(s.phi), std::sin(s.phi)));
}

gp_Pnt2d ObliqueDevelopment::frame_origin(double theta) const {
  const State s = at(theta);
  const ObliqueCone::Generator g = m_cone.generator(theta);
  const double speed = g.dg.Magnitude();
  const double c = -g.n.Dot(g.ddg) / (speed * speed);
  return gp_Pnt2d(s.base - m_offset * c * gp_XY(std::cos(s.phi), std::sin(s.phi)));
}

bool ObliqueDevelopment::inverse(const gp_Pnt2d& p, double& theta, double& rho) const {
  // f(theta): p across the generator at theta (positive before it); it
  // falls as theta grows where the generator runs through p on its
  // material side.
  const auto across = [&](double t, double& along) {
    const State s = at(t);
    const gp_XY d = p.XY() - s.base;
    const double c = std::cos(s.phi), sn = std::sin(s.phi);
    along = c * d.X() + sn * d.Y();
    return c * d.Y() - sn * d.X();
  };
  // A first guess: the developed angle of p, as on a cone of revolution,
  // within the range's developed angles.
  const double phi_first = phi(m_first), phi_last = phi(m_last);
  double target = std::atan2(p.Y(), p.X());
  const double middle = (phi_first + phi_last) / 2;
  target += 2 * kPi * std::round((middle - target) / (2 * kPi));
  target = std::clamp(target, phi_first, phi_last);
  double lo = m_first, hi = m_last;
  for (int i = 0; i < 64; ++i) {
    const double mid = (lo + hi) / 2;
    if (phi(mid) < target) lo = mid; else hi = mid;
  }
  double guess = (lo + hi) / 2;
  // A bracket about the guess: a before and b after the generator.
  double along = 0;
  double f = across(guess, along);
  double a = guess, b = guess;
  double fa = f, fb = f;
  const double reach = 8 * m_h;
  if (f >= 0) {
    while (fb > 0) {
      if (b >= m_last) return false;
      a = b; fa = fb;
      b = std::min(m_last, b + reach);
      fb = across(b, along);
    }
  } else {
    while (fa < 0) {
      if (a <= m_first) return false;
      b = a; fb = fa;
      a = std::max(m_first, a - reach);
      fa = across(a, along);
    }
  }
  // Newton's steps kept in the bracket, else halving it.
  double t = fa == fb ? a : a + (b - a) * fa / (fa - fb);
  for (int i = 0; i < 100; ++i) {
    f = across(t, along);
    if (f == 0) break;
    if (f > 0) a = t; else b = t;
    const ObliqueCone::Generator g = m_cone.generator(t);
    const double speed = g.dg.Magnitude();
    const double slope = -speed * along + m_offset * g.n.Dot(g.ddg) / speed;
    double next = slope < 0 ? t - f / slope : (a + b) / 2;
    if (!(next > a && next < b)) next = (a + b) / 2;
    if (std::abs(next - t) <= 1e-15 * std::max(1.0, std::abs(t)) || b - a <= 1e-15) {
      t = next;
      break;
    }
    t = next;
  }
  across(t, along);
  if (!(along > 0)) return false;
  theta = t;
  rho = along;
  return true;
}

} // namespace mitcad::geometry::detail
