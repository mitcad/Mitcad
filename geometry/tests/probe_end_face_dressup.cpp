// SPDX-License-Identifier: MIT
// A probe of OCCT's own chamfer and fillet at the end of a blend past a flat
// end face of a rounded body (mitcad#2229, GitHub mitcad/Mitcad#566), for
// the port patch's diagnosis: no ctest, run by hand (the command in the
// ticket). Prints, never fails.
//
// The body: the stepped block (x 0..10; y 0..5 z 0..5 and y 5..10 z 0..10)
// rounded by MitCad with 5 on its sides' and steps' edges and the step's
// inner edge (as GitHub #403's fifth design and #566's designs). Its back's
// bottom edge (y = 10, z = 0) ends at x = 0 and x = 10 on a flat face (the
// rounding's core region, reaching 5 from the edge along the bottom and the
// back and 2.89 along the diagonal) that ends in a rounding tangent to it
// (a torus band). A bevel wider than about 4.09, or a rounding wider than
// 5, has its end section run out of that face.
//
// For each size, OCCT's BRepFilletAPI_MakeChamfer / MakeFillet alone on a
// copy of the body: done or its failure; then the result's validity,
// solids, free edges, volume against the exact shape (the body less its
// common part with the bevel's wedge or the rounding's corner, along x),
// the pieces of the result outside the exact shape and of the exact shape
// missing from the result (volume and box each), its faces on the end
// planes x = 0 and x = 10 and its faces under 0.01 mm2, and the faces the
// edge generated. Before them, the end face: its edges, the faces beyond
// and whether they are tangent, and where the bevel's and the rounding's
// end section (in the plane x = 0) leaves and enters it.
//
// test_probe_end_face_dressup [chamfer|fillet|mitcad] [size...] (the mode
// also as --chamfer, --fillet, --mitcad; mitcad: MitCad's own rounding and
// its path past narrow faces, probe_mitcad)
// test_probe_end_face_dressup write <path.brep> (the body only, for the
// OCCT lab's DRAW case of port patch 2229)

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <optional>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ChFi3d.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "../src/cutouts.hpp"

namespace {

using namespace mitcad::geometry;
using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

std::string box_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  if (box.IsVoid()) {
    return "(empty)";
  }
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  char text[160];
  std::snprintf(text, sizeof text, "x %.4f..%.4f y %.4f..%.4f z %.4f..%.4f", x0, x1, y0, y1, z0, z1);
  return text;
}

const char* surface_kind(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face, false).GetType()) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Cone:
    return "cone";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_Torus:
    return "torus";
  case GeomAbs_BSplineSurface:
    return "bspline";
  default:
    return "other";
  }
}

// A rectangle extruded from z = 0 to `end`, its lines named c<first> to
// c<first + 3> as a sketch names them.
ShapePtr extruded(const std::string& feature, int first, double x, double y, double width, double height,
                  double end) {
  ExtrudeSpec spec;
  spec.feature = feature;
  Region region = rectangle_region(x, y, width, height);
  std::string key = "r{";
  for (int i = 0; i < 4; ++i) {
    const auto line = [first](int k) { return "c" + std::to_string(first + (k + 4) % 4); };
    region.loops[0].segments[static_cast<std::size_t>(i)].name = line(i) + "[" + line(i - 1) + "," + line(i + 1) + "]";
    key += (i == 0 ? "" : ",") + region.loops[0].segments[static_cast<std::size_t>(i)].name;
  }
  region.name = key + "}";
  spec.regions = {std::move(region)};
  spec.start = 0.0;
  spec.end = end;
  return extrude(spec);
}

// The name of the edge whose middle is at p (empty when none is).
std::string edge_at(const Shape& shape, const gp_Pnt& p) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    if (BRep_Tool::Degenerated(shape.edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(shape.edge(i));
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p) < 1e-6) {
      return shape.edge_name(i);
    }
  }
  return {};
}

// The edge of the shape whose middle is at p (null when none is).
TopoDS_Edge occt_edge_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p) < 1e-6) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The stepped block rounded by 5 on its eleven edges (MitCad's fillet).
ShapePtr rounded_block() {
  const ShapePtr low = extruded("F1", 1, 0, 0, 10, 5, 5);
  const ShapePtr high = extruded("F2", 5, 0, 5, 10, 5, 10);
  const ShapePtr block = boolean(BooleanOp::Join, {low.get()}, *high).pieces.at(0).shape;
  FilletSet round;
  for (const gp_Pnt& m : {gp_Pnt(0, 0, 2.5), gp_Pnt(0, 2.5, 5), gp_Pnt(0, 5, 7.5), gp_Pnt(0, 7.5, 10),
                          gp_Pnt(10, 0, 2.5), gp_Pnt(10, 2.5, 5), gp_Pnt(10, 5, 7.5), gp_Pnt(10, 7.5, 10),
                          gp_Pnt(5, 0, 5), gp_Pnt(5, 5, 10), gp_Pnt(5, 5, 5)}) {
    round.edges.push_back(edge_at(*block, m));
    if (round.edges.back().empty()) {
      std::printf("no edge at %g %g %g\n", m.X(), m.Y(), m.Z());
    }
  }
  round.radius = 5;
  return fillet("F3", *block, {round});
}

// The end face at the back's bottom edge's vertex (x, 10, 0): the faces
// there, the end face's edges and the faces beyond with their tangency, and
// where the end sections cross the end face's outline.
void describe_end(const TopoDS_Shape& body, double x) {
  const gp_Pnt at(x, 10, 0);
  Ancestors faces_at;
  Ancestors faces_of_edges;
  TopExp::MapShapesAndUniqueAncestors(body, TopAbs_VERTEX, TopAbs_FACE, faces_at);
  TopExp::MapShapesAndUniqueAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of_edges);
  for (int v = 1; v <= faces_at.Extent(); ++v) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(faces_at.FindKey(v));
    if (BRep_Tool::Pnt(vertex).Distance(at) > 1e-6) {
      continue;
    }
    std::printf("end vertex (%g, 10, 0): %d faces\n", x, faces_at(v).Extent());
    for (const TopoDS_Shape& f : faces_at(v)) {
      const TopoDS_Face& face = TopoDS::Face(f);
      std::printf("  face %s, area %.6f, %s\n", surface_kind(face), area_of(face), box_of(face).c_str());
      const BRepAdaptor_Surface surface(face, false);
      if (surface.GetType() != GeomAbs_Plane || std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1.0) > 1e-9) {
        continue;
      }
      // The end face: its edges, the faces beyond them.
      for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
        const BRepAdaptor_Curve curve(edge);
        std::printf("    edge %s, %s", curve.GetType() == GeomAbs_Line ? "line" : curve.GetType() == GeomAbs_Circle ? "circle" : "curve",
                    box_of(edge).c_str());
        if (faces_of_edges.Contains(edge)) {
          for (const TopoDS_Shape& other : faces_of_edges.FindFromKey(edge)) {
            if (other.IsSame(face)) {
              continue;
            }
            bool smooth = false;
            try {
              OCC_CATCH_SIGNALS
              smooth = ChFi3d::IsTangentFaces(edge, face, TopoDS::Face(other));
            } catch (const Standard_Failure&) {
            }
            std::printf(", beyond: %s%s", surface_kind(TopoDS::Face(other)), smooth ? " (tangent)" : "");
          }
        }
        std::printf("\n");
      }
      // The bevel's end line z = y - (10 - d) and the rounding's end arc,
      // sampled across the section: where they leave the face and enter it.
      for (const double d : {4.0, 4.5, 4.9, 5.0, 5.5, 7.0}) {
        std::printf("    bevel %g: end line", d);
        bool inside = true;
        for (int k = 0; k <= 400; ++k) {
          const double y = 10 - d + d * k / 400.0;
          const gp_Pnt p(x, y, y - (10 - d));
          const bool now = BRepClass_FaceClassifier(face, p, 1e-7).State() != TopAbs_OUT;
          if (now != inside) {
            std::printf(" %s at (y %.4f, z %.4f)", now ? "enters" : "leaves", y, y - (10 - d));
            inside = now;
          }
        }
        std::printf("\n");
      }
      for (const double r : {5.0, 6.0, 7.0, 10.0}) {
        std::printf("    rounding %g: end arc", r);
        bool inside = true;
        for (int k = 0; k <= 400; ++k) {
          const double a = 0.5 * 3.14159265358979323846 * k / 400.0;
          const gp_Pnt p(x, 10 - r + r * std::sin(a), r - r * std::cos(a));
          const bool now = BRepClass_FaceClassifier(face, p, 1e-7).State() != TopAbs_OUT;
          if (now != inside) {
            std::printf(" %s at (y %.4f, z %.4f)", now ? "enters" : "leaves", p.Y(), p.Z());
            inside = now;
          }
        }
        std::printf("\n");
      }
    }
  }
}

// The exact shape: the body less its common part with the bevel's wedge
// (the triangle y - (10 - d) > z) or the rounding's corner (the square of r
// less the circle about y = 10 - r, z = r), along x past the body.
TopoDS_Shape exact(const TopoDS_Shape& body, bool round, double size) {
  TopoDS_Shape tool;
  if (round) {
    const TopoDS_Shape square = BRepPrimAPI_MakeBox(gp_Pnt(-5, 10 - size, 0), gp_Pnt(15, 10, size)).Shape();
    const TopoDS_Shape ball =
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-5, 10 - size, size), gp_Dir(1, 0, 0)), size, 20).Shape();
    tool = BRepAlgoAPI_Cut(square, ball).Shape();
  } else {
    BRepBuilderAPI_MakePolygon triangle(gp_Pnt(-5, 10 - size, 0), gp_Pnt(-5, 10, 0), gp_Pnt(-5, 10, size), true);
    tool = BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(triangle.Wire()).Face(), gp_Vec(20, 0, 0)).Shape();
  }
  return BRepAlgoAPI_Cut(BRepBuilderAPI_Copy(body).Shape(), tool).Shape();
}

// A face's kind, area, centre and its share of a solid's volume (the volume
// between it and the origin, signed by its orientation): a volume that
// differs is found in the faces whose shares differ.
struct FaceShare {
  std::string kind;
  double area = 0.0;
  double share = 0.0;
  gp_Pnt centre;
  std::string box;
  bool paired = false;
};

std::vector<FaceShare> face_shares(const TopoDS_Shape& shape) {
  std::vector<FaceShare> out;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    FaceShare f;
    f.kind = surface_kind(TopoDS::Face(ex.Current()));
    GProp_GProps surface;
    BRepGProp::SurfaceProperties(ex.Current(), surface);
    f.area = surface.Mass();
    f.centre = surface.CentreOfMass();
    GProp_GProps volume;
    BRepGProp::VolumeProperties(ex.Current(), volume, 1e-9, false);
    f.share = volume.Mass();
    f.box = box_of(ex.Current());
    out.push_back(f);
  }
  return out;
}

// The result's faces paired with the exact shape's (same kind, the nearest
// centre within 0.5): pairs whose area or share differ by more than 1e-6,
// and the faces of either left unpaired, with the sums of the shares.
void compare_faces(const TopoDS_Shape& result, const TopoDS_Shape& reference) {
  std::vector<FaceShare> made = face_shares(result);
  std::vector<FaceShare> exact_faces = face_shares(reference);
  double made_sum = 0.0;
  double exact_sum = 0.0;
  for (FaceShare& f : made) {
    made_sum += f.share;
    int best = -1;
    double nearest = 0.5;
    for (int k = 0; k < static_cast<int>(exact_faces.size()); ++k) {
      const FaceShare& e = exact_faces[static_cast<std::size_t>(k)];
      if (!e.paired && e.kind == f.kind && e.centre.Distance(f.centre) < nearest) {
        nearest = e.centre.Distance(f.centre);
        best = k;
      }
    }
    if (best < 0) {
      std::printf("    face of the result alone: %s, area %.9f, share %.9f, %s\n", f.kind.c_str(), f.area, f.share,
                  f.box.c_str());
      continue;
    }
    FaceShare& e = exact_faces[static_cast<std::size_t>(best)];
    e.paired = true;
    f.paired = true;
    if (std::abs(f.area - e.area) > 1e-6 || std::abs(f.share - e.share) > 1e-6) {
      std::printf("    face %s: area %.9f (exact %.9f), share %.9f (exact %.9f), centres %.4f apart, %s\n",
                  f.kind.c_str(), f.area, e.area, f.share, e.share, nearest, f.box.c_str());
    }
  }
  for (const FaceShare& e : exact_faces) {
    exact_sum += e.share;
    if (!e.paired) {
      std::printf("    face of the exact shape alone: %s, area %.9f, share %.9f, %s\n", e.kind.c_str(), e.area,
                  e.share, e.box.c_str());
    }
  }
  std::printf("  faces: %zu (exact %zu), shares %.9f (exact %.9f)\n", made.size(), exact_faces.size(), made_sum,
              exact_sum);
}

void pieces(const char* what, const TopoDS_Shape& shape) {
  double total = 0.0;
  for (TopExp_Explorer ex(shape, TopAbs_SOLID); ex.More(); ex.Next()) {
    const double v = volume_of(ex.Current());
    total += v;
    if (std::abs(v) > 1e-9) {
      std::printf("    %s piece %.9f, %s\n", what, v, box_of(ex.Current()).c_str());
    }
  }
  std::printf("  %s: %.9f in all\n", what, total);
}

void probe(const TopoDS_Shape& body, bool round, double size) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(body).Shape();
  const TopoDS_Edge edge = occt_edge_at(copy, gp_Pnt(5, 10, 0));
  std::printf("\n%s %g\n", round ? "fillet" : "chamfer", size);
  if (edge.IsNull()) {
    std::printf("  no edge at (5, 10, 0)\n");
    return;
  }
  TopoDS_Shape result;
  try {
    OCC_CATCH_SIGNALS
    if (round) {
      BRepFilletAPI_MakeFillet maker(copy);
      maker.Add(size, edge);
      maker.Build();
      std::printf("  done %d, faulty contours %d, faulty vertices %d, has result %d\n", maker.IsDone() ? 1 : 0,
                  maker.NbFaultyContours(), maker.NbFaultyVertices(), maker.HasResult() ? 1 : 0);
      for (int i = 1; i <= maker.NbContours(); ++i) {
        std::printf("  contour %d status %d\n", i, static_cast<int>(maker.StripeStatus(i)));
      }
      if (maker.IsDone()) {
        result = maker.Shape();
        int generated = 0;
        for (const TopoDS_Shape& g : maker.Generated(edge)) {
          if (g.ShapeType() == TopAbs_FACE) {
            ++generated;
            std::printf("  generated face %s, area %.6f, %s\n", surface_kind(TopoDS::Face(g)), area_of(g),
                        box_of(g).c_str());
          }
        }
        std::printf("  %d faces generated\n", generated);
      } else if (maker.HasResult()) {
        std::printf("  partial result: %s\n", box_of(maker.BadShape()).c_str());
      }
    } else {
      BRepFilletAPI_MakeChamfer maker(copy);
      maker.Add(size, edge);
      maker.Build();
      std::printf("  done %d\n", maker.IsDone() ? 1 : 0);
      if (maker.IsDone()) {
        result = maker.Shape();
        int generated = 0;
        for (const TopoDS_Shape& g : maker.Generated(edge)) {
          if (g.ShapeType() == TopAbs_FACE) {
            ++generated;
            std::printf("  generated face %s, area %.6f, %s\n", surface_kind(TopoDS::Face(g)), area_of(g),
                        box_of(g).c_str());
          }
        }
        std::printf("  %d faces generated\n", generated);
      }
    }
  } catch (const Standard_Failure& failure) {
    std::printf("  OCCT throws: %s\n", failure.what());
    return;
  }
  if (result.IsNull()) {
    return;
  }
  int solids = 0;
  for (TopExp_Explorer ex(result, TopAbs_SOLID); ex.More(); ex.Next()) {
    ++solids;
  }
  Ancestors faces_of_edges;
  TopExp::MapShapesAndUniqueAncestors(result, TopAbs_EDGE, TopAbs_FACE, faces_of_edges);
  int free_edges = 0;
  for (int i = 1; i <= faces_of_edges.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces_of_edges.FindKey(i))) && faces_of_edges(i).Extent() != 2) {
      ++free_edges;
      std::printf("    edge in %d faces: %s\n", faces_of_edges(i).Extent(), box_of(faces_of_edges.FindKey(i)).c_str());
    }
  }
  // (Where the result is invalid, what the checker finds wrong and where.)
  const BRepCheck_Analyzer analyzer(result);
  if (!analyzer.IsValid()) {
    for (const TopAbs_ShapeEnum kind : {TopAbs_FACE, TopAbs_WIRE, TopAbs_EDGE, TopAbs_VERTEX}) {
      for (TopExp_Explorer ex(result, kind); ex.More(); ex.Next()) {
        const occ::handle<BRepCheck_Result>& check = analyzer.Result(ex.Current());
        if (check.IsNull()) {
          continue;
        }
        bool bad = false;
        for (const BRepCheck_Status status : check->Status()) {
          if (status != BRepCheck_NoError) {
            bad = true;
            std::printf("    check: %s %s: status %d\n", kind == TopAbs_FACE ? "face" : kind == TopAbs_WIRE ? "wire" : kind == TopAbs_EDGE ? "edge" : "vertex",
                        box_of(ex.Current()).c_str(), static_cast<int>(status));
          }
        }
        // (A bad face's wires edge by edge, in order, with the face across
        // each edge.)
        if (bad && kind == TopAbs_FACE) {
          const TopoDS_Face& face = TopoDS::Face(ex.Current());
          for (TopExp_Explorer wx(face, TopAbs_WIRE); wx.More(); wx.Next()) {
            std::printf("      wire\n");
            for (BRepTools_WireExplorer ew(TopoDS::Wire(wx.Current()), face); ew.More(); ew.Next()) {
              const TopoDS_Edge& wire_edge = ew.Current();
              const BRepAdaptor_Curve curve(wire_edge);
              const gp_Pnt a = curve.Value(curve.FirstParameter());
              const gp_Pnt b = curve.Value(curve.LastParameter());
              std::printf("        edge %s %s from (%.4f, %.4f, %.4f) to (%.4f, %.4f, %.4f)",
                          curve.GetType() == GeomAbs_Line ? "line" : curve.GetType() == GeomAbs_Circle ? "circle" : curve.GetType() == GeomAbs_Ellipse ? "ellipse" : "curve",
                          wire_edge.Orientation() == TopAbs_REVERSED ? "reversed" : "forward", a.X(), a.Y(), a.Z(), b.X(), b.Y(),
                          b.Z());
              if (faces_of_edges.Contains(wire_edge)) {
                for (const TopoDS_Shape& other : faces_of_edges.FindFromKey(wire_edge)) {
                  if (!other.IsSame(face)) {
                    std::printf(", across: %s %s", surface_kind(TopoDS::Face(other)), box_of(other).c_str());
                  }
                }
              }
              // (The wire_edge's checks on this face: its curve on the face.)
              const occ::handle<BRepCheck_Result>& on = analyzer.Result(wire_edge);
              if (!on.IsNull() && on->IsStatusOnShape(face)) {
                for (const BRepCheck_Status status : on->StatusOnShape(face)) {
                  if (status != BRepCheck_NoError) {
                    std::printf(", on the face: status %d", static_cast<int>(status));
                  }
                }
              }
              const BRepAdaptor_Curve on_face(wire_edge, face);
              const double middle = 0.5 * (on_face.FirstParameter() + on_face.LastParameter());
              std::printf(", its curve on the face at the middle (%.4f, %.4f, %.4f), the edge's (%.4f, %.4f, %.4f)",
                          on_face.Value(middle).X(), on_face.Value(middle).Y(), on_face.Value(middle).Z(),
                          curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).X(),
                          curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Y(),
                          curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Z());
              std::printf("\n");
            }
          }
        }
      }
    }
  }
  const TopoDS_Shape reference = exact(body, round, size);
  std::printf("  valid %d, solids %d, edges not in two faces %d, volume %.9f, exact %.9f, body %.9f\n",
              BRepCheck_Analyzer(result).IsValid() ? 1 : 0, solids, free_edges, volume_of(result),
              volume_of(reference), volume_of(body));
  try {
    OCC_CATCH_SIGNALS
    pieces("result outside the exact shape", BRepAlgoAPI_Cut(result, reference).Shape());
    pieces("exact shape missing from the result", BRepAlgoAPI_Cut(reference, result).Shape());
  } catch (const Standard_Failure& failure) {
    std::printf("  comparison throws: %s\n", failure.what());
  }
  for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    const double area = area_of(face);
    const BRepAdaptor_Surface surface(face, false);
    const bool end_plane = surface.GetType() == GeomAbs_Plane &&
                           std::abs(std::abs(surface.Plane().Axis().Direction().X()) - 1.0) < 1e-9;
    if (end_plane || area < 0.01) {
      std::printf("  face %s%s, area %.6f, %s\n", surface_kind(face), end_plane ? " on an end plane" : " (small)",
                  area, box_of(face).c_str());
    }
  }
  compare_faces(result, reference);
}

// MitCad's rounding of the back's bottom edge by r, and the steps of its
// path past narrow faces (dressup.cpp's past_narrow_faces, cutouts.hpp):
// whether the faces beside the edge count as narrow (overflow_failure, at
// the contact r and at the exactly-wide contact r (1 + 1e-3)), whether the
// space past them is found (fill_overflow, without and with the walls),
// and whether OCCT's fillet builds on that filled body.
void probe_mitcad(const Shape& body, double r) {
  std::printf("\nmitcad fillet %g\n", r);
  int back = -1;
  for (int e = 0; e < body.edge_count() && back < 0; ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(gp_Pnt(5, 10, 0)) < 1e-6) {
      back = e;
    }
  }
  if (back < 0) {
    std::printf("  no edge at (5, 10, 0)\n");
    return;
  }
  try {
    const ShapePtr rounded = fillet("F4", body, {body.edge_name(back)}, r);
    std::printf("  MitCad: valid %d, volume %.9f, exact %.9f, %zu notes, %zu infos\n",
                BRepCheck_Analyzer(rounded->occt()).IsValid() ? 1 : 0, volume_of(rounded->occt()),
                volume_of(exact(body.occt(), true, r)), rounded->notes().size(), rounded->infos().size());
    for (const std::string& note : rounded->notes()) {
      std::printf("    note: %s\n", note.c_str());
    }
    for (const std::string& info : rounded->infos()) {
      std::printf("    info: %s\n", info.c_str());
    }
  } catch (const std::exception& error) {
    std::printf("  MitCad: %s\n", error.what());
  }
  std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
  owner[static_cast<std::size_t>(back)] = 0;
  for (const double contact : {r, r * (1.0 + 1.0e-3)}) {
    try {
      const std::string where = mitcad::geometry::detail::overflow_failure(body, owner, contact, "fillet");
      std::printf("  narrow at contact %.6f: %s\n", contact, where.empty() ? "(none)" : where.c_str());
    } catch (const std::exception& error) {
      std::printf("  narrow at contact %.6f: throws %s\n", contact, error.what());
    }
  }
  for (const bool walls : {false, true}) {
    try {
      const std::optional<mitcad::geometry::detail::Cutouts> cut =
          mitcad::geometry::detail::fill_overflow(body, owner, r * (1.0 + 1.0e-3), 1.0, walls);
      if (!cut) {
        std::printf("  filled past the narrow faces (walls %d): none found\n", walls ? 1 : 0);
        continue;
      }
      std::printf("  filled past the narrow faces (walls %d): body volume %.6f, %s, %zu walls\n", walls ? 1 : 0,
                  volume_of(cut->body->occt()), box_of(cut->body->occt()).c_str(), cut->rising.size());
      std::vector<TopoDS_Edge> edges;
      for (int e = 0; e < cut->body->edge_count() && e < static_cast<int>(cut->owner.size()); ++e) {
        if (cut->owner[static_cast<std::size_t>(e)] >= 0) {
          edges.push_back(TopoDS::Edge(cut->body->edge(e)));
        }
      }
      const TopoDS_Shape copy = BRepBuilderAPI_Copy(cut->body->occt()).Shape();
      BRepFilletAPI_MakeFillet maker(copy);
      for (const TopoDS_Edge& edge : edges) {
        TopExp_Explorer ex(copy, TopAbs_EDGE);
        for (; ex.More(); ex.Next()) {
          const BRepAdaptor_Curve a(TopoDS::Edge(ex.Current()));
          const BRepAdaptor_Curve b(edge);
          if (a.Value(0.5 * (a.FirstParameter() + a.LastParameter()))
                  .Distance(b.Value(0.5 * (b.FirstParameter() + b.LastParameter()))) < 1e-6) {
            maker.Add(r, TopoDS::Edge(ex.Current()));
            break;
          }
        }
      }
      try {
        OCC_CATCH_SIGNALS
        maker.Build();
        std::printf("    OCCT's fillet on it (%zu edges): done %d", edges.size(), maker.IsDone() ? 1 : 0);
        if (maker.NbContours() > 0) {
          std::printf(", contour status %d", static_cast<int>(maker.StripeStatus(1)));
        }
        if (maker.IsDone()) {
          std::printf(", valid %d, volume %.6f", BRepCheck_Analyzer(maker.Shape()).IsValid() ? 1 : 0,
                      volume_of(maker.Shape()));
        }
        std::printf("\n");
      } catch (const Standard_Failure& failure) {
        std::printf("    OCCT's fillet on it throws: %s\n", failure.what());
      }
    } catch (const std::exception& error) {
      std::printf("  filled past the narrow faces (walls %d): throws %s\n", walls ? 1 : 0, error.what());
    }
  }
}

} // namespace

int main(int argc, char** argv) {
  const ShapePtr body = rounded_block();
  std::printf("rounded block: volume %.9f, %d faces, valid %d\n", volume_of(body->occt()), body->face_count(),
              BRepCheck_Analyzer(body->occt()).IsValid() ? 1 : 0);
  describe_end(body->occt(), 0.0);
  describe_end(body->occt(), 10.0);
  std::string which = argc > 1 ? argv[1] : "";
  // (Also as an option: --chamfer, --fillet, --mitcad.)
  while (!which.empty() && which.front() == '-') {
    which.erase(0, 1);
  }
  std::vector<double> sizes;
  for (int i = 2; i < argc; ++i) {
    sizes.push_back(std::atof(argv[i]));
  }
  if (which.empty() || which == "chamfer") {
    for (const double d : sizes.empty() ? std::vector<double>{4.0, 4.5, 4.9, 5.0, 5.5, 7.0} : sizes) {
      probe(body->occt(), false, d);
    }
  }
  if (which.empty() || which == "fillet") {
    for (const double r : sizes.empty() ? std::vector<double>{5.0, 6.0, 7.0, 9.99, 10.0} : sizes) {
      probe(body->occt(), true, r);
    }
  }
  if (which == "mitcad") {
    for (const double r : sizes.empty() ? std::vector<double>{10.0} : sizes) {
      probe_mitcad(*body, r);
    }
  }
  // The body only, written to the given path (the OCCT lab's DRAW case of
  // the port patch reads it: OCCT's own fillet does not build it).
  if (which == "write" && argc > 2) {
    const bool written = BRepTools::Write(body->occt(), argv[2]);
    std::printf("rounded block written to %s: %d\n", argv[2], written ? 1 : 0);
  }
  return 0;
}
