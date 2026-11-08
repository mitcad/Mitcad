// SPDX-License-Identifier: MIT
// mitcad-rounding-specs: one rounding spec of the .f3d corpora (mitcad#107,
// mitcad#1398) run again through the dressup (geometry/src/dressup.cpp, as
// the import runs it) and compared with the history's stored body after
// it. tools/f3d-rounding-specs-index.py --run drives it, one process per
// spec; core/import/docs/rounding-specs.md describes the whole.
//
//   mitcad-rounding-specs --input SHAPE [--after SHAPE] [--describe] [--chains] fillet|chamfer
//       [--corners rolling|setback|chamfer|miter|blend]
//       (--set KIND A B [chain|no-chain] [--edge NAME]... [--face NAME]...
//        [--reference FACE] [--flip])...
//
// SHAPE files are the kernel's stored-result bytes (the dressup dump's
// .shape files, with the input's face names). KIND is a fillet's
// `constant` (A the radius) or `chord_length` (A the width), or a chamfer's
// `equal_distance` (A), `two_distances` (A, B) or `distance_angle` (A, B
// the angle in radians). Prints `key: value` lines:
//   result: MATCH | NEAR | OFF | INVALID | FAIL
//   error: the dressup's error (FAIL)
//   faces, file_faces, change, file_change, relative: the result's faces
//     and the stored body's, the volume each removes or adds from the
//     input (mm³) and how far apart (relative to the file's change)
// MATCH: valid, the file's face count, the change within 1e-6 of the
// file's; NEAR: valid, the change within 1e-3; OFF: valid, further;
// INVALID: OCCT's checker refuses the result. Exit status 0 when it ran.
// --describe first prints the selected edges, their faces and the other
// faces at their vertices (each surface's kind and radii, `concave` where
// it bends away from the material), to class a spec by its geometry.
// --chains prints the edges the dressup's own tangent chain adds and, at
// each end of the chain and each turn in it (its edges meeting at an
// angle), the edges there: their angle to the chain's edge and the angle
// between their faces at their start, middle and end (mitcad#1435).

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <exception>
#include <fstream>
#include <initializer_list>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/persist.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/tangent_chain.hpp"

namespace {

namespace g = mitcad::geometry;

constexpr const char* kUsage =
    "usage: mitcad-rounding-specs --input SHAPE [--after SHAPE] [--describe] [--chains] fillet|chamfer\n"
    "         [--corners rolling|setback|chamfer|miter|blend]\n"
    "         (--set KIND A B [chain|no-chain] [--edge NAME]... [--face NAME]...\n"
    "          [--reference FACE] [--flip])...\n";

struct UsageError : std::runtime_error {
  using std::runtime_error::runtime_error;
};

g::ShapePtr read_shape(const std::string& path) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    throw std::runtime_error("cannot read " + path);
  }
  std::ostringstream bytes;
  bytes << in.rdbuf();
  const std::string data = bytes.str();
  g::ShapePtr shape = g::deserialize_shape(data);
  if (!shape) {
    throw std::runtime_error("not a shape: " + path);
  }
  return shape;
}

int face_count(const g::Shape& shape) {
  return shape.face_count();
}

double number(const std::string& text) {
  std::size_t used = 0;
  const double value = std::stod(text, &used);
  if (used != text.size()) {
    throw UsageError("not a number: " + text);
  }
  return value;
}

struct Set {
  std::string kind;
  double a = 0.0;
  double b = 0.0;
  bool chain = true;
  std::vector<std::string> edges;
  std::vector<std::string> faces;
  std::string reference;
  bool flip = false;
};

struct Spec {
  std::string input;
  std::string after;
  std::string type;
  std::string corners;
  std::vector<Set> sets;
  bool describe = false;
  bool chains = false;
};

Spec parse(const std::vector<std::string>& args) {
  Spec spec;
  const auto value = [&](std::size_t& i) -> const std::string& {
    if (i + 1 >= args.size()) {
      throw UsageError(args[i] + " needs a value");
    }
    return args[++i];
  };
  for (std::size_t i = 0; i < args.size(); ++i) {
    const std::string& arg = args[i];
    if (arg == "--input") {
      spec.input = value(i);
    } else if (arg == "--after") {
      spec.after = value(i);
    } else if (arg == "--corners") {
      spec.corners = value(i);
    } else if (arg == "--set") {
      if (i + 4 >= args.size()) {
        throw UsageError("--set needs KIND A B chain|no-chain");
      }
      Set set;
      set.kind = args[i + 1];
      set.a = number(args[i + 2]);
      set.b = number(args[i + 3]);
      set.chain = args[i + 4] != "no-chain";
      spec.sets.push_back(set);
      i += 4;
    } else if (arg == "--edge" || arg == "--face" || arg == "--reference" || arg == "--flip") {
      if (spec.sets.empty()) {
        throw UsageError(arg + " before --set");
      }
      Set& set = spec.sets.back();
      if (arg == "--edge") {
        set.edges.push_back(value(i));
      } else if (arg == "--face") {
        set.faces.push_back(value(i));
      } else if (arg == "--reference") {
        set.reference = value(i);
      } else {
        set.flip = true;
      }
    } else if (arg == "--chains") {
      spec.chains = true;
    } else if (arg == "--describe") {
      spec.describe = true;
    } else if (arg == "fillet" || arg == "chamfer") {
      spec.type = arg;
    } else {
      throw UsageError("unknown argument " + arg);
    }
  }
  if (spec.input.empty() || spec.type.empty() || spec.sets.empty()) {
    throw UsageError("an input, a type and a set are needed");
  }
  return spec;
}

// A face's surface, short: its kind and radii, `concave` where its
// normal points towards its axis (a cylinder, cone, sphere or torus whose
// material lies outside the circle: a hole, a pocket's corner, a fillet of
// an inside edge).
std::string surface_of(const TopoDS_Face& face) {
  BRepAdaptor_Surface s(face, false);
  char text[160];
  const bool reversed = face.Orientation() == TopAbs_REVERSED;
  switch (s.GetType()) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder: {
    const bool outward = s.Cylinder().Direct() != reversed;
    std::snprintf(text, sizeof text, "cylinder r %.4g%s", s.Cylinder().Radius(), outward ? "" : " concave");
    return text;
  }
  case GeomAbs_Cone:
    std::snprintf(text, sizeof text, "cone r %.4g half-angle %.4g deg", s.Cone().RefRadius(),
                  s.Cone().SemiAngle() * 57.29577951308232);
    return text;
  case GeomAbs_Sphere: {
    const bool outward = s.Sphere().Direct() != reversed;
    std::snprintf(text, sizeof text, "sphere r %.4g%s", s.Sphere().Radius(), outward ? "" : " concave");
    return text;
  }
  case GeomAbs_Torus: {
    const bool outward = s.Torus().Direct() != reversed;
    std::snprintf(text, sizeof text, "torus R %.4g r %.4g%s", s.Torus().MajorRadius(), s.Torus().MinorRadius(),
                  outward ? "" : " concave");
    return text;
  }
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

std::string face_text(const g::Shape& body, int f) {
  if (f < 0) {
    return "-";
  }
  const g::NameList& names = body.face_names(f);
  return "f" + std::to_string(f) + " " + surface_of(body.face(f)) + " (" +
         (names.empty() ? std::string("unnamed") : names.front()) + ")";
}

// The selected edges, their faces and the other faces at their vertices.
void describe(const Spec& spec, const g::Shape& body) {
  std::vector<int> faces_seen;
  std::vector<int> edges;
  for (const Set& s : spec.sets) {
    for (const std::string& name : s.edges) {
      const std::vector<int> found = body.find_edges(name);
      if (found.empty()) {
        std::printf("edge not found: %s\n", name.c_str());
      }
      edges.insert(edges.end(), found.begin(), found.end());
    }
  }
  for (const int e : edges) {
    const TopoDS_Edge& edge = body.edge(e);
    const std::array<int, 2> at = body.edge_faces(e);
    std::printf("edge e%d %s length %.4g\n  face %s\n  face %s\n", e,
                BRepAdaptor_Curve(edge).GetType() == GeomAbs_Line ? "line" : "curve",
                GCPnts_AbscissaPoint::Length(BRepAdaptor_Curve(edge)), face_text(body, at[0]).c_str(),
                face_text(body, at[1]).c_str());
    faces_seen.push_back(at[0]);
    faces_seen.push_back(at[1]);
  }
  for (const int e : edges) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(body.edge(e), first, last);
    for (const TopoDS_Vertex& v : {first, last}) {
      const int vi = body.vertex_index(v);
      if (vi < 0) {
        continue;
      }
      const gp_Pnt p = BRep_Tool::Pnt(v);
      for (const int f : body.vertex_faces(vi)) {
        if (std::find(faces_seen.begin(), faces_seen.end(), f) != faces_seen.end()) {
          continue;
        }
        faces_seen.push_back(f);
        std::printf("at vertex v%d (%.4f, %.4f, %.4f): face %s\n", vi, p.X(), p.Y(), p.Z(),
                    face_text(body, f).c_str());
      }
    }
  }
}

// The unit tangent of an edge at one of its vertices, pointing away from
// it (zero where the curve has no direction there).
gp_Vec tangent_away(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  const BRepAdaptor_Curve curve(edge);
  const bool at_first = TopExp::FirstVertex(edge).IsSame(vertex);
  gp_Pnt point;
  gp_Vec d;
  curve.D1(at_first ? curve.FirstParameter() : curve.LastParameter(), point, d);
  if (d.Magnitude() <= 1e-12) {
    return gp_Vec();
  }
  d.Normalize();
  return at_first ? d : -d;
}

// The angle between the faces of an edge (their outward normals) at its
// start, middle and end, in radians.
std::string crease(const g::Shape& body, int edge) {
  const std::array<int, 2> faces = body.edge_faces(edge);
  if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
    return "-";
  }
  std::string out;
  for (const double t : {0.0, 0.5, 1.0}) {
    gp_Vec n[2];
    for (int k = 0; k < 2; ++k) {
      const TopoDS_Face& face = body.face(faces[k]);
      double first = 0.0;
      double last = 0.0;
      const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(body.edge(edge), face, first, last);
      if (pcurve.IsNull()) {
        return "-";
      }
      const gp_Pnt2d uv = pcurve->Value(first + (last - first) * t);
      BRepLProp_SLProps props(BRepAdaptor_Surface(face, false), uv.X(), uv.Y(), 1, 1e-9);
      if (!props.IsNormalDefined()) {
        return "-";
      }
      n[k] = gp_Vec(props.Normal());
      if (face.Orientation() == TopAbs_REVERSED) {
        n[k].Reverse();
      }
    }
    char text[32];
    std::snprintf(text, sizeof text, "%s%.4g", out.empty() ? "" : " ", n[0].Angle(n[1]));
    out += text;
  }
  return out;
}

// The edges the dressup's own tangent chain adds to the selection, and at
// each end of the chain and each turn in it the edges there: their angle
// to the chain's edge (0 where one runs straight on from the other) and
// their faces' crease (crease()).
void chains(const Spec& spec, const g::Shape& body) {
  std::vector<int> selected;
  for (const Set& s : spec.sets) {
    for (const std::string& name : s.edges) {
      for (const int e : body.find_edges(name)) {
        selected.push_back(e);
      }
    }
  }
  std::vector<int> grown = selected;
  for (const std::vector<int>& chain : g::edge_tangent_chains(body)) {
    const bool touches = std::any_of(chain.begin(), chain.end(), [&](int e) {
      return std::find(selected.begin(), selected.end(), e) != selected.end();
    });
    for (const int e : chain) {
      if (touches && std::find(grown.begin(), grown.end(), e) == grown.end()) {
        grown.push_back(e);
        std::printf("chain adds e%d %s\n", e, body.edge_name(e).c_str());
      }
    }
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> edges_at;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_at);
  const auto in_chain = [&](int i) { return std::find(grown.begin(), grown.end(), i) != grown.end(); };
  for (const int e : grown) {
    for (const TopoDS_Vertex& vertex : {TopExp::FirstVertex(body.edge(e)), TopExp::LastVertex(body.edge(e))}) {
      const NCollection_List<TopoDS_Shape>& at = edges_at.FindFromKey(vertex);
      const gp_Vec away = tangent_away(body.edge(e), vertex);
      const auto angle_to = [&](int i) {
        const gp_Vec other = tangent_away(body.edge(i), vertex);
        return away.Magnitude() > 0.5 && other.Magnitude() > 0.5 ? away.Angle(-other) : -1.0;
      };
      // A chain's end, or a turn within it (an edge of the chain at an
      // angle there, listed from the lower edge number).
      bool end = true;
      bool turn = false;
      for (const TopoDS_Shape& o : at) {
        const int i = body.edge_index(o);
        if (i != e && in_chain(i)) {
          end = false;
          turn = turn || (i > e && angle_to(i) > 1e-2);
        }
      }
      if (!end && !turn) {
        continue;
      }
      const gp_Pnt p = BRep_Tool::Pnt(vertex);
      std::printf("chain %s at e%d v%d (%.5f, %.5f, %.5f)\n", end ? "end" : "turn", e, body.vertex_index(vertex),
                  p.X(), p.Y(), p.Z());
      for (const TopoDS_Shape& o : at) {
        const int i = body.edge_index(o);
        if (i == e || i < 0 || BRep_Tool::Degenerated(TopoDS::Edge(o))) {
          continue;
        }
        const std::array<int, 2> fe = body.edge_faces(e);
        const std::array<int, 2> fo = body.edge_faces(i);
        const bool shares = fe[0] == fo[0] || fe[0] == fo[1] || fe[1] == fo[0] || fe[1] == fo[1];
        std::printf("  e%d %s: angle %.6g%s%s, faces at %s\n", i, body.edge_name(i).c_str(), angle_to(i),
                    shares ? ", shares a face" : "", in_chain(i) ? ", in the chain" : "", crease(body, i).c_str());
      }
    }
  }
}

g::ShapePtr run(const Spec& spec, const g::Shape& body) {
  const std::string feature = "S";
  if (spec.type == "fillet") {
    std::vector<g::FilletSet> sets;
    for (const Set& s : spec.sets) {
      g::FilletSet set;
      set.edges = s.edges;
      set.faces = s.faces;
      set.tangent_chain = s.chain;
      if (s.kind == "constant") {
        set.size = g::FilletSize::Constant;
        set.radius = s.a;
      } else if (s.kind == "chord_length") {
        set.size = g::FilletSize::ChordLength;
        set.chord = s.a;
      } else {
        throw UsageError("fillet size " + s.kind + " is not run here");
      }
      sets.push_back(set);
    }
    return g::fillet(feature, body, sets, spec.corners != "setback");
  }
  std::vector<g::ChamferSet> sets;
  for (const Set& s : spec.sets) {
    g::ChamferSet set;
    set.edges = s.edges;
    set.faces = s.faces;
    set.tangent_chain = s.chain;
    set.spec.reference_face = s.reference;
    set.spec.flip = s.flip;
    if (s.kind == "equal_distance") {
      set.spec.type = g::ChamferType::EqualDistance;
      set.spec.distance = s.a;
    } else if (s.kind == "two_distances") {
      set.spec.type = g::ChamferType::TwoDistances;
      set.spec.distance = s.a;
      set.spec.distance2 = s.b;
    } else if (s.kind == "distance_angle") {
      set.spec.type = g::ChamferType::DistanceAngle;
      set.spec.distance = s.a;
      set.spec.angle = s.b;
    } else {
      throw UsageError("chamfer size " + s.kind + " is not run here");
    }
    sets.push_back(set);
  }
  g::ChamferCorner corner = g::ChamferCorner::Chamfer;
  if (spec.corners == "miter") {
    corner = g::ChamferCorner::Miter;
  } else if (spec.corners == "blend") {
    corner = g::ChamferCorner::Blend;
  }
  return g::chamfer(feature, body, sets, corner);
}

} // namespace

int main(int argc, char** argv) {
  const std::vector<std::string> args(argv + 1, argv + argc);
  Spec spec;
  try {
    spec = parse(args);
  } catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n%s", e.what(), kUsage);
    return 2;
  }
  try {
    const g::ShapePtr input = read_shape(spec.input);
    if (spec.describe) {
      describe(spec, *input);
    }
    if (spec.chains) {
      chains(spec, *input);
    }
    g::ShapePtr result;
    try {
      result = run(spec, *input);
    } catch (const UsageError&) {
      throw;
    } catch (const std::exception& e) {
      std::printf("result: FAIL\nerror: %s\n", e.what());
      return 0;
    }
    if (!result) {
      std::printf("result: FAIL\nerror: no result\n");
      return 0;
    }
    const double before = g::volume(*input);
    const double change = g::volume(*result) - before;
    std::printf("faces: %d\nchange: %.9g\n", face_count(*result), change);
    const bool valid = BRepCheck_Analyzer(result->occt()).IsValid();
    if (spec.after.empty()) {
      std::printf("result: %s\n", valid ? "BUILT" : "INVALID");
      return 0;
    }
    const g::ShapePtr after = read_shape(spec.after);
    const double file_change = g::volume(*after) - before;
    const double relative = std::abs(change - file_change) / std::max(std::abs(file_change), 1e-9);
    const int file_faces = face_count(*after);
    std::printf("file_faces: %d\nfile_change: %.9g\nrelative: %.3g\n", file_faces, file_change, relative);
    const char* verdict = "OFF";
    if (!valid) {
      verdict = "INVALID";
    } else if (relative <= 1e-6 && face_count(*result) == file_faces) {
      verdict = "MATCH";
    } else if (relative <= 1e-3) {
      verdict = "NEAR";
    }
    std::printf("result: %s\n", verdict);
    return 0;
  } catch (const UsageError& e) {
    std::fprintf(stderr, "%s\n%s", e.what(), kUsage);
    return 2;
  } catch (const std::exception& e) {
    std::fprintf(stderr, "%s\n", e.what());
    return 1;
  }
}
