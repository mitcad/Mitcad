// SPDX-License-Identifier: MIT
#include "mitcad/geometry/flex.hpp"

// Flex (mitcad#1713): the model's side of the router (flex_route.hpp). The
// routes come from a FlexRouter of the obstacles, the solids from its
// build_solid; here the two sides' terms are matched:
//
// - A section's y here (a sample's `normal`, away from the surface) is the
//   router's `down` reversed; its x, normal x tangent, is the router's
//   across, tangent x down.
// - A port's `direction` points out of the item it belongs to: the route
//   leaves the first node along it and reaches the last against it. The
//   router takes the way the route runs at either end.
// - A section whose reach is its resting distance every way is round to
//   the router; any other a rectangle of the two half sizes.
// - A bend radius that is not given means "does not bend that way" for a
//   section that is not round, and no limit for a round one.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "flex_mesh.hpp"
#include "mitcad/geometry/flex_route.hpp"
#include "util.hpp"

namespace mitcad::geometry {

namespace {

// MITCAD_FLEX_TRACE: a line on standard error for each router made (its
// obstacles meshed), route and solid, with its time: where an item's
// preview spends its time (mitcad#1713) without a profiler.
bool tracing() { return detail::flex_tracing(); }

using Clock = std::chrono::steady_clock;

double seconds_since(Clock::time_point start) {
  return std::chrono::duration<double>(Clock::now() - start).count();
}

// The routers of the obstacle sets last routed through: a router keeps its
// obstacles' triangles and what it searches them with, which a route of the
// same bodies (every move of a dragged node) needs again. The two last sets
// are kept, with their bodies.
std::shared_ptr<const routing::FlexRouter> router_of(const std::vector<ShapePtr>& obstacles) {
  struct Kept {
    std::vector<ShapePtr> obstacles;
    std::shared_ptr<const routing::FlexRouter> router;
  };
  static std::mutex mutex;
  static std::vector<Kept> kept;
  {
    const std::lock_guard<std::mutex> lock(mutex);
    for (const Kept& entry : kept) {
      if (entry.obstacles == obstacles) {
        return entry.router;
      }
    }
  }
  std::vector<TopoDS_Shape> shapes;
  for (const ShapePtr& obstacle : obstacles) {
    shapes.push_back(obstacle ? obstacle->occt() : TopoDS_Shape());
  }
  const Clock::time_point started = Clock::now();
  std::shared_ptr<const routing::FlexRouter> made = std::make_shared<const routing::FlexRouter>(shapes);
  if (tracing()) {
    std::fprintf(stderr, "flex-trace: router of %d bodies made in %.3f s\n", static_cast<int>(shapes.size()),
                 seconds_since(started));
  }
  const std::lock_guard<std::mutex> lock(mutex);
  kept.push_back(Kept{obstacles, made});
  if (kept.size() > 2) {
    kept.erase(kept.begin());
  }
  return made;
}

FlexProblemKind kind_of(routing::FlexProblemKind kind) {
  switch (kind) {
  case routing::FlexProblemKind::PassesThrough:
    return FlexProblemKind::PassesThrough;
  case routing::FlexProblemKind::BendTooTight:
    return FlexProblemKind::BendTooTight;
  case routing::FlexProblemKind::TwistOverLimit:
    return FlexProblemKind::TwistOverLimit;
  case routing::FlexProblemKind::EdgeBendTooTight:
    return FlexProblemKind::EdgeBendTooTight;
  case routing::FlexProblemKind::NoCleanRoute:
    break;
  }
  return FlexProblemKind::NoCleanRoute;
}

} // namespace

FlexRoute flex_route(const std::vector<ShapePtr>& obstacles, const FlexRouteSpec& spec) {
  if (spec.nodes.size() < 2) {
    throw std::invalid_argument("a flex item needs two nodes or more");
  }
  detail::require_positive("the distance of a flex item's route from the surface", spec.resting);
  const double half_width = spec.half_width > 0.0 ? spec.half_width : spec.resting;
  const double reach = spec.reach > 0.0 ? spec.reach : std::max(spec.resting, half_width);
  const bool round = std::abs(half_width - spec.resting) < 1.0e-9 && std::abs(reach - spec.resting) < 1.0e-9;

  routing::FlexSection section;
  section.kind = round ? routing::FlexSectionKind::Round : routing::FlexSectionKind::Rectangular;
  section.width = 2.0 * half_width;
  section.height = 2.0 * spec.resting;
  routing::FlexLimits limits;
  limits.bend_radius = spec.bend_radius.value_or(0.0);
  limits.edge_bend_radius = spec.bend_radius_edge.value_or(0.0);
  limits.max_twist = spec.max_twist.value_or(0.0);
  if (!round) {
    limits.flat_bend = spec.bend_radius.has_value();
    limits.edge_bend = spec.bend_radius_edge.has_value();
  }

  std::vector<routing::FlexNode> nodes;
  for (std::size_t k = 0; k < spec.nodes.size(); ++k) {
    const FlexNode& given = spec.nodes[k];
    routing::FlexNode node;
    node.point = given.point;
    node.twist = given.twist;
    if (given.obstacle >= 0) {
      const std::size_t body = static_cast<std::size_t>(given.obstacle);
      if (body >= obstacles.size() || !obstacles[body]) {
        throw std::invalid_argument("the body of a flex item's node is missing");
      }
      node.body = given.obstacle;
      if (!given.face.empty()) {
        const std::vector<int> found = obstacles[body]->find_faces(given.face);
        if (found.empty()) {
          throw std::invalid_argument(no_face_error(given.face));
        }
        // (A face in pieces: the body's surface nearest to the node.)
        if (found.size() == 1) {
          node.face = obstacles[body]->face(found.front());
        }
      }
    }
    const bool first = k == 0;
    const bool last = k + 1 == spec.nodes.size();
    if (given.direction && (first || last)) {
      const gp_Dir runs = first ? *given.direction : given.direction->Reversed();
      node.direction = runs;
      if (given.x_axis) {
        // x = normal x tangent with the normal against `down`: down is
        // x cross the way the route runs.
        const gp_Vec down = gp_Vec(*given.x_axis).Crossed(gp_Vec(runs));
        if (down.Magnitude() > 1.0e-9) {
          node.down = gp_Dir(down);
        }
      }
    }
    nodes.push_back(node);
  }

  const std::shared_ptr<const routing::FlexRouter> router = router_of(obstacles);
  const Clock::time_point started = Clock::now();
  const routing::FlexRoute routed = router->route(nodes, section, limits, spec.quick);
  if (tracing()) {
    std::fprintf(stderr, "flex-trace: route of %d nodes%s in %.3f s, %.3f mm\n", static_cast<int>(nodes.size()),
                 spec.quick ? " (quick)" : "", seconds_since(started), routed.length);
  }

  FlexRoute out;
  out.curve.push_back(routed.curve);
  for (const routing::FlexSample& sample : routed.samples) {
    FlexSample point;
    point.along = sample.at;
    point.point = sample.point;
    point.tangent = sample.tangent;
    point.normal = sample.down.Reversed();
    point.node = sample.node;
    if (sample.on_surface && sample.body >= 0 && sample.face >= 0) {
      const Shape& body = *obstacles[static_cast<std::size_t>(sample.body)];
      point.obstacle = sample.body;
      if (sample.face < body.face_count() && !body.face_names(sample.face).empty()) {
        point.face = body.face_names(sample.face).front();
      }
    }
    out.samples.push_back(std::move(point));
  }
  for (const routing::FlexProblem& problem : routed.problems) {
    for (const routing::FlexProblem::Place& place : problem.places) {
      FlexProblem item;
      item.kind = kind_of(problem.kind);
      item.from = place.from;
      item.to = place.to;
      item.obstacle = place.body;
      item.points.push_back(place.point);
      out.problems.push_back(std::move(item));
    }
  }
  return out;
}

ShapePtr flex_solid(const FlexSolidSpec& spec) {
  routing::FlexSection section;
  if (spec.round) {
    section.kind = routing::FlexSectionKind::Round;
    section.width = spec.width;
    section.wall = spec.wall.value_or(0.0);
  } else if (spec.rectangular) {
    section.kind = routing::FlexSectionKind::Rectangular;
    section.width = spec.width;
    section.height = spec.height;
    section.corner_radius = spec.corner_radius;
    section.wall = spec.wall.value_or(0.0);
  } else {
    if (spec.outline.size() != 1) {
      throw std::invalid_argument("a flex item's section is one closed outline");
    }
    section.kind = routing::FlexSectionKind::Profile;
    section.profile = spec.outline.front();
    // (The box round an outline is the routing's; the solid needs none.)
    section.width = spec.width > 0.0 ? spec.width : 1.0;
    section.height = spec.height > 0.0 ? spec.height : 1.0;
  }
  std::vector<routing::FlexSample> samples;
  for (const FlexSample& given : spec.samples) {
    routing::FlexSample sample;
    sample.point = given.point;
    sample.tangent = given.tangent;
    sample.down = given.normal.Reversed();
    sample.at = given.along;
    samples.push_back(sample);
  }
  const Clock::time_point started = Clock::now();
  ShapePtr solid;
  try {
    solid = routing::build_solid(spec.feature, spec.path, samples, section);
  } catch (const std::exception& failure) {
    if (tracing()) {
      // (The samples, to make the case again: point, tangent, down, along.)
      std::fprintf(stderr, "flex-trace: solid failed: %s\n", failure.what());
      for (const PathCurve& piece : spec.path) {
        std::fprintf(stderr, "flex-curve: degree %d\n", piece.curve.degree);
        for (const gp_Pnt& pole : piece.curve.poles) {
          std::fprintf(stderr, "flex-pole: %.17g %.17g %.17g\n", pole.X(), pole.Y(), pole.Z());
        }
        for (const double knot : piece.curve.knots) {
          std::fprintf(stderr, "flex-knot: %.17g\n", knot);
        }
      }
      for (const routing::FlexSample& sample : samples) {
        std::fprintf(stderr, "flex-sample: %.17g %.17g %.17g %.17g %.17g %.17g %.17g %.17g %.17g %.17g\n",
                     sample.point.X(), sample.point.Y(), sample.point.Z(), sample.tangent.X(), sample.tangent.Y(),
                     sample.tangent.Z(), sample.down.X(), sample.down.Y(), sample.down.Z(), sample.at);
      }
    }
    throw;
  }
  if (tracing()) {
    std::fprintf(stderr, "flex-trace: solid of %d samples in %.3f s\n", static_cast<int>(samples.size()),
                 seconds_since(started));
  }
  // The view meshes it by its section (io::MeshOptions::section): a round
  // one's diameter; a flat one's thickness, but at least a quarter of its
  // width, so that a ribbon is not cut finer along its route than across.
  if (solid) {
    const double wide = std::max(section.width, section.height);
    const double thin = std::min(section.width, section.height);
    solid->set_display_section(section.round() ? section.width : std::max(thin, wide / 4.0));
  }
  return solid;
}

FlexPort end_port(const Shape& shape, const std::string& cap) {
  const std::vector<int> found = shape.find_faces(cap);
  if (found.empty()) {
    throw std::invalid_argument(no_face_error(cap));
  }
  return detail::run("flex port", [&] {
    const TopoDS_Face& first = shape.face(found.front());
    TopLoc_Location location;
    if (BRep_Tool::Surface(first, location).IsNull() || BRepAdaptor_Surface(first).GetType() != GeomAbs_Plane) {
      throw std::invalid_argument(face_error(cap, "the end face is not planar", "the end " + cap + " is not planar"));
    }
    gp_Dir normal = BRepAdaptor_Surface(first).Plane().Axis().Direction();
    if (first.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    FlexPort port;
    port.direction = normal;
    // The middle of the outer boundary: of its curve's length, for one
    // piece; of the pieces' area for a cap in pieces.
    GProp_GProps props;
    if (found.size() == 1) {
      BRepGProp::LinearProperties(BRepTools::OuterWire(first), props);
    } else {
      std::vector<TopoDS_Shape> pieces;
      for (const int i : found) {
        pieces.push_back(shape.face(i));
      }
      BRepGProp::SurfaceProperties(detail::compound_of(pieces), props);
    }
    port.position = props.CentreOfMass();

    // The section's x: along the longest straight edge, turned so that its
    // largest component is positive; without one the model's x made square
    // to the direction, or its y.
    const gp_Vec out(normal);
    gp_Vec x = gp_Vec(1.0, 0.0, 0.0) - out * out.X();
    if (x.Magnitude() < 1.0e-6) {
      x = gp_Vec(0.0, 1.0, 0.0) - out * out.Y();
    }
    double longest = 0.0;
    for (const int i : found) {
      for (TopExp_Explorer e(shape.face(i), TopAbs_EDGE); e.More(); e.Next()) {
        const BRepAdaptor_Curve edge(TopoDS::Edge(e.Current()));
        if (edge.GetType() != GeomAbs_Line) {
          continue;
        }
        const double length = std::abs(edge.LastParameter() - edge.FirstParameter());
        if (length > longest + 1.0e-9) {
          longest = length;
          gp_Vec along(edge.Line().Direction());
          const double largest = std::max({std::abs(along.X()), std::abs(along.Y()), std::abs(along.Z())});
          const double leading = std::abs(along.X()) == largest   ? along.X()
                                 : std::abs(along.Y()) == largest ? along.Y()
                                                                  : along.Z();
          if (leading < 0.0) {
            along.Reverse();
          }
          x = along - out * along.Dot(out);
        }
      }
    }
    port.x_axis = gp_Dir(x);
    const gp_Vec across = out.Crossed(gp_Vec(port.x_axis));

    // The cap's extent along x and across it, over points of its edges.
    double low_x = 1.0e300;
    double high_x = -1.0e300;
    double low_y = 1.0e300;
    double high_y = -1.0e300;
    for (const int i : found) {
      for (TopExp_Explorer e(shape.face(i), TopAbs_EDGE); e.More(); e.Next()) {
        const BRepAdaptor_Curve edge(TopoDS::Edge(e.Current()));
        constexpr int kSteps = 32;
        for (int k = 0; k <= kSteps; ++k) {
          const double u = edge.FirstParameter() + (edge.LastParameter() - edge.FirstParameter()) * k / kSteps;
          const gp_Vec from(port.position, edge.Value(u));
          low_x = std::min(low_x, from.Dot(gp_Vec(port.x_axis)));
          high_x = std::max(high_x, from.Dot(gp_Vec(port.x_axis)));
          low_y = std::min(low_y, from.Dot(across));
          high_y = std::max(high_y, from.Dot(across));
        }
      }
    }
    port.width = high_x > low_x ? high_x - low_x : 0.0;
    port.height = high_y > low_y ? high_y - low_y : 0.0;
    return port;
  });
}

} // namespace mitcad::geometry
