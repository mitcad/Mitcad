// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sheet_face_grid.hpp"

#include <array>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// The most points a grid takes.
constexpr std::size_t kMostPoints = 4000000;

// The unit normal of the surface at (u, v), the face's way round; where
// the surface has none, the normal a little way from (u, v) towards
// (mid_u, mid_v): further each time. A null vector where none is found.
gp_Vec normal_at(const BRepAdaptor_Surface& surface, bool reversed, double u, double v, double mid_u,
                 double mid_v) {
  for (const double step : std::array<double, 5>{0.0, 1.0e-6, 1.0e-4, 1.0e-3, 1.0e-2}) {
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(u + (mid_u - u) * step, v + (mid_v - v) * step, p, du, dv);
    gp_Vec n = du.Crossed(dv);
    const double size = n.Magnitude();
    if (size > 0 && size > 1.0e-12 * du.Magnitude() * dv.Magnitude()) {
      n.Divide(size);
      if (reversed) n.Reverse();
      return n;
    }
  }
  return gp_Vec(0, 0, 0);
}

// The face's parameter bounds and what the samples share.
struct Frame {
  double u0 = 0;
  double u1 = 0;
  double v0 = 0;
  double v1 = 0;
  bool reversed = false;
  std::size_t columns = 1;
  std::size_t rows = 1;

  double mid_u() const { return (u0 + u1) / 2; }
  double mid_v() const { return (v0 + v1) / 2; }
};

// The place of (u, v) in the grid: x = i at column i, y = j at row j.
std::array<double, 2> cell_of(const Frame& frame, double u, double v) {
  const double du = frame.u1 - frame.u0;
  const double dv = frame.v1 - frame.v0;
  return {du > 0 ? (u - frame.u0) / du * static_cast<double>(frame.columns) : 0.0,
          dv > 0 ? (v - frame.v0) / dv * static_cast<double>(frame.rows) : 0.0};
}

// The face's wires (mitcad#2558): each edge at `steps` equal steps of its
// parameter on the face, in the direction its wire runs, edges of no
// length left out.
std::vector<FaceGridWire> wires_of(const TopoDS_Face& face, const BRepAdaptor_Surface& surface, const Frame& frame,
                                   std::size_t steps) {
  std::vector<FaceGridWire> wires;
  const TopoDS_Wire outer = BRepTools::OuterWire(face);
  std::size_t total = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    const TopoDS_Wire& wire = TopoDS::Wire(w.Current());
    FaceGridWire sampled;
    sampled.outer = !outer.IsNull() && wire.IsSame(outer);
    for (BRepTools_WireExplorer e(wire, face); e.More(); e.Next()) {
      throw_if_cancelled();
      const TopoDS_Edge& edge = e.Current();
      if (BRep_Tool::Degenerated(edge)) continue;
      total += steps + 1;
      if (total > kMostPoints) throw std::invalid_argument("a face's boundary of too many points");
      const BRepAdaptor_Curve2d curve(edge, face);
      double t0 = curve.FirstParameter();
      double t1 = curve.LastParameter();
      if (edge.Orientation() == TopAbs_REVERSED) std::swap(t0, t1);
      FaceGridEdge samples;
      samples.cells.reserve(steps + 1);
      samples.points.reserve(steps + 1);
      samples.normals.reserve(steps + 1);
      for (std::size_t k = 0; k <= steps; ++k) {
        const double t = t0 + (t1 - t0) * static_cast<double>(k) / static_cast<double>(steps);
        const gp_Pnt2d uv = curve.Value(t);
        samples.cells.push_back(cell_of(frame, uv.X(), uv.Y()));
        samples.points.push_back(surface.Value(uv.X(), uv.Y()));
        samples.normals.push_back(normal_at(surface, frame.reversed, uv.X(), uv.Y(), frame.mid_u(), frame.mid_v()));
      }
      sampled.edges.push_back(std::move(samples));
    }
    if (!sampled.edges.empty()) wires.push_back(std::move(sampled));
  }
  return wires;
}

// One face sampled.
FaceGrid grid_of(const TopoDS_Face& face, std::size_t columns, std::size_t rows, std::size_t edge_steps) {
  const BRepAdaptor_Surface surface(face);
  Frame frame;
  BRepTools::UVBounds(face, frame.u0, frame.u1, frame.v0, frame.v1);
  frame.reversed = face.Orientation() == TopAbs_REVERSED;
  frame.columns = columns;
  frame.rows = rows;
  const double u0 = frame.u0;
  const double u1 = frame.u1;
  const double v0 = frame.v0;
  const double v1 = frame.v1;
  const bool reversed = frame.reversed;
  const double mid_u = frame.mid_u();
  const double mid_v = frame.mid_v();
  FaceGrid grid;
  grid.points.reserve((columns + 1) * (rows + 1));
  grid.normals.reserve((columns + 1) * (rows + 1));
  for (std::size_t j = 0; j <= rows; ++j) {
    throw_if_cancelled();
    const double v = v0 + (v1 - v0) * static_cast<double>(j) / static_cast<double>(rows);
    for (std::size_t i = 0; i <= columns; ++i) {
      const double u = u0 + (u1 - u0) * static_cast<double>(i) / static_cast<double>(columns);
      grid.points.push_back(surface.Value(u, v));
      grid.normals.push_back(normal_at(surface, reversed, u, v, mid_u, mid_v));
    }
  }
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  grid.area = props.Mass();
  if (edge_steps > 0) grid.wires = wires_of(face, surface, frame, edge_steps);
  return grid;
}

} // namespace

std::vector<FaceGrid> sheet_face_grid(const Shape& shape, const std::string& name, std::size_t columns,
                                      std::size_t rows, std::size_t edge_steps) {
  return detail::run("sheet face grid", [&] {
    const std::vector<int> found = shape.find_faces(name);
    if (found.empty()) throw std::invalid_argument(no_face_error(name));
    if (columns == 0 || rows == 0) throw std::invalid_argument("a face's grid needs a column and a row");
    if ((columns + 1) * (rows + 1) > kMostPoints) throw std::invalid_argument("a face's grid of too many points");
    std::vector<FaceGrid> grids;
    for (const int f : found) grids.push_back(grid_of(shape.face(f), columns, rows, edge_steps));
    return grids;
  });
}

} // namespace mitcad::geometry
