// SPDX-License-Identifier: MIT
#include "GravityController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QDebug>
#include <QEvent>
#include <QJsonObject>
#include <QLineF>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QResizeEvent>
#include <QWidget>
#include <QtLogging>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "../OcctViewer.hpp"
#include "../flex/FlexEdit.hpp"
#include "../framework/Json.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"
#include "../sketch/SketchController.hpp"

namespace mitcad::gravity {
namespace {

// The steps' frame (about 60 a second). Each step runs on the model's
// worker, one at a time: a tick while one is computed is skipped, and the
// next simulates the real time since the last began, at most one of the
// model's frames (stepSeconds).
constexpr int kTickMs = 16;
// The settling is shown as it goes until the model says the item rests;
// one that has not come to rest after this long stops where it is, as a
// safety net (Enter keeps it so, a drag moves it again).
constexpr qint64 kSettleLiveMs = 20000;
// Kept while it still moves (OK at once after a drag): what is left of the
// settling, at most this long, in one step the panel waits for.
constexpr double kFinishSeconds = 2.0;
constexpr double kMarkerRadius = 20.0;  // the marker's circle (pixels)
constexpr double kMarkerMargin = 18.0;
constexpr double kHandleRadius = 6.0; // a node's handle (pixels)

QString windowPlace(const QPointF& at, const QPoint& window) {
  return QStringLiteral("%1,%2").arg(qRound(at.x() + window.x())).arg(qRound(at.y() + window.y()));
}

// The distance from `p` to the segment from `a` to `b`, and how far along
// it (0 to 1) its nearest point is.
std::pair<double, double> segmentDistance(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF ab = b - a;
  const double l2 = QPointF::dotProduct(ab, ab);
  const double t = l2 > 1e-12 ? std::clamp(QPointF::dotProduct(p - a, ab) / l2, 0.0, 1.0) : 0.0;
  return {QLineF(p, a + ab * t).length(), t};
}

} // namespace

// What the controller draws over the view; it takes no mouse input (the
// controller filters the view's own events).
class GravityOverlay : public QWidget {
public:
  GravityOverlay(GravityController& controller, QWidget* view) : QWidget(view), m_c(controller) {
    setObjectName(QStringLiteral("gravityOverlay"));
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setAttribute(Qt::WA_NoSystemBackground);
    setFocusPolicy(Qt::NoFocus);
    QFont small = font();
    small.setPointSizeF(9.0 * uiTextScale());
    small.setBold(true);
    setFont(small);
  }

protected:
  void paintEvent(QPaintEvent* /*event*/) override {
    const GravityController::Scene scene = m_c.scene();
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    const QColor halo = m_c.viewer().sketchHaloColor();
    const bool dark = halo.lightnessF() < 0.5;
    const QColor ink = viewColor(ColorRole::ViewInk, dark);
    const QColor handle = viewColor(ColorRole::Handle, dark);
    const QColor warning = viewColor(ColorRole::WarningBorder, dark);
    const QColor fill(halo.red(), halo.green(), halo.blue(), 215);

    // The hanging item along its chain, its body faded meanwhile.
    if (scene.rope.size() >= 2) {
      flex::paintFlexRope(painter, scene.rope, scene.look, m_c.ropeView());
    }
    // Its nodes: a filled handle is pinned, a ring hangs free.
    const double r = GravityController::handleRadius();
    for (const GravityController::Scene::Handle& h : scene.handles) {
      painter.setPen(QPen(handle, 2.0));
      painter.setBrush(h.pinned ? QBrush(handle) : QBrush(fill));
      painter.drawEllipse(h.at, r, r);
      if (h.pinned) {
        painter.setPen(Qt::NoPen);
        painter.setBrush(fill);
        painter.drawEllipse(h.at, r * 0.3, r * 0.3);
      }
    }
    painter.setBrush(Qt::NoBrush);

    // The held point and the pointer.
    if (scene.grabbed && scene.pointer) {
      painter.setPen(QPen(handle, 1.6, Qt::DashLine));
      painter.drawLine(*scene.grabbed, *scene.pointer);
      painter.setPen(QPen(handle, 2.0));
      painter.setBrush(fill);
      painter.drawEllipse(*scene.grabbed, 5.0, 5.0);
      painter.setBrush(Qt::NoBrush);
      painter.drawEllipse(*scene.pointer, 3.0, 3.0);
    }

    // The direction of gravity: a circle in the corner with an arrow the
    // way it pulls on the screen, a dot when it pulls towards the viewer, a
    // cross when away; "g" beside it.
    if (scene.marker) {
      const QPointF c = scene.markerAt;
      const double mr = scene.markerRadius;
      const QColor line = scene.sourceMoved ? warning : ink;
      painter.setPen(QPen(line, 1.2));
      painter.setBrush(fill);
      painter.drawEllipse(c, mr, mr);
      painter.setPen(QPen(line, 2.2, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
      painter.setBrush(Qt::NoBrush);
      const ScreenArrow& a = *scene.marker;
      if (a.endOn) {
        if (a.away) {
          const double s = mr * 0.42;
          painter.drawLine(c + QPointF(-s, -s), c + QPointF(s, s));
          painter.drawLine(c + QPointF(-s, s), c + QPointF(s, -s));
        } else {
          painter.setBrush(line);
          painter.drawEllipse(c, mr * 0.18, mr * 0.18);
        }
      } else {
        const double size = std::hypot(a.x, a.y);
        const QPointF d(a.x / size, -a.y / size); // the screen's y runs down
        const QPointF tip = c + d * (mr * 0.72);
        const QPointF tail = c - d * (mr * 0.72);
        painter.drawLine(tail, tip);
        const QPointF across(-d.y(), d.x());
        QPainterPath head;
        head.moveTo(tip);
        head.lineTo(tip - d * (mr * 0.38) + across * (mr * 0.26));
        head.lineTo(tip - d * (mr * 0.38) - across * (mr * 0.26));
        head.closeSubpath();
        painter.setBrush(line);
        painter.drawPath(head);
      }
      painter.setPen(QPen(line, 1.0));
      painter.drawText(QRectF(c.x() + mr + 3.0, c.y() - mr, 2.0 * mr, mr), Qt::AlignLeft | Qt::AlignBottom,
                       QStringLiteral("g"));
    }
  }

private:
  GravityController& m_c;
};

GravityController::GravityController(SketchHost& host, OcctViewer& viewer, QObject* parent)
    : QObject(parent), m_host(host), m_viewer(viewer) {
  m_overlay = new GravityOverlay(*this, &viewer);
  m_overlay->resize(viewer.size());
  m_overlay->hide();
  m_timer.setInterval(kTickMs);
  connect(&m_timer, &QTimer::timeout, this, &GravityController::tick);
  viewer.installEventFilter(this);
  // The marker, the item and its handles follow the camera.
  connect(&viewer, &OcctViewer::viewChanged, m_overlay, [this] {
    if (m_overlay->isVisible()) {
      m_overlay->update();
    }
  });
  connect(&viewer, &OcctViewer::cameraRested, this, [this] {
    logMarker();
    logHandles();
  });
}

GravityController::~GravityController() = default;

double GravityController::handleRadius() { return kHandleRadius * uiTextScale(); }

void GravityController::log(const QString& text) const { qDebug().noquote() << text; }

void GravityController::status(const QString& text) { m_host.showStatus(text, NoticeLevel::Hint); }

QString GravityController::hint() const {
  return tr("Drag the hanging %1 by any point; click a node to pin it or let it go. Enter keeps its shape, Esc puts "
            "it back.")
      .arg(m_name);
}

gp_Pnt GravityController::design(const Vec3& p) const { return gp_Pnt(p[0], p[1], p[2]).Transformed(m_placement); }

Vec3 GravityController::local(const gp_Pnt& p) const {
  const gp_Pnt back = p.Transformed(m_placement.Inverted());
  return {back.X(), back.Y(), back.Z()};
}

Vec3 GravityController::down() const {
  if (!m_direction.direction) {
    return {0.0, 0.0, -1.0};
  }
  const Vec3& d = *m_direction.direction;
  gp_Vec v(d[0], d[1], d[2]);
  v.Transform(m_placement.Inverted());
  return {v.X(), v.Y(), v.Z()};
}

// ---------------------------------------------------------------------------
// The model's state

void GravityController::reload() {
  if (!takeModel()) {
    // Another job computes the model: once it is done.
    if (!m_reloadWaiting) {
      m_reloadWaiting = true;
      m_host.whenIdle(this, [this] {
        m_reloadWaiting = false;
        reload();
      });
    }
    return;
  }
  QJsonObject answer;
  try {
    answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("gravity")}});
  } catch (const std::exception&) {
    return; // a model without gravity: nothing to show
  }
  m_direction = Direction::read(answer);
  const QString logged = m_direction.direction
                             ? QStringLiteral("Gravity direction: %1 from %2%3%4")
                                   .arg(vectorText(*m_direction.direction),
                                        m_direction.source.isEmpty() ? QStringLiteral("a vector") : m_direction.source,
                                        m_direction.reversed ? QStringLiteral(", reversed") : QString(),
                                        m_direction.sourceMoved ? QStringLiteral(", its source moved since") : QString())
                             : QStringLiteral("Gravity direction: none");
  if (logged != m_loggedDirection) {
    m_loggedDirection = logged;
    log(logged);
  }
  if (isHanging() && m_direction.hanging != m_flex) {
    // The model let it go: the design changed meanwhile (an undo, an
    // edit, an agent's command), or the session ended elsewhere.
    log(QStringLiteral("Hang: %1 let go: the design changed").arg(m_flex));
    status(tr("%1 no longer hangs: the design changed.").arg(m_name));
    end();
  }
  updateOverlay();
}

void GravityController::updateOverlay() {
  // The marker only while the panel is open; the item while it hangs.
  m_overlay->setVisible((m_panelOpen && m_direction.direction.has_value()) || isHanging());
  m_overlay->raise();
  m_overlay->update();
  logMarker();
}

void GravityController::setPanelOpen(bool open) {
  if (m_panelOpen == open) {
    return;
  }
  m_panelOpen = open;
  log(open ? QStringLiteral("Hang panel opened") : QStringLiteral("Hang panel closed"));
  m_overlay->resize(m_viewer.size());
  updateOverlay();
}

QString GravityController::refused() const {
  const QString why = m_hooks.lastError ? m_hooks.lastError() : QString();
  return why.isEmpty() ? tr("The item could not be let hang.") : why;
}

// ---------------------------------------------------------------------------
// What the panel wants

QString GravityController::apply(const HangWish& wish) {
  if (!takeModel()) {
    // Another job computes the model: once it is done.
    m_host.whenIdle(this, [this, wish] { apply(wish); });
    return QString();
  }
  if (m_hooks.blocked) {
    if (const QString why = m_hooks.blocked(); !why.isEmpty()) {
      return why;
    }
  }
  bool settle = false;
  // The direction first: the item hangs toward it, and one that hangs
  // already goes on hanging and settles again the new way.
  if (wish.gravity && !sameDirection(m_direction, *wish.gravity)) {
    if (!m_host.runCommand(*wish.gravity)) {
      log(QStringLiteral("Hang panel: gravity refused: %1").arg(refused()));
      return refused();
    }
    m_host.refreshScene(); // the marker, and a slack that hangs toward gravity
    reload();
    settle = isHanging();
  }
  if (!m_direction.direction) {
    release();
    return tr("Pick a planar face, a plane or an axis that gives the way gravity pulls.");
  }
  if (isHanging() && m_flex != wish.flex) {
    release(); // one item at a time: the one before is put back
  }
  if (!isHanging() && !hang(wish.flex, wish.damping)) {
    return refused();
  }
  if (!m_hanging) {
    return QString();
  }
  if (wish.length && std::abs(*wish.length - m_hanging->length) > 1e-6) {
    log(QStringLiteral("Hang length %1 mm").arg(*wish.length, 0, 'f', 3));
    if (!send(hangLengthCommand(*wish.length))) {
      return refused();
    }
    settle = true;
  }
  if (m_hanging && std::abs(wish.damping - m_hanging->damping) > 1e-9) {
    log(QStringLiteral("Hang damping %1").arg(wish.damping, 0, 'f', 3));
    if (!send(hangDampingCommand(wish.damping))) {
      return refused();
    }
  }
  if (m_hanging) {
    const QStringList pins = m_hanging->pinsWithEnds(wish.startPinned, wish.endPinned);
    if (pins != m_hanging->pins()) {
      log(QStringLiteral("Hang ends %1 %2")
              .arg(wish.startPinned ? QStringLiteral("pinned") : QStringLiteral("free"),
                   wish.endPinned ? QStringLiteral("pinned") : QStringLiteral("free")));
      if (!send(hangPinsCommand(pins))) {
        return refused();
      }
      settle = true;
    }
  }
  if (settle) {
    settleAgain();
  }
  return QString();
}

bool GravityController::clearGravity() {
  if (isHanging()) {
    // Put back first, so that nothing hangs toward a direction that goes.
    takeModel();
    log(QStringLiteral("Hang: %1 let go: gravity cleared").arg(m_flex));
    try {
      m_host.modelCommand(hangCancelCommand());
    } catch (const std::exception& failure) {
      log(QStringLiteral("Hang cancel: %1").arg(QString::fromUtf8(failure.what())));
    }
    end();
  }
  if (!m_host.runCommand(clearCommand())) {
    m_host.refreshScene();
    return false;
  }
  log(QStringLiteral("Gravity cleared"));
  m_host.refreshScene();
  m_host.showStatus(tr("The design has no direction of gravity now."), NoticeLevel::Information);
  return true;
}

// ---------------------------------------------------------------------------
// Let hang, keep, put back

bool GravityController::hang(const QString& flex, double damping) {
  QJsonObject answer;
  if (!m_host.runCommand(hangCommand(flex, damping), &answer)) {
    log(QStringLiteral("Hang refused: %1: %2").arg(flex, refused()));
    return false;
  }
  std::optional<Hanging> hanging = Hanging::read(answer);
  if (!hanging) {
    log(QStringLiteral("Hang: the model's answer is no hanging item"));
    return false;
  }
  m_flex = flex;
  m_hanging = std::move(hanging);
  m_placement = m_hooks.placement ? m_hooks.placement(flex) : gp_Trsf();
  // Its section's size, its name and the colour the view draws its body in.
  const bool dark = m_viewer.backgroundDark();
  QColor colour = viewColor(ColorRole::Body, dark);
  for (const BodyDisplay& body : m_viewer.shownBodies()) {
    if (body.color.isValid() && QString::fromStdString(body.uid).section(QLatin1Char('.'), 0, 0) == flex) {
      colour = body.color;
      break;
    }
  }
  m_look = flex::RopeLook();
  m_look.color = colour;
  m_name = tr("item");
  try {
    const QJsonObject item =
        m_host.queryObject({{QStringLiteral("query"), QStringLiteral("flex")}, {QStringLiteral("uid"), flex}});
    const flex::FlexItem read = flex::FlexItem::read(item);
    m_look = flex::ropeLook(read, colour);
    const QString name = item.value(QStringLiteral("item")).toObject().value(QStringLiteral("name")).toString();
    m_name = name.isEmpty() ? read.name : name;
  } catch (const std::exception&) {
    // Drawn as a thin line then, its colour kept.
  }
  m_phase = Phase::Settling;
  m_gate.reset();
  m_pending.reset();
  m_times = StepTimes();
  m_dragSteps = 0;
  m_settleSteps = 0;
  m_target.reset();
  m_pointer.reset();
  m_swallowing = false;
  m_settle.start();
  log(QStringLiteral("Hang started %1 (%2): %3").arg(flex, m_name, hangText(*m_hanging, down())));
  m_overlay->resize(m_viewer.size());
  updateOverlay();
  showFaded();
  status(hint());
  if (m_hooks.changed) {
    m_hooks.changed();
  }
  startSteps();
  return true;
}

bool GravityController::lock() {
  if (!isHanging()) {
    return false;
  }
  if (!takeModel()) {
    // (The panel's OK waits for an idle model; a step of the item is
    // waited for, another job is not.)
    log(QStringLiteral("Hang lock %1: another job computes the model").arg(m_flex));
    return false;
  }
  m_timer.stop();
  if (m_phase == Phase::Dragging || m_phase == Phase::Settling || !m_hanging || !m_hanging->atRest) {
    // Kept where it rests: what is left of the settling first, at once
    // (kFinishSeconds at most: then as it is).
    m_phase = Phase::Settling;
    if (m_gate.begin()) {
      ++m_settleSteps;
      const bool done = step(hangStepCommand(kFinishSeconds, std::nullopt));
      m_gate.end();
      if (!done) {
        return false; // let go meanwhile
      }
      rested(m_hanging && m_hanging->atRest);
    }
  }
  const QString name = m_name;
  const QString uid = m_flex;
  QJsonObject answer;
  log(QStringLiteral("Hang lock %1: %2").arg(uid, m_hanging ? hangText(*m_hanging, down()) : QString()));
  if (!m_host.runCommand(hangLockCommand(), &answer)) {
    // The model's words are in the status line (runCommand shows them); it
    // still hangs where the model left it, to be moved and kept again.
    log(QStringLiteral("Hang lock refused: %1: %2").arg(uid, m_hooks.lastError ? m_hooks.lastError() : QString()));
    reload(); // still hanging, or let go
    if (isHanging()) {
      m_phase = Phase::Held;
      logHandles();
    }
    return false;
  }
  QStringList nodes;
  for (const QJsonValue& id : answer.value(QStringLiteral("nodes")).toArray()) {
    nodes << id.toString();
  }
  log(QStringLiteral("Hang locked %1: nodes %2, deviation %3 mm")
          .arg(uid, nodes.join(QLatin1Char(',')))
          .arg(answer.value(QStringLiteral("deviation")).toDouble(-1.0), 0, 'f', 3));
  end();
  m_host.refreshScene(); // the item along its new nodes
  m_host.showStatus(tr("%1 keeps the shape it hung in.").arg(name), NoticeLevel::Information);
  return true;
}

void GravityController::release() {
  if (!isHanging()) {
    return;
  }
  if (!takeModel()) {
    m_host.whenIdle(this, [this] { release(); });
    return;
  }
  const QString name = m_name;
  try {
    m_host.modelCommand(hangCancelCommand());
  } catch (const std::exception& failure) {
    log(QStringLiteral("Hang cancel: %1").arg(QString::fromUtf8(failure.what())));
  }
  log(QStringLiteral("Hang cancelled: %1 put back after %2 drag step(s), %3 settling step(s)")
          .arg(m_flex)
          .arg(m_dragSteps)
          .arg(m_settleSteps));
  end();
  if (m_hooks.showBodies) {
    m_hooks.showBodies(); // as it was: the design did not change
  }
  m_host.showStatus(tr("%1 is back as it was.").arg(name), NoticeLevel::Information);
}

void GravityController::end() {
  m_timer.stop();
  m_phase = Phase::None;
  m_flex.clear();
  m_hanging.reset();
  m_gate.reset();
  m_pending.reset(); // a step in flight: its answer is dropped
  m_target.reset();
  m_pointer.reset();
  m_swallowing = false;
  m_loggedHandles.clear();
  setHand(OcctViewer::HandCursor::None);
  updateOverlay();
  if (m_hooks.changed) {
    m_hooks.changed();
  }
}

// ---------------------------------------------------------------------------
// Showing it

void GravityController::placeShown(std::vector<BodyDisplay>& bodies) const {
  if (!isHanging()) {
    return;
  }
  for (BodyDisplay& body : bodies) {
    // A Flex item's body: F7.b0 of the item F7.
    if (QString::fromStdString(body.uid).section(QLatin1Char('.'), 0, 0) == m_flex) {
      body.style = BodyDisplay::Style::Ghost;
    }
  }
}

void GravityController::showFaded() {
  std::vector<BodyDisplay> bodies = m_viewer.shownBodies();
  placeShown(bodies);
  m_viewer.setBodies(bodies);
  m_overlay->update();
}

GravityController::Scene GravityController::scene() const {
  Scene scene;
  // The marker only while the Let It Hang panel is open.
  if (m_panelOpen && m_direction.direction && m_viewer.isVisible()) {
    const gp_Dir view = m_viewer.viewDirection();
    const gp_Dir up = m_viewer.viewUp();
    scene.marker = screenArrow(*m_direction.direction, Vec3{view.X(), view.Y(), view.Z()}, Vec3{up.X(), up.Y(), up.Z()});
    const QRectF area = m_viewer.uncoveredArea().intersected(QRectF(m_overlay->rect()));
    const QRectF free = area.isEmpty() ? QRectF(m_overlay->rect()) : area;
    scene.markerRadius = kMarkerRadius * uiTextScale();
    // The lower left corner, above the view's own corner items.
    scene.markerAt = QPointF(free.left() + kMarkerMargin + scene.markerRadius,
                             free.bottom() - kMarkerMargin - 3.0 * scene.markerRadius);
    scene.sourceMoved = m_direction.sourceMoved;
  }
  if (!m_hanging) {
    return scene;
  }
  flex::FlexItem item;
  for (const Hanging::Frame& frame : m_hanging->frames) {
    flex::FlexFrame f;
    f.along = frame.along;
    f.point = frame.point;
    f.tangent = frame.tangent;
    f.normal = frame.normal;
    item.frames.push_back(f);
  }
  scene.rope = flex::ropeSamples(item, m_placement);
  scene.look = m_look;
  if (m_hanging->taut) {
    // Pulled tight between its pins: in the warning's colour.
    scene.look.color = viewColor(ColorRole::WarningBorder, m_viewer.backgroundDark());
  }
  if (m_viewer.isVisible()) {
    for (const HangNode& node : m_hanging->nodes) {
      scene.handles.push_back({m_viewer.toWidgetF(design(node.point)), node.pinned});
    }
  }
  if (m_phase == Phase::Dragging && m_pointer && m_hanging->grabbed) {
    const int held = m_hanging->nearest(m_grab);
    if (held >= 0) {
      scene.grabbed = m_viewer.toWidgetF(design(m_hanging->frames[static_cast<std::size_t>(held)].point));
      scene.pointer = m_pointer;
    }
  }
  return scene;
}

flex::RopeView GravityController::ropeView() const {
  flex::RopeView view;
  view.project = [this](const gp_Pnt& point) { return m_viewer.toWidgetF(point); };
  view.pixelSize = [this](const gp_Pnt& point) { return m_host.pixelSize(point); };
  view.towardViewer = m_viewer.viewDirection().Reversed();
  return view;
}

void GravityController::logHandles() {
  if (!m_hanging || !m_viewer.isVisible()) {
    return;
  }
  // Where the handles and the chain's middle are on the screen, for UI
  // tests (window pixels).
  const QPoint window = m_viewer.mapTo(m_viewer.window(), QPoint());
  QStringList lines;
  for (const HangNode& node : m_hanging->nodes) {
    lines << QStringLiteral("Hang node %1 %2 %3 at %4")
                 .arg(m_flex, node.id, node.pinned ? QStringLiteral("pinned") : QStringLiteral("free"),
                      windowPlace(m_viewer.toWidgetF(design(node.point)), window));
  }
  const Hanging::Frame& middle = m_hanging->frames[m_hanging->frames.size() / 2];
  lines << QStringLiteral("Hang middle of %1 at %2").arg(m_flex, windowPlace(m_viewer.toWidgetF(design(middle.point)), window));
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedHandles) {
    m_loggedHandles = logged;
    for (const QString& line : std::as_const(lines)) {
      log(line);
    }
  }
}

void GravityController::logMarker() {
  const Scene shown = scene();
  QString logged;
  if (shown.marker) {
    logged = QStringLiteral("Gravity marker: %1 at %2")
                 .arg(shown.marker->endOn
                          ? (shown.marker->away ? QStringLiteral("away from the viewer") : QStringLiteral("towards the viewer"))
                          : QStringLiteral("arrow %1,%2")
                                .arg(shown.marker->x, 0, 'f', 2)
                                .arg(shown.marker->y, 0, 'f', 2),
                      windowPlace(shown.markerAt, m_viewer.mapTo(m_viewer.window(), QPoint())));
  } else {
    logged = QStringLiteral("Gravity marker: none");
  }
  if (logged != m_loggedMarker) {
    m_loggedMarker = logged;
    log(logged);
  }
}

// ---------------------------------------------------------------------------
// The steps

void GravityController::startSteps() {
  m_frame.invalidate();
  if (!m_timer.isActive()) {
    m_timer.start();
  }
}

void GravityController::tick() {
  if (m_phase != Phase::Dragging && m_phase != Phase::Settling) {
    m_timer.stop();
    return;
  }
  // Another job, or calls waiting for the model's idle point (a change of
  // the panel, a command): they first, the next tick then.
  if (m_host.modelBusy() || m_host.idleCallsWaiting()) {
    return;
  }
  if (m_phase == Phase::Settling && m_settle.isValid() && m_settle.elapsed() > kSettleLiveMs) {
    rested(false); // not at rest after so long: it stops where it is
    return;
  }
  if (!m_gate.begin()) {
    return; // one in flight: this tick is skipped, nothing is queued
  }
  const double seconds = stepSeconds(m_frame.isValid() ? static_cast<double>(m_frame.nsecsElapsed()) / 1.0e9 : 1.0 / 60.0);
  m_frame.restart();
  QJsonObject command;
  if (m_phase == Phase::Dragging) {
    command = hangStepCommand(seconds, std::make_pair(m_grab, m_target.value_or(m_grab)));
    ++m_dragSteps;
  } else {
    command = hangStepCommand(seconds, std::nullopt);
    ++m_settleSteps;
  }
  startStep(command);
}

void GravityController::startStep(const QJsonObject& command) {
  const quint64 serial = ++m_serial;
  const Phase phase = m_phase;
  m_pending = serial;
  m_stepClock.start();
  // On the model's worker, the UI going on meanwhile: the answer comes to
  // stepDone.
  if (m_host.modelRuntimeCommandAsync(command, this, [this, serial, phase](const CommandHost::AsyncResult& result) {
        stepDone(serial, phase, result.answer, result.error, result.cancelled);
      })) {
    return;
  }
  // Not started (a read-only window refuses it, a host that computes in
  // place): as a command, at once.
  QJsonObject answer;
  QString error;
  try {
    answer = m_host.modelCommand(command);
  } catch (const std::exception& failure) {
    error = errorText(failure);
  }
  stepDone(serial, phase, answer, error, false);
}

void GravityController::stepDone(quint64 serial, Phase phase, const QJsonObject& answer, const QString& error,
                                 bool cancelled) {
  if (m_pending != serial) {
    return; // another command's answer came since (or it no longer hangs)
  }
  m_pending.reset();
  m_gate.end();
  m_times.add(static_cast<double>(m_stepClock.nsecsElapsed()) / 1.0e6);
  if (cancelled) {
    return; // the next tick
  }
  if (!error.isEmpty()) {
    refusedStep(error);
    return;
  }
  if (!take(answer)) {
    return;
  }
  if (phase == Phase::Settling && m_phase == Phase::Settling && m_hanging && m_hanging->atRest) {
    rested(true);
  }
}

void GravityController::supersede() {
  if (m_pending) {
    m_pending.reset(); // its answer, when it comes, is dropped
    m_gate.end();
  }
}

bool GravityController::takeModel() {
  supersede();
  return !m_host.modelBusy() || m_host.yieldModelJob();
}

bool GravityController::send(const QJsonObject& command) {
  supersede();
  if (!m_gate.begin()) {
    return false;
  }
  const bool done = step(command);
  m_gate.end();
  return done;
}

void GravityController::settleAgain() {
  if (!isHanging() || m_phase == Phase::Dragging) {
    return;
  }
  m_phase = Phase::Settling;
  m_settle.start();
  startSteps();
}

bool GravityController::step(const QJsonObject& command) {
  supersede(); // a step in flight: this command's answer comes after it
  QJsonObject answer;
  try {
    answer = m_host.modelCommand(command);
  } catch (const std::exception& failure) {
    refusedStep(errorText(failure));
    return false;
  }
  return take(answer);
}

void GravityController::refusedStep(const QString& why) {
  log(QStringLiteral("Hang step refused: %1").arg(why));
  m_timer.stop();
  m_gate.end();
  // Let go by the model (the design changed), or held as it was.
  reload();
  if (isHanging()) {
    m_phase = Phase::Held;
    m_host.showStatus(why, NoticeLevel::Error);
  }
}

bool GravityController::take(const QJsonObject& answer) {
  std::optional<Hanging> hanging = Hanging::read(answer);
  if (!hanging) {
    return false;
  }
  const bool wasTaut = m_hanging && m_hanging->taut;
  m_hanging = std::move(hanging);
  if (m_hanging->taut && !wasTaut) {
    log(QStringLiteral("Hang %1 pulled tight between its pins").arg(m_flex));
    status(tr("%1 is pulled tight between its pins: make it longer, or let a pin go.").arg(m_name));
  }
  m_overlay->update();
  return true;
}

void GravityController::rested(bool atRest) {
  m_timer.stop();
  m_phase = Phase::Held;
  log(QStringLiteral("Hang rest %1: %2; %3 drag step(s), %4 settling step(s), %5 skipped, %6")
          .arg(m_flex, m_hanging ? hangText(*m_hanging, down()) : QString())
          .arg(m_dragSteps)
          .arg(m_settleSteps)
          .arg(m_gate.skipped())
          .arg(m_times.text()));
  if (m_hanging && m_hanging->taut) {
    status(tr("%1 is pulled tight between its pins: make it longer, or let a pin go.").arg(m_name));
  } else {
    status(atRest ? tr("%1 rests. Enter keeps this shape; drag it again, click a node to pin it or let it go, or Esc "
                       "puts it back.")
                        .arg(m_name)
                  : tr("%1 has not come to rest. Enter keeps it as it is now; drag it again, or Esc puts it back.")
                        .arg(m_name));
  }
  logHandles();
}

void GravityController::togglePin(const QString& node) {
  if (!m_hanging) {
    return;
  }
  const HangNode* found = m_hanging->node(node);
  const bool pinning = found != nullptr && !found->pinned;
  log(QStringLiteral("Hang pin %1 %2").arg(node, pinning ? QStringLiteral("pinned") : QStringLiteral("let go")));
  const bool atEnd = !m_hanging->nodes.empty() &&
                     (m_hanging->nodes.front().id == node || m_hanging->nodes.back().id == node);
  if (!send(hangPinsCommand(m_hanging->toggled(node)))) {
    return;
  }
  status(pinning ? tr("The node is pinned: it stays where it is.") : tr("The node is let go: it hangs free."));
  // An end: the panel's Start pinned or End pinned follows.
  if (atEnd && m_hanging && m_hooks.endsChanged) {
    m_hooks.endsChanged(m_hanging->startPinned(), m_hanging->endPinned());
  }
  settleAgain();
}

// ---------------------------------------------------------------------------
// The mouse

QString GravityController::nodeAt(const QPointF& position) const {
  if (!m_hanging || !m_viewer.isVisible()) {
    return QString();
  }
  QString best;
  double nearest = 2.0 * handleRadius();
  for (const HangNode& node : m_hanging->nodes) {
    const double d = QLineF(position, m_viewer.toWidgetF(design(node.point))).length();
    if (d <= nearest) {
      nearest = d;
      best = node.id;
    }
  }
  return best;
}

int GravityController::chainAt(const QPointF& position) const {
  if (!m_hanging || !m_viewer.isVisible()) {
    return -1;
  }
  const flex::RopeView view = ropeView();
  int best = -1;
  double nearest = 1e300;
  const std::size_t count = m_hanging->frames.size();
  for (std::size_t i = 0; i + 1 < count; ++i) {
    const gp_Pnt a = design(m_hanging->frames[i].point);
    const gp_Pnt b = design(m_hanging->frames[i + 1].point);
    const auto [d, t] = segmentDistance(position, m_viewer.toWidgetF(a), m_viewer.toWidgetF(b));
    flex::RopeSample sample;
    sample.point = a;
    const double reach = flex::ropeHalfPixels(sample, m_look, view) + 4.0;
    if (d <= reach && d < nearest) {
      nearest = d;
      best = static_cast<int>(t < 0.5 ? i : i + 1);
    }
  }
  return best;
}

void GravityController::setHand(OcctViewer::HandCursor hand) {
  if (hand == m_viewer.handCursor()) {
    return;
  }
  m_viewer.setHandCursor(hand);
}

bool GravityController::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer) {
    if (event->type() == QEvent::Resize) {
      m_overlay->resize(static_cast<QResizeEvent*>(event)->size());
      logMarker();
    }
    if (isHanging() && mouse(event)) {
      return true;
    }
  }
  return QObject::eventFilter(watched, event);
}

bool GravityController::mouse(QEvent* event) {
  const auto plain = [](const QMouseEvent* e) {
    return (e->modifiers() & (Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)) == 0;
  };
  switch (event->type()) {
  case QEvent::MouseButtonPress: {
    auto* e = static_cast<QMouseEvent*>(event);
    const QPointF position = e->position();
    PressInput press;
    press.dragging = m_phase == Phase::Dragging;
    press.left = e->button() == Qt::LeftButton && plain(e);
    press.viewArea = !m_viewer.isOnOrientationCube(position) && !m_viewer.isCovered(position);
    QString node;
    int chain = -1;
    if (!press.dragging && press.left && press.viewArea) {
      node = nodeAt(position);
      press.onNode = !node.isEmpty();
      chain = press.onNode ? -1 : chainAt(position);
      press.onChain = chain >= 0;
    }
    const PressRoute route = routePress(press);
    if (route == PressRoute::Swallow) {
      return true; // another button meanwhile: the drag goes on
    }
    if (route == PressRoute::View) {
      // Navigation, the panel's pick beside the item (its Direction From),
      // the cube, a panel.
      if (press.left && press.viewArea && !press.dragging) {
        log(QStringLiteral("Hang press beside %1: the view's").arg(m_flex));
      }
      return false;
    }
    // The left button is the tool's from here until it goes up.
    m_swallowing = true;
    if (m_hooks.blocked) {
      if (const QString why = m_hooks.blocked(); !why.isEmpty()) {
        m_host.showStatus(why, NoticeLevel::Error);
        return true;
      }
    }
    if (route == PressRoute::TogglePin) {
      togglePin(node);
      return true;
    }
    m_swallowing = false; // the drag's own until the release
    m_grab = m_hanging->frames[static_cast<std::size_t>(chain)].point;
    m_through = design(m_grab);
    m_target = m_grab;
    m_pointer = position;
    m_phase = Phase::Dragging;
    setHand(OcctViewer::HandCursor::Closed);
    log(QStringLiteral("Hang drag of %1 started at %2").arg(m_flex, vectorText(m_grab, 1)));
    startSteps();
    return true;
  }
  case QEvent::MouseMove: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_phase != Phase::Dragging) {
      if (m_swallowing) {
        return true; // the left button pressed on a handle: no orbit
      }
      const bool over = !nodeAt(e->position()).isEmpty() || chainAt(e->position()) >= 0;
      setHand(over ? OcctViewer::HandCursor::Open : OcctViewer::HandCursor::None);
      return false;
    }
    m_pointer = e->position();
    if (const std::optional<gp_Pnt> on = m_viewer.viewPlanePoint(e->position(), m_through)) {
      m_target = local(*on);
    }
    m_overlay->update();
    return true;
  }
  case QEvent::MouseButtonRelease: {
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_phase != Phase::Dragging) {
      if (m_swallowing && e->button() == Qt::LeftButton) {
        m_swallowing = false;
        return true;
      }
      return false;
    }
    if (e->button() != Qt::LeftButton) {
      return true;
    }
    m_phase = Phase::Settling;
    m_pointer.reset();
    m_target.reset();
    m_settle.start();
    setHand(OcctViewer::HandCursor::Open);
    log(QStringLiteral("Hang drag of %1 released after %2 step(s): it settles").arg(m_flex).arg(m_dragSteps));
    m_overlay->update();
    startSteps();
    return true;
  }
  case QEvent::MouseButtonDblClick: {
    // The second click of a double click on the item or a handle is the
    // tool's too (it would pick, or open the item's edit session).
    auto* e = static_cast<QMouseEvent*>(event);
    if (m_phase == Phase::Dragging) {
      return true;
    }
    if (e->button() == Qt::LeftButton && plain(e) && !m_viewer.isOnOrientationCube(e->position()) &&
        !m_viewer.isCovered(e->position()) && (!nodeAt(e->position()).isEmpty() || chainAt(e->position()) >= 0)) {
      m_swallowing = true;
      return true;
    }
    return false;
  }
  default:
    break;
  }
  return false;
}

} // namespace mitcad::gravity
