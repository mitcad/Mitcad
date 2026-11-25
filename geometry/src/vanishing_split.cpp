// SPDX-License-Identifier: MIT
#include "vanishing_split.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstddef>
#include <exception>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Curve2d.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Precision.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Ancestors =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// Samples along an edge where its convexity is looked at: a sign change
// between two of them is a vanishing point, then found by bisection.
constexpr int kSamples = 128;
// A vanishing point nearer than this to an end of its edge splits nothing
// (mm): the faces are tangent at that vertex within what the walk into it
// takes for vanishing.
constexpr double kEndGap = 1.0e-3;
// The sine of the angle between the faces' normals below which an edge is
// smooth all along (face_select.cpp's kSmoothAngle).
constexpr double kSharp = 1.0e-3;
// The sine below which a sample has no sign (numerical noise about
// tangency).
constexpr double kNoise = 1.0e-7;
// The faces at a vertex are tangent there within this angle: OCCT's
// contour stops at such a vertex, each rounding ending in a point
// (ChFi3d_Builder::PerformElement, the vanishing end of port patch 0771).
constexpr double kTangentAtVertex = 5.0e-4;

// An edge between its two faces, with how convex it is along it.
class Sides {
public:
  Sides(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b)
      : m_a(a), m_b(b), m_curve(edge), m_onA(edge, a), m_onB(edge, b), m_surfaceA(a, false), m_surfaceB(b, false) {
    int found = 0;
    for (TopExp_Explorer x(a, TopAbs_EDGE); x.More(); x.Next()) {
      if (x.Current().IsSame(edge)) {
        m_reversedInA = x.Current().Orientation() == TopAbs_REVERSED;
        ++found;
      }
    }
    // (Once in the face, forward or reversed: not a seam of it.)
    m_valid = found == 1 &&
              (m_curve.LastParameter() - m_curve.FirstParameter()) > Precision::PConfusion();
  }

  bool valid() const { return m_valid; }
  double first() const { return m_curve.FirstParameter(); }
  double last() const { return m_curve.LastParameter(); }
  gp_Pnt point(double t) const { return m_curve.Value(t); }

  // T · (n_a × n_b): T the edge's unit tangent as it runs in face a, n_a and
  // n_b the faces' outward unit normals. With the material of face a on the
  // left of T (seen against n_a), this is the sine of the angle between the
  // normals, positive where the edge is convex and negative where it is
  // concave: the face b turns down from a's tangent plane at a convex edge
  // (an outward normal n_b, a tangent T along a box's top edge). None where
  // a normal or the tangent has no direction.
  std::optional<double> convexity(double t) const {
    gp_Vec na;
    gp_Vec nb;
    if (!normals(t, na, nb)) {
      return std::nullopt;
    }
    gp_Pnt p;
    gp_Vec d;
    m_curve.D1(t, p, d);
    if (d.Magnitude() <= gp::Resolution()) {
      return std::nullopt;
    }
    d.Normalize();
    if (m_reversedInA) {
      d.Reverse();
    }
    return d.Dot(na.Crossed(nb));
  }

  // n_a · n_b: positive where the faces turn the same way (tangent where
  // the convexity passes through nothing), negative where they fold back.
  std::optional<double> facing(double t) const {
    gp_Vec na;
    gp_Vec nb;
    if (!normals(t, na, nb)) {
      return std::nullopt;
    }
    return na.Dot(nb);
  }

  // The unit tangent of the edge's curve at t (along the curve).
  std::optional<gp_Vec> tangent(double t) const {
    gp_Pnt p;
    gp_Vec d;
    m_curve.D1(t, p, d);
    if (d.Magnitude() <= gp::Resolution()) {
      return std::nullopt;
    }
    return d.Normalized();
  }

private:
  static bool normal(const TopoDS_Face& face, const BRepAdaptor_Surface& surface, const gp_Pnt2d& uv,
                     gp_Vec& n) {
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), p, du, dv);
    n = du.Crossed(dv);
    if (n.Magnitude() <= gp::Resolution()) {
      return false;
    }
    n.Normalize();
    if (face.Orientation() == TopAbs_REVERSED) {
      n.Reverse();
    }
    return true;
  }

  bool normals(double t, gp_Vec& na, gp_Vec& nb) const {
    return normal(m_a, m_surfaceA, m_onA.Value(t), na) && normal(m_b, m_surfaceB, m_onB.Value(t), nb);
  }

  TopoDS_Face m_a;
  TopoDS_Face m_b;
  BRepAdaptor_Curve m_curve;
  BRepAdaptor_Curve2d m_onA;
  BRepAdaptor_Curve2d m_onB;
  BRepAdaptor_Surface m_surfaceA;
  BRepAdaptor_Surface m_surfaceB;
  bool m_reversedInA = false;
  bool m_valid = false;
};

// The two faces of the edge (as they lie in the body, oriented); false for
// a free edge, a seam, or an edge of more faces.
bool two_faces(const Ancestors& ancestors, const TopoDS_Edge& edge, TopoDS_Face& a, TopoDS_Face& b) {
  if (BRep_Tool::Degenerated(edge) || !ancestors.Contains(edge)) {
    return false;
  }
  const NCollection_List<TopoDS_Shape>& faces = ancestors.FindFromKey(edge);
  if (faces.Extent() != 2) {
    return false;
  }
  a = TopoDS::Face(faces.First());
  b = TopoDS::Face(faces.Last());
  return !a.IsSame(b) && !BRep_Tool::IsClosed(edge, a) && !BRep_Tool::IsClosed(edge, b);
}

// An end of an edge at a vertex: the curve's parameter there and its unit
// tangent pointing away from the edge.
struct End {
  double at = 0.0;
  gp_Vec away;
};

// The ends of the edge at the vertex (two for a closed edge).
std::vector<End> ends_at(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex, const Sides& sides) {
  std::vector<End> out;
  for (const bool at_first : {true, false}) {
    // (TopExp::FirstVertex: the forward vertex, at the curve's first
    // parameter, whatever the edge's orientation.)
    const TopoDS_Vertex end = at_first ? TopExp::FirstVertex(edge) : TopExp::LastVertex(edge);
    if (end.IsNull() || !end.IsSame(vertex)) {
      continue;
    }
    const double at = at_first ? sides.first() : sides.last();
    const std::optional<gp_Vec> d = sides.tangent(at);
    if (!d) {
      continue;
    }
    out.push_back({at, at_first ? d->Reversed() : *d});
  }
  return out;
}

// -1 concave, 1 convex, 0 where the faces are tangent at the parameter;
// none where it cannot be told (a face's normal not defined there, as at a
// degenerate corner of its surface).
std::optional<int> convexity_sign(const Sides& sides, double at) {
  const std::optional<double> c = sides.convexity(at);
  if (!c) {
    return std::nullopt;
  }
  if (std::abs(*c) < std::sin(kTangentAtVertex)) {
    return 0;
  }
  return *c > 0.0 ? 1 : -1;
}

// The name with every occurrence of a rename's key (the longest where
// several start at a place) replaced, in one pass. With `suffix`, a piece's
// name standing alone between parentheses (a face's key: fillet(<piece>))
// becomes its edge's name with the piece's place after the parenthesis, as
// a split face's "#k".
std::string renamed(const std::string& name, const UnsplitNames& names, bool suffix) {
  std::string out;
  for (std::size_t at = 0; at < name.size();) {
    const std::pair<const std::string, UnsplitNames::Rename>* longest = nullptr;
    // (The keys are edge and vertex names, which start with E{ or V{.)
    if (name[at] == 'E' || name[at] == 'V') {
      for (const auto& rename : names.renames) {
        if (name.compare(at, rename.first.size(), rename.first) == 0 &&
            (longest == nullptr || rename.first.size() > longest->first.size())) {
          longest = &rename;
        }
      }
    }
    if (longest == nullptr) {
      out += name[at];
      ++at;
      continue;
    }
    const std::size_t end = at + longest->first.size();
    // (Not a prefix of a longer "#k": E{a|b}#1 in E{a|b}#12.)
    if (end < name.size() && std::isdigit(static_cast<unsigned char>(name[end])) != 0) {
      out += name[at];
      ++at;
      continue;
    }
    const UnsplitNames::Rename& rename = longest->second;
    if (suffix && rename.piece >= 0 && at > 0 && name[at - 1] == '(' && end < name.size() && name[end] == ')') {
      out += rename.to + ")#" + std::to_string(rename.piece);
      at = end + 1;
      continue;
    }
    out += rename.to;
    at = end;
  }
  return out;
}

// A body with edges split: the shape, each piece's input edge and place
// along it (by the piece's TShape), and the image of each input face.
struct SplitShape {
  TopoDS_Shape shape;
  std::map<const TopoDS_TShape*, std::pair<int, int>> piece_of;
  std::vector<TopoDS_Shape> faces;
};

// A new vertex on the edge (forward) at its curve's parameter t, its
// tolerance reaching the edge's curves on its two faces there.
TopoDS_Vertex vertex_on(const TopoDS_Edge& edge, const TopoDS_Face& fa, const TopoDS_Face& fb, double t) {
  const BRepAdaptor_Curve curve(edge);
  const gp_Pnt p = curve.Value(t);
  const auto off = [&](const TopoDS_Face& face) {
    const BRepAdaptor_Curve2d on(edge, face);
    const BRepAdaptor_Surface surface(face, false);
    const gp_Pnt2d uv = on.Value(t);
    return p.Distance(surface.Value(uv.X(), uv.Y()));
  };
  const double tolerance =
      std::max({BRep_Tool::Tolerance(edge), Precision::Confusion(), 1.01 * off(fa), 1.01 * off(fb)});
  TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(p).Vertex();
  BRep_Builder().UpdateVertex(vertex, tolerance);
  return vertex;
}

// Whether the split body is valid where it changed (the input's faces it
// keeps as they were checked); else the checker's words in `fault`.
bool valid_where_split(const Shape& input, const TopoDS_Shape& split, std::string& fault) {
  ShapeMap kept;
  TopExp::MapShapes(input.occt(), TopAbs_FACE, kept);
  try {
    require_valid_where_changed(split, kept, "the split");
    return true;
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception& error) {
    fault = error.what();
    return false;
  }
}

// Each edge of `at` replaced by its pieces, which share its curves (trimmed
// to the pieces' ranges, as the kernel's booleans make their split edges)
// and meet at new vertices on it, as face_join.cpp's cuts split the edges
// they cross; the faces rebuilt with them. None where the result is not
// valid (why in `fault`).
std::optional<SplitShape> split_by_pieces(const Shape& input, const std::map<int, std::vector<double>>& at,
                                          const Ancestors& edge_faces, std::string& fault) {
  BRep_Builder builder;
  const occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  SplitShape out;
  for (const auto& [e, points] : at) {
    const TopoDS_Edge edge = TopoDS::Edge(input.edge(e).Oriented(TopAbs_FORWARD));
    TopoDS_Face fa;
    TopoDS_Face fb;
    two_faces(edge_faces, edge, fa, fb);
    TopoDS_Vertex v0;
    TopoDS_Vertex v1;
    TopExp::Vertices(edge, v0, v1);
    double first = 0.0;
    double last = 0.0;
    BRep_Tool::Range(edge, first, last);
    if (v0.IsNull() || v1.IsNull()) {
      fault = "an edge to split has no vertices";
      return std::nullopt;
    }
    std::vector<std::pair<double, TopoDS_Vertex>> cuts{{first, v0}};
    for (const double t : points) {
      cuts.emplace_back(t, vertex_on(edge, fa, fb, t));
    }
    cuts.emplace_back(last, v1);
    TopoDS_Compound parts;
    builder.MakeCompound(parts);
    for (std::size_t i = 0; i + 1 < cuts.size(); ++i) {
      TopoDS_Edge part = TopoDS::Edge(edge.EmptyCopied());
      part.Orientation(TopAbs_FORWARD);
      builder.Add(part, cuts[i].second.Oriented(TopAbs_FORWARD));
      builder.Add(part, cuts[i + 1].second.Oriented(TopAbs_REVERSED));
      builder.Range(part, cuts[i].first, cuts[i + 1].first);
      // (The new vertices only: the edge's own are the input's, which the
      // split must not change; a vertex at an end of an edge is at the end
      // of its range.)
      for (const std::size_t j : {i, i + 1}) {
        if (j != 0 && j + 1 != cuts.size()) {
          builder.UpdateVertex(cuts[j].second, cuts[j].first, part, BRep_Tool::Tolerance(cuts[j].second));
        }
      }
      out.piece_of[part.TShape().get()] = {e, static_cast<int>(i)};
      builder.Add(parts, part);
    }
    reshape->Replace(edge, parts);
  }
  out.shape = reshape->Apply(input.occt());
  if (out.shape.IsNull()) {
    fault = "the edges' replacement gives no shape";
    return std::nullopt;
  }
  if (!valid_where_split(input, out.shape, fault)) {
    return std::nullopt;
  }
  for (int f = 0; f < input.face_count(); ++f) {
    out.faces.push_back(reshape->Value(input.face(f)));
  }
  return out;
}

// The edges of `at` split by the kernel's splitter (the general fuse) with
// the new vertices on them as its tools: its split edges and the faces it
// builds again with them. None where it fails, splits otherwise or its
// result is not valid (why in `fault`).
std::optional<SplitShape> split_by_kernel(const Shape& input, const std::map<int, std::vector<double>>& at,
                                          const Ancestors& edge_faces, std::string& fault) {
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(input.occt());
  NCollection_List<TopoDS_Shape> tools;
  for (const auto& [e, points] : at) {
    const TopoDS_Edge edge = TopoDS::Edge(input.edge(e).Oriented(TopAbs_FORWARD));
    TopoDS_Face fa;
    TopoDS_Face fb;
    two_faces(edge_faces, edge, fa, fb);
    for (const double t : points) {
      tools.Append(vertex_on(edge, fa, fb, t));
    }
  }
  BRepAlgoAPI_Splitter splitter;
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.SetNonDestructive(true);
  build(splitter);
  if (!splitter.IsDone() || splitter.HasErrors()) {
    fault = "the splitter fails";
    return std::nullopt;
  }
  SplitShape out;
  int solids = 0;
  for (TopExp_Explorer x(splitter.Shape(), TopAbs_SOLID); x.More(); x.Next()) {
    out.shape = x.Current();
    ++solids;
  }
  if (solids != 1) {
    fault = "the splitter gives " + std::to_string(solids) + " solids";
    return std::nullopt;
  }
  for (const auto& [e, points] : at) {
    std::vector<std::pair<double, TopoDS_Shape>> pieces;
    for (const TopoDS_Shape& image : splitter.Modified(input.edge(e))) {
      if (image.ShapeType() == TopAbs_EDGE) {
        double first = 0.0;
        double last = 0.0;
        BRep_Tool::Range(TopoDS::Edge(image), first, last);
        pieces.emplace_back(first, image);
      }
    }
    if (pieces.size() != points.size() + 1) {
      fault = "the splitter splits an edge into " + std::to_string(pieces.size()) + " pieces, not " +
              std::to_string(points.size() + 1);
      return std::nullopt;
    }
    std::sort(pieces.begin(), pieces.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      out.piece_of[pieces[i].second.TShape().get()] = {e, static_cast<int>(i)};
    }
  }
  if (!valid_where_split(input, out.shape, fault)) {
    return std::nullopt;
  }
  for (int f = 0; f < input.face_count(); ++f) {
    const NCollection_List<TopoDS_Shape>& images = splitter.Modified(input.face(f));
    out.faces.push_back(images.IsEmpty() ? input.face(f) : images.First());
  }
  return out;
}

} // namespace

std::vector<double> vanishing_parameters(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b) {
  std::vector<double> out;
  if (BRep_Tool::Degenerated(edge) || a.IsSame(b)) {
    return out;
  }
  try {
    OCC_CATCH_SIGNALS
    // (Two planes meet along a line at one angle.)
    if (BRepAdaptor_Surface(a, false).GetType() == GeomAbs_Plane &&
        BRepAdaptor_Surface(b, false).GetType() == GeomAbs_Plane) {
      return out;
    }
    const Sides sides(edge, a, b);
    if (!sides.valid()) {
      return out;
    }
    const double first = sides.first();
    const double last = sides.last();
    // The stretches of one sign along the edge, each by its first and last
    // sample and its largest sine; samples within kNoise of tangent have
    // no sign.
    struct Run {
      double from = 0.0;
      double to = 0.0;
      bool convex = false;
      double largest = 0.0;
    };
    std::vector<Run> runs;
    for (int i = 0; i <= kSamples; ++i) {
      const double t = first + (last - first) * static_cast<double>(i) / kSamples;
      const std::optional<double> c = sides.convexity(t);
      if (!c || std::abs(*c) < kNoise) {
        continue;
      }
      const bool convex = *c > 0.0;
      if (runs.empty() || runs.back().convex != convex) {
        runs.push_back({t, t, convex, 0.0});
      }
      runs.back().to = t;
      runs.back().largest = std::max(runs.back().largest, std::abs(*c));
    }
    // Only stretches where the faces clearly meet at an angle count: a
    // near-tangent stretch whose sine wavers about nothing (faces tangent
    // within a file's tolerance) turns no edge.
    runs.erase(std::remove_if(runs.begin(), runs.end(), [](const Run& run) { return run.largest < kSharp; }),
               runs.end());
    const gp_Pnt start = sides.point(first);
    const gp_Pnt end = sides.point(last);
    for (std::size_t i = 0; i + 1 < runs.size(); ++i) {
      const bool convex = runs[i].convex;
      if (convex == runs[i + 1].convex) {
        continue;
      }
      // Bisection to the parameter's last bit, between the two stretches.
      double lo = runs[i].to;
      double hi = runs[i + 1].from;
      for (int k = 0; k < 200; ++k) {
        const double mid = 0.5 * (lo + hi);
        if (!(mid > lo && mid < hi)) {
          break;
        }
        const std::optional<double> c = sides.convexity(mid);
        if (!c) {
          break;
        }
        if (*c == 0.0) {
          lo = mid;
          hi = mid;
          break;
        }
        if ((*c > 0.0) == convex) {
          lo = mid;
        } else {
          hi = mid;
        }
      }
      const double root = 0.5 * (lo + hi);
      // (Where the faces fold back on each other the sine passes through
      // nothing as well: a knife edge, no vanishing rounding.)
      const std::optional<double> facing = sides.facing(root);
      if (!facing || *facing <= 0.0) {
        continue;
      }
      const gp_Pnt p = sides.point(root);
      if (p.Distance(start) <= kEndGap || p.Distance(end) <= kEndGap ||
          (!out.empty() && sides.point(out.back()).Distance(p) <= kEndGap)) {
        continue;
      }
      out.push_back(root);
    }
  } catch (const Standard_Failure&) {
    out.clear();
  }
  return out;
}

std::optional<VanishingSplit> split_at_vanishing_points(const Shape& input, const std::vector<FilletSet>& sets,
                                                       std::string* why) {
  const auto refused = [why](const char* reason) {
    if (why != nullptr) {
      *why = reason;
    }
    return std::optional<VanishingSplit>();
  };
  // (The kernel ends a constant rounding only in a point where it vanishes,
  // PerformFilletOnVertex.)
  for (const FilletSet& set : sets) {
    if (set.size != FilletSize::Constant || set.curvature) {
      return refused("a set is not a constant rounding");
    }
  }
  const int edges = input.edge_count();
  std::vector<int> chosen(static_cast<std::size_t>(edges), -1);
  try {
    for (std::size_t s = 0; s < sets.size(); ++s) {
      for (const int e : resolve_edges(input, sets[s].edges, sets[s].faces)) {
        int& set = chosen[static_cast<std::size_t>(e)];
        if (set >= 0 && set != static_cast<int>(s)) {
          return refused("an edge is in two sets"); // the fillet says so
        }
        set = static_cast<int>(s);
      }
    }
  } catch (const Cancelled&) {
    throw;
  } catch (const std::exception&) {
    return refused("the selection does not resolve"); // the fillet names what is wrong
  }
  Ancestors edge_faces;
  TopExp::MapShapesAndUniqueAncestors(input.occt(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
  Ancestors vertex_edges;
  TopExp::MapShapesAndUniqueAncestors(input.occt(), TopAbs_VERTEX, TopAbs_EDGE, vertex_edges);

  // The tangent chains of the sets that follow them: first the kernel's own
  // contours of the selected edges, as round_edges asks for them (its
  // contours stopping where their edges turn, StopChainsAtTurns), which are
  // the edges it rounds; the contours are only set up, not computed.
  {
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
    for (int e = 0; e < edges; ++e) {
      if (chosen[static_cast<std::size_t>(e)] >= 0) {
        selected.Add(input.edge(e));
      }
    }
    try {
      OCC_CATCH_SIGNALS
      BRepFilletAPI_MakeFillet maker(input.occt());
      maker.StopChainsAtTurns(selected);
      for (int e = 0; e < edges; ++e) {
        const int s = chosen[static_cast<std::size_t>(e)];
        if (s < 0 || !sets[static_cast<std::size_t>(s)].tangent_chain || maker.Contour(input.edge(e)) != 0) {
          continue;
        }
        maker.Add(sets[static_cast<std::size_t>(s)].radius, input.edge(e));
        const int contour = maker.Contour(input.edge(e));
        for (int k = 1; contour > 0 && k <= maker.NbEdges(contour); ++k) {
          const int o = input.edge_index(maker.Edge(contour, k));
          if (o >= 0 && chosen[static_cast<std::size_t>(o)] < 0) {
            chosen[static_cast<std::size_t>(o)] = s;
          }
        }
      }
    } catch (const Standard_Failure&) {
    }
  }
  // Then MitCad's own: edges that meet end to end (within
  // kTangentChainAngle) and are not one convex and the other concave at the
  // vertex they share, whatever they are in their middles. The pieces of an
  // edge split below are then rounded all, as the edge would be.
  std::vector<int> queue;
  for (int e = 0; e < edges; ++e) {
    const int s = chosen[static_cast<std::size_t>(e)];
    if (s >= 0 && sets[static_cast<std::size_t>(s)].tangent_chain) {
      queue.push_back(e);
    }
  }
  while (!queue.empty()) {
    const int e = queue.back();
    queue.pop_back();
    try {
      OCC_CATCH_SIGNALS
      const TopoDS_Edge& edge = input.edge(e);
      TopoDS_Face a;
      TopoDS_Face b;
      if (!two_faces(edge_faces, edge, a, b)) {
        continue;
      }
      const Sides sides(edge, a, b);
      if (!sides.valid()) {
        continue;
      }
      for (const TopoDS_Vertex& vertex : {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)}) {
        if (vertex.IsNull() || !vertex_edges.Contains(vertex)) {
          continue;
        }
        for (const End& own : ends_at(edge, vertex, sides)) {
          const std::optional<int> kind = convexity_sign(sides, own.at);
          if (kind && *kind == 0) {
            continue; // the rounding vanishes at the vertex
          }
          for (const TopoDS_Shape& next : vertex_edges.FindFromKey(vertex)) {
            const int o = input.edge_index(next);
            if (o < 0 || o == e || chosen[static_cast<std::size_t>(o)] >= 0) {
              continue;
            }
            const TopoDS_Edge& other = TopoDS::Edge(next);
            TopoDS_Face oa;
            TopoDS_Face ob;
            if (!two_faces(edge_faces, other, oa, ob) || is_smooth(other, oa, ob)) {
              continue;
            }
            const Sides other_sides(other, oa, ob);
            if (!other_sides.valid()) {
              continue;
            }
            for (const End& theirs : ends_at(other, vertex, other_sides)) {
              // (Not where its rounding vanishes at the vertex, nor where one
              // is convex there and the other concave; where either cannot
              // be told, as the kernel's chain takes it.)
              const std::optional<int> next_kind = convexity_sign(other_sides, theirs.at);
              const bool vanishes = next_kind && *next_kind == 0;
              const bool turns = kind && next_kind && *kind != *next_kind;
              if (own.away.Angle(theirs.away.Reversed()) <= kTangentChainAngle && !vanishes && !turns) {
                chosen[static_cast<std::size_t>(o)] = chosen[static_cast<std::size_t>(e)];
                queue.push_back(o);
                break;
              }
            }
          }
        }
      }
    } catch (const Standard_Failure&) {
    }
  }

  // The vanishing points of the edges to round.
  std::map<int, std::vector<double>> at;
  for (int e = 0; e < edges; ++e) {
    if (chosen[static_cast<std::size_t>(e)] < 0) {
      continue;
    }
    TopoDS_Face a;
    TopoDS_Face b;
    if (!two_faces(edge_faces, input.edge(e), a, b)) {
      continue;
    }
    std::vector<double> points = vanishing_parameters(input.edge(e), a, b);
    if (!points.empty()) {
      at[e] = std::move(points);
    }
  }
  if (at.empty()) {
    return refused("no edge to round has a point where its faces come tangent");
  }

  try {
    OCC_CATCH_SIGNALS
    // The edges split by their pieces put in their place; where the checker
    // finds that body invalid, by the kernel's splitter with the new
    // vertices as its tools (the general fuse's split edges and faces).
    // Both split the edges at the same points and change nothing else; the
    // checker sees the result where it changed, in the solid (as
    // require_valid_where_changed checks the booleans' results).
    std::string own_fault;
    std::string kernel_fault;
    std::optional<SplitShape> built = split_by_pieces(input, at, edge_faces, own_fault);
    if (!built) {
      built = split_by_kernel(input, at, edge_faces, kernel_fault);
    }
    if (!built) {
      if (why != nullptr) {
        *why = "the split body is not valid: " + own_fault + "; split by the kernel's splitter: " + kernel_fault;
      }
      return std::nullopt;
    }
    const std::map<const TopoDS_TShape*, std::pair<int, int>>& piece_of = built->piece_of;
    std::vector<Shape::NamedFace> faces;
    for (int f = 0; f < input.face_count(); ++f) {
      faces.push_back({built->faces[static_cast<std::size_t>(f)], input.face_names(f)});
    }
    VanishingSplit result;
    result.body = std::make_shared<Shape>(built->shape, faces);
    const Shape& body = *result.body;
    result.sets = sets;
    for (FilletSet& set : result.sets) {
      set.edges.clear();
      set.faces.clear();
    }
    // The set of each edge of the split body.
    std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
    for (int i = 0; i < body.edge_count(); ++i) {
      const auto piece = piece_of.find(body.edge(i).TShape().get());
      if (piece != piece_of.end()) {
        result.pieces[i] = piece->second;
        owner[static_cast<std::size_t>(i)] = chosen[static_cast<std::size_t>(piece->second.first)];
      } else if (const int e = input.edge_index(body.edge(i)); e >= 0) {
        owner[static_cast<std::size_t>(i)] = chosen[static_cast<std::size_t>(e)];
      }
    }
    if (result.pieces.empty()) {
      return refused("the split body has none of the pieces");
    }
    // Each edge by its name: a name that finds it and edges of its own set
    // only (several where the name has no "#k"), once.
    for (int i = 0; i < body.edge_count(); ++i) {
      const int s = owner[static_cast<std::size_t>(i)];
      if (s < 0) {
        continue;
      }
      const std::string& name = body.edge_name(i);
      if (name.empty()) {
        return refused("an edge to round has no name on the split body");
      }
      const std::vector<int> found = body.find_edges(name);
      if (std::find(found.begin(), found.end(), i) == found.end() ||
          std::any_of(found.begin(), found.end(), [&](int j) { return owner[static_cast<std::size_t>(j)] != s; })) {
        return refused("an edge to round is not found alone by its name on the split body");
      }
      std::vector<std::string>& listed = result.sets[static_cast<std::size_t>(s)].edges;
      if (std::find(listed.begin(), listed.end(), name) == listed.end()) {
        listed.push_back(name);
      }
    }
    for (const FilletSet& set : result.sets) {
      if (set.edges.empty()) {
        return refused("a set has no edge on the split body");
      }
    }
    return result;
  } catch (const Standard_Failure& failure) {
    if (why != nullptr) {
      *why = std::string("the split raised: ") + failure.what();
    }
    return std::nullopt;
  }
}

UnsplitNames unsplit_names(const Shape& input, const VanishingSplit& split) {
  UnsplitNames names;
  const Shape& body = *split.body;
  for (int i = 0; i < body.edge_count(); ++i) {
    const std::string& own = body.edge_name(i);
    if (own.empty()) {
      continue;
    }
    const auto piece = split.pieces.find(i);
    if (piece != split.pieces.end()) {
      const std::string& theirs = input.edge_name(piece->second.first);
      if (!theirs.empty()) {
        names.renames[own] = {theirs, piece->second.second};
      }
      continue;
    }
    const int e = input.edge_index(body.edge(i));
    if (e >= 0 && !input.edge_name(e).empty() && input.edge_name(e) != own) {
      names.renames[own] = {input.edge_name(e), -1};
    }
  }
  for (int v = 0; v < body.vertex_count(); ++v) {
    const std::string& own = body.vertex_name(v);
    const int i = own.empty() ? -1 : input.vertex_index(body.vertex(v));
    if (i >= 0 && !input.vertex_name(i).empty() && input.vertex_name(i) != own) {
      names.renames[own] = {input.vertex_name(i), -1};
    }
  }
  return names;
}

std::string with_unsplit_names(const std::string& text, const UnsplitNames& names) {
  return names.renames.empty() ? text : renamed(text, names, false);
}

ShapePtr with_unsplit_names(const ShapePtr& result, const UnsplitNames& names, const std::string& feature) {
  if (!result || names.renames.empty()) {
    return result;
  }
  const std::string made = feature + ":";
  std::vector<Shape::NamedFace> faces;
  for (int f = 0; f < result->face_count(); ++f) {
    NameList own = result->face_names(f);
    for (std::string& name : own) {
      // (Only the faces this fillet made: the others keep the input's
      // names, which may hold the same text for other edges.)
      if (name.compare(0, made.size(), made) == 0) {
        name = renamed(name, names, true);
      }
    }
    faces.push_back({result->face(f), own});
  }
  ShapePtr named = std::make_shared<Shape>(result->occt(), faces);
  named->copy_notes(*result);
  if (result->checked_valid().value_or(false)) {
    named->set_checked_valid(true);
  }
  return named;
}

} // namespace mitcad::geometry::detail
