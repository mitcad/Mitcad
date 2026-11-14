// SPDX-License-Identifier: MIT
#pragma once

#include <cstddef>
#include <memory>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

#include <AIS_DisplayMode.hxx>
#include <Poly_Polygon3D.hxx>
#include <Prs3d_Drawer.hxx>
#include <Prs3d_LineAspect.hxx>
#include <Prs3d_Presentation.hxx>
#include <PrsMgr_PresentationManager.hxx>
#include <SelectMgr_Selection.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_TShape.hxx>

namespace mitcad::geometry {
class Shape;
} // namespace mitcad::geometry

namespace mitcad {

// The view's lines (mitcad#1633): a body's edges, in every visual style,
// the edges and face boundaries of its highlights, of the hover and of the
// hover chain, sketch curves, projected curves and the profiles' outlines
// are all drawn from polylines made with these parameters, whatever the
// size of the shape they belong to: a chordal error of kViewEdgeDeviation
// and at most kViewEdgeAngle a step (a circle in at least 90 pieces a
// turn, a large one finer by the deviation). The same curve so gives the
// same polyline wherever it is drawn, and two curves that lie on each
// other (a projected curve on the body's edge) part by less than twice
// the deviation, under a pixel up to about 1 000 pixels a millimetre. An
// edge whose polyline would take more than kViewEdgeMaxPoints points (a
// circle of metres) is cut with a deviation four times as large, as often
// as that takes.
//
// Before, the lines were the edges' polygons on the faces' display meshes
// (a body's deviation by its size, up to 0.5 mm, mitcad#1275, and small
// circles in as few as 12 pieces a turn, mitcad#879), or polygons the
// presentations meshed on the UI thread with a deviation by the size of
// the shape drawn: at a deep zoom the round edges were polylines, and a
// highlight or a projected curve ran beside the line under it. Faces keep
// their display meshes (edges are cheap, faces are not).
//
// The model's shapes are never written here (mitcad#1553): a body's edges
// are drawn from detached copies (ViewEdges), new edges on the same curves
// that carry the polylines; only the view's own shapes (sketch entities,
// built from the sketch query's numbers, mitcad#1586) get their polylines
// in place (giveViewEdgePolygons).
constexpr double kViewEdgeDeviation = 0.001;
constexpr double kViewEdgeAngle = 4.0 * 3.14159265358979323846 / 180.0;
constexpr int kViewEdgeMaxPoints = 20000;

// An edge's polyline with the view's line parameters, in the coordinates
// of the edge as it is located; null when the edge has no curve
// (degenerated, or a mesh's edge).
occ::handle<Poly_Polygon3D> viewEdgePolygon(const TopoDS_Edge& edge);

// Gives every edge of a shape of the view's own its polyline as its 3D
// polygon (only for shapes no model thread reads: sketch entities).
void giveViewEdgePolygons(const TopoDS_Shape& own);

// Detached copies of a shape's edges for drawing (mitcad#1633): each edge
// with a curve as a new edge on the same curve and range with its
// polyline (viewEdgePolygon) as its 3D polygon and nothing else, so that
// drawing it reads no record of the model's edge and meshes nothing; an
// edge without a curve (a mesh body's) drawn as it is, from the meshes it
// has. Copies are taken from `caches` where one already has the edge (a
// shown body's, so that a highlight of its edge lies on the polyline the
// body is drawn with), else made, in parallel. Built when a body is shown,
// dropped with it (OcctViewer::BodyEntry::edges).
//
// Smooth seams (mitcad#1773, mitcad/geometry/smooth_seams.hpp) are left
// out of what is drawn: an edge with one surface on both sides (a
// cylinder's or a sphere's seam, the edge between two pieces of one face
// that a split left; with `named`, the Shape whose occt() `shape` is, by
// their names too) across which the surface runs on smoothly. They keep
// their copies (a highlight of such an edge, selected through the model,
// is drawn), but are not drawn with the shape in any visual style, and its
// FineEdges objects do not pick them (dropSmoothSeams). A cache whose
// source is the same shape gives its seams, so a highlight of the whole
// body leaves out what the body leaves out.
class ViewEdges {
public:
  explicit ViewEdges(const TopoDS_Shape& shape, const std::vector<const ViewEdges*>& caches = {},
                     const geometry::Shape* named = nullptr);

  // The shape the copies are of.
  const TopoDS_Shape& source() const { return m_source; }
  // What is drawn for it: a compound of the copies, each where and as the
  // source has its edge, but its smooth seams, and the source's free
  // vertices.
  const TopoDS_Shape& drawn() const { return m_drawn; }
  // The copy of the source's edge (any of its places), unlocated; null when
  // the source has no such edge or it is drawn as it is.
  TopoDS_Edge copyOf(const TopoDS_Shape& edge) const;
  // How many of the source's edges have a copy.
  std::size_t size() const { return m_copies.size(); }
  // How many of the source's edges are drawn, and how many are left out as
  // smooth seams (mitcad#1773).
  std::size_t drawnCount() const { return m_drawnCount; }
  std::size_t seamCount() const { return m_seams.size(); }
  // Whether the source's edge (any of its places) is a smooth seam, left
  // out of what is drawn and picked.
  bool smoothSeam(const TopoDS_Shape& edge) const;
  // For UI tests: of this shape's edges, how many `body` has too, and how
  // many of those are drawn with `body`'s own copy.
  std::pair<std::size_t, std::size_t> sharedWith(const ViewEdges& body) const;
  // Every copy, by the source edge's TShape.
  const std::unordered_map<const TopoDS_TShape*, TopoDS_Edge>& copies() const { return m_copies; }

private:
  TopoDS_Shape m_source;
  TopoDS_Shape m_drawn;
  std::unordered_map<const TopoDS_TShape*, TopoDS_Edge> m_copies;
  // The smooth seams, by the source edge's TShape.
  std::unordered_set<const TopoDS_TShape*> m_seams;
  std::size_t m_drawnCount = 0;
};

// Draws `drawn` (ViewEdges::drawn) into a presentation as lines, with the
// drawer's aspects, or `aspect` for every line when given (the face
// boundaries' look).
void drawViewEdges(const occ::handle<Prs3d_Presentation>& presentation, const TopoDS_Shape& drawn,
                   const occ::handle<Prs3d_Drawer>& drawer, const occ::handle<Prs3d_LineAspect>& aspect);

// Makes the hover and OCCT's own selection highlight of a face or an edge
// of the object draw its lines from the copies too: the selection's owners
// of decomposed faces and edges are replaced by owners whose highlight
// presentation is FineLines of their shape, with `edges`' copies (the face
// itself from its display mesh). Picking is unchanged: the owners keep the
// model's face or edge as their shape.
void drawOwnersFrom(const occ::handle<SelectMgr_Selection>& selection, const std::shared_ptr<const ViewEdges>& edges);

// A presentable shape (AIS_Shape, AIS_ColoredShape, the highlight's
// StdSelect_Shape) whose lines are drawn from detached copies
// (mitcad#1633): in the wireframe mode its edges and free vertices, in the
// shaded mode its faces from their display meshes and, when the drawer
// asks for face boundaries, the boundaries from the copies in the face
// boundary aspect. Its shape stays the model's.
template <class Base>
class FineLines : public Base {
public:
  template <class... Args>
  explicit FineLines(std::shared_ptr<const ViewEdges> edges, Args&&... args)
      : Base(std::forward<Args>(args)...), m_edges(std::move(edges)) {}

  const std::shared_ptr<const ViewEdges>& viewEdges() const { return m_edges; }

protected:
  void Compute(const occ::handle<PrsMgr_PresentationManager>& manager,
               const occ::handle<Prs3d_Presentation>& presentation, const int mode) override {
    const occ::handle<Prs3d_Drawer> drawer = this->Attributes();
    if (!m_edges || drawer.IsNull() || (mode != AIS_WireFrame && mode != AIS_Shaded)) {
      Base::Compute(manager, presentation, mode);
      return;
    }
    const bool faces = TopExp_Explorer(m_edges->source(), TopAbs_FACE).More();
    if (mode == AIS_WireFrame || !faces) {
      drawViewEdges(presentation, m_edges->drawn(), drawer, occ::handle<Prs3d_LineAspect>());
      return;
    }
    if (!drawer->FaceBoundaryDraw()) {
      Base::Compute(manager, presentation, mode);
      return;
    }
    // The faces without their boundaries from the meshes, then the
    // boundaries from the copies.
    drawer->SetFaceBoundaryDraw(false);
    Base::Compute(manager, presentation, mode);
    drawer->SetFaceBoundaryDraw(true);
    drawViewEdges(presentation, m_edges->drawn(), drawer, drawer->FaceBoundaryAspect());
  }

  std::shared_ptr<const ViewEdges> m_edges;
};

// Takes the smooth seams of `edges` (ViewEdges::smoothSeam) out of the
// selection: their decomposed edge owners' sensitive entities are dropped,
// so that no line is hovered or picked where none is drawn (mitcad#1773).
// The faces at them and the whole shape stay pickable there.
void dropSmoothSeams(const occ::handle<SelectMgr_Selection>& selection, const ViewEdges& edges);

// An interactive shape (AIS_Shape, AIS_ColoredShape) drawn as FineLines,
// whose faces' and edges' hover is drawn so too (drawOwnersFrom). Picking
// and selection stay the model's faces and edges, but for its smooth
// seams, which are not picked (dropSmoothSeams).
template <class Base>
class FineEdges : public FineLines<Base> {
public:
  using FineLines<Base>::FineLines;

protected:
  void ComputeSelection(const occ::handle<SelectMgr_Selection>& selection, const int mode) override {
    Base::ComputeSelection(selection, mode);
    if (this->m_edges) {
      dropSmoothSeams(selection, *this->m_edges);
      drawOwnersFrom(selection, this->m_edges);
    }
  }
};

} // namespace mitcad
