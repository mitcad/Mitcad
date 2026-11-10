// SPDX-License-Identifier: MIT
// Emboss by projection (mitcad#1091; emboss.hpp, projected_emboss): the
// profiles of a sketch projected onto any face along a direction and grown
// along the face's normals.
//
// Each profile region is extruded far both ways along the direction into a
// prism, and the prism's common part with the face is the region projected
// onto it: one piece where the direction's lines meet the face once, more
// where they meet it again (a sphere's far side). The piece nearest ahead
// of the sketch plane is taken, else the one nearest behind it; it must
// carry the region's whole area across the direction (the flux of the
// direction through it), or the region lies partly off the face. OCCT's
// thickening offset then grows the piece into a solid along the face's
// normals: the piece, its offset, and walls along the normals on its free
// edges.

#include "mitcad/geometry/emboss.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <map>
#include <numeric>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BOPTools_AlgoTools3D.hxx>
#include <TopoDS_Wire.hxx>
#include <TopoDS_Solid.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <NCollection_Sequence.hxx>
#include <NCollection_HSequence.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepLib.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepOffset_MakeSimpleOffset.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <IntTools_Context.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>

#include "emboss_projection.hpp"
#include "emboss_taper.hpp"
#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace detail {

namespace {

// The face's unit normal at `uv` (its orientation applied) and the point
// there; false where the surface is degenerate.
bool normal_at(const TopoDS_Face& face, const BRepAdaptor_Surface& surface, const gp_Pnt2d& uv, gp_Pnt& point,
               gp_Vec& normal) {
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), point, du, dv);
  normal = du.Crossed(dv);
  if (normal.Magnitude() < 1.0e-12) {
    return false;
  }
  normal.Normalize();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return true;
}

} // namespace

bool inner_point(const TopoDS_Face& face, gp_Pnt& point, gp_Vec& normal) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  BRepTopAdaptor_FClass2d inside(face, Precision::PConfusion());
  const BRepAdaptor_Surface surface(face);
  for (int n = 1; n <= 9; n += 2) {
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * (i + 0.5) / n, v0 + (v1 - v0) * (j + 0.5) / n);
        if (inside.Perform(uv) == TopAbs_IN && normal_at(face, surface, uv, point, normal)) {
          return true;
        }
      }
    }
  }
  // A sliver between the grid's points (a thin strip along the face's
  // edges): OCCT's search, which hatches the face across its parameters
  // (mitcad#1154).
  gp_Pnt found;
  gp_Pnt2d uv;
  const occ::handle<IntTools_Context> context = new IntTools_Context();
  if (BOPTools_AlgoTools3D::PointInFace(face, found, uv, context) != 0) {
    return false;
  }
  return normal_at(face, surface, uv, point, normal);
}

} // namespace detail

namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The offset's tolerance, as Thicken's (surfaces.cpp).
constexpr double kTolerance = 1.0e-6;
// How closely a piece's flux must match the region's area across the
// direction (relative): a region partly off the face loses far more.
constexpr double kCovered = 1.0e-3;

// The region keys of a profile face, for messages.
std::string regions_of(const detail::SweepFace& profile) {
  std::string text;
  for (const std::string& region : profile.regions) {
    text += (text.empty() ? "" : ", ") + region;
  }
  return text;
}

// An error about the profile `profile` (mitcad#1612): the person reads "the
// profile" and `said`, agents "the profile <its region keys>" and `named`;
// the face `face` is painted where one is given.
std::string profile_error(const detail::SweepFace& profile, const std::string& said, const std::string& named,
                          const std::string& face = std::string()) {
  std::vector<ErrorPlace> places;
  if (!face.empty()) {
    places.push_back(face_place(face));
  }
  return placed_error("the profile" + said, "the profile " + regions_of(profile) + named, places);
}

// The flux of the constant field `d` through the faces, their orientations
// applied: by Stokes' theorem half the circulation of d x r round their
// edges (d = curl(d x r / 2)). An edge a face holds twice (a seam) cancels;
// a degenerate one adds nothing. 4-point Gauss-Legendre on 64 pieces of
// each edge.
double flux(const std::vector<TopoDS_Face>& faces, const gp_Vec& d, const gp_Pnt& origin) {
  static constexpr double kNodes[4] = {-0.8611363115940526, -0.3399810435848563, 0.3399810435848563,
                                       0.8611363115940526};
  static constexpr double kWeights[4] = {0.3478548451374538, 0.6521451548625461, 0.6521451548625461,
                                         0.3478548451374538};
  constexpr int kPieces = 64;
  double sum = 0.0;
  for (const TopoDS_Face& face : faces) {
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      const TopAbs_Orientation orientation = edge.Orientation();
      if (BRep_Tool::Degenerated(edge) || (orientation != TopAbs_FORWARD && orientation != TopAbs_REVERSED)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      const double first = curve.FirstParameter();
      const double h = (curve.LastParameter() - first) / kPieces;
      double along = 0.0;
      for (int i = 0; i < kPieces; ++i) {
        for (int k = 0; k < 4; ++k) {
          gp_Pnt p;
          gp_Vec dp;
          curve.D1(first + h * (i + (kNodes[k] + 1.0) / 2.0), p, dp);
          along += kWeights[k] * d.Crossed(gp_Vec(origin, p)).Dot(dp);
        }
      }
      along *= h / 2.0;
      sum += orientation == TopAbs_REVERSED ? -along : along;
    }
  }
  return sum / 2.0;
}

// The faces of the shape in groups joined by the edges they share.
std::vector<std::vector<TopoDS_Face>> connected(const TopoDS_Shape& shape) {
  ShapeMap index;
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const int before = index.Extent();
    index.Add(f.Current());
    if (index.Extent() > before) {
      faces.push_back(TopoDS::Face(f.Current()));
    }
  }
  std::vector<int> parent(faces.size());
  std::iota(parent.begin(), parent.end(), 0);
  const auto root = [&parent](int i) {
    while (parent[static_cast<std::size_t>(i)] != i) {
      i = parent[static_cast<std::size_t>(i)];
    }
    return i;
  };
  Ancestors ancestors;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, ancestors);
  for (int e = 1; e <= ancestors.Extent(); ++e) {
    int first = -1;
    for (const TopoDS_Shape& face : ancestors.FindFromIndex(e)) {
      const int i = index.FindIndex(face) - 1;
      if (i < 0) {
        continue;
      }
      const int r = root(i);
      if (first < 0) {
        first = r;
      } else if (r != first) {
        parent[static_cast<std::size_t>(r)] = first;
      }
    }
  }
  std::vector<std::vector<TopoDS_Face>> groups;
  std::vector<int> group_of(faces.size(), -1);
  for (std::size_t i = 0; i < faces.size(); ++i) {
    const std::size_t r = static_cast<std::size_t>(root(static_cast<int>(i)));
    if (group_of[r] < 0) {
      group_of[r] = static_cast<int>(groups.size());
      groups.emplace_back();
    }
    groups[static_cast<std::size_t>(group_of[r])].push_back(faces[i]);
  }
  return groups;
}

// A piece of the face a region's prism holds: its faces, the flux of the
// direction through them and how far ahead of the sketch plane along the
// direction their centre lies: the distance along the direction's line
// through the centre from where it meets the plane (behind it negative).
struct Piece {
  std::vector<TopoDS_Face> faces;
  double flux = 0.0;
  double ahead = 0.0;
};

// The region projected along `along` onto the target's faces: the piece of
// them it covers. Throws when it misses them or lies partly off them.
std::vector<TopoDS_Face> projected_piece(const TopoDS_Shape& target, const detail::SweepFace& profile,
                                         const Frame& frame, const gp_Vec& along, double reach,
                                         const std::string& face_ref) {
  gp_Trsf back;
  back.SetTranslation(along * -reach);
  const ShapePtr prism = detail::named_prism(detail::transformed(profile, back), along * (2.0 * reach), {}, {});
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> objects;
  objects.Append(target);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(prism->occt());
  common.SetArguments(objects);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  detail::build(common);
  if (!common.IsDone() || common.HasErrors()) {
    throw std::runtime_error(profile_error(profile, " could not be projected onto the face",
                                           " could not be projected onto face " + face_ref, face_ref));
  }
  // The sketch plane's normal; the direction crosses the plane (one in it
  // is refused), so along . normal is not zero.
  const gp_Vec normal(frame.normal());
  std::vector<Piece> pieces;
  for (std::vector<TopoDS_Face>& faces : connected(common.Shape())) {
    Piece piece;
    piece.faces = std::move(faces);
    piece.flux = flux(piece.faces, along, frame.origin);
    GProp_GProps props;
    for (const TopoDS_Face& face : piece.faces) {
      GProp_GProps own;
      BRepGProp::SurfaceProperties(face, own);
      props.Add(own);
    }
    // Not (centre - origin) . along: that is the distance from the plane
    // only when the direction is the plane's normal, and a slanted
    // direction would take a far piece for a near one (mitcad#1154).
    piece.ahead = gp_Vec(frame.origin, props.CentreOfMass()).Dot(normal) / along.Dot(normal);
    pieces.push_back(std::move(piece));
  }
  if (pieces.empty()) {
    throw std::invalid_argument(profile_error(profile, " misses the face along the projection",
                                              " misses face " + face_ref + " along the projection", face_ref));
  }
  // Ahead of the sketch plane, the nearest first; then behind it, the
  // nearest first.
  std::sort(pieces.begin(), pieces.end(), [](const Piece& a, const Piece& b) {
    const bool a_ahead = a.ahead >= 0.0;
    const bool b_ahead = b.ahead >= 0.0;
    if (a_ahead != b_ahead) {
      return a_ahead;
    }
    return a_ahead ? a.ahead < b.ahead : a.ahead > b.ahead;
  });
  GProp_GProps area;
  BRepGProp::SurfaceProperties(profile.face, area);
  const double across = area.Mass() * std::abs(along.Dot(normal));
  const Piece& piece = pieces.front();
  if (std::abs(std::abs(piece.flux) - across) > kCovered * across) {
    const std::string off = " along the projection (or past where the face turns away from it)";
    throw std::invalid_argument(profile_error(profile, " falls partly off the face" + off,
                                              " falls partly off face " + face_ref + off, face_ref));
  }
  return piece.faces;
}

// The names of the walls over the region's edges: a point of the face is
// taken back along the direction onto the sketch plane, and the profile
// edge nearest it names the wall there.
class Segments {
public:
  Segments(const detail::SweepFace& profile, const Frame& frame, const gp_Vec& along)
      : m_profile(profile), m_origin(frame.origin), m_normal(frame.normal()), m_along(along) {}

  std::string segment_at(const gp_Pnt& point) const {
    const double back = gp_Vec(point, m_origin).Dot(m_normal) / m_along.Dot(m_normal);
    const TopoDS_Vertex flat = BRepBuilderAPI_MakeVertex(point.Translated(m_along * back)).Vertex();
    double best = std::numeric_limits<double>::infinity();
    std::string name;
    for (const auto& [edge, segment] : m_profile.edges) {
      BRepExtrema_DistShapeShape distance(flat, edge);
      if (distance.IsDone() && distance.Value() < best) {
        best = distance.Value();
        name = segment;
      }
    }
    return name;
  }

private:
  const detail::SweepFace& m_profile;
  gp_Pnt m_origin;
  gp_Vec m_normal;
  gp_Vec m_along;
};

// The distance from a point to the surface of `reference` and the foot of
// it there; infinite when the projection finds none.
double distance_to(const TopoDS_Face& reference, const gp_Pnt& point, gp_Pnt& foot) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(reference);
  if (surface.IsNull()) {
    return std::numeric_limits<double>::infinity();
  }
  GeomAPI_ProjectPointOnSurf projection(point, surface);
  if (projection.NbPoints() == 0) {
    return std::numeric_limits<double>::infinity();
  }
  foot = projection.NearestPoint();
  return projection.LowerDistance();
}

// Where a face of the grown solid lies: on the piece (0), at the depth
// over it (1), or between, a wall (2); by a point inside it. `foot` is the
// point of the piece nearest it, empty when the face has no point found
// inside it or no face of the piece projects it (a wall then, unnamed).
int place_of(const TopoDS_Face& face, const std::vector<TopoDS_Face>& piece, double depth, double tolerance,
             std::optional<gp_Pnt>& foot) {
  foot.reset();
  gp_Pnt p;
  gp_Vec n;
  if (!detail::inner_point(face, p, n)) {
    return 2;
  }
  double nearest = std::numeric_limits<double>::infinity();
  for (const TopoDS_Face& reference : piece) {
    gp_Pnt own;
    const double d = distance_to(reference, p, own);
    if (d < nearest) {
      nearest = d;
      foot = own;
    }
  }
  if (!foot) {
    return 2;
  }
  if (nearest <= tolerance) {
    return 0;
  }
  return std::abs(nearest - std::abs(depth)) <= tolerance ? 1 : 2;
}

// The free edges of the faces: those only one of them holds, not seams
// and not degenerate.
std::vector<TopoDS_Edge> boundary_of(const std::vector<TopoDS_Face>& faces) {
  BRep_Builder builder;
  TopoDS_Compound all;
  builder.MakeCompound(all);
  for (const TopoDS_Face& face : faces) {
    builder.Add(all, face);
  }
  Ancestors ancestors;
  TopExp::MapShapesAndAncestors(all, TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<TopoDS_Edge> result;
  for (int e = 1; e <= ancestors.Extent(); ++e) {
    const TopoDS_Edge& edge = TopoDS::Edge(ancestors.FindKey(e));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    std::vector<TopoDS_Shape> holders;
    bool seam = false;
    for (const TopoDS_Shape& face : ancestors.FindFromIndex(e)) {
      seam = seam || BRep_Tool::IsClosed(edge, TopoDS::Face(face));
      if (std::none_of(holders.begin(), holders.end(), [&](const TopoDS_Shape& f) { return f.IsSame(face); })) {
        holders.push_back(face);
      }
    }
    if (!seam && holders.size() == 1) {
      result.push_back(edge);
    }
  }
  return result;
}

// The piece grown by `depth` along its faces' normals into a solid named
// after `feature`: start(<region>) on the piece, end(<region>) its offset,
// side(<segment>) the walls on its free edges.
ShapePtr grown(const std::string& feature, const detail::SweepFace& profile, const std::vector<TopoDS_Face>& piece,
               double depth, const Segments& segments) {
  BRep_Builder builder;
  TopoDS_Shape input;
  if (piece.size() == 1) {
    input = piece.front();
  } else {
    TopoDS_Shell shell;
    builder.MakeShell(shell);
    for (const TopoDS_Face& face : piece) {
      builder.Add(shell, face);
    }
    input = shell;
  }
  // The free edges of the piece and the walls' names over them.
  std::vector<std::pair<TopoDS_Edge, std::string>> free_edges;
  for (const TopoDS_Edge& edge : boundary_of(piece)) {
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
    free_edges.emplace_back(edge, segments.segment_at(middle));
  }
  gp_Pnt inside;
  gp_Vec normal;
  if (!detail::inner_point(piece.front(), inside, normal)) {
    throw std::runtime_error(profile_error(profile, " projected onto the face has no inside",
                                           " projected onto the face has no inside"));
  }
  // Halfway through the solid asked for, and where its far side lies.
  const gp_Pnt middle = inside.Translated(normal * (depth / 2.0));
  const gp_Pnt far_point = inside.Translated(normal * depth);
  const double tolerance = std::min(1.0e-4, std::abs(depth) / 4.0);
  const double side_tolerance = 1.0e-3 + 1.0e-3 * std::abs(depth);
  const NameList start = detail::cap_names(feature, "start", profile);
  const NameList end = detail::cap_names(feature, "end", profile);
  // OCCT offsets along the surfaces' normals, which a face's orientation
  // may reverse: the other way when the solid lies on the wrong side.
  for (const double d : {depth, -depth}) {
    // OCCT's offset writes into its input.
    const detail::InputCopy copy(input);
    BRepOffset_MakeOffset maker;
    maker.Initialize(copy.shape(), d, kTolerance, BRepOffset_Skin, false, false, GeomAbs_Intersection, true);
    detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
    TopoDS_Shape result;
    // The offset's history, when OCCT's thickening made it.
    occ::handle<BRepTools_History> history;
    if (maker.IsDone() && TopExp_Explorer(maker.Shape(), TopAbs_SOLID).More()) {
      result = maker.Shape();
      NCollection_List<TopoDS_Shape> arguments;
      arguments.Append(copy.shape());
      history = new BRepTools_History(arguments, maker);
    } else {
      // Where the thickening fails (a free-form face it cannot join to its
      // walls), OCCT's simple offset: the faces offset, and walls between
      // their free edges and the edges' offsets.
      const detail::InputCopy again(input);
      BRepOffset_MakeSimpleOffset simple(again.shape(), d);
      simple.SetBuildSolidFlag(true);
      simple.SetTolerance(kTolerance);
      simple.Perform();
      if (simple.IsDone() && TopExp_Explorer(simple.GetResultShape(), TopAbs_SOLID).More()) {
        result = simple.GetResultShape();
        // Its solid comes inside out when the offset goes against the
        // faces' normals: turned over then (its volume is negative).
        GProp_GProps props;
        BRepGProp::VolumeProperties(result, props);
        if (props.Mass() < 0.0) {
          result.Reverse();
        }
      }
    }
    if (result.IsNull()) {
      const std::string why = " projected onto the face could not be grown along its normals (the face may "
                              "curve more tightly than the depth)";
      throw std::runtime_error(profile_error(profile, why, why));
    }
    detail::FaceNamer namer(result);
    // The side the solid lies on: the far face of the piece's first face
    // passes through the point `depth` along its normal, else (no far face
    // in the history) the solid holds the point halfway there.
    int side = 0; // 1 right, -1 wrong, 0 not known yet
    // The piece's images: on it the start, over it the end.
    for (const TopoDS_Face& face : piece) {
      if (history.IsNull()) {
        break;
      }
      const TopoDS_Shape original = copy.of(face);
      std::vector<TopoDS_Shape> images;
      for (const TopoDS_Shape& image : history->Generated(original)) {
        images.push_back(image);
      }
      for (const TopoDS_Shape& image : history->Modified(original)) {
        images.push_back(image);
      }
      if (namer.contains(original)) {
        images.push_back(original);
      }
      for (const TopoDS_Shape& image : images) {
        if (image.ShapeType() != TopAbs_FACE || !namer.contains(image) || namer.named(image)) {
          continue;
        }
        std::optional<gp_Pnt> foot;
        const int place = place_of(TopoDS::Face(image), piece, depth, tolerance, foot);
        if (place == 0) {
          namer.add(image, start, -1);
        } else if (place == 1) {
          namer.add(image, end, -1);
          if (side == 0 && face.IsSame(piece.front())) {
            gp_Pnt on_far;
            side = distance_to(TopoDS::Face(image), far_point, on_far) <= side_tolerance ? 1 : -1;
          }
        }
      }
    }
    // Without the history, the far face of a piece of one face: a face of
    // the solid on that face's offset.
    if (side == 0 && history.IsNull() && piece.size() == 1) {
      for (TopExp_Explorer f(result, TopAbs_FACE); f.More() && side == 0; f.Next()) {
        std::optional<gp_Pnt> foot;
        if (place_of(TopoDS::Face(f.Current()), piece, depth, tolerance, foot) == 1) {
          gp_Pnt on_far;
          side = distance_to(TopoDS::Face(f.Current()), far_point, on_far) <= side_tolerance ? 1 : -1;
        }
      }
    }
    if (side == 0) {
      const BRepClass3d_SolidClassifier classifier(result, middle, Precision::Confusion());
      side = classifier.State() == TopAbs_IN ? 1 : -1;
    }
    if (side < 0) {
      continue;
    }
    // The walls, from the free edges.
    for (const auto& [edge, name] : free_edges) {
      if (history.IsNull()) {
        break;
      }
      for (const TopoDS_Shape& image : history->Generated(copy.of(edge))) {
        if (!name.empty() && image.ShapeType() == TopAbs_FACE && namer.contains(image) && !namer.named(image)) {
          namer.add(image, name);
        }
      }
    }
    // What the history did not name, by where it lies: a wall by the
    // profile edge under the point of the piece below it.
    for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Face& face = TopoDS::Face(f.Current());
      if (namer.named(face)) {
        continue;
      }
      std::optional<gp_Pnt> foot;
      const int place = place_of(face, piece, depth, tolerance, foot);
      if (place == 0) {
        namer.add(face, start, -1);
      } else if (place == 1) {
        namer.add(face, end, -1);
      } else if (foot) {
        // Without a point on the piece below it the wall stays unnamed: a
        // guess (the origin's segment) could give it a wrong name.
        const std::string name = segments.segment_at(*foot);
        if (!name.empty()) {
          namer.add(face, name);
        }
      }
    }
    return namer.shape();
  }
  const std::string why = " projected onto the face could not be grown to the side asked for";
  throw std::runtime_error(profile_error(profile, why, why));
}


// The piece grown by `depth` with its walls tilted by the taper
// (mitcad#1547; EmbossProjectSpec::taper): the region moved in the sketch
// plane by |depth| tan(taper), projected onto the face and grown along its
// normals gives the top (that solid's far faces); the piece is the foot;
// each segment's wall is ruled between its edges on the foot and on the
// top, and the faces are sewn into the solid.
ShapePtr tapered(const std::string& feature, const detail::SweepFace& profile, const std::vector<TopoDS_Face>& piece,
                 const TopoDS_Shape& target, const Frame& frame, const gp_Vec& along, double reach,
                 const std::string& face_ref, double depth, double taper) {
  const std::string regions = regions_of(profile);
  const detail::MovedLoops loops =
      detail::moved_loops(profile.face, std::abs(depth) * std::tan(taper), regions);
  detail::SweepFace moved;
  moved.face = loops.face;
  moved.regions = profile.regions;
  // The segments whose edges the move shrank to nothing: their walls end
  // in a corner of the top.
  std::vector<std::string> vanished;
  for (const auto& [edge, name] : profile.edges) {
    if (loops.vanished.IsBound(edge)) {
      vanished.push_back(name);
    } else {
      moved.edges.emplace_back(loops.moved.Find(edge), name);
    }
  }
  const Segments foot_segments(profile, frame, along);
  const Segments top_segments(moved, frame, along);
  const std::vector<TopoDS_Face> top_piece = projected_piece(target, moved, frame, along, reach, face_ref);
  const ShapePtr top_solid = grown(feature, moved, top_piece, depth, top_segments);
  const NameList start = detail::cap_names(feature, "start", profile);
  const NameList end = detail::cap_names(feature, "end", profile);
  std::vector<TopoDS_Face> top;
  for (const std::string& name : end) {
    for (const int i : top_solid->find_faces(name)) {
      const TopoDS_Face& face = top_solid->face(i);
      if (std::none_of(top.begin(), top.end(), [&](const TopoDS_Face& f) { return f.IsSame(face); })) {
        top.push_back(face);
      }
    }
  }
  if (top.empty()) {
    throw std::runtime_error(placed_error("the tapered top of the profile could not be built",
                                          "the tapered top of the profile " + regions + " could not be built"));
  }
  // The boundaries of foot and top by the segment over them.
  std::map<std::string, NCollection_Sequence<TopoDS_Shape>> foot_edges;
  std::map<std::string, NCollection_Sequence<TopoDS_Shape>> top_edges;
  const auto middle_of = [](const TopoDS_Edge& edge) {
    const BRepAdaptor_Curve curve(edge);
    return curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
  };
  for (const TopoDS_Edge& edge : boundary_of(piece)) {
    foot_edges[foot_segments.segment_at(middle_of(edge))].Append(edge);
  }
  for (const TopoDS_Edge& edge : boundary_of(top)) {
    // The top lies over the moved region's projection: its segment there.
    const gp_Pnt middle = middle_of(edge);
    double nearest = std::numeric_limits<double>::infinity();
    gp_Pnt below = middle;
    for (const TopoDS_Face& reference : top_piece) {
      gp_Pnt own;
      const double d = distance_to(reference, middle, own);
      if (d < nearest) {
        nearest = d;
        below = own;
      }
    }
    top_edges[top_segments.segment_at(below)].Append(edge);
  }
  // The tolerance the faces are sewn with.
  const double tolerance = 1.0e-5 * std::max(1.0, std::abs(depth));
  // One wire of a segment's edges, from foot or top.
  const auto chain = [&](const NCollection_Sequence<TopoDS_Shape>& edges, const std::string& name) {
    occ::handle<NCollection_HSequence<TopoDS_Shape>> sequence = new NCollection_HSequence<TopoDS_Shape>();
    for (const TopoDS_Shape& edge : edges) {
      sequence->Append(edge);
    }
    const occ::handle<NCollection_HSequence<TopoDS_Shape>> wires =
        ShapeAnalysis_FreeBounds::ConnectEdgesToWires(sequence, tolerance, false);
    if (wires.IsNull() || wires->Length() != 1) {
      throw std::runtime_error(placed_error("a tapered wall of the profile could not be built",
                                            "the tapered wall " + name + " of the profile " + regions +
                                                " could not be built"));
    }
    return TopoDS::Wire(wires->Value(1));
  };
  BRepBuilderAPI_Sewing sewing(tolerance);
  for (const TopoDS_Face& face : piece) {
    sewing.Add(face);
  }
  for (const TopoDS_Face& face : top) {
    sewing.Add(face);
  }
  std::vector<std::pair<TopoDS_Face, std::string>> walls;
  for (const auto& [name, edges] : top_edges) {
    if (name.empty() || foot_edges.count(name) == 0) {
      throw std::invalid_argument(placed_error("the taper collapses the profile: its top has other edges than its foot",
                                               "the taper collapses the profile " + regions +
                                                   ": its top has other edges than its foot"));
    }
  }
  // The corners of the top, for walls whose top edge is gone.
  ShapeMap corners;
  for (const TopoDS_Face& face : top) {
    TopExp::MapShapes(face, TopAbs_VERTEX, corners);
  }
  for (const auto& [name, edges] : foot_edges) {
    const auto found = top_edges.find(name);
    const bool gone = std::find(vanished.begin(), vanished.end(), name) != vanished.end();
    if (name.empty() || (found == top_edges.end()) != gone) {
      throw std::invalid_argument(placed_error("the taper collapses the profile: its top has other edges than its foot",
                                               "the taper collapses the profile " + regions +
                                                   ": its top has other edges than its foot"));
    }
    const TopoDS_Wire foot_wire = chain(edges, name);
    if (gone) {
      // The wall from the foot's edge to the corner of the top nearest
      // both its ends.
      TopoDS_Vertex f0, f1;
      TopExp::Vertices(foot_wire, f0, f1);
      const gp_Pnt a = BRep_Tool::Pnt(f0);
      const gp_Pnt b = BRep_Tool::Pnt(f1);
      double nearest = std::numeric_limits<double>::infinity();
      TopoDS_Vertex apex;
      for (int i = 1; i <= corners.Extent(); ++i) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(corners(i)));
        if (p.Distance(a) + p.Distance(b) < nearest) {
          nearest = p.Distance(a) + p.Distance(b);
          apex = TopoDS::Vertex(corners(i));
        }
      }
      BRepOffsetAPI_ThruSections sections(false, true, 1.0e-7);
      sections.AddWire(foot_wire);
      sections.AddVertex(apex);
      detail::build(sections);
      if (apex.IsNull() || !sections.IsDone()) {
        throw std::runtime_error(placed_error("a tapered wall of the profile could not be built",
                                            "the tapered wall " + name + " of the profile " + regions +
                                                " could not be built"));
      }
      for (TopExp_Explorer f(sections.Shape(), TopAbs_FACE); f.More(); f.Next()) {
        sewing.Add(f.Current());
        walls.emplace_back(TopoDS::Face(f.Current()), name);
      }
      continue;
    }
    TopoDS_Wire top_wire = chain(found->second, name);
    if (!BRep_Tool::IsClosed(foot_wire)) {
      // The top's wire runs as the foot's: their starts meet.
      TopoDS_Vertex f0, f1, t0, t1;
      TopExp::Vertices(foot_wire, f0, f1);
      TopExp::Vertices(top_wire, t0, t1);
      const auto at = [](const TopoDS_Vertex& v) { return BRep_Tool::Pnt(v); };
      if (at(f0).Distance(at(t0)) + at(f1).Distance(at(t1)) > at(f0).Distance(at(t1)) + at(f1).Distance(at(t0))) {
        top_wire = TopoDS::Wire(top_wire.Reversed());
      }
    }
    BRepOffsetAPI_ThruSections sections(false, true, 1.0e-7);
    sections.AddWire(foot_wire);
    sections.AddWire(top_wire);
    detail::build(sections);
    if (!sections.IsDone()) {
      throw std::runtime_error(placed_error("a tapered wall of the profile could not be built",
                                            "the tapered wall " + name + " of the profile " + regions +
                                                " could not be built"));
    }
    for (TopExp_Explorer f(sections.Shape(), TopAbs_FACE); f.More(); f.Next()) {
      sewing.Add(f.Current());
      walls.emplace_back(TopoDS::Face(f.Current()), name);
    }
  }
  sewing.Perform();
  TopoDS_Shell shell;
  for (TopExp_Explorer e(sewing.SewedShape(), TopAbs_SHELL); e.More(); e.Next()) {
    if (!shell.IsNull()) {
      shell.Nullify();
      break;
    }
    shell = TopoDS::Shell(e.Current());
  }
  if (shell.IsNull() || sewing.NbFreeEdges() != 0) {
    throw std::runtime_error(placed_error("the tapered walls of the profile do not close up with its top and foot",
                                          "the tapered walls of the profile " + regions +
                                              " do not close up with its top and foot"));
  }
  BRep_Builder builder;
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  shell.Closed(true);
  builder.Add(solid, shell);
  BRepLib::OrientClosedSolid(solid);
  detail::FaceNamer namer(solid);
  const auto image = [&sewing](const TopoDS_Face& face) {
    return sewing.IsModified(face) ? sewing.Modified(face) : TopoDS_Shape(face);
  };
  for (const TopoDS_Face& face : piece) {
    namer.add(image(face), start, -1);
  }
  for (const TopoDS_Face& face : top) {
    namer.add(image(face), end, -1);
  }
  for (const auto& [wall, name] : walls) {
    namer.add(image(wall), name);
  }
  return namer.shape();
}
} // namespace

ShapePtr projected_emboss(const Shape& body, const EmbossProjectSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to emboss");
  }
  detail::require_finite("emboss depth", spec.depth);
  if (!(std::abs(spec.depth) > 1.0e-7)) {
    throw std::invalid_argument("the emboss has no depth");
  }
  const double length = spec.direction.Magnitude();
  if (!(length > gp::Resolution())) {
    throw std::invalid_argument("the projection direction has no length");
  }
  const gp_Vec along = spec.direction / length;
  if (std::abs(along.Dot(gp_Vec(spec.frame.normal()))) < 1.0e-6) {
    throw std::invalid_argument("the projection direction lies in the sketch plane");
  }
  if (!(std::abs(spec.taper) < 2.0 * std::atan(1.0) - 1.0e-9)) {
    throw std::invalid_argument("the emboss's taper angle must lie between -90 and 90 degrees");
  }
  return detail::run("emboss", [&] {
    BRep_Builder builder;
    TopoDS_Compound pieces;
    builder.MakeCompound(pieces);
    const std::vector<int> found = body.find_faces(spec.face);
    if (found.empty()) {
      throw std::invalid_argument(no_face_error(spec.face));
    }
    for (const int i : found) {
      builder.Add(pieces, body.face(i));
    }
    detail::require_surfaces(pieces, "emboss");
    // The booleans and offsets below work on a copy of the face.
    const detail::InputCopy copy(pieces);
    const TopoDS_Shape& target = copy.shape();
    const std::vector<detail::SweepFace> profiles = detail::region_faces(spec.feature, spec.frame, spec.regions);
    Bnd_Box box;
    BRepBndLib::Add(target, box);
    for (const detail::SweepFace& profile : profiles) {
      BRepBndLib::Add(profile.face, box);
    }
    // Far enough both ways to pass through the face from the sketch.
    const double reach = 2.0 * std::sqrt(box.SquareExtent()) + 1.0;
    std::vector<ShapePtr> parts;
    for (const detail::SweepFace& profile : profiles) {
      const std::vector<TopoDS_Face> piece = projected_piece(target, profile, spec.frame, along, reach, spec.face);
      if (spec.taper == 0.0) {
        parts.push_back(grown(spec.feature, profile, piece, spec.depth, Segments(profile, spec.frame, along)));
      } else {
        parts.push_back(tapered(spec.feature, profile, piece, target, spec.frame, along, reach, spec.face, spec.depth,
                                spec.taper));
      }
    }
    return detail::finished(*detail::fused(parts, "the emboss"), "the emboss");
  });
}

} // namespace mitcad::geometry
