// SPDX-License-Identifier: MIT
#pragma once

// Regression tests of the OCCT port's patches (third_party/vcpkg-ports,
// mitcad#1986): one file per patch, p<number>-<topic>.cpp, each registering
// its tests under the patch's number with MITCAD_PORT_TEST. The executable
// test_port_patches runs the tests of one patch with `--part <number>`
// (ctest geometry.port.<number>) or all of them without arguments. Each
// test builds a small synthetic body with OCCT alone and fails for the
// patch's reason when the port is built without that patch.

#include <cstdio>
#include <functional>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "../check.hpp"

namespace port {

struct Registered {
  const char* part;
  const char* name;
  void (*run)();
};

inline std::vector<Registered>& registry() {
  static std::vector<Registered> tests;
  return tests;
}

struct Registrar {
  Registrar(const char* part, const char* name, void (*run)()) { registry().push_back({part, name, run}); }
};

// The volume, integrated to a relative error of 1e-9.
inline double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The area of a shape's faces.
inline double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

inline bool valid(const TopoDS_Shape& shape) {
  return !shape.IsNull() && BRepCheck_Analyzer(shape).IsValid();
}

inline int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum kind) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, kind, all);
  return all.Extent();
}

// The largest tolerance of the shape's edges and vertices.
inline double max_tolerance(const TopoDS_Shape& shape) {
  double tol = 0.0;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    tol = std::max(tol, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    tol = std::max(tol, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
  }
  return tol;
}

// The edges of a shape whose middle point passes the test.
inline std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape,
                                            const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                      edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge&            edge  = TopoDS::Edge(all(i));
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && keep(curve->Value(0.5 * (first + last)))) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// Builds a fillet or chamfer; a null shape when it fails or throws.
template <typename Maker>
TopoDS_Shape built(Maker& maker, const char* what) {
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: %s\n", what, failure.what());
    return TopoDS_Shape();
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s: not done\n", what);
    return TopoDS_Shape();
  }
  return maker.Shape();
}

// The edges rounded with radius r; a null shape when it fails.
inline TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(r, edge);
  }
  return built(fillet, "fillet");
}

// The edges chamfered with an equal distance d; a null shape when it fails.
inline TopoDS_Shape chamfered(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double d) {
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Edge& edge : edges) {
    chamfer.Add(d, edge);
  }
  return built(chamfer, "chamfer");
}

} // namespace port

#define MITCAD_PORT_TEST_JOIN2(a, b) a##b
#define MITCAD_PORT_TEST_JOIN(a, b) MITCAD_PORT_TEST_JOIN2(a, b)
// Registers `function` (void()) as a test of the port patch `part` ("0110").
#define MITCAD_PORT_TEST(part, function)                                                           \
  static const ::port::Registrar MITCAD_PORT_TEST_JOIN(port_registrar_, __LINE__)(part, #function, function)
