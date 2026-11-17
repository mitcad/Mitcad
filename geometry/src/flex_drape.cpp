// SPDX-License-Identifier: MIT
#include "mitcad/geometry/flex_drape.hpp"

// Flex drape (mitcad#1713): the chain of a Flex item let hang, and the nodes
// it is kept by (flex_drape.hpp). The obstacles are the router's triangles
// (flex_mesh.hpp), kept for the obstacle sets last draped over: every step
// of a hanging item asks for the same set again.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <mutex>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <gp_XYZ.hxx>

#include "flex_mesh.hpp"
#include "util.hpp"

namespace mitcad::geometry {

namespace {

using detail::el;
using detail::FlexMesh;
using detail::FlexNearest;
using detail::isize;

constexpr double kTiny = 1.0e-12;

gp_XYZ xyz(const gp_Pnt& p) { return p.XYZ(); }

double norm(const gp_XYZ& v) { return v.Modulus(); }

gp_XYZ unit_or(const gp_XYZ& v, const gp_XYZ& fallback) {
  const double m = v.Modulus();
  return m > kTiny ? v / m : fallback;
}

// The meshes of the obstacle sets last draped over (two, with their bodies
// and deflections), as the router keeps its own (flex.cpp's router_of).
std::shared_ptr<const FlexMesh> mesh_of(const std::vector<ShapePtr>& obstacles, double deflection) {
  struct Kept {
    std::vector<ShapePtr> obstacles;
    double deflection = 0.0;
    std::shared_ptr<const FlexMesh> mesh;
  };
  static std::mutex mutex;
  static std::vector<Kept> kept;
  {
    const std::lock_guard<std::mutex> lock(mutex);
    for (const Kept& entry : kept) {
      if (entry.obstacles == obstacles && entry.deflection == deflection) {
        return entry.mesh;
      }
    }
  }
  std::vector<TopoDS_Shape> shapes;
  for (const ShapePtr& obstacle : obstacles) {
    shapes.push_back(obstacle ? obstacle->occt() : TopoDS_Shape());
  }
  // The router's angular deflection (FlexRouterOptions' default), the
  // linear one by the item's size (flex_drape_deflection).
  std::shared_ptr<const FlexMesh> made =
      detail::run("flex drape", [&] { return std::make_shared<const FlexMesh>(shapes, deflection, 0.2); });
  const std::lock_guard<std::mutex> lock(mutex);
  kept.push_back(Kept{obstacles, deflection, made});
  if (kept.size() > 2) {
    kept.erase(kept.begin());
  }
  return made;
}

// The name of a face of an obstacle (its first), empty for none.
std::string face_name(const std::vector<ShapePtr>& obstacles, int body, int face) {
  if (body < 0 || body >= isize(obstacles) || !el(obstacles, body) || face < 0) {
    return std::string();
  }
  const Shape& shape = *el(obstacles, body);
  if (face >= shape.face_count() || shape.face_names(face).empty()) {
    return std::string();
  }
  return shape.face_names(face).front();
}

// Where a point is pushed to stay `r` outside the obstacles: the point,
// the outward normal and the triangle it was pushed off; none when it is
// clear of them already.
struct Push {
  gp_XYZ to;
  gp_XYZ normal;
  int triangle = -1;
};

// The push of `p` by its nearest point of the obstacles `near` (asked
// within `r` or farther): as only the triangles nearer than `r` count.
std::optional<Push> push_from(const FlexMesh& mesh, const FlexNearest& near, const gp_XYZ& p, double r) {
  if (near.triangle < 0 || !((p - near.point).SquareModulus() < r * r)) {
    return std::nullopt;
  }
  const gp_XYZ from = p - near.point;
  const double d = norm(from);
  gp_XYZ n;
  if (near.inside) {
    // Out through the nearest point of the surface.
    n = d > kTiny ? from.Reversed() / d : unit_or(mesh.smooth_normal(near.triangle, near.point), gp_XYZ(0, 0, 1));
  } else {
    if (d >= r) {
      return std::nullopt;
    }
    n = d > kTiny ? from / d : unit_or(mesh.smooth_normal(near.triangle, near.point), gp_XYZ(0, 0, 1));
  }
  return Push{near.point + n * r, n, near.triangle};
}

// The tangential part of a point's move in a substep, held back by
// friction where it was pushed `depth` off a surface with normal `n`.
gp_XYZ with_friction(const gp_XYZ& at, const gp_XYZ& before, const gp_XYZ& n, double depth) {
  const gp_XYZ moved = at - before;
  const gp_XYZ along = moved - n * moved.Dot(n);
  const double slide = norm(along);
  if (slide <= kTiny) {
    return at;
  }
  const double hold = kDrapeFriction * depth;
  if (slide <= hold) {
    return at - along; // it sticks
  }
  return at - along * (hold / slide);
}

class Chain {
public:
  Chain(const FlexMesh& mesh, const FlexDrapeSpec& spec)
      : m_mesh(mesh), m_spec(spec), m_n(isize(spec.points)), m_x(static_cast<std::size_t>(m_n)),
        m_v(static_cast<std::size_t>(m_n), gp_XYZ(0, 0, 0)), m_w(static_cast<std::size_t>(m_n), 1.0) {
    for (int i = 0; i < m_n; ++i) {
      el(m_x, i) = xyz(el(spec.points, i));
      if (!spec.velocities.empty()) {
        el(m_v, i) = el(spec.velocities, i).XYZ();
      }
      if (!spec.pinned.empty() && el(spec.pinned, i) != 0) {
        el(m_w, i) = 0.0;
        el(m_v, i) = gp_XYZ(0, 0, 0);
      }
    }
    if (spec.grab && *spec.grab >= 0 && *spec.grab < m_n) {
      m_grab = *spec.grab;
      el(m_w, m_grab) = 0.0;
      el(m_v, m_grab) = gp_XYZ(0, 0, 0);
    }
    m_s = spec.segment;
    m_r = spec.resting;
    // Two runs of the chain keep the section's thickness apart (twice its
    // resting distance: a round one's diameter; a flat strip lies flat on
    // itself); neighbours along the chain that near are not checked.
    m_apart = 2.0 * m_r;
    m_skip = std::max(2, static_cast<int>(std::ceil(1.5 * m_apart / m_s)));
    m_length = m_s * (m_n - 1);
    if (spec.bend_radius > 0.0) {
      // Bending: C = |x0 + x2 - 2 x1| is s² times the curvature; its
      // energy EI/2 κ² s over the joint is (EI / s³) C² / 2, EI the
      // weight per length times the bending length cubed (the mass per
      // millimetre 1, so a point's mass is its segment's length).
      const double bending = kDrapeBendLength * spec.bend_radius;
      m_compliance = m_s * m_s * m_s / (kDrapeGravity * bending * bending * bending);
      m_bendLimit = m_s * m_s / spec.bend_radius;
    }
    m_lambda.assign(static_cast<std::size_t>(std::max(0, m_n - 2)), gp_XYZ(0, 0, 0));
    m_push.assign(static_cast<std::size_t>(m_n), gp_XYZ(0, 0, 0));
    m_damping = std::clamp(spec.damping, 0.0, 1.0);
    // The held points either side of each point (attach()): a pinned or
    // the grabbed one; they stay so while the chain runs.
    m_left.assign(static_cast<std::size_t>(m_n), -1);
    m_right.assign(static_cast<std::size_t>(m_n), -1);
    for (int i = 0, held = -1; i < m_n; ++i) {
      el(m_left, i) = held;
      if (el(m_w, i) <= 0.0) {
        held = i;
      }
    }
    for (int i = m_n - 1, held = -1; i >= 0; --i) {
      el(m_right, i) = held;
      if (el(m_w, i) <= 0.0) {
        held = i;
      }
    }
    // What each point and each segment's middle found of the obstacles
    // when it last asked (collide()): none yet.
    m_reach = kDrapeClearance * m_r;
    m_probeAt.assign(static_cast<std::size_t>(2 * m_n - 1), gp_XYZ(0, 0, 0));
    m_clear.assign(static_cast<std::size_t>(2 * m_n - 1), 0.0);
  }

  // Simulates `seconds` and what the step before left of a substep
  // (`carry`), from `still` seconds of rest, in whole substeps; the result.
  FlexDrapeResult run(double seconds, double still, double carry) {
    FlexDrapeResult out;
    out.still = still;
    const double rest_speed = std::max(kDrapeRestFloor, kDrapeRestSpeed * m_length);
    m_settleSpeed = kDrapeSettleSpeeds * rest_speed;
    const double total = seconds + carry;
    // (A whole number of substeps that rounding left a hair short of one
    // more is that one more.)
    int left = static_cast<int>(std::floor(std::min(total / kDrapeSubstep + 1.0e-6, 1.0e8)));
    out.carry = std::max(0.0, total - left * kDrapeSubstep);
    double time = 0.0;
    while (left > 0) {
      const int count = std::min(left, kDrapeFrameSubsteps);
      const double frame = count * kDrapeSubstep;
      m_start = m_x;
      for (int k = 0; k < count; ++k) {
        substep(kDrapeSubstep);
      }
      left -= count;
      time += frame;
      // The points' mean speed over the frame: a tremble from substep to
      // substep is no motion.
      double fastest = 0.0;
      for (int i = 0; i < m_n; ++i) {
        if (el(m_w, i) > 0.0) {
          fastest = std::max(fastest, norm(el(m_x, i) - el(m_start, i)) / frame);
        }
      }
      out.still = (fastest < rest_speed && m_grab < 0) ? out.still + frame : 0.0;
      if (out.still >= kDrapeRestSeconds) {
        out.carry = 0.0;
        break;
      }
    }
    out.time = time;
    out.at_rest = out.still >= kDrapeRestSeconds;
    out.substeps = m_substeps;
    out.collisions = m_collisions;
    out.queries = m_queries;
    for (int i = 0; i < m_n; ++i) {
      out.points.emplace_back(el(m_x, i));
      out.velocities.emplace_back(out.at_rest ? gp_XYZ(0, 0, 0) : el(m_v, i));
    }
    return out;
  }

  const std::vector<gp_XYZ>& points() const { return m_x; }

private:
  void substep(double h) {
    ++m_substeps;
    m_before = m_x;
    const std::vector<gp_XYZ>& before = m_before;
    const gp_XYZ g = m_spec.gravity.XYZ() * kDrapeGravity;
    // The air (kDrapeAirDamping), and harder the slower the whole chain is
    // once it settles: its last small swings die out.
    double rate = kDrapeAirDamping;
    if (m_grab < 0 && m_settleSpeed > 0.0) {
      double fastest = 0.0;
      for (int i = 0; i < m_n; ++i) {
        if (el(m_w, i) > 0.0) {
          fastest = std::max(fastest, norm(el(m_v, i)));
        }
      }
      rate += kDrapeSettleDamping * std::clamp(1.0 - fastest / m_settleSpeed, 0.0, 1.0);
    }
    const double keep = std::exp(-m_damping * rate * h);
    std::fill(m_push.begin(), m_push.end(), gp_XYZ(0, 0, 0));
    // The farthest a point moves in a substep: half the section, so that
    // it cannot pass through a wall thinner than that between two checks.
    const double most = 0.5 * m_r;
    for (int i = 0; i < m_n; ++i) {
      if (el(m_w, i) <= 0.0) {
        continue;
      }
      gp_XYZ v = (el(m_v, i) + g * h) * keep;
      gp_XYZ step = v * h;
      if (norm(step) > most) {
        step = step * (most / norm(step));
      }
      el(m_x, i) = el(m_x, i) + step;
    }
    if (m_grab >= 0) {
      const gp_XYZ to = xyz(m_spec.target) - el(m_x, m_grab);
      const double far = norm(to);
      el(m_x, m_grab) = el(m_x, m_grab) + (far > most ? to * (most / far) : to);
    }
    std::fill(m_lambda.begin(), m_lambda.end(), gp_XYZ(0, 0, 0));
    for (int round = 0; round < kDrapeIterations; ++round) {
      lengths(round % 2 == 1);
      attach();
      bends(h);
    }
    // Off the obstacles once a substep, after the rounds that move the
    // points most: the costly part, each point asking the obstacles' mesh.
    collide(before);
    // The bend radius and the lengths last, so that they hold where the
    // substep ends: in the rounds above, the bending's accumulated
    // multipliers know nothing of a joint the limit straightened and bend
    // it back toward the elastic shape in the next round, and friction
    // and the pushes off the obstacles leave the segments stretched (a
    // stretch that would stay where the item rests). Both are cheap: no
    // obstacle is asked.
    for (int round = 0; round < kDrapeIterations; ++round) {
      limits(kDrapeRoundSweeps);
      for (int pass = 0; pass < kDrapeLengthPasses; ++pass) {
        lengths(pass % 2 == 1);
      }
    }
    limits(kDrapeLimitSweeps);
    apart();
    for (int i = 0; i < m_n; ++i) {
      el(m_v, i) = el(m_w, i) > 0.0 ? (el(m_x, i) - el(before, i)) / h : gp_XYZ(0, 0, 0);
    }
    unbounce(h);
    wiggles(h);
  }

  // A push off an obstacle gives no speed away from it: a point pushed out
  // in this substep (its pushes summed, along the surfaces' normals) keeps
  // no more of its speed away from them than it had besides the pushes. A
  // point that came to the surface from where it was keeps none of it
  // anyway; one pushed out of a surface it had sunk into (by the lengths,
  // the bend radius or its own run, held after the obstacles) would
  // otherwise leave it as fast and come back: a tremble, a bounce.
  void unbounce(double h) {
    for (int i = 0; i < m_n; ++i) {
      const gp_XYZ& pushed = el(m_push, i);
      const double depth = norm(pushed);
      if (el(m_w, i) <= 0.0 || depth <= kTiny) {
        continue;
      }
      const gp_XYZ n = pushed / depth;
      const double away = el(m_v, i).Dot(n);
      if (away > 0.0) {
        el(m_v, i) = el(m_v, i) - n * std::min(away, depth / h);
      }
    }
  }

  // The item's own damping of its short wiggles (kDrapeShapeDamping): the
  // rate at which each joint's bend changes (v0 + v2 - 2 v1, nothing for a
  // chain moving or turning as a whole while it is straight) taken down by
  // a share each substep, by the points' weights. It only takes energy out.
  void wiggles(double h) {
    const double share = 1.0 - std::exp(-kDrapeShapeDamping * h);
    for (int i = 1; i + 1 < m_n; ++i) {
      const double w0 = el(m_w, i - 1);
      const double w1 = el(m_w, i);
      const double w2 = el(m_w, i + 1);
      const double sum = w0 + 4.0 * w1 + w2;
      if (sum <= 0.0) {
        continue;
      }
      const gp_XYZ change = el(m_v, i - 1) + el(m_v, i + 1) - el(m_v, i) * 2.0;
      const gp_XYZ by = change * (-share / sum);
      el(m_v, i - 1) = el(m_v, i - 1) + by * w0;
      el(m_v, i + 1) = el(m_v, i + 1) + by * w2;
      el(m_v, i) = el(m_v, i) - by * (2.0 * w1);
    }
  }

  // No point farther from the nearest held point on either side (a pinned
  // or the grabbed one) than the chain between them is long: long range
  // attachments, so that a long chain hanging from a pin does not stretch
  // under its weight between the substeps' few rounds.
  void attach() {
    for (int i = 0; i < m_n; ++i) {
      if (el(m_w, i) <= 0.0) {
        continue;
      }
      for (const int anchor : {el(m_left, i), el(m_right, i)}) {
        if (anchor < 0) {
          continue;
        }
        const gp_XYZ d = el(m_x, i) - el(m_x, anchor);
        const double l = norm(d);
        const double reach = m_s * std::abs(i - anchor);
        if (l > reach && l > kTiny) {
          el(m_x, i) = el(m_x, i) - d * ((l - reach) / l);
        }
      }
    }
  }

  // Each segment its length.
  void lengths(bool backwards) {
    for (int k = 0; k + 1 < m_n; ++k) {
      const int i = backwards ? m_n - 2 - k : k;
      const int j = i + 1;
      const double w = el(m_w, i) + el(m_w, j);
      if (w <= 0.0) {
        continue;
      }
      const gp_XYZ d = el(m_x, j) - el(m_x, i);
      const double l = norm(d);
      if (l <= kTiny) {
        continue;
      }
      const gp_XYZ fix = d * ((l - m_s) / (l * w));
      el(m_x, i) = el(m_x, i) + fix * el(m_w, i);
      el(m_x, j) = el(m_x, j) - fix * el(m_w, j);
    }
  }

  // Straight as stiff as the bend radius says (the elastic part; limits()
  // keeps it from bending tighter). The constraint is the vector d = x0 +
  // x2 - 2 x1 itself, each of its three parts linear in the points, with a
  // vector of multipliers: the same energy as its length |d|, but with no
  // turn of its direction where the joint is nearly straight, which would
  // turn the multipliers gathered in the rounds before against the bend
  // and put energy into the chain.
  void bends(double h) {
    if (m_bendLimit <= 0.0) {
      return;
    }
    const double tilde = m_compliance / (h * h);
    for (int i = 1; i + 1 < m_n; ++i) {
      const double w0 = el(m_w, i - 1) / m_s;
      const double w1 = el(m_w, i) / m_s;
      const double w2 = el(m_w, i + 1) / m_s;
      const double sum = w0 + 4.0 * w1 + w2;
      if (sum <= 0.0) {
        continue;
      }
      const gp_XYZ d = el(m_x, i - 1) + el(m_x, i + 1) - el(m_x, i) * 2.0;
      gp_XYZ& lambda = el(m_lambda, i - 1);
      const gp_XYZ dl = (d.Reversed() - lambda * tilde) / (sum + tilde);
      lambda += dl;
      apply_bend(i, dl, w0, w1, w2);
    }
  }

  // Never bent tighter than the bend radius: each joint over the limit
  // straightened to it, in sweeps from either end in turn until none is
  // over it by more than a thousandth of the limit (or `sweeps`):
  // straightening a joint bends its neighbours more, so that one sweep
  // leaves the joints behind it over the limit again.
  void limits(int sweeps) {
    if (m_bendLimit <= 0.0) {
      return;
    }
    for (int sweep = 0; sweep < sweeps; ++sweep) {
      const bool backwards = sweep % 2 == 1;
      double worst = 0.0;
      for (int k = 1; k + 1 < m_n; ++k) {
        const int i = backwards ? m_n - 1 - k : k;
        const double w0 = el(m_w, i - 1) / m_s;
        const double w1 = el(m_w, i) / m_s;
        const double w2 = el(m_w, i + 1) / m_s;
        const double sum = w0 + 4.0 * w1 + w2;
        if (sum <= 0.0) {
          continue;
        }
        const gp_XYZ d = el(m_x, i - 1) + el(m_x, i + 1) - el(m_x, i) * 2.0;
        const double c = norm(d);
        if (c > m_bendLimit) {
          worst = std::max(worst, c - m_bendLimit);
          apply_bend(i, d * (-(c - m_bendLimit) / (c * sum)), w0, w1, w2);
        }
      }
      if (worst <= 1.0e-3 * m_bendLimit) {
        break;
      }
    }
  }

  // A joint's correction `by` (along d's gradient: its ends one way, its
  // middle twice the other), by the points' weights.
  void apply_bend(int i, const gp_XYZ& by, double w0, double w1, double w2) {
    el(m_x, i - 1) = el(m_x, i - 1) + by * w0;
    el(m_x, i + 1) = el(m_x, i + 1) + by * w2;
    el(m_x, i) = el(m_x, i) - by * (2.0 * w1);
  }

  // Whether `p` (probe `k`: a point, or the middle of the segment after
  // point k - n) is pushed off the obstacles. The mesh is asked only once
  // the probe may have come within the resting distance of them since it
  // was asked last: asked within kDrapeClearance resting distances more,
  // what it finds clear there (less the resting distance) it may move
  // before it asks again.
  std::optional<Push> probe(int k, const gp_XYZ& p) {
    const double clear = el(m_clear, k);
    if (clear > 0.0 && (p - el(m_probeAt, k)).SquareModulus() < clear * clear) {
      return std::nullopt;
    }
    ++m_queries;
    const FlexNearest near = m_mesh.nearest(p, m_r + m_reach);
    std::optional<Push> push = push_from(m_mesh, near, p, m_r);
    el(m_probeAt, k) = p;
    if (push || near.inside) {
      el(m_clear, k) = 0.0; // pushed (or in a body): asked again next time
    } else {
      el(m_clear, k) = near.triangle < 0 ? m_reach : std::max(0.0, (p - near.point).Modulus() - m_r);
    }
    return push;
  }

  // Off the obstacles: each free or grabbed point, and each segment's
  // middle (its two ends moved by their weights), with friction.
  void collide(const std::vector<gp_XYZ>& before) {
    ++m_collisions;
    if (m_mesh.empty()) {
      return;
    }
    for (int i = 0; i < m_n; ++i) {
      if (el(m_w, i) <= 0.0 && i != m_grab) {
        continue;
      }
      if (const std::optional<Push> push = probe(i, el(m_x, i))) {
        const double depth = norm(push->to - el(m_x, i));
        el(m_x, i) = i == m_grab ? push->to : with_friction(push->to, el(before, i), push->normal, depth);
        el(m_push, i) += push->normal * depth;
      }
    }
    for (int i = 0; i + 1 < m_n; ++i) {
      const int j = i + 1;
      const double w = el(m_w, i) + el(m_w, j);
      if (w <= 0.0) {
        continue;
      }
      const gp_XYZ middle = (el(m_x, i) + el(m_x, j)) * 0.5;
      const std::optional<Push> push = probe(m_n + i, middle);
      if (!push) {
        continue;
      }
      const gp_XYZ by = push->to - middle;
      el(m_x, i) = el(m_x, i) + by * (2.0 * el(m_w, i) / w);
      el(m_x, j) = el(m_x, j) + by * (2.0 * el(m_w, j) / w);
      el(m_push, i) += by * (2.0 * el(m_w, i) / w);
      el(m_push, j) += by * (2.0 * el(m_w, j) / w);
    }
  }

  // Apart from its own run: points of the chain that are not near
  // neighbours keep `m_apart` between them (a grid of that size finds
  // them: the points' cells, sorted, kept from substep to substep).
  void apart() {
    if (m_n <= m_skip + 1) {
      return;
    }
    const double cell = m_apart;
    const auto key = [cell](const gp_XYZ& p, int dx, int dy, int dz) {
      const auto c = [cell](double v, int d) {
        return static_cast<std::int64_t>(std::floor(v / cell)) + d;
      };
      const std::int64_t x = c(p.X(), dx);
      const std::int64_t y = c(p.Y(), dy);
      const std::int64_t z = c(p.Z(), dz);
      return (x * 73856093) ^ (y * 19349663) ^ (z * 83492791);
    };
    m_cells.clear();
    for (int i = 0; i < m_n; ++i) {
      m_cells.emplace_back(key(el(m_x, i), 0, 0, 0), i);
    }
    std::sort(m_cells.begin(), m_cells.end());
    for (int i = 0; i < m_n; ++i) {
      for (int dx = -1; dx <= 1; ++dx) {
        for (int dy = -1; dy <= 1; ++dy) {
          for (int dz = -1; dz <= 1; ++dz) {
            const std::int64_t at = key(el(m_x, i), dx, dy, dz);
            const auto first = std::lower_bound(m_cells.begin(), m_cells.end(), std::make_pair(at, -1));
            for (auto found = first; found != m_cells.end() && found->first == at; ++found) {
              const int j = found->second;
              if (j <= i + m_skip) {
                continue;
              }
              const double w = el(m_w, i) + el(m_w, j);
              const gp_XYZ d = el(m_x, j) - el(m_x, i);
              const double l = norm(d);
              if (w <= 0.0 || l >= m_apart || l <= kTiny) {
                continue;
              }
              const gp_XYZ fix = d * ((l - m_apart) / (l * w));
              el(m_x, i) = el(m_x, i) + fix * el(m_w, i);
              el(m_x, j) = el(m_x, j) - fix * el(m_w, j);
            }
          }
        }
      }
    }
  }

  const FlexMesh& m_mesh;
  const FlexDrapeSpec& m_spec;
  int m_n = 0;
  std::vector<gp_XYZ> m_x;
  std::vector<gp_XYZ> m_v;
  std::vector<double> m_w;
  // The bending's multipliers, one vector for each joint (bends()).
  std::vector<gp_XYZ> m_lambda;
  // How far each point was pushed off the obstacles in the substep
  // (unbounce()).
  std::vector<gp_XYZ> m_push;
  double m_damping = kDrapeDefaultDamping;
  // Slower than this (mm/s), the air damps the chain harder.
  double m_settleSpeed = 0.0;
  int m_grab = -1;
  double m_s = 0.0;
  double m_r = 0.0;
  double m_apart = 0.0;
  int m_skip = 2;
  double m_length = 0.0;
  double m_compliance = 0.0;
  double m_bendLimit = 0.0;
  // The held points either side of each point, -1 for none (attach()).
  std::vector<int> m_left;
  std::vector<int> m_right;
  // Each probe's point where it last asked the obstacles and how far it
  // may move from there before it asks again (probe()), and how far beyond
  // the resting distance it asks.
  std::vector<gp_XYZ> m_probeAt;
  std::vector<double> m_clear;
  double m_reach = 0.0;
  // Kept from substep to substep: the points where the substep and the
  // frame began, the grid's cells (apart()).
  std::vector<gp_XYZ> m_before;
  std::vector<gp_XYZ> m_start;
  std::vector<std::pair<std::int64_t, int>> m_cells;
  // The work done (FlexDrapeResult's).
  int m_substeps = 0;
  int m_collisions = 0;
  int m_queries = 0;
};

// The chain's tangents (by its neighbours).
std::vector<gp_XYZ> tangents(const std::vector<gp_XYZ>& x) {
  const int n = isize(x);
  std::vector<gp_XYZ> out(static_cast<std::size_t>(n));
  gp_XYZ last(1, 0, 0);
  for (int i = 0; i < n; ++i) {
    const gp_XYZ& a = el(x, std::max(0, i - 1));
    const gp_XYZ& b = el(x, std::min(n - 1, i + 1));
    last = unit_or(b - a, last);
    el(out, i) = last;
  }
  return out;
}

// `y` made square to the unit `t`; none when it is along it.
std::optional<gp_XYZ> square_to(const gp_XYZ& y, const gp_XYZ& t) {
  const gp_XYZ s = y - t * y.Dot(t);
  const double m = norm(s);
  if (m <= 1.0e-6) {
    return std::nullopt;
  }
  return s / m;
}

// The section's y along the chain: the surface's normal where a point lies
// on one, carried along between with the least turn and blended between
// the two sides; against gravity where nothing holds it.
std::vector<gp_XYZ> section_ys(const std::vector<gp_XYZ>& x, const std::vector<FlexDrapeContact>& contacts,
                               const gp_Dir& gravity) {
  const int n = isize(x);
  const std::vector<gp_XYZ> t = tangents(x);
  std::vector<std::optional<gp_XYZ>> known(static_cast<std::size_t>(n));
  for (int i = 0; i < n; ++i) {
    if (el(contacts, i).obstacle >= 0) {
      el(known, i) = square_to(el(contacts, i).normal.XYZ(), el(t, i));
    }
  }
  const gp_XYZ up = gravity.XYZ().Reversed();
  const auto carry = [&](int from, int to, int step, std::vector<std::optional<gp_XYZ>>& out,
                         std::vector<int>& source) {
    std::optional<gp_XYZ> y;
    int at = -1;
    for (int i = from; i != to; i += step) {
      if (el(known, i)) {
        y = el(known, i);
        at = i;
      } else if (y) {
        y = square_to(*y, el(t, i));
        if (!y) {
          at = -1;
        }
      }
      el(out, i) = y;
      el(source, i) = at;
    }
  };
  std::vector<std::optional<gp_XYZ>> ahead(static_cast<std::size_t>(n));
  std::vector<std::optional<gp_XYZ>> behind(static_cast<std::size_t>(n));
  std::vector<int> from_ahead(static_cast<std::size_t>(n), -1);
  std::vector<int> from_behind(static_cast<std::size_t>(n), -1);
  carry(0, n, 1, ahead, from_ahead);
  carry(n - 1, -1, -1, behind, from_behind);
  std::vector<gp_XYZ> out(static_cast<std::size_t>(n));
  // Where nothing lies on a surface: against gravity, carried along.
  std::optional<gp_XYZ> free_y;
  for (int i = 0; i < n; ++i) {
    const std::optional<gp_XYZ>& a = el(ahead, i);
    const std::optional<gp_XYZ>& b = el(behind, i);
    std::optional<gp_XYZ> y;
    if (a && b && el(from_ahead, i) >= 0 && el(from_behind, i) >= 0 && el(from_behind, i) != el(from_ahead, i)) {
      const double span = static_cast<double>(el(from_behind, i) - el(from_ahead, i));
      const double f = span > 0.0 ? (i - el(from_ahead, i)) / span : 0.5;
      y = square_to(*a * (1.0 - f) + *b * f, el(t, i));
      if (!y) {
        y = a;
      }
    } else if (a) {
      y = a;
    } else if (b) {
      y = b;
    }
    if (!y) {
      free_y = free_y ? square_to(*free_y, el(t, i)) : square_to(up, el(t, i));
      if (!free_y) {
        free_y = square_to(gp_XYZ(1, 0, 0), el(t, i));
      }
      if (!free_y) {
        free_y = square_to(gp_XYZ(0, 1, 0), el(t, i));
      }
      y = free_y;
    }
    el(out, i) = y.value_or(gp_XYZ(0, 0, 1));
  }
  return out;
}

// What each point of the chain lies on: the nearest obstacle within its
// resting distance and a little more (the surface's deflection twice, at
// least 0.02 mm) of the point or of the middle of a segment beside it. On
// a convex surface the segments' middles are what touches it (collide()
// holds them off), and the points between rest farther off by the
// segment's sagitta there (about 0.36 mm for 10 mm segments on a radius of
// 35 mm; more round an edge).
std::vector<FlexDrapeContact> contacts_of(const FlexMesh& mesh, const std::vector<ShapePtr>& obstacles,
                                          const std::vector<gp_XYZ>& x, double resting, int& queries) {
  std::vector<FlexDrapeContact> out(x.size());
  if (mesh.empty()) {
    return out;
  }
  const double within = resting + std::max(0.02, 2.0 * mesh.deflection());
  const int count = isize(x);
  // Each point's and each segment's middle's nearest, asked once: a middle
  // is beside two points.
  std::vector<gp_XYZ> middles(static_cast<std::size_t>(std::max(0, count - 1)));
  std::vector<FlexNearest> at_points(static_cast<std::size_t>(count));
  std::vector<FlexNearest> at_middles(middles.size());
  for (int i = 0; i < count; ++i) {
    el(at_points, i) = mesh.nearest(el(x, i), within);
  }
  for (int i = 0; i + 1 < count; ++i) {
    el(middles, i) = (el(x, i) + el(x, i + 1)) * 0.5;
    el(at_middles, i) = mesh.nearest(el(middles, i), within);
  }
  queries += count + isize(middles);
  for (int i = 0; i < count; ++i) {
    // The point, then the middles of its segments: the nearest of them.
    FlexNearest near;
    gp_XYZ probe = el(x, i);
    for (int side = 0; side < 3; ++side) {
      const int j = side == 0 ? i : (side == 1 ? i - 1 : i + 1);
      if (j < 0 || j >= count) {
        continue;
      }
      const int middle = std::min(i, j);
      const gp_XYZ at = side == 0 ? el(x, i) : el(middles, middle);
      const FlexNearest& found = side == 0 ? el(at_points, i) : el(at_middles, middle);
      if (found.triangle >= 0 && (near.triangle < 0 || found.distance < near.distance)) {
        near = found;
        probe = at;
      }
    }
    if (near.triangle < 0) {
      continue;
    }
    const detail::FlexTriangle& triangle = mesh.triangle(near.triangle);
    FlexDrapeContact& contact = el(out, i);
    contact.obstacle = triangle.body;
    contact.face = face_name(obstacles, triangle.body, triangle.face);
    const gp_XYZ from = probe - near.point;
    const gp_XYZ smooth = unit_or(mesh.smooth_normal(near.triangle, near.point), gp_XYZ(0, 0, 1));
    const gp_XYZ n = near.inside || norm(from) <= 1.0e-6 ? smooth : from / norm(from);
    contact.normal = gp_Dir(n);
  }
  return out;
}

void check_chain(const FlexDrapeSpec& spec) {
  const std::size_t n = spec.points.size();
  if (n < 2) {
    throw std::invalid_argument("a hanging item needs two points or more");
  }
  detail::require_positive("the length of a hanging item's segments", spec.segment);
  detail::require_positive("the distance of a hanging item from the surface", spec.resting);
  if ((!spec.velocities.empty() && spec.velocities.size() != n) || (!spec.pinned.empty() && spec.pinned.size() != n)) {
    throw std::invalid_argument("a hanging item's velocities and pins must be one for each point");
  }
  if (!(spec.damping >= 0.0 && spec.damping <= 1.0)) {
    throw std::invalid_argument("a hanging item's damping must be from 0 to 1");
  }
  for (const gp_Pnt& p : spec.points) {
    if (!std::isfinite(p.X()) || !std::isfinite(p.Y()) || !std::isfinite(p.Z())) {
      throw std::invalid_argument("a hanging item's points must be finite");
    }
  }
}

// The distance from `p` to the polyline `line` (points `from` to `to`).
double polyline_distance(const gp_XYZ& p, const std::vector<gp_XYZ>& line, int from, int to) {
  double best = detail::kFlexFar;
  from = std::max(0, from);
  to = std::min(isize(line) - 1, to);
  for (int k = from; k < to; ++k) {
    const gp_XYZ& a = el(line, k);
    const gp_XYZ ab = el(line, k + 1) - a;
    const double l2 = ab.SquareModulus();
    const double t = l2 > kTiny ? std::clamp((p - a).Dot(ab) / l2, 0.0, 1.0) : 0.0;
    best = std::min(best, norm(p - (a + ab * t)));
  }
  if (from == to && from >= 0 && from < isize(line)) {
    best = norm(p - el(line, from));
  }
  return best;
}

// Douglas–Peucker: the points between `a` and `b` that hold the polyline
// of the chain within `tolerance` of the chords.
void simplify(const std::vector<gp_XYZ>& x, int a, int b, double tolerance, std::set<int>& out) {
  if (b <= a + 1) {
    return;
  }
  std::vector<gp_XYZ> chord{el(x, a), el(x, b)};
  int worst = -1;
  double far = tolerance;
  for (int i = a + 1; i < b; ++i) {
    const double d = polyline_distance(el(x, i), chord, 0, 1);
    if (d > far) {
      far = d;
      worst = i;
    }
  }
  if (worst < 0) {
    return;
  }
  out.insert(worst);
  simplify(x, a, worst, tolerance, out);
  simplify(x, worst, b, tolerance, out);
}

} // namespace

double flex_drape_segment(double length, double resting, double bend_radius) {
  double segment = 2.0 * resting;
  if (bend_radius > 0.0) {
    segment = std::min(segment, 0.5 * bend_radius);
  }
  if (length > 0.0) {
    segment = std::max(segment, length / kDrapeMostSegments);
  }
  return segment;
}

double flex_drape_deflection(double resting) { return std::clamp(resting / 20.0, 0.1, 0.5); }

FlexDrapeResult flex_drape_step(const std::vector<ShapePtr>& obstacles, const FlexDrapeSpec& spec) {
  check_chain(spec);
  if (!(spec.seconds >= 0.0) || !std::isfinite(spec.seconds)) {
    throw std::invalid_argument("the time a hanging item is simulated must be a number not below zero");
  }
  if (!(spec.carry >= 0.0) || !std::isfinite(spec.carry)) {
    throw std::invalid_argument("the time a hanging item carries over must be a number not below zero");
  }
  const std::shared_ptr<const FlexMesh> mesh = mesh_of(obstacles, flex_drape_deflection(spec.resting));
  Chain chain(*mesh, spec);
  FlexDrapeResult out = chain.run(spec.seconds, spec.still, std::min(spec.carry, kDrapeSubstep));
  out.contacts = contacts_of(*mesh, obstacles, chain.points(), spec.resting, out.queries);
  for (const gp_XYZ& y : section_ys(chain.points(), out.contacts, spec.gravity)) {
    out.normals.emplace_back(y);
  }
  return out;
}

FlexDrapeNodes flex_drape_nodes(const std::vector<ShapePtr>& obstacles, const FlexDrapeNodesSpec& spec) {
  const int n = isize(spec.points);
  if (n < 2) {
    throw std::invalid_argument("a hanging item needs two points or more");
  }
  if (spec.contacts.size() != spec.points.size()) {
    throw std::invalid_argument("a hanging item's contacts must be one for each point");
  }
  detail::require_positive("the tolerance of a hanging item's nodes", spec.tolerance);
  std::vector<gp_XYZ> x;
  for (const gp_Pnt& p : spec.points) {
    x.push_back(p.XYZ());
  }
  const auto on_face = [&](int i) {
    const FlexDrapeContact& c = el(spec.contacts, i);
    return c.obstacle >= 0 && c.obstacle < isize(obstacles) && !c.face.empty();
  };

  // No two nodes nearer along the chain than the item's own thickness or
  // its bend radius, but the points to keep (mitcad#1713: a pinned end
  // lies on the surface it is attached to and the chain's next point the
  // resting distance off it, 2 mm along; that point, where the chain left
  // the surface, became a node, and the route, lifted off the surface at
  // both, ran down to the one and back up to the pinned end: a turn back
  // the solid was cut short at, most of the item gone on Keep Shape).
  double chain = 0.0;
  for (int i = 1; i < n; ++i) {
    chain += norm(el(x, i) - el(x, i - 1));
  }
  const double gap = std::max(2.0 * spec.resting, spec.bend_radius.value_or(0.0));
  const int apart =
      chain > 0.0 ? std::max(1, static_cast<int>(std::ceil(gap / (chain / (n - 1)) - 1.0e-9))) : 1;
  std::set<int> nodes{0, n - 1};
  for (const int k : spec.keep) {
    if (k >= 0 && k < n) {
      nodes.insert(k);
    }
  }
  const std::shared_ptr<const FlexMesh> mesh = mesh_of(obstacles, flex_drape_deflection(spec.resting));
  // The points no node is made at: where the item cannot lie (in a body,
  // or nearer to one than its resting distance by more than a twentieth of
  // it, at least 0.1 mm), and where it only touches a face over its edge
  // (the way from the face to the point more than 30 degrees off the
  // face's normal there), which the route, lifting a node on a face along the face's
  // normal, does not give back (mitcad#1713: a 5 mm cable pinned at the
  // rim of a profile's 4.3 mm bore hung down into the bore, through its
  // wall and into the slot beside it; a node kept in the bore made the
  // route dip into it and out again, 0.3 mm round, and the solid along
  // that was the cable in a few short pieces). The route finds its own way
  // past them.
  std::vector<char> unfit(static_cast<std::size_t>(n), 0);
  {
    const double slack = std::max(0.05 * spec.resting, 0.1);
    const double square = std::sqrt(3.0) / 2.0; // cos 30 degrees
    for (int i = 0; i < n; ++i) {
      const FlexNearest near = mesh->nearest(el(x, i), spec.resting);
      if (near.triangle >= 0 && (near.inside || near.distance < spec.resting - slack)) {
        el(unfit, i) = 1;
      } else if (on_face(i)) {
        const FlexDrapeContact& c = el(spec.contacts, i);
        const FlexNearest hit = mesh->nearest_on(el(x, i), c.obstacle, -1);
        if (hit.triangle >= 0 &&
            unit_or(mesh->smooth_normal(hit.triangle, hit.point), c.normal.XYZ()).Dot(c.normal.XYZ()) < square) {
          el(unfit, i) = 1;
        }
      }
    }
  }
  // Whether a point has no node within `apart` points of it, and one may be
  // made there.
  const auto room = [&](int i) {
    const auto next = nodes.lower_bound(i - apart + 1);
    return el(unfit, i) == 0 && (next == nodes.end() || *next > i + apart - 1);
  };
  // Where the chain meets and leaves a surface.
  for (int i = 1; i < n; ++i) {
    if (on_face(i) != on_face(i - 1)) {
      const int at = on_face(i) ? i : i - 1;
      if (room(at)) {
        nodes.insert(at);
      }
    }
  }
  // Between them, the polyline's own points.
  {
    const std::vector<int> fixed(nodes.begin(), nodes.end());
    std::set<int> found;
    for (std::size_t k = 0; k + 1 < fixed.size(); ++k) {
      simplify(x, fixed[k], fixed[k + 1], spec.tolerance, found);
    }
    for (const int i : found) {
      if (room(i)) {
        nodes.insert(i);
      }
    }
  }

  FlexDrapeNodes out;
  for (int round = 0; round <= kDrapeNodeRounds; ++round) {
    out.nodes.assign(nodes.begin(), nodes.end());
    FlexRouteSpec route;
    route.feature = "drape";
    route.resting = spec.resting;
    route.half_width = spec.half_width;
    route.reach = spec.reach;
    route.bend_radius = spec.bend_radius;
    route.bend_radius_edge = spec.bend_radius_edge;
    route.max_twist = spec.max_twist;
    for (std::size_t k = 0; k < out.nodes.size(); ++k) {
      const int i = out.nodes[k];
      FlexNode node;
      node.point = el(spec.points, i);
      const bool first = k == 0;
      const bool last = k + 1 == out.nodes.size();
      if (first && spec.start_direction) {
        node.direction = spec.start_direction;
        node.x_axis = spec.start_x_axis;
      } else if (last && spec.end_direction) {
        node.direction = spec.end_direction;
        node.x_axis = spec.end_x_axis;
      } else if (on_face(i)) {
        // On the face: the surface's point below the centre line; the
        // nearest point of the body where the chain is nearer to it than
        // the resting distance (a pinned end on the surface it is attached
        // to, kept there as it was: the route then lifts it as the item's
        // own route will).
        const FlexDrapeContact& c = el(spec.contacts, i);
        node.obstacle = c.obstacle;
        node.face = c.face;
        node.normal = c.normal;
        node.point = gp_Pnt(el(x, i) - c.normal.XYZ() * spec.resting);
        const FlexNearest hit = mesh->nearest_on(el(x, i), c.obstacle, -1);
        const bool sunk = hit.inside && hit.distance > 1.0e-9;
        if (hit.triangle >= 0 && !sunk && hit.distance < spec.resting) {
          node.point = gp_Pnt(hit.point);
        }
      }
      route.nodes.push_back(std::move(node));
    }
    FlexRoute routed;
    try {
      routed = flex_route(obstacles, route);
    } catch (const std::exception& failure) {
      // No route through them: kept, the item would have none either.
      throw std::runtime_error(std::string("no route could be made through the points the item rests at: ") +
                               failure.what());
    }
    std::vector<gp_XYZ> line;
    std::vector<int> at(out.nodes.size(), -1);
    for (const FlexSample& sample : routed.samples) {
      if (sample.node >= 0 && sample.node < isize(at)) {
        el(at, sample.node) = isize(line);
      }
      line.push_back(sample.point.XYZ());
    }
    // Each stretch's farthest point from the route between its nodes (of
    // those with room for a node, the farthest to add).
    out.deviation = 0.0;
    std::vector<int> worst;
    for (std::size_t k = 0; k + 1 < out.nodes.size(); ++k) {
      const int a = out.nodes[k];
      const int b = out.nodes[k + 1];
      const int from = at[k] >= 0 ? at[k] : 0;
      const int to = at[k + 1] >= 0 ? at[k + 1] : isize(line) - 1;
      double far = 0.0;
      double far_with_room = 0.0;
      int farthest = -1;
      for (int i = a + 1; i < b; ++i) {
        const double d = polyline_distance(el(x, i), line, std::min(from, to), std::max(from, to));
        far = std::max(far, d);
        if (d > far_with_room && room(i)) {
          far_with_room = d;
          farthest = i;
        }
      }
      out.deviation = std::max(out.deviation, far);
      if (far_with_room > spec.tolerance && farthest >= 0) {
        worst.push_back(farthest);
      }
    }
    if (worst.empty() || round == kDrapeNodeRounds) {
      break;
    }
    nodes.insert(worst.begin(), worst.end());
  }
  return out;
}

} // namespace mitcad::geometry
