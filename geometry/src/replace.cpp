// SPDX-License-Identifier: MIT
#include "replace.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <set>
#include <stdexcept>

#include <BOPAlgo_MakerVolume.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLib.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
// A cell is inside or outside the body when no more than this share of
// its volume is on the other side.
constexpr double kMixed = 1.0e-4;

double diagonal(const Bnd_Box& box) { return box.IsVoid() ? 0.0 : std::sqrt(box.SquareExtent()); }

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// True when sample points of the face lie on the other face's surface.
bool lies_on(const TopoDS_Face& face, const TopoDS_Face& other) {
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(face, false);
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(other);
  for (const double s : {0.5, 0.3, 0.7}) {
    const gp_Pnt p = surface.Value(u0 + (u1 - u0) * s, v0 + (v1 - v0) * (1 - s));
    GeomAPI_ProjectPointOnSurf projection(p, target);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > 1.0e-6) {
      return false;
    }
  }
  return true;
}

// A face on the whole surface (or the given part of it), with the original
// face's location.
std::optional<TopoDS_Face> face_on(const occ::handle<Geom_Surface>& surface, double u0, double u1,
                                   double v0, double v1, const TopLoc_Location& location) {
  BRepBuilderAPI_MakeFace maker(surface, u0, u1, v0, v1, Precision::Confusion());
  if (!maker.IsDone()) {
    return std::nullopt;
  }
  return TopoDS::Face(maker.Face().Moved(location));
}

// The face's surface continued past its edges until it crosses the box:
// a plane as a face covering the box, cylinders and cones along their axes
// (a cone up to its apex), spheres and tori whole, other surfaces to their
// natural bounds and, where those end, extrapolated `reach` further (a
// B-spline continues with matching tangents). Directions in which the
// surface closes on itself stay as they are. The face itself when the
// surface cannot be continued.
TopoDS_Face extended(const TopoDS_Face& face, const Bnd_Box& around) {
  if (const auto plane = face_plane(face)) {
    return plane_face(*plane, around);
  }
  const double reach = std::max(1.0, diagonal(around));
  const BRepAdaptor_Surface surface(face);
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  std::optional<TopoDS_Face> result;
  switch (surface.GetType()) {
  case GeomAbs_Cylinder:
    result =
        BRepBuilderAPI_MakeFace(surface.Cylinder(), 0.0, kTwoPi, v0 - reach, v1 + reach).Face();
    break;
  case GeomAbs_Cone: {
    const gp_Cone cone = surface.Cone();
    // Up to the apex, on the side of it the face lies on.
    const double apex = -cone.RefRadius() / std::sin(cone.SemiAngle());
    const double gap = 1.0e-6 * reach;
    const bool after = (v0 + v1) / 2.0 > apex;
    const double low = after ? std::max(apex + gap, v0 - reach) : v0 - reach;
    const double high = after ? v1 + reach : std::min(apex - gap, v1 + reach);
    result = BRepBuilderAPI_MakeFace(cone, 0.0, kTwoPi, low, high).Face();
    break;
  }
  case GeomAbs_Sphere:
    result = BRepBuilderAPI_MakeFace(surface.Sphere()).Face();
    break;
  case GeomAbs_Torus:
    result = BRepBuilderAPI_MakeFace(surface.Torus()).Face();
    break;
  default: {
    TopLoc_Location location;
    occ::handle<Geom_Surface> geometry = BRep_Tool::Surface(face, location);
    // A trimmed surface continues as its basis.
    while (const auto trimmed = occ::down_cast<Geom_RectangularTrimmedSurface>(geometry)) {
      geometry = trimmed->BasisSurface();
    }
    double a0 = 0, a1 = 0, b0 = 0, b1 = 0;
    geometry->Bounds(a0, a1, b0, b1);
    if (geometry->IsKind(STANDARD_TYPE(Geom_BoundedSurface))) {
      // Extended on a copy: the body's surface stays as it is.
      occ::handle<Geom_BoundedSurface> bounded =
          occ::down_cast<Geom_BoundedSurface>(geometry->Copy());
      const bool u_closed = geometry->IsUClosed() || geometry->IsUPeriodic();
      const bool v_closed = geometry->IsVClosed() || geometry->IsVPeriodic();
      for (const bool after : {false, true}) {
        if (!u_closed) {
          GeomLib::ExtendSurfByLength(bounded, reach, 1, true, after);
        }
        if (!v_closed) {
          GeomLib::ExtendSurfByLength(bounded, reach, 1, false, after);
        }
      }
      geometry = bounded;
      geometry->Bounds(a0, a1, b0, b1);
    } else {
      // Surfaces of extrusion and revolution, offsets: infinite bounds
      // become the face's ones and `reach` more.
      a0 = Precision::IsInfinite(a0) ? u0 - reach : a0;
      a1 = Precision::IsInfinite(a1) ? u1 + reach : a1;
      b0 = Precision::IsInfinite(b0) ? v0 - reach : b0;
      b1 = Precision::IsInfinite(b1) ? v1 + reach : b1;
    }
    result = face_on(geometry, a0, a1, b0, b1, location);
    break;
  }
  }
  // The continued surface must hold the face (an extrapolation that does
  // not is of no use).
  if (!result || !lies_on(face, *result)) {
    return face;
  }
  return *result;
}

// True for the surfaces whose faces the boolean operations' merge joins by
// their geometry; others only when they are the same surface.
bool elementary(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face, false).GetType()) {
  case GeomAbs_Plane:
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_Sphere:
  case GeomAbs_Torus:
    return true;
  default:
    return false;
  }
}

// True when the two surfaces give the face's points at the same parameters.
bool same_parameters(const TopoDS_Face& face, const occ::handle<Geom_Surface>& a,
                     const occ::handle<Geom_Surface>& b) {
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  for (const double s : {0.0, 0.25, 0.5, 0.75, 1.0}) {
    for (const double t : {0.0, 0.5, 1.0}) {
      const double u = u0 + (u1 - u0) * s;
      const double v = v0 + (v1 - v0) * t;
      if (a->Value(u, v).Distance(b->Value(u, v)) > 1.0e-7) {
        return false;
      }
    }
  }
  return true;
}

// A copy of the body with faces moved onto the surfaces of the given faces
// (continued B-splines, which hold them at the same parameters), so that
// walls on those surfaces merge with them. A face whose surface does not
// match stays as it is.
ShapePtr on_surfaces(const Shape& body, const std::vector<std::pair<int, TopoDS_Face>>& moves) {
  BRepBuilderAPI_Copy copier(body.occt(), true, false);
  BRep_Builder builder;
  for (const auto& [index, wall] : moves) {
    const TopoDS_Face face = TopoDS::Face(copier.Modified(body.face(index)).First());
    TopLoc_Location location;
    const occ::handle<Geom_Surface> old = BRep_Tool::Surface(face, location);
    TopLoc_Location wall_location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(wall, wall_location);
    if (!(location == wall_location) || !same_parameters(face, old, surface)) {
      continue;
    }
    ShapeMap edges;
    TopExp::MapShapes(face, TopAbs_EDGE, edges);
    for (int i = 1; i <= edges.Extent(); ++i) {
      const TopoDS_Edge edge = TopoDS::Edge(edges(i).Oriented(TopAbs_FORWARD));
      const double tolerance = BRep_Tool::Tolerance(edge);
      double first = 0;
      double last = 0;
      if (BRep_Tool::IsClosed(edge, face)) {
        const occ::handle<Geom2d_Curve> forward =
            BRep_Tool::CurveOnSurface(edge, face, first, last);
        const occ::handle<Geom2d_Curve> reversed =
            BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, first, last);
        builder.UpdateEdge(edge, forward, reversed, surface, location, tolerance);
      } else {
        const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, face, first, last);
        if (curve.IsNull()) {
          continue;
        }
        builder.UpdateEdge(edge, curve, surface, location, tolerance);
      }
      builder.Range(edge, surface, location, first, last);
    }
    builder.UpdateFace(face, surface, location, BRep_Tool::Tolerance(face));
  }
  std::vector<Shape::NamedFace> named;
  for (int i = 0; i < body.face_count(); ++i) {
    named.push_back({copier.Modified(body.face(i)).First(), body.face_names(i)});
  }
  return std::make_shared<Shape>(copier.Shape(), named);
}

// The images of a face in an operation's result: its pieces, or itself.
template <class Operation>
std::vector<TopoDS_Shape> images_in(Operation& operation, const TopoDS_Shape& face) {
  if (operation.IsDeleted(face)) {
    return {};
  }
  const NCollection_List<TopoDS_Shape>& modified = operation.Modified(face);
  if (modified.IsEmpty()) {
    return {face};
  }
  return {modified.begin(), modified.end()};
}

// The faces of the body next to the replaced ones (sharing an edge).
std::vector<int> neighbours_of(const Shape& body, const std::set<int>& faces) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<int> result;
  for (int f : faces) {
    for (TopExp_Explorer it(body.face(f), TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || !ancestors.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int n = body.face_index(other);
        if (n >= 0 && faces.count(n) == 0 &&
            std::find(result.begin(), result.end(), n) == result.end()) {
          result.push_back(n);
        }
      }
    }
  }
  return result;
}

// The share of the cell's volume inside the body.
double inside_share(const Shape& body, const TopoDS_Shape& cell, double cell_volume) {
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(cell);
  common.SetArguments(arguments);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  detail::build(common);
  if (common.IsDone() && !common.HasErrors()) {
    return volume_of(common.Shape()) / cell_volume;
  }
  // The middle of the cell, when the boolean operation fails.
  GProp_GProps props;
  BRepGProp::VolumeProperties(cell, props);
  BRepClass3d_SolidClassifier classifier(body.occt(), props.CentreOfMass(), Precision::Confusion());
  return classifier.State() == TopAbs_IN ? 1.0 : 0.0;
}

// The shape with the cells added (fuse) or removed (cut), names carried;
// the coincident faces merge.
ShapePtr apply(BRepAlgoAPI_BooleanOperation&& operation, const Shape& shape,
               const std::vector<ShapePtr>& cells) {
  // On a copy of the shape and the cells, which share its faces, as a
  // boolean operation (boolean.cpp): the face merge writes into the
  // sub-shapes the boolean kept, and the shape may be the body itself.
  std::vector<TopoDS_Shape> inputs{shape.occt()};
  for (const ShapePtr& cell : cells) {
    inputs.push_back(cell->occt());
  }
  const InputCopy copy(inputs);
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(copy.shape(0));
  NCollection_List<TopoDS_Shape> tools;
  for (std::size_t i = 1; i < inputs.size(); ++i) {
    tools.Append(copy.shape(i));
  }
  operation.SetArguments(arguments);
  operation.SetTools(tools);
  operation.SetNonDestructive(true);
  detail::build(operation);
  if (!operation.IsDone() || operation.HasErrors()) {
    throw std::runtime_error("the body could not be joined with the region up to the target");
  }
  operation.SimplifyResult();
  FaceNamer namer(operation.Shape());
  namer.carry(operation, shape, copy);
  for (const ShapePtr& cell : cells) {
    namer.carry(operation, *cell, copy);
  }
  return namer.shape();
}

int solid_count(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

} // namespace

ShapePtr replace_by_slab(const Shape& body, const std::vector<int>& faces, const Tool& target,
                         const std::string& name) {
  const std::set<int> sources(faces.begin(), faces.end());
  // The target's faces as given.
  std::vector<TopoDS_Face> given;
  switch (target.kind) {
  case Tool::Kind::Plane:
    break;
  case Tool::Kind::Face:
  case Tool::Kind::Body: {
    if (!target.shape) {
      throw std::invalid_argument("the target has no body");
    }
    if (target.kind == Tool::Kind::Body && target.shape->occt().IsSame(body.occt())) {
      throw std::invalid_argument("the target must not be the body itself");
    }
    const std::vector<int> found =
        target.kind == Tool::Kind::Face
            ? resolve_faces(*target.shape, {target.face}, "target faces")
            : [&] {
                std::vector<int> all;
                for (int f = 0; f < target.shape->face_count(); ++f) {
                  all.push_back(f);
                }
                return all;
              }();
    for (int f : found) {
      given.push_back(target.shape->face(f));
    }
    if (given.empty()) {
      throw std::invalid_argument("the target body has no faces");
    }
    break;
  }
  case Tool::Kind::Curves:
    throw std::invalid_argument("the target must be a plane, a face or a body");
  }

  // The region the faces are continued through: the body and the target,
  // enlarged.
  Bnd_Box around = box_of(body.occt());
  for (const TopoDS_Face& face : given) {
    around.Add(box_of(face));
  }
  around.Enlarge(0.25 * std::max(1.0, diagonal(around)));

  // The target: a plane across the box, one face continued along its
  // surface, several faces as they are.
  std::vector<TopoDS_Shape> target_faces;
  if (target.kind == Tool::Kind::Plane) {
    target_faces.push_back(plane_face(target.plane, around));
  } else if (given.size() == 1) {
    target_faces.push_back(extended(given.front(), around));
  } else {
    target_faces.assign(given.begin(), given.end());
  }

  // Already on the target: only the names change.
  const bool on_target = std::all_of(sources.begin(), sources.end(), [&](int f) {
    return std::any_of(target_faces.begin(), target_faces.end(), [&](const TopoDS_Shape& t) {
      return lies_on(body.face(f), TopoDS::Face(t));
    });
  });
  if (on_target) {
    std::vector<Shape::NamedFace> named;
    for (int i = 0; i < body.face_count(); ++i) {
      named.push_back({body.face(i), sources.count(i) > 0 ? NameList{name} : body.face_names(i)});
    }
    return std::make_shared<Shape>(body.occt(), named);
  }

  // The neighbours continued along their surfaces; neighbours on one
  // surface share the first one's continuation.
  struct Wall {
    TopoDS_Face face;
    NameList names;
  };
  std::vector<Wall> walls;
  // Neighbours whose walls are on a continued copy of their surface
  // (B-splines): the merge after the boolean operations joins such faces
  // only on the same surface, so the body's faces move onto the copy.
  std::vector<std::pair<int, TopoDS_Face>> moves;
  for (int n : neighbours_of(body, sources)) {
    const TopoDS_Face& face = body.face(n);
    auto wall = std::find_if(walls.begin(), walls.end(),
                             [&](const Wall& w) { return lies_on(face, w.face); });
    if (wall == walls.end()) {
      walls.push_back({extended(face, around), body.face_names(n)});
      wall = walls.end() - 1;
    }
    TopLoc_Location a;
    TopLoc_Location b;
    if (!elementary(face) && BRep_Tool::Surface(wall->face, a) != BRep_Tool::Surface(face, b)) {
      moves.emplace_back(n, wall->face);
    }
  }

  // The cells the faces, the walls and the target divide space into.
  BOPAlgo_MakerVolume maker;
  for (int f : sources) {
    maker.AddArgument(body.face(f));
  }
  for (const Wall& wall : walls) {
    maker.AddArgument(wall.face);
  }
  for (const TopoDS_Shape& face : target_faces) {
    maker.AddArgument(face);
  }
  maker.SetRunParallel(false);
  maker.SetNonDestructive(true);
  maker.SetAvoidInternalShapes(true);
  maker.SetIntersect(true);
  detail::interruptible([&](const Message_ProgressRange& range) { maker.Perform(range); });
  if (maker.HasErrors()) {
    throw std::runtime_error("the target and the neighbouring faces do not close a region");
  }
  ShapeMap source_images;
  for (int f : sources) {
    for (const TopoDS_Shape& image : images_in(maker, body.face(f))) {
      source_images.Add(image);
    }
  }
  ShapeMap target_images;
  for (const TopoDS_Shape& face : target_faces) {
    for (const TopoDS_Shape& image : images_in(maker, face)) {
      target_images.Add(image);
    }
  }
  std::vector<ShapeMap> wall_images(walls.size());
  for (std::size_t w = 0; w < walls.size(); ++w) {
    for (const TopoDS_Shape& image : images_in(maker, walls[w].face)) {
      wall_images[w].Add(image);
    }
  }

  // The cells between the faces and the target: bounded by both. A cell
  // must be on one side of the body's surface: inside it (removed) or
  // outside (added).
  struct Cell {
    TopoDS_Shape solid;
    double volume = 0.0;
    bool inside = false;
    std::vector<int> sources; // indices into source_images
  };
  std::vector<Cell> cells;
  bool through = false;
  for (TopExp_Explorer it(maker.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
    Cell cell;
    cell.solid = it.Current();
    bool meets_target = false;
    for (TopExp_Explorer f(cell.solid, TopAbs_FACE); f.More(); f.Next()) {
      meets_target = meets_target || target_images.Contains(f.Current());
      const int s = source_images.FindIndex(f.Current());
      if (s > 0 && std::find(cell.sources.begin(), cell.sources.end(), s) == cell.sources.end()) {
        cell.sources.push_back(s);
      }
    }
    if (!meets_target || cell.sources.empty()) {
      continue;
    }
    cell.volume = volume_of(cell.solid);
    if (!(cell.volume > 0.0)) {
      continue;
    }
    const double share = inside_share(body, cell.solid, cell.volume);
    if (share > kMixed && share < 1.0 - kMixed) {
      // Through the body's surface elsewhere: the target lies past the
      // body's other faces there.
      through = true;
      continue;
    }
    cell.inside = share >= 0.5;
    cells.push_back(cell);
  }
  // A face piece between the target on both sides moves to the nearer
  // one: of two cells at a piece, the smaller.
  std::vector<bool> kept(cells.size(), false);
  for (int s = 1; s <= source_images.Extent(); ++s) {
    int best = -1;
    for (std::size_t c = 0; c < cells.size(); ++c) {
      const auto& own = cells[c].sources;
      if (std::find(own.begin(), own.end(), s) != own.end() &&
          (best < 0 || cells[c].volume < cells[static_cast<std::size_t>(best)].volume)) {
        best = static_cast<int>(c);
      }
    }
    if (best >= 0) {
      kept[static_cast<std::size_t>(best)] = true;
    }
  }

  // The cells with names: the target's pieces `name`, the walls their
  // neighbour's names, the faces' pieces none.
  std::vector<ShapePtr> added;
  std::vector<ShapePtr> removed;
  for (std::size_t c = 0; c < cells.size(); ++c) {
    if (!kept[c]) {
      continue;
    }
    std::vector<Shape::NamedFace> named;
    for (TopExp_Explorer f(cells[c].solid, TopAbs_FACE); f.More(); f.Next()) {
      if (target_images.Contains(f.Current())) {
        named.push_back({f.Current(), {name}});
        continue;
      }
      for (std::size_t w = 0; w < walls.size(); ++w) {
        if (wall_images[w].Contains(f.Current())) {
          named.push_back({f.Current(), walls[w].names});
          break;
        }
      }
    }
    const ShapePtr cell = std::make_shared<Shape>(cells[c].solid, named);
    (cells[c].inside ? removed : added).push_back(cell);
  }
  if (added.empty() && removed.empty()) {
    throw std::runtime_error(through ? "the target passes beyond the other faces of the body"
                                     : "the target does not meet the body where the faces are");
  }

  ShapePtr result = moves.empty() ? std::make_shared<Shape>(body) : on_surfaces(body, moves);
  if (!added.empty()) {
    result = apply(BRepAlgoAPI_Fuse(), *result, added);
  }
  if (!removed.empty()) {
    result = apply(BRepAlgoAPI_Cut(), *result, removed);
  }
  FaceNamer namer(result->occt());
  for (int i = 0; i < result->face_count(); ++i) {
    for (const std::string& n : result->face_names(i)) {
      namer.add(result->face(i), n);
    }
  }
  namer.finish();
  result = namer.shape();

  for (int f : sources) {
    for (const std::string& old : body.face_names(f)) {
      for (int i = 0; i < result->face_count(); ++i) {
        if (face_matches(result->face_names(i), old)) {
          throw std::runtime_error("face " + old + " could not be replaced: the target does not "
                                   "reach all of it");
        }
      }
    }
  }
  if (solid_count(result->occt()) > std::max(1, solid_count(body.occt()))) {
    throw std::runtime_error("the target splits the body");
  }
  if (solid_count(result->occt()) == 0) {
    throw std::runtime_error("the target removes the whole body");
  }
  return result;
}

} // namespace mitcad::geometry::detail
