// SPDX-License-Identifier: MIT
#include "ToolbarPanelReveal.hpp"

#include <QApplication>
#include <QEvent>
#include <QGuiApplication>
#include <QLineEdit>
#include <QMouseEvent>
#include <QtLogging>

#include <utility>

#include "TestSync.hpp"

namespace mitcad {

ToolbarPanelReveal::ToolbarPanelReveal(QWidget* bandParent, Hooks hooks)
    : QObject(bandParent), m_parent(bandParent), m_hooks(std::move(hooks)) {
  // The band: nothing painted, the pointer's moves taken (also without a
  // button held), so that a rest on it is heard.
  m_band = new QWidget(bandParent);
  m_band->setObjectName(QStringLiteral("toolbarPanelRevealBand"));
  m_band->setMouseTracking(true);
  m_band->setAttribute(Qt::WA_NoSystemBackground);
  m_band->setFocusPolicy(Qt::NoFocus);
  m_band->hide();
  m_band->installEventFilter(this);
  m_rest.setSingleShot(true);
  m_rest.setInterval(kRestMs);
  connect(&m_rest, &QTimer::timeout, this, [this] { rested(); });
  m_leave.setSingleShot(true);
  m_leave.setInterval(kLeaveMs);
  connect(&m_leave, &QTimer::timeout, this, [this] { leaveCheck(); });
  watch(bandParent);
}

ToolbarPanelReveal::~ToolbarPanelReveal() {
  if (m_revealed && qApp != nullptr) {
    qApp->removeEventFilter(this);
  }
}

void ToolbarPanelReveal::watch(QWidget* widget) {
  if (widget != nullptr && !m_watched.contains(widget)) {
    m_watched.append(widget);
    widget->installEventFilter(this);
  }
}

void ToolbarPanelReveal::updateBand() {
  if (m_band == nullptr) {
    return;
  }
  const QRect place = m_hooks.band ? m_hooks.band() : QRect();
  QString line;
  if (place.isEmpty()) {
    m_rest.stop();
    if (!m_band->isHidden()) {
      m_band->hide();
    }
    line = QStringLiteral("Toolbar panel reveal band: none");
  } else {
    if (m_band->geometry() != place) {
      m_band->setGeometry(place);
    }
    if (m_band->isHidden()) {
      m_band->show();
    }
    m_band->raise();
    QWidget* top = m_band->window();
    const QPoint middle = m_band->mapTo(top, m_band->rect().center());
    line = QStringLiteral("Toolbar panel reveal band at %1,%2: %3x%4")
               .arg(middle.x())
               .arg(middle.y())
               .arg(place.width())
               .arg(place.height());
  }
  // Only while its design is shown (not a design behind another's tab):
  // its places are the ones a test uses.
  if (line != m_loggedBand && m_parent != nullptr && m_parent->isVisible()) {
    m_loggedBand = line;
    qDebug().noquote() << line;
  }
}

void ToolbarPanelReveal::concealNow(const QString& how) { finish(how); }

bool ToolbarPanelReveal::eventFilter(QObject* watched, QEvent* event) {
  const QEvent::Type type = event->type();
  if (watched == m_band.data()) {
    switch (type) {
    case QEvent::Enter:
    case QEvent::MouseMove:
      // A move with a button held reaches the band only when the press
      // was on it: no rest then either.
      if (!m_revealed && !m_rest.isActive() && QGuiApplication::mouseButtons() == Qt::NoButton &&
          (type != QEvent::MouseMove || static_cast<QMouseEvent*>(event)->buttons() == Qt::NoButton)) {
        m_rest.start();
      }
      if (m_revealed) {
        m_inside = true;
        m_commandEnded = false;
        m_leave.stop();
      }
      break;
    case QEvent::Leave:
      m_rest.stop();
      break;
    default:
      break;
    }
    return QObject::eventFilter(watched, event);
  }
  // The places the band follows.
  if (type == QEvent::Move || type == QEvent::Resize || type == QEvent::Show || type == QEvent::Hide) {
    if (m_watched.contains(watched) && !m_bandQueued) {
      m_bandQueued = true;
      TestSync::singleShot(0, this, [this] {
        m_bandQueued = false;
        updateBand();
      });
    }
  }
  // While revealed (an application-wide filter): where the pointer is.
  if (m_revealed) {
    if (type == QEvent::MouseButtonPress) {
      // Where a press while revealed goes, for the UI tests (a command
      // started from the panel): the first widget the press reaches.
      const auto* widget = qobject_cast<QWidget*>(watched);
      if (widget != nullptr && m_pressLogged == nullptr) {
        m_pressLogged = widget;
        // A command this press starts comes from the panel (keepFor).
        m_inputInPanel = fromPanel(widget);
        const QWidget* named = widget;
        while (named != nullptr && named->objectName().isEmpty()) {
          named = named->parentWidget();
        }
        QWidget* panel = m_hooks.panel ? m_hooks.panel() : nullptr;
        qDebug().noquote() << QStringLiteral("Toolbar panel revealed: a press on %1 (%2 the panel)")
                                  .arg(named != nullptr ? named->objectName() : QStringLiteral("nothing"),
                                       panel != nullptr && (widget == panel || panel->isAncestorOf(widget))
                                           ? QStringLiteral("in")
                                           : QStringLiteral("outside"));
      }
    } else if (type == QEvent::MouseButtonRelease) {
      m_pressLogged = nullptr;
    } else if (type == QEvent::KeyPress || type == QEvent::ShortcutOverride) {
      // A key goes to the widget with the keyboard first (a shortcut's
      // key is offered to it as ShortcutOverride): in the panel's search
      // field or its list, a command it starts comes from the panel; a
      // key elsewhere (a command's shortcut over the view) does not. An
      // open menu takes the keys (Enter on its entry).
      if (const auto* widget = qobject_cast<QWidget*>(watched);
          widget != nullptr &&
          (widget == QApplication::focusWidget() || widget == QApplication::activePopupWidget())) {
        m_inputInPanel = fromPanel(widget);
      }
    }
    if (type == QEvent::MouseMove) {
      const auto* widget = qobject_cast<QWidget*>(watched);
      // A menu or a pop-up opened from the panel takes the pointer: it
      // decides nothing (panelBusy keeps the panel while it is open).
      if (widget != nullptr && widget->window()->windowType() != Qt::Popup) {
        const QPoint global = static_cast<QMouseEvent*>(event)->globalPosition().toPoint();
        m_inside = overPanel(global);
        if (m_inside) {
          m_leave.stop();
          m_commandEnded = false; // back over it: it goes on the next leave
        } else if (!m_leave.isActive()) {
          m_leave.start();
        }
      }
    } else if (type == QEvent::Leave) {
      // The pointer out of the window (not into a pop-up of it).
      QWidget* panel = m_hooks.panel ? m_hooks.panel() : nullptr;
      if (panel != nullptr && watched == panel->window() && QApplication::activePopupWidget() == nullptr) {
        m_inside = false;
        if (!m_leave.isActive()) {
          m_leave.start();
        }
      }
    }
  }
  return QObject::eventFilter(watched, event);
}

bool ToolbarPanelReveal::overPanel(const QPoint& global) const {
  if (QWidget* panel = m_hooks.panel ? m_hooks.panel() : nullptr; panel != nullptr && panel->isVisible()) {
    if (QRect(panel->mapToGlobal(QPoint(0, 0)), panel->size()).contains(global)) {
      return true;
    }
  }
  return m_band != nullptr && m_band->isVisible() &&
         QRect(m_band->mapToGlobal(QPoint(0, 0)), m_band->size()).contains(global);
}

bool ToolbarPanelReveal::panelBusy() const {
  if (QApplication::activePopupWidget() != nullptr || QApplication::activeModalWidget() != nullptr ||
      QGuiApplication::mouseButtons() != Qt::NoButton) {
    return true;
  }
  QWidget* panel = m_hooks.panel ? m_hooks.panel() : nullptr;
  QWidget* focus = QApplication::focusWidget();
  return panel != nullptr && focus != nullptr && panel->isAncestorOf(focus) && qobject_cast<QLineEdit*>(focus) != nullptr;
}

void ToolbarPanelReveal::rested() {
  if (m_revealed || !m_hooks.canReveal || !m_hooks.canReveal()) {
    return;
  }
  // No reveal in the middle of a drag, a menu or a dialog.
  if (QGuiApplication::mouseButtons() != Qt::NoButton || QWidget::mouseGrabber() != nullptr ||
      QApplication::activePopupWidget() != nullptr || QApplication::activeModalWidget() != nullptr) {
    return;
  }
  m_revealed = true;
  m_inside = true; // on the band
  qApp->installEventFilter(this);
  if (m_hooks.reveal) {
    m_hooks.reveal();
  }
  updateBand();
}

void ToolbarPanelReveal::leaveCheck() {
  if (!m_revealed || m_inside) {
    return;
  }
  if (kept()) {
    return; // asked again when its command ends (release)
  }
  if (panelBusy()) {
    m_leave.start(); // asked again once it is closed
    return;
  }
  finish(std::exchange(m_commandEnded, false) ? QStringLiteral("the command ended")
                                              : QStringLiteral("the pointer left it"));
}

bool ToolbarPanelReveal::kept() const { return m_keeper != nullptr && !m_keptName.isEmpty(); }

void ToolbarPanelReveal::keepFor(QObject* owner, const QString& name) {
  if (!m_revealed || !m_inputInPanel || owner == nullptr) {
    return;
  }
  m_keeper = owner;
  m_keptName = name;
  m_commandEnded = false;
  m_leave.stop();
  qDebug().noquote() << QStringLiteral("Toolbar panel kept (command panel %1 open)").arg(name);
}

void ToolbarPanelReveal::release(QObject* owner) {
  // Another command's end (one started from the panel after it keeps it
  // now), or nothing kept.
  if (m_keptName.isEmpty() || (m_keeper != nullptr && m_keeper != owner)) {
    return;
  }
  const QString name = std::exchange(m_keptName, QString());
  m_keeper = nullptr;
  qDebug().noquote() << QStringLiteral("Toolbar panel released (command %1 ended)").arg(name);
  if (m_revealed && !m_inside) {
    // The pointer is away (in the view, on the command's card): it goes
    // after the leave's wait, as after a leave.
    m_commandEnded = true;
    m_leave.start();
  }
}

bool ToolbarPanelReveal::fromPanel(const QWidget* widget) const {
  const QWidget* panel = m_hooks.panel ? m_hooks.panel() : nullptr;
  if (panel == nullptr) {
    return false;
  }
  // parentWidget() crosses windows: a group's menu's parent is its button.
  for (const QWidget* at = widget; at != nullptr; at = at->parentWidget()) {
    if (at == panel) {
      return true;
    }
  }
  return false;
}

void ToolbarPanelReveal::finish(const QString& how) {
  m_rest.stop();
  m_leave.stop();
  if (!m_revealed) {
    return;
  }
  m_revealed = false;
  m_pressLogged = nullptr;
  m_inside = false;
  m_inputInPanel = false;
  m_keeper = nullptr;
  m_keptName.clear();
  m_commandEnded = false;
  qApp->removeEventFilter(this);
  if (m_hooks.conceal) {
    m_hooks.conceal(how);
  }
  updateBand();
}

} // namespace mitcad
