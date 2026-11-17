// SPDX-License-Identifier: MIT
#include "FlexEditController.hpp"

#include "FlexRope.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <utility>
#include <vector>

#include <QDebug>
#include <QEvent>
#include <QFontMetricsF>
#include <QJsonArray>
#include <QLineEdit>
#include <QLineF>
#include <QLocale>
#include <QMenu>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QResizeEvent>
#include <QStringList>
#include <QTimer>
#include <QWidget>
#include <QtLogging>

#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Precision.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <Bnd_Box.hxx>
#include <BRep_Tool.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "../sketch/Sketch3dDraw.hpp"
#include "../sketch/SketchController.hpp"

namespace mitcad::flex {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kGrabPixels = 9.0;    // a handle under the press
constexpr double kAddPixels = 10.0;    // a "+" under the press
constexpr double kRoutePixels = 7.0;   // the route under the pointer
constexpr double kClearPixels = 16.0;  // the route's insert point keeps this far from a handle
constexpr double kClickPixels = 3.0;   // a press moved less than this is a click
constexpr int kRestMs = 300;           // the pointer rests: the route to where it is (owner 14.11.2026)
constexpr int kEdgeSamples = 64;       // places tried along an edge under the pointer
constexpr double kAddOffset = 26.0;    // a "+" beyond its end
constexpr double kRingPixels = 36.0;   // the twist ring's radius
constexpr double kRingLabelPixels = 8.0; // the badge kept this clear of the ring
constexpr double kKnobPixels = 10.0;   // its knob under the press
constexpr double kPortPixels = 14.0;   // an end port takes an end node this near
constexpr double kSnapTwist = 6.0 * kPi / 180.0;
constexpr int kRingSamples = 48;

double pixels(const QPointF& a, const QPointF& b) { return QLineF(a, b).length(); }

QString windowPlace(const QPointF& at, const QPoint& window) {
  return QStringLiteral("%1,%2").arg(qRound(at.x() + window.x())).arg(qRound(at.y() + window.y()));
}

// A message of the model as a line of its own: its first letter a capital.
QString sentence(QString text) {
  if (!text.isEmpty()) {
    text[0] = text.at(0).toUpper();
  }
  return text;
}

// Where a ray meets a face placed in the design, nearest to its origin;
// none when it misses it.
std::optional<gp_Pnt> rayHit(const TopoDS_Shape& face, const gp_Lin& ray) {
  try {
    IntCurvesFace_ShapeIntersector hits;
    hits.Load(face, Precision::Confusion());
    hits.PerformNearest(ray, 0.0, Precision::Infinite());
    if (hits.IsDone() && hits.NbPnt() > 0) {
      return hits.Pnt(1);
    }
  } catch (const Standard_Failure&) {
    return std::nullopt; // the node is not put on it then
  }
  return std::nullopt;
}

// The point of a vertex, a sketch point or a construction point as the
// view shows it; none for a shape without one.
std::optional<gp_Pnt> vertexPoint(const TopoDS_Shape& shape) {
  try {
    const TopExp_Explorer vertices(shape, TopAbs_VERTEX);
    if (vertices.More()) {
      return BRep_Tool::Pnt(TopoDS::Vertex(vertices.Current()));
    }
  } catch (const Standard_Failure&) {
    return std::nullopt;
  }
  return std::nullopt;
}

// The nearest point of a segment to a point, as its share of the segment.
double shareOnSegment(const QPointF& a, const QPointF& b, const QPointF& p) {
  const QPointF d = b - a;
  const double size = d.x() * d.x() + d.y() * d.y();
  if (size < 1e-12) {
    return 0.0;
  }
  return std::clamp(((p.x() - a.x()) * d.x() + (p.y() - a.y()) * d.y()) / size, 0.0, 1.0);
}

} // namespace

// What the controller draws over the view; it takes no mouse input (the
// controller filters the view's own events).
class FlexEditOverlay : public QWidget {
public:
  FlexEditOverlay(FlexEditController& controller, QWidget* view) : QWidget(view), m_c(controller) {
    setObjectName(QStringLiteral("flexEditOverlay"));
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
    QFont small = font();
    small.setPointSizeF(9.0 * uiTextScale());
    setFont(small);
  }

  // A text box beside a place (`around`: a point, or what the box keeps
  // clear of), kept inside the part of the view no panel covers.
  QRectF badgeBox(const QRectF& around, const QString& text) const {
    const QFontMetricsF metrics(font());
    QRectF box = metrics.boundingRect(QRectF(0.0, 0.0, 340.0, 240.0), Qt::TextWordWrap, text);
    box.adjust(-6.0, -4.0, 6.0, 4.0);
    const QPointF offset(18.0, 16.0);
    box.moveTopLeft(around.bottomRight() + offset);
    const QRectF whole = QRectF(rect()).adjusted(2.0, 2.0, -2.0, -2.0);
    QRectF area = m_c.viewer().uncoveredArea().adjusted(2.0, 2.0, -2.0, -2.0).intersected(whole);
    if (area.width() < box.width() || area.height() < box.height()) {
      area = whole;
    }
    if (box.right() > area.right()) {
      box.moveRight(around.left() - offset.x());
    }
    if (box.bottom() > area.bottom()) {
      box.moveBottom(around.top() - offset.y());
    }
    box.moveLeft(std::max(box.left(), area.left()));
    box.moveTop(std::max(box.top(), area.top()));
    return box;
  }

protected:
  void paintEvent(QPaintEvent* /*event*/) override {
    if (!m_c.isActive()) {
      return;
    }
    const FlexEditController::Scene scene = m_c.scene();
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QColor halo = m_c.viewer().sketchHaloColor();
    const bool dark = halo.lightnessF() < 0.5;
    const QColor ink = viewColor(ColorRole::SketchDimension, dark);
    const QColor handle = viewColor(ColorRole::Handle, dark);
    const QColor hover = viewColor(ColorRole::Hover, dark);
    const QColor warning = viewColor(ColorRole::WarningBorder, dark);
    const QColor error = viewColor(ColorRole::ErrorBorder, dark);
    const QColor fill(halo.red(), halo.green(), halo.blue(), 235);

    // The item while it is pulled: itself, from the route's samples, with
    // its section's size and its colour (its body is faded meanwhile). At
    // rest only its centre line, thin, so that it reads through the bodies
    // in front of it.
    if (!scene.rope.empty()) {
      paintFlexRope(painter, scene.rope, scene.look, m_c.ropeView());
    }
    if (scene.route.size() >= 2) {
      QColor line = scene.refused ? error : handle;
      line.setAlpha(scene.refused ? 255 : 170);
      painter.setPen(QPen(line, 1.3, scene.refused ? Qt::DashLine : Qt::SolidLine));
      painter.drawPolyline(scene.route);
    }
    for (const auto& [from, to] : scene.dashes) {
      paintDashedLine(painter, from, to, ink, halo);
    }
    // The tightest bend, when it is near its limit or over it.
    if (scene.bendMark) {
      painter.setBrush(Qt::NoBrush);
      painter.setPen(QPen(scene.bendOver ? error : warning, 2.5));
      painter.drawEllipse(*scene.bendMark, 11.0, 11.0);
      painter.drawEllipse(*scene.bendMark, 2.0, 2.0);
    }

    // Where the pulled or new node goes: a square on a face or an end, a
    // circle in space.
    if (scene.target) {
      painter.setPen(QPen(handle, 2.0));
      painter.setBrush(Qt::NoBrush);
      if (scene.targetAttached) {
        painter.drawRect(QRectF(*scene.target - QPointF(8.0, 8.0), QSizeF(16.0, 16.0)));
      } else {
        painter.drawEllipse(*scene.target, 9.0, 9.0);
      }
    }

    // The insert point under the pointer and the "+" at each end.
    const auto plus = [&painter, &fill](const QPointF& at, double radius, const QColor& colour, double width) {
      painter.setPen(QPen(colour, width));
      painter.setBrush(fill);
      painter.drawEllipse(at, radius, radius);
      const double arm = radius - 3.0;
      painter.drawLine(at - QPointF(arm, 0.0), at + QPointF(arm, 0.0));
      painter.drawLine(at - QPointF(0.0, arm), at + QPointF(0.0, arm));
    };
    if (scene.insert) {
      plus(*scene.insert, 7.0, hover, 2.0);
    }
    for (const std::optional<QPointF>& plusAt : {scene.addStart, scene.addEnd}) {
      if (plusAt) {
        plus(*plusAt, 8.0, scene.addHovered ? hover : handle, scene.addHovered ? 2.0 : 1.5);
      }
    }

    // The twist ring: the circle square to the route, the knob where the
    // section's own up points, its angle beside it.
    if (scene.ring) {
      painter.setBrush(Qt::NoBrush);
      painter.setPen(QPen(handle, 1.5));
      painter.drawPolygon(scene.ring->circle);
      painter.drawLine(scene.ring->centre, scene.ring->knob);
      painter.setBrush(handle);
      painter.drawEllipse(scene.ring->knob, 5.0, 5.0);
      painter.setBrush(Qt::NoBrush);
      const QString angle = degreesText(scene.ring->twist);
      const QRectF text = QFontMetricsF(font()).boundingRect(angle).adjusted(-4.0, -2.0, 4.0, 2.0);
      QRectF box(scene.ring->knob + QPointF(9.0, -text.height() - 4.0), text.size());
      painter.setPen(QPen(ink, 1.0));
      painter.setBrush(fill);
      painter.drawRoundedRect(box, 3.0, 3.0);
      painter.drawText(box, Qt::AlignCenter, angle);
      painter.setBrush(Qt::NoBrush);
    }

    // The nodes: a ring in space, a filled dot on a face, a square on an
    // end; a lost one in the warning's colour; the selected one filled.
    for (const FlexEditController::Handle& node : scene.handles) {
      const bool hovered = node.id == scene.hovered || node.id == scene.dragged;
      const bool selected = node.id == scene.selected;
      const QColor colour = node.lost ? warning : hovered ? hover : handle;
      const double radius = hovered || selected ? 7.0 : 6.0;
      painter.setPen(QPen(colour, hovered || selected ? 2.5 : 2.0));
      painter.setBrush(selected ? colour : fill);
      if (node.support == QStringLiteral("port")) {
        painter.drawRect(QRectF(node.at - QPointF(radius, radius), QSizeF(2.0 * radius, 2.0 * radius)));
      } else {
        painter.drawEllipse(node.at, radius, radius);
      }
      if (!selected && node.support != QStringLiteral("free") && node.support != QStringLiteral("port")) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(colour);
        painter.drawEllipse(node.at, 2.5, 2.5);
      }
    }
    painter.setBrush(Qt::NoBrush);

    // The length and the warnings beside the node.
    if (!scene.badge.isEmpty()) {
      const QRectF box =
          badgeBox(scene.badgeAround.isNull() ? QRectF(scene.badgeAt, QSizeF()) : scene.badgeAround, scene.badge);
      painter.setPen(QPen(scene.badgeWarns ? warning : ink, 1.0));
      painter.setBrush(fill);
      painter.drawRoundedRect(box, 4.0, 4.0);
      painter.setPen(ink);
      painter.drawText(box.adjusted(6.0, 4.0, -6.0, -4.0), Qt::TextWordWrap, scene.badge);
    }
  }

private:
  FlexEditController& m_c;
};

FlexEditController::FlexEditController(SketchHost& host, OcctViewer& viewer, QObject* parent)
    : QObject(parent), m_host(host), m_viewer(viewer) {
  m_overlay = new FlexEditOverlay(*this, &viewer);
  m_overlay->hide();
  m_twistField = new QLineEdit(&viewer);
  m_twistField->setObjectName(QStringLiteral("flexTwistField"));
  m_twistField->setAlignment(Qt::AlignRight);
  m_twistField->hide();
  connect(m_twistField, &QLineEdit::returnPressed, this, [this] { closeTwistField(true); });
  connect(m_twistField, &QLineEdit::editingFinished, this, [this] { closeTwistField(false); });
  viewer.installEventFilter(this);
  // The handles follow the camera; their places are logged once it rests.
  connect(&viewer, &OcctViewer::viewChanged, m_overlay, [this] {
    if (isActive()) {
      m_overlay->update();
    }
  });
  connect(&viewer, &OcctViewer::cameraRested, this, [this] {
    if (isActive() && m_edit.action == Action::None) {
      logPlaces();
    }
  });
  connect(&viewer, &OcctViewer::pointerLeft, this, [this] {
    if (isActive() && m_edit.action == Action::None) {
      setHover(Hover());
    }
  });
  // The selection card comes over the view after the item was selected.
  connect(
      &viewer, &OcctViewer::coveredChanged, this, [this] { bringClearOfCards(); }, Qt::QueuedConnection);
}

FlexEditController::~FlexEditController() = default;

void FlexEditController::log(const QString& text) const { qDebug().noquote() << text; }

void FlexEditController::show(const QString& uid, bool session) {
  if (uid == m_uid && session == m_session) {
    return;
  }
  hide();
  if (uid.isEmpty()) {
    return;
  }
  m_uid = uid;
  m_session = session;
  m_sessionDef = QJsonObject();
  m_awaitedDef = QJsonObject();
  m_reloadAfter = false;
  m_loggedPlaces.clear();
  m_loggedBadge.clear();
  reload();
  if (!isActive()) {
    return; // it is no Flex item the model knows
  }
  m_overlay->resize(m_viewer.size());
  m_overlay->show();
  m_overlay->raise();
  log(QStringLiteral("Flex edit shown: %1 %2, %3 nodes, length %4")
          .arg(m_uid, m_item.name)
          .arg(m_item.nodes.size())
          .arg(m_item.length, 0, 'f', 3));
  if (m_session) {
    m_host.showHint(tr("Drag a node to move it (Alt: free in space), click + to make the item longer (Esc ends "
                       "that), or click the item between two nodes to add one. Enter keeps the changes, Esc "
                       "cancels them."));
  } else {
    m_host.showHint(tr("Drag a node to move the item (Shift: the route follows the pointer; Alt: free in space), "
                       "click + to make it longer, or click the item between two nodes to add a node."));
  }
  refresh();
  m_shownAt.start();
  m_broughtClear = false;
  QTimer::singleShot(0, this, [this] { bringClearOfCards(); });
}

bool FlexEditController::underCard(const QPointF& position) const {
  const std::vector<QRect> cards = m_viewer.coveredRects();
  return std::any_of(cards.begin(), cards.end(), [&position](const QRect& card) {
    return QRectF(card).adjusted(-6, -6, 6, 6).contains(position);
  });
}

void FlexEditController::bringClearOfCards() {
  // (A moment after showing: the card the selection brings, not one the
  // user brought over the item later.)
  constexpr qint64 kCardMoment = 2000;
  if (!isActive() || m_broughtClear || m_edit.action != Action::None || !m_item.valid || !m_shownAt.isValid() ||
      m_shownAt.elapsed() > kCardMoment) {
    return;
  }
  const QRectF view(m_viewer.rect());
  std::vector<QPointF> handles;
  for (const FlexNode& node : m_item.nodes) {
    if (m_item.isEnd(node.id)) {
      handles.push_back(onScreen(node.point));
    }
  }
  for (const bool start : {true, false}) {
    if (const std::optional<QPointF> plus = addPlace(m_item, start)) {
      handles.push_back(*plus);
    }
  }
  const bool covered = std::any_of(handles.begin(), handles.end(), [&](const QPointF& at) {
    return view.contains(at) && underCard(at);
  });
  if (!covered) {
    return;
  }
  // The item with room for its "+" round it.
  Bnd_Box box;
  for (const Xyz& point : m_item.polyline) {
    box.Add(designPoint(point));
  }
  for (const FlexNode& node : m_item.nodes) {
    box.Add(designPoint(node.point));
  }
  if (box.IsVoid()) {
    return;
  }
  const Xyz& end = m_item.nodes.back().point;
  box.Enlarge((kAddOffset + kAddPixels + 6.0) * pixelSize(end));
  m_broughtClear = true;
  log(QStringLiteral("Flex edit: an end's handle under a card, the item brought clear of the cards"));
  m_viewer.bringClearOfPanels(box, true);
}

void FlexEditController::hide() {
  if (!isActive()) {
    return;
  }
  // No longer active first: what follows may bring the main window back
  // here (the view's own picks again).
  const QString uid = m_uid;
  m_uid.clear();
  closeTwistField(false);
  const bool previewShown = m_edit.shown;
  m_edit = Edit();
  showBody(false);
  m_ringNode.clear();
  ++m_generation;
  if (previewShown) {
    try {
      m_host.clearModelPreview();
    } catch (const std::exception&) {
      // Nothing was held then.
    }
  }
  m_item = FlexItem();
  m_session = false;
  m_sessionDef = QJsonObject();
  m_awaitedDef = QJsonObject();
  m_reloadAfter = false;
  m_ports.clear();
  m_selected.clear();
  m_hover = Hover();
  m_viewer.setDragCursor(false);
  if (!m_paintedPlaces.isEmpty()) {
    m_paintedPlaces = QJsonArray();
    if (m_hooks.paintPlaces) {
      m_hooks.paintPlaces(QStringLiteral("Flex"), QJsonArray());
    }
  }
  m_overlay->hide();
  log(QStringLiteral("Flex edit hidden: %1").arg(uid));
}

void FlexEditController::reload() {
  if (!isActive()) {
    return;
  }
  if (m_edit.running) {
    return; // the preview under way ends first; the edit's end reads again
  }
  if (m_host.modelBusy()) {
    m_host.whenIdle(this, [this] { reload(); });
    return;
  }
  if (m_session && m_edit.action != Action::None) {
    // The session's preview came while a node is pulled: read once it ends.
    m_reloadAfter = true;
    return;
  }
  // What was under way went with the model's change (an undo, an agent's
  // command); a node edit of its own has ended before it runs.
  if (m_edit.action != Action::None) {
    log(QStringLiteral("Flex edit: what was under way went with the model's change"));
    end(true);
  }
  m_reloadAfter = false;
  FlexItem item;
  try {
    item = FlexItem::read(m_host.queryObject({{QStringLiteral("query"), QStringLiteral("flex")},
                                             {QStringLiteral("uid"), m_uid},
                                             {QStringLiteral("frames"), true}}));
  } catch (const std::exception&) {
    // Gone from the model (an undo): the handles go with it.
  }
  if (m_session) {
    // The edit session (owner 14.11.2026): the nodes as the panel holds them
    // (or as the handles gave them while the panel had not taken them yet),
    // the route of the session's preview; the item rolled back for the
    // edit gives its uid, name and section.
    const QJsonObject held = m_hooks.sessionDefinition ? m_hooks.sessionDefinition() : QJsonObject();
    if (held.isEmpty()) {
      hide(); // the session ended
      return;
    }
    if (!m_awaitedDef.isEmpty() &&
        held.value(QStringLiteral("nodes")) != m_awaitedDef.value(QStringLiteral("nodes"))) {
      m_sessionDef = m_awaitedDef; // given, not taken yet
    } else {
      m_awaitedDef = QJsonObject();
      m_sessionDef = held;
    }
    if (item.uid.isEmpty()) {
      item.uid = m_uid;
    }
    if (!(item.width > 0.0) && m_item.width > 0.0) {
      item.round = m_item.round; // (as read before)
      item.width = m_item.width;
      item.height = m_item.height;
    }
    const FlexItem standing = itemOfDefinition(m_sessionDef, item);
    FlexItem routed;
    try {
      routed = FlexItem::read(m_host.queryObject({{QStringLiteral("query"), QStringLiteral("flex")},
                                                 {QStringLiteral("uid"), m_uid},
                                                 {QStringLiteral("preview"), true},
                                                 {QStringLiteral("frames"), true}}),
                              &standing);
    } catch (const std::exception&) {
      // No preview held now: the nodes alone.
    }
    // (A preview of other nodes, an older one, is not this route.)
    bool same = routed.valid && routed.hasRoute() && routed.nodes.size() == standing.nodes.size();
    for (std::size_t i = 0; same && i < standing.nodes.size(); ++i) {
      same = routed.nodes[i].id == standing.nodes[i].id;
    }
    item = same ? routed : standing;
    item.valid = !item.nodes.empty();
  }
  if (!item.valid) {
    hide();
    return;
  }
  m_item = item;
  m_placement = m_hooks.placement ? m_hooks.placement(m_uid) : gp_Trsf();
  m_occurrence = m_hooks.occurrence ? m_hooks.occurrence(m_uid) : QString();
  // The colour the view draws its body in: the item pulled is drawn in it.
  m_bodyColor = viewColor(ColorRole::Body, m_viewer.backgroundDark());
  for (const BodyDisplay& body : m_viewer.shownBodies()) {
    if (body.color.isValid() && QString::fromStdString(body.uid).section(QLatin1Char('.'), 0, 0) == m_uid) {
      m_bodyColor = body.color;
    }
  }
  if (!m_ringNode.isEmpty() && m_item.node(m_ringNode) == nullptr) {
    m_ringNode.clear();
  }
  if (!m_selected.isEmpty() && m_item.node(m_selected) == nullptr) {
    m_selected.clear();
  }
  // (In a session a preview comes after each change: an angle being typed
  // stays while its node does.)
  if (!m_session || m_item.node(m_twistFieldNode) == nullptr) {
    closeTwistField(false);
  }
  m_hover = Hover();
  paintPlaces();
  refresh();
}

gp_Pnt FlexEditController::designPoint(const Xyz& local) const {
  gp_Pnt point(local[0], local[1], local[2]);
  point.Transform(m_placement);
  return point;
}

Xyz FlexEditController::localPoint(const gp_Pnt& design) const {
  const gp_Pnt point = design.Transformed(m_placement.Inverted());
  return {point.X(), point.Y(), point.Z()};
}

Xyz FlexEditController::designDirection(const Xyz& local) const {
  gp_Vec direction(local[0], local[1], local[2]);
  direction.Transform(m_placement);
  return {direction.X(), direction.Y(), direction.Z()};
}

QPointF FlexEditController::onScreen(const Xyz& local) const { return m_viewer.toWidgetF(designPoint(local)); }

double FlexEditController::pixelSize(const Xyz& local) const {
  const double size = m_host.pixelSize(designPoint(local));
  return std::isfinite(size) && size > 0.0 ? size : 1.0;
}

const FlexItem& FlexEditController::shown() const {
  return m_edit.preview && !m_edit.refused ? *m_edit.preview : m_item;
}

std::optional<QPointF> FlexEditController::addPlace(const FlexItem& item, bool start) const {
  if (item.nodes.size() < 2) {
    return std::nullopt;
  }
  const QPointF end = onScreen(start ? item.nodes.front().point : item.nodes.back().point);
  // Outwards along the route as the screen shows it: from the first place
  // of the route that is apart from the end there.
  QPointF away(start ? -1.0 : 1.0, -1.0);
  const auto from = [&](const Xyz& inner) {
    const QPointF d = end - onScreen(inner);
    if (std::hypot(d.x(), d.y()) > 3.0) {
      away = d;
      return true;
    }
    return false;
  };
  bool found = false;
  if (item.hasRoute()) {
    const std::size_t count = item.polyline.size();
    for (std::size_t i = 1; i < count && !found; ++i) {
      found = from(start ? item.polyline[i] : item.polyline[count - 1 - i]);
    }
  }
  if (!found) {
    from(start ? item.nodes[1].point : item.nodes[item.nodes.size() - 2].point);
  }
  const double size = std::hypot(away.x(), away.y());
  return end + away * (kAddOffset / size);
}

std::optional<FlexEditController::Ring> FlexEditController::ringOf(const FlexItem& item) const {
  // The node turned now, else the one under the pointer (it stays while
  // the pointer goes to its knob), else the selected one.
  const bool turning = m_edit.action == Action::Twist;
  const bool idle = m_edit.action == Action::None;
  const QString id = turning ? m_edit.node : idle && !m_ringNode.isEmpty() ? m_ringNode : idle ? m_selected : QString();
  const FlexNode* node = id.isEmpty() || item.round ? nullptr : item.node(id);
  if (node == nullptr) {
    return std::nullopt;
  }
  Xyz centre = node->point;
  Xyz axis{0.0, 0.0, 1.0};
  Xyz up{0.0, 1.0, 0.0};
  double twist = node->twist;
  if (turning) {
    // As the pointer turns it, without waiting for the preview.
    centre = m_edit.axisPoint;
    axis = m_edit.axis;
    up = turned(m_edit.startNormal, axis, m_edit.twist - m_edit.startTwist);
    twist = m_edit.twist;
  } else if (const std::optional<FlexFrame> frame = frameAt(item, node->along)) {
    axis = frame->tangent;
    up = frame->normal;
  } else {
    return std::nullopt;
  }
  const double radius = kRingPixels * pixelSize(centre);
  const Xyz side = cross(axis, up);
  Ring ring;
  ring.node = id;
  ring.twist = twist;
  ring.centre = onScreen(centre);
  ring.knob = onScreen(add(centre, scaled(up, radius)));
  for (int i = 0; i < kRingSamples; ++i) {
    const double angle = 2.0 * kPi * i / kRingSamples;
    ring.circle << onScreen(
        add(centre, add(scaled(up, radius * std::cos(angle)), scaled(side, radius * std::sin(angle)))));
  }
  return ring;
}

std::optional<std::pair<RoutePlace, double>> FlexEditController::routePlaceAt(const QPointF& position) const {
  if (!m_item.hasRoute()) {
    return std::nullopt;
  }
  const std::vector<double> lengths = lengthsAlong(m_item.polyline);
  double best = std::numeric_limits<double>::max();
  RoutePlace place;
  QPointF before = onScreen(m_item.polyline.front());
  for (std::size_t i = 1; i < m_item.polyline.size(); ++i) {
    const QPointF after = onScreen(m_item.polyline[i]);
    const double share = shareOnSegment(before, after, position);
    const double away = pixels(before + (after - before) * share, position);
    if (away < best) {
      best = away;
      place.point = add(m_item.polyline[i - 1], scaled(sub(m_item.polyline[i], m_item.polyline[i - 1]), share));
      place.along = lengths[i - 1] + (lengths[i] - lengths[i - 1]) * share;
    }
    before = after;
  }
  return std::make_pair(place, best);
}

FlexEditController::Hover FlexEditController::hoverAt(const QPointF& position) const {
  Hover hover;
  if (!m_item.valid) {
    return hover;
  }
  if (const std::optional<Ring> ring = ringOf(m_item)) {
    if (pixels(ring->knob, position) <= kKnobPixels) {
      hover.kind = Hover::Kind::Knob;
      hover.node = ring->node;
      return hover;
    }
  }
  double best = kGrabPixels;
  double nearestHandle = std::numeric_limits<double>::max();
  for (const FlexNode& node : m_item.nodes) {
    const double away = pixels(onScreen(node.point), position);
    nearestHandle = std::min(nearestHandle, away);
    if (away <= best) {
      best = away;
      hover.kind = Hover::Kind::Handle;
      hover.node = node.id;
    }
  }
  if (hover.kind != Hover::Kind::None) {
    return hover;
  }
  const std::optional<QPointF> startPlus = addPlace(m_item, true);
  if (startPlus && pixels(*startPlus, position) <= kAddPixels) {
    hover.kind = Hover::Kind::AddStart;
    return hover;
  }
  const std::optional<QPointF> endPlus = addPlace(m_item, false);
  if (endPlus && pixels(*endPlus, position) <= kAddPixels) {
    hover.kind = Hover::Kind::AddEnd;
    return hover;
  }
  if (nearestHandle > kClearPixels) {
    // Anywhere over the item as the screen shows it: within half its
    // section's reach of the centre line there (and a little more for a
    // thin one).
    if (const auto place = routePlaceAt(position)) {
      const double reach = 0.5 * m_item.reach() / pixelSize(place->first.point) + 2.0;
      if (place->second <= std::max(kRoutePixels, reach)) {
        // Only between two nodes and clear of each by the section's size
        // along the route: the item's end piece beyond its first or last
        // node is its "+" (mitcad#1713: a click there added a node at the
        // route's start "after" the first node, the route turned back on
        // itself and the item was lost), and a node beside another one's
        // place on the route would make it turn back too.
        const double clear = std::max(m_item.reach(), 1e-6);
        const double along = place->first.along;
        if (along <= m_item.nodes.front().along + clear) {
          hover.kind = Hover::Kind::AddStart;
          return hover;
        }
        if (along >= m_item.nodes.back().along - clear) {
          hover.kind = Hover::Kind::AddEnd;
          return hover;
        }
        const bool besideNode = std::any_of(m_item.nodes.begin(), m_item.nodes.end(), [&](const FlexNode& node) {
          return std::abs(node.along - along) < clear;
        });
        if (besideNode) {
          return hover;
        }
        hover.kind = Hover::Kind::Route;
        hover.place = place->first;
        hover.node = nodeBefore(m_item, along);
      }
    }
  }
  return hover;
}

QString FlexEditController::badgeText(bool* warns) const {
  const FlexItem& item = shown();
  const bool editing = m_edit.action != Action::None && m_edit.action != Action::Press &&
                       m_edit.action != Action::Menu;
  *warns = false;
  if (m_selected.isEmpty() && !editing) {
    return QString();
  }
  const QString unitName = m_host.lengthUnit();
  const double millimetres = m_host.unitMillimetres();
  QStringList lines;
  lines << tr("Length %1 %2").arg(sketch::formatLength(item.length, millimetres), unitName);
  const std::optional<double> bendLimit = item.bendLimit();
  if (item.smallestBend && bendLimit) {
    lines << tr("Tightest bend %1 %3, limit %2 %3")
                 .arg(sketch::formatLength(item.smallestBend->radius, millimetres),
                      sketch::formatLength(*bendLimit, millimetres), unitName);
    if (item.bendState() == FlexItem::BendState::Over) {
      *warns = true;
    }
  }
  // Over its edge apart (mitcad#1713: a warning about the edge stood under
  // the flat bend): against its limit, or where the item does not bend so
  // at all, only while the model warns of it.
  if (item.smallestBendEdge) {
    const std::optional<double> edgeLimit = item.edgeBendLimit();
    if (edgeLimit) {
      lines << tr("Tightest bend over its edge %1 %3, limit %2 %3")
                   .arg(sketch::formatLength(item.smallestBendEdge->radius, millimetres),
                        sketch::formatLength(*edgeLimit, millimetres), unitName);
    } else if (item.edgeBendState() == FlexItem::BendState::Over) {
      lines << tr("Tightest bend over its edge %1 %2, its type does not bend over its edge")
                   .arg(sketch::formatLength(item.smallestBendEdge->radius, millimetres), unitName);
    }
    if (item.edgeBendState() == FlexItem::BendState::Over) {
      *warns = true;
    }
  }
  if (m_edit.action == Action::Twist) {
    lines << tr("Twist %1").arg(degreesText(m_edit.twist));
  }
  if (editing && m_edit.target &&
      (m_edit.action == Action::Drag || m_edit.action == Action::Extend || m_edit.action == Action::Insert)) {
    switch (m_edit.target->kind) {
    case NodeTarget::Kind::Face:
      lines << tr("On this face");
      break;
    case NodeTarget::Kind::Port:
      lines << tr("Joins this end");
      break;
    case NodeTarget::Kind::Link:
      lines << (m_edit.targetOnCurve ? tr("On this edge") : tr("On this point"));
      break;
    case NodeTarget::Kind::Free:
      lines << (m_edit.forcedFree ? tr("Free in space (Alt held)") : tr("Free in space"));
      break;
    }
  }
  if (m_edit.refused) {
    lines << tr("The item cannot be routed here: release leaves it as it was.");
    *warns = true;
  } else if (item.status == QStringLiteral("error")) {
    lines << tr("No item can be made on this route.");
    *warns = true;
  }
  for (const QString& message : item.problemMessages()) {
    lines << sentence(modelMessage(message));
    *warns = true;
  }
  return lines.join(QLatin1Char('\n'));
}

FlexEditController::Scene FlexEditController::scene() const {
  Scene out;
  if (!isActive()) {
    return out;
  }
  const FlexItem& item = shown();
  const bool idle = m_edit.action == Action::None;
  out.refused = m_edit.refused;
  // A node in hand whose route is not worked out yet (owner 14.11.2026:
  // without Shift, until the pointer rests): the item's centre line as it
  // was and dashed lines from the nodes beside the target to it.
  const bool placing = m_edit.action == Action::Drag || m_edit.action == Action::Extend ||
                       (m_edit.action == Action::Insert && m_edit.moved);
  const bool waiting = m_edit.began && placing && !m_edit.preview && !m_edit.refused;
  if (waiting && m_edit.target) {
    const QPointF to = onScreen(m_edit.target->at);
    const auto index = [this](const QString& id) {
      for (std::size_t i = 0; i < m_item.nodes.size(); ++i) {
        if (m_item.nodes[i].id == id) {
          return static_cast<int>(i);
        }
      }
      return -1;
    };
    const int count = static_cast<int>(m_item.nodes.size());
    std::vector<int> beside;
    if (m_edit.action == Action::Drag) {
      const int k = index(m_edit.node);
      beside = {k - 1, k + 1};
      if (k < 0) {
        beside.clear();
      }
    } else if (m_edit.action == Action::Extend) {
      beside = {m_edit.atStart ? 0 : count - 1};
    } else {
      const int k = index(m_edit.node);
      beside = {k, k + 1};
      if (k < 0) {
        beside.clear();
      }
    }
    for (const int i : beside) {
      if (i >= 0 && i < count) {
        out.dashes.emplace_back(onScreen(m_item.nodes[static_cast<std::size_t>(i)].point), to);
      }
    }
  }
  if (m_edit.began && !m_edit.refused && !waiting) {
    // Pulled: the item itself, where the route is now.
    out.rope = ropeSamples(item, m_placement);
    out.look = ropeLook(item, m_bodyColor);
  } else {
    for (const Xyz& point : item.polyline) {
      out.route << onScreen(point);
    }
  }
  // Ringed only where it is a fault (FlexItem::bendState), over the flat
  // side or over the edge (the edge's where both are).
  if (item.smallestBendEdge && item.edgeBendState() == FlexItem::BendState::Over) {
    out.bendMark = onScreen(item.smallestBendEdge->point);
    out.bendOver = true;
  } else if (item.smallestBend && item.bendState() == FlexItem::BendState::Over) {
    out.bendMark = onScreen(item.smallestBend->point);
    out.bendOver = true;
  }
  for (const FlexNode& node : item.nodes) {
    out.handles.push_back({node.id, onScreen(node.point), node.support, node.lost, item.isEnd(node.id)});
  }
  if (idle) {
    out.addStart = addPlace(item, true);
    out.addEnd = addPlace(item, false);
    out.addHovered = m_hover.kind == Hover::Kind::AddStart || m_hover.kind == Hover::Kind::AddEnd;
    if (m_hover.kind == Hover::Kind::Route) {
      out.insert = onScreen(m_hover.place.point);
    }
    if (m_hover.kind == Hover::Kind::Handle) {
      out.hovered = m_hover.node;
    }
  } else if (m_edit.action == Action::Insert && !m_edit.moved) {
    out.insert = onScreen(m_edit.anchor);
  }
  if (m_edit.target && (m_edit.action == Action::Drag || m_edit.action == Action::Extend ||
                        (m_edit.action == Action::Insert && m_edit.moved))) {
    out.target = onScreen(m_edit.target->at);
    out.targetAttached = m_edit.target->kind != NodeTarget::Kind::Free;
  }
  out.ring = ringOf(item);
  out.selected = m_selected;
  if (m_edit.action == Action::Drag || m_edit.action == Action::Press) {
    out.dragged = m_edit.node;
  }
  out.badge = badgeText(&out.badgeWarns);
  if (!out.badge.isEmpty()) {
    const FlexNode* beside = item.node(m_edit.action == Action::Drag || m_edit.action == Action::Twist
                                           ? m_edit.node
                                           : m_selected);
    if (m_edit.action == Action::Extend || m_edit.action == Action::Insert) {
      beside = nullptr; // beside the pointer: the node is not there yet
    }
    out.badgeAt = beside != nullptr ? onScreen(beside->point) : m_pointer;
    if (out.target) {
      out.badgeAt = *out.target;
    } else if (beside != nullptr && out.ring && out.ring->node == beside->id) {
      // Clear of the twist ring at that node, its knob and its angle
      // (mitcad#1713: the badge covered the knob and the angle).
      out.badgeAround = out.ring->circle.boundingRect().adjusted(-kRingLabelPixels, -kRingLabelPixels,
                                                                 kRingLabelPixels, kRingLabelPixels);
    }
  }
  return out;
}

void FlexEditController::refresh() {
  if (!isActive()) {
    return;
  }
  m_overlay->update();
  if (m_edit.action == Action::None) {
    logPlaces();
  }
  bool warns = false;
  const QString badge = badgeText(&warns).replace(QLatin1Char('\n'), QStringLiteral(" | "));
  // (Logged again when it moves to another node: a node taken in hand
  // shows the same length beside it.)
  const QString logged = badge + QLatin1Char('@') + m_selected;
  if (logged != m_loggedBadge) {
    m_loggedBadge = logged;
    if (!badge.isEmpty()) {
      log(QStringLiteral("Flex edit badge: %1").arg(badge));
    }
  }
}

void FlexEditController::logPlaces() {
  if (!isActive() || !m_item.valid) {
    return;
  }
  const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
  QStringList lines;
  for (const FlexNode& node : m_item.nodes) {
    lines << QStringLiteral("Flex node %1 %2%3 at %4")
                 .arg(node.id, node.support, node.lost ? QStringLiteral(" lost") : QString(),
                      windowPlace(onScreen(node.point), window));
  }
  if (const std::optional<QPointF> plusAt = addPlace(m_item, true)) {
    lines << QStringLiteral("Flex add start at %1").arg(windowPlace(*plusAt, window));
  }
  if (const std::optional<QPointF> plusAt = addPlace(m_item, false)) {
    lines << QStringLiteral("Flex add end at %1").arg(windowPlace(*plusAt, window));
  }
  for (const Stretch& stretch : stretches(m_item)) {
    lines << QStringLiteral("Flex insert after %1 at %2")
                 .arg(stretch.after, windowPlace(onScreen(stretch.middle.point), window));
  }
  if (const std::optional<Ring> ring = ringOf(m_item)) {
    lines << QStringLiteral("Flex twist knob %1 at %2").arg(ring->node, windowPlace(ring->knob, window));
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      log(line);
    }
  }
}

void FlexEditController::paintPlaces() {
  QJsonArray places = isActive() ? shown().problemPlaces() : QJsonArray();
  if (isActive() && m_edit.target && m_edit.target->kind != NodeTarget::Kind::Free) {
    // What the node would take, as a notice's place.
    const QJsonObject ref = m_edit.target->ref.toObject();
    QJsonObject place;
    const auto bodyPlace = [&](const char* kind) {
      place = QJsonObject{{QStringLiteral("kind"), QString::fromLatin1(kind)},
                          {QStringLiteral("body"), ref.value(QStringLiteral("body"))},
                          {QStringLiteral("name"), ref.value(QString::fromLatin1(kind))}};
    };
    if (ref.contains(QStringLiteral("face"))) {
      bodyPlace("face");
    } else if (ref.contains(QStringLiteral("edge"))) {
      bodyPlace("edge");
    } else if (ref.contains(QStringLiteral("vertex"))) {
      bodyPlace("vertex");
    } else if (ref.contains(QStringLiteral("point")) && ref.contains(QStringLiteral("sketch"))) {
      place = QJsonObject{{QStringLiteral("kind"), QStringLiteral("sketch_point")},
                          {QStringLiteral("sketch"), ref.value(QStringLiteral("sketch"))},
                          {QStringLiteral("name"), ref.value(QStringLiteral("point"))}};
    }
    const QString occurrence = ref.value(QStringLiteral("occurrence")).toString();
    if (!place.isEmpty() && !occurrence.isEmpty()) {
      place.insert(QStringLiteral("occurrence"), occurrence);
    }
    if (!place.isEmpty()) {
      places.append(place);
    }
  }
  if (places == m_paintedPlaces) {
    return;
  }
  m_paintedPlaces = places;
  if (m_hooks.paintPlaces) {
    m_hooks.paintPlaces(QStringLiteral("Flex"), places);
  }
}

void FlexEditController::setHover(const Hover& hover) {
  if (hover == m_hover) {
    return;
  }
  m_hover = hover;
  m_viewer.setDragCursor(hover.kind == Hover::Kind::Handle || hover.kind == Hover::Kind::Knob);
  m_overlay->update();
}

void FlexEditController::selectNode(const QString& id) {
  if (id == m_selected) {
    return;
  }
  closeTwistField(false);
  m_selected = id;
  log(QStringLiteral("Flex node selected: %1").arg(id.isEmpty() ? QStringLiteral("none") : id));
  refresh();
}

bool FlexEditController::refused() { return m_hooks.refuseEdit && m_hooks.refuseEdit(); }

SelectFilter FlexEditController::pickFilter() const {
  // What a pulled node can be put on, whatever the idle selection takes.
  return SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::SketchPoint | SelectKind::Point;
}

RopeView FlexEditController::ropeView() const {
  RopeView view;
  view.project = [this](const gp_Pnt& point) { return m_viewer.toWidgetF(point); };
  view.pixelSize = [this](const gp_Pnt& point) { return m_host.pixelSize(point); };
  view.towardViewer = m_viewer.viewDirection().Reversed();
  return view;
}

void FlexEditController::loadPorts() {
  // The ends an end node can join: every Pipe's and other Flex item's, not
  // the item's own. A port of another component is asked for again where
  // its component is seen, so that its place is in the item's coordinates
  // and its reference says where it was picked.
  m_ports.clear();
  const auto take = [this](const QJsonArray& ports, const QString& occurrence) {
    for (const QJsonValue& value : ports) {
      const QJsonObject port = value.toObject();
      const std::optional<Xyz> position = xyzOf(port.value(QStringLiteral("position")));
      const QString feature = port.value(QStringLiteral("feature")).toString();
      QJsonObject ref = port.value(QStringLiteral("ref")).toObject();
      if (!position || feature == m_uid || ref.isEmpty()) {
        continue;
      }
      if (occurrence != m_occurrence) {
        ref.insert(QStringLiteral("occurrence"), occurrence);
        if (!m_occurrence.isEmpty()) {
          ref.insert(QStringLiteral("context"), m_occurrence);
        }
      }
      m_ports.push_back({ref, *position, feature});
    }
  };
  try {
    const QJsonArray listed = m_host.query({{QStringLiteral("query"), QStringLiteral("end_ports")}}).toArray();
    QJsonArray own;
    QStringList elsewhere; // bodies of other components, each asked once
    for (const QJsonValue& value : listed) {
      const QString body =
          value.toObject().value(QStringLiteral("ref")).toObject().value(QStringLiteral("body")).toString();
      if (m_host.bodyOccurrence(body) == m_occurrence) {
        own.append(value);
      } else if (!elsewhere.contains(body)) {
        elsewhere << body;
      }
    }
    take(own, m_occurrence);
    const QString component = m_hooks.component ? m_hooks.component(m_uid) : QString();
    for (const QString& body : std::as_const(elsewhere)) {
      const QString occurrence = m_host.bodyOccurrence(body);
      QJsonObject query{{QStringLiteral("query"), QStringLiteral("end_ports")},
                        {QStringLiteral("body"), body},
                        {QStringLiteral("occurrence"), occurrence}};
      if (!m_occurrence.isEmpty()) {
        query.insert(QStringLiteral("context"), m_occurrence);
      }
      if (!component.isEmpty()) {
        query.insert(QStringLiteral("component"), component);
      }
      take(m_host.query(query).toArray(), occurrence);
    }
  } catch (const std::exception&) {
    // No ports, or only those read so far: faces and space are left.
  }
}

void FlexEditController::fadeBody() {
  // (In a session no route is drawn while a node is pulled, and the
  // panel's previews replace the bodies shown meanwhile: the item stays as
  // it is, the dashed lines show where the node goes.)
  if (m_session) {
    return;
  }
  // The item's own body see-through and not picked while the item is
  // pulled: the overlay draws the item where its route is now.
  m_savedBodies = m_viewer.shownBodies();
  std::vector<BodyDisplay> bodies = m_savedBodies;
  bool any = false;
  for (BodyDisplay& body : bodies) {
    if (QString::fromStdString(body.uid).section(QLatin1Char('.'), 0, 0) == m_uid) {
      body.style = BodyDisplay::Style::Ghost;
      any = true;
    }
  }
  if (any) {
    m_viewer.setBodies(bodies);
    m_bodiesFaded = true;
    log(QStringLiteral("Flex edit: the item's body faded, the item drawn from its route"));
  } else {
    m_savedBodies.clear();
  }
  m_viewer.setHighlights({}); // the selection's highlight of the old solid too
}

void FlexEditController::showBody(bool stale) {
  if (m_bodiesFaded) {
    if (!stale) {
      m_viewer.setBodies(m_savedBodies);
    }
    log(QStringLiteral("Flex edit: the item's body shown again"));
  }
  m_bodiesFaded = false;
  m_savedBodies.clear();
}

bool FlexEditController::begin(Action action) {
  // (In a session the model may be computing the panel's preview: a drag
  // needs none of it.)
  if (refused() || (!m_session && m_host.modelBusy())) {
    return false;
  }
  closeTwistField(false);
  QJsonObject def;
  if (m_session) {
    def = m_sessionDef; // the session's nodes (owner 14.11.2026)
  } else {
    try {
      def = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), m_uid}})
                .value(QStringLiteral("def"))
                .toObject();
    } catch (const std::exception&) {
      return false; // gone from the model
    }
  }
  if (def.isEmpty()) {
    return false;
  }
  m_ports.clear();
  if (action != Action::Twist) {
    loadPorts();
  }
  m_edit.action = action;
  m_edit.def = def;
  m_edit.began = true;
  // (In a session the panel's Route input picks what a node goes on.)
  if (action != Action::Twist && !m_session) {
    m_viewer.setPickFilter(pickFilter());
  }
  fadeBody();
  setHover(Hover());
  m_ringNode.clear();
  return true;
}

void FlexEditController::end(bool restore, bool bodiesStale) {
  const bool previewShown = m_edit.shown;
  const bool began = m_edit.began;
  m_edit = Edit();
  ++m_generation;
  m_loggedTarget.clear();
  m_loggedPreview.clear();
  if (previewShown) {
    try {
      m_host.clearModelPreview();
    } catch (const std::exception&) {
      // Nothing was held then.
    }
  }
  showBody(bodiesStale);
  if (began && restore && m_hooks.restoreView) {
    m_hooks.restoreView();
  }
  if (isActive()) {
    paintPlaces();
    refresh();
  }
  if (m_reloadAfter && isActive()) {
    // The session's preview that came meanwhile, once the model is idle.
    QTimer::singleShot(0, this, [this] {
      if (isActive() && m_edit.action == Action::None) {
        reload();
      }
    });
  }
}

std::optional<NodeTarget> FlexEditController::targetAt(const QPointF& position, bool endNode, bool forceFree,
                                                       bool* onCurve) const {
  *onCurve = false;
  if (m_viewer.isCovered(position)) {
    return std::nullopt; // over a panel: the last place stays
  }
  const auto ray = m_viewer.pointerRay(position);
  if (!forceFree) {
    // An end near the pointer takes an end node.
    if (endNode) {
      for (const Port& port : m_ports) {
        if (pixels(onScreen(port.position), position) <= kPortPixels) {
          return NodeTarget{NodeTarget::Kind::Port, port.position, port.ref};
        }
      }
    }
    // What is under the pointer, nearest first as a click takes it (an
    // edge or a point before the face it lies on); the item's own body
    // apart.
    const std::vector<SelectionItem> under = ray ? m_viewer.itemsAt(position) : std::vector<SelectionItem>();
    for (const SelectionItem& item : under) {
      const bool ofBody =
          item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
      if (ofBody && item.owner.section(QLatin1Char('.'), 0, 0) == m_uid) {
        continue;
      }
      // Geometry of another component is linked through where it was
      // picked and where the item's own component is seen.
      const auto reference = [this, &item]() -> QJsonValue {
        const QJsonValue plain = item.reference();
        if (!plain.isObject() || item.occurrence == m_occurrence) {
          return plain;
        }
        QJsonObject ref = plain.toObject();
        ref.insert(QStringLiteral("occurrence"), item.occurrence);
        if (!m_occurrence.isEmpty()) {
          ref.insert(QStringLiteral("context"), m_occurrence);
        }
        return ref;
      };
      if (item.kind == SelectKind::Face) {
        if (endNode) {
          const auto port = std::find_if(m_ports.begin(), m_ports.end(), [&item](const Port& candidate) {
            return candidate.ref.value(QStringLiteral("body")).toString() == item.owner &&
                   candidate.ref.value(QStringLiteral("face")).toString() == item.name;
          });
          if (port != m_ports.end()) {
            return NodeTarget{NodeTarget::Kind::Port, port->position, port->ref};
          }
        }
        if (const std::optional<gp_Pnt> hit = rayHit(m_host.itemShape(item), gp_Lin(ray->first, ray->second))) {
          return NodeTarget{NodeTarget::Kind::Face, localPoint(*hit), reference()};
        }
      } else if (item.kind == SelectKind::Edge) {
        const std::optional<gp_Pnt> on = edgePointAt(m_host.itemShape(item), position);
        if (on) {
          *onCurve = true;
          return NodeTarget{NodeTarget::Kind::Link, localPoint(*on), reference()};
        }
      } else if (item.kind == SelectKind::Vertex || item.kind == SelectKind::SketchPoint ||
                 item.kind == SelectKind::Point) {
        if (const std::optional<gp_Pnt> at = vertexPoint(m_host.itemShape(item))) {
          return NodeTarget{NodeTarget::Kind::Link, localPoint(*at), reference()};
        }
      }
    }
  }
  // Off everything, or kept off it: free on the view's plane through the
  // last place.
  if (const std::optional<gp_Pnt> free = m_viewer.viewPlanePoint(position, designPoint(m_edit.anchor))) {
    return NodeTarget{NodeTarget::Kind::Free, localPoint(*free), QJsonValue()};
  }
  return std::nullopt;
}

std::optional<gp_Pnt> FlexEditController::edgePointAt(const TopoDS_Shape& shape, const QPointF& position) const {
  // The place of the edge nearest to the pointer on the screen, of
  // samples along it: the model then takes the edge's point nearest to it.
  std::optional<gp_Pnt> best;
  double nearest = std::numeric_limits<double>::max();
  try {
    for (TopExp_Explorer edges(shape, TopAbs_EDGE); edges.More(); edges.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      const double first = curve.FirstParameter();
      const double last = curve.LastParameter();
      if (!std::isfinite(first) || !std::isfinite(last)) {
        continue;
      }
      for (int i = 0; i <= kEdgeSamples; ++i) {
        const gp_Pnt point = curve.Value(first + (last - first) * i / kEdgeSamples);
        const double away = pixels(m_viewer.toWidgetF(point), position);
        if (away < nearest) {
          nearest = away;
          best = point;
        }
      }
    }
  } catch (const Standard_Failure&) {
    return std::nullopt; // the node is not put on it then
  }
  return best;
}

void FlexEditController::pointerTo(const QPointF& position, Qt::KeyboardModifiers modifiers) {
  m_pointer = position;
  const bool endNode =
      m_edit.action == Action::Extend || (m_edit.action == Action::Drag && m_item.isEnd(m_edit.node));
  // Alt keeps the node off what is under the pointer (owner 14.11.2026:
  // Alt alone, Shift being the live route's).
  const bool forceFree = modifiers.testFlag(Qt::AltModifier);
  const bool live = modifiers.testFlag(Qt::ShiftModifier);
  bool onCurve = false;
  const std::optional<NodeTarget> target = targetAt(position, endNode, forceFree, &onCurve);
  if (!target ||
      (m_edit.target && *m_edit.target == *target && forceFree == m_edit.forcedFree && live == m_edit.live)) {
    return;
  }
  const bool moved = !m_edit.target || !(*m_edit.target == *target);
  m_edit.target = target;
  m_edit.targetOnCurve = onCurve;
  m_edit.forcedFree = forceFree && target->kind == NodeTarget::Kind::Free;
  m_edit.live = live;
  const QString logged = targetLog(*target) + (m_edit.forcedFree ? QStringLiteral(" (Alt)") : QString());
  if (logged != m_loggedTarget) {
    m_loggedTarget = logged;
    log(QStringLiteral("Flex edit target: %1").arg(logged));
  }
  paintPlaces();
  if (live) {
    schedulePreview(); // the route on every move, as it follows
  } else {
    // A dashed line to the pointer, no model call, until the pointer rests
    // here: then the route once (each place a drag ends on is a node).
    // The route shown before is of where the pointer was.
    if (moved && !m_edit.running) {
      m_edit.preview.reset();
      m_edit.refused = false;
      m_edit.previewedTarget.reset();
    }
    const int serial = ++m_edit.restSerial;
    QTimer::singleShot(kRestMs, this, [this, generation = m_generation, serial] {
      if (generation == m_generation && serial == m_edit.restSerial && isActive()) {
        schedulePreview();
      }
    });
  }
  refresh();
}

void FlexEditController::turnTo(const QPointF& position) {
  m_pointer = position;
  // A press on the knob that does not move turns nothing (nor snaps).
  if (!m_edit.moved) {
    if (pixels(position, m_edit.pressAt) <= kClickPixels) {
      return;
    }
    m_edit.moved = true;
  }
  const Xyz axis = unit(designDirection(m_edit.axis));
  const std::optional<gp_Pnt> on = m_viewer.planePoint(
      position, gp_Ax3(designPoint(m_edit.axisPoint), gp_Dir(axis[0], axis[1], axis[2])));
  if (!on) {
    return; // the ring is seen edge-on here: the last angle stays
  }
  const Xyz toward = sub(localPoint(*on), m_edit.axisPoint);
  if (length(toward) < 1e-9) {
    return;
  }
  const double twist =
      snappedTwist(m_edit.startTwist + angleAbout(m_edit.startVector, toward, m_edit.axis), kSnapTwist);
  if (std::abs(twist - m_edit.twist) < 1e-9) {
    return;
  }
  m_edit.twist = twist;
  schedulePreview();
  refresh();
}

void FlexEditController::schedulePreview() {
  // In an edit session no route follows the pointer (owner 14.11.2026):
  // dashed lines while a node is pulled, the panel's preview after the
  // release.
  if (m_session) {
    return;
  }
  // Never more than one preview waiting: while one is computed, the
  // pointer's newest place is taken when it ends.
  if (m_edit.running) {
    m_edit.again = true;
    return;
  }
  if (m_edit.pending) {
    return;
  }
  m_edit.pending = true;
  QTimer::singleShot(0, this, [this, generation = m_generation] {
    if (generation == m_generation) {
      applyPreview();
    }
  });
}

QJsonObject FlexEditController::previewDefinition() const {
  switch (m_edit.action) {
  case Action::Drag:
    return withNodeAt(m_edit.def, m_edit.node, *m_edit.target);
  case Action::Extend:
    return withNodeAdded(m_edit.def, *m_edit.target,
                         m_edit.atStart ? QStringLiteral("start") : QStringLiteral("end"));
  case Action::Insert:
    return withNodeAdded(m_edit.def, *m_edit.target, QJsonObject{{QStringLiteral("after"), m_edit.node}});
  case Action::Twist:
    return withTwist(m_edit.def, m_edit.node, m_edit.twist);
  default:
    break;
  }
  return QJsonObject();
}

void FlexEditController::applyPreview() {
  m_edit.pending = false;
  const bool turning = m_edit.action == Action::Twist;
  const bool placing = m_edit.action == Action::Drag || m_edit.action == Action::Extend ||
                       (m_edit.action == Action::Insert && m_edit.moved);
  if (!isActive() || !m_edit.began || (!turning && !placing)) {
    return;
  }
  if (m_host.modelBusy()) {
    // A job computes the model: after it.
    m_edit.pending = true;
    QTimer::singleShot(15, this, [this, generation = m_generation] {
      if (generation == m_generation) {
        applyPreview();
      }
    });
    return;
  }
  // Nothing new since the last one.
  if (turning) {
    if (m_edit.previewedTwist && std::abs(*m_edit.previewedTwist - m_edit.twist) < 1e-9) {
      return;
    }
  } else if (!m_edit.target || (m_edit.previewedTarget && *m_edit.previewedTarget == *m_edit.target)) {
    return;
  }
  const std::optional<NodeTarget> target = m_edit.target;
  const double twist = m_edit.twist;
  const QJsonObject def = previewDefinition();
  const quint64 generation = m_generation;
  std::optional<FlexItem> preview;
  QString problem;
  m_edit.running = true;
  try {
    // The route alone: no solid is built for a pointer move.
    m_host.modelPreview(compactJson(editFeatureCommand(m_uid, routeOnly(def))), m_item.name);
    QJsonObject query{{QStringLiteral("query"), QStringLiteral("flex")},
                      {QStringLiteral("uid"), m_uid},
                      {QStringLiteral("preview"), true},
                      {QStringLiteral("frames"), true}}; // how the section lies, for the item drawn
    preview = FlexItem::read(m_host.queryObject(query), &m_item);
    if (!preview->valid) {
      preview.reset();
    }
  } catch (const std::exception& failure) {
    problem = QString::fromUtf8(failure.what());
  }
  if (generation != m_generation) {
    // It ended meanwhile: nothing of it is shown, and the model holds no
    // preview of it.
    try {
      m_host.clearModelPreview();
    } catch (const std::exception&) {
      // Nothing was held then.
    }
    return;
  }
  m_edit.running = false;
  m_edit.shown = true;
  m_edit.previewedTarget = target;
  m_edit.previewedTwist = twist;
  m_edit.refused = !preview.has_value();
  m_edit.problem = problem;
  m_edit.preview = preview;
  if (!turning && !m_edit.live && m_edit.target && target && !(*m_edit.target == *target)) {
    // The pointer went on meanwhile without Shift: this route is of where
    // it was; the dashed line again until it rests (owner 14.11.2026).
    m_edit.refused = false;
    m_edit.preview.reset();
    m_edit.previewedTarget.reset();
  }
  const QString state = m_edit.refused
                            ? QStringLiteral("refused: %1").arg(problem)
                            : QStringLiteral("%1 nodes, %2 warnings")
                                  .arg(preview->nodes.size())
                                  .arg(preview->problems.size());
  if (state != m_loggedPreview) {
    m_loggedPreview = state;
    log(QStringLiteral("Flex edit preview: %1").arg(state));
  }
  paintPlaces();
  refresh();
  if (m_edit.cancelAfter) {
    cancel();
    return;
  }
  if (m_edit.commitAfter) {
    commit();
    return;
  }
  if (m_edit.again) {
    m_edit.again = false;
    schedulePreview();
  }
}

bool FlexEditController::run(const QJsonObject& command, const QString& what, QJsonObject* result) {
  if (m_session) {
    // The edit session (owner 14.11.2026): the change goes into the panel,
    // which previews it; OK keeps the session as one undo step.
    log(QStringLiteral("Flex edit session change: %1").arg(QString::fromUtf8(compactJson(command))));
    QString made;
    const QJsonObject def = withNodeCommand(m_sessionDef, command, &made);
    if (def.isEmpty() || !m_hooks.sessionApply ||
        !m_hooks.sessionApply(def.value(QStringLiteral("nodes")).toArray(), what)) {
      log(QStringLiteral("Flex edit %1: not taken; nothing changed").arg(what));
      refresh();
      return false;
    }
    // The handles at the nodes given until the panel's preview shows their
    // route (reload).
    m_sessionDef = def;
    m_awaitedDef = def;
    m_item = itemOfDefinition(def, m_item);
    if (result != nullptr && !made.isEmpty()) {
      result->insert(QStringLiteral("node"), made);
    }
    QStringList ids;
    for (const FlexNode& node : m_item.nodes) {
      ids << node.id;
    }
    log(QStringLiteral("Flex edit %1: in the session; nodes %2").arg(what, ids.join(QLatin1Char(' '))));
    paintPlaces();
    refresh();
    return true;
  }
  log(QStringLiteral("Flex edit command: %1").arg(QString::fromUtf8(compactJson(command))));
  QJsonObject answer;
  if (!m_host.runCommand(command, &answer)) {
    log(QStringLiteral("Flex edit %1: refused by the model; nothing changed").arg(what));
    m_host.refreshScene();
    return false;
  }
  // The item could not be made at all with the edit (no route, no solid):
  // the edit is taken back, so that the item is not lost for it
  // (mitcad#1713), and the message band says why in words. Only then: an
  // item made with warnings (a bend tighter than its radius, a body passed
  // through) is made, and the edit is kept (editFailure). (An item that had
  // failed before keeps the edit, `new` false: it may be a step towards
  // mending it.)
  const QString failure = editFailure(answer, m_uid);
  if (!failure.isEmpty()) {
    log(QStringLiteral("Flex edit %1: the item failed (%2); undone").arg(what, failure));
    m_host.runCommand(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("undo")}});
    m_host.refreshScene();
    reload();
    // (Last, after the undo's own status and the scene's: the owner saw only
    // a flash when the undo's "Recomputed" line came after it.)
    const QString message = tr("The item could not be made so, and the change was taken back: %1").arg(failure);
    m_host.showError(message);
    log(QStringLiteral("Flex edit %1: said \"%2\"").arg(what, message));
    return false;
  }
  if (result != nullptr) {
    *result = answer;
  }
  m_host.refreshScene();
  reload(); // as the model has it now, also where the main window did not ask
  if (isActive()) {
    QStringList ids;
    for (const FlexNode& node : m_item.nodes) {
      ids << node.id;
    }
    log(QStringLiteral("Flex edit %1: done; nodes %2, length %3, %4 warnings")
            .arg(what, ids.join(QLatin1Char(' ')))
            .arg(m_item.length, 0, 'f', 3)
            .arg(m_item.problems.size()));
  }
  return true;
}

void FlexEditController::commit() {
  if (m_edit.running) {
    m_edit.commitAfter = true; // once the preview under way has ended
    return;
  }
  const Edit edit = m_edit;
  QJsonObject command;
  QString what;
  switch (edit.action) {
  case Action::Drag:
    what = QStringLiteral("drag %1").arg(edit.node);
    if (edit.target && !edit.refused) {
      command = dragCommand(m_item, edit.node, *edit.target);
    }
    break;
  case Action::Extend:
    what = edit.atStart ? QStringLiteral("add at the start") : QStringLiteral("add at the end");
    if (edit.target && !edit.refused) {
      command = addNodeCommand(m_uid, *edit.target,
                               edit.atStart ? QStringLiteral("start") : QStringLiteral("end"));
    }
    break;
  case Action::Insert:
    what = QStringLiteral("add after %1").arg(edit.node);
    if (edit.target && !edit.refused) {
      command = addNodeCommand(m_uid, *edit.target, QJsonObject{{QStringLiteral("after"), edit.node}});
    }
    break;
  case Action::Twist:
    what = QStringLiteral("twist %1").arg(edit.node);
    if (!edit.refused && std::abs(edit.twist - edit.startTwist) > 1e-9) {
      command = setTwistCommand(m_uid, edit.node, edit.twist);
    }
    break;
  default:
    break;
  }
  end(true);
  if (command.isEmpty()) {
    log(QStringLiteral("Flex edit %1: nothing taken, nothing changed").arg(what));
    return;
  }
  QJsonObject result;
  if (run(command, what, &result) && (edit.action == Action::Extend || edit.action == Action::Insert)) {
    // The new node is the one in hand next.
    const QString made = result.value(QStringLiteral("node")).toString();
    if (isActive() && m_item.node(made) != nullptr) {
      selectNode(made);
    }
    // In a session "+" stays on: the next click adds the next end node,
    // until Esc or a click on that "+" again (owner 14.11.2026).
    // (Not after "+" dragged out: then nothing stays on the pointer.)
    if (m_session && edit.action == Action::Extend && edit.armed && isActive()) {
      armExtend(edit.atStart);
    }
  }
}

void FlexEditController::armExtend(bool atStart) {
  if (m_item.nodes.empty() || !begin(Action::Extend)) {
    return;
  }
  m_edit.atStart = atStart;
  m_edit.armed = true;
  m_edit.pressAt = m_pointer;
  m_edit.node = atStart ? m_item.nodes.front().id : m_item.nodes.back().id;
  m_edit.anchor = atStart ? m_item.nodes.front().point : m_item.nodes.back().point;
  m_host.showHint(tr("Click where the next node goes. Esc, or a click on + again, ends adding nodes."));
  log(QStringLiteral("Flex edit add: armed at the %1, each click adds a node")
          .arg(atStart ? QStringLiteral("start") : QStringLiteral("end")));
  refresh();
}

void FlexEditController::commitInsertAtRoute() {
  // A click on the route: the node goes where the route is, on the face
  // it lies on there, so that the item keeps its way.
  const Edit edit = m_edit;
  end(false);
  if (refused()) {
    return;
  }
  NodeTarget target{NodeTarget::Kind::Free, edit.anchor, QJsonObject()};
  if (const std::optional<FlexRun> on = runAt(m_item, edit.along)) {
    target.kind = NodeTarget::Kind::Face;
    target.ref = QJsonObject{{QStringLiteral("body"), on->body}, {QStringLiteral("face"), on->face}};
  }
  QJsonObject result;
  if (run(addNodeCommand(m_uid, target, QJsonObject{{QStringLiteral("after"), edit.node}}),
          QStringLiteral("add after %1").arg(edit.node), &result)) {
    const QString made = result.value(QStringLiteral("node")).toString();
    if (isActive() && m_item.node(made) != nullptr) {
      selectNode(made);
    }
  }
}

bool FlexEditController::cancel() {
  if (!isActive()) {
    return false;
  }
  if (m_twistField->isVisible()) {
    closeTwistField(false);
    return true;
  }
  if (m_edit.action != Action::None) {
    if (m_edit.running) {
      m_edit.cancelAfter = true;
      m_edit.commitAfter = false;
      return true;
    }
    log(QStringLiteral("Flex edit: cancelled, nothing changed"));
    end(true);
    return true;
  }
  if (!m_selected.isEmpty()) {
    selectNode(QString());
    return true;
  }
  return false;
}

bool FlexEditController::deleteSelectedNode() {
  if (!isActive() || m_selected.isEmpty() || m_edit.action != Action::None) {
    return false;
  }
  if (refused()) {
    return true;
  }
  if (m_item.nodes.size() <= 2) {
    m_host.showStatus(tr("An item keeps at least two nodes."), NoticeLevel::Information);
    log(QStringLiteral("Flex edit delete %1: refused, two nodes are left").arg(m_selected));
    return true;
  }
  const QString node = m_selected;
  selectNode(QString());
  run(deleteNodeCommand(m_uid, node), QStringLiteral("delete %1").arg(node));
  return true;
}

void FlexEditController::showMenu(const QString& id, const QPoint& globalPosition) {
  const FlexNode* node = m_item.node(id);
  if (node == nullptr) {
    return;
  }
  selectNode(id);
  QMenu menu(&m_viewer);
  menu.setObjectName(QStringLiteral("flexNodeMenu"));
  QAction* remove = menu.addAction(tr("Delete Node"));
  remove->setEnabled(m_item.nodes.size() > 2);
  QAction* detach = menu.addAction(tr("Detach"));
  detach->setEnabled(node->support != QStringLiteral("free"));
  QAction* setTwist = nullptr;
  QAction* resetTwist = nullptr;
  if (!m_item.round) {
    menu.addSeparator();
    setTwist = menu.addAction(tr("Set Twist…"));
    resetTwist = menu.addAction(tr("Reset Twist"));
    resetTwist->setEnabled(std::abs(node->twist) > 1e-9);
  }
  log(QStringLiteral("Flex node menu: %1").arg(id));
  const QAction* chosen = menu.exec(globalPosition);
  if (chosen == nullptr || !isActive() || m_item.node(id) == nullptr) {
    return;
  }
  if (chosen == setTwist) {
    editTwist(id);
    return;
  }
  if (refused()) {
    return;
  }
  if (chosen == remove) {
    deleteSelectedNode();
  } else if (chosen == detach) {
    run(detachCommand(m_uid, id), QStringLiteral("detach %1").arg(id));
  } else if (chosen == resetTwist) {
    run(setTwistCommand(m_uid, id, 0.0), QStringLiteral("twist %1").arg(id));
  }
}

void FlexEditController::editTwist(const QString& id) {
  const FlexNode* node = m_item.node(id);
  if (node == nullptr || m_item.round) {
    return;
  }
  selectNode(id);
  const std::optional<Ring> ring = ringOf(m_item);
  const QPointF at = ring ? ring->knob : onScreen(node->point);
  m_twistFieldNode = id;
  QString degrees = degreesText(node->twist);
  degrees.chop(1); // the field holds the number
  m_twistField->setText(degrees);
  m_twistField->setToolTip(tr("Type the twist in degrees."));
  m_twistField->resize(qRound(72.0 * uiTextScale()), m_twistField->sizeHint().height());
  QPoint corner(qRound(at.x()) + 12, qRound(at.y()) - m_twistField->height() / 2);
  corner.setX(std::clamp(corner.x(), 2, std::max(2, m_viewer.width() - m_twistField->width() - 2)));
  corner.setY(std::clamp(corner.y(), 2, std::max(2, m_viewer.height() - m_twistField->height() - 2)));
  m_twistField->move(corner);
  m_twistField->show();
  m_twistField->raise();
  m_twistField->setFocus(Qt::OtherFocusReason);
  m_twistField->selectAll();
  const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint(0, 0));
  log(QStringLiteral("Flex twist field %1 at %2")
          .arg(id, windowPlace(QPointF(m_twistField->geometry().center()), window)));
}

void FlexEditController::closeTwistField(bool take) {
  if (m_twistFieldNode.isEmpty()) {
    return;
  }
  const QString id = m_twistFieldNode;
  QString text = m_twistField->text().trimmed();
  m_twistFieldNode.clear(); // first: hiding the field ends its editing again
  m_twistField->hide();
  m_viewer.setFocus(Qt::OtherFocusReason);
  if (!take || !isActive() || m_item.node(id) == nullptr) {
    return;
  }
  if (text.endsWith(QChar(0x00B0))) {
    text.chop(1);
    text = text.trimmed();
  }
  bool ok = false;
  double degrees = QLocale().toDouble(text, &ok);
  if (!ok) {
    degrees = text.toDouble(&ok);
  }
  if (!ok) {
    m_host.showStatus(tr("The twist is an angle in degrees, such as 90."), NoticeLevel::Warning);
    log(QStringLiteral("Flex twist field: not a number, nothing changed"));
    return;
  }
  if (refused()) {
    return;
  }
  run(setTwistCommand(m_uid, id, degrees * kPi / 180.0), QStringLiteral("twist %1").arg(id));
}

bool FlexEditController::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer && isActive()) {
    if (event->type() == QEvent::Resize) {
      m_overlay->resize(static_cast<QResizeEvent*>(event)->size());
    } else if (event->type() == QEvent::Leave && m_edit.action == Action::None) {
      setHover(Hover());
    }
    if (mouse(event)) {
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}

bool FlexEditController::mouse(QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    const QPointF position = e->position();
    const bool left = (e->buttons() & Qt::LeftButton) != 0;
    // The view navigates with the other buttons, whatever is under way.
    if ((e->buttons() & (Qt::MiddleButton | Qt::RightButton)) != 0 && m_edit.action != Action::Menu) {
      return false;
    }
    switch (m_edit.action) {
    case Action::None:
      m_pointer = position;
      if (e->buttons() == Qt::NoButton) {
        const Hover hover = m_viewer.isCovered(position) ? Hover() : hoverAt(position);
        // A node of an item that is not round under the pointer shows its
        // twist ring, which stays while the pointer goes to its knob.
        QString ringNode = m_ringNode;
        if (hover.kind == Hover::Kind::Handle) {
          ringNode = m_item.round ? QString() : hover.node;
        } else if (hover.kind != Hover::Kind::Knob && !ringNode.isEmpty()) {
          const FlexNode* node = m_item.node(ringNode);
          if (node == nullptr || pixels(onScreen(node->point), position) > kRingPixels + kKnobPixels + 6.0) {
            ringNode.clear();
          }
        }
        if (ringNode != m_ringNode) {
          m_ringNode = ringNode;
          m_overlay->update();
          logPlaces();
        }
        setHover(hover);
      }
      return false;
    case Action::Press:
      if (pixels(position, m_edit.pressAt) > kClickPixels) {
        const QString id = m_edit.node;
        const FlexNode* node = m_item.node(id);
        const QPointF pressAt = m_edit.pressAt;
        if (node == nullptr || !begin(Action::Drag)) {
          m_edit = Edit();
          refresh();
          return true;
        }
        m_edit.node = id;
        m_edit.pressAt = pressAt;
        m_edit.moved = true;
        m_edit.anchor = node->point;
        selectNode(id); // the node pulled is the selected one
        log(QStringLiteral("Flex edit drag: started %1").arg(id));
        pointerTo(position, e->modifiers());
      }
      return true;
    case Action::Drag:
      pointerTo(position, e->modifiers());
      return true;
    case Action::Extend:
      if (left && !m_edit.moved && pixels(position, m_edit.pressAt) > kClickPixels) {
        m_edit.moved = true;
      }
      if (m_edit.moved || m_edit.armed) {
        pointerTo(position, e->modifiers());
      }
      return true;
    case Action::Insert:
      if (!m_edit.moved && pixels(position, m_edit.pressAt) > kClickPixels) {
        const Edit pressed = m_edit;
        if (!begin(Action::Insert)) {
          m_edit = Edit();
          refresh();
          return true;
        }
        m_edit.node = pressed.node;
        m_edit.pressAt = pressed.pressAt;
        m_edit.anchor = pressed.anchor;
        m_edit.along = pressed.along;
        m_edit.moved = true;
        log(QStringLiteral("Flex edit insert: dragged out after %1").arg(m_edit.node));
      }
      if (m_edit.moved) {
        pointerTo(position, e->modifiers());
      }
      return true;
    case Action::Twist:
      turnTo(position);
      return true;
    case Action::Menu:
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    const QPointF position = e->position();
    if (m_viewer.isOnOrientationCube(position) || m_viewer.isCovered(position)) {
      return false;
    }
    if (e->button() == Qt::RightButton && m_edit.action == Action::None) {
      const Hover hover = hoverAt(position);
      if (hover.kind == Hover::Kind::Handle) {
        m_edit.action = Action::Menu;
        m_edit.node = hover.node;
        return true; // its menu opens on the release
      }
      return false;
    }
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_edit.action == Action::Extend && m_edit.armed) {
      // A click on that "+" again ends adding nodes (owner 14.11.2026).
      const Hover over = hoverAt(position);
      if ((over.kind == Hover::Kind::AddStart && m_edit.atStart) ||
          (over.kind == Hover::Kind::AddEnd && !m_edit.atStart)) {
        log(QStringLiteral("Flex edit add: ended by a click on +"));
        m_swallowRelease = true;
        end(true);
        return true;
      }
      m_edit.pressAt = position;
      pointerTo(position, e->modifiers());
      return true; // placed on the release
    }
    if (m_edit.action != Action::None) {
      return true;
    }
    if ((e->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier)) != 0) {
      return false; // adds to the selection, as the view does
    }
    m_pointer = position;
    const Hover hover = hoverAt(position);
    switch (hover.kind) {
    case Hover::Kind::Knob: {
      const FlexNode* node = m_item.node(hover.node);
      const std::optional<FlexFrame> frame = node != nullptr ? frameAt(m_item, node->along) : std::nullopt;
      if (!frame || !begin(Action::Twist)) {
        return true;
      }
      m_edit.node = hover.node;
      m_edit.pressAt = position;
      m_edit.startTwist = node->twist;
      m_edit.twist = node->twist;
      m_edit.axisPoint = node->point;
      m_edit.axis = frame->tangent;
      m_edit.startNormal = frame->normal;
      m_edit.startVector = frame->normal;
      {
        const Xyz axis = unit(designDirection(m_edit.axis));
        if (const std::optional<gp_Pnt> on = m_viewer.planePoint(
                position, gp_Ax3(designPoint(m_edit.axisPoint), gp_Dir(axis[0], axis[1], axis[2])))) {
          const Xyz toward = sub(localPoint(*on), m_edit.axisPoint);
          if (length(toward) > 1e-9) {
            m_edit.startVector = toward;
          }
        }
      }
      selectNode(hover.node);
      log(QStringLiteral("Flex edit twist: started %1 at %2").arg(m_edit.node, degreesText(m_edit.startTwist)));
      refresh();
      return true;
    }
    case Hover::Kind::Handle:
      m_edit = Edit();
      m_edit.action = Action::Press;
      m_edit.node = hover.node;
      m_edit.pressAt = position;
      m_overlay->update();
      return true;
    case Hover::Kind::AddStart:
    case Hover::Kind::AddEnd: {
      const bool atStart = hover.kind == Hover::Kind::AddStart;
      if (!begin(Action::Extend)) {
        return true;
      }
      m_edit.atStart = atStart;
      m_edit.pressAt = position;
      m_edit.node = atStart ? m_item.nodes.front().id : m_item.nodes.back().id;
      m_edit.anchor = atStart ? m_item.nodes.front().point : m_item.nodes.back().point;
      log(QStringLiteral("Flex edit add: started at the %1").arg(atStart ? QStringLiteral("start")
                                                                         : QStringLiteral("end")));
      refresh();
      return true;
    }
    case Hover::Kind::Route:
      m_edit = Edit();
      m_edit.action = Action::Insert;
      m_edit.node = hover.node;
      m_edit.pressAt = position;
      m_edit.anchor = hover.place.point;
      m_edit.along = hover.place.along;
      m_overlay->update();
      return true;
    case Hover::Kind::None:
      break;
    }
    // A press elsewhere is the view's: it picks, and the node in hand is
    // let go.
    if (!m_selected.isEmpty()) {
      selectNode(QString());
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    const QPointF position = e->position();
    if (e->button() == Qt::RightButton) {
      if (m_edit.action != Action::Menu) {
        return false;
      }
      const QString node = m_edit.node;
      m_edit = Edit();
      showMenu(node, e->globalPosition().toPoint());
      return true;
    }
    if (e->button() != Qt::LeftButton) {
      return false;
    }
    if (m_swallowRelease) {
      m_swallowRelease = false;
      return true;
    }
    switch (m_edit.action) {
    case Action::None:
    case Action::Menu:
      return false;
    case Action::Press: {
      // A click on a handle selects the node.
      const QString id = m_edit.node;
      m_edit = Edit();
      selectNode(id);
      m_overlay->update();
      return true;
    }
    case Action::Drag:
      pointerTo(position, e->modifiers());
      commit();
      return true;
    case Action::Extend:
      if (m_edit.moved || m_edit.armed) {
        pointerTo(position, e->modifiers());
        if (m_edit.target) {
          commit();
        }
      } else {
        // A click on "+": the next click places the node (in a session each
        // next click, until Esc or a click on "+" again).
        m_edit.armed = true;
        m_host.showHint(m_session ? tr("Click where the next node goes. Esc, or a click on + again, ends adding nodes.")
                                  : tr("Click where the new node goes. Esc cancels."));
        log(QStringLiteral("Flex edit add: armed, the next click places the node"));
        refresh();
      }
      return true;
    case Action::Insert:
      if (m_edit.moved) {
        pointerTo(position, e->modifiers());
        commit();
      } else {
        commitInsertAtRoute();
      }
      return true;
    case Action::Twist:
      turnTo(position);
      commit();
      return true;
    }
    return false;
  }
  case QEvent::MouseButtonDblClick: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || m_edit.action != Action::None) {
      return m_edit.action != Action::None;
    }
    const Hover hover = hoverAt(e->position());
    if (hover.kind == Hover::Kind::Knob || (hover.kind == Hover::Kind::Handle && !m_item.round)) {
      m_swallowRelease = true; // the double click's own release
      editTwist(hover.node);
      return true;
    }
    if (hover.kind != Hover::Kind::None) {
      m_swallowRelease = true;
      return true;
    }
    return false;
  }
  default:
    break;
  }
  return false;
}

} // namespace mitcad::flex
