// SPDX-License-Identifier: MIT
#pragma once

// A probe of mitcad#2170, temporary: run only with MITCAD_PROBE_2170 set,
// printed and never checked. How OCCT's boolean splits a face one of whose
// vertices several of its wires pass (an inner wire touching the outer wire
// there, a wire passing a vertex twice): the edges the boolean gives the
// face builder (as BOPAlgo_Builder::BuildSplitFaces collects them), the
// edges at each vertex used more than twice with their directions in the
// face's parameters, the loops the wire splitter makes of them, whether
// each is a hole, and the faces built with the checker's findings. On the
// reported design (its finned rods joined with each body they meet, the 26
// bodies' union, plates across a rod at the touching vertex) and on a small
// case built from scratch (a prism of a square with a triangular hole
// touching its outer wire at a vertex, joined with plates).

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <sstream>
#include <string>
#include <vector>

#include <BOPAlgo_Builder.hxx>
#include <BOPAlgo_BuilderFace.hxx>
#include <BOPDS_DS.hxx>
#include <BOPDS_FaceInfo.hxx>
#include <BOPDS_PaveBlock.hxx>
#include <BOPTools_AlgoTools.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Surface.hxx>
#include <IntTools_Context.hxx>
#include <IntTools_FClass2d.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

namespace probe2170 {

using ShapeSet = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

inline const char* kind_name(TopAbs_ShapeEnum type) {
  if (type == TopAbs_SOLID) {
    return "solid";
  }
  if (type == TopAbs_SHELL) {
    return "shell";
  }
  if (type == TopAbs_FACE) {
    return "face";
  }
  if (type == TopAbs_WIRE) {
    return "wire";
  }
  if (type == TopAbs_EDGE) {
    return "edge";
  }
  if (type == TopAbs_VERTEX) {
    return "vertex";
  }
  return "shape";
}

inline const char* orientation_name(TopAbs_Orientation orientation) {
  if (orientation == TopAbs_FORWARD) {
    return "F";
  }
  if (orientation == TopAbs_REVERSED) {
    return "R";
  }
  if (orientation == TopAbs_INTERNAL) {
    return "I";
  }
  return "E";
}

inline const char* surface_name(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  const GeomAbs_SurfaceType type = surface.GetType();
  if (type == GeomAbs_Plane) {
    return "plane";
  }
  if (type == GeomAbs_Cylinder) {
    return "cylinder";
  }
  if (type == GeomAbs_Cone) {
    return "cone";
  }
  if (type == GeomAbs_Sphere) {
    return "sphere";
  }
  if (type == GeomAbs_Torus) {
    return "torus";
  }
  return "other";
}

inline std::string status_text(BRepCheck_Status status) {
  std::ostringstream out;
  BRepCheck::Print(status, out);
  std::string text = out.str();
  while (!text.empty() && (text.back() == '\n' || text.back() == '\r' || text.back() == ' ')) {
    text.pop_back();
  }
  return text;
}

// The checker's findings on the sub-shapes of a shape: each one's own
// statuses and those in the context of another; at most `limit` lines.
// The faces with findings go into `faces`.
inline int print_findings(const TopoDS_Shape& shape, const char* prefix, int limit, ShapeSet* faces = nullptr) {
  const BRepCheck_Analyzer analyzer(shape);
  if (analyzer.IsValid()) {
    std::printf("%svalid\n", prefix);
    return 0;
  }
  ShapeSet subs;
  static const TopAbs_ShapeEnum kinds[] = {TopAbs_SOLID, TopAbs_SHELL, TopAbs_FACE,
                                           TopAbs_WIRE,  TopAbs_EDGE,  TopAbs_VERTEX};
  for (const TopAbs_ShapeEnum kind : kinds) {
    TopExp::MapShapes(shape, kind, subs);
  }
  int lines = 0;
  for (int i = 1; i <= subs.Extent(); ++i) {
    const occ::handle<BRepCheck_Result>& result = analyzer.Result(subs(i));
    if (result.IsNull()) {
      continue;
    }
    std::string said;
    for (const BRepCheck_Status status : result->Status()) {
      if (status != BRepCheck_NoError) {
        said += " " + status_text(status);
      }
    }
    for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
      for (const BRepCheck_Status status : result->StatusOnShape()) {
        if (status != BRepCheck_NoError) {
          said += " " + status_text(status) + " (in a " + kind_name(result->ContextualShape().ShapeType()) + ")";
          if (faces != nullptr && result->ContextualShape().ShapeType() == TopAbs_FACE) {
            faces->Add(result->ContextualShape());
          }
        }
      }
    }
    if (said.empty()) {
      continue;
    }
    if (faces != nullptr && subs(i).ShapeType() == TopAbs_FACE) {
      faces->Add(subs(i));
    }
    if (lines < limit) {
      std::printf("%s%s %d:%s\n", prefix, kind_name(subs(i).ShapeType()), i, said.c_str());
    }
    ++lines;
  }
  std::printf("%s%d sub-shapes with findings\n", prefix, lines);
  return lines;
}

// The point of an edge's curve on the face at the start of its travel
// (`first`, by the edge's orientation) or at its end, and the direction
// from there into the edge.
inline bool end_of(const TopoDS_Edge& edge, const TopoDS_Face& face, bool first, gp_Pnt2d& point, gp_Vec2d& into) {
  double f = 0.0;
  double l = 0.0;
  const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, face, f, l);
  if (curve.IsNull()) {
    return false;
  }
  const bool forward = edge.Orientation() != TopAbs_REVERSED;
  const bool at_first = first == forward;
  const double t = at_first ? f : l;
  const double step = (l - f) * 1e-4 * (at_first ? 1.0 : -1.0);
  point = curve->Value(t);
  into = gp_Vec2d(point, curve->Value(t + step));
  return true;
}

inline double length_of(const TopoDS_Shape& edge) {
  GProp_GProps props;
  BRepGProp::LinearProperties(edge, props);
  return props.Mass();
}

// The signed area a wire bounds in the face's parameters (its edges'
// curves sampled in the wire's order) and the largest gap between the end
// of one edge and the start of the next.
inline double signed_area(const TopoDS_Shape& wire, const TopoDS_Face& face, double& gap) {
  std::vector<gp_Pnt2d> points;
  gap = 0.0;
  bool started = false;
  gp_Pnt2d first_start;
  gp_Pnt2d last_end;
  for (TopoDS_Iterator it(wire); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Value());
    double f = 0.0;
    double l = 0.0;
    const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, face, f, l);
    if (curve.IsNull()) {
      continue;
    }
    const bool forward = edge.Orientation() != TopAbs_REVERSED;
    const gp_Pnt2d start = curve->Value(forward ? f : l);
    if (started) {
      gap = std::max(gap, last_end.Distance(start));
    } else {
      first_start = start;
      started = true;
    }
    for (int k = 0; k <= 32; ++k) {
      const double s = static_cast<double>(k) / 32.0;
      points.push_back(curve->Value(forward ? f + (l - f) * s : l - (l - f) * s));
    }
    last_end = curve->Value(forward ? l : f);
  }
  if (started) {
    gap = std::max(gap, last_end.Distance(first_start));
  }
  double area = 0.0;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const gp_Pnt2d& a = points[i];
    const gp_Pnt2d& b = points[(i + 1) % points.size()];
    area += a.X() * b.Y() - b.X() * a.Y();
  }
  return 0.5 * area;
}

// The face's wires: each edge with its orientation, vertices (numbered in
// `vertices`), its ends in the face's parameters and its length.
inline void dump_face(const TopoDS_Face& face, const char* prefix) {
  ShapeSet vertices;
  int w = 0;
  for (TopoDS_Iterator wires(face); wires.More(); wires.Next(), ++w) {
    std::printf("%swire %d (%s):\n", prefix, w, orientation_name(wires.Value().Orientation()));
    for (TopoDS_Iterator it(wires.Value()); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Value());
      const int v1 = vertices.Add(TopExp::FirstVertex(edge, true));
      const int v2 = vertices.Add(TopExp::LastVertex(edge, true));
      gp_Pnt2d a;
      gp_Pnt2d b;
      gp_Vec2d da;
      gp_Vec2d db;
      end_of(edge, face, true, a, da);
      end_of(edge, face, false, b, db);
      std::printf("%s  %s v%d (%.6f %.6f) -> v%d (%.6f %.6f), length %.6f%s%s\n", prefix,
                  orientation_name(edge.Orientation()), v1, a.X(), a.Y(), v2, b.X(), b.Y(), length_of(edge),
                  BRep_Tool::IsClosed(edge, face) ? ", closed on the face" : "",
                  BRep_Tool::Degenerated(edge) ? ", degenerated" : "");
    }
  }
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertices(i)));
    std::printf("%sv%d at (%.6f %.6f %.6f), tolerance %.3g\n", prefix, i, p.X(), p.Y(), p.Z(),
                BRep_Tool::Tolerance(TopoDS::Vertex(vertices(i))));
  }
}

// The most uses of one vertex by the face's edges (each use of an edge in
// the face counted, a seam twice).
inline int most_vertex_uses(const TopoDS_Face& face) {
  NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    for (TopoDS_Iterator v(it.Current()); v.More(); v.Next()) {
      const int index = uses.FindIndex(v.Value());
      if (index == 0) {
        uses.Add(v.Value(), 1);
      } else {
        ++uses(index);
      }
    }
  }
  int most = 0;
  for (int i = 1; i <= uses.Extent(); ++i) {
    most = std::max(most, uses(i));
  }
  return most;
}

inline int wire_count(const TopoDS_Shape& face) {
  int n = 0;
  for (TopoDS_Iterator it(face); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// How the boolean splits `input` (a face of an argument): the edges
// BOPAlgo_Builder::BuildSplitFaces gives the face builder, the vertices
// used more than twice, the face builder's loops and faces.
inline void trace_split(BOPAlgo_Builder& builder, const TopoDS_Face& input, const char* title) {
  const BOPDS_PDS ds = builder.PDS();
  const int index = ds->Index(input);
  std::printf("    [%s] %s face, %d wires, DS index %d\n", title, surface_name(input), wire_count(input), index);
  if (index < 0 || !ds->HasFaceInfo(index)) {
    std::printf("      no face info\n");
    return;
  }
  const BOPDS_FaceInfo& info = ds->FaceInfo(index);
  std::printf("      pave blocks in %d, on %d, section %d\n", info.PaveBlocksIn().Extent(),
              info.PaveBlocksOn().Extent(), info.PaveBlocksSc().Extent());
  TopoDS_Face face = input;
  face.Orientation(TopAbs_FORWARD);
  std::printf("      the face as given:\n");
  dump_face(face, "        ");
  const auto& images = builder.Images();
  const occ::handle<IntTools_Context> context = new IntTools_Context;
  NCollection_List<TopoDS_Shape> edges;
  std::vector<std::string> origins;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seams_done;
  const auto both = [&](TopoDS_Edge edge, const char* origin) {
    edge.Orientation(TopAbs_FORWARD);
    edges.Append(edge);
    origins.push_back(origin);
    edge.Orientation(TopAbs_REVERSED);
    edges.Append(edge);
    origins.push_back(origin);
  };
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const NCollection_List<TopoDS_Shape>* splits = images.Seek(edge);
    if (splits == nullptr) {
      if (edge.Orientation() == TopAbs_INTERNAL) {
        both(edge, "bound, internal");
      } else {
        edges.Append(edge);
        origins.push_back("bound");
      }
      continue;
    }
    const bool seam = BRep_Tool::IsClosed(edge, face);
    for (NCollection_List<TopoDS_Shape>::Iterator s(*splits); s.More(); s.Next()) {
      TopoDS_Edge split = TopoDS::Edge(s.Value());
      if (BRep_Tool::Degenerated(edge)) {
        split.Orientation(edge.Orientation());
        edges.Append(split);
        origins.push_back("split, degenerated");
        continue;
      }
      if (edge.Orientation() == TopAbs_INTERNAL) {
        both(split, "split, internal");
        continue;
      }
      if (seam) {
        if (seams_done.Add(split)) {
          both(split, BRep_Tool::IsClosed(split, face) ? "split of a seam" : "split of a seam, not closed");
        }
        continue;
      }
      split.Orientation(edge.Orientation());
      if (BOPTools_AlgoTools::IsSplitToReverse(split, edge, context)) {
        split.Reverse();
      }
      edges.Append(split);
      origins.push_back("split");
    }
  }
  for (int j = 1; j <= info.PaveBlocksIn().Extent(); ++j) {
    both(TopoDS::Edge(ds->Shape(info.PaveBlocksIn()(j)->Edge())), "in");
  }
  for (int j = 1; j <= info.PaveBlocksSc().Extent(); ++j) {
    both(TopoDS::Edge(ds->Shape(info.PaveBlocksSc()(j)->Edge())), "section");
  }
  // The edges, numbered, with their vertices numbered.
  ShapeSet vertices;
  std::vector<TopoDS_Edge> listed;
  std::printf("      the edges given to the face builder:\n");
  int k = 0;
  for (NCollection_List<TopoDS_Shape>::Iterator it(edges); it.More(); it.Next(), ++k) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Value());
    listed.push_back(edge);
    const int v1 = vertices.Add(TopExp::FirstVertex(edge, true));
    const int v2 = vertices.Add(TopExp::LastVertex(edge, true));
    gp_Pnt2d a;
    gp_Pnt2d b;
    gp_Vec2d da;
    gp_Vec2d db;
    const bool has_curve = end_of(edge, face, true, a, da) && end_of(edge, face, false, b, db);
    std::printf("        e%d %s %s: v%d (%.6f %.6f) -> v%d (%.6f %.6f), length %.6f%s%s\n", k,
                orientation_name(edge.Orientation()), origins[static_cast<std::size_t>(k)].c_str(), v1, a.X(), a.Y(),
                v2, b.X(), b.Y(), length_of(edge), has_curve ? "" : ", no curve on the face",
                BRep_Tool::IsClosed(edge, face) ? ", closed on the face" : "");
  }
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(vertices(i));
    std::vector<std::string> at;
    for (std::size_t e = 0; e < listed.size(); ++e) {
      for (int end = 0; end < 2; ++end) {
        const TopoDS_Vertex v = end == 0 ? TopExp::FirstVertex(listed[e], true) : TopExp::LastVertex(listed[e], true);
        if (!v.IsSame(vertex)) {
          continue;
        }
        gp_Pnt2d point;
        gp_Vec2d into;
        if (!end_of(listed[e], face, end == 0, point, into)) {
          continue;
        }
        char text[256];
        std::snprintf(text, sizeof text, "e%zu %s at (%.6f %.6f) direction %.3f deg", e, end == 0 ? "out" : "in",
                      point.X(), point.Y(), std::atan2(into.Y(), into.X()) * 57.29577951308232);
        at.emplace_back(text);
      }
    }
    if (at.size() <= 2) {
      continue;
    }
    const gp_Pnt p = BRep_Tool::Pnt(vertex);
    std::printf("      v%d at (%.6f %.6f %.6f), tolerance %.3g, %zu uses:\n", i, p.X(), p.Y(), p.Z(),
                BRep_Tool::Tolerance(vertex), at.size());
    for (const std::string& line : at) {
      std::printf("        %s\n", line.c_str());
    }
  }
  // The face builder on those edges: its loops (the wire splitter's) and
  // its faces.
  BOPAlgo_BuilderFace face_builder;
  face_builder.SetFace(face);
  face_builder.SetShapes(edges);
  face_builder.SetContext(context);
  face_builder.Perform();
  std::printf("      face builder: errors %d, warnings %d, %d loops, %d faces\n", face_builder.HasErrors() ? 1 : 0,
              face_builder.HasWarnings() ? 1 : 0, face_builder.Loops().Extent(), face_builder.Areas().Extent());
  TopLoc_Location location;
  const occ::handle<Geom_Surface>& surface = BRep_Tool::Surface(face, location);
  int l = 0;
  for (NCollection_List<TopoDS_Shape>::Iterator it(face_builder.Loops()); it.More(); it.Next(), ++l) {
    std::string members;
    for (TopoDS_Iterator e(it.Value()); e.More(); e.Next()) {
      int found = -1;
      for (std::size_t n = 0; n < listed.size(); ++n) {
        if (listed[n].IsEqual(e.Value())) {
          found = static_cast<int>(n);
        }
      }
      members += found >= 0 ? " e" + std::to_string(found) : std::string(" ?");
    }
    double gap = 0.0;
    const double area = signed_area(it.Value(), face, gap);
    TopoDS_Face loop_face;
    BRep_Builder().MakeFace(loop_face, surface, location, BRep_Tool::Tolerance(face));
    BRep_Builder().Add(loop_face, it.Value());
    const IntTools_FClass2d classifier(loop_face, BRep_Tool::Tolerance(face));
    std::printf("        loop %d:%s; area %.6g, largest gap in its order %.3g, hole %d\n", l, members.c_str(), area,
                gap, classifier.IsHole() ? 1 : 0);
  }
  int a = 0;
  for (NCollection_List<TopoDS_Shape>::Iterator it(face_builder.Areas()); it.More(); it.Next(), ++a) {
    std::printf("        face %d: %d wires\n", a, wire_count(it.Value()));
    print_findings(it.Value(), "          ", 6);
  }
}

// The faces of `shape` with a vertex used more than twice.
inline std::vector<TopoDS_Face> touching_faces(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (most_vertex_uses(face) > 2) {
      faces.push_back(face);
    }
  }
  return faces;
}

struct Joined {
  bool done = false;
  bool valid = false;
  int solids = 0;
};

// OCCT's exact union of the two (the first the argument), as the union
// makes it; where invalid, the checker's findings and the split of each
// face of either with a vertex used more than twice or with an invalid
// image.
// The vertices the face's own wires meet at points more than half a period
// apart in its parameters (port patch 2170's condition), edges closed on
// the face or degenerated left out.
inline int period_apart_vertices(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, true);
  const double u_period = surface.IsUPeriodic() ? surface.UPeriod() : 0.0;
  const double v_period = surface.IsVPeriodic() ? surface.VPeriod() : 0.0;
  if (u_period <= 0.0 && v_period <= 0.0) {
    return 0;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, gp_Pnt2d, TopTools_ShapeMapHasher> first;
  ShapeSet apart;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge) || BRep_Tool::IsClosed(edge, face)) {
      continue;
    }
    for (int end = 0; end < 2; ++end) {
      gp_Pnt2d point;
      gp_Vec2d into;
      if (!end_of(edge, face, end == 0, point, into)) {
        continue;
      }
      const TopoDS_Vertex vertex = end == 0 ? TopExp::FirstVertex(edge, true) : TopExp::LastVertex(edge, true);
      const int index = first.FindIndex(vertex);
      if (index == 0) {
        first.Add(vertex, point);
      } else if ((u_period > 0.0 && std::abs(first(index).X() - point.X()) > 0.5 * u_period) ||
                 (v_period > 0.0 && std::abs(first(index).Y() - point.Y()) > 0.5 * v_period)) {
        apart.Add(vertex);
      }
    }
  }
  return apart.Extent();
}

// With `force`, the faces of either with a vertex used more than twice are
// traced also where the union is valid, and each face whose own wires meet
// a vertex a period apart is listed.
inline Joined join_and_trace(const TopoDS_Shape& argument, const TopoDS_Shape& tool, bool trace,
                             bool force = false) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(argument);
  tools.Append(tool);
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.Build();
  Joined out;
  out.done = fuse.IsDone() && !fuse.HasErrors();
  if (!out.done) {
    return out;
  }
  for (TopExp_Explorer it(fuse.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
    ++out.solids;
  }
  out.valid = BRepCheck_Analyzer(fuse.Shape()).IsValid();
  if ((out.valid && !force) || !trace) {
    return out;
  }
  if (force) {
    for (const TopoDS_Shape* shape : {&argument, &tool}) {
      int f = 0;
      for (TopExp_Explorer it(*shape, TopAbs_FACE); it.More(); it.Next(), ++f) {
        const TopoDS_Face& face = TopoDS::Face(it.Current());
        const int apart = period_apart_vertices(face);
        if (apart > 0) {
          std::printf("    %s face %d (%s, %d wires): %d vertices its wires meet a period apart\n",
                      shape == &argument ? "argument" : "tool", f, surface_name(face), wire_count(face), apart);
        }
      }
    }
  }
  ShapeSet bad;
  print_findings(fuse.Shape(), "    ", 12, &bad);
  BOPAlgo_Builder& builder = *fuse.Builder();
  const auto& images = builder.Images();
  int traced = 0;
  for (const TopoDS_Shape* shape : {&argument, &tool}) {
    for (TopExp_Explorer it(*shape, TopAbs_FACE); it.More() && traced < 4; it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      bool invalid_image = false;
      if (const NCollection_List<TopoDS_Shape>* split = images.Seek(face)) {
        for (NCollection_List<TopoDS_Shape>::Iterator s(*split); s.More(); s.Next()) {
          invalid_image = invalid_image || bad.Contains(s.Value());
        }
      }
      const bool touching = most_vertex_uses(face) > 2;
      if (!invalid_image && !touching) {
        continue;
      }
      trace_split(builder, face, invalid_image ? (touching ? "invalid image, touching" : "invalid image") : "touching");
      ++traced;
    }
  }
  return out;
}

// A prism 10 high of a 10 x 10 square whose triangular hole touches the
// square's right side at its middle vertex (one vertex of both wires): the
// prism's edge there lies on four faces.
inline TopoDS_Shape touching_hole_prism() {
  const auto vertex = [](double x, double y) { return BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, 0.0)).Vertex(); };
  const TopoDS_Vertex a = vertex(0, 0);
  const TopoDS_Vertex b = vertex(10, 0);
  const TopoDS_Vertex t = vertex(10, 5);
  const TopoDS_Vertex c = vertex(10, 10);
  const TopoDS_Vertex d = vertex(0, 10);
  const TopoDS_Vertex u = vertex(6, 3);
  const TopoDS_Vertex w = vertex(6, 7);
  const auto edge = [](const TopoDS_Vertex& v1, const TopoDS_Vertex& v2) {
    return BRepBuilderAPI_MakeEdge(v1, v2).Edge();
  };
  BRepBuilderAPI_MakeWire outer;
  outer.Add(edge(a, b));
  outer.Add(edge(b, t));
  outer.Add(edge(t, c));
  outer.Add(edge(c, d));
  outer.Add(edge(d, a));
  // Clockwise: the hole.
  BRepBuilderAPI_MakeWire hole;
  hole.Add(edge(t, u));
  hole.Add(edge(u, w));
  hole.Add(edge(w, t));
  BRepBuilderAPI_MakeFace maker(gp_Pln(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), outer.Wire());
  maker.Add(hole.Wire());
  return BRepPrimAPI_MakePrism(maker.Face(), gp_Vec(0, 0, 10)).Shape();
}

inline void probe_synthetic() {
  std::printf("probe 2170: synthetic cases\n");
  const TopoDS_Shape prism = touching_hole_prism();
  std::printf("  the prism: %d faces with a vertex used more than twice\n",
              static_cast<int>(touching_faces(prism).size()));
  print_findings(prism, "    ", 6);
  struct Plate {
    const char* name;
    gp_Pnt corner;
    double dx;
    double dy;
    double dz;
  };
  const Plate plates[] = {
      {"across the top over the hole", gp_Pnt(5, -1, 8), 2, 12, 4},
      {"across the top through the touching vertex", gp_Pnt(9, -1, 8), 2, 12, 4},
      {"around the touching vertex on the top", gp_Pnt(9.5, 4, 8), 1, 2, 4},
      {"on the top over part of the hole", gp_Pnt(2, 2, 10), 6, 6, 1},
      {"within the hole at the top", gp_Pnt(7, 4, 8), 1, 2, 4},
      {"through the edge on four faces at mid height", gp_Pnt(8, 3, 4), 4, 4, 1},
      {"across the top past the hole, not touching it", gp_Pnt(1, -1, 8), 2, 12, 4},
  };
  for (const Plate& plate : plates) {
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(plate.corner, plate.dx, plate.dy, plate.dz).Shape();
    std::printf("  plate %s:\n", plate.name);
    const Joined joined = join_and_trace(prism, box, true);
    std::printf("    done %d, %d solids, valid %d\n", joined.done ? 1 : 0, joined.solids, joined.valid ? 1 : 0);
  }
}

// A case of MITCAD_GEOMETRY_CASES (`folder` with body.brep and pad.brep):
// OCCT's exact union of the two, its volume against theirs, and the split
// of each face of either with a vertex used more than twice.
inline void probe_case(const std::string& folder) {
  std::printf("probe 2170: the case in %s\n", folder.c_str());
  TopoDS_Shape body;
  TopoDS_Shape pad;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (folder + "/body.brep").c_str(), builder) ||
      !BRepTools::Read(pad, (folder + "/pad.brep").c_str(), builder)) {
    std::printf("  not read\n");
    return;
  }
  const auto volume = [](const TopoDS_Shape& shape) {
    GProp_GProps props;
    BRepGProp::VolumeProperties(shape, props);
    return props.Mass();
  };
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  arguments.Append(body);
  tools.Append(pad);
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.Build();
  std::printf("  body %.6f, pad %.6f, exact union %.6f, warnings %d\n", volume(body), volume(pad),
              fuse.IsDone() ? volume(fuse.Shape()) : -1.0, fuse.HasWarnings() ? 1 : 0);
  const Joined joined = join_and_trace(body, pad, true, true);
  std::printf("  done %d, %d solids, valid %d\n", joined.done ? 1 : 0, joined.solids, joined.valid ? 1 : 0);
}

inline bool boxes_meet(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  Bnd_Box box_a;
  Bnd_Box box_b;
  BRepBndLib::Add(a, box_a);
  BRepBndLib::Add(b, box_b);
  box_a.Enlarge(1e-3);
  return !box_a.IsOut(box_b);
}

inline int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The reported design's bodies, the hub first; `inside` the bodies lying
// wholly inside others.
inline void probe_reported(const std::vector<TopoDS_Shape>& bodies, const std::vector<std::size_t>& inside) {
  std::printf("probe 2170: the reported design\n");
  std::vector<std::size_t> rods;
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    if (face_count(bodies[i]) == 20) {
      rods.push_back(i);
    }
  }
  for (const std::size_t rod : rods) {
    const std::vector<TopoDS_Face> faces = touching_faces(bodies[rod]);
    std::printf("  finned rod %zu: %d faces, %zu with a vertex used more than twice:", rod, face_count(bodies[rod]),
                faces.size());
    for (const TopoDS_Face& face : faces) {
      std::printf(" (%s, %d wires, %d uses)", surface_name(face), wire_count(face), most_vertex_uses(face));
    }
    std::printf("\n");
    print_findings(bodies[rod], "    ", 4);
  }
  if (!rods.empty()) {
    std::printf("  the first finned rod's faces with a vertex used more than twice:\n");
    for (const TopoDS_Face& face : touching_faces(bodies[rods.front()])) {
      std::printf("    %s face, %s:\n", surface_name(face), orientation_name(face.Orientation()));
      dump_face(face, "      ");
    }
  }
  // Each finned rod with each body it meets.
  int traced = 0;
  for (const std::size_t rod : rods) {
    for (std::size_t i = 0; i < bodies.size(); ++i) {
      if (i == rod || std::find(inside.begin(), inside.end(), i) != inside.end() ||
          !boxes_meet(bodies[rod], bodies[i])) {
        continue;
      }
      std::printf("  finned rod %zu with body %zu (%d faces):\n", rod, i, face_count(bodies[i]));
      const Joined joined = join_and_trace(bodies[rod], bodies[i], traced < 2);
      if (!joined.valid) {
        ++traced;
      }
      std::printf("    done %d, %d solids, valid %d\n", joined.done ? 1 : 0, joined.solids, joined.valid ? 1 : 0);
    }
  }
  // The union of the bodies not lying inside others: its findings and
  // the input faces their faces come from.
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  NCollection_List<TopoDS_Shape> tools;
  std::vector<std::size_t> kept;
  for (std::size_t i = 0; i < bodies.size(); ++i) {
    if (std::find(inside.begin(), inside.end(), i) == inside.end()) {
      (kept.empty() ? arguments : tools).Append(bodies[i]);
      kept.push_back(i);
    }
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  fuse.SetRunParallel(true);
  fuse.Build();
  std::printf("  the union of the %zu bodies not inside others: done %d, warnings %d\n", kept.size(),
              fuse.IsDone() ? 1 : 0, fuse.HasWarnings() ? 1 : 0);
  if (fuse.IsDone()) {
    ShapeSet bad;
    print_findings(fuse.Shape(), "    ", 20, &bad);
    const auto& images = fuse.Builder()->Images();
    for (const std::size_t i : kept) {
      int f = 0;
      for (TopExp_Explorer it(bodies[i], TopAbs_FACE); it.More(); it.Next(), ++f) {
        const NCollection_List<TopoDS_Shape>* split = images.Seek(it.Current());
        int invalid = 0;
        int count = 0;
        if (split != nullptr) {
          for (NCollection_List<TopoDS_Shape>::Iterator s(*split); s.More(); s.Next()) {
            ++count;
            invalid += bad.Contains(s.Value()) ? 1 : 0;
          }
        } else {
          invalid = bad.Contains(it.Current()) ? 1 : 0;
        }
        if (invalid != 0) {
          const TopoDS_Face& face = TopoDS::Face(it.Current());
          std::printf("    body %zu face %d (%s, %d wires, %d uses at a vertex): %d of %d images invalid\n", i, f,
                      surface_name(face), wire_count(face), most_vertex_uses(face), invalid, count);
        }
      }
    }
  }
  // Plates across the first finned rod, square to its rod's axis, at the
  // vertex its faces use more than twice and beside it.
  if (rods.empty()) {
    return;
  }
  const TopoDS_Shape& rod = bodies[rods.front()];
  for (const TopoDS_Face& face : touching_faces(rod)) {
    const BRepAdaptor_Surface surface(face, false);
    if (surface.GetType() != GeomAbs_Cylinder) {
      continue;
    }
    const gp_Ax1 axis = surface.Cylinder().Axis();
    // The vertex used most.
    NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      for (TopoDS_Iterator v(it.Current()); v.More(); v.Next()) {
        const int index = uses.FindIndex(v.Value());
        if (index == 0) {
          uses.Add(v.Value(), 1);
        } else {
          ++uses(index);
        }
      }
    }
    int most = 1;
    for (int i = 2; i <= uses.Extent(); ++i) {
      if (uses(i) > uses(most)) {
        most = i;
      }
    }
    const gp_Pnt at = BRep_Tool::Pnt(TopoDS::Vertex(uses.FindKey(most)));
    const gp_Vec along(axis.Direction());
    const double s = gp_Vec(axis.Location(), at).Dot(along);
    for (const double offset : {0.0, 0.3, 1.0, 3.0, -3.0}) {
      const gp_Pnt centre = axis.Location().Translated(along * (s + offset));
      const gp_Ax2 frame(centre, axis.Direction());
      const gp_Pnt corner = centre.Translated(gp_Vec(frame.XDirection()) * -6.0 + gp_Vec(frame.YDirection()) * -6.0 +
                                              along * -0.5);
      const TopoDS_Shape plate =
          BRepPrimAPI_MakeBox(gp_Ax2(corner, axis.Direction(), frame.XDirection()), 12.0, 12.0, 1.0).Shape();
      std::printf("  a plate across the first finned rod at %.1f mm from its touching vertex:\n", offset);
      const Joined joined = join_and_trace(rod, plate, offset == 0.0 || offset == 1.0);
      std::printf("    done %d, %d solids, valid %d\n", joined.done ? 1 : 0, joined.solids, joined.valid ? 1 : 0);
    }
    break;
  }
}

} // namespace probe2170
