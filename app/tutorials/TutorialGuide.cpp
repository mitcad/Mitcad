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
#include <QtLogging>

#include "../framework/Theme.hpp"

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

namespace {

constexpr int kTickMs = 150;
// A typed value is taken once it stayed so this long.
constexpr qint64 kValueSettleMs = 600;
constexpr int kNoteMs = 3000;
constexpr int kCalloutWidth = 280;
constexpr int kGap = 12;

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
  }
  return QString();
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
}

TutorialGuide::~TutorialGuide() {
  qApp->removeEventFilter(this);
  delete m_highlight;
  delete m_callout;
}

void TutorialGuide::start(const GuideStep& step) {
  if (m_active) {
    end(QStringLiteral("replaced"));
  }
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
  m_highlight->setGeometry(window->rect());
  m_highlight->setTarget(QRect());
  m_highlight->show();
  m_highlight->raise();
  m_callout->show();
  m_callout->raise();
  placeCallout(QRect());
  emit calloutMoved();
  qApp->installEventFilter(this);
  m_tick.start();
  qInfo().noquote() << QStringLiteral("Guide: step %1 started: target %2, %3")
                           .arg(step.id, targetText(step),
                                step.navigation ? QStringLiteral("navigation free") : QStringLiteral("navigation held"));
  emit changed();
  tick();
}

void TutorialGuide::end(const QString& reason) {
  if (!m_active) {
    return;
  }
  m_active = false;
  m_tick.stop();
  qApp->removeEventFilter(this);
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

void TutorialGuide::refuseCommand(const QString& id, const QString& name) {
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

QJsonObject TutorialGuide::state() const {
  QJsonObject state{{QStringLiteral("active"), m_active}, {QStringLiteral("sequence"), m_sequence}};
  if (m_sequence > 0) {
    state.insert(QStringLiteral("step"), m_step.toJson());
  }
  if (!m_lastEvent.isEmpty()) {
    state.insert(QStringLiteral("last_event"), m_lastEvent);
  }
  if (m_active) {
    state.insert(QStringLiteral("target_shown"), !m_target.isNull());
  }
  return state;
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
  m_active = false;
  m_tick.stop();
  qApp->removeEventFilter(this);
  if (m_highlight) {
    m_highlight->hide();
  }
  if (m_callout) {
    m_callout->hide();
    emit calloutMoved();
  }
  qInfo().noquote() << QStringLiteral("Guide: step %1 done: %2").arg(m_step.id, what);
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
  }
  if (m_highlight) {
    m_highlight->raise();
  }
  if (m_callout) {
    m_callout->raise();
  }

  switch (m_step.done) {
  case GuideStep::Done::Finished: break; // commandFinished
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

bool TutorialGuide::holdsBack(QWidget* widget, int button, QString* place) const {
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
      // A panel open, a plane being picked, a sketch: their clicks are free.
      const bool commandOpen = (m_host.openCommand && !m_host.openCommand().isEmpty()) ||
                               (m_host.idle && !m_host.idle());
      return !m_step.takesPicks() && !commandOpen;
    }
    return !m_step.navigation;
  }
  for (const QWidget* w = widget; w != nullptr && w != window; w = w->parentWidget()) {
    const QString name = w->objectName();
    if (name == QLatin1String("browserDock")) {
      *place = name;
      return m_step.target != GuideStep::Target::Browser && !m_step.takesPicks();
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
  if (!m_active || !watched->isWidgetType()) {
    return false;
  }
  auto* widget = static_cast<QWidget*>(watched);
  switch (event->type()) {
  case QEvent::ShortcutOverride:
  case QEvent::KeyPress: {
    auto* key = static_cast<QKeyEvent*>(event);
    // The design's window is a widget of its document frame (mitcad#1172):
    // a key of a widget in it, or of its frame while it is shown.
    const bool ours = descends(widget, m_host.window) ||
                      (widget->window() == m_host.window->window() && m_host.window->isVisible());
    if (key->key() != Qt::Key_Escape || key->modifiers() != Qt::NoModifier || !ours ||
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
    if (!holdsBack(widget, mouse->button(), &place)) {
      return false;
    }
    m_heldWidget = widget;
    if (place != m_lastHeldBack || event->type() == QEvent::MouseButtonPress) {
      m_lastHeldBack = place;
      qInfo().noquote() << QStringLiteral("Guide: input held back on %1 during step %2").arg(place, m_step.id);
    }
    showNote(tr("Held back during this tutorial step."));
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
  case QEvent::Wheel:
    return m_host.viewer != nullptr && descends(widget, m_host.viewer) && !m_step.navigation;
  default: return false;
  }
}

} // namespace mitcad
