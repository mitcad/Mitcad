// SPDX-License-Identifier: MIT
// The guide step shown in a design window (mitcad#2012): see TutorialGuide.hpp.
#include "TutorialGuide.hpp"

#include <algorithm>
#include <utility>

#include <QApplication>
#include <QDialog>
#include <QEvent>
#include <QFrame>
#include <QKeyEvent>
#include <QLabel>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QPaintEvent>
#include <QPushButton>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <QtLogging>

#include "../framework/Theme.hpp"
#include "GuideBindings.hpp"

namespace mitcad {

// The outline around the target: over the whole window, transparent for
// the mouse.
class GuideHighlight : public QWidget {
public:
  explicit GuideHighlight(QWidget* window) : QWidget(window) {
    setObjectName(QStringLiteral("guideHighlight"));
    setAttribute(Qt::WA_TransparentForMouseEvents, true);
    setAttribute(Qt::WA_NoSystemBackground, true);
    setFocusPolicy(Qt::NoFocus);
  }
  void setTarget(const QRect& target) {
    if (target != m_target) {
      m_target = target;
      update();
    }
  }

protected:
  void paintEvent(QPaintEvent*) override {
    if (m_target.isNull()) {
      return;
    }
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);
    QPen pen(themeColor(ColorRole::Action, palette()), 3);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.drawRoundedRect(QRectF(m_target.adjusted(-4, -4, 4, 4)), 6, 6);
  }

private:
  QRect m_target;
};

// The demonstration's layer (mitcad#2366): over the whole window,
// transparent for the mouse, the page's picture of the frame shown where
// the frame is.
class GuideDemonstrationLayer : public QWidget {
public:
  explicit GuideDemonstrationLayer(QWidget* window) : QWidget(window) {
    setObjectName(QStringLiteral("guideDemonstration"));
    setAttribute(Qt::WA_TransparentForMouseEvents, true);
    setAttribute(Qt::WA_NoSystemBackground, true);
    setFocusPolicy(Qt::NoFocus);
  }
  void setFrame(const QImage& image, const QRectF& rect) {
    const QRect before = m_rect.toAlignedRect();
    m_image = image;
    m_rect = rect;
    update(before.united(rect.toAlignedRect()).adjusted(-1, -1, 1, 1));
  }
  void clear() {
    if (!m_image.isNull()) {
      update(m_rect.toAlignedRect().adjusted(-1, -1, 1, 1));
    }
    m_image = QImage();
    m_rect = QRectF();
  }

protected:
  void paintEvent(QPaintEvent*) override {
    if (m_image.isNull()) {
      return;
    }
    QPainter painter(this);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    painter.drawImage(m_rect, m_image);
  }

private:
  QImage m_image;
  QRectF m_rect;
};

namespace {

constexpr int kTickMs = 150;
// A typed value is taken once it stayed so this long.
constexpr qint64 kValueSettleMs = 600;
constexpr int kNoteMs = 3000;
constexpr int kCalloutWidth = 280;
constexpr int kGap = 12;
// A demonstration's frames are placed at about 60 a second.
constexpr int kDemonstrationMs = 16;
// A wheel held back is said to the page at most once in this time.
constexpr qint64 kWheelHeldMs = 1000;

// The page's name of a place holdsBack names (docs/tutorials.md "Events").
QString heldPlace(const QString& place) {
  if (place == QLatin1String("the view")) {
    return QStringLiteral("view");
  }
  if (place == QLatin1String("browserDock")) {
    return QStringLiteral("browser");
  }
  if (place == QLatin1String("timelineDock")) {
    return QStringLiteral("timeline");
  }
  if (place == QLatin1String("projectsDock")) {
    return QStringLiteral("projects");
  }
  if (place == QLatin1String("configurationDock")) {
    return QStringLiteral("configuration");
  }
  return place;
}

QString buttonName(Qt::MouseButton button) {
  switch (button) {
  case Qt::LeftButton: return QStringLiteral("left");
  case Qt::MiddleButton: return QStringLiteral("middle");
  case Qt::RightButton: return QStringLiteral("right");
  default: return QStringLiteral("other");
  }
}

// Whether `widget` is `ancestor` or inside it, across windows (a card of
// native glass is a window of its own, a child of the design's).
bool descends(const QWidget* widget, const QWidget* ancestor) {
  for (const QWidget* w = widget; w != nullptr; w = w->parentWidget()) {
    if (w == ancestor) {
      return true;
    }
  }
  return false;
}

QString centerText(const QWidget* widget, const QWidget* window) {
  const QPoint center = widget->mapTo(window, widget->rect().center());
  return QStringLiteral("%1,%2").arg(center.x()).arg(center.y());
}

QString targetText(const GuideStep& step) {
  switch (step.target) {
  case GuideStep::Target::Command: return QStringLiteral("command %1").arg(step.targetName);
  case GuideStep::Target::Input: return QStringLiteral("input %1").arg(step.targetName);
  case GuideStep::Target::Browser: return QStringLiteral("Browser row %1").arg(step.targetName);
  case GuideStep::Target::View:
    return step.pickKinds.isEmpty() ? QStringLiteral("the view")
                                    : QStringLiteral("the view (%1)").arg(step.pickKinds.join(QStringLiteral(", ")));
  case GuideStep::Target::WorkspaceSelector: return QStringLiteral("the workspace selector");
  case GuideStep::Target::Workspace: return QStringLiteral("workspace %1").arg(step.targetName);
  case GuideStep::Target::Tab: return QStringLiteral("tab %1").arg(step.targetName);
  case GuideStep::Target::Timeline: return QStringLiteral("the timeline");
  }
  return QString();
}

// "orbited", "panned", "zoomed": a navigation (guideNavigationActions()) done.
QString navigatedText(const QString& action) {
  if (action == QLatin1String("pan")) {
    return QStringLiteral("panned");
  }
  return action + QStringLiteral("ed");
}

} // namespace

TutorialGuide::TutorialGuide(GuideHost host, QObject* parent) : QObject(parent), m_host(std::move(host)) {
  m_tick.setInterval(kTickMs);
  connect(&m_tick, &QTimer::timeout, this, &TutorialGuide::tick);
  m_noteTimer.setSingleShot(true);
  connect(&m_noteTimer, &QTimer::timeout, this, [this] {
    if (m_note != nullptr) {
      m_note->hide();
      if (m_callout) {
        m_callout->adjustSize();
        placeCallout(m_target);
      }
    }
  });
  m_demonstrationTimer.setInterval(kDemonstrationMs);
  connect(&m_demonstrationTimer, &QTimer::timeout, this, &TutorialGuide::demonstrationTick);
}

TutorialGuide::~TutorialGuide() {
  qApp->removeEventFilter(this);
  delete m_highlight;
  delete m_demonstrationLayer;
  delete m_callout;
}

void TutorialGuide::start(const GuideStep& step) {
  if (m_active) {
    end(QStringLiteral("replaced"));
  }
  releaseHold(QStringLiteral("next step"));
  m_step = step;
  m_active = true;
  ++m_sequence;
  QWidget* window = m_host.window;
  if (!m_highlight) {
    m_highlight = new GuideHighlight(window);
  }
  if (!m_callout) {
    auto* callout = new QFrame(window);
    callout->setObjectName(QStringLiteral("guideCallout"));
    callout->setFrameShape(QFrame::Box);
    callout->setLineWidth(2);
    callout->setAutoFillBackground(true);
    QPalette palette = callout->palette();
    palette.setColor(QPalette::Window, themeColor(ColorRole::Raised, palette));
    palette.setColor(QPalette::WindowText, themeColor(ColorRole::Action, palette));
    callout->setPalette(palette);
    auto* layout = new QVBoxLayout(callout);
    layout->setContentsMargins(12, 10, 12, 10);
    layout->setSpacing(6);
    m_text = new QLabel(callout);
    m_text->setObjectName(QStringLiteral("guideCalloutText"));
    m_text->setWordWrap(true);
    m_text->setTextFormat(Qt::PlainText);
    QPalette textPalette = m_text->palette();
    textPalette.setColor(QPalette::WindowText, themeColor(ColorRole::Text, textPalette));
    m_text->setPalette(textPalette);
    layout->addWidget(m_text);
    m_note = new QLabel(callout);
    m_note->setObjectName(QStringLiteral("guideCalloutNote"));
    m_note->setWordWrap(true);
    m_note->setTextFormat(Qt::PlainText);
    QPalette notePalette = m_note->palette();
    notePalette.setColor(QPalette::WindowText, themeColor(ColorRole::WarningText, notePalette));
    m_note->setPalette(notePalette);
    m_note->hide();
    layout->addWidget(m_note);
    m_leaveButton = new QPushButton(tr("Leave Tutorial"), callout);
    m_leaveButton->setObjectName(QStringLiteral("guideLeave"));
    m_leaveButton->setFocusPolicy(Qt::NoFocus);
    layout->addWidget(m_leaveButton, 0, Qt::AlignRight);
    connect(m_leaveButton, &QPushButton::clicked, this, [this] {
      if (m_host.leave) {
        m_host.leave();
      } else {
        end(QStringLiteral("leave"));
      }
    });
    callout->setFixedWidth(kCalloutWidth);
    m_callout = callout;
  }
  m_text->setText(step.text);
  m_note->hide();
  m_callout->adjustSize();
  m_target = QRect();
  m_targetLogged = false;
  m_calloutLogged.clear();
  m_valueMatched.invalidate();
  m_typedLogged.clear();
  m_pickedAtStart = m_host.pickedKinds ? m_host.pickedKinds() : QStringList();
  m_heldWidget = nullptr;
  m_lastHeldBack.clear();
  m_navigationsLogged.clear();
  m_highlight->setGeometry(window->rect());
  m_highlight->setTarget(QRect());
  m_highlight->show();
  m_highlight->raise();
  m_callout->show();
  m_callout->raise();
  placeCallout(QRect());
  emit calloutMoved();
  updateEventFilter();
  m_tick.start();
  qInfo().noquote() << QStringLiteral("Guide: step %1 started: target %2, %3")
                           .arg(step.id, targetText(step),
                                step.navigation ? QStringLiteral("navigation free") : QStringLiteral("navigation held"));
  emit changed();
  tick();
}

void TutorialGuide::end(const QString& reason) {
  if (m_holding) {
    releaseHold(reason);
    return;
  }
  if (!m_active) {
    return;
  }
  stopDemonstration(QStringLiteral("step"));
  m_active = false;
  m_tick.stop();
  updateEventFilter();
  if (m_highlight) {
    m_highlight->hide();
  }
  if (m_callout) {
    m_callout->hide();
    emit calloutMoved();
  }
  qInfo().noquote() << QStringLiteral("Guide: step %1 left (%2)").arg(m_step.id, reason);
  emitEvent({{QStringLiteral("event"), QStringLiteral("step_left")}, {QStringLiteral("reason"), reason}});
}

void TutorialGuide::releaseHold(const QString& reason) {
  if (!m_holding) {
    return;
  }
  m_holding = false;
  m_heldWidget = nullptr;
  updateEventFilter();
  qInfo().noquote() << QStringLiteral("Guide: hold after step %1 released (%2)").arg(m_step.id, reason);
  emit changed();
}

void TutorialGuide::refuseCommand(const QString& id, const QString& name) {
  if (m_holding) {
    // The callout is hidden during the hold: the log and the page's event.
    qInfo().noquote() << QStringLiteral("Guide: command %1 refused during the hold after step %2").arg(id, m_step.id);
    emitEvent({{QStringLiteral("event"), QStringLiteral("refused")}, {QStringLiteral("command"), id}});
    return;
  }
  qInfo().noquote() << QStringLiteral("Guide: command %1 refused during step %2").arg(id, m_step.id);
  showNote(tr("%1 is not part of this step.").arg(name));
  emitEvent({{QStringLiteral("event"), QStringLiteral("refused")}, {QStringLiteral("command"), id}});
}

void TutorialGuide::commandFinished(const QString& id, bool committed) {
  if (!m_active || m_step.done != GuideStep::Done::Finished || id != m_step.doneName || !committed) {
    return;
  }
  finish({{QStringLiteral("command"), id}, {QStringLiteral("committed"), true}},
         QStringLiteral("command %1 finished").arg(id));
}

void TutorialGuide::viewNavigated(const QString& action) {
  if (!m_active || m_step.done != GuideStep::Done::Navigate) {
    return;
  }
  if (action != m_step.doneName) {
    // Another navigation (a zoom during an orbit step): the step stays.
    if (!m_navigationsLogged.contains(action)) {
      m_navigationsLogged << action;
      qInfo().noquote() << QStringLiteral("Guide: step %1 view %2, not its %3")
                               .arg(m_step.id, navigatedText(action), m_step.doneName);
    }
    return;
  }
  finish({{QStringLiteral("navigate"), action}}, QStringLiteral("the view %1").arg(navigatedText(action)));
}

void TutorialGuide::holdView(bool held, const QString& why) {
  if (held == m_viewHeld) {
    return;
  }
  m_viewHeld = held;
  if (!held && !m_active && !m_holding) {
    m_heldWidget = nullptr;
  }
  updateEventFilter();
  qInfo().noquote() << (held ? QStringLiteral("Guide: view held (%1)").arg(why) : QStringLiteral("Guide: view free"));
}

void TutorialGuide::updateEventFilter() {
  const bool wanted = m_active || m_holding || m_viewHeld;
  if (wanted == m_filtering) {
    return;
  }
  m_filtering = wanted;
  if (wanted) {
    qApp->installEventFilter(this);
  } else {
    qApp->removeEventFilter(this);
  }
}

bool TutorialGuide::holdsViewInput(QWidget* widget, QEvent* event) {
  switch (event->type()) {
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick: {
    // The view's picks, as a step that takes none holds them; navigation
    // (the middle and right buttons, the wheel) stays free.
    // A panel open, a plane being picked, a sketch: their clicks are free,
    // as during a step.
    const auto* mouse = static_cast<QMouseEvent*>(event);
    const bool commandOpen = (m_host.openCommand && !m_host.openCommand().isEmpty()) ||
                             (m_host.idle && !m_host.idle());
    if (m_host.viewer == nullptr || !descends(widget, m_host.viewer) || mouse->button() != Qt::LeftButton ||
        commandOpen) {
      return false;
    }
    m_heldWidget = widget;
    if (event->type() == QEvent::MouseButtonPress) {
      qInfo().noquote() << QStringLiteral("Guide: input held back on the view while the tutorial pane is centred");
    }
    return true;
  }
  case QEvent::MouseButtonRelease:
  case QEvent::MouseMove:
    if (m_heldWidget && widget == m_heldWidget) {
      if (event->type() == QEvent::MouseButtonRelease) {
        m_heldWidget = nullptr;
      }
      return true;
    }
    return false;
  default: return false;
  }
}

QJsonObject TutorialGuide::state() const {
  QJsonObject state{{QStringLiteral("active"), m_active},
                    {QStringLiteral("holding"), m_holding},
                    {QStringLiteral("sequence"), m_sequence}};
  if (m_sequence > 0) {
    state.insert(QStringLiteral("step"), m_step.toJson());
  }
  if (!m_lastEvent.isEmpty()) {
    state.insert(QStringLiteral("last_event"), m_lastEvent);
  }
  if (m_active) {
    state.insert(QStringLiteral("target_shown"), !m_target.isNull());
    if (!m_target.isNull()) {
      // In the design window's logical pixels (mitcad#2366): its size for
      // a page that sizes its demonstration's pictures.
      state.insert(QStringLiteral("target"), QJsonObject{{QStringLiteral("x"), m_target.x()},
                                                         {QStringLiteral("y"), m_target.y()},
                                                         {QStringLiteral("width"), m_target.width()},
                                                         {QStringLiteral("height"), m_target.height()}});
    }
    state.insert(QStringLiteral("demonstrating"), m_demonstrating);
  }
  return state;
}

bool TutorialGuide::demonstrate(const GuideDemonstration& demonstration, QString* error) {
  if (!m_active) {
    *error = QStringLiteral("No guide step is shown: a demonstration plays over a step's target");
    return false;
  }
  if (demonstration.frames.isEmpty()) {
    *error = QStringLiteral("The demonstration has no frames");
    return false;
  }
  if (m_demonstrating) {
    stopDemonstration(QStringLiteral("replaced"));
  }
  QWidget* window = m_host.window;
  if (!m_demonstrationLayer) {
    m_demonstrationLayer = new GuideDemonstrationLayer(window);
  }
  m_demonstration = demonstration;
  m_demonstrating = true;
  m_demonstrationFrame = -1;
  m_demonstrationHidden = false;
  m_demonstrationLayer->setAccessibleName(demonstration.label);
  m_demonstrationLayer->clear();
  m_demonstrationLayer->setGeometry(window->rect());
  m_demonstrationLayer->show();
  m_demonstrationLayer->raise();
  if (m_callout) {
    m_callout->raise();
  }
  const QString plays = demonstration.repeat == 0   ? QStringLiteral("until stopped")
                        : demonstration.repeat == 1 ? QStringLiteral("played once")
                                                    : QStringLiteral("played %1 times").arg(demonstration.repeat);
  qInfo().noquote() << QStringLiteral("Guide: demonstration on step %1 started: %2 frames, %3%4")
                           .arg(m_step.id)
                           .arg(demonstration.frames.size())
                           .arg(plays,
                                demonstration.label.isEmpty() ? QString()
                                                              : QStringLiteral(", \"%1\"").arg(demonstration.label));
  m_demonstrationClock.start();
  m_demonstrationTimer.start();
  emit changed();
  demonstrationTick();
  return true;
}

void TutorialGuide::stopDemonstration(const QString& reason) {
  if (!m_demonstrating) {
    return;
  }
  m_demonstrating = false;
  m_demonstrationTimer.stop();
  if (m_demonstrationLayer) {
    m_demonstrationLayer->clear();
    m_demonstrationLayer->hide();
  }
  m_demonstration = GuideDemonstration();
  qInfo().noquote() << QStringLiteral("Guide: demonstration on step %1 ended (%2)").arg(m_step.id, reason);
  emitEvent({{QStringLiteral("event"), QStringLiteral("demonstration_ended")}, {QStringLiteral("reason"), reason}});
}

QString TutorialGuide::topText(const QPointF& point) const {
  QWidget* window = m_host.window;
  const QPoint at = window->window()->mapFromGlobal(window->mapToGlobal(point.toPoint()));
  return QStringLiteral("%1,%2").arg(at.x()).arg(at.y());
}

void TutorialGuide::demonstrationTick() {
  if (!m_demonstrating || !m_demonstrationLayer) {
    return;
  }
  if (!m_active) {
    stopDemonstration(QStringLiteral("step"));
    return;
  }
  QWidget* window = m_host.window;
  if (m_demonstrationLayer->geometry() != window->rect()) {
    m_demonstrationLayer->setGeometry(window->rect());
  }
  // The target as it is now: it follows the window's size and its panes.
  const QRect target = m_host.targetRect ? m_host.targetRect(m_step) : QRect();
  if (target.isNull()) {
    if (!m_demonstrationHidden) {
      m_demonstrationHidden = true;
      m_demonstrationFrame = -1;
      m_demonstrationLayer->clear();
      qInfo().noquote() << QStringLiteral("Guide: demonstration on step %1 waits for its target").arg(m_step.id);
    }
    return;
  }
  m_demonstrationHidden = false;
  const QRectF area(target);
  const GuideDemonstration::Pose pose = m_demonstration.poseAt(m_demonstrationClock.elapsed(), area);
  if (pose.ended) {
    stopDemonstration(QStringLiteral("played"));
    return;
  }
  if (pose.frame < 0) {
    m_demonstrationFrame = -1;
    m_demonstrationLayer->clear();
    return;
  }
  m_demonstrationLayer->setFrame(m_demonstration.frames.at(pose.frame).image,
                                 m_demonstration.frameRect(pose.frame, pose.point));
  if (pose.frame != m_demonstrationFrame) {
    m_demonstrationFrame = pose.frame;
    // The frame's own place (where a gliding frame arrives).
    const QPointF place = m_demonstration.framePoint(pose.frame, area);
    qInfo().noquote() << QStringLiteral("Guide: demonstration frame %1 at %2 (%3 the target)")
                             .arg(pose.frame + 1)
                             .arg(topText(place),
                                  area.contains(place) ? QStringLiteral("in") : QStringLiteral("outside"));
  }
}

QRect TutorialGuide::calloutRect() const {
  if (!m_active || m_callout.isNull() || !m_callout->isVisible()) {
    return QRect();
  }
  return m_callout->geometry();
}

void TutorialGuide::emitEvent(QJsonObject event) {
  event.insert(QStringLiteral("step"), m_step.id);
  event.insert(QStringLiteral("sequence"), m_sequence);
  m_lastEvent = event;
  emit guideEvent(event);
  emit changed();
}

void TutorialGuide::finish(const QJsonObject& done, const QString& what) {
  stopDemonstration(QStringLiteral("step"));
  m_active = false;
  m_tick.stop();
  // A step with "hold" (mitcad#2368): the filter stays and holds the input
  // until releaseHold; the callout goes as after any step.
  m_holding = m_step.hold;
  updateEventFilter();
  m_noteTimer.stop();
  if (m_highlight) {
    m_highlight->hide();
  }
  if (m_callout) {
    m_callout->hide();
    emit calloutMoved();
  }
  qInfo().noquote() << QStringLiteral("Guide: step %1 done: %2").arg(m_step.id, what);
  if (m_holding) {
    qInfo().noquote() << QStringLiteral("Guide: holding input after step %1").arg(m_step.id);
  }
  emitEvent({{QStringLiteral("event"), QStringLiteral("step_done")}, {QStringLiteral("done"), done}});
}

void TutorialGuide::tick() {
  if (!m_active) {
    return;
  }
  QWidget* window = m_host.window;
  if (m_highlight && m_highlight->geometry() != window->rect()) {
    m_highlight->setGeometry(window->rect());
  }
  const QRect target = m_host.targetRect ? m_host.targetRect(m_step) : QRect();
  if (target != m_target || !m_targetLogged) {
    m_target = target;
    m_targetLogged = true;
    if (m_highlight) {
      m_highlight->setTarget(target);
    }
    placeCallout(target);
    qDebug().noquote() << (target.isNull() ? QStringLiteral("Guide: step %1 target not shown").arg(m_step.id)
                                           : QStringLiteral("Guide: step %1 target at %2,%3 %4x%5")
                                                 .arg(m_step.id)
                                                 .arg(target.x())
                                                 .arg(target.y())
                                                 .arg(target.width())
                                                 .arg(target.height()));
    if (!target.isNull()) {
      // The same in the top window's coordinates, as the toolbar logs its
      // places ("Ribbon workspace at"; the design's window is a page of its
      // document frame): UI tests check the outline holds them (mitcad#2370).
      const QWidget* top = window->window();
      const QPoint at = top->mapFromGlobal(window->mapToGlobal(target.topLeft()));
      qDebug().noquote() << QStringLiteral("Guide: step %1 outline at %2,%3 %4x%5")
                                .arg(m_step.id)
                                .arg(at.x())
                                .arg(at.y())
                                .arg(target.width())
                                .arg(target.height());
    }
  }
  if (m_highlight) {
    m_highlight->raise();
  }
  if (m_demonstrating && m_demonstrationLayer) {
    m_demonstrationLayer->raise();
  }
  if (m_callout) {
    m_callout->raise();
  }

  switch (m_step.done) {
  case GuideStep::Done::Finished: break; // commandFinished
  case GuideStep::Done::Navigate: break; // viewNavigated
  case GuideStep::Done::Started:
    if (m_host.openCommand && m_host.openCommand() == m_step.doneName) {
      finish({{QStringLiteral("started"), m_step.doneName}},
             QStringLiteral("the panel of %1 opened").arg(m_step.doneName));
    }
    break;
  case GuideStep::Done::Value: {
    const QString typed = m_host.inputText ? m_host.inputText(m_step.doneName) : QString();
    if (typed != m_typedLogged) {
      m_typedLogged = typed;
      qDebug().noquote() << QStringLiteral("Guide: step %1 input %2 is '%3'").arg(m_step.id, m_step.doneName, typed);
    }
    if (!guideValueMatches(typed, m_step.value)) {
      m_valueMatched.invalidate();
    } else if (!m_valueMatched.isValid()) {
      m_valueMatched.start();
    } else if (m_valueMatched.elapsed() >= kValueSettleMs) {
      finish({{QStringLiteral("input"), m_step.doneName}, {QStringLiteral("value"), typed}},
             QStringLiteral("%1 typed into %2").arg(typed, m_step.doneName));
    }
    break;
  }
  case GuideStep::Done::Mode:
    if (m_host.mode && m_host.mode() == m_step.doneName) {
      finish({{QStringLiteral("mode"), m_step.doneName}}, QStringLiteral("the mode is %1").arg(m_step.doneName));
    }
    break;
  case GuideStep::Done::Opened:
    // The workspace selector's list shown (mitcad#2370).
    if (m_host.workspaceListOpen && m_host.workspaceListOpen()) {
      finish({{QStringLiteral("opened"), m_step.doneName}}, QStringLiteral("the workspace selector opened"));
    }
    break;
  case GuideStep::Done::Chosen:
    // The workspace shown with the selector's list closed: at once when it
    // is shown already as the step starts (the list not open), else once
    // the user chose it in the list (mitcad#2370).
    if (m_host.workspace && m_host.workspace() == m_step.doneName &&
        !(m_host.workspaceListOpen && m_host.workspaceListOpen())) {
      finish({{QStringLiteral("chosen"), m_step.doneName}},
             QStringLiteral("the workspace %1 chosen").arg(m_step.doneName));
    }
    break;
  case GuideStep::Done::Activated:
    // The tab shown: at once when it is shown already (mitcad#2370).
    if (m_host.tab && m_host.tab() == m_step.doneName) {
      finish({{QStringLiteral("activated"), m_step.doneName}},
             QStringLiteral("the tab %1 activated").arg(m_step.doneName));
    }
    break;
  case GuideStep::Done::None: break; // ended by the page, MCP, Escape or Leave Tutorial
  case GuideStep::Done::Pick: {
    const QStringList picked = m_host.pickedKinds ? m_host.pickedKinds() : QStringList();
    if (picked == m_pickedAtStart) {
      break;
    }
    const auto match = std::find_if(picked.begin(), picked.end(),
                                    [this](const QString& kind) { return m_step.pickMatches(kind); });
    if (match != picked.end()) {
      const QString kind = *match;
      finish({{QStringLiteral("pick"), kind}}, QStringLiteral("a %1 picked").arg(kind));
    }
    break;
  }
  }
}

void TutorialGuide::placeCallout(const QRect& target) {
  if (!m_callout) {
    return;
  }
  QWidget* window = m_host.window;
  const QSize size = m_callout->sizeHint().expandedTo(QSize(kCalloutWidth, 0));
  QPoint at;
  if (target.isNull()) {
    // No target shown: at the top of the view, in the middle.
    QRect area = window->rect();
    if (m_host.viewer != nullptr && m_host.viewer->isVisible()) {
      area = QRect(m_host.viewer->mapTo(window, QPoint(0, 0)), m_host.viewer->size());
    }
    at = QPoint(area.center().x() - size.width() / 2, area.top() + kGap);
  } else if (target.right() + kGap + size.width() <= window->width()) {
    at = QPoint(target.right() + kGap, target.top());
  } else if (target.bottom() + kGap + size.height() <= window->height()) {
    at = QPoint(target.left(), target.bottom() + kGap);
  } else {
    at = QPoint(target.left() - kGap - size.width(), target.top());
  }
  at.setX(std::clamp(at.x(), 0, std::max(0, window->width() - size.width())));
  at.setY(std::clamp(at.y(), 0, std::max(0, window->height() - size.height())));
  if (m_callout->geometry() != QRect(at, size)) {
    m_callout->setGeometry(QRect(at, size));
    emit calloutMoved();
  }
  const QString logged = QStringLiteral("Guide callout at %1, leave button at %2")
                             .arg(centerText(m_callout, window), centerText(m_leaveButton, window));
  if (logged != m_calloutLogged) {
    m_calloutLogged = logged;
    qDebug().noquote() << QStringLiteral("Guide callout at %1").arg(centerText(m_callout, window));
    qDebug().noquote() << QStringLiteral("Guide leave button at %1").arg(centerText(m_leaveButton, window));
  }
}

void TutorialGuide::showNote(const QString& note) {
  if (m_note == nullptr || !m_active) {
    return;
  }
  m_note->setText(note);
  m_note->show();
  m_callout->adjustSize();
  placeCallout(m_target);
  m_noteTimer.start(kNoteMs);
}

bool TutorialGuide::holdsBack(QWidget* widget, const QMouseEvent& mouse, QString* place) const {
  const Qt::MouseButton button = mouse.button();
  QWidget* window = m_host.window;
  const QWidget* top = widget->window();
  // Menus and dialogs (the step's command's) and other windows are free.
  if (qobject_cast<const QMenu*>(top) != nullptr || qobject_cast<const QDialog*>(top) != nullptr ||
      !descends(widget, window)) {
    return false;
  }
  if (m_callout && descends(widget, m_callout)) {
    return false;
  }
  if (m_host.viewer != nullptr && descends(widget, m_host.viewer)) {
    *place = QStringLiteral("the view");
    if (button == Qt::LeftButton) {
      // A left press that navigates by the active scheme (Alt + left
      // orbits in some, the orientation cube's drag in all) is navigation,
      // free while the step allows it (mitcad#2369), during the hold after
      // it too.
      const QPoint at = m_host.viewer->mapFromGlobal(mouse.globalPosition().toPoint());
      const bool navigates =
          m_step.navigation && m_host.navigationPress && m_host.navigationPress(at, button, mouse.modifiers());
      // During the hold after a step (mitcad#2368) every pick, a drawing
      // tool's point too.
      if (m_holding) {
        return !navigates;
      }
      // A panel open, a plane being picked, a sketch: their clicks are free.
      const bool commandOpen = (m_host.openCommand && !m_host.openCommand().isEmpty()) ||
                               (m_host.idle && !m_host.idle());
      if (m_step.takesPicks() || commandOpen) {
        return false;
      }
      return !navigates;
    }
    return !m_step.navigation;
  }
  for (const QWidget* w = widget; w != nullptr && w != window; w = w->parentWidget()) {
    const QString name = w->objectName();
    if (name == QLatin1String("browserDock")) {
      *place = name;
      return m_holding || (m_step.target != GuideStep::Target::Browser && !m_step.takesPicks());
    }
    if (name == QLatin1String("timelineDock") || name == QLatin1String("projectsDock") ||
        name == QLatin1String("configurationDock")) {
      *place = name;
      return true;
    }
  }
  return false;
}

bool TutorialGuide::eventFilter(QObject* watched, QEvent* event) {
  if ((!m_active && !m_holding && !m_viewHeld) || !watched->isWidgetType()) {
    return false;
  }
  auto* widget = static_cast<QWidget*>(watched);
  if (!m_active && !m_holding) {
    // No step and no hold: only the view's picks, held while the pane is
    // centred.
    return holdsViewInput(widget, event);
  }
  switch (event->type()) {
  case QEvent::ShortcutOverride:
  case QEvent::KeyPress: {
    // During the hold Escape is the window's own (it ends a drawing tool);
    // the commands of keys are held by allowsCommand.
    if (m_holding) {
      return false;
    }
    auto* key = static_cast<QKeyEvent*>(event);
    // The design's window is a widget of its document frame (mitcad#1172):
    // a key of a widget in it, or of its frame while it is shown.
    const bool ours = descends(widget, m_host.window) ||
                      (widget->window() == m_host.window->window() && m_host.window->isVisible());
    // Escape in a popup (a menu, the workspace selector's list) closes the
    // popup, not the step (mitcad#2370).
    const bool popup = widget->window()->windowType() == Qt::Popup;
    if (key->key() != Qt::Key_Escape || key->modifiers() != Qt::NoModifier || !ours || popup ||
        !(m_host.idle && m_host.idle())) {
      return false;
    }
    // Escape with no command open ends the step (not the window's own
    // Escape, which would clear the selection).
    event->accept();
    if (event->type() == QEvent::KeyPress) {
      end(QStringLiteral("escape"));
    }
    return true;
  }
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick: {
    QString place;
    const auto* mouse = static_cast<QMouseEvent*>(event);
    if (!holdsBack(widget, *mouse, &place)) {
      return false;
    }
    m_heldWidget = widget;
    if (place != m_lastHeldBack || event->type() == QEvent::MouseButtonPress) {
      m_lastHeldBack = place;
      qInfo().noquote() << (m_holding ? QStringLiteral("Guide: input held back on %1 during the hold after step %2")
                                      : QStringLiteral("Guide: input held back on %1 during step %2"))
                                         .arg(place, m_step.id);
    }
    showNote(tr("Held back during this tutorial step."));
    emitHeld(place,
             event->type() == QEvent::MouseButtonDblClick ? QStringLiteral("double_click") : QStringLiteral("click"),
             mouse->button(), mouse->modifiers());
    return true;
  }
  case QEvent::MouseButtonRelease:
  case QEvent::MouseMove:
    if (m_heldWidget && widget == m_heldWidget) {
      if (event->type() == QEvent::MouseButtonRelease) {
        m_heldWidget = nullptr;
      }
      return true;
    }
    return false;
  case QEvent::Wheel: {
    if (m_host.viewer == nullptr || !descends(widget, m_host.viewer) || m_step.navigation) {
      return false;
    }
    if (!m_wheelHeld.isValid() || m_wheelHeld.elapsed() >= kWheelHeldMs) {
      m_wheelHeld.start();
      const auto* wheel = static_cast<QWheelEvent*>(event);
      emitHeld(QStringLiteral("the view"), QStringLiteral("wheel"), Qt::NoButton, wheel->modifiers());
    }
    return true;
  }
  default: return false;
  }
}

void TutorialGuide::emitHeld(const QString& place, const QString& input, Qt::MouseButton button,
                             Qt::KeyboardModifiers modifiers) {
  QJsonObject event{{QStringLiteral("event"), QStringLiteral("held")},
                    {QStringLiteral("place"), heldPlace(place)},
                    {QStringLiteral("input"), input},
                    {QStringLiteral("modifiers"), guideModifierNames(modifiers)}};
  if (button != Qt::NoButton) {
    event.insert(QStringLiteral("button"), buttonName(button));
  }
  qDebug().noquote() << QStringLiteral("Guide: held %1 on %2 during step %3").arg(input, heldPlace(place), m_step.id);
  emitEvent(event);
}

} // namespace mitcad
