// SPDX-License-Identifier: MIT
#pragma once

// A drawing's sheet (mitcad#707): paints what the model's `drawing` query
// gives (core/model-api/src/commands.md, "Drawings in the design": groups
// of lines, arcs, polylines, fills, shading and texts in millimetres of
// paper, the views' boxes, the annotations' boxes) on a sheet of paper,
// zoomed and panned with the wheel and the middle button, and turns the
// user's clicks into what the active tool waits for (DrawingTools.hpp).
// Nothing of the model is computed here: points go to the model in sheet
// millimetres, and the model says what a click on a view picks.
//
// Logs for UI tests: "Drawing sheet view x0,y0 x1,y1 x2,y2" (where the
// sheet points (0, 0), (100, 0) and (0, 100) are in the window) whenever
// that changes, "Drawing view <id> at x,y" (the middle of its box in the
// window), "Drawing annotation <id> at x,y" (the middle of its box),
// "Drawing view <id> selected", "Drawing annotation <id> selected",
// "Drawing sheet drawn: <n> views, <n> items, <n> center marks" when a
// sheet comes, "Drawing annotations drawn: 1 = 20 ±0.1 | 2 broken" when
// its annotations change, "Drawing placing view" when a line, a circle or
// picks wait for the click that places what they make, "Drawing preview:
// <what>" when what is placed shows under the pointer meanwhile (a view's
// box, a dimension's lines, a leader), "Drawing selection: view 3, annotation
// 1" when Ctrl+click adds to the selection or takes from it, "Drawing point
// <n> at x,y" for each click of a line or an arc drawn outside the model
// (mitcad#729). A view the
// model could not draw keeps a red box at its place, as does an annotation
// whose picks are gone, to be selected and deleted.
//
// Several items are selected with Ctrl+click (mitcad#725): Delete deletes
// them and a drag moves them together; the right button asks for the
// sheet's menu of what is under the pointer, a double-click on a note for
// its text.

#include <QJsonArray>
#include <QJsonObject>
#include <QLineF>
#include <QList>
#include <QMargins>
#include <QPixmap>
#include <QPoint>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QSize>
#include <QString>
#include <QVector>
#include <QWidget>

#include "DrawingTools.hpp"

namespace mitcad::drawings {

// A view of the sheet, as the query gives it.
struct SheetView {
  int id = 0;
  QString type; // base, projected, section, detail
  int parent = 0;
  QRectF box; // on the sheet, mm; empty when it shows nothing
  QString label;
  QString error;
  QJsonObject json; // all the query says of it
};

// An annotation of the sheet (a view's, or a text note), as the query
// gives it.
struct SheetAnnotation {
  int id = 0;
  int view = 0; // 0: a text note on the sheet
  QString type; // linear, angle, …, note, text
  QString label;
  bool broken = false;
  QString error;
  QRectF box;       // on the sheet, mm
  QJsonObject json; // all the query says of it
  // What it draws (its group `A<id>`), on the sheet: its lines and arcs as
  // segments, its texts and fills as areas. A click picks it on them, not
  // anywhere in its box (mitcad#717: a box could cover a view).
  QVector<QLineF> lines;
  QVector<QPolygonF> areas;
  // A point on it that picks it (its first text's middle, else its first
  // line's): where it is logged.
  QPointF handle;
};

class DrawingSheetView : public QWidget {
  Q_OBJECT

public:
  explicit DrawingSheetView(QWidget* parent = nullptr);

  // A sheet of the `drawing` query (its size, views, annotations and
  // groups); the view is fitted the first time and when the sheet's size
  // changes, and keeps its zoom and pan otherwise (mitcad#751: the next
  // model command does not undo a zoom).
  void setSheet(const QJsonObject& sheet);
  // What the clicks are for; None: the selection. With `place`, one more
  // click after a line or a circle says where the new view goes.
  void setPick(SheetPick pick, bool place = false);
  SheetPick pick() const { return m_pick; }
  // A line over the top of the sheet (the active tool's hint); none when
  // empty. Logged as "Drawing hint: <text>".
  void setHint(const QString& text);
  // The view picked first (a projected view's parent): the next clicks
  // beside it place views projected from it.
  void setParentView(int id);
  int parentView() const { return m_parentView; }

  // The view or the annotation selected alone, or the first one of several.
  int selectedView() const { return m_selected; }
  // Selects one view alone (0: nothing).
  void selectView(int id);
  const QVector<SheetView>& views() const { return m_views; }
  const SheetView* view(int id) const;
  // The view under a sheet point, the smallest first; 0 for none.
  int viewAt(const QPointF& sheet) const;

  // The annotations: one is selected alone (then no view), or several
  // items with Ctrl+click.
  int selectedAnnotation() const { return m_selectedAnnotation; }
  void selectAnnotation(int id);
  // Everything selected (mitcad#725): the views, the annotations, and how
  // many in all; nothing selected clears it.
  QList<int> selectedViews() const;
  QList<int> selectedAnnotations() const;
  int selectionSize() const;
  void clearSelection();
  const QVector<SheetAnnotation>& annotations() const { return m_annotations; }
  const SheetAnnotation* annotation(int id) const;
  // The annotation whose lines or texts are under a sheet point (near
  // them on the screen), the nearest; one without any (a broken one) by
  // its box. 0 for none.
  int annotationAt(const QPointF& sheet) const;

  // What the model found for an annotation tool's clicks so far, drawn
  // over the sheet (the `drawing_pick` query's `items`); empty: none.
  void setPickMarks(const QJsonArray& items);
  // What an annotation tool's click would pick where the pointer is
  // (mitcad#725), drawn lighter; empty: nothing.
  void setHoverMarks(const QJsonArray& items);
  // An annotation tool's picks are there (or it takes none): the next
  // click places what it makes.
  void startPlacing();
  // Whether a placing click is awaited.
  bool isPlacing() const { return m_placing; }
  // What follows the pointer while an annotation is placed (mitcad#725):
  // "dimension" (lines from the two points, sheet mm, to a dimension line
  // through the pointer), "leader" (a line from the point to the pointer
  // and a box there), empty for none.
  void setPlacingPreview(const QString& kind, const QVector<QPointF>& points);

  // Drops a tool's picks so far (and a placing click awaited); false when
  // there were none.
  bool cancelPicks();

  // The whole sheet in the widget, less its covered margins.
  void fit();
  // What covers the widget at its sides (mitcad#1008: the Floating
  // layout's browser card over the sheet's left): the sheet is fitted
  // beside it; a sheet shown as fitted is fitted again.
  void setFitMargins(const QMargins& margins);
  QPointF toSheet(const QPointF& widget) const;
  QPointF toWidget(const QPointF& sheet) const;
  // How many times the sheet itself (the paper and the groups' lines,
  // fills and texts) was painted (mitcad#759): it is kept as a picture
  // painted again only for a new sheet, a zoom, a pan or another size, and
  // the selection, the hover and the previews are painted over it. For
  // tests.
  int sheetPaints() const { return m_sheetPaints; }

signals:
  void viewSelected(int id);
  // A view dragged with the selection, by this on the sheet.
  void viewMoved(int id, QPointF delta);
  // What a tool waits for came: the view it is on (0: none) and the
  // points; a circle's radius on the sheet.
  void picked(const ToolPicks& picks);
  void deleteRequested(int id);
  // A selected detail's circle dragged on its parent: its new centre (sheet
  // mm).
  void detailCircleMoved(int id, QPointF center);
  // An annotation tool's click (SheetPick::Picks) before its placing one:
  // the view under it (0: none) and where it is; the workspace asks the
  // model what it picks.
  void pickRequested(int view, QPointF at);
  void annotationSelected(int id);
  // An annotation dragged with the selection, by this on the sheet.
  void annotationMoved(int id, QPointF delta);
  void annotationDeleteRequested(int id);
  // Esc: the picks so far, the tool, the selection or the drawing is done
  // (the workspace says which).
  void toolCancelled();
  // A tool's line or circle is drawn: the next click places its view.
  void placing();
  // The pointer moved while an annotation tool waits for a pick: the view
  // under it (0: none) and where it is; the workspace asks the model what
  // a click there would pick. A view of -1: the pointer left the sheet.
  void hoverRequested(int view, QPointF at);
  // The selection changed by Ctrl+click (several items, or one again).
  void selectionChanged();
  // Several selected items dragged together, by this on the sheet.
  void selectionMoved(QPointF delta);
  // Delete with several items selected.
  void deleteSelectionRequested();
  // The right button on the sheet (with no tool): what is under the
  // pointer is selected first; `at` on the sheet, `global` on the screen.
  void contextMenuRequested(QPointF at, QPoint global);
  // A double-click on an annotation (with no tool).
  void annotationDoubleClicked(int id);

protected:
  bool event(QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;
  void contextMenuEvent(QContextMenuEvent* event) override;
  void enterEvent(QEnterEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void wheelEvent(QWheelEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;

private:
  // Paints the paper and the groups' items into the sheet's picture, of
  // `pixels` at `ratio` device pixels per pixel.
  void paintSheet(const QSize& pixels, qreal ratio);
  // The sheet's picture is painted again before the next paint.
  void sheetMoved();
  // Whether what is painted follows the pointer (a tool's rubber band or
  // preview): every move paints again then, else only a change of what is
  // under the pointer does.
  bool followsPointer() const;
  // Paints the groups' items with the sheet's transform.
  void paintItems(QPainter& painter) const;
  // Paints the annotations' selection, broken ones and an annotation
  // tool's picks.
  void paintAnnotations(QPainter& painter) const;
  // The circle of a detail view on its parent (sheet mm): false when the
  // view is no detail.
  bool detailCircle(int id, QPointF* center, double* radius) const;
  void logPlaces();
  void logAnnotations();
  void resetPicks();
  // Ctrl+click: the view or annotation goes into the selection, or out of
  // it when it is in.
  void toggleSelected(int view, int annotation);
  bool isSelectedView(int id) const;
  bool isSelectedAnnotation(int id) const;
  void logSelection();
  // The preview of what is placed under the pointer (mitcad#725).
  void paintPreview(QPainter& painter) const;
  // The box a view would have at the pointer while it is placed, sheet
  // mm: the parent's for a projected view or a section, the detail's
  // circle at twice its parent's scale; empty when it is not known.
  QRectF previewBox() const;
  void logPreview(const QString& what);

  QVector<SheetView> m_views;
  QVector<SheetAnnotation> m_annotations;
  QJsonArray m_groups;
  double m_width = 297.0; // mm
  double m_height = 210.0;
  // Pixels per millimetre and where the sheet's lower left corner is.
  double m_zoom = 1.0;
  QPointF m_origin;
  // Showing the whole sheet: a resize fits it again. A zoom or a pan ends
  // it.
  bool m_fitted = false;
  // Fitted once: a later sheet of the same size keeps the zoom and the pan
  // (mitcad#751).
  bool m_everFitted = false;
  QMargins m_fitMargins; // setFitMargins
  // The paper with the groups' items, as painted last (mitcad#759).
  QPixmap m_sheetPicture;
  bool m_sheetPainted = false;
  int m_sheetPaints = 0;

  SheetPick m_pick = SheetPick::None;
  bool m_place = false;   // a placing click after the line or circle
  bool m_placing = false; // the line or circle drawn, waiting for it
  QString m_hint;
  int m_selected = 0;
  int m_selectedAnnotation = 0;
  // Further items Ctrl+click selected besides the first.
  QList<int> m_moreViews;
  QList<int> m_moreAnnotations;
  // A press on an item of several selected keeps them for a drag; a
  // click without a drag then selects it alone.
  bool m_dragSelection = false;
  QJsonArray m_hoverMarks;
  QString m_previewKind;
  QVector<QPointF> m_previewPoints;
  QString m_loggedPreview;
  int m_parentView = 0;
  int m_hovered = 0;
  int m_hoveredAnnotation = 0;
  // A tool's picks so far, and the pointer for the rubber band.
  ToolPicks m_picks;
  QJsonArray m_pickMarks;
  QPointF m_pointer; // sheet mm
  bool m_pointerIn = false;
  // A drag: of a view or an annotation with the selection, of the sheet
  // with the middle button, out of a view with the projected view's tool,
  // of a selected detail's circle on its parent.
  enum class Drag { None, View, Pan, OutOfView, DetailCircle, Annotation };
  Drag m_drag = Drag::None;
  int m_dragView = 0; // the view, the detail or the annotation dragged
  QPointF m_dragFrom;   // sheet mm (a pan: widget px)
  QPointF m_dragOffset; // a view's drag so far, mm
  bool m_dragMoved = false;
  QString m_loggedPlaces;
  QString m_loggedAnnotations;
};

} // namespace mitcad::drawings
