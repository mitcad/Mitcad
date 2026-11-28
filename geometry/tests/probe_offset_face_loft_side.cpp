// SPDX-License-Identifier: MIT
// A probe, no ctest (mitcad#2546, GitHub mitcad/Mitcad#723): Offset Face on
// the outer wall of a hollow loft, read from a body written by
// `mitcad-cli export <design> <body.brep> --bodies <uid>`. Prints, never
// fails.
//
// test_geometry_loft_shell probe_offset_face_loft_side <body.brep> [distance] [out folder]
//
// For the body: every face (index, surface with its B-spline form: degrees,
// periodic or not, poles, knots and their multiplicities, continuity; an
// offset surface's distance and basis), and every edge of the faces that
// are not planes (its curve, its curves on the face, closed on the face or
// not, range, tolerance, ends). Then for each face that is not a plane: the
// geometry kernel's offset of the body with that face moved by `distance`
// (as Offset Face moves whole faces), done or its error, and the result's
// solids, shells, free edges and validity; the same with the other faces
// that are not planes left out (an open shell, whose offset keeps its gaps
// and lists their free edges); and offset_faces on it with Tangent Chain.
// With an out folder the kernel's inputs and results are written there as
// BREP files.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_ListOfStatus.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepGProp.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom2d_Curve.hxx>
#include <Message_ProgressRange.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace {

using mitcad::geometry::Shape;

std::string out_folder;
int written = 0;

void write(const std::string& what, const TopoDS_Shape& shape) {
  if (out_folder.empty() || shape.IsNull()) {
    return;
  }
  const std::string path = out_folder + "/" + std::to_string(++written) + "-" + what + ".brep";
  std::printf("  written %s\n", path.c_str());
  BRepTools::Write(shape, path.c_str());
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

const char* continuity(GeomAbs_Shape shape) {
  switch (shape) {
  case GeomAbs_C0:
    return "C0";
  case GeomAbs_G1:
    return "G1";
  case GeomAbs_C1:
    return "C1";
  case GeomAbs_G2:
    return "G2";
  case GeomAbs_C2:
    return "C2";
  case GeomAbs_C3:
    return "C3";
  default:
    return "CN";
  }
}

void print_bspline(const occ::handle<Geom_BSplineSurface>& s) {
  std::printf("    bspline degrees %d x %d, poles %d x %d, %s%s%s, %s\n", s->UDegree(), s->VDegree(), s->NbUPoles(),
              s->NbVPoles(), s->IsUPeriodic() ? "u periodic" : "u not periodic",
              s->IsVPeriodic() ? ", v periodic" : ", v not periodic", s->IsURational() || s->IsVRational()
                                                                          ? ", rational"
                                                                          : "",
              continuity(s->Continuity()));
  std::printf("    u knots");
  for (int i = 1; i <= s->NbUKnots(); ++i) {
    std::printf(" %.9g/%d", s->UKnot(i), s->UMultiplicity(i));
  }
  std::printf("\n    v knots");
  for (int i = 1; i <= s->NbVKnots(); ++i) {
    std::printf(" %.9g/%d", s->VKnot(i), s->VMultiplicity(i));
  }
  std::printf("\n");
}

void print_surface(const occ::handle<Geom_Surface>& surface, int depth = 0) {
  if (surface.IsNull() || depth > 3) {
    return;
  }
  std::printf("    surface %s\n", surface->DynamicType()->Name());
  if (const auto s = occ::down_cast<Geom_BSplineSurface>(surface)) {
    print_bspline(s);
  } else if (const auto o = occ::down_cast<Geom_OffsetSurface>(surface)) {
    std::printf("    offset by %.9g of\n", o->Offset());
    print_surface(o->BasisSurface(), depth + 1);
  } else if (const auto t = occ::down_cast<Geom_RectangularTrimmedSurface>(surface)) {
    print_surface(t->BasisSurface(), depth + 1);
  }
}

void print_edge(const TopoDS_Face& face, const TopoDS_Edge& edge) {
  if (BRep_Tool::Degenerated(edge)) {
    std::printf("    edge degenerated\n");
    return;
  }
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  const bool closed = BRep_Tool::IsClosed(edge, face);
  std::printf("    edge%s %s [%.9g, %.9g] tolerance %.3g", closed ? " (closed on the face)" : "",
              curve.IsNull() ? "no 3D curve" : curve->DynamicType()->Name(), first, last,
              BRep_Tool::Tolerance(edge));
  if (const auto b = occ::down_cast<Geom_BSplineCurve>(curve)) {
    std::printf(" degree %d poles %d", b->Degree(), b->NbPoles());
  }
  if (!curve.IsNull()) {
    const gp_Pnt a = curve->Value(first);
    const gp_Pnt z = curve->Value(last);
    std::printf(" (%.6f %.6f %.6f)-(%.6f %.6f %.6f)", a.X(), a.Y(), a.Z(), z.X(), z.Y(), z.Z());
  }
  std::printf("\n");
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  for (const TopAbs_Orientation o : {TopAbs_FORWARD, TopAbs_REVERSED}) {
    double f2 = 0.0;
    double l2 = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(o)), forward, f2, l2);
    if (pcurve.IsNull()) {
      continue;
    }
    const gp_Pnt2d a = pcurve->Value(f2);
    const gp_Pnt2d z = pcurve->Value(l2);
    std::printf("      on the face (%s): %s [%.9g, %.9g] (%.9g %.9g)-(%.9g %.9g)\n",
                o == TopAbs_FORWARD ? "forward" : "reversed", pcurve->DynamicType()->Name(), f2, l2, a.X(), a.Y(),
                z.X(), z.Y());
    if (!closed) {
      break;
    }
  }
}

void print_face(int i, const TopoDS_Face& face, bool edges) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(face, box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  if (!box.IsVoid()) {
    box.Get(x0, y0, z0, x1, y1, z1);
  }
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  std::printf("face %d%s area %.6f box [%.4f %.4f %.4f]-[%.4f %.4f %.4f] uv [%.9g %.9g]x[%.9g %.9g] tolerance %.3g\n",
              i, face.Orientation() == TopAbs_REVERSED ? " (reversed)" : "", area_of(face), x0, y0, z0, x1, y1, z1,
              u0, u1, v0, v1, BRep_Tool::Tolerance(face));
  TopLoc_Location location;
  print_surface(BRep_Tool::Surface(face, location));
  if (!edges) {
    return;
  }
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    print_edge(face, TopoDS::Edge(it.Current()));
  }
}

void describe(const char* what, const TopoDS_Shape& shape) {
  int solids = 0, shells = 0, faces = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  for (TopExp_Explorer it(shape, TopAbs_SHELL); it.More(); it.Next()) {
    ++shells;
  }
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++faces;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  int free_edges = 0;
  for (int i = 1; i <= map.Extent(); ++i) {
    if (map(i).Extent() == 1 && !BRep_Tool::Degenerated(TopoDS::Edge(map.FindKey(i)))) {
      ++free_edges;
      if (free_edges <= 40) {
        const BRepAdaptor_Curve curve(TopoDS::Edge(map.FindKey(i)));
        const gp_Pnt a = curve.Value(curve.FirstParameter());
        const gp_Pnt b = curve.Value(curve.LastParameter());
        std::printf("  free edge (%.4f %.4f %.4f)-(%.4f %.4f %.4f)\n", a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z());
      }
    }
  }
  const BRepCheck_Analyzer check(shape);
  std::printf("%s: %d solids, %d shells, %d faces, %d free edges, %s, volume %.6f\n", what, solids, shells, faces,
              free_edges, check.IsValid() ? "valid" : "INVALID", volume_of(shape));
  if (check.IsValid()) {
    return;
  }
  // The sub-shapes the check refuses, with their first status.
  int listed = 0;
  for (const TopAbs_ShapeEnum type : {TopAbs_VERTEX, TopAbs_EDGE, TopAbs_FACE, TopAbs_SHELL}) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
    TopExp::MapShapes(shape, type, all);
    for (int i = 1; i <= all.Extent() && listed < 30; ++i) {
      const occ::handle<BRepCheck_Result> result = check.Result(all(i));
      if (result.IsNull()) {
        continue;
      }
      for (const BRepCheck_Status status : result->Status()) {
        if (status != BRepCheck_NoError) {
          std::printf("  check: %s %d status %d\n",
                      type == TopAbs_VERTEX ? "vertex" : type == TopAbs_EDGE ? "edge" : type == TopAbs_FACE ? "face" : "shell",
                      i, static_cast<int>(status));
          ++listed;
          break;
        }
      }
    }
  }
}

TopoDS_Shape solid_of(const TopoDS_Shape& shape) {
  if (shape.ShapeType() == TopAbs_COMPOUND) {
    TopoDS_Iterator it(shape);
    if (it.More() && it.Value().ShapeType() == TopAbs_SOLID) {
      return it.Value();
    }
  }
  return shape;
}

// The kernel's offset of the body (a copy, as move_faces takes one) with
// `face` moved; `left_out` faces are not in the shell given to it.
void kernel_offset(const Shape& body, int face, double distance, const std::vector<int>& left_out,
                   const std::string& tag) {
  std::printf("-- the kernel's offset of the body, face %d by %g%s\n", face, distance,
              left_out.empty() ? "" : ", other curved faces left out");
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copy(body.occt(), true, false);
    const TopoDS_Shape copied = copy.Shape();
    TopoDS_Shape input = solid_of(copied);
    if (!left_out.empty()) {
      BRep_Builder builder;
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      for (int i = 0; i < body.face_count(); ++i) {
        bool skip = false;
        for (int l : left_out) {
          skip = skip || l == i;
        }
        if (!skip) {
          builder.Add(shell, copy.Modified(body.face(i)).First());
        }
      }
      input = shell;
    }
    write(tag + "-input", input);
    BRepOffset_MakeOffset maker;
    maker.Initialize(input, 0.0, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false, false);
    maker.SetOffsetOnFace(TopoDS::Face(copy.Modified(body.face(face)).First()), distance);
    maker.MakeOffsetShape(Message_ProgressRange());
    std::printf("done %d error %d\n", maker.IsDone() ? 1 : 0, static_cast<int>(maker.Error()));
    if (maker.IsDone() && !maker.Shape().IsNull()) {
      describe("result", maker.Shape());
      write(tag + "-result", maker.Shape());
      int k = 0;
      for (TopExp_Explorer it(maker.Shape(), TopAbs_FACE); it.More(); it.Next()) {
        print_face(k++, TopoDS::Face(it.Current()), false);
      }
    } else if (maker.IsDone()) {
      std::printf("result: no shape\n");
    }
  } catch (const Standard_Failure& failure) {
    std::printf("raised: %s\n", failure.what());
  }
}

void mitcad_offset(const Shape& body, int face, double distance, bool chain) {
  std::printf("-- offset_faces %d by %g%s\n", face, distance, chain ? ", Tangent Chain" : "");
  try {
    const mitcad::geometry::ShapePtr result =
        mitcad::geometry::offset_faces("F9", body, {body.face_names(face).front()}, distance, chain);
    describe("result", result->occt());
    std::printf("volume change %.6f\n", volume_of(result->occt()) - volume_of(body.occt()));
    write("offset-faces-" + std::to_string(face), result->occt());
  } catch (const std::exception& error) {
    std::printf("failed: %s\n", error.what());
  }
}

} // namespace

MITCAD_BUNDLED_TEST_ARGS(probe_offset_face_loft_side) {
  if (argc < 2) {
    std::printf("usage: test_geometry_loft_shell probe_offset_face_loft_side <body.brep> [distance] [out folder]\n");
    return 0;
  }
  const double distance = argc > 2 ? std::atof(argv[2]) : 2.0;
  out_folder = argc > 3 ? argv[3] : "";
  TopoDS_Shape read;
  BRep_Builder builder;
  if (!BRepTools::Read(read, argv[1], builder)) {
    std::printf("cannot read %s\n", argv[1]);
    return 0;
  }
  TopoDS_Shape solid;
  for (TopExp_Explorer it(read, TopAbs_SOLID); it.More(); it.Next()) {
    if (solid.IsNull()) {
      solid = it.Current();
    } else {
      std::printf("several solids; the first is used\n");
      break;
    }
  }
  if (solid.IsNull()) {
    std::printf("no solid in %s\n", argv[1]);
    return 0;
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(solid, TopAbs_FACE, all);
  std::vector<Shape::NamedFace> named;
  for (int i = 1; i <= all.Extent(); ++i) {
    named.push_back({all(i), {"f" + std::to_string(i - 1)}});
  }
  const Shape body(solid, named);
  describe("body", body.occt());
  std::vector<int> curved;
  for (int i = 0; i < body.face_count(); ++i) {
    const bool plane = BRepAdaptor_Surface(body.face(i)).GetType() == GeomAbs_Plane;
    print_face(i, body.face(i), !plane);
    if (!plane) {
      curved.push_back(i);
    }
  }
  for (int f : curved) {
    kernel_offset(body, f, distance, {}, "kernel-" + std::to_string(f));
    std::vector<int> others;
    for (int g : curved) {
      if (g != f) {
        others.push_back(g);
      }
    }
    if (!others.empty()) {
      kernel_offset(body, f, distance, others, "kernel-open-" + std::to_string(f));
    }
    mitcad_offset(body, f, distance, true);
  }
  return 0;
}
