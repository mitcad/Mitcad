// SPDX-License-Identifier: MIT
#pragma once

// Editing a Flex item directly in the 3D view (mitcad#1713, app/COMMANDS.md
// "Flex editing"). In the item's edit session (owner 14.11.2026: Edit Flex,
// a double-click on the item or on its row; the Flex panel open on it) its
// nodes are handles over the view beside the panel; a single click on the
// item only selects it. The model routes everything (the `flex` query of
// the session's preview); this draws the handles and gives each edit to the
// panel (MainWindow's sessionApply), which previews it; OK keeps the
// session as one undo step, Esc or Cancel takes it back, Undo its last
// change. (Outside a session, `show` without it, each edit is its own
// `flex.*` command and undo step.) The edits:
//
//  - a node dragged slides on its face, takes the face, edge, vertex or
//    point it is dropped on (painted while it is dragged), an end port (an
//    end node on a pipe's or another item's end), or is free on the view's
//    plane through its last place where nothing is under the pointer, or
//    while Alt is held. Meanwhile the item's own body is faded; dashed
//    lines run from the nodes beside it to the pointer, and once the
//    pointer rests (300 ms), or on every move while Shift is held, the item
//    is drawn where its route is now (FlexRope.hpp), from the model's
//    route-only preview, one at a time;
//  - "+" at each end adds a node there (a click arms it and the next click
//    places it, or it is dragged out); anywhere over the item between two
//    nodes an insert point is offered, clicked or dragged out;
//  - a click on a handle selects the node: Delete (and its menu) deletes
//    it. A node of a section that is not round shows a ring under the
//    pointer that turns the section about the route, snapping at quarter
//    turns, its angle typed after a double click;
//  - the item's length, its tightest bend against its limit (of a section
//    that is not round over its flat side and over its edge apart; marked
//    on the item where it is over it) and the item's warnings are shown
//    beside the node, the warnings' places painted in the view;
//  - Esc takes back what is under way, then the node's selection.
//
// The view keeps its navigation: only the left button on a handle, a "+",
// the ring or the route is taken, and the right button on a handle (its
// menu). The plain logic is FlexEdit.hpp.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QString>

#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Selection.hpp"
#include "FlexEdit.hpp"
#include "FlexRope.hpp"

class QEvent;
class QLineEdit;

namespace mitcad {

class SketchHost;

namespace flex {

class FlexEditOverlay;

class FlexEditController : public QObject {
  Q_OBJECT

public:
  FlexEditController(SketchHost& host, OcctViewer& viewer, QObject* parent = nullptr);
  ~FlexEditController() override;

  // What the main window gives.
  struct Hooks {
    // Where the view shows the item's component (the placement of the
    // occurrence that places the feature), and that occurrence's path
    // (empty in the root).
    std::function<gp_Trsf(const QString& uid)> placement;
    std::function<QString(const QString& uid)> occurrence;
    // Paints places in the view as a notice's places are painted (the
    // item's warnings, the face a dragged node would take); none clears.
    std::function<void(const QString& source, const QJsonArray& places)> paintPlaces;
    // In a read-only window: says why nothing is edited and returns true.
    std::function<bool()> refuseEdit;
    // The view's own pick filter and highlights again, after an edit's
    // picks (faces only) end.
    std::function<void()> restoreView;
    // The component the item is in (its uid), for the end ports of other
    // components.
    std::function<QString(const QString& uid)> component;
    // The item's edit session (owner 14.11.2026): its nodes as the panel
    // holds them ({"type": "flex", "nodes": [...]}; empty outside one), and
    // the nodes an edit of the handles gives put into the panel, with what
    // it was for the log (false when the panel did not take them).
    std::function<QJsonObject()> sessionDefinition;
    std::function<bool(const QJsonArray& nodes, const QString& what)> sessionApply;
  };
  void setHooks(Hooks hooks) { m_hooks = std::move(hooks); }

  // Shows the handles of the Flex item `uid` (the same again: nothing);
  // hide() takes them away, with what is under way. `session`: beside the
  // item's edit session (owner 14.11.2026): an edit changes the panel's
  // nodes, not the model (OK keeps the session as one undo step); the
  // handles draw the previewed route; no route follows the pointer (dashed
  // lines while a node is dragged, the item's body not faded); "+" clicked
  // adds an end node at each
  // click until Esc or a click on that "+" again.
  void show(const QString& uid, bool session = false);
  void hide();
  bool isActive() const { return !m_uid.isEmpty(); }
  bool inSession() const { return isActive() && m_session; }
  const QString& uid() const { return m_uid; }
  // Reads the item again after the model changed (an edit, undo, an
  // agent's command); the handles go when it is gone.
  void reload();

  // Esc: the angle being typed, the drag, the armed "+" or the turn under
  // way is taken back, else the selected node is deselected; false when
  // there was none of them (the main window clears its selection).
  bool cancel();
  // Delete: deletes the selected node (one undo step); false when no node
  // is selected, so that the key deletes the main window's selection.
  bool deleteSelectedNode();
  const QString& selectedNode() const { return m_selected; }

  // --- What the overlay draws (widget pixels).

  struct Handle {
    QString id;
    QPointF at;
    QString support; // free, face, edge, point or port
    bool lost = false;
    bool end = false;
  };
  struct Ring {
    QString node;
    QPointF centre;
    QPolygonF circle; // the ring, square to the route
    QPointF knob;     // on it, where the section's own "up" points
    double twist = 0.0;
  };
  struct Scene {
    QPolygonF route;                 // the item's centre line, at rest
    std::vector<RopeSample> rope;    // the item itself while it is pulled (paintFlexRope)
    RopeLook look;
    std::optional<QPointF> bendMark; // the tightest bend, near its limit or over it
    bool bendOver = false;
    bool refused = false;            // the preview failed: the item stays as it is
    std::vector<Handle> handles;
    std::optional<QPointF> addStart; // the "+" before the first node
    std::optional<QPointF> addEnd;   // and after the last
    std::optional<QPointF> insert;   // the insert point under the pointer
    std::optional<QPointF> target;   // where the pulled or new node goes
    bool targetAttached = false;     // onto a face or a port
    // Before its route is worked out (owner 14.11.2026): thin dashed lines
    // from the nodes beside it to the target.
    std::vector<std::pair<QPointF, QPointF>> dashes;
    std::optional<Ring> ring;
    QString hovered;                 // the handle under the pointer
    QString selected;
    QString dragged;
    bool addHovered = false;
    QString badge;                   // the length and the warnings
    bool badgeWarns = false;
    QPointF badgeAt;
    QRectF badgeAround;              // what the badge keeps clear of, when not just badgeAt
  };
  Scene scene() const;
  // How the view shows the design, for the item drawn while it is pulled.
  RopeView ropeView() const;
  // While a node is pulled: what it can be put on is what the view picks
  // (faces, edges, vertices, sketch and construction points), whatever the
  // idle selection filter takes; the main window keeps it so meanwhile.
  bool picking() const { return m_edit.began && m_edit.action != Action::Twist; }
  SelectFilter pickFilter() const;
  const OcctViewer& viewer() const { return m_viewer; }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  enum class Action {
    None,
    Press,  // a handle pressed: a click selects it, a move drags it
    Drag,   // a node follows the pointer
    Extend, // a new end node follows the pointer ("+")
    Insert, // a new node between two follows the pointer
    Twist,  // the ring's knob turns the section
    Menu,   // the right button went down on a handle
  };
  struct Port {
    QJsonObject ref;
    Xyz position{};
    QString feature;
  };
  // What is under way.
  struct Edit {
    Action action = Action::None;
    QString node;         // pulled, turned, or the node an insert comes after
    bool atStart = false; // Extend: before the first node
    bool moved = false;   // the pointer went past a click
    bool armed = false;   // Extend: placed by the next click
    bool began = false;   // begin() ran: the definition is read, the view picks faces
    bool commitAfter = false; // released while a preview was computed
    bool cancelAfter = false; // Esc while one was
    QPointF pressAt;
    Xyz anchor{};         // the free plane goes through it
    bool targetOnCurve = false; // the target is on an edge, not a point
    bool forcedFree = false;    // Alt keeps it off what is under the pointer
    // Shift held (owner 14.11.2026): the route follows the pointer on every
    // move; without it a dashed line until the pointer rests (restSerial:
    // the move the wait is for) or the release.
    bool live = false;
    int restSerial = 0;
    double along = 0.0;   // Insert: where on the route it began
    std::optional<NodeTarget> target;
    // Twist.
    double startTwist = 0.0;
    double twist = 0.0;
    Xyz axisPoint{};
    Xyz axis{0.0, 0.0, 1.0};
    Xyz startVector{1.0, 0.0, 0.0};
    Xyz startNormal{0.0, 1.0, 0.0};
    // The preview.
    QJsonObject def;                    // the feature's definition when it began
    bool pending = false;               // a preview waits for its turn
    bool running = false;               // one is computed now
    bool again = false;                 // the pointer moved meanwhile
    bool shown = false;                 // the model holds a preview
    bool refused = false;
    QString problem;
    std::optional<NodeTarget> previewedTarget;
    std::optional<double> previewedTwist;
    std::optional<FlexItem> preview;
  };
  struct Hover {
    enum class Kind { None, Handle, AddStart, AddEnd, Route, Knob };
    Kind kind = Kind::None;
    QString node;
    RoutePlace place;
    bool operator==(const Hover& other) const {
      return kind == other.kind && node == other.node && distance(place.point, other.place.point) <= 1e-9;
    }
  };

  // The item's points in the design and on the screen.
  gp_Pnt designPoint(const Xyz& local) const;
  Xyz localPoint(const gp_Pnt& design) const;
  Xyz designDirection(const Xyz& local) const;
  QPointF onScreen(const Xyz& local) const;
  double pixelSize(const Xyz& local) const;
  // The item as shown now: the preview while one is shown.
  const FlexItem& shown() const;
  // Where each "+" is drawn: a little beyond the end, along the route.
  std::optional<QPointF> addPlace(const FlexItem& item, bool start) const;
  // The ring of the selected (or turned) node, when the section is not round.
  std::optional<Ring> ringOf(const FlexItem& item) const;
  // The nearest place of the route on the screen and its distance in pixels.
  std::optional<std::pair<RoutePlace, double>> routePlaceAt(const QPointF& position) const;
  Hover hoverAt(const QPointF& position) const;

  bool mouse(QEvent* event);
  void setHover(const Hover& hover);
  void selectNode(const QString& id);
  bool refused();
  // Starts what a press began: reads the definition and the ports, and
  // makes the view pick faces only.
  bool begin(Action action);
  // Ends what is under way, the model's preview with it; `restore` gives
  // the view its own picks back.
  void end(bool restore, bool bodiesStale = false);
  // The item's own body see-through and unpicked while it is pulled, and
  // back (`stale`: the view has shown the bodies anew meanwhile).
  void fadeBody();
  void showBody(bool stale);
  void loadPorts();
  // Where the pointer puts the node: a port, the face under it, or the
  // view's plane through the anchor.
  std::optional<NodeTarget> targetAt(const QPointF& position, bool endNode, bool forceFree, bool* onCurve) const;
  std::optional<gp_Pnt> edgePointAt(const TopoDS_Shape& shape, const QPointF& position) const;
  void pointerTo(const QPointF& position, Qt::KeyboardModifiers modifiers);
  void turnTo(const QPointF& position);
  void schedulePreview();
  void applyPreview();
  QJsonObject previewDefinition() const;
  void paintPlaces();
  void commit();
  void commitInsertAtRoute();
  bool run(const QJsonObject& command, const QString& what, QJsonObject* result = nullptr);
  void showMenu(const QString& node, const QPoint& globalPosition);
  void editTwist(const QString& node);
  void closeTwistField(bool take);
  QString badgeText(bool* warns) const;
  void refresh();
  // The handles' places in the window, for UI tests, when they moved.
  void logPlaces();
  void log(const QString& text) const;
  // An end's handle or "+" under a card floating over the view (the
  // Floating layout's selection card, mitcad#1713): the view moved
  // sideways, as little as it takes, so that the item and its "+" are clear
  // of the cards (OcctViewer::bringClearOfPanels; fitted into the free area
  // when larger). Once per item shown, within a moment of showing it (the
  // card comes after the selection); a view the user moves later stays.
  void bringClearOfCards();
  bool underCard(const QPointF& position) const;

  SketchHost& m_host;
  OcctViewer& m_viewer;
  Hooks m_hooks;
  FlexEditOverlay* m_overlay = nullptr;
  QLineEdit* m_twistField = nullptr;
  QString m_twistFieldNode;
  QString m_uid;
  bool m_session = false;      // beside the item's edit session
  QJsonObject m_sessionDef;    // the session's definition as the handles last saw or gave it
  QJsonObject m_awaitedDef;    // given to the panel and not seen back yet (the model was busy)
  bool m_reloadAfter = false;  // the session's preview came while an edit was under way
  // Starts "+" again at that end, its next click adding the next end node.
  void armExtend(bool atStart);
  FlexItem m_item;
  gp_Trsf m_placement;
  QString m_occurrence;
  std::vector<Port> m_ports;
  QString m_selected;
  QString m_ringNode; // the node whose twist ring the pointer brought up
  QColor m_bodyColor;
  std::vector<BodyDisplay> m_savedBodies; // as the view showed them before the item's body was faded
  bool m_bodiesFaded = false;
  Hover m_hover;
  Edit m_edit;
  QPointF m_pointer;
  // Counts the edits ended: a preview's timer or answer of an ended one
  // is dropped.
  quint64 m_generation = 0;
  bool m_swallowRelease = false; // the release that follows a double click
  QJsonArray m_paintedPlaces;
  QString m_loggedPlaces;
  QString m_loggedBadge;
  QString m_loggedTarget;
  QString m_loggedPreview;
  // When the item was shown, and whether its handles were brought clear of
  // the cards since (bringClearOfCards).
  QElapsedTimer m_shownAt;
  bool m_broughtClear = false;
};

} // namespace flex
} // namespace mitcad
