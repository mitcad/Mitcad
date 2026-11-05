// SPDX-License-Identifier: MIT
#include "mitcad/geometry/sketch_ref.hpp"

#include <algorithm>
#include <array>
#include <map>
#include <set>
#include <stdexcept>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <GeomConvert_CompCurveToBSplineCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

occ::handle<Geom_BSplineCurve> as_bspline(const BRepAdaptor_Curve& curve) {
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  occ::handle<Geom_BSplineCurve> spline;
  if (curve.GetType() == GeomAbs_BSplineCurve) {
    spline = occ::handle<Geom_BSplineCurve>::DownCast(curve.BSpline()->Copy());
  } else if (curve.GetType() == GeomAbs_BezierCurve) {
    spline = GeomConvert::CurveToBSplineCurve(curve.Bezier());
  } else {
    GeomConvert_ApproxCurve approx(curve.Trim(first, last, 1e-9), 1e-6, GeomAbs_C2, 64, 9);
    if (!approx.HasResult()) {
      throw std::runtime_error("an edge curve could not be converted");
    }
    return approx.Curve();
  }
  if (spline->IsPeriodic()) {
    spline->SetNotPeriodic();
  }
  if (first > spline->FirstParameter() + 1e-12 || last < spline->LastParameter() - 1e-12) {
    spline->Segment(first, last);
  }
  return spline;
}

ModelCurve curve_of(const occ::handle<Geom_BSplineCurve>& spline) {
  ModelCurve out;
  out.kind = ModelCurveKind::BSpline;
  out.degree = spline->Degree();
  for (int i = 1; i <= spline->NbPoles(); ++i) {
    out.poles.push_back(spline->Pole(i));
  }
  if (spline->IsRational()) {
    for (int i = 1; i <= spline->NbPoles(); ++i) {
      out.weights.push_back(spline->Weight(i));
    }
  }
  const auto& knots = spline->KnotSequence();
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    out.knots.push_back(knots(i));
  }
  return out;
}

ModelCurve curve_of(const TopoDS_Edge& edge) {
  ModelCurve out;
  BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  switch (curve.GetType()) {
  case GeomAbs_Line:
    out.kind = ModelCurveKind::Line;
    out.start = curve.Value(first);
    out.end = curve.Value(last);
    return out;
  case GeomAbs_Circle:
  case GeomAbs_Ellipse: {
    out.kind = ModelCurveKind::Conic;
    gp_Ax2 position;
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Circ circle = curve.Circle();
      position = circle.Position();
      out.major = out.minor = circle.Radius();
    } else {
      const gp_Elips ellipse = curve.Ellipse();
      position = ellipse.Position();
      out.major = ellipse.MajorRadius();
      out.minor = ellipse.MinorRadius();
    }
    out.center = position.Location();
    out.normal = position.Direction();
    out.x_axis = position.XDirection();
    out.first = first;
    out.last = last;
    out.closed = last - first >= kTwoPi - 1e-9;
    return out;
  }
  default:
    return curve_of(as_bspline(curve));
  }
}

bool starts_with(const std::string& text, const char* prefix) { return text.rfind(prefix, 0) == 0; }

// The non-degenerate boundary edges of face f, each once.
std::vector<int> face_edges(const Shape& shape, int f) {
  std::vector<int> edges;
  for (TopExp_Explorer it(shape.face(f), TopAbs_EDGE); it.More(); it.Next()) {
    const int e = shape.edge_index(it.Current());
    bool seen = false;
    for (const int known : edges) {
      seen = seen || known == e;
    }
    if (e >= 0 && !seen && !BRep_Tool::Degenerated(TopoDS::Edge(it.Current()))) {
      edges.push_back(e);
    }
  }
  return edges;
}

// How far apart (mm) the ends of two section pieces may be and still meet:
// each face is cut by itself, so the pieces of neighbouring faces end
// where each intersection put the common edge.
constexpr double kMeet = 1e-4;

// A curve where the plane cuts a face (or faces: an edge in the plane).
struct Piece {
  TopoDS_Edge edge;
  std::vector<int> faces;
  gp_Pnt first;
  gp_Pnt last;
  bool spline = false;
};

bool beside(const gp_Pnt& a, const gp_Pnt& b) { return a.Distance(b) <= kMeet; }

double side_of(const gp_Pln& plane, const gp_Pnt& p) {
  return gp_Vec(plane.Location(), p).Dot(gp_Vec(plane.Axis().Direction()));
}

// Whether the face's box reaches both sides of the plane (or touches it).
bool straddles(const TopoDS_Face& face, const gp_Pln& plane) {
  Bnd_Box box;
  BRepBndLib::Add(face, box);
  if (box.IsVoid()) {
    return false;
  }
  double x[2];
  double y[2];
  double z[2];
  box.Get(x[0], y[0], z[0], x[1], y[1], z[1]);
  bool below = false;
  bool above = false;
  for (int i = 0; i < 8; ++i) {
    const double d = side_of(plane, gp_Pnt(x[i & 1], y[(i >> 1) & 1], z[(i >> 2) & 1]));
    below = below || d <= kMeet;
    above = above || d >= -kMeet;
  }
  return below && above;
}

// The pieces of the plane's section of the faces, each once.
std::vector<Piece> section_pieces(const Shape& shape, const std::vector<int>& faces, const gp_Pln& plane) {
  std::vector<Piece> pieces;
  for (const int f : faces) {
    const TopoDS_Face& face = shape.face(f);
    if (!straddles(face, plane)) {
      continue;
    }
    // OCCT's section reads the face's surface (mitcad#399).
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      throw std::invalid_argument("a mesh body's face (triangles without a surface) has no section curves");
    }
    BRepAlgoAPI_Section section(face, plane, false);
    section.Approximation(true);
    section.ComputePCurveOn1(false);
    section.Build();
    if (!section.IsDone()) {
      throw std::runtime_error("the plane's section of a face failed");
    }
    for (TopExp_Explorer it(section.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      BRepAdaptor_Curve curve(edge);
      const double t0 = curve.FirstParameter();
      const double t1 = curve.LastParameter();
      Piece piece;
      piece.edge = edge;
      piece.faces.push_back(f);
      piece.first = curve.Value(t0);
      piece.last = curve.Value(t1);
      const gp_Pnt middle = curve.Value((t0 + t1) / 2.0);
      if (beside(piece.first, piece.last) && beside(piece.first, middle)) {
        continue;
      }
      const GeomAbs_CurveType type = curve.GetType();
      piece.spline = type != GeomAbs_Line && type != GeomAbs_Circle && type != GeomAbs_Ellipse;
      // An edge in the plane is cut from both its faces: one piece.
      bool same = false;
      for (Piece& known : pieces) {
        const bool ends = (beside(known.first, piece.first) && beside(known.last, piece.last)) ||
                          (beside(known.first, piece.last) && beside(known.last, piece.first));
        if (!ends) {
          continue;
        }
        BRepAdaptor_Curve other(known.edge);
        if (beside(other.Value((other.FirstParameter() + other.LastParameter()) / 2.0), middle)) {
          known.faces.push_back(f);
          same = true;
          break;
        }
      }
      if (!same) {
        pieces.push_back(piece);
      }
    }
  }
  return pieces;
}

// A piece's curve as a B-spline, reversed when asked.
occ::handle<Geom_BSplineCurve> spline_of(const Piece& piece, bool reversed) {
  occ::handle<Geom_BSplineCurve> spline = as_bspline(BRepAdaptor_Curve(piece.edge));
  if (reversed) {
    spline->Reverse();
  }
  return spline;
}

// Where a piece goes on: the other piece at one of its ends and whether
// that piece meets it with its last end.
struct Neighbour {
  int piece = -1;
  bool at_last = false;
};

// The B-spline pieces in chains: per chain its pieces in order, each with
// whether it runs reversed. A B-spline piece goes on into another where
// those two are the only pieces that end there.
std::vector<std::vector<std::pair<int, bool>>> chains(const std::vector<Piece>& pieces) {
  const int n = static_cast<int>(pieces.size());
  const auto meeting = [&](const gp_Pnt& p) {
    int count = 0;
    for (const Piece& q : pieces) {
      count += (beside(p, q.first) ? 1 : 0) + (beside(p, q.last) ? 1 : 0);
    }
    return count;
  };
  // [0]: at the piece's first end, [1]: at its last.
  std::vector<std::array<Neighbour, 2>> next(static_cast<std::size_t>(n));
  for (int i = 0; i < n; ++i) {
    const Piece& piece = pieces[static_cast<std::size_t>(i)];
    if (!piece.spline) {
      continue;
    }
    for (int end = 0; end < 2; ++end) {
      const gp_Pnt& p = end == 0 ? piece.first : piece.last;
      if (meeting(p) != 2) {
        continue;
      }
      for (int j = 0; j < n; ++j) {
        const Piece& other = pieces[static_cast<std::size_t>(j)];
        if (j == i || !other.spline) {
          continue;
        }
        if (beside(p, other.first) || beside(p, other.last)) {
          next[static_cast<std::size_t>(i)][static_cast<std::size_t>(end)] = {j, beside(p, other.last)};
          break;
        }
      }
    }
  }
  std::vector<std::vector<std::pair<int, bool>>> out;
  std::vector<bool> used(static_cast<std::size_t>(n), false);
  for (int i = 0; i < n; ++i) {
    if (!pieces[static_cast<std::size_t>(i)].spline || used[static_cast<std::size_t>(i)]) {
      continue;
    }
    // Back to the chain's start (round a closed chain once).
    int current = i;
    bool reversed = false;
    for (int step = 0; step < n; ++step) {
      const Neighbour behind = next[static_cast<std::size_t>(current)][reversed ? 1U : 0U];
      if (behind.piece < 0 || behind.piece == i || used[static_cast<std::size_t>(behind.piece)]) {
        break;
      }
      reversed = !behind.at_last;
      current = behind.piece;
    }
    std::vector<std::pair<int, bool>> chain;
    for (int step = 0; step < n; ++step) {
      chain.emplace_back(current, reversed);
      used[static_cast<std::size_t>(current)] = true;
      const Neighbour ahead = next[static_cast<std::size_t>(current)][reversed ? 0U : 1U];
      if (ahead.piece < 0 || used[static_cast<std::size_t>(ahead.piece)]) {
        break;
      }
      reversed = ahead.at_last;
      current = ahead.piece;
    }
    out.push_back(chain);
  }
  return out;
}

} // namespace

std::vector<ModelCurve> curves_of(const Shape& shape, const std::string& name) {
  return detail::run("model curves", [&] {
    std::vector<ModelCurve> curves;
    if (starts_with(name, "V{")) {
      for (const int i : shape.find_vertices(name)) {
        ModelCurve point;
        point.kind = ModelCurveKind::Point;
        point.start = BRep_Tool::Pnt(shape.vertex(i));
        curves.push_back(point);
      }
      return curves;
    }
    std::vector<int> edges;
    if (starts_with(name, "E{")) {
      edges = shape.find_edges(name);
    } else {
      for (const int f : shape.find_faces(name)) {
        for (const int e : face_edges(shape, f)) {
          bool seen = false;
          for (const int known : edges) {
            seen = seen || known == e;
          }
          if (!seen) {
            edges.push_back(e);
          }
        }
      }
    }
    for (const int e : edges) {
      curves.push_back(curve_of(shape.edge(e)));
    }
    return curves;
  });
}

std::vector<NamedBoundaryCurve> face_boundary(const Shape& shape, const std::string& face) {
  return detail::run("face boundary", [&] {
    std::map<std::string, int> sourceImages;
    if (shape.has_open_edge_names()) {
      for (int e = 0; e < shape.edge_count(); ++e) {
        const auto adjacent = shape.edge_faces(e);
        if (shape.edge_name(e).empty() && adjacent[0] >= 0 && adjacent[1] < 0) {
          for (const std::string& source : shape.open_edge_names(e)) ++sourceImages[source];
        }
      }
    }
    std::map<int, int> uses;
    for (const int f : shape.find_faces(face)) {
      if (BRepAdaptor_Surface(shape.face(f)).GetType() != GeomAbs_Plane) {
        throw std::invalid_argument("face " + face + " is not planar");
      }
      for (const int e : face_edges(shape, f)) {
        ++uses[e];
      }
    }
    std::vector<NamedBoundaryCurve> result;
    std::set<std::string> names;
    result.reserve(uses.size());
    for (const auto& [e, count] : uses) {
      // The edge between two selected face pieces is internal to their
      // union, not a boundary. Selecting just one piece keeps that edge.
      if (count != 1) {
        continue;
      }
      std::string name = shape.edge_name(e);
      if (name.empty()) {
        const NameList& sources = shape.open_edge_names(e);
        if (sources.size() > 1) {
          throw std::invalid_argument("ambiguous profile boundary: an edge has multiple source profiles");
        }
        const auto adjacent = shape.edge_faces(e);
        if (sources.empty() || adjacent[0] < 0 || adjacent[1] >= 0) {
          throw std::invalid_argument("the face boundary has an unnamed edge without source-profile provenance");
        }
        name = sources.front();
        if (sourceImages[name] != 1) {
          throw std::invalid_argument("ambiguous profile boundary: a source profile edge has multiple images");
        }
      }
      if (!names.insert(name).second) {
        throw std::invalid_argument("ambiguous profile boundary: a source profile edge has multiple images");
      }
      result.push_back({name, curve_of(shape.edge(e))});
    }
    std::sort(result.begin(), result.end(), [](const NamedBoundaryCurve& a, const NamedBoundaryCurve& b) {
      return a.name < b.name;
    });
    return result;
  });
}

IndexedElement indexed_element(const Shape& shape, char kind, int index) {
  return detail::run("element by index", [&] {
    IndexedElement out;
    if (index < 0) {
      return out;
    }
    switch (kind) {
    case 'F':
      if (index < shape.face_count()) {
        out.found = true;
        const NameList& names = shape.face_names(index);
        if (!names.empty()) {
          out.name = names.front();
        }
        for (const int e : face_edges(shape, index)) {
          out.curves.push_back(curve_of(shape.edge(e)));
        }
      }
      break;
    case 'E':
      if (index < shape.edge_count()) {
        out.found = true;
        out.name = shape.edge_name(index);
        if (!BRep_Tool::Degenerated(shape.edge(index))) {
          out.curves.push_back(curve_of(shape.edge(index)));
        }
      }
      break;
    case 'V':
      if (index < shape.vertex_count()) {
        out.found = true;
        out.name = shape.vertex_name(index);
        ModelCurve point;
        point.kind = ModelCurveKind::Point;
        point.start = BRep_Tool::Pnt(shape.vertex(index));
        out.curves.push_back(point);
      }
      break;
    default:
      throw std::invalid_argument(std::string("an element kind F, E or V, not ") + kind);
    }
    return out;
  });
}

std::vector<SectionCurve> section_curves(const Shape& shape, const std::string& name, const gp_Pnt& origin,
                                         const gp_Dir& normal) {
  return detail::run("section curves", [&] {
    std::vector<int> faces;
    if (name.empty()) {
      for (int f = 0; f < shape.face_count(); ++f) {
        faces.push_back(f);
      }
    } else {
      faces = shape.find_faces(name);
    }
    const std::vector<Piece> pieces = section_pieces(shape, faces, gp_Pln(origin, normal));
    const auto names = [&](const std::vector<int>& of, std::vector<std::string>& into) {
      for (const int f : of) {
        const NameList& found = shape.face_names(f);
        if (!found.empty() && std::find(into.begin(), into.end(), found.front()) == into.end()) {
          into.push_back(found.front());
        }
      }
    };
    // Each chain where its first piece is.
    const std::vector<std::vector<std::pair<int, bool>>> joined = chains(pieces);
    std::vector<int> chain_at(pieces.size(), -1);
    for (std::size_t c = 0; c < joined.size(); ++c) {
      for (const auto& step : joined[c]) {
        chain_at[static_cast<std::size_t>(step.first)] = static_cast<int>(c);
      }
    }
    std::vector<SectionCurve> out;
    std::vector<bool> done(joined.size(), false);
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      const int c = chain_at[i];
      if (c < 0) {
        SectionCurve curve;
        curve.curve = curve_of(pieces[i].edge);
        names(pieces[i].faces, curve.faces);
        out.push_back(curve);
        continue;
      }
      if (done[static_cast<std::size_t>(c)]) {
        continue;
      }
      done[static_cast<std::size_t>(c)] = true;
      const std::vector<std::pair<int, bool>>& chain = joined[static_cast<std::size_t>(c)];
      SectionCurve curve;
      if (chain.size() == 1) {
        curve.curve = curve_of(pieces[static_cast<std::size_t>(chain[0].first)].edge);
      } else {
        GeomConvert_CompCurveToBSplineCurve concat(
            spline_of(pieces[static_cast<std::size_t>(chain[0].first)], chain[0].second));
        for (std::size_t k = 1; k < chain.size(); ++k) {
          const Piece& piece = pieces[static_cast<std::size_t>(chain[k].first)];
          if (!concat.Add(spline_of(piece, chain[k].second), kMeet, true)) {
            throw std::runtime_error("the section's pieces could not be joined");
          }
        }
        curve.curve = curve_of(concat.BSplineCurve());
      }
      for (const auto& step : chain) {
        names(pieces[static_cast<std::size_t>(step.first)].faces, curve.faces);
      }
      out.push_back(curve);
    }
    return out;
  });
}

std::vector<gp_Pnt> section_points(const Shape& shape, const std::string& name, const gp_Pnt& origin,
                                   const gp_Dir& normal) {
  return detail::run("section points", [&] {
    const gp_Pln plane(origin, normal);
    std::vector<gp_Pnt> points;
    for (const int e : shape.find_edges(name)) {
      const TopoDS_Edge& edge = shape.edge(e);
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      // Whether the edge's box reaches both sides of the plane.
      Bnd_Box box;
      BRepBndLib::Add(edge, box);
      if (box.IsVoid()) {
        continue;
      }
      double x[2];
      double y[2];
      double z[2];
      box.Get(x[0], y[0], z[0], x[1], y[1], z[1]);
      bool below = false;
      bool above = false;
      for (int i = 0; i < 8; ++i) {
        const double d = side_of(plane, gp_Pnt(x[i & 1], y[(i >> 1) & 1], z[(i >> 2) & 1]));
        below = below || d <= kMeet;
        above = above || d >= -kMeet;
      }
      if (!below || !above) {
        continue;
      }
      // The section's vertices: crossings, and the ends of what lies in
      // the plane.
      BRepAlgoAPI_Section section(edge, plane, false);
      section.Build();
      if (!section.IsDone()) {
        throw std::runtime_error("the plane's section of an edge failed");
      }
      for (TopExp_Explorer it(section.Shape(), TopAbs_VERTEX); it.More(); it.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(it.Current()));
        bool seen = false;
        for (const gp_Pnt& known : points) {
          seen = seen || beside(known, p);
        }
        if (!seen) {
          points.push_back(p);
        }
      }
    }
    return points;
  });
}

ModelCurve edge_curve(const TopoDS_Edge& edge) { return curve_of(edge); }

} // namespace mitcad::geometry
