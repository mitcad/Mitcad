// SPDX-License-Identifier: MIT
#include "mitcad/geometry/boundary_fill.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BOPAlgo_CellsBuilder.hxx>
#include <BOPAlgo_MakerVolume.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElSLib.hxx>
#include <GProp_GProps.hxx>
#include <IntAna_Int3Pln.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Message_ProgressRange.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_State.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/user_error.hpp"

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// A point as agents read it: "(10, 5, 2.5)" (mitcad#1612: the person reads
// "a point", the point painted).
std::string point_text(const gp_Pnt& p) {
  char text[128];
  std::snprintf(text, sizeof text, "(%g, %g, %g)", p.X(), p.Y(), p.Z());
  return text;
}

// Classifies points against the solids of one tool (each classifier set up
// once).
class ToolSolids {
public:
  explicit ToolSolids(const TopoDS_Shape& shape) {
    for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
      m_classifiers.push_back(std::make_unique<BRepClass3d_SolidClassifier>(solid.Current()));
    }
  }

  bool empty() const { return m_classifiers.empty(); }

  // IN inside a solid, else ON on the boundary of one, else OUT.
  TopAbs_State state(const gp_Pnt& point) {
    TopAbs_State found = TopAbs_OUT;
    for (const auto& classifier : m_classifiers) {
      classifier->Perform(point, Precision::Confusion());
      if (classifier->State() == TopAbs_IN) {
        return TopAbs_IN;
      }
      if (classifier->State() == TopAbs_ON) {
        found = TopAbs_ON;
      }
    }
    return found;
  }

private:
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> m_classifiers;
};

// The cell with the pieces of a face that it has side by side merged
// again (OCCT's ShapeUpgrade_UnifySameDomain), the names of `pieces`
// carried over; null when the merge fails or leaves an invalid shape.
// `pieces` is not finished: a merged face takes its pieces' names once.
ShapePtr merged(const detail::FaceNamer& pieces) {
  try {
    OCC_CATCH_SIGNALS
    ShapeUpgrade_UnifySameDomain unify(pieces.result(), true, true, false);
    unify.Build();
    const TopoDS_Shape result = unify.Shape();
    if (result.IsNull() || !detail::is_valid(result)) {
      return nullptr;
    }
    const occ::handle<BRepTools_History> history = unify.History();
    detail::FaceNamer namer(result);
    namer.adopt(pieces, [&](const TopoDS_Shape& face) -> TopoDS_Shape {
      if (history.IsNull()) {
        return TopoDS_Shape();
      }
      const NCollection_List<TopoDS_Shape>& images = history->Modified(face);
      return images.IsEmpty() ? TopoDS_Shape() : images.First();
    });
    namer.finish();
    namer.set_unified(true);
    namer.set_valid(true); // checked above
    return namer.shape();
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

} // namespace

std::vector<BoundaryCell> boundary_cells(const std::vector<const Shape*>& tools,
                                         const std::vector<gp_Pnt>& points) {
  return detail::run("boundary fill", [&] {
    if (tools.empty()) {
      throw std::runtime_error("no tool bodies");
    }
    std::vector<TopoDS_Shape> inputs;
    for (const Shape* tool : tools) {
      detail::require_surfaces(tool->occt(), "boundary fill");
      inputs.push_back(tool->occt());
    }
    // OCCT's general fuse writes into the sub-shapes of its arguments: it
    // works on copies of the tools (history.hpp).
    const detail::InputCopy copy(inputs);

    // The tools that hold each point.
    std::vector<ToolSolids> solids;
    for (std::size_t i = 0; i < tools.size(); ++i) {
      solids.emplace_back(copy.shape(i));
      if (solids.back().empty()) {
        throw std::runtime_error("tool " + std::to_string(i + 1) + " has no solid");
      }
    }
    std::vector<std::vector<std::size_t>> holders(points.size());
    for (std::size_t p = 0; p < points.size(); ++p) {
      for (std::size_t i = 0; i < tools.size(); ++i) {
        const TopAbs_State state = solids[i].state(points[p]);
        if (state == TopAbs_ON) {
          const std::string tool = " lies on the boundary of tool " + std::to_string(i + 1);
          throw std::runtime_error(
              placed_error("a point" + tool, "the point " + point_text(points[p]) + tool, {point_place(points[p])}));
        }
        if (state == TopAbs_IN) {
          holders[p].push_back(i);
        }
      }
      if (holders[p].empty()) {
        const std::string none = " is inside no tool body: no enclosed cell there";
        throw std::runtime_error(
            placed_error("a point" + none, "the point " + point_text(points[p]) + none, {point_place(points[p])}));
      }
    }

    std::vector<BoundaryCell> cells;
    cells.reserve(points.size());
    // One tool: its one cell is all of it.
    if (tools.size() == 1) {
      const ShapePtr whole = detail::working_copy(*tools.front());
      for (std::size_t p = 0; p < points.size(); ++p) {
        cells.push_back({whole, holders[p]});
      }
      return cells;
    }

    // The tools' faces divide space into parts once; a cell is the parts
    // inside the tools that hold its point and outside the others.
    BOPAlgo_CellsBuilder builder;
    for (std::size_t i = 0; i < tools.size(); ++i) {
      builder.AddArgument(copy.shape(i));
    }
    builder.SetNonDestructive(true);
    builder.SetRunParallel(true);
    detail::interruptible([&](const Message_ProgressRange& range) { builder.Perform(range); });
    if (builder.HasErrors()) {
      throw std::runtime_error("the tool bodies could not be divided into cells");
    }
    std::map<std::vector<std::size_t>, ShapePtr> made;
    for (std::size_t p = 0; p < points.size(); ++p) {
      const auto known = made.find(holders[p]);
      if (known != made.end()) {
        cells.push_back({known->second, holders[p]});
        continue;
      }
      NCollection_List<TopoDS_Shape> take;
      NCollection_List<TopoDS_Shape> avoid;
      std::size_t next = 0;
      for (std::size_t i = 0; i < tools.size(); ++i) {
        const bool holds = next < holders[p].size() && holders[p][next] == i;
        if (holds) {
          ++next;
          take.Append(copy.shape(i));
        } else {
          avoid.Append(copy.shape(i));
        }
      }
      builder.RemoveAllFromResult();
      builder.AddToResult(take, avoid);
      const TopoDS_Shape found = builder.Shape();
      if (found.IsNull() || !TopExp_Explorer(found, TopAbs_SOLID).More()) {
        throw std::runtime_error(placed_error("the cell at a point came out empty",
                                              "the cell at " + point_text(points[p]) + " came out empty",
                                              {point_place(points[p])}));
      }
      // Each face of the cell takes the names of the tool faces it is a
      // piece of, by the history of the parts the cell kept.
      const occ::handle<BRepTools_History> history = builder.History();
      if (history.IsNull()) {
        throw std::runtime_error("the division into cells has no history");
      }
      detail::FaceNamer pieces(found);
      for (const Shape* tool : tools) {
        for (int f = 0; f < tool->face_count(); ++f) {
          const TopoDS_Shape face = copy.of(tool->face(f));
          if (history->IsRemoved(face)) {
            continue;
          }
          const NCollection_List<TopoDS_Shape>& images = history->Modified(face);
          if (images.IsEmpty()) {
            pieces.add(face, tool->face_names(f), -1);
          }
          for (const TopoDS_Shape& image : images) {
            pieces.add(image, tool->face_names(f), -1);
          }
        }
      }
      ShapePtr cell = merged(pieces);
      if (!cell) {
        pieces.finish();
        cell = pieces.shape();
      }
      made.emplace(holders[p], cell);
      cells.push_back({cell, holders[p]});
    }
    return cells;
  });
}

// --- Surface bodies and planes as tools (mitcad#2149)

namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The box the planes are cut to: of the tools and the points (with planes
// alone, of the points where three planes cross), grown by a tenth of its
// size and at least 1 mm.
Bnd_Box span_of(const std::vector<TopoDS_Shape>& tools, const std::vector<gp_Pln>& planes,
                const std::vector<gp_Pnt>& points) {
  Bnd_Box box;
  for (const TopoDS_Shape& tool : tools) {
    BRepBndLib::Add(tool, box);
  }
  for (const gp_Pnt& point : points) {
    box.Add(point);
  }
  if (tools.empty()) {
    for (std::size_t i = 0; i < planes.size(); ++i) {
      for (std::size_t j = i + 1; j < planes.size(); ++j) {
        for (std::size_t k = j + 1; k < planes.size(); ++k) {
          const IntAna_Int3Pln crossing(planes[i], planes[j], planes[k]);
          if (crossing.IsDone() && !crossing.IsEmpty()) {
            box.Add(crossing.Value());
          }
        }
      }
    }
  }
  if (box.IsVoid()) {
    for (const gp_Pln& plane : planes) {
      box.Add(plane.Location());
    }
  }
  if (box.IsVoid()) {
    throw std::runtime_error("nothing to bound: no tools, planes or points");
  }
  box.Enlarge(std::max(0.1 * std::sqrt(box.SquareExtent()), 1.0));
  return box;
}

// The plane cut to a square about the box's middle that reaches past every
// corner of the box.
TopoDS_Face plane_face(const gp_Pln& plane, const Bnd_Box& box) {
  const gp_Pnt middle((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
  const double reach = std::sqrt(box.SquareExtent());
  double u = 0.0;
  double v = 0.0;
  ElSLib::Parameters(plane, middle, u, v);
  BRepBuilderAPI_MakeFace make(plane, u - reach, u + reach, v - reach, v + reach);
  if (!make.IsDone()) {
    throw std::runtime_error("a plane could not be cut to the tools' box");
  }
  return make.Face();
}

// All the faces of the shapes divided once and every part of space they
// close off made a solid (OCCT's volume maker), faces that close nothing
// off left out.
struct Division {
  TopoDS_Shape result;
  occ::handle<BRepTools_History> history;
};

Division divide(const std::vector<TopoDS_Shape>& shapes) {
  BOPAlgo_MakerVolume maker;
  for (const TopoDS_Shape& shape : shapes) {
    maker.AddArgument(shape);
  }
  maker.SetIntersect(true);
  maker.SetAvoidInternalShapes(true);
  maker.SetNonDestructive(true);
  maker.SetRunParallel(true);
  detail::interruptible([&](const Message_ProgressRange& range) { maker.Perform(range); });
  if (maker.HasErrors()) {
    throw std::runtime_error("the faces of the tools could not be divided into regions");
  }
  return {maker.Shape(), maker.History()};
}

std::vector<TopoDS_Shape> solids_of(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Shape> solids;
  if (shape.IsNull()) {
    return solids;
  }
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    solids.push_back(solid.Current());
  }
  return solids;
}

// A point inside the solid: its centre of mass where that is inside, else
// half way from a point of one of its faces to the next face along the
// normal into it; none when no such point is found.
std::optional<gp_Pnt> inside_point(const TopoDS_Shape& solid) {
  BRepClass3d_SolidClassifier classifier(solid);
  const auto inside = [&classifier](const gp_Pnt& point) {
    classifier.Perform(point, Precision::Confusion());
    return classifier.State() == TopAbs_IN;
  };
  GProp_GProps properties;
  BRepGProp::VolumeProperties(solid, properties);
  if (std::abs(properties.Mass()) > Precision::Confusion() && inside(properties.CentreOfMass())) {
    return properties.CentreOfMass();
  }
  IntCurvesFace_ShapeIntersector shooter;
  shooter.Load(solid, Precision::Confusion());
  constexpr int kSteps = 3;
  for (TopExp_Explorer explorer(solid, TopAbs_FACE); explorer.More(); explorer.Next()) {
    const TopoDS_Face& face = TopoDS::Face(explorer.Current());
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    BRepTopAdaptor_FClass2d in_face(face, Precision::PConfusion());
    const BRepAdaptor_Surface surface(face);
    for (int a = 0; a < kSteps; ++a) {
      for (int b = 0; b < kSteps; ++b) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * (a + 0.5) / kSteps, v0 + (v1 - v0) * (b + 0.5) / kSteps);
        if (in_face.Perform(uv) != TopAbs_IN) {
          continue;
        }
        gp_Pnt at;
        gp_Vec du;
        gp_Vec dv;
        surface.D1(uv.X(), uv.Y(), at, du, dv);
        const gp_Vec normal = du.Crossed(dv);
        if (normal.Magnitude() < 1e-12) {
          continue;
        }
        // Into the solid against the face's outward normal, and the other
        // way should the face be turned.
        for (const double way : {-1.0, 1.0}) {
          const double sign = face.Orientation() == TopAbs_REVERSED ? -way : way;
          const gp_Dir into(normal * sign);
          shooter.Perform(gp_Lin(at, into), Precision::Confusion(), Precision::Infinite());
          double nearest = Precision::Infinite();
          for (int i = 1; i <= shooter.NbPnt(); ++i) {
            const double w = shooter.WParameter(i);
            if (w > 10.0 * Precision::Confusion() && w < nearest) {
              nearest = w;
            }
          }
          if (nearest >= Precision::Infinite()) {
            continue;
          }
          const gp_Pnt middle = at.Translated(gp_Vec(into) * (nearest / 2.0));
          if (inside(middle)) {
            return middle;
          }
        }
      }
    }
  }
  return std::nullopt;
}

// Whether the point lies on one of the shapes' faces (within their
// tolerance, at least 1e-5 mm).
bool on_faces(const gp_Pnt& point, const std::vector<TopoDS_Shape>& shapes) {
  const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
  for (const TopoDS_Shape& shape : shapes) {
    for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
      BRepExtrema_DistShapeShape distance(vertex, face.Current());
      if (distance.IsDone() &&
          distance.Value() <= std::max(1.0e-5, BRep_Tool::Tolerance(TopoDS::Face(face.Current())))) {
        return true;
      }
    }
  }
  return false;
}

// Where a region open at `point` leaks out: the tools and planes divided
// again inside a box a little wider than `box`, whose faces close off what
// they leave open, and points along the tools' free edges (where the
// surfaces end) that lie in or on the part of the box around the point; at
// most 24, spread along them.
std::vector<gp_Pnt> gap_of(const gp_Pnt& point, const std::vector<TopoDS_Shape>& tools,
                           const std::vector<gp_Pln>& planes, const Bnd_Box& box) {
  Bnd_Box wider = box;
  wider.Enlarge(0.05 * std::sqrt(box.SquareExtent()) + 1.0);
  std::vector<TopoDS_Shape> shapes = tools;
  for (const gp_Pln& plane : planes) {
    shapes.push_back(plane_face(plane, wider));
  }
  shapes.push_back(BRepPrimAPI_MakeBox(wider.CornerMin(), wider.CornerMax()).Shell());
  const Division division = divide(shapes);
  std::unique_ptr<BRepClass3d_SolidClassifier> around;
  for (const TopoDS_Shape& solid : solids_of(division.result)) {
    auto classifier = std::make_unique<BRepClass3d_SolidClassifier>(solid);
    classifier->Perform(point, Precision::Confusion());
    if (classifier->State() == TopAbs_IN) {
      around = std::move(classifier);
      break;
    }
  }
  std::vector<gp_Pnt> found;
  if (!around) {
    return found;
  }
  for (const TopoDS_Shape& tool : tools) {
    Ancestors ancestors;
    TopExp::MapShapesAndAncestors(tool, TopAbs_EDGE, TopAbs_FACE, ancestors);
    for (int e = 1; e <= ancestors.Extent(); ++e) {
      const TopoDS_Edge& edge = TopoDS::Edge(ancestors.FindKey(e));
      if (ancestors.FindFromIndex(e).Extent() != 1 || BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      for (const double t : {0.1, 0.3, 0.5, 0.7, 0.9}) {
        const gp_Pnt at = curve.Value(curve.FirstParameter() + t * (curve.LastParameter() - curve.FirstParameter()));
        around->Perform(at, Precision::Confusion());
        if (around->State() == TopAbs_IN || around->State() == TopAbs_ON) {
          found.push_back(at);
        }
      }
    }
  }
  constexpr std::size_t kMost = 24;
  if (found.size() > kMost) {
    std::vector<gp_Pnt> spread;
    for (std::size_t i = 0; i < kMost; ++i) {
      spread.push_back(found[i * found.size() / kMost]);
    }
    found = std::move(spread);
  }
  return found;
}

// The region with the tools' and planes' names on its faces (as
// boundary_cells names a cell's).
ShapePtr named_region(const TopoDS_Shape& solid, const Division& division, const std::vector<const Shape*>& tools,
                      const detail::InputCopy& copy, const std::vector<TopoDS_Face>& plane_faces,
                      const std::vector<std::string>& plane_names) {
  detail::FaceNamer pieces(solid);
  const auto add = [&](const TopoDS_Shape& face, const NameList& names) {
    if (division.history.IsNull() || division.history->IsRemoved(face)) {
      return;
    }
    const NCollection_List<TopoDS_Shape>& images = division.history->Modified(face);
    if (images.IsEmpty()) {
      pieces.add(face, names, -1);
    }
    for (const TopoDS_Shape& image : images) {
      pieces.add(image, names, -1);
    }
  };
  for (const Shape* tool : tools) {
    for (int f = 0; f < tool->face_count(); ++f) {
      add(copy.of(tool->face(f)), tool->face_names(f));
    }
  }
  for (std::size_t i = 0; i < plane_faces.size(); ++i) {
    add(plane_faces[i], NameList{i < plane_names.size() ? plane_names[i] : std::string()});
  }
  ShapePtr region = merged(pieces);
  if (!region) {
    pieces.finish();
    region = pieces.shape();
  }
  return region;
}

} // namespace

BoundaryRegions boundary_regions(const std::vector<const Shape*>& tools, const std::vector<gp_Pln>& planes,
                                 const std::vector<std::string>& plane_names, const std::vector<gp_Pnt>& points) {
  return detail::run("boundary fill", [&] {
    if (tools.empty() && planes.empty()) {
      throw std::runtime_error("no tool bodies or planes");
    }
    std::vector<TopoDS_Shape> inputs;
    for (const Shape* tool : tools) {
      detail::require_surfaces(tool->occt(), "boundary fill");
      if (!TopExp_Explorer(tool->occt(), TopAbs_FACE).More()) {
        throw std::runtime_error("a tool has no faces");
      }
      inputs.push_back(tool->occt());
    }
    // OCCT's general fuse writes into the sub-shapes of its arguments: it
    // works on copies of the tools (history.hpp).
    const detail::InputCopy copy(inputs);
    std::vector<TopoDS_Shape> copies;
    for (std::size_t i = 0; i < tools.size(); ++i) {
      copies.push_back(copy.shape(i));
    }
    const Bnd_Box box = span_of(copies, planes, points);
    std::vector<TopoDS_Face> plane_faces;
    for (const gp_Pln& plane : planes) {
      plane_faces.push_back(plane_face(plane, box));
    }
    std::vector<TopoDS_Shape> shapes = copies;
    shapes.insert(shapes.end(), plane_faces.begin(), plane_faces.end());
    const Division division = divide(shapes);

    // The regions with their points, in the order of the points.
    struct Found {
      TopoDS_Shape solid;
      gp_Pnt point;
    };
    std::vector<Found> found;
    for (const TopoDS_Shape& solid : solids_of(division.result)) {
      if (const std::optional<gp_Pnt> point = inside_point(solid)) {
        found.push_back({solid, *point});
      }
    }
    const auto before = [](const gp_Pnt& a, const gp_Pnt& b) {
      constexpr double kSame = 1e-6;
      if (std::abs(a.X() - b.X()) > kSame) {
        return a.X() < b.X();
      }
      if (std::abs(a.Y() - b.Y()) > kSame) {
        return a.Y() < b.Y();
      }
      return a.Z() < b.Z() - kSame;
    };
    std::stable_sort(found.begin(), found.end(),
                     [&before](const Found& a, const Found& b) { return before(a.point, b.point); });

    BoundaryRegions result;
    std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> classifiers;
    for (const Found& region : found) {
      result.regions.push_back(
          {named_region(region.solid, division, tools, copy, plane_faces, plane_names), region.point});
      classifiers.push_back(std::make_unique<BRepClass3d_SolidClassifier>(region.solid));
    }
    std::vector<TopoDS_Shape> faces = copies;
    faces.insert(faces.end(), plane_faces.begin(), plane_faces.end());
    for (const gp_Pnt& point : points) {
      BoundaryPointPlace place;
      for (std::size_t r = 0; r < classifiers.size(); ++r) {
        classifiers[r]->Perform(point, Precision::Confusion());
        const TopAbs_State state = classifiers[r]->State();
        if (state == TopAbs_IN) {
          place.kind = BoundaryPointPlace::Kind::Region;
          place.region = r;
          break;
        }
        if (state == TopAbs_ON) {
          place.kind = BoundaryPointPlace::Kind::OnFace;
          break;
        }
      }
      if (place.kind == BoundaryPointPlace::Kind::Open) {
        if (on_faces(point, faces)) {
          place.kind = BoundaryPointPlace::Kind::OnFace;
        } else {
          place.gap = gap_of(point, copies, planes, box);
        }
      }
      result.places.push_back(std::move(place));
    }
    return result;
  });
}

} // namespace mitcad::geometry
