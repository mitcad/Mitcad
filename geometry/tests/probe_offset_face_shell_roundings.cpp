// SPDX-License-Identifier: MIT
// A probe of Offset Face on the inside of a shelled, rounded block
// (mitcad#2457, GitHub mitcad/Mitcad#678): no ctest, run by hand on a body
// written by `mitcad-cli export <design> <body.brep> --bodies <uid>
// --coordinates component`. Prints, never fails.
//
// test_geometry_loft_shell probe_offset_face_shell_roundings <body.brep> [distance]
//
// For the body: every face (index, surface, its radii or plane, area, box);
// for the faces of interest (cylinders and tori of minor radius `radius`,
// default 12.99, and planes facing -z) their neighbours across each edge,
// with the edge's curve, length and the angle between the faces' normals at
// a tenth, the middle and nine tenths of it. Then offset_faces on each face
// of interest alone and on all of them together, and the geometry kernel's
// own offset of the body with those faces moved (as Offset Face moves whole
// faces): done or its error, the result's solids, shells, free edges and
// validity.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert_SurfToAnaSurf.hxx>
#include <Geom_ConicalSurface.hxx>
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
#include <gp_Cylinder.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Torus.hxx>

#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/shape.hpp"
#include "../src/canonical.hpp"
#include "bundled_test.hpp"

namespace {

using mitcad::geometry::Shape;

const char* surface_name(GeomAbs_SurfaceType type) {
  switch (type) {
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
  case GeomAbs_BezierSurface:
    return "bezier";
  case GeomAbs_BSplineSurface:
    return "bspline";
  case GeomAbs_SurfaceOfRevolution:
    return "revolution";
  case GeomAbs_SurfaceOfExtrusion:
    return "extrusion";
  case GeomAbs_OffsetSurface:
    return "offset";
  default:
    return "other";
  }
}

const char* curve_name(GeomAbs_CurveType type) {
  switch (type) {
  case GeomAbs_Line:
    return "line";
  case GeomAbs_Circle:
    return "circle";
  case GeomAbs_Ellipse:
    return "ellipse";
  case GeomAbs_BSplineCurve:
    return "bspline";
  case GeomAbs_BezierCurve:
    return "bezier";
  case GeomAbs_OffsetCurve:
    return "offset";
  default:
    return "other";
  }
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

void print_face(int i, const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face);
  Bnd_Box box;
  BRepBndLib::AddOptimal(face, box, false, false);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  if (!box.IsVoid()) {
    box.Get(x0, y0, z0, x1, y1, z1);
  }
  std::printf("face %d %s%s area %.6f box [%.4f %.4f %.4f]-[%.4f %.4f %.4f]", i,
              surface_name(surface.GetType()), face.Orientation() == TopAbs_REVERSED ? " (reversed)" : "",
              area_of(face), x0, y0, z0, x1, y1, z1);
  switch (surface.GetType()) {
  case GeomAbs_Plane: {
    const gp_Pln plane = surface.Plane();
    gp_Dir n = plane.Axis().Direction();
    if (face.Orientation() == TopAbs_REVERSED) {
      n.Reverse();
    }
    std::printf(" normal (%.6f %.6f %.6f) at (%.6f %.6f %.6f)", n.X(), n.Y(), n.Z(), plane.Location().X(),
                plane.Location().Y(), plane.Location().Z());
    break;
  }
  case GeomAbs_Cylinder: {
    const gp_Cylinder c = surface.Cylinder();
    std::printf(" radius %.6f axis (%.4f %.4f %.4f) through (%.4f %.4f %.4f)", c.Radius(),
                c.Axis().Direction().X(), c.Axis().Direction().Y(), c.Axis().Direction().Z(),
                c.Location().X(), c.Location().Y(), c.Location().Z());
    break;
  }
  case GeomAbs_Torus: {
    const gp_Torus t = surface.Torus();
    std::printf(" major %.6f minor %.6f axis (%.4f %.4f %.4f) centre (%.4f %.4f %.4f)", t.MajorRadius(),
                t.MinorRadius(), t.Axis().Direction().X(), t.Axis().Direction().Y(),
                t.Axis().Direction().Z(), t.Location().X(), t.Location().Y(), t.Location().Z());
    break;
  }
  default:
    break;
  }
  std::printf(" tolerance %.2g\n", BRep_Tool::Tolerance(face));
}

gp_Dir outward_normal(const TopoDS_Face& face, const TopoDS_Edge& edge, double t, bool& ok) {
  ok = false;
  double first = 0.0, last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return gp_Dir(0, 0, 1);
  }
  const gp_Pnt2d uv = pcurve->Value(first + t * (last - first));
  const BRepAdaptor_Surface surface(face);
  BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1.0e-9);
  if (!props.IsNormalDefined()) {
    return gp_Dir(0, 0, 1);
  }
  gp_Dir n = props.Normal();
  if (face.Orientation() == TopAbs_REVERSED) {
    n.Reverse();
  }
  ok = true;
  return n;
}

void print_neighbours(const Shape& body, int f,
                      const NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>,
                                                       TopTools_ShapeMapHasher>& ancestors) {
  const TopoDS_Face& face = body.face(f);
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      std::printf("  edge degenerated\n");
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    GProp_GProps props;
    BRepGProp::LinearProperties(edge, props);
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    std::printf("  edge %s length %.6f (%.4f %.4f %.4f)-(%.4f %.4f %.4f) tolerance %.2g", curve_name(curve.GetType()),
                props.Mass(), a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z(), BRep_Tool::Tolerance(edge));
    if (!ancestors.Contains(edge)) {
      std::printf(" (no faces)\n");
      continue;
    }
    for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
      const int g = body.face_index(other);
      if (g == f) {
        continue;
      }
      std::printf(" -> face %d angles", g);
      for (const double t : {0.1, 0.5, 0.9}) {
        bool ok_a = false, ok_b = false;
        const gp_Dir na = outward_normal(face, edge, t, ok_a);
        const gp_Dir nb = outward_normal(TopoDS::Face(other), edge, t, ok_b);
        if (ok_a && ok_b) {
          std::printf(" %.3g", na.Angle(nb));
        } else {
          std::printf(" ?");
        }
      }
    }
    std::printf("\n");
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
        std::printf("  free edge %s (%.4f %.4f %.4f)-(%.4f %.4f %.4f)\n", curve_name(curve.GetType()), a.X(),
                    a.Y(), a.Z(), b.X(), b.Y(), b.Z());
      }
    }
  }
  const BRepCheck_Analyzer check(shape);
  std::printf("%s: %d solids, %d shells, %d faces, %d free edges, %s, volume %.6f\n", what, solids, shells,
              faces, free_edges, check.IsValid() ? "valid" : "INVALID", volume_of(shape));
}

// The B-spline face as an analytic surface, within several tolerances.
void analytic(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  for (const double exact : {1.0e-7, 1.0e-6, 1.0e-5, 1.0e-4}) {
    GeomConvert_SurfToAnaSurf convert(surface);
    const occ::handle<Geom_Surface> found = convert.ConvertToAnalytical(exact);
    if (found.IsNull()) {
      std::printf("  within %g: none\n", exact);
      continue;
    }
    std::printf("  within %g: %s gap %.3g", exact, found->DynamicType()->Name(), convert.Gap());
    if (const occ::handle<Geom_ConicalSurface> cone = occ::down_cast<Geom_ConicalSurface>(found)) {
      const gp_Ax1 axis = cone->Axis();
      std::printf(" half angle %.9f radius %.9f axis (%.6f %.6f %.6f) through (%.6f %.6f %.6f)",
                  cone->SemiAngle(), cone->RefRadius(), axis.Direction().X(), axis.Direction().Y(),
                  axis.Direction().Z(), axis.Location().X(), axis.Location().Y(), axis.Location().Z());
    }
    std::printf("\n");
  }
}

// The kernel's offset of the body with `faces` moved (Offset Face's whole
// faces path); with `open` the body's shell without that face, so that the
// offset keeps its shell with gaps and lists their free edges.
void kernel_offset(const Shape& body, const std::vector<int>& faces, double distance, int open = -1) {
  std::printf("-- the kernel's offset of the body, faces");
  for (int f : faces) {
    std::printf(" %d", f);
  }
  std::printf(" by %g%s\n", distance, open >= 0 ? ", left open" : "");
  try {
    OCC_CATCH_SIGNALS
    TopoDS_Shape input = body.occt();
    if (input.ShapeType() == TopAbs_COMPOUND) {
      TopoDS_Iterator it(input);
      if (it.More() && it.Value().ShapeType() == TopAbs_SOLID) {
        input = it.Value();
      }
    }
    if (open >= 0) {
      BRep_Builder builder;
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      for (int i = 0; i < body.face_count(); ++i) {
        if (i != open) {
          builder.Add(shell, body.face(i));
        }
      }
      input = shell;
    }
    BRepOffset_MakeOffset maker;
    maker.Initialize(input, 0.0, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, false, false);
    for (int f : faces) {
      maker.SetOffsetOnFace(body.face(f), distance);
    }
    maker.MakeOffsetShape(Message_ProgressRange());
    std::printf("done %d error %d\n", maker.IsDone() ? 1 : 0, static_cast<int>(maker.Error()));
    if (maker.IsDone()) {
      describe("result", maker.Shape());
    }
  } catch (const Standard_Failure& failure) {
    std::printf("raised: %s\n", failure.what());
  }
}

void mitcad_offset(const Shape& body, const std::vector<int>& faces, double distance) {
  std::vector<std::string> names;
  std::printf("-- offset_faces");
  for (int f : faces) {
    std::printf(" %d", f);
    names.push_back(body.face_names(f).front());
  }
  std::printf(" by %g\n", distance);
  try {
    const mitcad::geometry::ShapePtr result =
        mitcad::geometry::offset_faces("F9", body, names, distance, false);
    describe("result", result->occt());
    std::printf("volume change %.6f\n", volume_of(result->occt()) - volume_of(body.occt()));
  } catch (const std::exception& error) {
    std::printf("failed: %s\n", error.what());
  }
}

} // namespace

MITCAD_BUNDLED_TEST_ARGS(probe_offset_face_shell_roundings) {
  if (argc < 2) {
    std::printf("usage: test_geometry_loft_shell probe_offset_face_shell_roundings <body.brep> [distance] [radius]\n");
    return 0;
  }
  const double distance = argc > 2 ? std::atof(argv[2]) : 2.0;
  const double radius = argc > 3 ? std::atof(argv[3]) : 12.99;
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
  const bool dump_all = argc > 4 && std::string(argv[4]) == "all";
  if (dump_all) {
    for (int i = 0; i < body.face_count(); ++i) {
      print_face(i, body.face(i));
    }
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<int> interest;
  for (int i = 0; i < body.face_count(); ++i) {
    const BRepAdaptor_Surface surface(body.face(i));
    bool pick = false;
    if (surface.GetType() == GeomAbs_Cylinder) {
      pick = std::abs(surface.Cylinder().Radius() - radius) < 1.0e-3;
    } else if (surface.GetType() == GeomAbs_Torus) {
      pick = std::abs(surface.Torus().MinorRadius() - radius) < 1.0e-3;
    } else if (surface.GetType() == GeomAbs_Plane) {
      gp_Dir n = surface.Plane().Axis().Direction();
      if (body.face(i).Orientation() == TopAbs_REVERSED) {
        n.Reverse();
      }
      Bnd_Box box;
      BRepBndLib::AddOptimal(body.face(i), box, false, false);
      pick = n.Z() < -0.999999 && area_of(body.face(i)) > 100.0 && !box.IsVoid() && box.CornerMin().Z() > 50.0;
    }
    if (pick) {
      interest.push_back(i);
      std::printf("== face of interest %d\n", i);
      print_face(i, body.face(i));
      print_neighbours(body, i, ancestors);
    }
  }
  if (!dump_all) {
    // The second round: the B-spline faces beside the faces of interest as
    // analytic surfaces, and the kernel's offsets of parts of the
    // selection, of the body with a face far off left open (its free edges
    // show where the offset faces do not close up), and of the body with
    // its B-spline faces analytic.
    for (int i = 0; i < body.face_count(); ++i) {
      const BRepAdaptor_Surface surface(body.face(i));
      if (surface.GetType() == GeomAbs_BSplineSurface) {
        print_face(i, body.face(i));
        analytic(body.face(i));
      }
    }
    std::vector<int> rest;
    int torus = -1;
    for (int f : interest) {
      if (BRepAdaptor_Surface(body.face(f)).GetType() == GeomAbs_Torus) {
        torus = f;
      } else {
        rest.push_back(f);
      }
    }
    kernel_offset(body, rest, distance);
    if (torus >= 0) {
      kernel_offset(body, {torus}, distance);
    }
    int bottom = -1;
    double lowest = 1.0e100;
    for (int i = 0; i < body.face_count(); ++i) {
      const BRepAdaptor_Surface surface(body.face(i));
      Bnd_Box box;
      BRepBndLib::AddOptimal(body.face(i), box, false, false);
      if (surface.GetType() == GeomAbs_Plane && !box.IsVoid() && box.CornerMax().Z() < lowest &&
          box.CornerMax().Z() - box.CornerMin().Z() < 1.0e-6) {
        lowest = box.CornerMax().Z();
        bottom = i;
      }
    }
    std::printf("== left open: face %d\n", bottom);
    kernel_offset(body, interest, distance, bottom);
    if (torus >= 0) {
      kernel_offset(body, {torus}, distance, bottom);
    }
    for (const double exact : {1.0e-7, 1.0e-6, 1.0e-5}) {
      std::printf("== the body with its B-spline faces analytic within %g\n", exact);
      const mitcad::geometry::ShapePtr canonical =
          mitcad::geometry::detail::canonical_dressup(Shape(TopoDS_Shape()), body, exact);
      if (!canonical) {
        std::printf("nothing converts\n");
        continue;
      }
      describe("analytic body", canonical->occt());
      std::vector<int> moved;
      int moved_torus = -1;
      for (int f : interest) {
        const std::vector<int> found = canonical->find_faces("f" + std::to_string(f));
        if (!found.empty()) {
          moved.push_back(found.front());
          if (f == torus) {
            moved_torus = found.front();
          }
        }
      }
      for (int i = 0; i < canonical->face_count(); ++i) {
        if (!canonical->face_names(i).empty() && canonical->face_names(i).front() == "f61") {
          print_face(i, canonical->face(i));
        }
      }
      kernel_offset(*canonical, moved, distance);
      if (moved_torus >= 0) {
        mitcad_offset(*canonical, {moved_torus}, distance);
      }
      mitcad_offset(*canonical, moved, distance);
    }
    return 0;
  }
  for (int f : interest) {
    mitcad_offset(body, {f}, distance);
  }
  mitcad_offset(body, interest, distance);
  kernel_offset(body, interest, distance);
  return 0;
}
