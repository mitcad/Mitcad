// SPDX-License-Identifier: MIT
#include "bridge/min_radius.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad_bridge/kernel/min_radius.h"

namespace mitcad::bridge {
namespace {

// Points across each parameter of the face, its bounds included (where a
// cone's or a variable rounding's curvature is largest).
constexpr int kGrid = 16;
// Rounds of a finer grid about the largest found, each half the step.
constexpr int kRefinements = 5;
// Below this curvature (1/mm, a radius of 1000 km) a face does not bend.
constexpr double kFlat = 1e-9;

// The largest curvature found so far and where, in the face's parameters.
struct Largest {
  double curvature = -1.0; // none yet
  double u = 0.0;
  double v = 0.0;
};

// One face's surface, its parameter bounds and its inside.
class FaceSampler {
public:
  explicit FaceSampler(const TopoDS_Face& face)
      : m_surface(face), m_props(m_surface, 2, 1.0e-9), m_inside(face, BRep_Tool::Tolerance(face)) {
    BRepTools::UVBounds(face, m_u0, m_u1, m_v0, m_v1);
  }

  GeomAbs_SurfaceType type() const { return m_surface.GetType(); }
  bool bounded() const {
    return std::isfinite(m_u0) && std::isfinite(m_u1) && std::isfinite(m_v0) && std::isfinite(m_v1);
  }

  // The parameters clamped to the face's bounds.
  double u(double value) const { return std::clamp(value, m_u0, m_u1); }
  double v(double value) const { return std::clamp(value, m_v0, m_v1); }
  double uSpan() const { return m_u1 - m_u0; }
  double vSpan() const { return m_v1 - m_v0; }
  double u0() const { return m_u0; }
  double v0() const { return m_v0; }

  // Whether the point of the parameters lies on the face (inside its
  // loops, or on them).
  bool inside(double u, double v) const {
    const TopAbs_State state = m_inside.Perform(gp_Pnt2d(u, v));
    return state == TopAbs_IN || state == TopAbs_ON;
  }

  // The largest magnitude of the principal curvatures there; negative
  // where it is not defined (a cone's apex, a sphere's pole).
  double curvature(double u, double v) {
    m_props.SetParameters(u, v);
    if (!m_props.IsCurvatureDefined()) {
      return -1.0;
    }
    const double k = std::max(std::abs(m_props.MaxCurvature()), std::abs(m_props.MinCurvature()));
    return std::isfinite(k) ? k : -1.0;
  }

  // Takes the point when it lies on the face and bends more.
  void sample(double u, double v, Largest& largest) {
    if (!inside(u, v)) {
      return;
    }
    const double k = curvature(u, v);
    if (k > largest.curvature) {
      largest = Largest{k, u, v};
    }
  }

  gp_Pnt point(double u, double v) const { return m_surface.Value(u, v); }

private:
  BRepAdaptor_Surface m_surface;
  BRepLProp_SLProps m_props;
  BRepTopAdaptor_FClass2d m_inside;
  double m_u0 = 0.0;
  double m_u1 = 0.0;
  double m_v0 = 0.0;
  double m_v1 = 0.0;
};

// A point of a face without a surface (a mesh body's facet): its first
// vertex.
gp_Pnt firstVertex(const TopoDS_Face& face) {
  for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
    return BRep_Tool::Pnt(TopoDS::Vertex(it.Current()));
  }
  return gp_Pnt(0.0, 0.0, 0.0);
}

MinRadiusFace measure(const TopoDS_Face& face, std::size_t index) {
  MinRadiusFace result{};
  result.face = index;
  result.curvature = 0.0;
  const auto at = [&result](const gp_Pnt& p) {
    result.at[0] = p.X();
    result.at[1] = p.Y();
    result.at[2] = p.Z();
  };
  if (BRep_Tool::Surface(face).IsNull()) {
    at(firstVertex(face));
    return result;
  }
  FaceSampler sampler(face);
  if (!sampler.bounded()) {
    throw std::invalid_argument("a face without bounds has no curvature to measure");
  }
  const double uMiddle = sampler.u0() + sampler.uSpan() / 2.0;
  const double vMiddle = sampler.v0() + sampler.vSpan() / 2.0;
  if (sampler.type() == GeomAbs_Plane) {
    at(sampler.point(uMiddle, vMiddle));
    return result;
  }
  // A grid over the face's bounds, its points on the face only.
  Largest largest;
  for (int i = 0; i < kGrid; ++i) {
    for (int j = 0; j < kGrid; ++j) {
      sampler.sample(sampler.u0() + sampler.uSpan() * i / (kGrid - 1), sampler.v0() + sampler.vSpan() * j / (kGrid - 1),
                     largest);
    }
  }
  if (largest.curvature < 0.0) {
    // A face too narrow for the grid: its middle, inside or not.
    largest = Largest{std::max(0.0, sampler.curvature(uMiddle, vMiddle)), uMiddle, vMiddle};
  }
  // Finer grids about the largest, unless the curvature is the same
  // everywhere (a cylinder, a sphere).
  const GeomAbs_SurfaceType type = sampler.type();
  if (type != GeomAbs_Cylinder && type != GeomAbs_Sphere) {
    double du = sampler.uSpan() / (kGrid - 1);
    double dv = sampler.vSpan() / (kGrid - 1);
    for (int round = 0; round < kRefinements; ++round) {
      du /= 2.0;
      dv /= 2.0;
      const Largest centre = largest;
      for (int i = -2; i <= 2; ++i) {
        for (int j = -2; j <= 2; ++j) {
          if (i != 0 || j != 0) {
            sampler.sample(sampler.u(centre.u + i * du), sampler.v(centre.v + j * dv), largest);
          }
        }
      }
    }
  }
  result.curvature = largest.curvature < kFlat ? 0.0 : largest.curvature;
  at(sampler.point(largest.u, largest.v));
  return result;
}

} // namespace

rust::Vec<MinRadiusFace> face_curvatures(const geometry::Shape& shape, rust::Slice<const rust::String> faces) {
  const geometry::CheckedOperation check("the curvature of faces");
  std::vector<int> indices;
  if (faces.empty()) {
    for (int f = 0; f < shape.face_count(); ++f) {
      indices.push_back(f);
    }
  } else {
    std::vector<char> taken(static_cast<std::size_t>(shape.face_count()), 0);
    for (const rust::String& name : faces) {
      const std::vector<int> found = shape.find_faces(std::string(name));
      if (found.empty()) {
        throw std::invalid_argument("the body has no face " + std::string(name));
      }
      for (const int f : found) {
        if (taken[static_cast<std::size_t>(f)] == 0) {
          taken[static_cast<std::size_t>(f)] = 1;
          indices.push_back(f);
        }
      }
    }
  }
  rust::Vec<MinRadiusFace> result;
  result.reserve(indices.size());
  for (const int f : indices) {
    geometry::throw_if_cancelled();
    result.push_back(measure(shape.face(f), static_cast<std::size_t>(f)));
  }
  return result;
}

} // namespace mitcad::bridge
