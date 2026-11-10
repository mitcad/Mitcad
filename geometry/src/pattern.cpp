// SPDX-License-Identifier: MIT
#include "mitcad/geometry/pattern.hpp"

#include <algorithm>
#include <cmath>
#include <numeric>
#include <optional>
#include <stdexcept>
#include <utility>

#include <Message_Alert.hxx>
#include <Message_Report.hxx>
#include <TopoDS_AlertWithShape.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_List.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <ShapeFix_Face.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_HSequence.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>

#include "history.hpp"
#include "mitcad/analysis/common.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/query.hpp"
#include "union_groups.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTolerance = 1.0e-6;

// The first fuzzy value a union within a tolerance tries (mm, mitcad#397;
// as a join's, boolean.cpp).
constexpr double kFuzzyStart = 1.0e-5;

// The most shapes with free-form faces that a union joins after the
// elementary ones (united, mitcad#400 and mitcad#1273).
constexpr std::size_t kFewFreeForm = 4;

// The fewest shapes a union looks for near misses between before it unites
// them all (united, mitcad#1278): a union of fewer, made again where it went
// wrong, costs little.
constexpr std::size_t kProbeFrom = 8;

double area(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

gp_Pln plane_of(const TopoDS_Face& face) {
  const Handle(Geom_Surface) surface = BRep_Tool::Surface(face);
  const Handle(Geom_Plane) plane = Handle(Geom_Plane)::DownCast(surface);
  if (plane.IsNull()) {
    throw std::invalid_argument("the openings of the faces must lie in planes");
  }
  return plane->Pln();
}

bool coplanar(const TopoDS_Face& a, const TopoDS_Face& b) {
  const gp_Pln pa = plane_of(a);
  const gp_Pln pb = plane_of(b);
  return pa.Axis().Direction().IsParallel(pb.Axis().Direction(), 1.0e-9) &&
         pa.Distance(pb.Location()) <= kTolerance;
}

// True when a vertex of `inner` lies inside `outer`.
bool contains(const TopoDS_Face& outer, const TopoDS_Face& inner) {
  TopExp_Explorer vertex(inner, TopAbs_VERTEX);
  gp_Pnt point;
  if (vertex.More()) {
    point = BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
  } else {
    // A wire of one closed edge without a vertex: a point on the edge.
    TopExp_Explorer edge(inner, TopAbs_EDGE);
    if (!edge.More()) {
      return false;
    }
    double first = 0.0;
    double last = 0.0;
    const Handle(Geom_Curve) curve = BRep_Tool::Curve(TopoDS::Edge(edge.Current()), first, last);
    point = curve->Value(first);
  }
  BRepClass_FaceClassifier classifier(outer, point, kTolerance);
  return classifier.State() == TopAbs_IN;
}

// Caps across openings: a cap inside a larger coplanar cap becomes its hole.
std::vector<TopoDS_Face> nest(std::vector<TopoDS_Face> caps) {
  std::stable_sort(caps.begin(), caps.end(), [](const TopoDS_Face& a, const TopoDS_Face& b) {
    return area(a) > area(b);
  });
  std::vector<TopoDS_Face> result;
  for (const TopoDS_Face& cap : caps) {
    bool hole = false;
    for (TopoDS_Face& outer : result) {
      if (!coplanar(outer, cap) || !contains(outer, cap)) {
        continue;
      }
      BRepBuilderAPI_MakeFace maker(outer);
      for (TopExp_Explorer wire(cap, TopAbs_WIRE); wire.More(); wire.Next()) {
        maker.Add(TopoDS::Wire(wire.Current()));
      }
      ShapeFix_Face fix(maker.Face());
      fix.FixOrientation();
      outer = fix.Face();
      hole = true;
      break;
    }
    if (!hole) {
      result.push_back(cap);
    }
  }
  return result;
}

// The orientation an edge has in a face, as the face uses it.
TopAbs_Orientation orientation_in(const TopoDS_Shape& face, const TopoDS_Shape& edge) {
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    if (it.Current().IsSame(edge)) {
      return it.Current().Orientation();
    }
  }
  return TopAbs_EXTERNAL;
}

// Whether a union of the shapes lost material they hold (mitcad#1273): it
// has no solid where they have some, or holds less volume than the largest
// of them by more than 1e-6 of the larger of 1 and that volume. A union
// never does. OCCT's exact union of a nut lying inside a threaded carriage,
// whose bore's seam and chamfer circle pass a few micrometres from the
// vertices of the carriage's thread, split a face of the carriage with
// edges left over and came out empty: its result checks as valid (it has
// nothing to check). The volumes are measured only for a union OCCT warned
// about (lost).
class MaterialCheck {
public:
  explicit MaterialCheck(const std::vector<const Shape*>& shapes) : m_shapes(shapes) {
    for (const Shape* shape : shapes) {
      m_solids = m_solids || TopExp_Explorer(shape->occt(), TopAbs_SOLID).More();
    }
  }

  // No solid where the shapes have some.
  bool empty(const TopoDS_Shape& result) const {
    return m_solids && !TopExp_Explorer(result, TopAbs_SOLID).More();
  }

  // Empty, or less volume than the largest shape.
  bool lost(const TopoDS_Shape& result) const {
    if (!m_solids) {
      return false;
    }
    if (empty(result)) {
      return true;
    }
    if (m_largest < 0.0) {
      m_largest = 0.0;
      for (const Shape* shape : m_shapes) {
        m_largest = std::max(m_largest, volume(*shape));
      }
    }
    const double held = analysis::volume_properties(result).Mass();
    return held < m_largest - 1.0e-6 * std::max(1.0, m_largest);
  }

private:
  const std::vector<const Shape*>& m_shapes;
  bool m_solids = false;
  mutable double m_largest = -1.0;
};

// The shape, marked as found valid by OCCT's checker (Shape::checked_valid):
// a union checks its result, and the operations on it then check only
// what they change (mitcad#1278).
ShapePtr checked(ShapePtr shape) {
  shape->set_checked_valid(true);
  return shape;
}

// Whether a face of the shape lies on a free-form surface (not a plane,
// cylinder, cone, sphere or torus): a B-spline, a swept surface, an offset.
bool free_form(const Shape& shape) {
  for (int i = 0; i < shape.face_count(); ++i) {
    switch (BRepAdaptor_Surface(shape.face(i), false).GetType()) {
    case GeomAbs_Plane:
    case GeomAbs_Cylinder:
    case GeomAbs_Cone:
    case GeomAbs_Sphere:
    case GeomAbs_Torus:
      break;
    default:
      return true;
    }
  }
  return false;
}

// How a union starts (united): with the near misses between its shapes
// united first (detail::near_misses) when it has enough shapes, or with the
// exact union of all (a union whose near misses are united already).
enum class Start { probe, exact };

ShapePtr united(const std::vector<const Shape*>& shapes, double within, Start start = Start::probe) {
  if (shapes.size() == 1) {
    return std::make_shared<Shape>(*shapes.front());
  }
  // Shapes whose boxes meet, directly or through others, are united group
  // by group, the groups beside each other, and put together without a
  // boolean operation (union_groups.hpp, mitcad#1273). A pattern's copies
  // are mostly apart: fusing 90 of them took most of a minute; one union of
  // a machine's 257 bodies at once took most of a minute and 12.7 GB. Boxes
  // within the tolerance of a union within one meet (its fuzzy value may
  // join them).
  const std::vector<std::vector<std::size_t>> groups =
      detail::box_groups(shapes, std::max(kTolerance, within));
  if (groups.size() > 1) {
    return detail::grouped_union(shapes, groups,
                                 [within](const std::vector<const Shape*>& group) { return united(group, within); });
  }
  // (mitcad#400) The shapes of elementary faces are joined first, and the
  // shapes with free-form faces then with their union. OCCT's union
  // intersects every pair of faces that meet; a sketch's lines split a
  // slab's rim and a tread's sides into many faces, which the face merge
  // after the union makes one again. Joined in one go, each of those pieces
  // met a helical stringer's faces on the same cylinder: a free-form face
  // against a cylinder it lies on is the costliest pair, walked over the
  // whole of both, and the merge after it broke the result, which was then
  // joined again without it (a spiral stair's 40 treads and two stringers:
  // 10 s on 16 cores, 72 s on fewer, 2292 faces). Joined first, the
  // treads' merged faces meet the stringers once each. Only a few shapes
  // with free-form faces among many (mitcad#1273): a machine's 257 bodies,
  // 30 of them with threads and other free-form faces, each meeting a few
  // others, took two unions of most of the bodies so, and a fault between
  // a body joined first and one joined after could be mended only in the
  // union of all.
  std::vector<const Shape*> elementary;
  std::vector<const Shape*> rest;
  for (const Shape* shape : shapes) {
    (free_form(*shape) ? rest : elementary).push_back(shape);
  }
  if (elementary.size() > 1 && !rest.empty() && rest.size() <= kFewFreeForm) {
    const ShapePtr first = united(elementary, within);
    rest.insert(rest.begin(), first.get());
    return united(rest, within);
  }
  // (mitcad#1278) Shapes that meet only within micrometres, a vertex of
  // one near a face of the other but not on it (detail::near_misses: two
  // copies of a plate whose side walls lie 1e-4 mm apart, a nut inside a
  // threaded carriage whose thread's vertices lie micrometres from the
  // nut's bore), are where OCCT's exact union goes wrong. Before the union
  // of many, such pairs, and those linked through each other, are united
  // first, each such union beside the others and finding its own fuzzy
  // value where its exact one is wrong (below); the union of all is then
  // made once, with them in their places. Found only in the union of all,
  // such places took a second union of all (below): a machine's 257
  // bodies, 105 places in 18 of them, two unions of some 10 s each. Pairs
  // whose boxes do not meet have no such place.
  if (start == Start::probe && shapes.size() >= kProbeFrom) {
    const std::vector<std::pair<std::size_t, std::size_t>> pairs =
        detail::box_pairs(shapes, std::max(kTolerance, within));
    const std::vector<char> missed = detail::near_misses(shapes, pairs, detail::kUnusedReach);
    std::vector<std::pair<std::size_t, std::size_t>> links;
    for (std::size_t k = 0; k < pairs.size(); ++k) {
      if (missed[k] != 0) {
        links.push_back(pairs[k]);
      }
    }
    std::vector<std::vector<std::size_t>> spots;
    for (std::vector<std::size_t>& group : detail::linked_groups(shapes.size(), links)) {
      if (group.size() > 1) {
        spots.push_back(std::move(group));
      }
    }
    if (!spots.empty() && spots.front().size() < shapes.size()) {
      std::vector<ShapePtr> mended(spots.size());
      detail::parallel_jobs(spots.size(), [&](std::size_t k) {
        std::vector<const Shape*> members;
        for (const std::size_t i : spots[k]) {
          members.push_back(shapes[i]);
        }
        mended[k] = united(members, within, Start::exact);
      });
      // The shapes in their order, each spot's union in the place of its
      // first shape.
      std::vector<std::size_t> spot_of(shapes.size(), spots.size());
      for (std::size_t k = 0; k < spots.size(); ++k) {
        for (const std::size_t i : spots[k]) {
          spot_of[i] = k;
        }
      }
      std::vector<const Shape*> next;
      for (std::size_t i = 0; i < shapes.size(); ++i) {
        const std::size_t k = spot_of[i];
        if (k == spots.size()) {
          next.push_back(shapes[i]);
        } else if (spots[k].front() == i) {
          next.push_back(mended[k].get());
        }
      }
      return united(next, within, Start::exact);
    }
  }
  // The union, whether OCCT left edges or faces of it unused (a face it
  // could not rebuild of its pieces, faces it could not make a solid of:
  // such a union lacks what those built) and the boxes of those, and
  // whether it has no solid where the shapes have some. `merged` is the
  // union with its faces merged, `plain` as OCCT's union left them (the
  // merge works on copies of what it changes, SetSafeInputMode, so the
  // union it starts from stays as it was). Without `whole`, a union OCCT
  // left pieces of unused, or an empty one, is not named (it is not used).
  struct Fused {
    std::optional<detail::FaceNamer> merged;
    std::optional<detail::FaceNamer> plain;
    bool dropped = false;
    bool empty = false;
    bool warned = false; // any warning of OCCT's
    std::vector<Bnd_Box> unused;
  };
  const MaterialCheck material(shapes);
  const auto fused = [&](double fuzzy, bool whole) {
    // On a copy of the shapes, as a boolean operation (boolean.cpp): the
    // face merge writes into the sub-shapes the union kept.
    std::vector<TopoDS_Shape> inputs;
    for (const Shape* shape : shapes) {
      inputs.push_back(shape->occt());
    }
    const detail::InputCopy copy(inputs);
    BRepAlgoAPI_Fuse fuse;
    NCollection_List<TopoDS_Shape> arguments;
    arguments.Append(copy.shape(0));
    NCollection_List<TopoDS_Shape> tools;
    for (std::size_t i = 1; i < shapes.size(); ++i) {
      tools.Append(copy.shape(i));
    }
    fuse.SetArguments(arguments);
    fuse.SetTools(tools);
    fuse.SetNonDestructive(true);
    // On all cores, as the booleans (boolean.cpp): the union of a part
    // with a thread insert whose faces run 25 turns took 20 s on one
    // (mitcad#235).
    fuse.SetRunParallel(true);
    if (fuzzy > 0.0) {
      fuse.SetFuzzyValue(fuzzy);
    }
    detail::build(fuse);
    if (!fuse.IsDone() || fuse.HasErrors()) {
      throw std::runtime_error("the union failed");
    }
    Fused out;
    out.warned = fuse.HasWarnings();
    out.empty = material.empty(fuse.Shape());
    // Where the unused pieces are: the edges an alert lists beside its
    // face, else what it lists. Sub-shapes of one shape that the union's
    // tolerances made meet (an acquired self-intersection) are such a
    // place too: a threaded carriage and a mount whose stud stands in the
    // thread's hole, coaxial with it, came out of the exact union as two
    // solids, OCCT warning only of that.
    for (const occ::handle<Message_Alert>& alert : fuse.GetReport()->GetAlerts(Message_Warning)) {
      if (!detail::leaves_pieces_unused(alert)) {
        continue;
      }
      out.dropped = true;
      const occ::handle<TopoDS_AlertWithShape> with = occ::down_cast<TopoDS_AlertWithShape>(alert);
      Bnd_Box box;
      if (!with.IsNull() && !with->GetShape().IsNull()) {
        for (TopoDS_Iterator it(with->GetShape()); it.More(); it.Next()) {
          if (it.Value().ShapeType() != TopAbs_FACE) {
            BRepBndLib::Add(it.Value(), box);
          }
        }
        if (box.IsVoid()) {
          BRepBndLib::Add(with->GetShape(), box);
        }
      }
      out.unused.push_back(box);
    }
    if (!whole && (out.dropped || out.empty)) {
      return out;
    }
    const auto named = [&] {
      detail::FaceNamer namer(fuse.Shape());
      for (const Shape* shape : shapes) {
        namer.carry(fuse, *shape, copy);
      }
      namer.finish();
      return namer;
    };
    out.plain.emplace(named());
    fuse.SimplifyResult();
    out.merged.emplace(named());
    return out;
  };
  // The union to take of a candidate: nothing left unused, a solid where
  // the shapes have one, and valid; its faces merged, else as OCCT's union
  // left them where the merge breaks it (as a boolean operation's,
  // boolean.cpp).
  const auto sound = [&](const Fused& candidate) -> const detail::FaceNamer* {
    if (candidate.dropped || candidate.empty || !candidate.merged) {
      return nullptr;
    }
    const detail::FaceNamer* taken = &*candidate.merged;
    if (!detail::is_valid(taken->result())) {
      taken = &*candidate.plain;
      if (!detail::is_valid(taken->result())) {
        return nullptr;
      }
    }
    return taken;
  };
  const auto lost_in = [&](const Fused& candidate) {
    return candidate.empty ||
           (candidate.warned && candidate.plain && material.lost(candidate.plain->result()));
  };
  Fused exact = fused(0.0, false);
  if (const detail::FaceNamer* taken = sound(exact)) {
    return checked(taken->shape());
  }
  const bool wrong = exact.dropped || lost_in(exact);
  // (mitcad#1273) A union whose pieces OCCT left unused, or that lost
  // material, is wrong whatever the tolerance: OCCT's exact union of
  // shapes that meet only within micrometres (two copies of a plate whose
  // side walls lie 1e-4 mm apart; a nut inside a threaded carriage, its
  // bore's seam micrometres from the vertices of the thread's faces)
  // splits their faces with edges left over. Where the unused pieces lie,
  // the shapes whose boxes reach them are united first, group by group of
  // meeting boxes, each such union finding its own fuzzy value (below);
  // the union of all is then made again with those unions in their
  // places, exactly. So the fuzzy value stays where it is needed, and the
  // union of all is made twice, not once for every fuzzy value tried; a
  // union of many whose near misses (above) were the wrong places is made
  // once.
  if (wrong && !exact.unused.empty() && shapes.size() > 2) {
    std::vector<const Shape*> reaching;
    std::vector<const Shape*> others;
    for (const Shape* shape : shapes) {
      Bnd_Box box;
      BRepBndLib::Add(shape->occt(), box);
      box.Enlarge(detail::kUnusedReach);
      bool reaches = false;
      for (const Bnd_Box& place : exact.unused) {
        reaches = reaches || (!place.IsVoid() && !box.IsOut(place));
      }
      (reaches ? reaching : others).push_back(shape);
    }
    if (reaching.size() > 1 && reaching.size() < shapes.size()) {
      std::vector<const Shape*> next = others;
      std::vector<std::vector<std::size_t>> spots;
      for (std::vector<std::size_t>& spot : detail::box_groups(reaching, detail::kUnusedReach)) {
        if (spot.size() == 1) {
          next.push_back(reaching[spot.front()]);
        } else {
          spots.push_back(std::move(spot));
        }
      }
      // (The spots beside each other, mitcad#1278.)
      std::vector<ShapePtr> repaired(spots.size());
      detail::parallel_jobs(spots.size(), [&](std::size_t k) {
        std::vector<const Shape*> members;
        for (const std::size_t i : spots[k]) {
          members.push_back(reaching[i]);
        }
        repaired[k] = united(members, within);
      });
      for (const ShapePtr& spot : repaired) {
        next.push_back(spot.get());
      }
      if (next.size() < shapes.size()) {
        return united(next, within, Start::exact);
      }
    }
  }
  // (mitcad#397) Within the caller's tolerance: the smallest fuzzy value
  // that gives a sound union (faces that lie on each other only within an
  // importer's geometric tolerance); a wrong union tries fuzzy values from
  // kUnusedStart up to kUnusedReach also without one.
  if (wrong || within > 0.0) {
    const double lowest = wrong ? detail::kUnusedStart : kFuzzyStart;
    const double reach = std::max(within, wrong ? detail::kUnusedReach : kFuzzyStart);
    std::vector<double> steps;
    for (double decade = lowest; decade < reach; decade *= 10.0) {
      steps.push_back(decade);
      if (3.0 * decade < reach) {
        steps.push_back(3.0 * decade);
      }
    }
    steps.push_back(reach);
    // An exact union that is only invalid takes, as before, the first
    // valid one where no fuzzy value gives a sound one.
    ShapePtr valid;
    for (const double step : steps) {
      try {
        const Fused fuzzy = fused(step, !wrong);
        if (const detail::FaceNamer* taken = sound(fuzzy)) {
          return checked(taken->shape());
        }
        if (!wrong && !valid && fuzzy.merged) {
          if (detail::is_valid(fuzzy.merged->result())) {
            valid = checked(fuzzy.merged->shape());
          } else if (detail::is_valid(fuzzy.plain->result())) {
            valid = checked(fuzzy.plain->shape());
          }
        }
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
        throw_if_cancelled();
      }
    }
    if (valid) {
      return valid;
    }
  }
  // A union OCCT left pieces of unused that no fuzzy value mends stands
  // as it was when it is valid and holds the shapes' material, as before.
  if (!exact.plain) {
    exact = fused(0.0, true);
  }
  if (lost_in(exact)) {
    throw std::runtime_error(exact.empty ? "the union of the shapes gave an empty result"
                                         : "the union lost material of the shapes it joins");
  }
  const detail::FaceNamer& taken =
      detail::is_valid(exact.merged->result()) ? *exact.merged : *exact.plain;
  detail::require_valid(taken.result(), "the union");
  return checked(taken.shape());
}

} // namespace

ShapePtr unite(const std::vector<const Shape*>& shapes, double within) {
  if (shapes.empty()) {
    throw std::invalid_argument("there are no shapes to unite");
  }
  detail::require_finite("union tolerance", within);
  for (const Shape* shape : shapes) {
    detail::require_surfaces(shape->occt(), "union");
  }
  return detail::run("union", [&] { return united(shapes, within); });
}

FaceTool face_tool(const Shape& body, const std::vector<std::string>& names) {
  if (names.empty()) {
    throw std::invalid_argument("no faces selected");
  }
  detail::require_surfaces(body.occt(), "faces");
  return detail::run("faces", [&] {
    std::vector<int> indices;
    for (const std::string& name : names) {
      const std::vector<int> found = body.find_faces(name);
      if (found.empty()) {
        throw std::invalid_argument(no_face_error(name));
      }
      for (int face : found) {
        if (std::find(indices.begin(), indices.end(), face) == indices.end()) {
          indices.push_back(face);
        }
      }
    }
    // A copy of the faces as the body orients them (outward from its
    // material), so that the caps do not add to the body's edges.
    BRep_Builder builder;
    TopoDS_Compound selection;
    builder.MakeCompound(selection);
    for (int face : indices) {
      builder.Add(selection, body.face(face));
    }
    BRepBuilderAPI_Copy copier(selection);
    std::vector<TopoDS_Face> faces;
    for (TopoDS_Iterator it(copier.Shape()); it.More(); it.Next()) {
      faces.push_back(TopoDS::Face(it.Value()));
    }
    if (faces.size() != indices.size()) {
      throw std::runtime_error("the faces could not be copied");
    }
    TopoDS_Shell shell;
    builder.MakeShell(shell);
    for (const TopoDS_Face& face : faces) {
      builder.Add(shell, face);
    }

    // The openings: edges on one face only (a seam is on its face twice).
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
        owners;
    TopExp::MapShapesAndUniqueAncestors(shell, TopAbs_EDGE, TopAbs_FACE, owners);
    Handle(NCollection_HSequence<TopoDS_Shape>) open_edges = new NCollection_HSequence<TopoDS_Shape>;
    for (int i = 1; i <= owners.Extent(); ++i) {
      const TopoDS_Edge& edge = TopoDS::Edge(owners.FindKey(i));
      const NCollection_List<TopoDS_Shape>& at = owners(i);
      if (at.Extent() == 1 && !BRep_Tool::Degenerated(edge) &&
          !BRep_Tool::IsClosed(edge, TopoDS::Face(at.First()))) {
        open_edges->Append(edge);
      }
    }
    std::vector<TopoDS_Face> caps;
    if (!open_edges->IsEmpty()) {
      const Handle(NCollection_HSequence<TopoDS_Shape>) wires =
          ShapeAnalysis_FreeBounds::ConnectEdgesToWires(open_edges, kTolerance, true);
      for (int i = 1; i <= wires->Length(); ++i) {
        const TopoDS_Wire wire = TopoDS::Wire(wires->Value(i));
        if (!BRep_Tool::IsClosed(wire)) {
          throw std::invalid_argument("the openings of the faces are not closed");
        }
        BRepBuilderAPI_MakeFace maker(wire, true);
        if (!maker.IsDone()) {
          throw std::invalid_argument("the openings of the faces must lie in planes");
        }
        caps.push_back(maker.Face());
      }
      caps = nest(std::move(caps));
    }
    // Each cap uses its edges the other way round from the face next to it.
    for (TopoDS_Face& cap : caps) {
      TopExp_Explorer edge(cap, TopAbs_EDGE);
      if (edge.More() && owners.Contains(edge.Current())) {
        const TopoDS_Shape& neighbour = owners.FindFromKey(edge.Current()).First();
        if (orientation_in(cap, edge.Current()) == orientation_in(neighbour, edge.Current())) {
          cap.Reverse();
        }
      }
      builder.Add(shell, cap);
    }

    // The faces and caps in groups that share edges, a solid of each: faces
    // of separate cut-outs or bosses (mitcad#719) are not one shell.
    std::vector<TopoDS_Shape> members;
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> index_of;
    for (TopoDS_Iterator it(shell); it.More(); it.Next()) {
      members.push_back(it.Value());
      index_of.Add(it.Value());
    }
    std::vector<int> root(members.size());
    std::iota(root.begin(), root.end(), 0);
    const auto find = [&root](int k) {
      while (root[k] != k) {
        root[k] = root[root[k]];
        k = root[k];
      }
      return k;
    };
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
        edge_faces;
    TopExp::MapShapesAndUniqueAncestors(shell, TopAbs_EDGE, TopAbs_FACE, edge_faces);
    for (int i = 1; i <= edge_faces.Extent(); ++i) {
      const NCollection_List<TopoDS_Shape>& at = edge_faces(i);
      const int first = find(index_of.FindIndex(at.First()) - 1);
      for (NCollection_List<TopoDS_Shape>::Iterator it(at); it.More(); it.Next()) {
        const int other = find(index_of.FindIndex(it.Value()) - 1);
        root[other] = first;
      }
    }
    std::vector<std::vector<std::size_t>> groups;
    std::vector<int> group_of(members.size(), -1);
    for (std::size_t k = 0; k < members.size(); ++k) {
      const int r = find(static_cast<int>(k));
      if (group_of[r] < 0) {
        group_of[r] = static_cast<int>(groups.size());
        groups.emplace_back();
      }
      groups[group_of[r]].push_back(k);
    }
    FaceTool result;
    TopoDS_Compound solids;
    builder.MakeCompound(solids);
    TopoDS_Shape tool;
    for (std::size_t g = 0; g < groups.size(); ++g) {
      TopoDS_Shell part;
      builder.MakeShell(part);
      for (std::size_t k : groups[g]) {
        builder.Add(part, members[k]);
      }
      TopoDS_Solid solid;
      builder.MakeSolid(solid);
      builder.Add(solid, part);
      GProp_GProps props;
      BRepGProp::VolumeProperties(solid, props);
      // The faces point away from the body's material: outward of a boss,
      // into a pocket.
      const bool material = props.Mass() > 0.0;
      if (g == 0) {
        result.material = material;
      } else if (material != result.material) {
        throw std::invalid_argument("the faces make both bosses and pockets");
      }
      if (!material) {
        builder.MakeSolid(solid);
        builder.Add(solid, part.Reversed());
      }
      detail::require_valid(solid, "the solid of the faces");
      builder.Add(solids, solid);
      tool = solid;
    }
    if (groups.size() > 1) {
      tool = solids;
    }
    detail::FaceNamer namer(tool);
    for (std::size_t k = 0; k < faces.size(); ++k) {
      for (const std::string& name : body.face_names(indices[k])) {
        namer.add(faces[k], name);
      }
    }
    namer.finish();
    result.tool = namer.shape();
    return result;
  });
}

} // namespace mitcad::geometry
