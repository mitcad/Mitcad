// SPDX-License-Identifier: MIT
#include "view/ViewEdges.hpp"

#include <map>
#include <set>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedMap.hxx>
#include <OSD_Parallel.hxx>
#include <Select3D_SensitiveEntity.hxx>
#include <SelectMgr_SensitiveEntity.hxx>
#include <Standard_Failure.hxx>
#include <StdPrs_WFShape.hxx>
#include <StdSelect_BRepOwner.hxx>
#include <StdSelect_Shape.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>

#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/smooth_seams.hpp"

namespace mitcad {

occ::handle<Poly_Polygon3D> viewEdgePolygon(const TopoDS_Edge& edge) {
  if (edge.IsNull() || BRep_Tool::Degenerated(edge) || !BRep_Tool::IsGeometric(edge)) {
    return {};
  }
  try {
    const BRepAdaptor_Curve curve(edge);
    double deviation = kViewEdgeDeviation;
    constexpr int kRounds = 12;
    for (int round = 0; round < kRounds; ++round, deviation *= 4.0) {
      const GCPnts_TangentialDeflection points(curve, kViewEdgeAngle, deviation, 2, 1.0e-9, 1.0e-7);
      const int count = points.NbPoints();
      if (count < 2) {
        return {};
      }
      if (count > kViewEdgeMaxPoints && round + 1 < kRounds) {
        continue; // a curve of metres: coarser
      }
      NCollection_Array1<gp_Pnt> nodes(1, count);
      NCollection_Array1<double> parameters(1, count);
      for (int i = 1; i <= count; ++i) {
        nodes(i) = points.Value(i);
        parameters(i) = points.Parameter(i);
      }
      occ::handle<Poly_Polygon3D> polygon = new Poly_Polygon3D(nodes, parameters);
      polygon->Deflection(deviation);
      return polygon;
    }
  } catch (const Standard_Failure&) {
    // Drawn from its curve by the presentation, as before.
  }
  return {};
}

void giveViewEdgePolygons(const TopoDS_Shape& own) {
  if (own.IsNull()) {
    return;
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(own, TopAbs_EDGE, edges);
  BRep_Builder builder;
  for (int i = 1; i <= edges.Extent(); ++i) {
    // In the edge's own frame: its other places take it moved with them.
    const TopoDS_Edge bare = TopoDS::Edge(edges(i).Located(TopLoc_Location()));
    const occ::handle<Poly_Polygon3D> polygon = viewEdgePolygon(bare);
    if (!polygon.IsNull()) {
      builder.UpdateEdge(bare, polygon);
    }
  }
}

namespace {

// A new edge on the edge's curve and range (unlocated, forward) with its
// polyline as its 3D polygon; null when the edge has no curve or the new
// edge cannot be made (it is then drawn as it is). Only reads the edge.
TopoDS_Edge detachedCopy(const TopoDS_Edge& edge) {
  const TopoDS_Edge bare = TopoDS::Edge(edge.Located(TopLoc_Location()).Oriented(TopAbs_FORWARD));
  if (BRep_Tool::Degenerated(bare)) {
    return {};
  }
  try {
    double first = 0.0;
    double last = 0.0;
    // The curve in the edge's frame (transformed into a new one when the
    // edge keeps it with a location of its own).
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(bare, first, last);
    if (curve.IsNull()) {
      return {};
    }
    BRepBuilderAPI_MakeEdge maker(curve, first, last);
    if (!maker.IsDone()) {
      return {};
    }
    TopoDS_Edge copy = maker.Edge();
    const occ::handle<Poly_Polygon3D> polygon = viewEdgePolygon(copy);
    if (polygon.IsNull()) {
      return {};
    }
    BRep_Builder().UpdateEdge(copy, polygon);
    return copy;
  } catch (const Standard_Failure&) {
    return {};
  }
}

// The compound drawn for `shape`: each edge's copy where and as the shape
// has the edge (`copyOf` null: the edge itself), but the edges `leftOut`
// takes, and the free vertices; `count` the edges in it.
template <class CopyOf, class LeftOut>
TopoDS_Shape drawnCompound(const TopoDS_Shape& shape, const CopyOf& copyOf, const LeftOut& leftOut,
                           std::size_t& count) {
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  count = 0;
  if (shape.IsNull()) {
    return compound;
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Shape& edge = edges(i);
    if (leftOut(edge)) {
      continue;
    }
    ++count;
    const TopoDS_Edge copy = copyOf(edge);
    builder.Add(compound, copy.IsNull() ? edge : copy.Located(edge.Location()).Oriented(edge.Orientation()));
  }
  for (TopExp_Explorer vertex(shape, TopAbs_VERTEX, TopAbs_EDGE); vertex.More(); vertex.Next()) {
    builder.Add(compound, vertex.Current());
  }
  return compound;
}

} // namespace

ViewEdges::ViewEdges(const TopoDS_Shape& shape, const std::vector<const ViewEdges*>& caches,
                     const geometry::Shape* named)
    : m_source(shape) {
  if (shape.IsNull()) {
    return;
  }
  // The smooth seams (mitcad#1773): a cache of the same shape's (the body
  // a highlight of the whole of it is drawn over) knows them already.
  const ViewEdges* same = nullptr;
  if (named == nullptr) {
    for (const ViewEdges* cache : caches) {
      if (cache != nullptr && cache->source().IsPartner(shape)) {
        same = cache;
        break;
      }
    }
  }
  if (same != nullptr) {
    m_seams = same->m_seams;
  } else {
    const geometry::ShapeMap seams = geometry::smooth_seams(shape, named);
    for (int i = 1; i <= seams.Extent(); ++i) {
      m_seams.insert(seams(i).TShape().get());
    }
  }
  // Each edge once (its TShape), wherever the shape has it.
  std::vector<TopoDS_Edge> fresh;
  std::set<const TopoDS_TShape*> seen;
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const TopoDS_TShape* key = edge.TShape().get();
    if (!seen.insert(key).second) {
      continue;
    }
    TopoDS_Edge known;
    for (const ViewEdges* cache : caches) {
      if (cache != nullptr) {
        known = cache->copyOf(edge);
        if (!known.IsNull()) {
          break;
        }
      }
    }
    if (!known.IsNull()) {
      m_copies.emplace(key, known);
    } else {
      fresh.push_back(edge);
    }
  }
  // The new copies on all cores: each task reads its own edge and makes
  // new edges only.
  std::vector<TopoDS_Edge> made(fresh.size());
  OSD_Parallel::For(
      0, static_cast<int>(fresh.size()),
      [&fresh, &made](int i) { made[static_cast<std::size_t>(i)] = detachedCopy(fresh[static_cast<std::size_t>(i)]); },
      fresh.size() < 64);
  for (std::size_t i = 0; i < fresh.size(); ++i) {
    if (!made[i].IsNull()) {
      m_copies.emplace(fresh[i].TShape().get(), made[i]);
    }
  }
  m_drawn = drawnCompound(
      shape, [this](const TopoDS_Shape& edge) { return copyOf(edge); },
      [this](const TopoDS_Shape& edge) { return smoothSeam(edge); }, m_drawnCount);
}

bool ViewEdges::smoothSeam(const TopoDS_Shape& edge) const {
  return !edge.IsNull() && !m_seams.empty() && m_seams.count(edge.TShape().get()) > 0;
}

TopoDS_Edge ViewEdges::copyOf(const TopoDS_Shape& edge) const {
  if (edge.IsNull()) {
    return {};
  }
  const auto found = m_copies.find(edge.TShape().get());
  return found != m_copies.end() ? found->second : TopoDS_Edge();
}

std::pair<std::size_t, std::size_t> ViewEdges::sharedWith(const ViewEdges& body) const {
  std::size_t both = 0;
  std::size_t shared = 0;
  for (const auto& [key, copy] : m_copies) {
    const auto found = body.m_copies.find(key);
    if (found == body.m_copies.end()) {
      continue;
    }
    ++both;
    if (found->second.TShape() == copy.TShape()) {
      ++shared;
    }
  }
  return {both, shared};
}

void drawViewEdges(const occ::handle<Prs3d_Presentation>& presentation, const TopoDS_Shape& drawn,
                   const occ::handle<Prs3d_Drawer>& drawer, const occ::handle<Prs3d_LineAspect>& aspect) {
  if (drawn.IsNull()) {
    return;
  }
  // Never meshed: the copies carry their polylines (an edge drawn as it is
  // has the meshes a job made).
  occ::handle<Prs3d_Drawer> lines = new Prs3d_Drawer();
  lines->SetLink(drawer);
  lines->SetAutoTriangulation(false);
  if (!aspect.IsNull()) {
    lines->SetWireAspect(aspect);
    lines->SetFreeBoundaryAspect(aspect);
    lines->SetUnFreeBoundaryAspect(aspect);
  }
  try {
    StdPrs_WFShape::Add(presentation, drawn, lines);
  } catch (const Standard_Failure&) {
    // Nothing drawn rather than a view that fails.
  }
}

namespace {

// An owner of a decomposed face or edge whose highlight (the hover, OCCT's
// own selection highlight) is drawn with the lines of the copies: its
// presentation (StdSelect_BRepOwner's myPrsSh) made here before the base
// class would make one of the model's shape.
class DrawnPartOwner : public StdSelect_BRepOwner {
public:
  DrawnPartOwner(const occ::handle<StdSelect_BRepOwner>& owner, std::shared_ptr<const ViewEdges> edges)
      : StdSelect_BRepOwner(owner->Shape(), owner->Priority(), owner->ComesFromDecomposition()),
        m_edges(std::move(edges)) {
    SetSelectable(owner->Selectable());
  }

  void HilightWithColor(const occ::handle<PrsMgr_PresentationManager>& manager,
                        const occ::handle<Prs3d_Drawer>& style, const int mode) override {
    if (!myPrsSh.IsNull() && myPrsSh->ToBeUpdated(true)) {
      myPrsSh.Nullify();
    }
    if (myPrsSh.IsNull() && m_edges) {
      const std::vector<const ViewEdges*> caches{m_edges.get()};
      myPrsSh = new FineLines<StdSelect_Shape>(std::make_shared<const ViewEdges>(Shape(), caches), Shape(), style);
    }
    StdSelect_BRepOwner::HilightWithColor(manager, style, mode);
  }

private:
  std::shared_ptr<const ViewEdges> m_edges;
};

} // namespace

void dropSmoothSeams(const occ::handle<SelectMgr_Selection>& selection, const ViewEdges& edges) {
  if (selection.IsNull() || edges.seamCount() == 0) {
    return;
  }
  // The entities kept, in their order; the rest are let go as they are
  // (SelectMgr_Selection::Clear would also clear the kept ones' owners).
  NCollection_DynamicArray<occ::handle<SelectMgr_SensitiveEntity>>& entities = selection->ChangeEntities();
  std::vector<occ::handle<SelectMgr_SensitiveEntity>> kept;
  kept.reserve(static_cast<std::size_t>(entities.Length()));
  for (int i = 0; i < entities.Length(); ++i) {
    const occ::handle<SelectMgr_SensitiveEntity>& entity = entities.Value(i);
    if (!entity.IsNull() && !entity->BaseSensitive().IsNull()) {
      const occ::handle<StdSelect_BRepOwner> owner =
          occ::handle<StdSelect_BRepOwner>::DownCast(entity->BaseSensitive()->OwnerId());
      if (!owner.IsNull() && owner->ComesFromDecomposition() && owner->HasShape() &&
          owner->Shape().ShapeType() == TopAbs_EDGE && edges.smoothSeam(owner->Shape())) {
        continue;
      }
    }
    kept.push_back(entity);
  }
  if (kept.size() == static_cast<std::size_t>(entities.Length())) {
    return;
  }
  entities.Clear();
  for (const occ::handle<SelectMgr_SensitiveEntity>& entity : kept) {
    entities.Append(entity);
  }
}

void drawOwnersFrom(const occ::handle<SelectMgr_Selection>& selection, const std::shared_ptr<const ViewEdges>& edges) {
  if (selection.IsNull() || !edges) {
    return;
  }
  // One new owner for each owner of the selection (its entities may share one).
  std::map<const SelectMgr_EntityOwner*, occ::handle<SelectMgr_EntityOwner>> replaced;
  const auto& entities = selection->Entities();
  for (int i = 0; i < entities.Length(); ++i) {
    const occ::handle<SelectMgr_SensitiveEntity>& entity = entities.Value(i);
    if (entity.IsNull() || entity->BaseSensitive().IsNull()) {
      continue;
    }
    const occ::handle<Select3D_SensitiveEntity>& sensitive = entity->BaseSensitive();
    const occ::handle<StdSelect_BRepOwner> owner = occ::handle<StdSelect_BRepOwner>::DownCast(sensitive->OwnerId());
    if (owner.IsNull() || !owner->ComesFromDecomposition() || !owner->HasShape()) {
      continue;
    }
    const TopAbs_ShapeEnum type = owner->Shape().ShapeType();
    if (type != TopAbs_EDGE && type != TopAbs_FACE) {
      continue;
    }
    auto known = replaced.find(owner.get());
    if (known == replaced.end()) {
      known = replaced.emplace(owner.get(), new DrawnPartOwner(owner, edges)).first;
    }
    sensitive->Set(known->second);
  }
}

} // namespace mitcad
